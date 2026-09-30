//! Channel operators — TouchDesigner's CHOPs as composable signal filters.
//!
//! # The capability
//!
//! TouchDesigner's CHOP layer (§2.8 L3) is how a live installation turns raw
//! numbers — a sensor, an audio envelope, a MIDI fader, the mouse — into
//! motion that looks intended: the *Lag* CHOP smooths a jittery fader, the
//! *Filter* CHOP (one-euro) kills sensor noise without lag on fast moves, the
//! *Speed* CHOP integrates a rate into a position, *Slope* differentiates,
//! *Trigger* turns a threshold crossing into an ADSR envelope, *Math*
//! remaps ranges, *Limit* clamps or wraps, *Hold* samples-and-holds, and
//! *Delay* plays the signal back late. [`lfo`](crate::lfo) and
//! [`noise`](crate::noise) are already the LFO and Noise CHOPs; this module
//! is the rest of the everyday set.
//!
//! Each operator is a [`Chop`]: `process(input, dt) -> output`, stateful,
//! frame-rate independent (every time constant is in seconds, not frames),
//! and chainable with [`Chain`] — the wire between two CHOPs.

use std::collections::VecDeque;
use std::fmt;

/// A channel operator.
pub trait Chop: fmt::Debug {
    /// Feed one sample taken `dt` seconds after the previous one.
    fn process(&mut self, input: f32, dt: f32) -> f32;
    /// Forget history.
    fn reset(&mut self);
}

/// Exponential smoothing with separate rise and fall times — the Lag CHOP.
#[derive(Debug, Clone)]
pub struct Lag {
    pub rise: f32,
    pub fall: f32,
    state: Option<f32>,
}

impl Lag {
    /// Seconds to cover ~63% of a step up and down respectively.
    #[must_use]
    pub const fn new(rise: f32, fall: f32) -> Self {
        Self { rise, fall, state: None }
    }
}

impl Chop for Lag {
    fn process(&mut self, input: f32, dt: f32) -> f32 {
        let prev = self.state.unwrap_or(input);
        let tau = if input > prev { self.rise } else { self.fall };
        let a = if tau <= 0.0 { 1.0 } else { 1.0 - (-dt / tau).exp() };
        let out = prev + (input - prev) * a;
        self.state = Some(out);
        out
    }
    fn reset(&mut self) {
        self.state = None;
    }
}

/// The 1€ filter (Casiez et al. 2012) — the Filter CHOP's "one euro" type:
/// heavy smoothing when still, little lag when moving fast.
#[derive(Debug, Clone)]
pub struct OneEuro {
    pub min_cutoff: f32,
    pub beta: f32,
    pub d_cutoff: f32,
    x: Option<f32>,
    dx: f32,
}

impl OneEuro {
    #[must_use]
    pub const fn new(min_cutoff: f32, beta: f32) -> Self {
        Self {
            min_cutoff,
            beta,
            d_cutoff: 1.0,
            x: None,
            dx: 0.0,
        }
    }
}

fn alpha(cutoff: f32, dt: f32) -> f32 {
    let tau = 1.0 / (2.0 * std::f32::consts::PI * cutoff.max(1e-4));
    1.0 / (1.0 + tau / dt.max(1e-6))
}

impl Chop for OneEuro {
    fn process(&mut self, input: f32, dt: f32) -> f32 {
        let Some(prev) = self.x else {
            self.x = Some(input);
            return input;
        };
        let raw_dx = (input - prev) / dt.max(1e-6);
        self.dx += (raw_dx - self.dx) * alpha(self.d_cutoff, dt);
        let cutoff = self.min_cutoff + self.beta * self.dx.abs();
        let out = prev + (input - prev) * alpha(cutoff, dt);
        self.x = Some(out);
        out
    }
    fn reset(&mut self) {
        self.x = None;
        self.dx = 0.0;
    }
}

