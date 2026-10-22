//! Sound in: WAV bytes to 16 kHz mono samples, and samples to the log-mel
//! spectrogram Whisper listens to.
//!
//! ```text
//!  WAV (PCM 8/16/24/32-bit or float, any rate, any channels)
//!    -> mono (channels averaged) -> 16 kHz (linear interpolation)
//!  samples -> frames of 400 every 160 (25 ms every 10 ms), Hann window
//!          -> |DFT|^2, 201 bins -> 80 mel bands (the model's filters)
//!          -> log10, floor at (max - 8), (x + 4) / 4
//! ```
//!
//! The spectrogram is whisper.cpp's `log_mel_spectrogram` step for step:
//! the same padding (200 reflected samples before, 30 s of silence after),
//! the same frame count, the same clamping.

pub const SAMPLE_RATE: u32 = 16_000;
pub const N_FFT: usize = 400;
pub const HOP: usize = 160;
/// Frames in one 30-second window.
pub const WINDOW_FRAMES: usize = 3000;

/// Decode a WAV file to 16 kHz mono floats in [-1, 1].
pub fn wav_to_samples(bytes: &[u8]) -> Result<Vec<f32>, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at =
        |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let mut at = 12;
    let mut fmt: Option<(u16, u16, u32, u16)> = None; // format, channels, rate, bits
    let mut data: Option<&[u8]> = None;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let n = u32_at(at + 4) as usize;
        let body = &bytes[at + 8..(at + 8 + n).min(bytes.len())];
        match id {
            b"fmt " if body.len() >= 16 => {
                let mut format = u16_at(at + 8);
                if format == 0xFFFE && body.len() >= 26 {
                    format = u16_at(at + 8 + 24); // WAVE_FORMAT_EXTENSIBLE: the real format
                }
                fmt = Some((format, u16_at(at + 10), u32_at(at + 12), u16_at(at + 22)));
            }
            b"data" => data = Some(body),
            _ => {}
        }
        at += 8 + n + (n & 1);
    }
    let (format, channels, rate, bits) = fmt.ok_or("the WAV file has no format chunk")?;
    let data = data.ok_or("the WAV file has no data")?;
    if channels == 0 || rate == 0 {
        return Err("the WAV file has no channels".into());
    }
    let width = usize::from(bits / 8);
    let sample = |b: &[u8]| -> Option<f32> {
        Some(match (format, bits) {
            (1, 8) => (f32::from(b[0]) - 128.0) / 128.0,
            (1, 16) => f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0,
            (1, 24) => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0,
            (1, 32) => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0,
            (3, 32) => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            _ => return None,
        })
    };
    let frame = width * usize::from(channels);
    if frame == 0 {
        return Err(format!("{bits}-bit samples are not supported"));
    }
    let mut mono = Vec::with_capacity(data.len() / frame);
    for f in data.chunks_exact(frame) {
        let mut sum = 0.0;
        for c in f.chunks_exact(width) {
            sum += sample(c).ok_or_else(|| format!("WAV format {format} with {bits}-bit samples is not supported (use PCM or float)"))?;
        }
        mono.push(sum / f32::from(channels));
    }
    Ok(resample(&mono, rate, SAMPLE_RATE))
}

/// Linear-interpolation resampling. Speech recognition does not need more.
pub fn resample(x: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || x.is_empty() {
        return x.to_vec();
    }
    let n = (x.len() as u64 * u64::from(to) / u64::from(from)) as usize;
    let step = f64::from(from) / f64::from(to);
    (0..n)
        .map(|i| {
            let pos = i as f64 * step;
            let j = pos as usize;
            let frac = (pos - j as f64) as f32;
            let a = x[j.min(x.len() - 1)];
            let b = x[(j + 1).min(x.len() - 1)];
            a + (b - a) * frac
        })
        .collect()
}

