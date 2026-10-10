//! RIFF/WAVE: the one format this crate reads and writes.
//!
//! WAV is chosen not because it is good but because it is *knowable*: PCM
//! samples, a header that says how wide they are, and nothing a UI's click
//! needs that it does not say. Every browser's `AudioContext`, every DAW's
//! export and every audio tutorial on earth can produce one, which makes it
//! the format an application's assets are most likely to already be in.
//!
//! # What is supported, out loud
//!
//! * PCM 8-bit (unsigned), 16-, 24- and 32-bit (signed), and IEEE float 32-
//!   and 64-bit, in plain `fmt ` chunks and in `WAVE_FORMAT_EXTENSIBLE`
//!   (whose sub-format GUID is checked, and whose channel mask is kept).
//!   Anything else — ADPCM, µ-law, an unknown GUID — is refused by name
//!   rather than misread.
//! * [`read_wav`] returns mono or stereo [`Samples`], the shape UI sound
//!   has, and refuses more channels; [`read_wav_multi`] returns any layout
//!   as [`MultiSamples`](crate::channels::MultiSamples).
//! * [`write_wav`] writes 16-bit mono/stereo, as it always has;
//!   [`write_wav_format`] writes any [`SampleFormat`] and any layout, as
//!   `WAVE_FORMAT_EXTENSIBLE` with a channel mask whenever plain `fmt `
//!   cannot say it (more than two channels, or more than 16 bits).
//! * The chunks are walked, not assumed: `fmt ` and `data` are found wherever
//!   they sit, so a file with a `LIST` chunk before `data` (metadata, and
//!   common — a DAW writes the project's name there) parses like any other.
//!
//! ```
//! use vieww_audio::{write_wav, Samples};
//!
//! // A quarter-second of a soft 1 kHz tone, mono, at 44.1 kHz.
//! let rate = 44_100;
//! let frames = rate / 4;
//! let samples: Vec<f32> = (0..frames)
//!     .map(|frame| {
//!         (std::f32::consts::TAU * 1_000.0 * frame as f32 / rate as f32).sin() * 0.25
//!     })
//!     .collect();
//! let tone = Samples::mono(samples, rate);
//!
//! let bytes = write_wav(&tone).expect("in-memory buffers always encode");
//! let decoded = vieww_audio::read_wav(&bytes).expect("and what we wrote, we read");
//!
//! assert_eq!(decoded.samples.rate, rate);
//! assert_eq!(decoded.samples.channels, 1);
//! // 16-bit PCM round-trips at ±1/32768 — one quantisation step.
//! let worst = decoded
//!     .samples
//!     .data
//!     .iter()
//!     .zip(&tone.data)
//!     .map(|(decoded, original)| (decoded - original).abs())
//!     .fold(0.0, f32::max);
//! assert!(worst < 1.0 / 32_000.0, "quantisation only, worst {worst}");
//! ```

use crate::Samples;

/// Why a WAV did not parse or encode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PcmError {
    /// The file is shorter than a header, or a chunk says it is longer than
    /// the file is.
    Truncated,
    /// The magic is not `RIFF....WAVE` — not a WAV at all, and pretending
    /// otherwise would be guessing at every field after it.
    NotWave,
    /// A PCM width this reader refuses: 24-bit, float, or anything else.
    /// Says which, because "cannot read the file" and "cannot read the
    /// file *because it is 24-bit*" are different support tickets.
    Unsupported(&'static str),
    /// The `fmt ` chunk is missing, so there is nothing to decode against.
    NoFormatChunk,
    /// The `data` chunk is missing: a header with no sound in it.
    NoDataChunk,
    /// The format code is not plain PCM.
    NotPcm(u16),
}

