//! Time the text model: `cargo run --release -p wright-infer --example bench
//! -- <model.gguf> [prompt tokens] [new tokens]` (`T=<n>` sets threads).
//!
//! Prefill (the prompt) is bound by arithmetic, decode (each new token) by
//! how fast memory streams the weights past the cores.

use std::time::Instant;

use wright_infer::qwen3::Model;
use wright_infer::sample::Sampler;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let Some(path) = a.get(1) else {
        eprintln!("usage: bench <model.gguf> [prompt tokens] [new tokens]");
        std::process::exit(2);
    };
    let t0 = Instant::now();
    let mut m = Model::load(std::path::Path::new(path)).unwrap_or_else(|e| panic!("{e}"));
    if let Some(n) = std::env::var("T").ok().and_then(|s| s.parse().ok()) {
        m.set_threads(n);
    }
    let np: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(200);
    let nd: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(32);
    println!(
        "load {:.2}s ({:.2} GB)",
        t0.elapsed().as_secs_f32(),
        m.weight_bytes() as f32 / 1e9
    );
    let ids: Vec<u32> = (0..np as u32).map(|i| 1000 + i).collect();
    let mut c = m.cache(np + nd + 1);
    let t = Instant::now();
    let mut l = m.forward(&mut c, &ids).unwrap();
    let pf = t.elapsed().as_secs_f32();
    println!(
        "prefill {np} tokens: {pf:.2}s ({:.1} tok/s)",
        np as f32 / pf
    );
    let t = Instant::now();
    let mut s = Sampler::greedy();
    for _ in 0..nd {
        let tok = s.sample(&l);
        l = m.forward(&mut c, &[tok]).unwrap();
    }
    let d = t.elapsed().as_secs_f32();
    println!("decode {nd} tokens: {d:.2}s ({:.2} tok/s)", nd as f32 / d);
}
