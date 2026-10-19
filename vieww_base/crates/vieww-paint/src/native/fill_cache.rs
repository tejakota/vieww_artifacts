//! The general path fill, remembered: `paint_shape`'s coverage masks, keyed
//! on the exact bits that produce them.
//!
//! # The gap this closes
//!
//! Every repeat-shaped cost in this renderer grew a cache when a measurement
//! named it: clip masks (`ClipCache`), shadow patches (`ShadowCache`), glyph
//! outlines (`GlyphRasterCache`), blur bands (`effects::blur_cached`), text
//! shaping (`vieww-text`'s shape cache). One hot site stayed uncached: the
//! **general** fill. [`super::reference`]'s `paint_shape` calls
//! `rasterize(polylines, clip)` on every non-rectangular fill, every repaint
//! — icons, rounded outlines, arbitrary vector shapes — and the scanline walk
//! is the most expensive thing a fill does. A damaged repaint re-runs every
//! command overlapping the damage; a full repaint (scroll, theme flip, window
//! resize) re-runs all of them. Static shapes paid for their masks every
//! time, and the input to that walk is a pure function of its inputs.
//!
//! # What is cached, and what the key is
//!
//! [`rasterize`](super::geometry::fill::rasterize) reads exactly two things:
//! the polylines (their point bits, their closed flags, their count) and the
//! clip rectangle. The key is an FNV-1a 64 over all of it, byte-wise, the
//! same discipline as the blur band's hash — a change in any bit of any
//! coordinate spreads. Two keys are equal exactly when the walk's inputs are
//! equal, so a hit is definitionally the same bytes a miss would have
//! produced: **byte-identity here is structural** — the oracle is
//! memoised, never reimplemented. (A 64-bit hash collision would hand one
//! shape another's mask; the blur band hash already carries that arithmetic,
//! and both can widen to 128 bits the day it stops being hypothetical.)
//!
//! The rectangle fast path is *not* intercepted — `paint_shape` calls
//! `rasterize`, which takes it before this cache is consulted, so a
//! four-point panel never pays the hash at all.
//!
//! # The budget
//!
//! [`super::residency::ResidencyCache`], the same budgeted LRU the blur bands
//! and glyph outlines use: default 8 MiB (`VIEWW_PATH_CACHE_MB` to move it),
//! one oversized one-off admitted over budget per `residency.rs`'s softness
//! rule, `0` a true bypass — no hashing, no storage, straight to the walk —
//! because a caller who asks for zero cache should not pay even the hash.

use vieww_foundation::Rect;

use super::geometry::fill::rasterize;
use super::geometry::flatten::Polyline;
use super::residency::ResidencyCache;

/// The default budget: the blur cache's figure. A full-screen HD coverage
/// mask is about 8 MB, so the budget holds roughly one of those or a few
/// dozen panel-shaped masks — and the softness rule means a single giant
/// one-off still works, it just evicts its neighbours.
pub(crate) const DEFAULT_FILL_CACHE_BUDGET_BYTES: usize = 8 * 1024 * 1024;

/// FNV-1a 64, byte-wise, over everything [`rasterize`] reads: each
/// polyline's point count and point bits and closed flag, the polyline
/// count, and the clip rectangle's four bits.
fn hash_polylines(polylines: &[Polyline]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mix = |hash: &mut u64, bytes: &[u8]| {
        for &byte in bytes {
            *hash ^= byte as u64;
            *hash = hash.wrapping_mul(0x100_0000_01b3);
        }
    };
    mix(&mut hash, &polylines.len().to_le_bytes());
    for polyline in polylines {
        mix(&mut hash, &polyline.points.len().to_le_bytes());
        for &(x, y) in &polyline.points {
            mix(&mut hash, &x.to_bits().to_le_bytes());
            mix(&mut hash, &y.to_bits().to_le_bytes());
        }
        mix(&mut hash, &[u8::from(polyline.closed)]);
    }
    hash
}

