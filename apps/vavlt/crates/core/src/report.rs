//! Measurement results. This is what turns the priors in `estimate::RATIOS`
//! into numbers from a real corpus.

use crate::classify::AssetClass;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One codec run against one file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Measurement {
    pub codec: String,
    /// Whether this codec claims byte- or pixel-exact reconstruction. Drives
    /// how the result is reported: a lossy ratio must never be presented in a
    /// way that reads as "verified".
    pub lossless: bool,
    pub in_bytes: u64,
    pub out_bytes: u64,
    pub encode_ms: u128,
    /// Present only for lossless codecs: decode + byte-compare actually ran.
    pub roundtrip_ok: Option<bool>,
    pub decode_ms: Option<u128>,
    /// Perceptual score for lossy codecs — VMAF for video, SSIMULACRA2 for
    /// images. `None` means the gate did not run, and the ratio above is
    /// therefore unqualified.
    pub quality: Option<f64>,
    pub error: Option<String>,
}

impl Measurement {
    pub fn saved_pct(&self) -> f32 {
        if self.in_bytes == 0 {
            return 0.0;
        }
        (1.0 - self.out_bytes as f32 / self.in_bytes as f32) * 100.0
    }

    /// MB/s of input consumed. The number that decides whether a codec is
    /// viable on a phone at all.
    pub fn throughput_mbs(&self) -> f32 {
        if self.encode_ms == 0 {
            return f32::INFINITY;
        }
        (self.in_bytes as f32 / 1_048_576.0) / (self.encode_ms as f32 / 1000.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileResult {
    pub path: String,
    pub class: AssetClass,
    pub bytes: u64,
    pub hash: String,
    pub measurements: Vec<Measurement>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodecRollup {
    pub files: usize,
    pub lossless: bool,
    pub in_bytes: u64,
    pub out_bytes: u64,
    pub encode_ms: u128,
    pub roundtrip_failures: usize,
    pub errors: usize,
    /// Perceptual scores, where the gate ran. `quality_scored < files` means
    /// some of this row's ratio is unqualified.
    pub quality_scored: usize,
    pub quality_sum: f64,
    pub quality_min: Option<f64>,
}

impl CodecRollup {
    pub fn saved_pct(&self) -> f32 {
        if self.in_bytes == 0 {
            return 0.0;
        }
        (1.0 - self.out_bytes as f32 / self.in_bytes as f32) * 100.0
    }
    pub fn throughput_mbs(&self) -> f32 {
        if self.encode_ms == 0 {
            return f32::INFINITY;
        }
        (self.in_bytes as f32 / 1_048_576.0) / (self.encode_ms as f32 / 1000.0)
    }

    pub fn quality_mean(&self) -> Option<f64> {
        (self.quality_scored > 0).then(|| self.quality_sum / self.quality_scored as f64)
    }

    /// True when this row's savings are quality-verified end to end: lossless
    /// codecs with a clean roundtrip, or lossy codecs scored on every file.
    pub fn verified(&self) -> bool {
        if self.lossless {
            self.roundtrip_failures == 0 && self.errors == 0
        } else {
            self.quality_scored == self.files && self.files > 0
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aggregate {
    pub files: usize,
    pub total_bytes: u64,
    /// Bytes recoverable by whole-file dedup alone — exact, not modeled.
    pub exact_dupe_bytes: u64,
    pub exact_dupe_files: usize,
    pub by_class: BTreeMap<String, (usize, u64)>,
    /// Keyed `"<class>/<codec>"` so JPEG-Lepton and PNG-oxipng stay separate.
    pub by_codec: BTreeMap<String, CodecRollup>,
}

pub fn aggregate(results: &[FileResult]) -> Aggregate {
    let mut by_class: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    let mut by_codec: BTreeMap<String, CodecRollup> = BTreeMap::new();
    let mut seen: BTreeMap<&str, (usize, u64)> = BTreeMap::new();
    let mut total = 0u64;
    let mut dupe_bytes = 0u64;
    let mut dupe_files = 0usize;

    for r in results {
        total += r.bytes;

        let e = by_class.entry(r.class.label().to_string()).or_default();
        e.0 += 1;
        e.1 += r.bytes;

        // Whole-file dedup: every occurrence after the first is recoverable.
        let d = seen.entry(r.hash.as_str()).or_insert((0, r.bytes));
        d.0 += 1;
        if d.0 > 1 {
            dupe_files += 1;
            dupe_bytes += r.bytes;
        }

        for m in &r.measurements {
            let key = format!("{}/{}", r.class.label(), m.codec);
            let c = by_codec.entry(key).or_default();
            if m.error.is_some() {
                c.errors += 1;
                continue;
            }
            c.files += 1;
            c.lossless = m.lossless;
            c.in_bytes += m.in_bytes;
            c.out_bytes += m.out_bytes;
            c.encode_ms += m.encode_ms;
            if m.roundtrip_ok == Some(false) {
                c.roundtrip_failures += 1;
            }
            if let Some(q) = m.quality {
                c.quality_scored += 1;
                c.quality_sum += q;
                c.quality_min = Some(c.quality_min.map_or(q, |m| m.min(q)));
            }
        }
    }

    Aggregate {
        files: results.len(),
        total_bytes: total,
        exact_dupe_bytes: dupe_bytes,
        exact_dupe_files: dupe_files,
        by_class,
        by_codec,
    }
}
