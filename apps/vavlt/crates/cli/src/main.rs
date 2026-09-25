//! Desktop measurement harness for the vavlt engine.
//!
//! Two commands mirroring the two phases on device:
//!
//!   vavlt scan <dir>     metadata only, no pixel reads — what the tier screen shows
//!   vavlt measure <dir>  runs the real codecs — what the tier screen *should* show
//!
//! The gap between them is the point. Every ratio in `vavlt_core::estimate::RATIOS`
//! is a published-benchmark prior; `measure` replaces it with a number from a
//! real corpus.

mod fmt;
mod measure;
mod scan;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "vavlt", about = "vavlt engine measurement harness")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Classify a library from headers only and print the estimated savings per tier.
    Scan {
        dir: PathBuf,
        /// Write the estimate as JSON.
        #[arg(long)]
        json: Option<PathBuf>,
    },
    /// Run the real codecs and report measured ratios, throughput and roundtrip proof.
    Measure {
        dir: PathBuf,
        /// Cap the number of files (per class) to keep runs quick.
        #[arg(long, default_value_t = 200)]
        limit: usize,
        /// Skip files larger than this many MB.
        #[arg(long, default_value_t = 128)]
        max_mb: u64,
        /// Also transcode video via ffmpeg. Slow — software x265.
        #[arg(long)]
        video: bool,
        /// CRF for the video transcode.
        #[arg(long, default_value_t = 28)]
        crf: u32,
        /// Score every transcode with VMAF. Roughly doubles video runtime.
        #[arg(long)]
        vmaf: bool,
        #[arg(long)]
        json: Option<PathBuf>,
        #[arg(long, default_value_t = 0)]
        jobs: usize,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Scan { dir, json } => scan::run(&dir, json.as_deref()),
        Cmd::Measure {
            dir,
            limit,
            max_mb,
            video,
            crf,
            vmaf,
            json,
            jobs,
        } => measure::run(measure::Args {
            dir,
            limit,
            max_mb,
            video,
            crf,
            vmaf,
            json,
            jobs,
        }),
    }
}