/// The fill cache's key: the content hash of the polylines plus the clip
/// rectangle's exact bits. [`Eq`] on this is "the walk's inputs are equal",
/// which is what makes a hit the same bytes as a miss.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FillKey {
    clip: [u32; 4],
    polylines: u64,
}

impl FillKey {
    fn new(polylines: &[Polyline], clip: Rect) -> Self {
        Self {
            clip: [
                clip.left.to_bits(),
                clip.top.to_bits(),
                clip.right.to_bits(),
                clip.bottom.to_bits(),
            ],
            polylines: hash_polylines(polylines),
        }
    }
}

/// The fill cache: budgeted, LRU-evicting, stats-carrying — the reasoning
/// the blur cache's doc records ("a derived, deterministic, read-only value
/// re-derivable at any time") applies to a coverage mask word for word.
pub(crate) struct FillCache {
    inner: ResidencyCache<FillKey, super::geometry::fill::CoverageMask>,
    /// `false` only when `VIEWW_PATH_CACHE_MB=0`: a bypass skips even the
    /// key hash, and the stats stay at zero because no cache is running.
    enabled: bool,
}

impl FillCache {
    /// A fill cache at the default budget, or the `VIEWW_PATH_CACHE_MB` one
    /// when that is set and parses — read here rather than at the call sites
    /// so every renderer in the process gets the same trade, as the blur
    /// cache's constructor does.
    pub(crate) fn from_env() -> Self {
        let budget = std::env::var("VIEWW_PATH_CACHE_MB")
            .ok()
            .as_deref()
            .and_then(super::pool::mb_env_bytes);
        match budget {
            Some(0) => Self {
                inner: ResidencyCache::new(0),
                enabled: false,
            },
            Some(bytes) => Self {
                inner: ResidencyCache::new(bytes),
                enabled: true,
            },
            None => Self {
                inner: ResidencyCache::new(DEFAULT_FILL_CACHE_BUDGET_BYTES),
                enabled: true,
            },
        }
    }

    /// The coverage mask for these polylines over this clip — resident if
    /// the exact input is, walked and stored if not. The returned mask is
    /// a clone: `paint_shape` mutates nothing it reads but owns the value
    /// beyond the cache's eviction horizon.
    pub(crate) fn coverage(
        &mut self,
        polylines: &[Polyline],
        clip: Rect,
    ) -> super::geometry::fill::CoverageMask {
        if !self.enabled {
            return rasterize(polylines, clip);
        }
        let key = FillKey::new(polylines, clip);
        self.inner
            .get_or_insert_with(
                key,
                || rasterize(polylines, clip),
                |mask| mask.data.len() * std::mem::size_of::<f32>(),
            )
            .clone()
    }

    /// Hits, misses and evictions over this cache's lifetime.
    pub(crate) fn stats(&self) -> super::residency::ResidencyStats {
        self.inner.stats()
    }
}

#[cfg(test)]
mod tests {
    use super::super::geometry::fill::CoverageMask;
    use super::*;

    fn lcg(state: &mut u64) -> f32 {
        *state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((*state >> 33) as f64 / ((u64::MAX >> 33) as f64) * 200.0) as f32
    }

    /// A deterministic irregular polygon — not axis-aligned, so it takes
    /// the scanline walk, not the rectangle fast path.
    fn blob(seed: u64) -> Vec<Polyline> {
        let mut state = seed;
        let mut points = Vec::new();
        for i in 0..7 {
            let a = (i as f32 / 7.0) * std::f32::consts::TAU;
            let r = 10.0 + lcg(&mut state) * 20.0;
            points.push((60.0 + r * a.cos(), 60.0 + r * a.sin()));
        }
        vec![Polyline {
            points,
            closed: true,
        }]
    }

    fn assert_same_mask(a: &CoverageMask, b: &CoverageMask) {
        assert_eq!(a.x0, b.x0);
        assert_eq!(a.y0, b.y0);
        assert_eq!(a.width, b.width);
        assert_eq!(a.height, b.height);
        for (x, y) in a.data.iter().zip(b.data.iter()) {
            assert_eq!(x.to_bits(), y.to_bits());
        }
    }

