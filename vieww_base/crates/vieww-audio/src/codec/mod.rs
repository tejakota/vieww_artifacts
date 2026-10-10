//! Audio codecs, written in this repository — no codec library underneath.
//!
//! | Format | Decode | Encode | Notes |
//! |---|---|---|---|
//! | WAV | [`wav`](crate::wav) | yes | 8/16/24/32-bit int, 32/64-bit float, EXTENSIBLE, any layout |
//! | AIFF / AIFF-C | [`aiff`] | yes (16/24) | `NONE`, `twos`, `sowt`, `fl32`, `fl64` |
//! | FLAC | [`flac`] | yes | all subframe types, all stereo modes, MD5-verified |
//! | MP3 | [`mp3`] | — | MPEG-1/2/2.5 Layer III, joint stereo, gapless (LAME tag) |
//! | Ogg Vorbis | [`vorbis`] | — | floor 1, residue 0/1/2, coupling; granule trimming |
//! | AAC-LC | [`aac`] | — | ADTS and MP4/M4A (with edit-list priming removal) |
//!
//! Every decoder returns [`Decoded`]: interleaved `f32` at the stream's own
//! rate. [`decode`] sniffs the container and dispatches. Each decoder is
//! tested against files made by an independent encoder (ffmpeg with LAME,
//! libvorbis, its FLAC and AAC encoders) and against that tool's own decode
//! of the same file — see `tests/fixtures/make.sh`.

pub mod aac;
mod aac_tables;
pub mod aiff;
pub mod bits;
pub mod flac;
pub mod md5;
pub mod mp3;
mod mp3_tables;
pub mod mp4;
pub mod ogg;
pub mod vorbis;

/// A decoded stream.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    /// Interleaved samples in `-1..=1`.
    pub data: Vec<f32>,
    /// Channels.
    pub channels: u8,
    /// Sample rate.
    pub rate: u32,
}

impl Decoded {
    /// Frames.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.data.len() / usize::from(self.channels.max(1))
    }

    /// As [`Samples`](crate::Samples) when mono or stereo.
    #[must_use]
    pub fn to_samples(&self) -> Option<crate::Samples> {
        match self.channels {
            1 => Some(crate::Samples::mono(self.data.clone(), self.rate)),
            2 => Some(crate::Samples::stereo(self.data.clone(), self.rate)),
            _ => None,
        }
    }
}

/// Why a stream did not decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    /// Not this codec's format at all.
    NotThisFormat,
    /// Ended early.
    Truncated,
    /// A structural violation, named.
    Malformed(&'static str),
    /// A valid feature this decoder does not implement, named.
    Unsupported(&'static str),
    /// A checksum did not match.
    Checksum(&'static str),
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotThisFormat => f.write_str("not this format"),
            Self::Truncated => f.write_str("the stream ends early"),
            Self::Malformed(w) => write!(f, "malformed: {w}"),
            Self::Unsupported(w) => write!(f, "unsupported: {w}"),
            Self::Checksum(w) => write!(f, "checksum mismatch: {w}"),
        }
    }
}

impl std::error::Error for CodecError {}

/// The container a byte stream looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// RIFF/WAVE.
    Wav,
    /// FORM/AIFF or AIFC.
    Aiff,
    /// fLaC.
    Flac,
    /// Ogg (Vorbis).
    Ogg,
    /// MPEG audio (MP3), possibly behind ID3.
    Mp3,
    /// AAC in ADTS frames.
    Adts,
    /// ISO base media (MP4/M4A).
    Mp4,
}