impl std::fmt::Display for PcmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated => f.write_str("the WAV ends before its own header says it should"),
            Self::NotWave => f.write_str("not a RIFF/WAVE file"),
            Self::Unsupported(what) => write!(f, "unsupported WAV feature: {what}"),
            Self::NoFormatChunk => f.write_str("no `fmt ` chunk: the sample width is unknown"),
            Self::NoDataChunk => f.write_str("no `data` chunk: a header with no sound"),
            Self::NotPcm(code) => write!(f, "format code {code} is not plain PCM"),
        }
    }
}

impl std::error::Error for PcmError {}

/// A parsed WAV: everything `fmt ` and `data` said, decoded to [`Samples`].
///
/// The intermediate type exists because a caller sometimes wants the *file's*
/// facts — the original bit depth, say — rather than the decoded buffer's,
/// and re-deriving them from `f32`s is exactly the guesswork this parser
/// exists to avoid.
#[derive(Debug, Clone, PartialEq)]
pub struct PcmSamples {
    /// The decoded samples, interleaved, in `-1..=1`.
    pub samples: Samples,
    /// The bit depth the file carried: 8, 16, 24, 32 or 64.
    pub bits: u16,
    /// Whether the samples were IEEE floating point.
    pub float: bool,
}

/// A sample encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    /// 8-bit unsigned.
    U8,
    /// 16-bit signed.
    I16,
    /// 24-bit signed.
    I24,
    /// 32-bit signed.
    I32,
    /// 32-bit IEEE float.
    F32,
    /// 64-bit IEEE float.
    F64,
}

impl SampleFormat {
    /// Bits per sample.
    #[must_use]
    pub const fn bits(self) -> u16 {
        match self {
            Self::U8 => 8,
            Self::I16 => 16,
            Self::I24 => 24,
            Self::I32 | Self::F32 => 32,
            Self::F64 => 64,
        }
    }

    /// Bytes per sample.
    #[must_use]
    pub const fn bytes(self) -> usize {
        self.bits() as usize / 8
    }

    /// Whether this is a float format.
    #[must_use]
    pub const fn is_float(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    /// Decode one sample from little-endian bytes.
    #[must_use]
    pub fn decode_le(self, b: &[u8]) -> f32 {
        match self {
            Self::U8 => (f32::from(b[0]) - 128.0) / 128.0,
            Self::I16 => f32::from(i16::from_le_bytes([b[0], b[1]])) / f32::from(i16::MAX),
            Self::I24 => {
                let v = i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8;
                v as f32 / 8_388_607.0
            }
            Self::I32 => {
                (f64::from(i32::from_le_bytes([b[0], b[1], b[2], b[3]])) / 2_147_483_647.0) as f32
            }
            Self::F32 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            Self::F64 => {
                f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]) as f32
            }
        }
    }

    /// Encode one sample as little-endian bytes (clamped for integers).
    pub fn encode_le(self, v: f32, out: &mut Vec<u8>) {
        let c = v.clamp(-1.0, 1.0);
        match self {
            Self::U8 => out.push(((c * 128.0).round() + 128.0).clamp(0.0, 255.0) as u8),
            Self::I16 => out.extend(((c * f32::from(i16::MAX)).round() as i16).to_le_bytes()),
            Self::I24 => {
                let q = (f64::from(c) * 8_388_607.0).round() as i32;
                out.extend(&q.to_le_bytes()[..3]);
            }
            Self::I32 => {
                out.extend(((f64::from(c) * 2_147_483_647.0).round() as i32).to_le_bytes())
            }
            Self::F32 => out.extend(v.to_le_bytes()),
            Self::F64 => out.extend(f64::from(v).to_le_bytes()),
        }
    }
}

/// What a WAV header said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavInfo {
    /// The sample encoding.
    pub format: SampleFormat,
    /// Channels.
    pub channels: u16,
    /// Rate.
    pub rate: u32,
    /// `WAVE_FORMAT_EXTENSIBLE` channel mask (0 if none was given).
    pub channel_mask: u32,
}

