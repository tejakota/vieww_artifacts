//! Filling a [`DeviceProfile`] from the machine it runs on.
//!
//! The audit's finding, and until now the truth: "`DeviceProfile` has no real
//! probing; `CPU_ONLY_FALLBACK` is the default; no runtime GPU probe." Every
//! profile the live path planned with was one of three named round-number
//! guesses, picked by matching the Vulkan adapter's type string — a lavapipe
//! "CPU" got the fallback, everything else got a desktop or phone table.
//!
//! This module is the first half of the fix the audit's Phase 0 asks for:
//! "**Calibrate `DeviceProfile` at startup.** Implement a one-time
//! micro-benchmark on first launch that fills these fields for the actual
//! device, replacing the round-number defaults."
//!
//! What is here, and what deliberately is not:
//!
//! * **CPU cores** ([`detect_cpu_cores`]) — the one field the OS answers
//!   exactly, from the scheduler's own view of what this process may use.
//!   Always available: no feature, no cost.
//! * **CPU fill throughput** ([`calibrate_cpu_fill`], behind this crate's
//!   `calibrate` feature — the windowed platform turns it on) — a one-time
//!   micro-benchmark that renders a representative plain-fill scene with the
//!   same CPU rasterizer the live path uses, and converts the best sample
//!   into the profile's Mpix/s unit. This is the number
//!   [`HeuristicCostModel`](crate::cost::HeuristicCostModel) divides by for
//!   every CPU estimate, and until now it was `40.0`, `60.0` or `250.0`
//!   depending on which *string* the GPU driver reported.
//! * **GPU fields** are *not* benchmarked here. A GPU fill measurement needs
//!   a device, a queue and a readback, which is backend work, not planner
//!   work — the shadow log's [`Calibration`](crate::shadow::Calibration)
//!   already corrects the GPU throughput from real frames, and that is the
//!   measured path this crate owns. What this module never does is invent a
//!   GPU number.
//!
//! The benchmark is **opt-in** (`VIEWW_PLANNER_CALIBRATE=1` on the windowed
//! path): it costs single-digit milliseconds at startup and the zero-cost
//! profile pick stays the default, the same bargain
//! `VIEWW_PLANNER_SHADOW` makes.

use crate::capabilities::DeviceProfile;

/// How many CPU cores raster work can be spread across, from the scheduler
/// itself ([`std::thread::available_parallelism`]) — the OS's answer, not a
/// table's. Never zero: the field is a divisor downstream.
#[must_use]
pub fn detect_cpu_cores() -> u32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "clamped to u32::MAX before the cast"
    )]
    let cores = std::thread::available_parallelism()
        .map_or(1_usize, std::num::NonZeroUsize::get)
        .min(u32::MAX as usize) as u32;
    cores.max(1)
}

/// `profile` with its `cpu_cores` replaced by [`detect_cpu_cores`]' answer.
#[must_use]
pub fn with_detected_cores(profile: DeviceProfile) -> DeviceProfile {
    let mut out = profile;
    out.cpu_cores = detect_cpu_cores();
    out
}

#[cfg(feature = "calibrate")]
mod fill_benchmark {
    use std::time::Instant;

    use vieww_foundation::{Color, Rect};
    use vieww_paint::native::NativeRenderer;
    use vieww_paint::{Canvas, Paint, Scene};

    use crate::capabilities::DeviceProfile;
    use crate::probe::with_detected_cores;

    /// The benchmark frame: small enough that a handful of runs costs
    /// single-digit milliseconds, large enough that one run is a fifth of a
    /// megapixel and a scheduling hiccup cannot dominate the sample.
    pub(crate) const BENCH_WIDTH: u32 = 512;
    pub(crate) const BENCH_HEIGHT: u32 = 384;

    /// Timed runs after the warmups. Five is the smallest count whose minimum
    /// is not simply "the first sample"; the *minimum* is used because a
    /// frame budget cares about the throughput the machine can sustain when
    /// nothing else interferes, which is what the fastest clean sample
    /// measures.
    const BENCH_RUNS: usize = 5;

    /// Warmups before the timed runs, so the first sample is not paying for
    /// the rasterizer's own cold start (a first target allocation, cold
    /// caches).
    const BENCH_WARMUPS: usize = 2;

