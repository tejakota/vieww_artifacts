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
//! smaller caches and pays the occasional re-derivation instead. **Pixels
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

const MIB: usize = 1024 * 1024;

impl CacheBudgets {
    /// What a renderer gets when nobody said otherwise: the budgets the two
    /// caches shipped with, unchanged — so desktop builds, tests and the
    /// raster-identity gallery see exactly the trade they saw before.
    pub const DEFAULT: Self = Self {
        blur_bytes: super::effects::DEFAULT_BLUR_CACHE_BUDGET_BYTES,
        fill_bytes: super::fill_cache::DEFAULT_FILL_CACHE_BUDGET_BYTES,
    };

    /// A low-memory device (the LG Stylo 5 class): a quarter of the default.
    /// Still enough for the static blurs and icon fills of one screen, which
    /// is the reuse the caches exist for.
    pub const LOW_MEMORY: Self = Self {
        blur_bytes: 2 * MIB,
        fill_bytes: 2 * MIB,
    };

    /// A mid-range device (the Pixel 5a / A36 class): half the default.
    pub const MID_MEMORY: Self = Self {
        blur_bytes: 4 * MIB,
        fill_bytes: 4 * MIB,
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

    #[test]
    fn the_default_is_what_the_caches_shipped_with() {
        assert_eq!(CacheBudgets::DEFAULT.blur_bytes, 8 * MIB);
        assert_eq!(CacheBudgets::DEFAULT.fill_bytes, 8 * MIB);
    }

    #[test]
    fn the_tiers_only_ever_shrink_the_default() {
        for tier in [CacheBudgets::LOW_MEMORY, CacheBudgets::MID_MEMORY] {
            assert!(tier.blur_bytes <= CacheBudgets::DEFAULT.blur_bytes);
            assert!(tier.fill_bytes <= CacheBudgets::DEFAULT.fill_bytes);
            assert!(
                tier.blur_bytes > 0 && tier.fill_bytes > 0,
                "a tier never disables a cache"
            );
        }
    }
}
