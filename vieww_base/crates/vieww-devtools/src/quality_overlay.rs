//! The quality overlay: what the device-tier policy and the frame clock are
//! doing, in the few lines a HUD, a suite log, or a farm summary can carry.
//!
//! # Why this is a report, not a widget
//!
//! This workspace draws no overlay pixels — `vieww-devtools` has no window
//! and no renderer of its own, and a HUD baked into the app's own scene
//! would put pixels on the CPU path that the fidelity reference
//! (`docs/FIDELITY.md` §1) does not have. What an overlay is *for*, though,
//! is answering "what is this device's quality policy, and is the frame
//! clock inside it" at a glance — and that answer is a handful of numbers
//! the app already measured, formatted. So this module is the same shape as
//! every other inspector here: a report over real inputs, which a shell
//! (a devtools UI, the on-device suite's log, the Device Farm summary) can
//! present however it presents things.
//!
//! # What it reads, and what it does not compute twice
//!
//! - The frame numbers come from a [`FrameTimeline`] the caller already
//!   feeds — the same ring buffer a HUD would chart. [`FrameWindow`] is
//!   `FrameTimeline`'s own average/p95/max/over-budget queries over one
//!   window; no second stopwatch and no second percentile.
//! - The tier, the device line, and the engaged GPU scale come from the
//!   caller as strings, because the probes that produce them
//!   (`vieww-platform-winit`'s `device_facts`, `gpu_scale_status`) live in
//!   the platform layer and this crate does not depend on it — the same
//!   reason [`crate::gpu_inspector::GpuReport::build`] takes its heap and
//!   queues as arguments instead of finding them. The caller passes what
//!   its own probes said, or `None` when a probe said nothing; no field
//!   here is invented to fill a gap.

use std::time::Duration;

use crate::frame_timeline::FrameTimeline;

/// The frame clock over one window of a [`FrameTimeline`], as an overlay
/// states it: the average, the tail, the worst frame, and how many frames
/// missed the budget they were recorded against.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameWindow {
    /// Frames the window covered.
    pub samples: usize,
    /// Mean total frame time. Zero for an empty window — an empty window
    /// reports zero everywhere and lets `samples == 0` say why.
    pub average: Duration,
    /// Nearest-rank 95th percentile of total frame time — the number a
    /// jank budget is written against (`FrameTimeline::p95`).
    pub p95: Duration,
    /// The worst frame in the window (`FrameTimeline::max`).
    pub max: Duration,
    /// Frames whose total exceeded their own recorded budget
    /// (`FrameTimeline::dropped_frame_count`).
    pub over_budget: usize,
}

/// The quality policy and the frame clock, side by side.
///
/// Not `Eq`: the frame numbers are measurements, and the same reasoning
/// [`crate::gpu_inspector::GpuReport`] gives for the same choice applies.
#[derive(Debug, Clone, PartialEq)]
pub struct QualityOverlay {
    /// The resolved device tier's name — `low`, `medium`, `high`. The
    /// policy that picked it is the platform layer's (`device_tier`);
    /// this is the answer, not the reasoning.
    pub tier: String,
    /// The device as its probes described it — RAM, cores, SoC — the same
    /// line the on-device suite observes as `device`.
    pub device: String,
    /// The GPU path's engaged resolution scale and where it came from, as
    /// `gpu_scale_status` words it (`"GPU path renders at 0.50x …"`), or
    /// `None` when the path is at full resolution.
    pub gpu_scale: Option<String>,
    /// The frame clock over the window.
    pub frame: FrameWindow,
    /// The budget those frames were recorded against, from the timeline's
    /// newest sample ([`FrameTimeline::latest_budget`]). `None` when the
    /// timeline is empty.
    pub budget: Option<Duration>,
}

