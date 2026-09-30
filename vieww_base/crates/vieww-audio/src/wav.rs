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
//! * PCM 16-bit and 8-bit unsigned, mono and stereo — the four combinations
//!   every WAV file in the wild actually is. 24-bit and float are refused
//!   with [`PcmError::Unsupported`] rather than misread, because a codec that
//!   silently drops to a different bit depth is a codec nobody can trust.
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
    /// The bit depth the file carried: 8 or 16.
    pub bits: u16,
}

/// Decode a WAV file.
///
/// # Errors
///
/// Every [`PcmError`], with the reason a file was refused rather than a bare
/// `None` — "this file is 24-bit" is actionable, "this file failed" is not.
pub fn read_wav(bytes: &[u8]) -> Result<PcmSamples, PcmError> {
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
    let mut format: Option<(u16, u16, u16, u32)> = None; // (code, channels, bits, rate)
    let mut data: Option<&[u8]> = None;

    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32::from_le_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]])
            as usize;
        let payload_start = at + 8;
        let payload_end = payload_start + size;
        if payload_end > bytes.len() {
            return Err(PcmError::Truncated);
        }

        if id == b"fmt " {
            if size < 16 {
                return Err(PcmError::Truncated);
            }
            let payload = &bytes[payload_start..payload_end];
            let code = u16::from_le_bytes([payload[0], payload[1]]);
            let channels = u16::from_le_bytes([payload[2], payload[3]]);
            let rate = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
            let bits = u16::from_le_bytes([payload[14], payload[15]]);
            format = Some((code, channels, bits, rate));
        } else if id == b"data" {
            data = Some(&bytes[payload_start..payload_end]);
        }

        // Chunks are word-aligned: an odd size skips one pad byte.
        at = payload_end + (size & 1);
    }

    let Some((code, channels, bits, rate)) = format else {
        return Err(PcmError::NoFormatChunk);
    };
    if code != 1 {
        return Err(PcmError::NotPcm(code));
    }
    let Some(data) = data else {
        return Err(PcmError::NoDataChunk);
    };

    let decoded = match bits {
        16 => decode_16(data),
        8 => decode_8(data),
        24 => return Err(PcmError::Unsupported("24-bit PCM")),
        32 => return Err(PcmError::Unsupported("32-bit PCM or float")),
        _ => return Err(PcmError::Unsupported("an unknown bit depth")),
    };

    match channels {
        1 => Ok(PcmSamples {
            samples: Samples::mono(decoded, rate),
            bits,
        }),
        2 => Ok(PcmSamples {
            samples: Samples::stereo(decoded, rate),
            bits,
        }),
        _ => Err(PcmError::Unsupported("more than two channels")),
    }
}

/// 16-bit signed PCM, little-endian, to `f32` in `-1..=1`.
fn decode_16(data: &[u8]) -> Vec<f32> {
    data.chunks_exact(2)
        .map(|pair| {
            i16::from_le_bytes([pair[0], pair[1]]) as f32 / i16::MAX as f32
        })
        .collect()
}

/// 8-bit unsigned PCM — the WAV oddity: silence is 128, not 0.
fn decode_8(data: &[u8]) -> Vec<f32> {
    data.iter()
        .map(|byte| (i16::from(*byte) - 128) as f32 / 128.0)
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Envelope, Tone, Waveform};

    fn tone(rate: u32, frames: usize) -> Samples {
        #[expect(clippy::cast_precision_loss, reason = "a test tone is exact enough in f32")]
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
        let original = Samples::stereo(
            (0..50).flat_map(|_| [0.5_f32, 0.0]).collect(),
            48_000,
        );
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
        assert_eq!(decoded.samples.data, vec![0.0, 127.0 / 128.0, -1.0, 1.0 / 128.0]);
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
        fmt[0..2].copy_from_slice(&3_u16.to_le_bytes()); // IEEE float
        file.extend_from_slice(&fmt);
        file.extend_from_slice(b"data");
        file.extend_from_slice(&0_u32.to_le_bytes());

        assert_eq!(read_wav(&file), Err(PcmError::NotPcm(3)));
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