/// Guess the format from the first bytes.
#[must_use]
pub fn sniff(b: &[u8]) -> Option<Format> {
    if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WAVE" {
        Some(Format::Wav)
    } else if b.len() >= 12 && &b[0..4] == b"FORM" && (&b[8..12] == b"AIFF" || &b[8..12] == b"AIFC")
    {
        Some(Format::Aiff)
    } else if b.starts_with(b"fLaC") {
        Some(Format::Flac)
    } else if b.starts_with(b"OggS") {
        Some(Format::Ogg)
    } else if b.len() >= 8 && &b[4..8] == b"ftyp" {
        Some(Format::Mp4)
    } else if b.starts_with(b"ID3") {
        Some(Format::Mp3)
    } else if b.len() >= 2 && b[0] == 0xFF && b[1] & 0xF6 == 0xF0 {
        Some(Format::Adts)
    } else if b.len() >= 2 && b[0] == 0xFF && b[1] & 0xE0 == 0xE0 {
        Some(Format::Mp3)
    } else {
        None
    }
}

/// Decode any supported format.
///
/// # Errors
/// [`CodecError::NotThisFormat`] when nothing recognises it, or the
/// matching decoder's error.
pub fn decode(bytes: &[u8]) -> Result<Decoded, CodecError> {
    match sniff(bytes).ok_or(CodecError::NotThisFormat)? {
        Format::Wav => {
            let (m, _) =
                crate::wav::read_wav_multi(bytes).map_err(|_| CodecError::Malformed("WAV"))?;
            Ok(Decoded {
                channels: u8::try_from(m.layout.channels()).unwrap_or(u8::MAX),
                data: m.data,
                rate: m.rate,
            })
        }
        Format::Aiff => aiff::decode(bytes),
        Format::Flac => flac::decode(bytes),
        Format::Ogg => vorbis::decode(bytes),
        Format::Mp3 => mp3::decode(bytes),
        Format::Adts => aac::decode_adts(bytes),
        Format::Mp4 => aac::decode_mp4(bytes),
    }
}

/// Signal-to-noise ratio in dB of `test` against `reference`, after the
/// best alignment within `±max_lag` frames — how the decoders are graded.
#[must_use]
pub fn snr_db(reference: &[f32], test: &[f32], channels: usize, max_lag: usize) -> (f64, isize) {
    let ch = channels.max(1);
    let fr = reference.len().min(test.len()) / ch;
    let mut best = (f64::NEG_INFINITY, 0isize);
    let max_lag = isize::try_from(max_lag).unwrap_or(0);
    for lag in -max_lag..=max_lag {
        let mut sig = 0.0f64;
        let mut err = 0.0f64;
        for f in 0..fr {
            let Ok(g) = usize::try_from(isize::try_from(f).unwrap_or(0) + lag) else {
                continue;
            };
            if g >= fr {
                continue;
            }
            for c in 0..ch {
                let r = f64::from(reference[f * ch + c]);
                let t = f64::from(test[g * ch + c]);
                sig += r * r;
                err += (r - t) * (r - t);
            }
        }
        let snr = 10.0 * (sig / err.max(1e-30)).log10();
        if snr > best.0 {
            best = (snr, lag);
        }
    }
    best
}

#[cfg(test)]
pub(crate) fn s16le(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(2)
        .map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])) / 32_768.0)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_every_fixture() {
        let f = |b: &[u8]| sniff(b);
        assert_eq!(
            f(include_bytes!("../../tests/fixtures/source16.wav")),
            Some(Format::Wav)
        );
        assert_eq!(
            f(include_bytes!("../../tests/fixtures/src24.aiff")),
            Some(Format::Aiff)
        );
        assert_eq!(
            f(include_bytes!("../../tests/fixtures/src16.flac")),
            Some(Format::Flac)
        );
        assert_eq!(
            f(include_bytes!("../../tests/fixtures/src.ogg")),
            Some(Format::Ogg)
        );
        assert_eq!(
            f(include_bytes!("../../tests/fixtures/src.mp3")),
            Some(Format::Mp3)
        );
        assert_eq!(
            f(include_bytes!("../../tests/fixtures/src_mono.aac")),
            Some(Format::Adts)
        );
        assert_eq!(
            f(include_bytes!("../../tests/fixtures/src.m4a")),
            Some(Format::Mp4)
        );
        assert_eq!(f(b"hello"), None);
    }
}
