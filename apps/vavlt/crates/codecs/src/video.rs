//! Video measurement via the `ffmpeg` CLI.
//!
//! This is the **desktop stand-in only**. On device the engine drives
//! VideoToolbox (`VTCompressionSession`) and NDK `AMediaCodec` directly — both
//! are C APIs callable from Rust with no Swift or Kotlin.
//!
//! Software x265 here will be far slower than a phone's hardware encoder, so
//! treat the *ratio* as meaningful and the *timing* as not transferable.

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;
use std::time::Instant;

pub struct VideoResult {
    pub out_bytes: u64,
    pub encode_ms: u128,
    pub vmaf: Option<f64>,
}

pub fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Does this ffmpeg build have libvmaf? Without it the quality gate can't be
/// measured and ratios alone are misleading.
pub fn vmaf_available() -> bool {
    Command::new("ffmpeg")
        .args(["-hide_banner", "-filters"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("libvmaf"))
        .unwrap_or(false)
}

/// Transcode to HEVC at `crf`, optionally re-encoding audio to Opus.
///
/// Audio matters more than it looks: negligible on 4K (AAC 256k against
/// ~50 Mbps video) but 10-12% of a 720p messaging clip.
pub fn transcode_hevc(
    src: &Path,
    dst: &Path,
    crf: u32,
    opus_kbps: Option<u32>,
    threads: usize,
) -> Result<VideoResult> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(src)
        .args(["-c:v", "libx265", "-crf", &crf.to_string()])
        .args(["-preset", "medium", "-tag:v", "hvc1"])
        .args(["-threads", &threads.to_string()]);

    match opus_kbps {
        Some(k) => cmd.args(["-c:a", "libopus", "-b:a", &format!("{k}k")]),
        None => cmd.args(["-c:a", "copy"]),
    };
    // Carry rotation/creation metadata; losing it is a visible regression.
    cmd.args(["-map_metadata", "0", "-movflags", "+faststart"]).arg(dst);

    let t = Instant::now();
    let out = cmd.output().context("spawning ffmpeg")?;
    let encode_ms = t.elapsed().as_millis();

    if !out.status.success() {
        bail!("ffmpeg: {}", String::from_utf8_lossy(&out.stderr).trim());
    }

    Ok(VideoResult {
        out_bytes: std::fs::metadata(dst)?.len(),
        encode_ms,
        vmaf: None,
    })
}

/// Score `distorted` against `reference`. This is the gate that decides whether
/// a transcode is allowed to replace an original.
pub fn vmaf(reference: &Path, distorted: &Path, threads: usize) -> Result<f64> {
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "info", "-i"])
        .arg(distorted)
        .arg("-i")
        .arg(reference)
        .args([
            "-lavfi",
            &format!("libvmaf=n_threads={threads}"),
            "-f",
            "null",
            "-",
        ])
        .output()
        .context("spawning ffmpeg for vmaf")?;

    let stderr = String::from_utf8_lossy(&out.stderr);
    stderr
        .rsplit_once("VMAF score: ")
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or_else(|| anyhow::anyhow!("could not parse VMAF from ffmpeg output"))
}
