//! The measurement pass — runs real codecs over a real corpus.
//!
//! This is what turns the priors in `vavlt_core::estimate::RATIOS` into facts.
//! Lossless codecs are proved by decode-and-compare, never assumed.

use crate::fmt;
use crate::scan;
use anyhow::Result;
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use vavlt_codecs::{default_set, video};
use vavlt_core::{content_hash, report, AssetClass, FileResult, Measurement};

pub struct Args {
    pub dir: PathBuf,
    pub limit: usize,
    pub max_mb: u64,
    pub video: bool,
    pub crf: u32,
    pub vmaf: bool,
    pub json: Option<PathBuf>,
    pub jobs: usize,
}

pub fn run(args: Args) -> Result<()> {
    if args.jobs > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(args.jobs)
            .build_global()
            .ok();
    }

    let all = scan::collect(&args.dir)?;
    if all.is_empty() {
        println!("No files found under {}", args.dir.display());
        return Ok(());
    }

    // Cap per class so one huge bucket doesn't dominate the run.
    let max_bytes = args.max_mb * 1024 * 1024;
    let mut per_class: HashMap<AssetClass, usize> = HashMap::new();
    let selected: Vec<_> = all
        .iter()
        .filter(|a| a.bytes <= max_bytes && a.bytes > 0)
        .filter(|a| {
            let c = per_class.entry(a.class).or_insert(0);
            *c += 1;
            *c <= args.limit
        })
        .cloned()
        .collect();

    println!(
        "Measuring {} of {} files (limit {}/class, max {} MB)…",
        selected.len(),
        all.len(),
        args.limit,
        args.max_mb
    );

    let codecs = default_set();

    let results: Vec<FileResult> = selected
        .par_iter()
        .filter_map(|asset| {
            let bytes = std::fs::read(&asset.path).ok()?;
            let hash = content_hash(&bytes);

            let measurements = codecs
                .iter()
                .filter(|c| c.applies(asset.class))
                .map(|c| match c.run(&bytes) {
                    Ok(o) => Measurement {
                        codec: c.name().to_string(),
                        lossless: c.lossless(),
                        in_bytes: asset.bytes,
                        out_bytes: o.out_bytes,
                        encode_ms: o.encode_ms,
                        roundtrip_ok: o.roundtrip_ok,
                        decode_ms: o.decode_ms,
                        // No perceptual gate in the harness yet: SSIMULACRA2
                        // lands with the photo lossy path.
                        quality: None,
                        error: None,
                    },
                    Err(e) => Measurement {
                        codec: c.name().to_string(),
                        lossless: c.lossless(),
                        in_bytes: asset.bytes,
                        out_bytes: asset.bytes,
                        encode_ms: 0,
                        roundtrip_ok: None,
                        decode_ms: None,
                        quality: None,
                        error: Some(e.to_string()),
                    },
                })
                .collect();

            Some(FileResult {
                path: asset.path.clone(),
                class: asset.class,
                bytes: asset.bytes,
                hash,
                measurements,
            })
        })
        .collect();

    let mut results = results;
    if args.video {
        // Merge into the existing rows rather than appending: a second
        // FileResult for the same path would double-count bytes and corrupt
        // both the class rollup and the dedup count.
        let extra = measure_video(&selected, args.crf, args.vmaf)?;
        let by_path: HashMap<String, usize> = results
            .iter()
            .enumerate()
            .map(|(i, r)| (r.path.clone(), i))
            .collect();
        for (path, m) in extra {
            match by_path.get(&path) {
                Some(&i) => results[i].measurements.push(m),
                None => eprintln!("video result for unknown path {path}"),
            }
        }
    }

    let agg = report::aggregate(&results);
    print_report(&agg, args.video);

    if let Some(p) = &args.json {
        std::fs::write(p, serde_json::to_string_pretty(&agg)?)?;
        println!("\nWrote {}", p.display());
    }
    Ok(())
}

