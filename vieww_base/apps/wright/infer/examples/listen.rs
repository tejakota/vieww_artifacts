//! Transcribe a WAV file with a Whisper model, timed:
//! `cargo run --release -p wright-infer --example listen -- <ggml-*.bin> <file.wav> [language]`
use std::time::Instant;

use wright_infer::audio::wav_to_samples;
use wright_infer::whisper::Whisper;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("usage: listen <ggml-*.bin> <file.wav> [language]");
        std::process::exit(2);
    }
    let t = Instant::now();
    let w = Whisper::load(std::path::Path::new(&a[1])).unwrap_or_else(|e| panic!("{e}"));
    println!("load {:.2}s", t.elapsed().as_secs_f32());
    let samples = wav_to_samples(&std::fs::read(&a[2]).unwrap()).unwrap_or_else(|e| panic!("{e}"));
    let t = Instant::now();
    let (mel, n_len) = w.mel(&samples);
    println!("mel {:.2}s", t.elapsed().as_secs_f32());
    let t = Instant::now();
    let _ = w.encode(&mel, n_len, 0);
    println!("encode {:.2}s", t.elapsed().as_secs_f32());
    let t = Instant::now();
    let out = w.transcribe(&samples, a.get(3).map(String::as_str)).unwrap_or_else(|e| panic!("{e}"));
    println!("transcribe {:.2}s ({} tokens, {})", t.elapsed().as_secs_f32(), out.tokens.len(), out.language);
    println!("{}", out.text);
}
