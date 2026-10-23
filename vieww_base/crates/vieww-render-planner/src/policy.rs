//! Per-pass placement decisions.
//!
//! This is the policy the review this workspace's docs quote asks for
//! directly: "CPU/GPU/hybrid should be a policy, not just three
//! independent backends." [`decide`] is that policy, as one pure function
//! from (pass shape, cost estimate, device, budget) to a [`Placement`] —
//! nothing here executes anything, which is what makes every rule in it
//! independently testable.

use vieww_render_graph::PassDesc;
use vieww_scene::Affinity;

use crate::capabilities::DeviceProfile;
use crate::cost::{affinity_bias, ExecutorEstimate};
use crate::frame_budget::FrameBudget;
use crate::hybrid::{self, Split};
use crate::power;

/// The fallback, and the floor, for [`hybrid_min_pixels`] — the round-number
/// guess this policy lived with before the threshold was derived from the
/// device profile. It is what a profile with no usable GPU numbers gets (and
/// such a profile never splits anyway — [`decide`] returns
/// [`Placement::Cpu`] before the split is considered), and the floor keeps a
/// fast-GPU-low-overhead profile from making splitting *more* eager than the
/// old guess on the strength of numbers nobody measured.
pub const HYBRID_MIN_PIXELS: f32 = 250_000.0;

/// The ceiling for [`hybrid_min_pixels`]: a pass bigger than this is worth a
/// split's coordination cost on any profile that can split at all, because
/// four megapixels is a quarter of a 4K screen — beyond it, an unreachably
/// high threshold would only ever be a profile bug made policy.
const HYBRID_MAX_PIXELS: f32 = 4_000_000.0;

/// How many of the GPU's "pixels filled during one dispatch overhead" a
/// pass must touch before splitting is considered — see
/// [`hybrid_min_pixels`]. Two, because at the throughput-balancing split
/// each side carries half the pass, and a split whose GPU half does not even
/// cover the dispatch it triggered has paid for nothing.
const HYBRID_SPLIT_OVERHEAD_MULTIPLES: f32 = 2.0;

/// The smallest pass, in touched pixels, that is worth splitting across
/// both executors — [`hybrid::worth_splitting`]'s threshold, derived from
/// the device profile rather than assumed.
///
/// # The derivation
///
/// A split's coordination cost — partitioning the work, a second submit, a
/// merge — is dominated by one extra GPU dispatch, and the honest unit for
/// "one dispatch" is time: `gpu_dispatch_overhead_ms`. Converted through
/// the GPU's own fill rate it becomes *pixels the GPU could have filled
/// during one dispatch overhead*, the smallest quantity a split can trade
/// against. [`decide`]'s split makes both sides finish together, so each
/// carries about half the pass; the pass is worth splitting only when that
/// half still dwarfs the overhead it pays for — hence
/// [`HYBRID_SPLIT_OVERHEAD_MULTIPLES`] × overhead-pixels, clamped to
/// [`HYBRID_MIN_PIXELS`, `HYBRID_MAX_PIXELS`].
///
/// On the reference profiles: a discrete desktop (0.05 ms overhead, 8
/// Gpx/s) gets 800,000 pixels — a third of a 1080p screen — where the old
/// constant guessed 250,000, and a tile-based phone (0.3 ms, 1.2 Gpx/s)
/// gets 720,000. Both are *less* eager to split than the guess, which is
/// the direction a real coordination cost pushes.
#[must_use]
pub fn hybrid_min_pixels(device: &DeviceProfile) -> f32 {
    let overhead_pixels =
        device.gpu_dispatch_overhead_ms / 1000.0 * device.gpu_fill_mpix_per_sec * 1_000_000.0;
    if !overhead_pixels.is_finite() || overhead_pixels <= 0.0 {
        return HYBRID_MIN_PIXELS;
    }
    (HYBRID_SPLIT_OVERHEAD_MULTIPLES * overhead_pixels)
        .clamp(HYBRID_MIN_PIXELS, HYBRID_MAX_PIXELS)
}

/// Where a pass ends up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Placement {
    Cpu,
    Gpu,
    /// Split across both, in the given proportion.
    Hybrid(Split),
}

impl Placement {
    /// The fraction of this placement's work that runs on the GPU — `0.0`
    /// for [`Self::Cpu`], `1.0` for [`Self::Gpu`], the split's own fraction
    /// for [`Self::Hybrid`]. What [`crate::planner::FramePlan::gpu_share`]
    /// sums over every pass.
    #[must_use]
    pub fn gpu_fraction(self) -> f32 {
        match self {
            Self::Cpu => 0.0,
            Self::Gpu => 1.0,
            Self::Hybrid(s) => s.gpu_fraction(),
        }
    }
}