    #[test]
    fn a_second_identical_fill_is_a_hit_and_the_same_bytes() {
        let mut cache = FillCache::from_env();
        let polylines = blob(3);
        let clip = Rect::new(0.0, 0.0, 200.0, 200.0);

        let first = cache.coverage(&polylines, clip);
        assert_same_mask(&first, &rasterize(&polylines, clip));

        let second = cache.coverage(&polylines, clip);
        assert_same_mask(&first, &second);

        let stats = cache.stats();
        assert_eq!(stats.hits, 1, "second call must hit");
        assert_eq!(stats.misses, 1, "first call must miss");
    }

    #[test]
    fn a_changed_bit_of_geometry_is_a_miss() {
        let mut cache = FillCache::from_env();
        let clip = Rect::new(0.0, 0.0, 200.0, 200.0);
        let polylines = blob(5);

        let _ = cache.coverage(&polylines, clip);

        let mut nudged = blob(5);
        nudged[0].points[0].0 += 0.001;
        let _ = cache.coverage(&nudged, clip);

        let stats = cache.stats();
        assert_eq!(stats.hits, 0, "a changed coordinate must miss");
        assert_eq!(stats.misses, 2);
    }

    #[test]
    fn a_changed_clip_is_a_miss() {
        let mut cache = FillCache::from_env();
        let polylines = blob(6);
        let a = Rect::new(0.0, 0.0, 200.0, 200.0);
        let b = Rect::new(0.0, 0.0, 190.0, 200.0);

        let _ = cache.coverage(&polylines, a);
        let _ = cache.coverage(&polylines, b);

        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 2);
    }

    #[test]
    fn zero_budget_bypasses_without_paying_the_hash() {
        // Constructed directly rather than through the env: tests run in
        // parallel in one process, and an env var set here would race any
        // other test's `from_env`. The bypass path is the whole function's
        // early return, so its observable contract is: correct bytes, no
        // cache state.
        let mut cache = FillCache {
            inner: ResidencyCache::new(0),
            enabled: false,
        };
        let polylines = blob(8);
        let clip = Rect::new(0.0, 0.0, 200.0, 200.0);
        let mask = cache.coverage(&polylines, clip);
        assert_same_mask(&mask, &rasterize(&polylines, clip));
        assert_eq!(
            cache.stats().misses + cache.stats().hits,
            0,
            "bypass counts nothing"
        );
    }

    #[test]
    fn the_cache_evicts_to_its_budget_and_stays_correct() {
        // 64 KB of budget, ~tens-of-KB masks: each new blob evicts the last
        // and every answer is still the oracle's, bit for bit.
        let mut cache = FillCache {
            inner: ResidencyCache::new(64 * 1024),
            enabled: true,
        };
        let clip = Rect::new(0.0, 0.0, 200.0, 200.0);
        let mut first_mask = None;
        for seed in 1..=6u64 {
            let polylines = blob(seed);
            let mask = cache.coverage(&polylines, clip);
            assert_same_mask(&mask, &rasterize(&polylines, clip));
            if seed == 1 {
                first_mask = Some(mask);
            }
        }
        assert!(cache.stats().evictions > 0, "the budget must have bitten");
        // Seed 1's mask is long gone; asking again is a miss with the same bytes.
        let again = cache.coverage(&blob(1), clip);
        assert_same_mask(&first_mask.unwrap(), &again);
        assert_eq!(cache.stats().hits, 0);
    }

    #[test]
    fn an_empty_path_set_round_trips() {
        let mut cache = FillCache::from_env();
        let clip = Rect::new(0.0, 0.0, 200.0, 200.0);
        let empty: Vec<Polyline> = Vec::new();
        let a = cache.coverage(&empty, clip);
        let b = cache.coverage(&empty, clip);
        assert!(a.is_empty() && b.is_empty());
        assert_eq!(cache.stats().hits, 1, "even emptiness is memoised");
    }
}
