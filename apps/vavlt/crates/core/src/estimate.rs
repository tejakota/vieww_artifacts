//! Metadata-only estimation — the tier screen's source of truth.
//!
//! Reads no pixel data. On device this runs against `PHAsset`/`MediaStore`
//! rows, so it needs no media permission and opens no files.
//!
//! Every constant in [`RATIOS`] is an *assumption*. The `measure` command
//! replaces them with numbers from a real corpus; `vault scan --compare`
//! diffs the two.

use crate::classify::{AssetClass, AssetMeta};
use crate::tier::Tier;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Fraction of bytes saved, per (class, tier). Published-benchmark priors.
pub const RATIOS: &[(AssetClass, [f32; 3])] = &[
    // class                      Move   Deep   Deep+
    (AssetClass::Jpeg, [0.21, 0.50, 0.70]),
    (AssetClass::Png, [0.18, 0.80, 0.85]),
    (AssetClass::Heic, [0.00, 0.00, 0.15]),
    (AssetClass::VideoH264, [0.00, 0.44, 0.70]),
    (AssetClass::VideoHevc, [0.00, 0.10, 0.40]),
    (AssetClass::VideoAv1, [0.00, 0.00, 0.25]),
    (AssetClass::VideoOther, [0.00, 0.30, 0.55]),
    (AssetClass::Raw, [0.03, 0.03, 0.03]),
    (AssetClass::AudioLossless, [0.45, 0.45, 0.60]),
    (AssetClass::AudioLossy, [0.00, 0.00, 0.10]),
    (AssetClass::Document, [0.65, 0.65, 0.65]),
    (AssetClass::Other, [0.10, 0.10, 0.10]),
];

/// Whole-file dedup yield, applied on top. Corpus-dependent; measured exactly
/// by the harness rather than assumed.
pub const ASSUMED_DEDUP: f32 = 0.12;

/// vavlt overhead the user must be told about: grid thumbnails and the
/// full-screen previews the before/after comparison viewer needs.
///
/// Reported savings are **net of this**, per invariant "savings are measured,
/// not projected".
pub const THUMB_BYTES: u64 = 15 * 1024;
pub const PREVIEW_BYTES: u64 = 80 * 1024;

fn ratio(class: AssetClass, tier: Tier) -> f32 {
    let idx = match tier {
        Tier::Move => 0,
        Tier::DeepMove => 1,
        Tier::DeepMovePlus => 2,
    };
    RATIOS
        .iter()
        .find(|(c, _)| *c == class)
        .map(|(_, r)| r[idx])
        .unwrap_or(0.0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Estimate {
    pub tier: Tier,
    pub total_bytes: u64,
    /// Savings from re-encoding alone.
    pub codec_saved: u64,
    /// Savings from whole-file dedup (exact — duplicates are counted, not modeled).
    pub dedup_saved: u64,
    /// Thumbnails + previews the vault must store. Subtracted from the total.
    pub overhead_bytes: u64,
    /// What the user actually gets back. `codec + dedup - overhead`.
    pub net_saved: u64,
    pub untouched_count: usize,
    pub by_class: BTreeMap<String, ClassRollup>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClassRollup {
    pub count: usize,
    pub bytes: u64,
    pub saved: u64,
}

impl Estimate {
    pub fn net_pct(&self) -> f32 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        self.net_saved as f32 / self.total_bytes as f32 * 100.0
    }
}

/// `exact_dupe_bytes` comes from whole-file hashing when available; pass `None`
/// to fall back to [`ASSUMED_DEDUP`].
pub fn estimate_library(
    assets: &[AssetMeta],
    tier: Tier,
    exact_dupe_bytes: Option<u64>,
) -> Estimate {
    let mut by_class: BTreeMap<String, ClassRollup> = BTreeMap::new();
    let mut total = 0u64;
    let mut codec_saved = 0u64;
    let mut untouched = 0usize;
    let mut previewable = 0u64;

    for a in assets {
        total += a.bytes;
        let r = ratio(a.class, tier);
        let saved = (a.bytes as f64 * r as f64) as u64;
        if saved == 0 {
            untouched += 1;
        }
        codec_saved += saved;
        if a.class.is_photo() || a.class.is_video() {
            previewable += 1;
        }

        let e = by_class.entry(a.class.label().to_string()).or_default();
        e.count += 1;
        e.bytes += a.bytes;
        e.saved += saved;
    }

    let dedup_saved =
        exact_dupe_bytes.unwrap_or((total as f64 * ASSUMED_DEDUP as f64) as u64);
    let overhead_bytes = previewable * (THUMB_BYTES + PREVIEW_BYTES);
    let net_saved = (codec_saved + dedup_saved).saturating_sub(overhead_bytes);

    Estimate {
        tier,
        total_bytes: total,
        codec_saved,
        dedup_saved,
        overhead_bytes,
        net_saved,
        untouched_count: untouched,
        by_class,
    }
}