impl QualityOverlay {
    /// Build the overlay from a timeline the caller feeds and the facts its
    /// own probes reported.
    ///
    /// `window` is how many recent frames the frame clock covers — the
    /// whole timeline for a session summary, a shorter tail for a HUD that
    /// should react to what happened lately. Every number is read from the
    /// timeline's own queries; nothing here re-times, re-sorts, or
    /// re-percentiles anything.
    #[must_use]
    pub fn build(
        tier: &str,
        device: &str,
        gpu_scale: Option<&str>,
        timeline: &FrameTimeline,
        window: usize,
    ) -> Self {
        let frame = if timeline.is_empty() {
            FrameWindow {
                samples: 0,
                average: Duration::ZERO,
                p95: Duration::ZERO,
                max: Duration::ZERO,
                over_budget: 0,
            }
        } else {
            FrameWindow {
                samples: timeline.len().min(window.max(1)),
                average: timeline.average(window).unwrap_or(Duration::ZERO),
                p95: timeline.p95(window).unwrap_or(Duration::ZERO),
                max: timeline.max(window).unwrap_or(Duration::ZERO),
                over_budget: timeline.dropped_frame_count(window),
            }
        };
        Self {
            tier: tier.to_owned(),
            device: device.to_owned(),
            gpu_scale: gpu_scale.map(str::to_owned),
            frame,
            budget: timeline.latest_budget(),
        }
    }

    /// The overlay as one line — the form the on-device suite observes and
    /// the Device Farm summary prints, where a value is one line by
    /// construction (`run-android.sh` copies `key: value` lines).
    ///
    /// An empty timeline says so rather than printing zeros that would
    /// look like a measured 0 ms frame.
    #[must_use]
    pub fn summary_line(&self) -> String {
        if self.frame.samples == 0 {
            return format!("tier {} — no frames recorded yet", self.tier);
        }
        let mut line = format!(
            "tier {} · median {:?} / p95 {:?} / worst {:?}",
            self.tier, self.frame.average, self.frame.p95, self.frame.max
        );
        if let Some(budget) = self.budget {
            line += &format!(
                " vs budget {:?} · {}/{} over",
                budget, self.frame.over_budget, self.frame.samples
            );
        }
        if let Some(scale) = &self.gpu_scale {
            line += &format!(" · {scale}");
        }
        line
    }

