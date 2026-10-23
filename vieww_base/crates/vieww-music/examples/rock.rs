//! Render the 12-bar rock demonstration.
//!
//! `cargo run --release -p vieww-music --example rock -- [out-dir]`

use vieww_music::pieces;
use vieww_music::render::{render_flac, Settings};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/vieww-music".into());
    std::fs::create_dir_all(&dir)?;
    let score = pieces::rock();
    let path = format!("{dir}/rock-12bar.flac");
    std::fs::write(&path, render_flac(&score, Settings::default())?)?;
    std::fs::write(
        format!("{dir}/rock-12bar.mid"),
        score.to_smf(480).to_bytes(),
    )?;
    println!("wrote {path}: {} ({:.1} s)", score.title, score.seconds());
    Ok(())
}
