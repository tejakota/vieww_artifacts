//! Modulation effects: chorus, flanger, phaser, tremolo, auto-pan, vibrato
//! and ring modulation.
//!
//! All of them are an LFO moving something: a short delay's length (chorus,
//! flanger, vibrato), the corner of a chain of all-passes (phaser), the
//! gain (tremolo, ring mod) or the pan (auto-pan). The LFOs here are
//! [`modulation::Lfo`](crate::modulation::Lfo)s evaluated at a sample count,
//! so every effect is reproducible, and each takes an optional tempo so its
//! rate can lock to the transport like any other synced modulation.

use super::filter::{Allpass1, DelayLine};
use super::Stereo;
use crate::dsp::Processor;
use crate::modulation::{Lfo, LfoShape};

/// Shared LFO clock for the effects.
#[derive(Debug, Clone)]
struct Clock {
    lfo: Lfo,
    rate: f64,
    n: u64,
    bpm: f64,
}

impl Clock {
    fn new(lfo: Lfo, rate: f32) -> Self {
        Self {
            lfo,
            rate: f64::from(rate),
            n: 0,
            bpm: 120.0,
        }
    }

    fn at(&self, phase_offset: f64) -> f32 {
        #[expect(clippy::cast_precision_loss, reason = "sample counts")]
        let t = self.n as f64 / self.rate;
        let mut l = self.lfo;
        l.phase += phase_offset;
        l.at_seconds(t, self.bpm)
    }

    fn tick(&mut self) {
        self.n += 1;
    }
}

/// A multi-voice chorus: `voices` delays between 7 and 30 ms, each swept by
/// the LFO at a different phase, alternated across the stereo field.
#[derive(Debug, Clone)]
pub struct Chorus {
    line: DelayLine,
    clock: Clock,
    voices: usize,
    rate: f32,
    /// Centre delay in ms.
    pub delay_ms: f32,
    /// Sweep depth in ms.
    pub depth_ms: f32,
    /// Wet/dry 0..=1.
    pub mix: f32,
}

impl Chorus {
    /// A chorus.
    #[must_use]
    pub fn new(voices: usize, rate_hz: f64, rate: f32) -> Self {
        Self {
            line: DelayLine::new((rate * 0.06) as usize),
            clock: Clock::new(Lfo::hz(LfoShape::Triangle, rate_hz), rate),
            voices: voices.max(1),
            rate,
            delay_ms: 15.0,
            depth_ms: 5.0,
            mix: 0.5,
        }
    }
}

impl Stereo for Chorus {
    fn process_stereo(&mut self, l: f32, r: f32) -> (f32, f32) {
        let mono = (l + r) * 0.5;
        self.line.push(mono);
        let mut wl = 0.0;
        let mut wr = 0.0;
        for v in 0..self.voices {
            #[expect(clippy::cast_precision_loss, reason = "voice counts")]
            let ph = v as f64 / self.voices as f64;
            let d = (self.delay_ms + self.depth_ms * self.clock.at(ph)) * 0.001 * self.rate;
            let s = self.line.tap_frac(d.max(0.0) + 1.0);
            if v % 2 == 0 {
                wl += s;
            } else {
                wr += s;
            }
        }
        if self.voices == 1 {
            wr = wl;
        }
        self.clock.tick();
        #[expect(clippy::cast_precision_loss, reason = "voice counts")]
        let norm = 2.0 / self.voices.max(2) as f32;
        (
            l * (1.0 - self.mix) + wl * norm * self.mix,
            r * (1.0 - self.mix) + wr * norm * self.mix,
        )
    }
}

/// A flanger: a 0.5–10 ms swept delay with feedback, mixed with the dry
/// signal so the comb's notches sweep.
#[derive(Debug, Clone)]
pub struct Flanger {
    line: DelayLine,
    clock: Clock,
    rate: f32,
    /// Minimum delay in ms.
    pub min_ms: f32,
    /// Maximum delay in ms.
    pub max_ms: f32,
    /// Feedback −0.95..=0.95.
    pub feedback: f32,
    /// Wet/dry.
    pub mix: f32,
    last: f32,
}

