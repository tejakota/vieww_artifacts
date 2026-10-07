//! The clock an animation reads — and what happens when a frame arrives late.
//!
//! GSAP's ticker (§2.1 L3 of the comparison document) does one thing every
//! other part of this crate refuses to do: it *adjusts time*. When the main
//! thread stalls for half a second, a pure function of wall time jumps half a
//! second ahead on the next frame — correct, and on a busy page it looks like a
//! teleport. GSAP's `lagSmoothing(threshold, adjustedLag)` says: if a single
//! frame's delta exceeds `threshold`, pretend only `adjustedLag` passed.
//!
//! This module keeps the crate's rule — every animation is a function of the
//! timestamp it is handed — and moves the adjustment to where it belongs: the
//! **clock** that produces that timestamp. A [`LagSmoothing`] clock is fed raw
//! frame timestamps and hands out *animation time*, which is wall time minus
//! every stall it chose to forgive. Animations sampled at animation time stay
//! pure functions; they are simply sampled on a clock that skips stalls.
//!
//! [`FrameClock`] is the plain alternative: a fixed-rate clock for offline
//! rendering (the film harness), where frame `n` is exactly `n / fps` and
//! there is no wall time at all.
//!
//! ```
//! use std::time::Duration;
//! use vieww_animation::clock::LagSmoothing;
//!
//! let ms = Duration::from_millis;
//! // GSAP's default: frames over 500 ms are treated as 33 ms.
//! let mut clock = LagSmoothing::new(ms(500), ms(33));
//! assert_eq!(clock.tick(ms(0)), ms(0));
//! assert_eq!(clock.tick(ms(16)), ms(16));
//! // The main thread stalls for two seconds:
//! assert_eq!(clock.tick(ms(2016)), ms(49), "the stall was forgiven");
//! assert_eq!(clock.forgiven(), ms(1967));
//! ```

use std::time::Duration;

/// A clock that forgives stalls, after GSAP's `ticker.lagSmoothing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LagSmoothing {
    threshold: Duration,
    adjusted: Duration,
    last_wall: Option<Duration>,
    animation_time: Duration,
    forgiven: Duration,
    stalls: u32,
    time_scale_milli: u32,
}

impl LagSmoothing {
    /// Frames whose delta exceeds `threshold` advance animation time by only
    /// `adjusted`. `adjusted` is clamped to `threshold`.
    #[must_use]
    pub fn new(threshold: Duration, adjusted: Duration) -> Self {
        Self {
            threshold,
            adjusted: adjusted.min(threshold),
            last_wall: None,
            animation_time: Duration::ZERO,
            forgiven: Duration::ZERO,
            stalls: 0,
            time_scale_milli: 1000,
        }
    }

    /// GSAP's defaults: 500 ms threshold, 33 ms adjusted lag.
    #[must_use]
    pub fn gsap_default() -> Self {
        Self::new(Duration::from_millis(500), Duration::from_millis(33))
    }

    /// No smoothing at all — `lagSmoothing(0)` in GSAP.
    #[must_use]
    pub fn disabled() -> Self {
        Self::new(Duration::MAX, Duration::MAX)
    }

    /// The global `timeScale`: animation time runs at `scale` × wall time
    /// (resolution 1/1000).
    #[must_use]
    pub fn time_scale(mut self, scale: f32) -> Self {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let m = (scale.max(0.0) * 1000.0).round() as u32;
        self.time_scale_milli = m;
        self
    }

    /// Feed one frame's wall timestamp; returns the animation time to sample at.
    pub fn tick(&mut self, wall: Duration) -> Duration {
        let delta = match self.last_wall {
            None => Duration::ZERO,
            Some(prev) => wall.saturating_sub(prev),
        };
        self.last_wall = Some(wall);
        let step = if delta > self.threshold {
            self.stalls += 1;
            self.forgiven += delta - self.adjusted;
            self.adjusted
        } else {
            delta
        };
        let scaled = step * self.time_scale_milli / 1000;
        self.animation_time += scaled;
        self.animation_time
    }