/// Video goes through ffmpeg serially — x265 already saturates all cores, and
/// running several in parallel just distorts the timings.
fn measure_video(
    assets: &[vavlt_core::AssetMeta],
    crf: u32,
    want_vmaf: bool,
) -> Result<Vec<(String, Measurement)>> {
    if !video::ffmpeg_available() {
        println!("\n[video] ffmpeg not found — skipping video measurement.");
        return Ok(Vec::new());
    }
    let vmaf_ok = !want_vmaf || video::vmaf_available();
    if want_vmaf && !vmaf_ok {
        println!("[video] this ffmpeg has no libvmaf — ratios only, no quality gate.");
    }

    let vids: Vec<_> = assets.iter().filter(|a| a.class.is_video()).collect();
    if vids.is_empty() {
        return Ok(Vec::new());
    }
    println!("\n[video] transcoding {} clips to HEVC crf={crf} (software x265)…", vids.len());

    let tmp = std::env::temp_dir().join("vavlt-measure");
    std::fs::create_dir_all(&tmp)?;
    let threads = num_threads();
    let mut out = Vec::new();

    for a in vids {
        let src = PathBuf::from(&a.path);
        let dst = tmp.join(format!("{}.mp4", content_hash(a.path.as_bytes())));

        let m = match video::transcode_hevc(&src, &dst, crf, Some(64), threads) {
            Ok(mut r) => {
                if want_vmaf && vmaf_ok {
                    r.vmaf = video::vmaf(&src, &dst, threads).ok();
                }
                match r.vmaf {
                    Some(v) => println!(
                        "  {} → {} (VMAF {v:.1})",
                        fmt::bytes(a.bytes),
                        fmt::bytes(r.out_bytes)
                    ),
                    None => println!(
                        "  {} → {} (unscored)",
                        fmt::bytes(a.bytes),
                        fmt::bytes(r.out_bytes)
                    ),
                }
                Measurement {
                    codec: format!("hevc-crf{crf}"),
                    lossless: false,
                    in_bytes: a.bytes,
                    out_bytes: r.out_bytes,
                    encode_ms: r.encode_ms,
                    roundtrip_ok: None,
                    decode_ms: None,
                    quality: r.vmaf,
                    error: None,
                }
            }
            Err(e) => Measurement {
                codec: format!("hevc-crf{crf}"),
                lossless: false,
                in_bytes: a.bytes,
                out_bytes: a.bytes,
                encode_ms: 0,
                roundtrip_ok: None,
                decode_ms: None,
                quality: None,
                error: Some(e.to_string()),
            },
        };
        let _ = std::fs::remove_file(&dst);
        out.push((a.path.clone(), m));
    }
    Ok(out)
}

fn num_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

fn print_report(agg: &report::Aggregate, showed_video: bool) {
    fmt::header(&format!(
        "Measured {} files · {}",
        agg.files,
        fmt::bytes(agg.total_bytes)
    ));

    println!("{:<16} {:>7} {:>12}", "CLASS", "COUNT", "BYTES");
    for (class, (count, bytes)) in &agg.by_class {
        println!("{:<16} {:>7} {:>12}", class, count, fmt::bytes(*bytes));
    }

    fmt::header("Codec results (measured, not estimated)");
    println!(
        "{:<26} {:>6} {:>10} {:>10} {:>9} {:>8} {:>22}",
        "CLASS / CODEC", "FILES", "IN", "OUT", "SAVED", "MB/s", "VERIFICATION"
    );

    let mut unqualified = 0usize;
    for (key, c) in &agg.by_codec {
        // A lossy ratio must never render as "ok". Either a perceptual score
        // backs it, or it is labelled unqualified.
        let verification = if c.lossless {
            if c.roundtrip_failures > 0 {
                format!("ROUNDTRIP FAIL x{}", c.roundtrip_failures)
            } else {
                "lossless, roundtrip ok".to_string()
            }
        } else {
            match (c.quality_mean(), c.quality_min) {
                (Some(mean), Some(min)) if c.quality_scored == c.files => {
                    format!("lossy, q {mean:.1} min {min:.1}")
                }
                (Some(mean), _) => {
                    unqualified += 1;
                    format!("lossy, {}/{} scored q{mean:.0}", c.quality_scored, c.files)
                }
                _ => {
                    unqualified += 1;
                    "lossy, UNQUALIFIED".to_string()
                }
            }
        };

        println!(
            "{:<26} {:>6} {:>10} {:>10} {:>8.1}% {:>8.1} {:>22}",
            key,
            c.files,
            fmt::bytes(c.in_bytes),
            fmt::bytes(c.out_bytes),
            c.saved_pct(),
            c.throughput_mbs(),
            verification
        );
        if c.errors > 0 {
            println!("{:<26} {:>6} errors", "", c.errors);
        }
    }

    if unqualified > 0 {
        println!(
            "\n{unqualified} lossy row(s) have no perceptual score. Those savings are NOT",
        );
        println!("comparable to the lossless rows and must not be quoted as achievable —");
        println!("a quality gate (SSIMULACRA2 for images, VMAF for video) decides what ships.");
    }

    fmt::header("Whole-file dedup (exact)");
    println!(
        "{} duplicate files · {} recoverable ({:.1}% of corpus)",
        agg.exact_dupe_files,
        fmt::bytes(agg.exact_dupe_bytes),
        if agg.total_bytes > 0 {
            agg.exact_dupe_bytes as f32 / agg.total_bytes as f32 * 100.0
        } else {
            0.0
        }
    );
    println!("Near-duplicates (bursts, re-saves, edits) are NOT counted here — that needs the embedding model.");

    if !showed_video {
        println!("\nVideo not measured. Re-run with --video to include the H.264 → HEVC path,");
        println!("which is where the majority of a real phone library's bytes live.");
    }
    println!("\nNote: throughput is desktop x86. Phone HW encoders are far faster for video,");
    println!("somewhat slower for CPU codecs. Ratios transfer; timings do not.");
}