impl Flanger {
    /// A flanger.
    #[must_use]
    pub fn new(rate_hz: f64, feedback: f32, rate: f32) -> Self {
        Self {
            line: DelayLine::new((rate * 0.02) as usize),
            clock: Clock::new(Lfo::hz(LfoShape::Sine, rate_hz), rate),
            rate,
            min_ms: 0.5,
            max_ms: 6.0,
            feedback: feedback.clamp(-0.95, 0.95),
            mix: 0.5,
            last: 0.0,
        }
    }

    /// The current delay in samples (for tests and meters).
    #[must_use]
    pub fn current_delay(&self) -> f32 {
        let u = (self.clock.at(0.0) + 1.0) * 0.5;
        (self.min_ms + (self.max_ms - self.min_ms) * u) * 0.001 * self.rate
    }
}

impl Processor for Flanger {
    fn process(&mut self, x: f32) -> f32 {
        let d = self.current_delay().max(1.0);
        self.line.push(x + self.last * self.feedback);
        // `tap_frac(1)` is the sample just pushed, so a delay of `d` is `d + 1`.
        let wet = self.line.tap_frac(d + 1.0);
        self.last = wet;
        self.clock.tick();
        x * (1.0 - self.mix) + wet * self.mix
    }
}

/// A phaser: `stages` first-order all-passes whose shared corner sweeps
/// between `min_hz` and `max_hz`, with feedback, mixed with the dry signal.
/// Each pair of stages makes one notch.
#[derive(Debug, Clone)]
pub struct Phaser {
    stages: Vec<Allpass1>,
    clock: Clock,
    rate: f32,
    /// Lowest notch corner.
    pub min_hz: f32,
    /// Highest notch corner.
    pub max_hz: f32,
    /// Feedback −0.95..=0.95.
    pub feedback: f32,
    /// Wet/dry (0.5 gives the deepest notches).
    pub mix: f32,
    last: f32,
}

impl Phaser {
    /// A phaser with `stages` all-passes (even: 4, 6, 8, 12).
    #[must_use]
    pub fn new(stages: usize, rate_hz: f64, rate: f32) -> Self {
        Self {
            stages: vec![Allpass1::default(); stages.max(2)],
            clock: Clock::new(Lfo::hz(LfoShape::Sine, rate_hz), rate),
            rate,
            min_hz: 300.0,
            max_hz: 3000.0,
            feedback: 0.0,
            mix: 0.5,
            last: 0.0,
        }
    }

    /// Hold the sweep at `u` in 0..=1 (rate 0 patches, tests).
    pub fn set_corner_unit(&mut self, u: f32) {
        let hz = self.min_hz * (self.max_hz / self.min_hz).powf(u.clamp(0.0, 1.0));
        let a = Allpass1::coefficient(hz, self.rate);
        for s in &mut self.stages {
            s.a = a;
        }
    }
}

impl Processor for Phaser {
    fn process(&mut self, x: f32) -> f32 {
        let u = (self.clock.at(0.0) + 1.0) * 0.5;
        self.set_corner_unit(u);
        let mut y = x + self.last * self.feedback;
        for s in &mut self.stages {
            y = s.process(y);
        }
        self.last = y;
        self.clock.tick();
        x * (1.0 - self.mix) + y * self.mix
    }
}

/// Tremolo: gain modulated between `1 − depth` and 1.
#[derive(Debug, Clone)]
pub struct Tremolo {
    clock: Clock,
    /// Depth 0..=1.
    pub depth: f32,
}

impl Tremolo {
    /// A tremolo with any LFO (synced or free).
    #[must_use]
    pub fn new(lfo: Lfo, depth: f32, rate: f32) -> Self {
        Self {
            clock: Clock::new(lfo, rate),
            depth,
        }
    }

    /// Set the transport tempo for synced LFOs.
    pub const fn set_bpm(&mut self, bpm: f64) {
        self.clock.bpm = bpm;
    }
}

impl Processor for Tremolo {
    fn process(&mut self, x: f32) -> f32 {
        let u = (self.clock.at(0.0) + 1.0) * 0.5;
        self.clock.tick();
        x * (1.0 - self.depth * (1.0 - u))
    }
}

/// Auto-pan: an LFO moves a constant-power pan.
#[derive(Debug, Clone)]
pub struct AutoPan {
    clock: Clock,
    /// Width 0..=1.
    pub depth: f32,
}

impl AutoPan {
    /// An auto-panner.
    #[must_use]
    pub fn new(lfo: Lfo, depth: f32, rate: f32) -> Self {
        Self {
            clock: Clock::new(lfo, rate),
            depth,
        }
    }
}