/// The log-mel spectrogram: `n_mel` rows of `n_len` frames (row-major),
/// and `n_len`. `filters` is `n_mel x 201` from the model file.
pub fn log_mel(samples: &[f32], filters: &[f32], n_mel: usize) -> (Vec<f32>, usize) {
    let n_bins = N_FFT / 2 + 1;
    let pad = N_FFT / 2;
    let n = samples.len();
    // [200 reflected][samples][30 s + 200 of zeros]
    let mut padded = vec![0.0f32; n + SAMPLE_RATE as usize * 30 + 2 * pad];
    padded[pad..pad + n].copy_from_slice(samples);
    let reflect = pad.min(n.saturating_sub(1));
    for i in 0..reflect {
        padded[pad - 1 - i] = samples[1 + i];
    }
    let n_len = (padded.len() - N_FFT) / HOP;
    let real = n + pad; // past this, frames are silence
    let hann: Vec<f32> = (0..N_FFT)
        .map(|i| 0.5 * (1.0 - (2.0 * std::f64::consts::PI * i as f64 / N_FFT as f64).cos() as f32))
        .collect();
    let (cos, sin): (Vec<f32>, Vec<f32>) = (0..N_FFT)
        .map(|i| (2.0 * std::f64::consts::PI * i as f64 / N_FFT as f64).sin_cos())
        .map(|(s, c)| (c as f32, s as f32))
        .unzip();
    let floor = (1e-10f64).log10() as f32;
    let mut mel = vec![floor; n_mel * n_len];
    let mut frame = vec![0.0f32; N_FFT];
    let mut power = vec![0.0f32; n_bins];
    for i in 0..(real / HOP + 1).min(n_len) {
        let off = i * HOP;
        let take = N_FFT.min(real - off);
        for j in 0..N_FFT {
            frame[j] = if j < take {
                hann[j] * padded[off + j]
            } else {
                0.0
            };
        }
        for (k, p) in power.iter_mut().enumerate() {
            let (mut re, mut im) = (0.0f32, 0.0f32);
            for (j, x) in frame.iter().enumerate() {
                let idx = k * j % N_FFT;
                re += x * cos[idx];
                im -= x * sin[idx];
            }
            *p = re * re + im * im;
        }
        for j in 0..n_mel {
            let sum: f64 = power
                .iter()
                .zip(&filters[j * n_bins..(j + 1) * n_bins])
                .map(|(p, f)| f64::from(*p) * f64::from(*f))
                .sum();
            mel[j * n_len + i] = sum.max(1e-10).log10() as f32;
        }
    }
    let max = mel.iter().fold(f32::MIN, |m, v| m.max(*v)) - 8.0;
    for v in &mut mel {
        *v = (v.max(max) + 4.0) / 4.0;
    }
    (mel, n_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(rate: u32, channels: u16, frames: &[i16]) -> Vec<u8> {
        let data: Vec<u8> = frames.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut b = b"RIFF".to_vec();
        b.extend((36 + data.len() as u32).to_le_bytes());
        b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(channels.to_le_bytes());
        b.extend(rate.to_le_bytes());
        b.extend((rate * u32::from(channels) * 2).to_le_bytes());
        b.extend((channels * 2).to_le_bytes());
        b.extend(16u16.to_le_bytes());
        b.extend(b"data");
        b.extend((data.len() as u32).to_le_bytes());
        b.extend(data);
        b
    }

    #[test]
    fn wav_decodes_to_16k_mono() {
        let s = wav_to_samples(&wav(16_000, 1, &[0, 16384, -32768])).unwrap();
        assert_eq!(s, vec![0.0, 0.5, -1.0]);
        // Stereo 48 kHz: channels averaged, a third of the frames.
        let frames: Vec<i16> = (0..4800).flat_map(|_| [16384i16, 0]).collect();
        let s = wav_to_samples(&wav(48_000, 2, &frames)).unwrap();
        assert_eq!(s.len(), 1600);
        assert!(s.iter().all(|v| (*v - 0.25).abs() < 1e-6));
        assert!(wav_to_samples(b"not audio").is_err());
    }

    #[test]
    fn a_tone_lights_up_its_mel_band() {
        // Identity-ish filters: band j = bin 10 j.
        let (n_mel, bins) = (20, N_FFT / 2 + 1);
        let mut filters = vec![0.0f32; n_mel * bins];
        for j in 0..n_mel {
            filters[j * bins + 10 * j] = 1.0;
        }
        // 1 kHz = bin 25 (16000 / 400 = 40 Hz a bin)... use 1600 Hz = bin 40 = band 4.
        let x: Vec<f32> = (0..16_000)
            .map(|i| (2.0 * std::f32::consts::PI * 1600.0 * i as f32 / 16_000.0).sin())
            .collect();
        let (mel, n_len) = log_mel(&x, &filters, n_mel);
        assert_eq!(n_len, (16_000 + 480_000) / HOP);
        let mid = 50;
        let band = (0..n_mel)
            .max_by(|a, b| mel[a * n_len + mid].total_cmp(&mel[b * n_len + mid]))
            .unwrap();
        assert_eq!(band, 4);
        // Silence after the sound sits at the floor: (max - 8 + 4) / 4.
        let max = mel.iter().fold(f32::MIN, |m, v| m.max(*v));
        assert!((mel[n_len - 1] - (max - 2.0)).abs() < 1e-5);
    }
}
