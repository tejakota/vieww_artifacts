//! Render the Carnatic demonstration: Mayamalavagowla in Adi tala, with
//! tambura, mridangam, gamakas and a korvai that lands on sam.
//!
//! `cargo run --release -p vieww-music --example carnatic -- [out-dir] [seed]`

use vieww_music::carnatic::swara::write_phrase;
use vieww_music::pieces;
use vieww_music::render::{render_flac, Settings};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| "target/vieww-music".into());
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(15);
    std::fs::create_dir_all(&dir)?;

    let (score, plan) = pieces::carnatic(seed);
    println!(
        "{} — raga {} (melakarta {}), {} ({})",
        score.title,
        plan.raga.name,
        plan.raga.melakarta.number(),
        plan.tala.name(),
        plan.tala.notation()
    );
    println!("melody: {}", write_phrase(&plan.melody));
    println!(
        "korvai: {} at beat {}, landing on sam at beat {}",
        plan.mora.spoken(),
        plan.korvai_start,
        plan.sam
    );
    let path = format!("{dir}/carnatic-seed{seed}.flac");
    std::fs::write(&path, render_flac(&score, Settings::default())?)?;
    let mid = format!("{dir}/carnatic-seed{seed}.mid");
    std::fs::write(&mid, score.to_smf(480).to_bytes())?;
    println!("wrote {path} ({:.1} s) and {mid}", score.seconds());
    Ok(())
}
