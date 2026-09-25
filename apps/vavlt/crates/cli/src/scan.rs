//! Metadata-only pass — the desktop mirror of the on-device tier screen.
//!
//! Reads a header prefix to classify, never decodes pixels, and never opens a
//! file it was not pointed at. On device the equivalent pass reads `PHAsset` /
//! `MediaStore` rows and needs no media permission at all.

use crate::fmt;
use anyhow::Result;
use std::path::Path;
use vavlt_core::{classify_path, estimate_library, AssetMeta, Tier};
use walkdir::WalkDir;

pub fn collect(dir: &Path) -> Result<Vec<AssetMeta>> {
    let mut out = Vec::new();
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        match classify_path(entry.path()) {
            Ok(m) => out.push(m),
            Err(e) => eprintln!("skip {}: {e}", entry.path().display()),
        }
    }
    Ok(out)
}

pub fn run(dir: &Path, json: Option<&Path>) -> Result<()> {
    let assets = collect(dir)?;
    if assets.is_empty() {
        println!("No files found under {}", dir.display());
        return Ok(());
    }

    let total: u64 = assets.iter().map(|a| a.bytes).sum();
    fmt::header(&format!(
        "Scanned {} items · {}  (metadata only — no pixels read)",
        assets.len(),
        fmt::bytes(total)
    ));

    let base = estimate_library(&assets, Tier::Move, None);
    println!("{:<16} {:>7} {:>12}", "CLASS", "COUNT", "BYTES");
    for (class, roll) in &base.by_class {
        println!(
            "{:<16} {:>7} {:>12}",
            class,
            roll.count,
            fmt::bytes(roll.bytes)
        );
    }

    fmt::header("Estimated savings per tier");
    println!(
        "{:<12} {:>10} {:>10} {:>10} {:>10} {:>8}",
        "TIER", "CODEC", "DEDUP", "OVERHEAD", "NET", "NET %"
    );

    let mut estimates = Vec::new();
    for tier in Tier::all() {
        let e = estimate_library(&assets, tier, None);
        println!(
            "{:<12} {:>10} {:>10} {:>10} {:>10} {:>7.1}%",
            tier.label(),
            fmt::bytes(e.codec_saved),
            fmt::bytes(e.dedup_saved),
            format!("-{}", fmt::bytes(e.overhead_bytes)),
            fmt::bytes(e.net_saved),
            e.net_pct(),
        );
        estimates.push(e);
    }

    println!(
        "\nOverhead is thumbnails + previews the vavlt must store ({} + {} per media item).",
        fmt::bytes(vavlt_core::estimate::THUMB_BYTES),
        fmt::bytes(vavlt_core::estimate::PREVIEW_BYTES)
    );
    println!("Dedup here is the {:.0}% prior; `measure` counts duplicates exactly.", vavlt_core::estimate::ASSUMED_DEDUP * 100.0);
    println!("These are priors, not measurements. Run `vavlt measure` to replace them.");

    if let Some(p) = json {
        std::fs::write(p, serde_json::to_string_pretty(&estimates)?)?;
        println!("\nWrote {}", p.display());
    }
    Ok(())
}