    /// Animation time as of the last tick.
    #[must_use]
    pub const fn now(&self) -> Duration {
        self.animation_time
    }

    /// Total wall time skipped by smoothing.
    #[must_use]
    pub const fn forgiven(&self) -> Duration {
        self.forgiven
    }

    /// How many frames were treated as stalls.
    #[must_use]
    pub const fn stalls(&self) -> u32 {
        self.stalls
    }
}

/// A fixed-rate clock: frame `n` is at exactly `n / fps` seconds.
///
/// The offline-render clock (Manim, Motion Canvas, After Effects' render
/// queue): no wall time, no drift, and the same frame index always maps to the
/// same nanosecond, so a re-render is byte-identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameClock {
    fps: u32,
}

impl FrameClock {
    #[must_use]
    pub const fn new(fps: u32) -> Self {
        Self { fps: if fps == 0 { 1 } else { fps } }
    }

    /// The frames per second.
    #[must_use]
    pub const fn fps(&self) -> u32 {
        self.fps
    }

    /// The timestamp of frame `n`, computed without accumulating error.
    #[must_use]
    pub fn at(&self, frame: u64) -> Duration {
        // Ceiling, so `frame_at(at(n)) == n` exactly under floor division.
        let fps = u128::from(self.fps);
        let nanos = (u128::from(frame) * 1_000_000_000 + fps - 1) / fps;
        #[allow(clippy::cast_possible_truncation)]
        Duration::from_nanos(nanos as u64)
    }

    /// The frame showing at time `t` (floor).
    #[must_use]
    pub fn frame_at(&self, t: Duration) -> u64 {
        #[allow(clippy::cast_possible_truncation)]
        let f = (t.as_nanos() * u128::from(self.fps) / 1_000_000_000) as u64;
        f
    }

    /// How many frames `seconds` lasts (rounded to nearest).
    #[must_use]
    pub fn frames_in(&self, seconds: f64) -> u64 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = (seconds * f64::from(self.fps)).round().max(0.0) as u64;
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    #[test]
    fn steady_frames_pass_through_unchanged() {
        let mut c = LagSmoothing::gsap_default();
        for i in 0..100 {
            assert_eq!(c.tick(ms(i * 16)), ms(i * 16));
        }
        assert_eq!(c.stalls(), 0);
    }

    #[test]
    fn a_stall_is_forgiven_and_counted() {
        let mut c = LagSmoothing::new(ms(100), ms(20));
        c.tick(ms(0));
        c.tick(ms(10));
        assert_eq!(c.tick(ms(1010)), ms(30));
        assert_eq!(c.tick(ms(1020)), ms(40), "time resumes from the forgiven point");
        assert_eq!(c.stalls(), 1);
        assert_eq!(c.forgiven(), ms(980));
    }

    #[test]
    fn disabled_smoothing_is_wall_time() {
        let mut c = LagSmoothing::disabled();
        c.tick(ms(0));
        assert_eq!(c.tick(ms(5000)), ms(5000));
    }

    #[test]
    fn time_scale_slows_the_clock() {
        let mut c = LagSmoothing::disabled().time_scale(0.5);
        c.tick(ms(0));
        assert_eq!(c.tick(ms(1000)), ms(500));
    }

    #[test]
    fn adjusted_never_exceeds_threshold() {
        let mut c = LagSmoothing::new(ms(10), ms(50));
        c.tick(ms(0));
        assert_eq!(c.tick(ms(1000)), ms(10));
    }

    #[test]
    fn frame_clock_is_exact_and_invertible() {
        let c = FrameClock::new(60);
        assert_eq!(c.at(60), Duration::from_secs(1));
        assert_eq!(c.at(18_000), Duration::from_secs(300));
        for f in [0, 1, 59, 60, 61, 12_345] {
            assert_eq!(c.frame_at(c.at(f)), f);
        }
        assert_eq!(c.frames_in(2.5), 150);
    }
}
