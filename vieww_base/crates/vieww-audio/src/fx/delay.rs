//! A tempo-synced stereo delay: straight or ping-pong, with filters in the
//! feedback loop so repeats darken and thin as they decay — the "dub"
//! delay every DAW ships, locked to the transport.
//!
//! The original [`Delay`](crate::dsp::Delay) is a mono feedback line set in
//! seconds. This one sets its time from a [`Division`] and a tempo, so a
//! dotted-eighth echo stays a dotted eighth when the song speeds up, and its
//! read head glides to the new time (no zipper, no click) instead of
//! jumping.

use super::filter::{DelayLine, Svf, SvfMode};
use super::Stereo;
use crate::dsp::Processor;
use crate::modulation::Division;

/// How the delay sets its time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DelayTime {
    /// Milliseconds.
    Ms(f32),
    /// A note division at the current tempo.
    Sync(Division),
}

/// A stereo delay.
#[derive(Debug, Clone)]
pub struct StereoDelay {
    lines: [DelayLine; 2],
    /// Time setting.
    pub time: DelayTime,
    /// Feedback 0..0.98.
    pub feedback: f32,
    /// Ping-pong: each repeat crosses to the other side.
    pub ping_pong: bool,
    /// Wet level.
    pub wet: f32,
    /// Dry level.
    pub dry: f32,
    hp: [Svf; 2],
    lp: [Svf; 2],
    rate: f32,
    bpm: f64,
    current: f32,
    glide: f32,
}

impl StereoDelay {
    /// A delay holding up to `max_seconds`.
    #[must_use]
    pub fn new(time: DelayTime, feedback: f32, max_seconds: f32, rate: f32) -> Self {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "lengths"
        )]
        let cap = (max_seconds * rate) as usize + 8;
        let mut d = Self {
            lines: [DelayLine::new(cap), DelayLine::new(cap)],
            time,
            feedback: feedback.clamp(0.0, 0.98),
            ping_pong: false,
            wet: 0.35,
            dry: 1.0,
            hp: [
                Svf::new(SvfMode::HighPass, 120.0, 0.707, rate),
                Svf::new(SvfMode::HighPass, 120.0, 0.707, rate),
            ],
            lp: [
                Svf::new(SvfMode::LowPass, 6000.0, 0.707, rate),
                Svf::new(SvfMode::LowPass, 6000.0, 0.707, rate),
            ],
            rate,
            bpm: 120.0,
            current: 0.0,
            glide: (-1.0 / (0.05 * rate)).exp(),
        };
        d.current = d.target_samples();
        d
    }

    /// Set the tempo for synced times.
    pub const fn set_bpm(&mut self, bpm: f64) {
        self.bpm = bpm;
    }

    /// Set the feedback filters (high-pass and low-pass corners).
    pub fn set_filters(&mut self, hp_hz: f32, lp_hz: f32) {
        for f in &mut self.hp {
            f.set(hp_hz, 0.707);
        }
        for f in &mut self.lp {
            f.set(lp_hz, 0.707);
        }
    }

    /// The target delay in samples.
    #[must_use]
    pub fn target_samples(&self) -> f32 {
        let max = self.lines[0].capacity().saturating_sub(4);
        #[expect(clippy::cast_precision_loss, reason = "capacity")]
        let max = max as f32;
        let s = match self.time {
            DelayTime::Ms(ms) => ms * 0.001 * self.rate,
            #[expect(clippy::cast_possible_truncation, reason = "f32 delay")]
            DelayTime::Sync(d) => (d.beats * 60.0 / self.bpm) as f32 * self.rate,
        };
        s.clamp(1.0, max)
    }
}

impl Stereo for StereoDelay {
    fn process_stereo(&mut self, l: f32, r: f32) -> (f32, f32) {
        let target = self.target_samples();
        self.current = target + (self.current - target) * self.glide;
        if (self.current - target).abs() < 1e-3 {
            self.current = target;
        }
        // Read before writing: the tap is `current` samples old.
        let dl = self.lines[0].tap_frac(self.current);
        let dr = self.lines[1].tap_frac(self.current);
        let fl = self.lp[0].process(self.hp[0].process(dl)) * self.feedback;
        let fr = self.lp[1].process(self.hp[1].process(dr)) * self.feedback;
        if self.ping_pong {
            // Mono input enters the left line; each repeat swaps sides.
            self.lines[0].push((l + r) * 0.5 + fr);
            self.lines[1].push(fl);
        } else {
            self.lines[0].push(l + fl);
            self.lines[1].push(r + fr);
        }
        (l * self.dry + dl * self.wet, r * self.dry + dr * self.wet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse(d: &mut StereoDelay, n: usize) -> (Vec<f32>, Vec<f32>) {
        let mut l = Vec::new();
        let mut r = Vec::new();
        for i in 0..n {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let (a, b) = d.process_stereo(x, x);
            l.push(a);
            r.push(b);
        }
        (l, r)
    }

    fn first_peak(x: &[f32], from: usize) -> usize {
        from + x[from..]
            .iter()
            .position(|v| v.abs() > 0.05)
            .unwrap_or(usize::MAX - from)
    }

    #[test]
    fn synced_time_follows_the_tempo() {
        let mut d = StereoDelay::new(DelayTime::Sync(Division::dotted(8)), 0.0, 2.0, 1000.0);
        d.dry = 0.0;
        d.wet = 1.0;
        d.set_filters(1.0, 450.0);
        d.set_bpm(120.0);
        d.current = d.target_samples();
        assert!(
            (d.target_samples() - 375.0).abs() < 1e-3,
            "dotted 8th at 120 = 375 ms"
        );
        let (l, _) = impulse(&mut d, 1000);
        let p = first_peak(&l, 1);
        assert!((374..=377).contains(&p), "{p}");
        d.set_bpm(60.0);
        assert!((d.target_samples() - 750.0).abs() < 1e-3);
    }

    #[test]
    fn ping_pong_alternates_sides() {
        let mut d = StereoDelay::new(DelayTime::Ms(100.0), 0.7, 1.0, 1000.0);
        d.ping_pong = true;
        d.dry = 0.0;
        d.wet = 1.0;
        d.set_filters(1.0, 450.0);
        let (l, r) = impulse(&mut d, 400);
        let e = |x: &[f32], a: usize| {
            x[a.saturating_sub(5)..a + 5]
                .iter()
                .fold(0.0f32, |m, v| m.max(v.abs()))
        };
        assert!(e(&l, 100) > 0.3 && e(&r, 100) < 0.05, "first repeat left");
        assert!(e(&r, 200) > 0.1 && e(&l, 200) < 0.05, "second repeat right");
        assert!(e(&l, 300) > 0.05 && e(&r, 300) < 0.02, "third left again");
    }

    #[test]
    fn feedback_repeats_decay() {
        let mut d = StereoDelay::new(DelayTime::Ms(50.0), 0.5, 1.0, 1000.0);
        d.dry = 0.0;
        d.wet = 1.0;
        d.set_filters(1.0, 450.0);
        let (l, _) = impulse(&mut d, 400);
        let peak = |c: usize| l[c - 3..c + 3].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak(100) < peak(50) && peak(150) < peak(100));
    }

    #[test]
    fn changing_time_glides() {
        let mut d = StereoDelay::new(DelayTime::Ms(100.0), 0.0, 1.0, 48_000.0);
        d.process_stereo(0.0, 0.0);
        let before = d.current;
        d.time = DelayTime::Ms(200.0);
        d.process_stereo(0.0, 0.0);
        assert!(
            d.current > before && d.current < before * 1.01,
            "glides, no jump"
        );
    }
}