    /// The overlay as the lines a HUD would draw: policy first, then the
    /// frame clock. Kept separate from [`Self::summary_line`] because a
    /// screen can afford three lines where a suite log wants one.
    #[must_use]
    pub fn hud_lines(&self) -> Vec<String> {
        let scale = self
            .gpu_scale
            .clone()
            .unwrap_or_else(|| "full resolution".to_owned());
        let mut lines = vec![
            format!("vieww quality — tier {} · GPU {}", self.tier, scale),
            format!("device: {}", self.device),
        ];
        if self.frame.samples == 0 {
            lines.push("frames: none recorded yet".to_owned());
        } else {
            let budget = self
                .budget
                .map(|b| format!(" vs {b:?} budget"))
                .unwrap_or_default();
            lines.push(format!(
                "frames: median {:?} · p95 {:?} · worst {:?}{budget} \
                 ({} of {} over)",
                self.frame.average,
                self.frame.p95,
                self.frame.max,
                self.frame.over_budget,
                self.frame.samples,
            ));
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::QualityOverlay;
    use crate::frame_timeline::FrameTimeline;
    use vieww_paint::FrameStats;

    /// The same synthetic sample `frame_timeline`'s tests use: `total` is
    /// the only field the overlay's numbers read.
    fn sample(number: u64, millis: u64, budget_millis: u64) -> FrameStats {
        FrameStats {
            number,
            timestamp: Duration::from_millis(number * 16),
            animate: Duration::ZERO,
            build: Duration::ZERO,
            layout: Duration::ZERO,
            paint: Duration::ZERO,
            composite: Duration::ZERO,
            total: Duration::from_millis(millis),
            budget: Duration::from_millis(budget_millis),
            damage_area: 0.0,
            damage_regions: 0,
        }
    }

    fn timeline_with(millis: &[u64], budget_millis: u64) -> FrameTimeline {
        let mut timeline = FrameTimeline::new(millis.len());
        for (index, &millis) in millis.iter().enumerate() {
            timeline.record(sample(index as u64 + 1, millis, budget_millis));
        }
        timeline
    }

    #[test]
    fn an_empty_timeline_reports_that_it_has_no_frames() {
        let timeline = FrameTimeline::new(8);
        let overlay = QualityOverlay::build("low", "RAM unreadable", None, &timeline, 600);
        assert_eq!(overlay.frame.samples, 0);
        assert_eq!(overlay.budget, None);
        assert!(
            overlay.summary_line().contains("no frames recorded yet"),
            "zeros would read as a measured 0 ms frame: {}",
            overlay.summary_line()
        );
        assert!(overlay
            .hud_lines()
            .iter()
            .any(|line| line.contains("none recorded yet")));
    }

    #[test]
    fn the_summary_line_carries_tier_clock_budget_and_scale() {
        let timeline = timeline_with(&[10, 20, 30, 40, 50], 16);
        let overlay = QualityOverlay::build(
            "low",
            "tier low, 1.9 GB",
            Some("GPU path renders at 0.50x resolution (VIEWW_GPU_SCALE=auto: tier low)"),
            &timeline,
            600,
        );
        let line = overlay.summary_line();
        assert!(line.starts_with("tier low · "), "{line}");
        // mean (10+20+30+40+50)/5 = 30ms; p95 nearest-rank over 5 =
        // ceil(4.75)=5th = 50ms; worst 50ms; budget 16ms; the 10ms frame
        // is under it, so 4 of 5 over.
        assert!(line.contains("median 30ms"), "{line}");
        assert!(line.contains("p95 50ms"), "{line}");
        assert!(line.contains("worst 50ms"), "{line}");
        assert!(line.contains("4/5 over"), "{line}");
        assert!(line.contains("0.50x"), "{line}");
        // One line by construction: the farm's key copy takes one.
        assert!(!line.contains('\n'), "{line}");
    }

    #[test]
    fn full_resolution_says_so_instead_of_nothing() {
        let timeline = timeline_with(&[5, 6, 7], 16);
        let overlay = QualityOverlay::build("high", "tier high, 11.5 GB", None, &timeline, 600);
        let line = overlay.summary_line();
        assert!(!line.contains("· ·"), "{line}");
        let hud = overlay.hud_lines();
        assert!(
            hud[0].contains("GPU full resolution"),
            "an absent scale is a fact the HUD should state, not omit: {:?}",
            hud[0]
        );
    }

    #[test]
    fn the_window_limits_how_far_back_the_clock_looks() {
        let timeline = timeline_with(&[5, 5, 5, 100, 100, 100], 16);
        // Whole timeline: mean is pulled up by the three 100ms frames.
        let all = QualityOverlay::build("low", "d", None, &timeline, 600);
        // Last three only: the calm start is outside the window.
        let tail = QualityOverlay::build("low", "d", None, &timeline, 3);
        assert_eq!(all.frame.samples, 6);
        assert_eq!(tail.frame.samples, 3);
        assert_eq!(tail.frame.average, Duration::from_millis(100));
        assert!(
            all.frame.average < tail.frame.average,
            "the window is what lets a HUD react to lately, not to history: {:?} vs {:?}",
            all.frame.average,
            tail.frame.average
        );
    }

    #[test]
    fn over_budget_counts_against_each_frames_own_recorded_budget() {
        // The budget here comes from the samples, not a parameter — so a
        // timeline whose budget changed mid-session is counted honestly.
        let mut timeline = FrameTimeline::new(8);
        for millis in 1..=4u64 {
            timeline.record(sample(millis, millis, 16));
        }
        for millis in 5..=8u64 {
            timeline.record(sample(millis, millis * 10, 8));
        }
        let overlay = QualityOverlay::build("low", "d", None, &timeline, 8);
        assert_eq!(
            overlay.frame.over_budget, 4,
            "1..=4ms under 16, 50..=80 over 8"
        );
        assert_eq!(
            overlay.budget,
            Some(Duration::from_millis(8)),
            "the newest sample's budget is the window's"
        );
    }
}
