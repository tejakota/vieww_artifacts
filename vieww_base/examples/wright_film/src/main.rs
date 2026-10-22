//! wright_film — the Wright product film.
//!
//! ```console
//! cargo run --release -p wright_film -- census        # pass 1: walk, count, sample
//! cargo run --release -p wright_film -- master        # pass 2: segments → wright_film.mp4
//! cargo run --release -p wright_film -- preview W06   # one scene, every 12th frame
//! ```
//!
//! Everything lands under `WRIGHT_FILM_OUT` (default `wright_film_out/`):
//! `wright_film.mp4` (1920×1080 · 60 fps · 2:19), `wright_teaser.mp4`
//! (the 60-second door), per-scene segments, 4×4 contact sheets, the
//! census manifest, and `demo-home/` — the real firing the film shows,
//! whose store the proof movement reads.
//!
//! The film's contract: a frame is a pure function of its index, every
//! number in the chrome is derived (never typed), and movement III is
//! the actual application — see `film` for the scene map and `wright_app`
//! for the mounting.

mod film;
mod kit;
mod m0_ask;
mod m1_plan;
mod m2_run;
mod m3_proof;
mod m4_refire;
mod master;
mod wright_app;

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_else(|| {
        eprintln!("wright_film — census | master | preview <scene>");
        std::process::exit(2);
    });
    let rest = args.next().unwrap_or_default();
    let word = if mode == "preview" { rest } else { mode };
    match master::run(&word) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("wright_film: {e}");
            ExitCode::FAILURE
        }
    }
}