/// Decide where one pass should run.
///
/// `pixels` is the pass's approximate touched-pixel count (from its output
/// resource dimensions) — the same unit [`crate::cost::CostModel`] uses.
#[must_use]
pub fn decide(
    pass: &PassDesc,
    pixels: f32,
    estimate: ExecutorEstimate,
    device: &DeviceProfile,
    budget: &FrameBudget,
) -> Placement {
    let Some(gpu_ms) = estimate.gpu_ms else {
        return Placement::Cpu;
    };

    let bias = affinity_bias(pass.cost.gpu_affinity);

    // A pass whose shape is fundamentally CPU-shaped never moves, no
    // matter how idle the GPU is — the review's own example (a single
    // small fill, a short text run) where GPU dispatch overhead alone
    // would lose.
    if pass.cost.gpu_affinity == Affinity::Cpu {
        return Placement::Cpu;
    }

    // A pass that is only sensible on a GPU goes there whenever one
    // exists, regardless of the raw number — the CPU "fallback" this
    // models is a correctness fallback (device without the affinity's
    // preferred hardware), not a cost comparison.
    if pass.cost.gpu_affinity == Affinity::GpuOnly {
        return Placement::Gpu;
    }

    // GPU-leaning work gets a modest discount to its effective cost,
    // modelling benefits a raw pixel-throughput number doesn't capture on
    // its own (residency across frames, batching many instances) —
    // capped well short of ever inventing a win the raw numbers don't
    // support.
    let effective_gpu_ms = gpu_ms * (1.0 - 0.3 * bias.max(0.0));
    let cpu_ms = estimate.cpu_ms;

    let budget_tight = budget.is_tight(device.frame_deadline_ms() * 0.2);

    // Neither side clearly wins and the pass is big enough to bother:
    // split it. "Big enough" is derived from the device — see
    // [`hybrid_min_pixels`] — not the old round-number guess.
    let ratio = effective_gpu_ms / cpu_ms.max(0.000_1);
    if hybrid::worth_splitting(pixels, hybrid_min_pixels(device)) && (0.3..=3.0).contains(&ratio) {
        let split = hybrid::split(
            pixels,
            device.cpu_fill_mpix_per_sec * 1_000_000.0,
            device.gpu_fill_mpix_per_sec * 1_000_000.0,
            device.gpu_dispatch_overhead_ms / 1000.0,
        );
        if split.cpu_fraction > 0.02 && split.gpu_fraction() > 0.02 {
            return Placement::Hybrid(split);
        }
    }

    let (faster_ms, faster_on_gpu) = if effective_gpu_ms <= cpu_ms {
        (effective_gpu_ms, true)
    } else {
        (cpu_ms, false)
    };
    let (slower_ms, slower_on_gpu) = if faster_on_gpu {
        (cpu_ms, false)
    } else {
        (effective_gpu_ms, true)
    };

    let faster_energy = power::energy_cost(faster_ms, faster_on_gpu, device);
    let slower_energy = power::energy_cost(slower_ms, slower_on_gpu, device);

    if power::prefer_lower_energy(
        device,
        budget_tight,
        faster_ms,
        faster_energy,
        slower_ms,
        slower_energy,
        0.5,
    ) {
        return if slower_on_gpu {
            Placement::Gpu
        } else {
            Placement::Cpu
        };
    }

    if faster_on_gpu {
        Placement::Gpu
    } else {
        Placement::Cpu
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vieww_render_graph::PassKind;
    use vieww_scene::{Cacheability, CostHint, Dynamicity};

    fn pass_with_affinity(affinity: Affinity) -> PassDesc {
        PassDesc::new(
            "p",
            PassKind::Raster,
            CostHint {
                dynamicity: Dynamicity::Occasional,
                cacheability: Cacheability::High,
                gpu_affinity: affinity,
                estimated_bytes: 0,
                cpu_complexity: vieww_scene::PLAIN_FILL_COMPLEXITY,
            },
        )
    }

    #[test]
    fn no_gpu_estimate_means_cpu_regardless_of_affinity() {
        let p = decide(
            &pass_with_affinity(Affinity::GpuOnly),
            1_000_000.0,
            ExecutorEstimate {
                cpu_ms: 1.0,
                gpu_ms: None,
            },
            &DeviceProfile::CPU_ONLY_FALLBACK,
            &FrameBudget::new(16.0),
        );
        assert_eq!(p, Placement::Cpu);
    }

    #[test]
    fn cpu_affinity_never_moves_even_when_gpu_looks_cheaper() {
        let p = decide(
            &pass_with_affinity(Affinity::Cpu),
            10.0,
            ExecutorEstimate {
                cpu_ms: 5.0,
                gpu_ms: Some(0.001),
            },
            &DeviceProfile::DESKTOP_DISCRETE,
            &FrameBudget::new(16.0),
        );
        assert_eq!(p, Placement::Cpu);
    }

    #[test]
    fn gpu_only_affinity_always_goes_to_the_gpu_when_present() {
        let p = decide(
            &pass_with_affinity(Affinity::GpuOnly),
            10.0,
            ExecutorEstimate {
                cpu_ms: 0.1,
                gpu_ms: Some(5.0),
            },
            &DeviceProfile::DESKTOP_DISCRETE,
            &FrameBudget::new(16.0),
        );
        assert_eq!(p, Placement::Gpu);
    }

    #[test]
    fn a_huge_balanced_pass_with_comparable_costs_gets_split() {
        let p = decide(
            &pass_with_affinity(Affinity::Balanced),
            2_000_000.0,
            ExecutorEstimate {
                cpu_ms: 10.0,
                gpu_ms: Some(11.0),
            },
            &DeviceProfile::DESKTOP_DISCRETE,
            &FrameBudget::new(16.0),
        );
        assert!(matches!(p, Placement::Hybrid(_)));
    }

    #[test]
    fn a_tiny_pass_is_never_split_even_if_costs_are_close() {
        let p = decide(
            &pass_with_affinity(Affinity::Balanced),
            100.0,
            ExecutorEstimate {
                cpu_ms: 0.01,
                gpu_ms: Some(0.011),
            },
            &DeviceProfile::DESKTOP_DISCRETE,
            &FrameBudget::new(16.0),
        );
        assert!(!matches!(p, Placement::Hybrid(_)));
    }

    #[test]
    fn the_split_threshold_is_derived_from_the_dispatch_overhead() {
        // Two overheads, one GPU: the pixels the GPU fills during its own
        // dispatch overhead, doubled — the old 250k guess only as a floor.
        let desktop = DeviceProfile::DESKTOP_DISCRETE;
        let expect = 2.0 * (desktop.gpu_dispatch_overhead_ms / 1000.0)
            * desktop.gpu_fill_mpix_per_sec
            * 1_000_000.0;
        assert!((hybrid_min_pixels(&desktop) - expect).abs() < 1.0);
        assert!(hybrid_min_pixels(&desktop) > HYBRID_MIN_PIXELS);

        // A phone's higher overhead pushes its threshold up too, not down.
        let phone = DeviceProfile::MOBILE_TILE_BASED;
        assert!(hybrid_min_pixels(&phone) > HYBRID_MIN_PIXELS);

        // The threshold scales with the overhead: same GPU, half the
        // overhead, (almost) half the threshold.
        let mut cheaper = desktop;
        cheaper.gpu_dispatch_overhead_ms = desktop.gpu_dispatch_overhead_ms / 2.0;
        assert!(hybrid_min_pixels(&cheaper) < hybrid_min_pixels(&desktop));
    }

    #[test]
    fn a_profile_without_usable_gpu_numbers_keeps_the_fallback_threshold() {
        // No GPU: the old round number, and `decide` never reaches the
        // split anyway (no gpu estimate).
        assert_eq!(hybrid_min_pixels(&DeviceProfile::CPU_ONLY_FALLBACK), HYBRID_MIN_PIXELS);
        // A GPU with zero modeled overhead cannot justify splitting below
        // the floor either: the floor is what keeps unmeasured optimism
        // from making the policy more eager than the guess it replaced.
        let mut free = DeviceProfile::DESKTOP_DISCRETE;
        free.gpu_dispatch_overhead_ms = 0.0;
        assert_eq!(hybrid_min_pixels(&free), HYBRID_MIN_PIXELS);
    }

    #[test]
    fn a_pass_above_the_derived_threshold_but_below_the_old_guess_stays_whole() {
        // 500k pixels with comparable costs: the old constant would have
        // split it, the desktop's derived 800k threshold says the GPU's
        // half would not cover its own dispatch — and the policy agrees.
        let device = DeviceProfile::DESKTOP_DISCRETE;
        let p = decide(
            &pass_with_affinity(Affinity::Balanced),
            500_000.0,
            ExecutorEstimate {
                cpu_ms: 10.0,
                gpu_ms: Some(10.0),
            },
            &device,
            &FrameBudget::new(16.0),
        );
        assert!(
            !matches!(p, Placement::Hybrid(_)),
            "below the derived threshold the pass stays whole"
        );
    }
}
