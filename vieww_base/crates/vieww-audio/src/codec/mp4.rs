//! ISO base media (MP4 / M4A) demuxing for audio.
//!
//! Finds the first audio track (`hdlr` = `soun`), reads its sample table —
//! sizes (`stsz`/`stz2`), chunk offsets (`stco`/`co64`) and the
//! sample-to-chunk map (`stsc`) — to list every access unit's byte range,
//! takes the decoder configuration from `esds` (the AudioSpecificConfig an
//! AAC decoder needs), and reads the edit list (`elst`) so encoder priming
//! and padding can be trimmed: the first `media_time` samples are the
//! encoder's priming, and `segment_duration` is how long is meant to play.

use super::CodecError;

/// The audio track's essentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioTrack {
    /// Codec configuration bytes (AudioSpecificConfig for AAC).
    pub config: Vec<u8>,
    /// Each sample's byte range in the file.
    pub samples: Vec<(usize, usize)>,
    /// Media timescale (samples per second for audio).
    pub timescale: u32,
    /// Movie timescale (for edit-list durations).
    pub movie_timescale: u32,
    /// Priming samples to skip (edit-list media time).
    pub skip: u64,
    /// Samples to play after the skip (edit-list duration, in media units).
    pub play: Option<u64>,
}

fn be32(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

fn be64(b: &[u8], i: usize) -> Option<u64> {
    Some(u64::from_be_bytes(b.get(i..i + 8)?.try_into().ok()?))
}

/// Iterate boxes in `b`: (type, body, body offset within `b`).
fn boxes(b: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 8 <= b.len() {
        let Some(size32) = be32(b, at) else { break };
        let kind = [b[at + 4], b[at + 5], b[at + 6], b[at + 7]];
        let (hdr, size) = match size32 {
            0 => (8, b.len() - at),
            1 => (
                16,
                be64(b, at + 8)
                    .and_then(|v| usize::try_from(v).ok())
                    .unwrap_or(0),
            ),
            n => (8, n as usize),
        };
        if size < hdr || at + size > b.len() {
            break;
        }
        out.push((kind, &b[at + hdr..at + size]));
        at += size;
    }
    out
}

fn child<'a>(b: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(b)
        .into_iter()
        .find(|(k, _)| k == kind)
        .map(|(_, v)| v)
}

/// Read an MPEG-4 descriptor length (up to four 7-bit bytes).
fn desc_len(b: &[u8], at: &mut usize) -> Option<usize> {
    let mut n = 0usize;
    for _ in 0..4 {
        let v = *b.get(*at)?;
        *at += 1;
        n = (n << 7) | usize::from(v & 0x7F);
        if v & 0x80 == 0 {
            break;
        }
    }
    Some(n)
}

/// The DecoderSpecificInfo inside an `esds` box body.
fn esds_config(e: &[u8]) -> Option<Vec<u8>> {
    let mut at = 4; // version + flags
    if *e.get(at)? != 0x03 {
        return None;
    }
    at += 1;
    desc_len(e, &mut at)?;
    at += 2; // ES_ID
    let flags = *e.get(at)?;
    at += 1;
    if flags & 0x80 != 0 {
        at += 2;
    }
    if flags & 0x40 != 0 {
        let l = usize::from(*e.get(at)?);
        at += 1 + l;
    }
    if flags & 0x20 != 0 {
        at += 2;
    }
    if *e.get(at)? != 0x04 {
        return None;
    }
    at += 1;
    desc_len(e, &mut at)?;
    at += 13; // objectType, streamType, bufferSize, max/avg bitrate
    if *e.get(at)? != 0x05 {
        return None;
    }
    at += 1;
    let l = desc_len(e, &mut at)?;
    e.get(at..at + l).map(<[u8]>::to_vec)
}