/// Integrate a rate into a position — the Speed CHOP. Optional limits clamp
/// (and zero the rate at the wall, like a physical stop).
#[derive(Debug, Clone, Default)]
pub struct Speed {
    pub value: f32,
    pub limits: Option<(f32, f32)>,
}

impl Speed {
    #[must_use]
    pub const fn new(start: f32) -> Self {
        Self { value: start, limits: None }
    }
    #[must_use]
    pub const fn limits(mut self, lo: f32, hi: f32) -> Self {
        self.limits = Some((lo, hi));
        self
    }
}

impl Chop for Speed {
    fn process(&mut self, rate: f32, dt: f32) -> f32 {
        self.value += rate * dt;
        if let Some((lo, hi)) = self.limits {
            self.value = self.value.clamp(lo, hi);
        }
        self.value
    }
    fn reset(&mut self) {
        self.value = 0.0;
    }
}

/// Rate of change per second — the Slope CHOP.
#[derive(Debug, Clone, Default)]
pub struct Slope {
    prev: Option<f32>,
}

impl Chop for Slope {
    fn process(&mut self, input: f32, dt: f32) -> f32 {
        let out = self.prev.map_or(0.0, |p| (input - p) / dt.max(1e-6));
        self.prev = Some(input);
        out
    }
    fn reset(&mut self) {
        self.prev = None;
    }
}

/// Attack–decay–sustain–release on a threshold gate — the Trigger CHOP.
#[derive(Debug, Clone)]
pub struct Envelope {
    pub threshold: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    level: f32,
    stage: Stage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

impl Envelope {
    #[must_use]
    pub const fn new(threshold: f32, attack: f32, decay: f32, sustain: f32, release: f32) -> Self {
        Self {
            threshold,
            attack,
            decay,
            sustain,
            release,
            level: 0.0,
            stage: Stage::Idle,
        }
    }