impl Stereo for AutoPan {
    fn process_stereo(&mut self, l: f32, r: f32) -> (f32, f32) {
        let p = self.clock.at(0.0) * self.depth;
        self.clock.tick();
        let (gl, gr) = crate::synth::equal_power(p);
        let k = std::f32::consts::SQRT_2;
        (l * gl * k, r * gr * k)
    }
}

/// Vibrato: a swept delay with no dry signal — pitch wobble.
#[derive(Debug, Clone)]
pub struct Vibrato {
    line: DelayLine,
    clock: Clock,
    rate: f32,
    /// Depth in ms of delay swing.
    pub depth_ms: f32,
}

impl Vibrato {
    /// A vibrato at `rate_hz`.
    #[must_use]
    pub fn new(rate_hz: f64, depth_ms: f32, rate: f32) -> Self {
        Self {
            line: DelayLine::new((rate * 0.03) as usize),
            clock: Clock::new(Lfo::hz(LfoShape::Sine, rate_hz), rate),
            rate,
            depth_ms,
        }
    }
}

impl Processor for Vibrato {
    fn process(&mut self, x: f32) -> f32 {
        self.line.push(x);
        let d = (self.depth_ms * (1.0 + self.clock.at(0.0))) * 0.001 * self.rate + 1.0;
        self.clock.tick();
        self.line.tap_frac(d)
    }
}

/// Ring modulation by a sine carrier: sum and difference frequencies.
#[derive(Debug, Clone)]
pub struct RingMod {
    phase: f64,
    inc: f64,
    /// Wet/dry.
    pub mix: f32,
}

impl RingMod {
    /// Ring modulation at `hz`.
    #[must_use]
    pub fn new(hz: f32, rate: f32) -> Self {
        Self {
            phase: 0.0,
            inc: f64::from(hz) / f64::from(rate),
            mix: 1.0,
        }
    }
}

impl Processor for RingMod {
    fn process(&mut self, x: f32) -> f32 {
        #[expect(clippy::cast_possible_truncation, reason = "f32 audio")]
        let c = (std::f64::consts::TAU * self.phase).sin() as f32;
        self.phase = (self.phase + self.inc).rem_euclid(1.0);
        x * (1.0 - self.mix) + x * c * self.mix
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modulation::Division;
    use crate::synth::testing::{level_near, rms, spectrum};

    const RATE: f32 = 48_000.0;

    fn sine(hz: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| {
                #[expect(clippy::cast_precision_loss, reason = "test")]
                let t = i as f32 / RATE;
                (2.0 * std::f32::consts::PI * hz * t).sin()
            })
            .collect()
    }

    #[test]
    fn tremolo_swings_between_its_depth_bounds() {
        let mut t = Tremolo::new(Lfo::hz(LfoShape::Sine, 4.0), 0.6, RATE);
        let y: Vec<f32> = (0..48_000).map(|_| t.process(1.0)).collect();
        let lo = y.iter().copied().fold(f32::MAX, f32::min);
        let hi = y.iter().copied().fold(f32::MIN, f32::max);
        assert!((lo - 0.4).abs() < 1e-3 && (hi - 1.0).abs() < 1e-3);
    }

    #[test]
    fn synced_tremolo_locks_to_tempo() {
        let mut t = Tremolo::new(
            Lfo::synced(LfoShape::Square, Division::straight(4)),
            1.0,
            RATE,
        );
        t.set_bpm(120.0);
        let y: Vec<f32> = (0..48_000).map(|_| t.process(1.0)).collect();
        // A quarter at 120 BPM is 0.5 s: on for 0.25 s, off for 0.25 s.
        assert!(
            (y[6_000] - 1.0).abs() < 1e-6
                && y[18_000].abs() < 1e-6
                && (y[30_000] - 1.0).abs() < 1e-6
        );
    }

    #[test]
    fn auto_pan_moves_energy_between_channels() {
        let mut a = AutoPan::new(Lfo::hz(LfoShape::Sine, 1.0), 1.0, RATE);
        let mut l = Vec::new();
        let mut r = Vec::new();
        for _ in 0..48_000 {
            let (x, y) = a.process_stereo(1.0, 1.0);
            l.push(x);
            r.push(y);
        }
        assert!(
            l[36_000] > 1.3 && r[36_000] < 0.1,
            "hard left at the trough"
        );
        assert!(r[12_000] > 1.3 && l[12_000] < 0.1, "hard right at the peak");
        let p0 = l[0] * l[0] + r[0] * r[0];
        let p1 = l[12_000] * l[12_000] + r[12_000] * r[12_000];
        assert!((p0 - p1).abs() < 1e-3, "constant power");
    }