/// The KSDATAFORMAT_SUBTYPE GUID tail shared by PCM (1) and float (3).
const GUID_TAIL: [u8; 14] = [
    0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71,
];

/// Where a WAV file's samples are, from its first bytes alone — what a
/// disk streamer needs to read the samples in pieces.
///
/// `head` is the start of the file (a few kilobytes is plenty; it must
/// reach the `data` chunk header) and `file_len` its full length. Returns
/// the format, the byte offset of the first sample, and the byte length
/// of the sample data. A `data` size of 0 or `0xFFFF_FFFF` (what live
/// recorders write before they know) means "to the end of the file".
///
/// # Errors
/// Not a WAV, no `fmt ` before `data`, or a format this crate does not
/// decode.
pub fn read_wav_header(head: &[u8], file_len: u64) -> Result<(WavInfo, u64, u64), PcmError> {
    if head.len() < 12 {
        return Err(PcmError::Truncated);
    }
    if &head[0..4] != b"RIFF" || &head[8..12] != b"WAVE" {
        return Err(PcmError::NotWave);
    }
    let mut info: Option<WavInfo> = None;
    let mut at = 12usize;
    while at + 8 <= head.len() {
        let id = &head[at..at + 4];
        let size = u32::from_le_bytes([head[at + 4], head[at + 5], head[at + 6], head[at + 7]]);
        let start = at + 8;
        if id == b"data" {
            let i = info.ok_or(PcmError::NoFormatChunk)?;
            let off = start as u64;
            let avail = file_len.saturating_sub(off);
            let len = if size == 0 || size == u32::MAX {
                avail
            } else {
                u64::from(size).min(avail)
            };
            let frame = (i.format.bytes() * usize::from(i.channels.max(1))) as u64;
            return Ok((i, off, len - len % frame.max(1)));
        }
        let end = start + size as usize;
        if id == b"fmt " {
            let f = head.get(start..end).ok_or(PcmError::Truncated)?;
            if f.len() < 16 {
                return Err(PcmError::Truncated);
            }
            let mut code = u16::from_le_bytes([f[0], f[1]]);
            let channels = u16::from_le_bytes([f[2], f[3]]);
            let rate = u32::from_le_bytes([f[4], f[5], f[6], f[7]]);
            let bits = u16::from_le_bytes([f[14], f[15]]);
            let mut channel_mask = 0;
            if code == 0xFFFE {
                if f.len() < 40 {
                    return Err(PcmError::Truncated);
                }
                channel_mask = u32::from_le_bytes([f[20], f[21], f[22], f[23]]);
                if f[26..40] != GUID_TAIL {
                    return Err(PcmError::Unsupported(
                        "an unknown WAVE_FORMAT_EXTENSIBLE sub-format",
                    ));
                }
                code = u16::from_le_bytes([f[24], f[25]]);
            }
            let format = match (code, bits) {
                (1, 8) => SampleFormat::U8,
                (1, 16) => SampleFormat::I16,
                (1, 24) => SampleFormat::I24,
                (1, 32) => SampleFormat::I32,
                (3, 32) => SampleFormat::F32,
                (3, 64) => SampleFormat::F64,
                (1 | 3, _) => return Err(PcmError::Unsupported("this sample width")),
                (c, _) => return Err(PcmError::NotPcm(c)),
            };
            if channels == 0 {
                return Err(PcmError::Unsupported("zero channels"));
            }
            info = Some(WavInfo {
                format,
                channels,
                rate,
                channel_mask,
            });
        }
        at = end + (size as usize & 1);
    }
    Err(PcmError::Truncated)
}

/// Decode a WAV file.
///
/// # Errors
///
/// Every [`PcmError`], with the reason a file was refused rather than a bare
/// `None` — "this file is 24-bit" is actionable, "this file failed" is not.
pub fn read_wav(bytes: &[u8]) -> Result<PcmSamples, PcmError> {
    let (data, info) = decode(bytes)?;
    let samples = match info.channels {
        1 => Samples::mono(data, info.rate),
        2 => Samples::stereo(data, info.rate),
        _ => {
            return Err(PcmError::Unsupported(
                "more than two channels (use read_wav_multi)",
            ))
        }
    };
    Ok(PcmSamples {
        samples,
        bits: info.format.bits(),
        float: info.format.is_float(),
    })
}

