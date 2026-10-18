//! The quality profile: where a device tier turns into render decisions.
//!
//! # Why this exists
//!
//! [`App::auto_rasterizer`](crate::App::auto_rasterizer) chooses *who* draws;
//! nothing yet decides *how much* the draw is allowed to cost. On the farm's
//! Low phone both answers miss the budget (LG Stylo 5: CPU 36.2 ms, GPU
//! 40.1 ms against 16.7 — Device Farm, 2026-10-17), and the cheapest lever a
//! fill-rate-bound GPU frame has is how many pixels it fills. A
//! [`QualityProfile`] is the one struct those decisions live in: the tier
//! picks one ([`QualityProfile::for_tier`]), and the render path reads it.
//!
//! # What it gates today, and what it is for tomorrow
//!
//! One field: [`resolution_scale`][Self::resolution_scale], read by the GPU
//! path's direct route (`NativeRenderer::present_planned` →
//! `SceneRenderer::render_planned_to`). Effect gates (blurs, shadows — the
//! advisory list's next candidates) join the struct when the scene planner
//! can skip them; a field nothing reads would be a guess about a renderer
//! that has not been measured yet.
//!
//! # Why the scale is opt-in, and off by default
//!
//! The CPU path never scales: its output is the fidelity reference
//! (`docs/FIDELITY.md` §1) and it must stay byte-identical. The GPU path at a
//! scale below 1.0 renders the plan into smaller targets and the final blit
//! enlarges them — pixels move, so the default is full resolution and
//! `VIEWW_GPU_SCALE` opts in (`auto`, or a number between 0.25 and 1.0).
//! Device Farm's `gpuscale` pass measures what it buys on the phones where
//! the GPU path currently loses: if 0.5× turns the LG Stylo 5's 40.1 ms into
//! a pass, the tier profile has earned its default; if not, nothing changed.

use std::sync::OnceLock;

use crate::device_tier::{device_facts, DeviceTier};

/// The render decisions a device tier gets by default. Every field is a
/// default, not a mandate: explicit application settings and diagnostics
/// (`VIEWW_GPU_SCALE`) override the profile, and the profile's own numbers
/// are what Device Farm measures against.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QualityProfile {
    /// The fraction of the swapchain's resolution the GPU path renders at.
    /// `1.0` is full resolution; the CPU path is unaffected by this field
    /// whatever it says (see the module docs).
    pub resolution_scale: f32,
}

impl QualityProfile {
    /// The Low tier: the LG Stylo 5 class. A quarter of the pixels — the
    /// single biggest lever on a fill-rate-bound frame, and the first thing
    /// the `gpuscale` pass tests.
    pub const LOW: Self = Self {
        resolution_scale: 0.5,
    };

    /// The Medium tier: the A36 / iPhone 13 class, where measurements already
    /// pick a rasterizer that passes. Three quarters keeps the density loss
    /// at the edge of noticeability while still cutting a third of the fill.
    pub const MEDIUM: Self = Self {
        resolution_scale: 0.75,
    };

    /// The High tier: everything passes with room, so nothing is scaled.
    pub const HIGH: Self = Self {
        resolution_scale: 1.0,
    };

    /// The profile a tier gets by default.
    #[must_use]
    pub const fn for_tier(tier: DeviceTier) -> Self {
        match tier {
            DeviceTier::Low => Self::LOW,
            DeviceTier::Medium => Self::MEDIUM,
            DeviceTier::High => Self::HIGH,
        }
    }
}

/// The resolution scale the GPU path renders at this process, from
/// `VIEWW_GPU_SCALE`: `None` is full resolution (the default), `auto` takes
/// the probed tier's profile, and a number is clamped into `0.25..=1.0`.
/// Read once, before any thread that could set the environment races it.
pub(crate) fn gpu_scale() -> Option<f32> {
    static SCALE: OnceLock<Option<f32>> = OnceLock::new();
    *SCALE.get_or_init(|| {
        let requested = std::env::var("VIEWW_GPU_SCALE").ok()?;
        let requested = requested.trim();
        if requested.is_empty() || requested.eq_ignore_ascii_case("off") {
            return None;
        }
        let scale = if requested.eq_ignore_ascii_case("auto") {
            QualityProfile::for_tier(device_facts().tier).resolution_scale
        } else {
            requested.parse::<f32>().ok()?.clamp(0.25, 1.0)
        };
        // A scale of 1.0 is "off" by another name: the render path should not
        // run its smaller-targets-and-scaled-blit machinery for no change.
        (scale < 1.0).then_some(scale)
    })
}

/// The GPU scale line the on-device suite observes as `gpu_scale` — `None`
/// when the process is at full resolution, so the suite log stays quiet
/// unless the mechanism is actually engaged.
#[must_use]
pub fn gpu_scale_status() -> Option<String> {
    let scale = gpu_scale()?;
    let source = std::env::var("VIEWW_GPU_SCALE")
        .map(|value| {
            if value.trim().eq_ignore_ascii_case("auto") {
                format!("auto: tier {}", device_facts().tier.name())
            } else {
                "set".to_owned()
            }
        })
        .unwrap_or_else(|_| "set".to_owned());
    Some(format!("GPU path renders at {scale:.2}x resolution (VIEWW_GPU_SCALE={source})"))
}

#[cfg(test)]
mod tests {
    use super::{gpu_scale, QualityProfile};
    use crate::device_tier::DeviceTier;

    #[test]
    fn the_profiles_match_the_tiers() {
        assert_eq!(QualityProfile::for_tier(DeviceTier::Low), QualityProfile::LOW);
        assert_eq!(
            QualityProfile::for_tier(DeviceTier::Medium),
            QualityProfile::MEDIUM
        );
        assert_eq!(QualityProfile::for_tier(DeviceTier::High), QualityProfile::HIGH);
    }

    #[test]
    fn the_scale_is_off_unless_asked() {
        // Only meaningful when nothing asked; when something did, whichever
        // test calls `gpu_scale` first caches the answer for all of them.
        if std::env::var_os("VIEWW_GPU_SCALE").is_none() {
            assert_eq!(gpu_scale(), None);
        }
    }
}