/// Find the first audio track.
///
/// # Errors
/// Not MP4, or no usable audio track.
pub fn audio_track(file: &[u8]) -> Result<AudioTrack, CodecError> {
    if file.len() < 8 || &file[4..8] != b"ftyp" {
        return Err(CodecError::NotThisFormat);
    }
    let moov = child(file, b"moov").ok_or(CodecError::Malformed("no moov box"))?;
    let movie_timescale = child(moov, b"mvhd")
        .and_then(|m| {
            if m.first() == Some(&1) {
                be32(m, 20)
            } else {
                be32(m, 12)
            }
        })
        .unwrap_or(1000);
    for (k, trak) in boxes(moov) {
        if &k != b"trak" {
            continue;
        }
        let Some(mdia) = child(trak, b"mdia") else {
            continue;
        };
        let is_audio = child(mdia, b"hdlr").is_some_and(|h| h.get(8..12) == Some(b"soun"));
        if !is_audio {
            continue;
        }
        let timescale = child(mdia, b"mdhd")
            .and_then(|m| {
                if m.first() == Some(&1) {
                    be32(m, 20)
                } else {
                    be32(m, 12)
                }
            })
            .unwrap_or(44_100);
        let stbl = child(mdia, b"minf")
            .and_then(|m| child(m, b"stbl"))
            .ok_or(CodecError::Malformed("no sample table"))?;
        let stsd = child(stbl, b"stsd").ok_or(CodecError::Malformed("no stsd"))?;
        let entry = stsd.get(8..).ok_or(CodecError::Truncated)?;
        let mut config = Vec::new();
        for (ek, eb) in boxes(entry) {
            if &ek == b"mp4a" {
                // AudioSampleEntry: 28 bytes of fields, then child boxes.
                if let Some(esds) = eb.get(28..).and_then(|c| child(c, b"esds")) {
                    config = esds_config(esds).unwrap_or_default();
                }
            }
        }
        // Sizes.
        let sizes: Vec<usize> = if let Some(s) = child(stbl, b"stsz") {
            let fixed = be32(s, 4).unwrap_or(0) as usize;
            let n = be32(s, 8).ok_or(CodecError::Truncated)? as usize;
            // A count field is 32 raw bits in a box whose length is right
            // there beside it. With a fixed sample size the table itself is
            // absent but the media it describes is not: `vec![fixed; n]`
            // from a claim of four billion is a multi-gigabyte allocation
            // from a hundred-byte file, so the claim is bounded both by the
            // box (variable sizes) and by the file times a compression
            // allowance no real encoder approaches (fixed sizes).
            if fixed != 0 {
                // `as u128` from usize: widening, and `u128::from(usize)` is
                // not a conversion std provides.
                let claimed = fixed as u128 * n as u128;
                if claimed > file.len() as u128 * 32 {
                    return Err(CodecError::Malformed(
                        "stsz claims more media than the file holds",
                    ));
                }
            } else if n as u128 * 4 > s.len().saturating_sub(12) as u128 {
                return Err(CodecError::Malformed("stsz count exceeds its box"));
            }
            if fixed != 0 {
                vec![fixed; n]
            } else {
                (0..n)
                    .map(|i| be32(s, 12 + 4 * i).unwrap_or(0) as usize)
                    .collect()
            }
        } else {
            return Err(CodecError::Unsupported("stz2 sample sizes"));
        };
        let offsets: Vec<u64> = if let Some(c) = child(stbl, b"stco") {
            let n = be32(c, 4).ok_or(CodecError::Truncated)? as usize;
            if n as u128 * 4 > c.len().saturating_sub(8) as u128 {
                return Err(CodecError::Malformed("stco count exceeds its box"));
            }
            (0..n)
                .map(|i| u64::from(be32(c, 8 + 4 * i).unwrap_or(0)))
                .collect()
        } else if let Some(c) = child(stbl, b"co64") {
            let n = be32(c, 4).ok_or(CodecError::Truncated)? as usize;
            if n as u128 * 8 > c.len().saturating_sub(8) as u128 {
                return Err(CodecError::Malformed("co64 count exceeds its box"));
            }
            (0..n).map(|i| be64(c, 8 + 8 * i).unwrap_or(0)).collect()
        } else {
            return Err(CodecError::Malformed("no chunk offsets"));
        };
        let stsc: Vec<(u32, u32)> = child(stbl, b"stsc").map_or_else(Vec::new, |s| {
            // Entries are 12 bytes from offset 8; a count that exceeds its
            // own box is truncated to what the box actually holds, which
            // serves a real file cut short better than trusting the count
            // would serve a hostile one.
            let fits = (be32(s, 4).unwrap_or(0) as usize).min(s.len().saturating_sub(8) / 12);
            (0..fits)
                .map(|i| {
                    (
                        be32(s, 8 + 12 * i).unwrap_or(1),
                        be32(s, 12 + 12 * i).unwrap_or(1),
                    )
                })
                .collect()
        });
        let mut samples = Vec::with_capacity(sizes.len());
        let mut si = 0;
        for (ci, &off) in offsets.iter().enumerate() {
            let chunk = u32::try_from(ci + 1).unwrap_or(u32::MAX);
            let per = stsc
                .iter()
                .rev()
                .find(|(first, _)| *first <= chunk)
                .map_or(1, |(_, n)| *n as usize);
            let mut pos = usize::try_from(off).map_err(|_| CodecError::Malformed("offset"))?;
            for _ in 0..per {
                let Some(&sz) = sizes.get(si) else { break };
                if pos + sz > file.len() {
                    return Err(CodecError::Truncated);
                }
                samples.push((pos, sz));
                pos += sz;
                si += 1;
            }
        }
        let (mut skip, mut play) = (0, None);
        if let Some(elst) = child(trak, b"edts").and_then(|e| child(e, b"elst")) {
            let v1 = elst.first() == Some(&1);
            let n = be32(elst, 4).unwrap_or(0);
            // The first non-empty edit decides priming and duration.
            let mut at = 8;
            for _ in 0..n {
                let (dur, mt) = if v1 {
                    (
                        be64(elst, at).unwrap_or(0),
                        be64(elst, at + 8).unwrap_or(0).cast_signed(),
                    )
                } else {
                    (
                        u64::from(be32(elst, at).unwrap_or(0)),
                        i64::from(be32(elst, at + 4).unwrap_or(0).cast_signed()),
                    )
                };
                at += if v1 { 20 } else { 12 };
                if mt >= 0 {
                    skip = mt.unsigned_abs();
                    // Duration is in movie units; convert to media units.
                    play = Some(dur * u64::from(timescale) / u64::from(movie_timescale.max(1)));
                    break;
                }
            }
        }
        return Ok(AudioTrack {
            config,
            samples,
            timescale,
            movie_timescale,
            skip,
            play,
        });
    }
    Err(CodecError::Malformed("no audio track"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_aac_track_in_the_fixture() {
        let f = include_bytes!("../../tests/fixtures/src.m4a");
        let t = audio_track(f).unwrap();
        assert_eq!(t.timescale, 44_100);
        // ffmpeg appends explicit "no SBR" signalling after the 2-byte core.
        assert!(t.config.len() >= 2, "AudioSpecificConfig present");
        assert_eq!(t.config[0] >> 3, 2, "object type 2 = LC");
        assert_eq!(
            t.samples.len(),
            19,
            "⌈(17 640 + 1 024 priming) / 1 024⌉ access units"
        );
        assert_eq!(t.skip, 1024, "AAC priming");
        assert_eq!(t.play, Some(17_640));
    }

    #[test]
    fn rejects_non_mp4() {
        assert_eq!(
            audio_track(b"RIFF....WAVE").err(),
            Some(CodecError::NotThisFormat)
        );
    }

    #[test]
    fn a_lying_stsz_count_is_rejected_not_allocated() {
        // `vec![fixed; n]` trusted a 32-bit count in a box whose own length
        // said otherwise: a hundred-byte file claiming four billion
        // four-byte samples asked for sixteen gigabytes before the first
        // byte of audio was read. The claim is now bounded by the file
        // times a compression allowance no real encoder approaches.
        fn box_of(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
            let mut b = Vec::with_capacity(8 + body.len());
            b.extend_from_slice(&u32::try_from(8 + body.len()).unwrap_or(0).to_be_bytes());
            b.extend_from_slice(kind);
            b.extend_from_slice(body);
            b
        }
        let stsz = {
            let mut body = Vec::new();
            body.extend_from_slice(&[0; 4]); // version/flags
            body.extend_from_slice(&4u32.to_be_bytes()); // fixed sample size
            body.extend_from_slice(&0xFFFF_FFF0u32.to_be_bytes()); // the lie
            box_of(b"stsz", &body)
        };
        let stco = {
            let mut body = Vec::new();
            body.extend_from_slice(&[0; 4]);
            body.extend_from_slice(&1u32.to_be_bytes()); // one chunk
            body.extend_from_slice(&0u32.to_be_bytes());
            box_of(b"stco", &body)
        };
        let stsd = box_of(b"stsd", &[0; 8]); // version/flags + entry count 0
        let hdlr = {
            let mut body = vec![0; 8];
            body.extend_from_slice(b"soun"); // handler type at body offset 8
            box_of(b"hdlr", &body)
        };
        let stbl = box_of(b"stbl", &[stsd, stsz, stco].concat());
        let minf = box_of(b"minf", &stbl);
        let mdia = box_of(b"mdia", &[hdlr, minf].concat());
        let trak = box_of(b"trak", &mdia);
        let moov = box_of(b"moov", &trak);
        let mut file = box_of(b"ftyp", b"isom\0\0\0\0mp42");
        file.extend_from_slice(&moov);
        assert_eq!(
            audio_track(&file).err(),
            Some(CodecError::Malformed(
                "stsz claims more media than the file holds"
            ))
        );
    }
}