/// Decode a WAV of any channel count into a
/// [`MultiSamples`](crate::channels::MultiSamples), with what the header
/// said. The layout follows the channel mask when there is one, the count
/// otherwise.
///
/// # Errors
///
/// Every [`PcmError`].
pub fn read_wav_multi(bytes: &[u8]) -> Result<(crate::channels::MultiSamples, WavInfo), PcmError> {
    use crate::channels::{Layout, MultiSamples};
    let (data, info) = decode(bytes)?;
    let n = usize::from(info.channels);
    let layout = match (n, info.channel_mask) {
        (_, 0) => Layout::for_channels(n),
        (1, 0x4) => Layout::Mono,
        (2, 0x3) => Layout::Stereo,
        (4, 0x33) => Layout::Quad,
        (6, 0x3F | 0x60F) => Layout::Surround51,
        (8, 0x63F | 0xFF) => Layout::Surround71,
        _ => Layout::Discrete(n),
    };
    Ok((
        MultiSamples {
            data,
            layout,
            rate: info.rate,
        },
        info,
    ))
}

/// The shared decoder: chunk walk, format resolution, sample decode.
fn decode(bytes: &[u8]) -> Result<(Vec<f32>, WavInfo), PcmError> {
    // RIFF header: "RIFF", size, "WAVE". The size is advisory (streaming
    // writers get it wrong; players ignore it), so it is read and never
    // trusted — the chunk walk is what decides where the file ends.
    if bytes.len() < 12 {
        return Err(PcmError::Truncated);
    }
    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(PcmError::NotWave);
    }

    // Walk the chunks: 4-byte id, 4-byte little-endian size, then payload,
    // padded to an even length. `fmt ` may come after `data`; nothing here
    // depends on order.
    let mut fmt: Option<&[u8]> = None;
    let mut data: Option<&[u8]> = None;

    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32::from_le_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]])
            as usize;
        let payload_start = at + 8;
        let payload_end = payload_start.checked_add(size).ok_or(PcmError::Truncated)?;
        if payload_end > bytes.len() {
            return Err(PcmError::Truncated);
        }
        if id == b"fmt " {
            if size < 16 {
                return Err(PcmError::Truncated);
            }
            fmt = Some(&bytes[payload_start..payload_end]);
        } else if id == b"data" {
            data = Some(&bytes[payload_start..payload_end]);
        }
        // Chunks are word-aligned: an odd size skips one pad byte.
        at = payload_end + (size & 1);
    }

    let Some(f) = fmt else {
        return Err(PcmError::NoFormatChunk);
    };
    let mut code = u16::from_le_bytes([f[0], f[1]]);
    let channels = u16::from_le_bytes([f[2], f[3]]);
    let rate = u32::from_le_bytes([f[4], f[5], f[6], f[7]]);
    let bits = u16::from_le_bytes([f[14], f[15]]);
    let mut channel_mask = 0;
    if code == 0xFFFE {
        // WAVE_FORMAT_EXTENSIBLE: cbSize, valid bits, mask, GUID.
        if f.len() < 40 {
            return Err(PcmError::Truncated);
        }
        channel_mask = u32::from_le_bytes([f[20], f[21], f[22], f[23]]);
        if f[26..40] != GUID_TAIL {
            return Err(PcmError::Unsupported(
                "an unknown WAVE_FORMAT_EXTENSIBLE sub-format",
            ));
        }
        code = u16::from_le_bytes([f[24], f[25]]);
    }
    if code != 1 && code != 3 {
        return Err(PcmError::NotPcm(code));
    }
    let Some(data) = data else {
        return Err(PcmError::NoDataChunk);
    };
    let format = match (code, bits) {
        (1, 8) => SampleFormat::U8,
        (1, 16) => SampleFormat::I16,
        (1, 24) => SampleFormat::I24,
        (1, 32) => SampleFormat::I32,
        (3, 32) => SampleFormat::F32,
        (3, 64) => SampleFormat::F64,
        _ => return Err(PcmError::Unsupported("an unknown bit depth")),
    };
    if channels == 0 {
        return Err(PcmError::Unsupported("zero channels"));
    }
    let width = format.bytes();
    let frame = width * usize::from(channels);
    let usable = data.len() / frame * frame;
    let decoded = data[..usable]
        .chunks_exact(width)
        .map(|b| format.decode_le(b))
        .collect();
    Ok((
        decoded,
        WavInfo {
            format,
            channels,
            rate,
            channel_mask,
        },
    ))
}

