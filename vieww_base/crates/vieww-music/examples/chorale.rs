//! Render the chorale twice — 12-TET and just intonation — to hear the
//! thirds settle.
//!
//! `cargo run --release -p vieww-music --example chorale -- [out-dir]`

use vieww_music::pieces;
use vieww_music::render::{render_flac, Settings};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/vieww-music".into());
    std::fs::create_dir_all(&dir)?;
    for (just, name) in [(false, "chorale-12tet"), (true, "chorale-just")] {
        let score = pieces::chorale(just);
        let path = format!("{dir}/{name}.flac");
        std::fs::write(&path, render_flac(&score, Settings::default())?)?;
        println!("wrote {path}: {} ({:.1} s)", score.title, score.seconds());
    }
    let ratios: Vec<String> = pieces::just_ratios()
        .iter()
        .map(ToString::to_string)
        .collect();
    println!("just chromatic ratios above D: {}", ratios.join(" "));
    Ok(())
}