    /// Whether the gate is currently open (attack, decay or sustain).
    #[must_use]
    pub fn is_open(&self) -> bool {
        matches!(self.stage, Stage::Attack | Stage::Decay | Stage::Sustain)
    }
}

impl Chop for Envelope {
    fn process(&mut self, input: f32, dt: f32) -> f32 {
        let gate = input >= self.threshold;
        if gate && !self.is_open() {
            self.stage = Stage::Attack;
        } else if !gate && self.is_open() {
            self.stage = Stage::Release;
        }
        let rate = |secs: f32, span: f32| if secs <= 0.0 { f32::INFINITY } else { span / secs };
        match self.stage {
            Stage::Idle => self.level = 0.0,
            Stage::Attack => {
                self.level += rate(self.attack, 1.0) * dt;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.level -= rate(self.decay, 1.0 - self.sustain) * dt;
                if self.level <= self.sustain {
                    self.level = self.sustain;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => self.level = self.sustain,
            Stage::Release => {
                self.level -= rate(self.release, 1.0) * dt;
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
            }
        }
        self.level
    }
    fn reset(&mut self) {
        self.level = 0.0;
        self.stage = Stage::Idle;
    }
}

/// `from` range to `to` range, optionally clamped — the Math CHOP's Range
/// page, and Processing's `map()`.
#[derive(Debug, Clone, Copy)]
pub struct Remap {
    pub from: (f32, f32),
    pub to: (f32, f32),
    pub clamp: bool,
}

impl Chop for Remap {
    fn process(&mut self, input: f32, _dt: f32) -> f32 {
        let span = self.from.1 - self.from.0;
        let mut u = if span.abs() < 1e-12 { 0.0 } else { (input - self.from.0) / span };
        if self.clamp {
            u = u.clamp(0.0, 1.0);
        }
        self.to.0 + (self.to.1 - self.to.0) * u
    }
    fn reset(&mut self) {}
}

/// What a [`Limit`] does at its bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitMode {
    Clamp,
    /// Wrap around (a phase, an angle).
    Loop,
    /// Bounce back and forth.
    ZigZag,
}

/// Keep a value in range — the Limit CHOP.
#[derive(Debug, Clone, Copy)]
pub struct Limit {
    pub min: f32,
    pub max: f32,
    pub mode: LimitMode,
}

impl Chop for Limit {
    fn process(&mut self, x: f32, _dt: f32) -> f32 {
        let span = self.max - self.min;
        if span <= 0.0 {
            return self.min;
        }
        match self.mode {
            LimitMode::Clamp => x.clamp(self.min, self.max),
            LimitMode::Loop => self.min + (x - self.min).rem_euclid(span),
            LimitMode::ZigZag => {
                let p = (x - self.min).rem_euclid(2.0 * span);
                self.min + if p > span { 2.0 * span - p } else { p }
            }
        }
    }
    fn reset(&mut self) {}
}

/// Sample the input while `hold` is off, freeze while on — the Hold CHOP.
/// The hold flag is the second input, set with [`SampleHold::set_hold`].
#[derive(Debug, Clone, Default)]
pub struct SampleHold {
    pub hold: bool,
    value: f32,
}

impl SampleHold {
    pub fn set_hold(&mut self, hold: bool) {
        self.hold = hold;
    }
}

impl Chop for SampleHold {
    fn process(&mut self, input: f32, _dt: f32) -> f32 {
        if !self.hold {
            self.value = input;
        }
        self.value
    }
    fn reset(&mut self) {
        self.value = 0.0;
    }
}

/// Play the input back `delay` seconds late — the Delay CHOP.
#[derive(Debug, Clone)]
pub struct Delay {
    pub delay: f32,
    history: VecDeque<(f32, f32)>,
    clock: f32,
}

impl Delay {
    #[must_use]
    pub const fn new(delay: f32) -> Self {
        Self {
            delay,
            history: VecDeque::new(),
            clock: 0.0,
        }
    }
}

impl Chop for Delay {
    fn process(&mut self, input: f32, dt: f32) -> f32 {
        self.clock += dt;
        self.history.push_back((self.clock, input));
        let target = self.clock - self.delay;
        while self.history.len() > 2 && self.history[1].0 <= target {
            self.history.pop_front();
        }
        let (t0, v0) = self.history[0];
        match self.history.get(1) {
            Some(&(t1, v1)) if target >= t0 && t1 > t0 => v0 + (v1 - v0) * ((target - t0) / (t1 - t0)).clamp(0.0, 1.0),
            _ => v0,
        }
    }
    fn reset(&mut self) {
        self.history.clear();
        self.clock = 0.0;
    }
}

/// CHOPs wired in series.
#[derive(Debug, Default)]
pub struct Chain {
    stages: Vec<Box<dyn Chop>>,
}

impl Chain {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn then(mut self, chop: impl Chop + 'static) -> Self {
        self.stages.push(Box::new(chop));
        self
    }
}

impl Chop for Chain {
    fn process(&mut self, input: f32, dt: f32) -> f32 {
        self.stages.iter_mut().fold(input, |x, s| s.process(x, dt))
    }
    fn reset(&mut self) {
        for s in &mut self.stages {
            s.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(c: &mut dyn Chop, input: impl Fn(usize) -> f32, n: usize, dt: f32) -> Vec<f32> {
        (0..n).map(|i| c.process(input(i), dt)).collect()
    }

    #[test]
    fn lag_is_frame_rate_independent_and_asymmetric() {
        let mut a = Lag::new(0.5, 0.1);
        let mut b = Lag::new(0.5, 0.1);
        a.process(0.0, 1.0 / 60.0);
        b.process(0.0, 1.0 / 30.0);
        let va = *run(&mut a, |_| 1.0, 60, 1.0 / 60.0).last().unwrap();
        let vb = *run(&mut b, |_| 1.0, 30, 1.0 / 30.0).last().unwrap();
        assert!((va - vb).abs() < 1e-4, "{va} vs {vb}");
        assert!((va - (1.0 - (-2.0f32).exp())).abs() < 1e-3, "one second is two time constants");
        let down = *run(&mut a, |_| 0.0, 6, 1.0 / 60.0).last().unwrap();
        assert!(down < va * 0.5, "falls faster than it rose");
    }

    #[test]
    fn one_euro_smooths_jitter_but_tracks_fast_motion() {
        let jitter = |i: usize| if i % 2 == 0 { 0.05 } else { -0.05 };
        let mut f = OneEuro::new(1.0, 0.5);
        let still = run(&mut f, jitter, 120, 1.0 / 60.0);
        assert!(still[60..].iter().all(|v| v.abs() < 0.02));
        let mut g = OneEuro::new(1.0, 0.5);
        #[allow(clippy::cast_precision_loss)]
        let ramp = run(&mut g, |i| i as f32 * 5.0, 60, 1.0 / 60.0);
        assert!((ramp[59] - 295.0).abs() < 30.0, "little lag on a fast ramp: {}", ramp[59]);
    }

    #[test]
    fn speed_integrates_and_slope_differentiates() {
        let mut s = Speed::new(0.0).limits(-10.0, 10.0);
        let v = run(&mut s, |_| 4.0, 30, 0.1);
        assert!((v[9] - 4.0).abs() < 1e-4 && (v[29] - 10.0).abs() < 1e-4);
        let mut d = Slope::default();
        #[allow(clippy::cast_precision_loss)]
        let r = run(&mut d, |i| i as f32 * 3.0, 5, 0.5);
        assert_eq!(r[0], 0.0);
        assert!((r[4] - 6.0).abs() < 1e-4);
    }

    #[test]
    fn envelope_runs_adsr_on_the_gate() {
        let mut e = Envelope::new(0.5, 0.1, 0.1, 0.5, 0.2);
        let gate = |i: usize| if i < 50 { 1.0 } else { 0.0 };
        let out = run(&mut e, gate, 100, 0.01);
        assert!((out[9] - 1.0).abs() < 0.02, "attack peak {}", out[9]);
        assert!((out[40] - 0.5).abs() < 1e-4, "sustain");
        assert!(out[53] < 0.5 && out[53] > 0.0, "releasing: {}", out[53]);
        assert_eq!(out[99], 0.0);
    }

    #[test]
    fn remap_limit_hold_delay() {
        let mut r = Remap { from: (0.0, 10.0), to: (100.0, 200.0), clamp: true };
        assert_eq!(r.process(5.0, 0.0), 150.0);
        assert_eq!(r.process(20.0, 0.0), 200.0);
        let mut l = Limit { min: 0.0, max: 1.0, mode: LimitMode::Loop };
        assert!((l.process(2.25, 0.0) - 0.25).abs() < 1e-6);
        let mut z = Limit { min: 0.0, max: 1.0, mode: LimitMode::ZigZag };
        assert!((z.process(1.25, 0.0) - 0.75).abs() < 1e-6);
        let mut h = SampleHold::default();
        h.process(3.0, 0.0);
        h.set_hold(true);
        assert_eq!(h.process(9.0, 0.0), 3.0);
        let mut d = Delay::new(0.5);
        #[allow(clippy::cast_precision_loss)]
        let out = run(&mut d, |i| i as f32, 20, 0.1);
        assert!((out[19] - 14.0).abs() < 1e-3, "half a second late: {}", out[19]);
    }

    #[test]
    fn chains_compose() {
        let mut c = Chain::new()
            .then(Remap { from: (0.0, 1.0), to: (0.0, 10.0), clamp: false })
            .then(Limit { min: 0.0, max: 5.0, mode: LimitMode::Clamp });
        assert_eq!(c.process(0.3, 0.0), 3.0);
        assert_eq!(c.process(0.9, 0.0), 5.0);
    }
}