    #[test]
    fn flanger_sweeps_its_delay_and_notches() {
        let mut f = Flanger::new(0.25, 0.0, RATE);
        let d0 = f.current_delay();
        for _ in 0..48_000 {
            f.process(0.0);
        }
        let d1 = f.current_delay();
        assert!((d0 - d1).abs() > 10.0, "{d0} {d1}");
        // Frozen at a 1 ms delay, mix 0.5: a notch at 500 Hz.
        let mut f = Flanger::new(0.0, 0.0, RATE);
        f.min_ms = 1.0;
        f.max_ms = 1.0;
        let x = sine(500.0, 9_600);
        let y: Vec<f32> = x.iter().map(|v| f.process(*v)).collect();
        assert!(rms(&y[4_800..]) < 0.01, "{}", rms(&y[4_800..]));
        let mut f = Flanger::new(0.0, 0.0, RATE);
        f.min_ms = 1.0;
        f.max_ms = 1.0;
        let x = sine(1000.0, 9_600);
        let y: Vec<f32> = x.iter().map(|v| f.process(*v)).collect();
        assert!(rms(&y[4_800..]) > 0.6, "peak at 1 kHz");
    }

    #[test]
    fn phaser_notch_follows_its_corner() {
        // Four stages make two notches; the first is where the cascade's
        // phase reaches −180°, i.e. each stage −45°: below the corner.
        let level = |corner: f32, hz: f32| {
            let mut p = Phaser::new(4, 0.0, RATE);
            p.min_hz = corner;
            p.max_hz = corner;
            let x = sine(hz, 19_200);
            let y: Vec<f32> = x.iter().map(|v| p.process(*v)).collect();
            rms(&y[9_600..])
        };
        // Bilinear-exact: tan(πf/fs) = tan(πfc/fs)·tan(π/8).
        let notch = RATE / std::f32::consts::PI
            * ((std::f32::consts::PI * 1000.0 / RATE).tan() * (std::f32::consts::PI / 8.0).tan())
                .atan();
        assert!(level(1000.0, notch) < 0.02, "{}", level(1000.0, notch));
        assert!(level(4000.0, notch) > 0.5);
    }

    #[test]
    fn chorus_is_bounded_and_stereo() {
        let mut c = Chorus::new(4, 0.8, RATE);
        let x = sine(330.0, 48_000);
        let mut l = Vec::new();
        let mut r = Vec::new();
        for v in &x {
            let (a, b) = c.process_stereo(*v, *v);
            l.push(a);
            r.push(b);
        }
        assert!(l.iter().chain(&r).all(|v| v.abs() < 1.5));
        let diff: f64 = l
            .iter()
            .zip(&r)
            .map(|(a, b)| f64::from((a - b).abs()))
            .sum();
        assert!(diff > 100.0, "channels differ");
    }

    #[test]
    fn vibrato_wobbles_pitch() {
        let mut v = Vibrato::new(6.0, 2.0, RATE);
        let x = sine(1000.0, 32_768);
        let y: Vec<f32> = x.iter().map(|s| v.process(*s)).collect();
        let (m, bin) = spectrum(&y[8_192..8_192 + 16_384], RATE);
        // Sidebands appear at ±6 Hz multiples, spreading the 1 kHz line.
        let centre = level_near(&m, bin, 1000.0, 0);
        let spread = level_near(&m, bin, 1012.0, 1);
        assert!(spread > centre * 0.05, "{centre} {spread}");
    }

    #[test]
    fn ring_mod_makes_sum_and_difference() {
        let mut rm = RingMod::new(375.0, RATE);
        let x = sine(1500.0, 16_384);
        let y: Vec<f32> = x.iter().map(|v| rm.process(*v)).collect();
        let (m, bin) = spectrum(&y, RATE);
        assert!(level_near(&m, bin, 1125.0, 2) > 0.2);
        assert!(level_near(&m, bin, 1875.0, 2) > 0.2);
        assert!(level_near(&m, bin, 1500.0, 2) < 1e-3);
    }
}