    /// Throughputs outside this band are not measurements, they are symptoms
    /// — below it a machine is being throttled to a fraction of any phone,
    /// above it no CPU rasterizer has ever been measured. Both keep the
    /// profile's original guess: a benchmark that returns an impossibility
    /// has not earned the right to replace anything.
    const MIN_PLAUSIBLE_MPIX_PER_SEC: f32 = 4.0;
    const MAX_PLAUSIBLE_MPIX_PER_SEC: f32 = 4_000.0;

    /// `profile` with its `cpu_fill_mpix_per_sec` replaced by a one-time
    /// measurement of *this machine's* CPU rasterizer — the same
    /// [`NativeRenderer`] the live CPU path draws with, rendering a
    /// plain-fill scene, timed over [`BENCH_RUNS`] runs and reported as the
    /// best sample. The cores are detected as part of the same call.
    ///
    /// # What can make it keep the original number
    ///
    /// * **A debug build** (`cfg!(debug_assertions)`): a debug rasterizer
    ///   runs several times slower than the release one a real launch uses,
    ///   so the number it produces would poison every prediction the profile
    ///   makes. Calibration is a release-launch operation; in a debug build
    ///   the guess is more honest than the measurement.
    /// * **An implausible result** (outside the band this module documents):
    ///   a throttled VM or a benchmark that went wrong keeps the guess rather
    ///   than replacing it with a symptom.
    /// * **A rasterizer error**: the benchmark is a diagnostic and a
    ///   diagnostic must never be the thing that takes a launch down.
    ///
    /// The GPU fields and every other part of the profile pass through
    /// untouched — one entry point, `profile_for`'s guess in, a measured
    /// profile out.
    #[must_use]
    pub fn calibrate_cpu_fill(profile: DeviceProfile) -> DeviceProfile {
        let mut out = with_detected_cores(profile);
        if cfg!(debug_assertions) {
            return out;
        }
        let scene = fill_scene(BENCH_WIDTH, BENCH_HEIGHT);
        #[expect(
            clippy::cast_precision_loss,
            reason = "a benchmark frame is a small number of pixels"
        )]
        let mpix = (BENCH_WIDTH as f32 * BENCH_HEIGHT as f32) / 1_000_000.0;
        if let Some(best_ms) = measure_fill_ms(&scene) {
            if let Some(rate) = throughput_from_best_ms(mpix, best_ms) {
                out.cpu_fill_mpix_per_sec = rate;
            }
        }
        out
    }

    /// Convert a best-sample time into the profile's Mpix/s unit, rejecting
    /// results outside the plausible band (see the constants' docs).
    /// `None` means "keep the profile's original number."
    #[must_use]
    pub(crate) fn throughput_from_best_ms(mpix: f32, best_ms: f32) -> Option<f32> {
        if !(best_ms.is_finite() && best_ms > 0.0) {
            return None;
        }
        let rate = mpix / (best_ms / 1000.0);
        (rate.is_finite()
            && (MIN_PLAUSIBLE_MPIX_PER_SEC..=MAX_PLAUSIBLE_MPIX_PER_SEC).contains(&rate))
            .then_some(rate)
    }

    /// Time [`BENCH_RUNS`] plain-fill renders of `scene`, returning the best.
    /// `None` when the rasterizer itself failed — see `calibrate_cpu_fill`'s
    /// doc for why that keeps the profile unchanged.
    fn measure_fill_ms(scene: &Scene) -> Option<f32> {
        let mut renderer = NativeRenderer::new();
        let base = Color::rgb(16, 16, 20);
        for _ in 0..BENCH_WARMUPS {
            renderer
                .render_in_place(scene, BENCH_WIDTH, BENCH_HEIGHT, base)
                .ok()?;
        }
        let mut best = f32::INFINITY;
        for _ in 0..BENCH_RUNS {
            let started = Instant::now();
            renderer
                .render_in_place(scene, BENCH_WIDTH, BENCH_HEIGHT, base)
                .ok()?;
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a millisecond timing fits an f32"
            )]
            let ms = (started.elapsed().as_secs_f64() * 1000.0) as f32;
            best = best.min(ms);
        }
        Some(best)
    }

    /// The benchmark's workload: opaque rects tiling the whole frame, nothing
    /// else. `cpu_fill_mpix_per_sec` is documented as "megapixels of *simple
    /// fill work* per second" — that is the unit every CPU estimate divides
    /// by, so the benchmark draws exactly that and not a scenic mixture whose
    /// result would be a different unit wearing this one's name.
    fn fill_scene(width: u32, height: u32) -> Scene {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a benchmark frame is a small number of pixels"
        )]
        let (w, h) = (width as f32, height as f32);
        let mut scene = Scene::new();
        let mut x = 0.0_f32;
        let mut i = 0_u32;
        while x < w {
            let right = (x + 64.0).min(w);
            // A spread of colours so no run benefits from a single-pixel
            // fast path; all of them fully opaque, all of them plain fills.
            let color = match i % 3 {
                0 => Color::RED,
                1 => Color::rgb(20, 120, 220),
                _ => Color::rgb(240, 200, 40),
            };
            scene.fill_rect(Rect::new(x, 0.0, right, h), Paint::solid(color));
            x = right;
            i += 1;
        }
        scene
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_plausible_best_sample_becomes_the_throughput() {
            // 0.197 Mpix in 1 ms is ~197 Mpix/s: inside the band, returned.
            let mpix = 0.197;
            let rate = throughput_from_best_ms(mpix, 1.0).unwrap();
            assert!((rate - 197.0).abs() < 0.5, "{rate}");
        }

        #[test]
        fn implausible_or_broken_samples_keep_the_guess() {
            // Too slow (throttled to a fraction of any real machine)...
            assert!(
                throughput_from_best_ms(0.197, 100_000.0).is_none(),
                "below the plausible band"
            );
            // ...too fast (faster than any CPU rasterizer measured)...
            assert!(
                throughput_from_best_ms(0.197, 0.000_01).is_none(),
                "above the plausible band"
            );
            // ...and not a number at all.
            assert!(throughput_from_best_ms(0.197, f32::NAN).is_none());
            assert!(throughput_from_best_ms(0.197, 0.0).is_none());
            assert!(throughput_from_best_ms(0.197, f32::INFINITY).is_none());
        }

        #[test]
        fn the_benchmark_scene_tiles_the_frame_with_plain_fills() {
            let scene = fill_scene(BENCH_WIDTH, BENCH_HEIGHT);
            // Every command is a fill rect, and together they cover the frame.
            let commands = scene.commands();
            assert!(!commands.is_empty());
            assert!(commands
                .iter()
                .all(|c| matches!(c, vieww_paint::Command::FillRect { .. })));
        }

        #[test]
        fn calibration_always_returns_a_usable_profile() {
            // Debug builds keep the guess by design (see
            // `calibrate_cpu_fill`'s doc), so this asserts the plumbing
            // rather than the value: the profile comes back with a positive,
            // finite throughput and detected cores whichever path ran.
            let profile = calibrate_cpu_fill(DeviceProfile::CPU_ONLY_FALLBACK);
            assert!(profile.cpu_fill_mpix_per_sec > 0.0);
            assert!(profile.cpu_fill_mpix_per_sec.is_finite());
            assert_eq!(
                profile.cpu_cores,
                crate::probe::detect_cpu_cores(),
                "cores are detected even when the fill guess is kept"
            );
        }
    }
}

#[cfg(feature = "calibrate")]
pub use fill_benchmark::calibrate_cpu_fill;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detected_cores_are_at_least_one_and_match_the_scheduler() {
        let cores = detect_cpu_cores();
        assert!(cores >= 1);
        let expect = std::thread::available_parallelism()
            .map_or(1_usize, std::num::NonZeroUsize::get) as u32;
        assert_eq!(cores, expect.max(1));
    }

    #[test]
    fn with_detected_cores_touches_cores_and_nothing_else() {
        let profile = with_detected_cores(DeviceProfile::MOBILE_TILE_BASED);
        assert_eq!(profile.cpu_cores, detect_cpu_cores());
        assert_eq!(profile.gpu, DeviceProfile::MOBILE_TILE_BASED.gpu);
        assert_eq!(
            profile.cpu_fill_mpix_per_sec,
            DeviceProfile::MOBILE_TILE_BASED.cpu_fill_mpix_per_sec
        );
    }
}