/// Encode samples as 16-bit PCM mono/stereo WAV.
///
/// Always 16-bit: the depth every decoder in this file reads, the depth CD
/// audio settled on, and a depth whose quantisation (`±1/32768`) is below
/// what any UI speaker reproduces — the round-trip test is what proves that
/// claim, not this comment.
///
/// # Errors
///
/// Never, in-memory; the `Result` is the shape a file-writing caller wants
/// to chain. More than two channels is refused rather than guessed at.
pub fn write_wav(samples: &Samples) -> Result<Vec<u8>, PcmError> {
    if samples.channels > 2 {
        return Err(PcmError::Unsupported("more than two channels"));
    }

    let channels = u16::from(samples.channels);
    let data_bytes = samples.data.len() * 2;
    // RIFF (12) + fmt (8 + 16) + data (8 + payload), word-aligned by
    // construction: 16-bit samples are always an even payload.
    let file_size = 4 + 8 + 16 + 8 + data_bytes;

    let mut out = Vec::with_capacity(12 + 8 + 16 + 8 + data_bytes);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(file_size as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");

    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&samples.rate.to_le_bytes());
    let byte_rate = samples.rate * u32::from(channels) * 2;
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&(channels * 2).to_le_bytes()); // block align
    out.extend_from_slice(&16_u16.to_le_bytes());

    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_bytes as u32).to_le_bytes());
    for sample in &samples.data {
        let quantised = (sample * i16::MAX as f32)
            .round()
            .clamp(-i16::MAX as f32, i16::MAX as f32) as i16;
        out.extend_from_slice(&quantised.to_le_bytes());
    }

    Ok(out)
}

