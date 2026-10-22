//! Voice input: microphone samples in, text out.
//!
//! ```text
//!  f32 samples (any rate, any channels)  ->  16 kHz mono 16-bit WAV
//!      -> POST <provider base>/audio/transcriptions   (multipart/form-data)
//!      <- {"text": "..."}
//! ```
//!
//! Capture itself lives in the UI (it needs the platform's audio backend);
//! this module is the pure half — resampling, WAV bytes, the request and the
//! answer — so it can be tested with no microphone and no network.
//!
//! By default the model is the built-in Whisper (`whisper-base`, run on
//! this machine by `wright-infer`: speech never leaves it). Any other
//! model goes to OpenAI's speech-to-text shape, which OpenAI, Groq and
//! local servers (whisper.cpp's server, faster-whisper-server, LocalAI)
//! all speak. The model comes from `voice model="..."` in config.kdl and
//! is routed like any other model: the first provider whose `models`
//! pattern matches it serves it.

use crate::config::{Config, ProviderKind};
use crate::net::{self, NetConfig};
use crate::secrets;
use wright_core::wire::http::{parse_url, Request};
use wright_core::wire::json;

/// The rate speech models are trained at; anything higher is wasted bytes.
pub const RATE: u32 = 16_000;

/// The longest recording sent: two minutes is a long instruction, and it
/// keeps one upload under 4 MB.
pub const MAX_SECONDS: usize = 120;

/// Interleaved frames at `rate` with `channels` -> mono at [`RATE`].
///
/// Downmix by averaging the channels, then resample by averaging each
/// output sample's window of input samples (a box filter — crude, and
/// plenty for speech, which carries almost nothing above 4 kHz).
pub fn to_speech_rate(interleaved: &[f32], channels: usize, rate: u32) -> Vec<f32> {
    let channels = channels.max(1);
    let mono: Vec<f32> = interleaved
        .chunks(channels)
        .map(|f| f.iter().sum::<f32>() / f.len() as f32)
        .collect();
    if rate == RATE || rate == 0 {
        return mono;
    }
    let step = rate as f64 / RATE as f64;
    let out_len = (mono.len() as f64 / step) as usize;
    (0..out_len)
        .map(|i| {
            let a = (i as f64 * step) as usize;
            let b = (((i + 1) as f64 * step) as usize).clamp(a + 1, mono.len());
            mono[a..b].iter().sum::<f32>() / (b - a) as f32
        })
        .collect()
}

/// 16-bit PCM mono WAV at [`RATE`].
pub fn wav(samples: &[f32]) -> Vec<u8> {
    let samples = &samples[..samples.len().min(RATE as usize * MAX_SECONDS)];
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes()); // bytes per second
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    out
}

/// The multipart body for one transcription request.
fn multipart(boundary: &str, model: &str, wav: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(wav.len() + 512);
    let field = |b: &mut Vec<u8>, name: &str, value: &str| {
        b.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
    };
    field(&mut b, "model", model);
    field(&mut b, "response_format", "json");
    b.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"voice.wav\"\r\nContent-Type: audio/wav\r\n\r\n").as_bytes());
    b.extend_from_slice(wav);
    b.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    b
}

/// Turn speech into text with the configured voice model.
///
/// # Errors
/// No provider serves the voice model, the provider is not
/// OpenAI-compatible, the key is missing, or the request failed — each in
/// words a person can act on.
pub fn transcribe(cfg: &Config, wav: &[u8]) -> Result<String, String> {
    let model = &cfg.voice_model;
    let Some(p) = cfg.provider_for(model) else {
        return Err(format!(
            "no provider serves the voice model '{model}'. Add one to ~/.wright/config.kdl, e.g. \
             provider \"openai\" base=\"https://api.openai.com/v1\" key=\"keychain:wright/openai\" models=\"gpt-*,whisper-*\""
        ));
    };
    if p.kind == ProviderKind::Builtin {
        return crate::local::transcribe(model, wav);
    }
    if p.kind != ProviderKind::OpenAi {
        return Err(format!(
            "'{}' cannot transcribe speech; point `voice model=` at an OpenAI-compatible provider",
            p.name
        ));
    }
    let url = parse_url(&format!(
        "{}/audio/transcriptions",
        p.base_url.trim_end_matches('/')
    ))?;
    let boundary = format!(
        "wright{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let mut req = Request::new("POST", url)
        .header(
            "Content-Type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .body(multipart(&boundary, model, wav));
    if let Some(reference) = &p.key {
        let key = secrets::resolve(reference)
            .map_err(|e| format!("voice needs the {} key: {e}", p.name))?;
        req = req.header("Authorization", &format!("Bearer {}", key.expose()));
    }
    let resp = net::send(&req, &NetConfig::from_env())?;
    if resp.status != 200 {
        return Err(format!(
            "{} returned HTTP {} for speech-to-text: {}",
            p.name,
            resp.status,
            resp.body_text().chars().take(200).collect::<String>()
        ));
    }
    let v = json::parse(&resp.body_text())
        .map_err(|e| format!("speech-to-text answer was not JSON: {e}"))?;
    Ok(v.str_of("text").unwrap_or("").trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_48k_becomes_mono_16k() {
        // One second of stereo at 48 kHz: left +0.5, right -0.5 -> silence.
        let frames: Vec<f32> = (0..48_000).flat_map(|_| [0.5f32, -0.5]).collect();
        let out = to_speech_rate(&frames, 2, 48_000);
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|s| s.abs() < 1e-6));
    }

    #[test]
    fn the_wav_header_is_what_a_decoder_expects() {
        let w = wav(&[0.0, 1.0, -1.0]);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes([w[24], w[25], w[26], w[27]]), RATE);
        assert_eq!(
            u32::from_le_bytes([w[40], w[41], w[42], w[43]]),
            6,
            "three samples, two bytes each"
        );
        assert_eq!(i16::from_le_bytes([w[46], w[47]]), 32767);
    }

    #[test]
    fn multipart_carries_the_model_and_the_file() {
        let body = multipart("XYZ", "whisper-1", b"RIFFdata");
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("name=\"model\"\r\n\r\nwhisper-1\r\n"));
        assert!(text.contains("filename=\"voice.wav\""));
        assert!(text.ends_with("--XYZ--\r\n"));
    }

    #[test]
    fn a_missing_provider_says_how_to_fix_it() {
        let cfg = Config::parse("voice model=\"nobody-serves-this\"\n", "test").unwrap();
        let e = transcribe(&cfg, &wav(&[0.0])).unwrap_err();
        assert!(e.contains("config.kdl"), "{e}");
    }
}
