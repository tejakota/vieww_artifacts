//! The process's default budgets for the derived-value caches, settable once
//! by the platform layer before the first renderer exists.
//!
//! # Why this exists
//!
//! The blur-band cache (`effects::blur_cached`, commit 2b03351) and the
//! path-fill cache (`fill_cache.rs`, f004294) each default to 8 MiB. On a
//! desktop that is the right trade; on a 2.7 GB phone it is not obviously
//! so, and Device Farm run f91d07c (2026-10-19) read the CPU path's PSS
//! 10–16 MB higher than earlier runs after both landed (LG Stylo 5 76 → 92
//! MB, Galaxy S26 Ultra 78 → 88 MB) — the same size as the two budgets
//! together. That was across different units of each phone, so it is a
//! reading, not a proof; the `cpubare` Device Farm pass is the same-unit A/B.
//!
//! Either way the budgets should follow the device: a low-memory phone gets
//! smaller caches and pays the occasional re-derivation instead.
//!
//! Run 39382602099 (2026-10-20) then measured each cache off on its own, on
//! the same unit: the path-fill cache cost frame time on every phone (−1.89
//! / −6.62 / −0.73 ms median with it off, Stylo 5 / A73 / S26 Ultra), and the
//! blur cache cost a little on the two smaller phones (−1.09 / −0.20 ms) and
//! paid only on the S26 Ultra (+0.73 ms). So the fill budget is 0 — off —
//! everywhere, and the blur cache runs on the High tier and the desktop
//! default only. The row-parallel walk, the third addition, stays: it is
//! the one that paid on the low phone (+2.68 ms without it). **Pixels
//! cannot move**: both caches memoise a deterministic function, a miss runs
//! the same function a hit replays, so a smaller budget changes only how
//! often the function runs.
//!
//! # Precedence
//!
//! `VIEWW_BLUR_CACHE_MB` / `VIEWW_PATH_CACHE_MB`, when set, win — they are
//! the A/B and memory-tight knobs and must keep meaning what they say. Then
//! whatever [`set_process_cache_budgets`] stored. Then [`CacheBudgets::DEFAULT`].
//! The setting is read when a renderer is constructed, so it has to be made
//! before the first one; a later call affects only renderers built after it.

use std::sync::RwLock;

/// The byte budgets of the derived-value caches a renderer builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheBudgets {
    /// The blurred-band cache (`VIEWW_BLUR_CACHE_MB` overrides).
    pub blur_bytes: usize,
    /// The path-fill coverage-mask cache (`VIEWW_PATH_CACHE_MB` overrides).
    pub fill_bytes: usize,
}

impl CacheBudgets {
    /// What a renderer gets when nobody said otherwise, and what the High
    /// tier gets: the blur cache at the budget it shipped with, the path-fill
    /// cache off (`VIEWW_PATH_CACHE_MB` opts in). Pixels are the same either
    /// way, so the raster-identity gallery is unaffected.
    pub const DEFAULT: Self = Self {
        blur_bytes: super::effects::DEFAULT_BLUR_CACHE_BUDGET_BYTES,
        fill_bytes: 0,
    };

    /// A low-memory device (the LG Stylo 5 class): both caches off. Each
    /// measured slower on the Stylo 5 with it than without, and the fill
    /// cache cost 13 MB of PSS there.
    pub const LOW_MEMORY: Self = Self {
        blur_bytes: 0,
        fill_bytes: 0,
    };

    /// A mid-range device (the Galaxy A73 / Pixel 5a class): both caches
    /// off, for the same measurement on the A73.
    pub const MID_MEMORY: Self = Self {
        blur_bytes: 0,
        fill_bytes: 0,
    };
}

static PROCESS: RwLock<CacheBudgets> = RwLock::new(CacheBudgets::DEFAULT);

/// Sets the budgets renderers built from now on start with. The platform
/// layer calls this once, from the device tier, before opening a window.
pub fn set_process_cache_budgets(budgets: CacheBudgets) {
    // A poisoned lock still holds a valid `CacheBudgets` (it is `Copy` and
    // written whole), so recover it rather than refusing the setting.
    *PROCESS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = budgets;
}

/// The budgets a renderer built now starts with (before the environment's
/// overrides, which the caches' own constructors apply).
#[must_use]
pub fn process_cache_budgets() -> CacheBudgets {
    *PROCESS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: usize = 1024 * 1024;

    #[test]
    fn the_default_keeps_the_blur_cache_and_not_the_fill_cache() {
        assert_eq!(CacheBudgets::DEFAULT.blur_bytes, 8 * MIB);
        assert_eq!(CacheBudgets::DEFAULT.fill_bytes, 0);
    }

    #[test]
    fn the_tiers_only_ever_shrink_the_default() {
        for tier in [CacheBudgets::LOW_MEMORY, CacheBudgets::MID_MEMORY] {
            assert!(tier.blur_bytes <= CacheBudgets::DEFAULT.blur_bytes);
            assert_eq!(tier.fill_bytes, 0, "the fill cache is off on every tier");
        }
    }

    #[test]
    fn the_fill_cache_is_off_by_default() {
        // A zero budget is the bypass (no hash, no store), not a one-entry
        // cache — and with nothing set, that is what a renderer gets.
        use super::super::fill_cache::FillCache;
        assert!(!FillCache::with_budget(CacheBudgets::DEFAULT.fill_bytes).is_enabled());
        if std::env::var_os("VIEWW_PATH_CACHE_MB").is_none() {
            assert!(!FillCache::from_env().is_enabled());
        }
        assert!(FillCache::with_budget(8 * MIB).is_enabled());
    }
}