/// Encode any layout in any [`SampleFormat`].
///
/// Plain `fmt ` (PCM or float code) for mono/stereo at 8 or 16 bits;
/// `WAVE_FORMAT_EXTENSIBLE` with the layout's channel mask otherwise, as the
/// format's own documentation requires for more than two channels or more
/// than 16 bits.
///
/// # Errors
///
/// [`PcmError::Unsupported`] for zero channels or a file over 4 GiB.
pub fn write_wav_format(
    samples: &crate::channels::MultiSamples,
    format: SampleFormat,
) -> Result<Vec<u8>, PcmError> {
    let ch = samples.layout.channels();
    let channels = u16::try_from(ch).map_err(|_| PcmError::Unsupported("too many channels"))?;
    if channels == 0 {
        return Err(PcmError::Unsupported("zero channels"));
    }
    let width = format.bytes();
    let data_bytes = samples.data.len() * width;
    let extensible = channels > 2 || format.bits() > 16;
    let fmt_len: u32 = if extensible { 40 } else { 16 };
    let total = 4 + 8 + fmt_len as usize + 8 + data_bytes + (data_bytes & 1);
    let total32 = u32::try_from(total).map_err(|_| PcmError::Unsupported("a file over 4 GiB"))?;
    let mut out = Vec::with_capacity(total + 8);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&total32.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&fmt_len.to_le_bytes());
    let code: u16 = if format.is_float() { 3 } else { 1 };
    out.extend_from_slice(&(if extensible { 0xFFFE } else { code }).to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&samples.rate.to_le_bytes());
    let block = u32::from(channels) * u32::from(format.bits() / 8);
    out.extend_from_slice(&(samples.rate * block).to_le_bytes());
    out.extend_from_slice(&u16::try_from(block).unwrap_or(u16::MAX).to_le_bytes());
    out.extend_from_slice(&format.bits().to_le_bytes());
    if extensible {
        out.extend_from_slice(&22_u16.to_le_bytes());
        out.extend_from_slice(&format.bits().to_le_bytes());
        out.extend_from_slice(&samples.layout.wave_mask().to_le_bytes());
        out.extend_from_slice(&code.to_le_bytes());
        out.extend_from_slice(&GUID_TAIL);
    }
    out.extend_from_slice(b"data");
    out.extend_from_slice(&u32::try_from(data_bytes).unwrap_or(u32::MAX).to_le_bytes());
    for v in &samples.data {
        format.encode_le(*v, &mut out);
    }
    if data_bytes & 1 == 1 {
        out.push(0);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Envelope, Tone, Waveform};

    fn tone(rate: u32, frames: usize) -> Samples {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a test tone is exact enough in f32"
        )]
        let rate = rate as f32;
        let data: Vec<f32> = (0..frames)
            .map(|frame| (std::f32::consts::TAU * 1_000.0 * frame as f32 / rate).sin() * 0.25)
            .collect();
        Samples::mono(data, rate as u32)
    }

    #[test]
    fn a_written_wav_reads_back_identically_modulo_quantisation() {
        let original = tone(44_100, 100);
        let bytes = write_wav(&original).unwrap();
        let decoded = read_wav(&bytes).unwrap();

        assert_eq!(decoded.samples.rate, 44_100);
        assert_eq!(decoded.samples.channels, 1);
        assert_eq!(decoded.bits, 16);
        assert_eq!(decoded.samples.data.len(), 100);

        let worst = decoded
            .samples
            .data
            .iter()
            .zip(&original.data)
            .map(|(decoded, original)| (decoded - original).abs())
            .fold(0.0, f32::max);
        assert!(worst < 1.0 / 32_000.0, "quantisation only, worst {worst}");
    }

    #[test]
    fn stereo_round_trips() {
        // Left full-scale, right silent: a stereo identity the decode must
        // preserve channel by channel.
        let original = Samples::stereo((0..50).flat_map(|_| [0.5_f32, 0.0]).collect(), 48_000);
        let bytes = write_wav(&original).unwrap();
        let decoded = read_wav(&bytes).unwrap();

        assert_eq!(decoded.samples.channels, 2);
        assert_eq!(decoded.samples.frames(), 50);
        assert!((decoded.samples.data[0] - 0.5).abs() < 1.0 / 32_000.0);
        assert_eq!(decoded.samples.data[1], 0.0);
    }

    #[test]
    fn a_list_chunk_before_data_is_walked_past() {
        // Build: RIFF/WAVE + LIST chunk + fmt + data.
        let original = tone(8_000, 10);
        let mut file = Vec::new();
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&0_u32.to_le_bytes()); // advisory; never trusted
        file.extend_from_slice(b"WAVE");

        // A `LIST` chunk carrying a 13-byte (odd, padded) info payload.
        file.extend_from_slice(b"LIST");
        let payload = b"INFOmyproject"; // 13 bytes, so one pad byte follows
        file.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        file.extend_from_slice(payload);
        file.push(0); // pad

        file.extend_from_slice(b"fmt ");
        file.extend_from_slice(&16_u32.to_le_bytes());
        file.extend_from_slice(&1_u16.to_le_bytes());
        file.extend_from_slice(&1_u16.to_le_bytes());
        file.extend_from_slice(&8_000_u32.to_le_bytes());
        file.extend_from_slice(&16_000_u32.to_le_bytes());
        file.extend_from_slice(&2_u16.to_le_bytes());
        file.extend_from_slice(&16_u16.to_le_bytes());

        file.extend_from_slice(b"data");
        let pcm: Vec<u8> = original
            .data
            .iter()
            .flat_map(|sample| {
                let quantised = (sample * i16::MAX as f32).round() as i16;
                quantised.to_le_bytes()
            })
            .collect();
        file.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
        file.extend_from_slice(&pcm);

        let decoded = read_wav(&file).unwrap();
        assert_eq!(decoded.samples.data.len(), 10);
    }

    #[test]
    fn not_a_wave_is_said_so() {
        assert_eq!(read_wav(b"NOTARIFFABCD"), Err(PcmError::NotWave));
        assert_eq!(read_wav(b"RIF"), Err(PcmError::Truncated));
    }

    #[test]
    fn a_chunk_past_the_end_is_truncation_not_a_guess() {
        // A data chunk that claims 100 bytes with 4 present.
        let mut file = Vec::new();
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&100_u32.to_le_bytes());
        file.extend_from_slice(b"WAVE");
        file.extend_from_slice(b"data");
        file.extend_from_slice(&100_u32.to_le_bytes());
        file.extend_from_slice(&[0; 4]);
        assert_eq!(read_wav(&file), Err(PcmError::Truncated));
    }

    #[test]
    fn missing_chunks_are_named() {
        // fmt only: no data.
        let mut file = Vec::new();
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&0_u32.to_le_bytes());
        file.extend_from_slice(b"WAVE");
        file.extend_from_slice(b"fmt ");
        file.extend_from_slice(&16_u32.to_le_bytes());
        let mut fmt = vec![0; 16];
        fmt[0..2].copy_from_slice(&1_u16.to_le_bytes()); // plain PCM
        file.extend_from_slice(&fmt);
        assert_eq!(read_wav(&file), Err(PcmError::NoDataChunk));

        // data only: no fmt.
        let mut file = Vec::new();
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&0_u32.to_le_bytes());
        file.extend_from_slice(b"WAVE");
        file.extend_from_slice(b"data");
        file.extend_from_slice(&4_u32.to_le_bytes());
        file.extend_from_slice(&[0; 4]);
        assert_eq!(read_wav(&file), Err(PcmError::NoFormatChunk));
    }

    #[test]
    fn eight_bit_waves_decode_with_silence_at_128() {
        // Hand-build an 8-bit mono WAV: values 128, 255, 0, 129.
        let mut file = Vec::new();
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&0_u32.to_le_bytes());
        file.extend_from_slice(b"WAVE");
        file.extend_from_slice(b"fmt ");
        file.extend_from_slice(&16_u32.to_le_bytes());
        file.extend_from_slice(&1_u16.to_le_bytes());
        file.extend_from_slice(&1_u16.to_le_bytes());
        file.extend_from_slice(&8_000_u32.to_le_bytes());
        file.extend_from_slice(&8_000_u32.to_le_bytes());
        file.extend_from_slice(&1_u16.to_le_bytes());
        file.extend_from_slice(&8_u16.to_le_bytes());
        file.extend_from_slice(b"data");
        file.extend_from_slice(&4_u32.to_le_bytes());
        file.extend_from_slice(&[128, 255, 0, 129]);

        let decoded = read_wav(&file).unwrap();
        assert_eq!(decoded.bits, 8);
        // 8-bit unsigned tops out one step below +1: 127/128.
        assert_eq!(
            decoded.samples.data,
            vec![0.0, 127.0 / 128.0, -1.0, 1.0 / 128.0]
        );
    }

    #[test]
    fn non_pcm_formats_are_refused_with_the_code() {
        let mut file = Vec::new();
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&0_u32.to_le_bytes());
        file.extend_from_slice(b"WAVE");
        file.extend_from_slice(b"fmt ");
        file.extend_from_slice(&16_u32.to_le_bytes());
        let mut fmt = vec![0; 16];
        fmt[0..2].copy_from_slice(&2_u16.to_le_bytes()); // MS ADPCM
        file.extend_from_slice(&fmt);
        file.extend_from_slice(b"data");
        file.extend_from_slice(&0_u32.to_le_bytes());

        assert_eq!(read_wav(&file), Err(PcmError::NotPcm(2)));
    }

    #[test]
    fn every_format_round_trips_through_write_wav_format() {
        use crate::channels::{Layout, MultiSamples};
        let src = MultiSamples {
            data: (0..600).map(|i| ((i as f32) * 0.037).sin() * 0.9).collect(),
            layout: Layout::Surround51,
            rate: 48_000,
        };
        for (fmt, tol) in [
            (SampleFormat::U8, 1.0 / 100.0),
            (SampleFormat::I16, 1.0 / 30_000.0),
            (SampleFormat::I24, 1.0 / 8_000_000.0),
            (SampleFormat::I32, 1e-7),
            (SampleFormat::F32, 0.0),
            (SampleFormat::F64, 0.0),
        ] {
            let bytes = write_wav_format(&src, fmt).unwrap();
            let (back, info) = read_wav_multi(&bytes).unwrap();
            assert_eq!(info.format, fmt);
            assert_eq!(
                info.channel_mask, 0x60F,
                "5.1 written as EXTENSIBLE with its mask"
            );
            assert_eq!(back.layout, Layout::Surround51);
            assert_eq!(back.frames(), 100);
            let worst = back
                .data
                .iter()
                .zip(&src.data)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0, f32::max);
            assert!(worst <= tol, "{fmt:?}: {worst}");
        }
    }

    #[test]
    fn stereo_float_reads_through_the_simple_api_and_surround_is_refused_there() {
        use crate::channels::{Layout, MultiSamples};
        let st = MultiSamples {
            data: vec![0.25, -0.5, 0.75, 1.0],
            layout: Layout::Stereo,
            rate: 44_100,
        };
        let p = read_wav(&write_wav_format(&st, SampleFormat::F32).unwrap()).unwrap();
        assert!(p.float && p.bits == 32);
        assert_eq!(p.samples.data, vec![0.25, -0.5, 0.75, 1.0]);
        let q = MultiSamples {
            data: vec![0.0; 8],
            layout: Layout::Quad,
            rate: 44_100,
        };
        assert!(matches!(
            read_wav(&write_wav_format(&q, SampleFormat::I16).unwrap()),
            Err(PcmError::Unsupported(_))
        ));
    }

    #[test]
    fn reads_ffmpegs_24_bit_fixture() {
        let bytes = include_bytes!("../tests/fixtures/source24m.wav");
        let p = read_wav(bytes).unwrap();
        assert_eq!(
            (p.bits, p.samples.channels, p.samples.rate),
            (24, 1, 44_100)
        );
        assert_eq!(p.samples.frames(), 17_640);
        let peak = p.samples.data.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!((peak - 0.5).abs() < 0.01, "{peak}");
    }

    #[test]
    fn the_enveloped_tone_doctest_holds() {
        // The module doctest's arithmetic, as a test: a rendered click is
        // loud in the middle and silent at the end.
        let click = Tone::new(2_000.0, Waveform::Sine).envelope(Envelope::attack_release(
            std::time::Duration::from_millis(5),
            std::time::Duration::from_millis(40),
        ));
        let rendered = crate::Mixer::new(44_100).add(click, 0.5).render();

        assert_eq!(rendered.channels, 1);
        assert_eq!(rendered.rate, 44_100);
        assert!(rendered.peak() > 0.2);
        assert!(rendered.amplitude_at(std::time::Duration::from_millis(49)) < 0.01);
    }
}
