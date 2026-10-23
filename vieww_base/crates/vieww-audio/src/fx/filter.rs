//! Filters built to be *modulated*: the state-variable filter, the Moog-style
//! ladder, one-poles, all-passes, combs and a DC blocker.
//!
//! [`Biquad`](crate::dsp::Biquad) is the right filter for a fixed EQ band —
//! it is exact to the RBJ cookbook — but its coefficients are expensive to
//! recompute and it misbehaves when they change every sample, which is what
//! a filter sweeping under an envelope does. The filters here are
//! *topology-preserving* (trapezoidal-integrated, after Zavalishin and
//! Simper): the cutoff can move at audio rate without zipper noise or
//! blow-ups, which is the whole job of a synth filter.

use std::f32::consts::PI;

use crate::dsp::Processor;

/// Which output of the [`Svf`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SvfMode {
    /// 12 dB/oct low-pass.
    #[default]
    LowPass,
    /// 12 dB/oct high-pass.
    HighPass,
    /// Band-pass (constant skirt).
    BandPass,
    /// Notch.
    Notch,
    /// Peak (low minus high).
    Peak,
    /// All-pass.
    AllPass,
}

/// Simper's trapezoidal state-variable filter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Svf {
    mode: SvfMode,
    rate: f32,
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    ic1: f32,
    ic2: f32,
}

impl Svf {
    /// A filter at `cutoff` Hz with resonance `q` (0.707 = Butterworth).
    #[must_use]
    pub fn new(mode: SvfMode, cutoff: f32, q: f32, rate: f32) -> Self {
        let mut f = Self {
            mode,
            rate,
            k: 1.0,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
            ic1: 0.0,
            ic2: 0.0,
        };
        f.set(cutoff, q);
        f
    }

    /// Retune — cheap enough to call every sample.
    pub fn set(&mut self, cutoff: f32, q: f32) {
        let fc = cutoff.clamp(1.0, self.rate * 0.49);
        let g = (PI * fc / self.rate).tan();
        self.k = 1.0 / q.max(0.05);
        self.a1 = 1.0 / (1.0 + g * (g + self.k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    /// Change the output.
    pub const fn set_mode(&mut self, mode: SvfMode) {
        self.mode = mode;
    }

    /// Clear state.
    pub const fn reset(&mut self) {
        self.ic1 = 0.0;
        self.ic2 = 0.0;
    }

    /// All outputs at once: (low, band, high).
    pub fn tick(&mut self, v0: f32) -> (f32, f32, f32) {
        let v3 = v0 - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        let high = v0 - self.k * v1 - v2;
        (v2, v1, high)
    }
}

impl Processor for Svf {
    fn process(&mut self, x: f32) -> f32 {
        let (low, band, high) = self.tick(x);
        match self.mode {
            SvfMode::LowPass => low,
            SvfMode::HighPass => high,
            SvfMode::BandPass => band,
            SvfMode::Notch => low + high,
            SvfMode::Peak => low - high,
            SvfMode::AllPass => low + high - self.k * band,
        }
    }
}

/// A four-pole transistor-ladder low-pass with resonance — the Moog sound.
///
/// Huovilainen's non-linear model (DAFx 2004), structured as his reference
/// code: four one-pole stages each with a `tanh` at its input, a half-sample
/// delay on the feedback path for phase compensation, his polynomial
/// corrections for cutoff and resonance, and 2× oversampling so the
/// saturation's harmonics do not alias. Resonance 0..=1; at 1 and above it
/// self-oscillates, and the `tanh` stages are what hold that oscillation at
/// a finite amplitude instead of letting it grow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ladder {
    rate: f32,
    tune: f32,
    acr: f32,
    resonance: f32,
    res_quad: f32,
    stage: [f32; 4],
    stage_tanh: [f32; 3],
    delay: [f32; 6],
}

impl Ladder {
    /// The thermal-voltage scale: input level at which the stages saturate.
    const THERMAL: f32 = 0.5;

    /// A ladder at `cutoff` Hz.
    #[must_use]
    pub fn new(cutoff: f32, resonance: f32, rate: f32) -> Self {
        let mut l = Self {
            rate,
            tune: 0.0,
            acr: 1.0,
            resonance: 0.0,
            res_quad: 0.0,
            stage: [0.0; 4],
            stage_tanh: [0.0; 3],
            delay: [0.0; 6],
        };
        l.set(cutoff, resonance);
        l
    }

    /// Retune.
    pub fn set(&mut self, cutoff: f32, resonance: f32) {
        let fc = cutoff.clamp(5.0, self.rate * 0.45) / self.rate;
        let f = fc * 0.5; // the oversampled rate
        let fc2 = fc * fc;
        let fc3 = fc2 * fc;
        let fcr = 1.8730 * fc3 + 0.4955 * fc2 - 0.6490 * fc + 0.9988;
        self.acr = -3.9364 * fc2 + 1.8409 * fc + 0.9968;
        self.tune = (1.0 - (-(2.0 * PI) * f * fcr).exp()) / Self::THERMAL;
        self.resonance = resonance.max(0.0);
        self.res_quad = 4.0 * self.resonance * self.acr;
    }

    fn step(&mut self, x: f32) {
        let th = Self::THERMAL;
        let input = x - self.res_quad * self.delay[5];
        self.stage[0] = self.delay[0] + self.tune * ((input * th).tanh() - self.stage_tanh[0]);
        self.delay[0] = self.stage[0];
        for k in 1..4 {
            let inp = self.stage[k - 1];
            self.stage_tanh[k - 1] = (inp * th).tanh();
            let other = if k == 3 {
                (self.delay[k] * th).tanh()
            } else {
                self.stage_tanh[k]
            };
            self.stage[k] = self.delay[k] + self.tune * (self.stage_tanh[k - 1] - other);
            self.delay[k] = self.stage[k];
        }
        self.delay[5] = (self.stage[3] + self.delay[4]) * 0.5;
        self.delay[4] = self.stage[3];
    }
}

impl Processor for Ladder {
    fn process(&mut self, x: f32) -> f32 {
        self.step(x);
        self.step(x);
        self.delay[5]
    }
}

/// A one-pole low-pass (6 dB/oct) — smoothing, tone controls, damping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OnePole {
    a: f32,
    z: f32,
}

impl OnePole {
    /// Clear state.
    pub const fn clear(&mut self) {
        self.z = 0.0;
    }

    /// A smoother with corner `cutoff` Hz.
    #[must_use]
    pub fn new(cutoff: f32, rate: f32) -> Self {
        Self {
            a: (-2.0 * PI * cutoff.max(0.0) / rate).exp(),
            z: 0.0,
        }
    }

    /// A smoother with time constant `seconds` (63% settle).
    #[must_use]
    pub fn from_time(seconds: f32, rate: f32) -> Self {
        Self {
            a: if seconds <= 0.0 {
                0.0
            } else {
                (-1.0 / (seconds * rate)).exp()
            },
            z: 0.0,
        }
    }

    /// Move the corner without touching the state.
    pub fn set_cutoff(&mut self, cutoff: f32, rate: f32) {
        self.a = (-2.0 * PI * cutoff.max(0.0) / rate).exp();
    }

    /// Jump to `v`.
    pub const fn reset(&mut self, v: f32) {
        self.z = v;
    }

    /// The current output.
    #[must_use]
    pub const fn value(&self) -> f32 {
        self.z
    }
}

impl Processor for OnePole {
    fn process(&mut self, x: f32) -> f32 {
        self.z = x + self.a * (self.z - x);
        self.z
    }
}

/// A first-order all-pass `H(z) = (a + z⁻¹)/(1 + a z⁻¹)` — the phaser stage.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Allpass1 {
    /// The coefficient.
    pub a: f32,
    z: f32,
}

impl Allpass1 {
    /// Coefficient for a 90° phase shift at `hz`.
    #[must_use]
    pub fn coefficient(hz: f32, rate: f32) -> f32 {
        let t = (PI * hz.clamp(1.0, rate * 0.49) / rate).tan();
        (t - 1.0) / (t + 1.0)
    }
}

impl Processor for Allpass1 {
    fn process(&mut self, x: f32) -> f32 {
        let y = self.a * x + self.z;
        self.z = x - self.a * y;
        y
    }
}

/// Removes DC: a one-zero, one-pole high-pass at ~10 Hz.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DcBlocker {
    r: f32,
    x1: f32,
    y1: f32,
}

impl DcBlocker {
    /// A blocker at `rate`.
    #[must_use]
    pub fn new(rate: f32) -> Self {
        Self {
            r: 1.0 - 2.0 * PI * 10.0 / rate,
            x1: 0.0,
            y1: 0.0,
        }
    }
}

impl Processor for DcBlocker {
    fn process(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + self.r * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

/// A delay line with fractional (cubic-interpolated) reads.
#[derive(Debug, Clone, PartialEq)]
pub struct DelayLine {
    buf: Vec<f32>,
    write: usize,
}

impl DelayLine {
    /// Clear the line to silence, keeping its length.
    pub fn clear(&mut self) {
        self.buf.fill(0.0);
    }

    /// A line holding up to `max` samples.
    #[must_use]
    pub fn new(max: usize) -> Self {
        Self {
            buf: vec![0.0; max.max(4) + 4],
            write: 0,
        }
    }

    /// Push a sample.
    pub fn push(&mut self, x: f32) {
        self.buf[self.write] = x;
        self.write = (self.write + 1) % self.buf.len();
    }

    /// The sample written `d` pushes ago (`d ≥ 1`), integer.
    #[must_use]
    pub fn tap(&self, d: usize) -> f32 {
        let n = self.buf.len();
        self.buf[(self.write + n - d.clamp(1, n - 1)) % n]
    }

    /// A fractional tap, cubic (Hermite) interpolated, `d ≥ 1`.
    #[must_use]
    pub fn tap_frac(&self, d: f32) -> f32 {
        #[expect(clippy::cast_precision_loss, reason = "buffer lengths")]
        let d = d.clamp(1.0, (self.buf.len() - 3) as f32);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped"
        )]
        let i = d.floor() as usize;
        let f = d - d.floor();
        let y0 = self.tap(i.saturating_sub(1).max(1));
        let y1 = self.tap(i);
        let y2 = self.tap(i + 1);
        let y3 = self.tap(i + 2);
        // Hermite between y1 (at d=i) and y2 (at d=i+1).
        let c0 = y1;
        let c1 = 0.5 * (y2 - y0);
        let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
        let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
        ((c3 * f + c2) * f + c1) * f + c0
    }

    /// Capacity in samples.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.buf.len() - 4
    }
}

/// A feedback comb with damping in the loop — the Freeverb building block.
#[derive(Debug, Clone, PartialEq)]
pub struct Comb {
    line: DelayLine,
    len: usize,
    /// Loop gain.
    pub feedback: f32,
    /// Loop low-pass amount 0..1.
    pub damp: f32,
    store: f32,
}

impl Comb {
    /// Clear state.
    pub fn clear(&mut self) {
        self.line.clear();
        self.store = 0.0;
    }

    /// A comb of `len` samples.
    #[must_use]
    pub fn new(len: usize, feedback: f32, damp: f32) -> Self {
        Self {
            line: DelayLine::new(len),
            len,
            feedback,
            damp,
            store: 0.0,
        }
    }
}

impl Processor for Comb {
    fn process(&mut self, x: f32) -> f32 {
        let out = self.line.tap(self.len);
        self.store = out * (1.0 - self.damp) + self.store * self.damp;
        self.line.push(x + self.store * self.feedback);
        out
    }
}

/// A Schroeder all-pass diffuser of `len` samples.
#[derive(Debug, Clone, PartialEq)]
pub struct SchroederAllpass {
    line: DelayLine,
    len: usize,
    /// Gain (0.5 in Freeverb).
    pub gain: f32,
}

impl SchroederAllpass {
    /// Clear state.
    pub fn clear(&mut self) {
        self.line.clear();
    }

    /// A diffuser.
    #[must_use]
    pub fn new(len: usize, gain: f32) -> Self {
        Self {
            line: DelayLine::new(len),
            len,
            gain,
        }
    }
}

impl Processor for SchroederAllpass {
    fn process(&mut self, x: f32) -> f32 {
        let delayed = self.line.tap(self.len);
        let v = x + self.gain * delayed;
        self.line.push(v);
        delayed - self.gain * v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn gain_at(p: &mut impl Processor, hz: f32) -> f32 {
        let n = 48_000;
        let mut peak = 0.0f32;
        for i in 0..n {
            #[expect(clippy::cast_precision_loss, reason = "test")]
            let x = (2.0 * PI * hz * i as f32 / RATE).sin();
            let y = p.process(x);
            if i > n / 2 {
                peak = peak.max(y.abs());
            }
        }
        peak
    }

    fn db(x: f32) -> f32 {
        20.0 * x.log10()
    }

    #[test]
    fn svf_lowpass_is_minus_3db_at_cutoff_and_12db_per_octave() {
        let mk = || {
            Svf::new(
                SvfMode::LowPass,
                1000.0,
                std::f32::consts::FRAC_1_SQRT_2,
                RATE,
            )
        };
        assert!((db(gain_at(&mut mk(), 1000.0)) + 3.0).abs() < 0.3);
        assert!(db(gain_at(&mut mk(), 100.0)).abs() < 0.1);
        // An octave pair well below Nyquist: the bilinear transform's zero at
        // Nyquist legitimately steepens the slope above ~fs/8.
        let a = db(gain_at(&mut mk(), 2000.0));
        let b = db(gain_at(&mut mk(), 4000.0));
        assert!((a - b - 12.0).abs() < 1.5, "{a} {b}");
    }

    #[test]
    fn svf_modes_behave() {
        let hp = gain_at(&mut Svf::new(SvfMode::HighPass, 1000.0, 0.707, RATE), 100.0);
        assert!(db(hp) < -35.0);
        let bp = gain_at(
            &mut Svf::new(SvfMode::BandPass, 1000.0, 0.707, RATE),
            1000.0,
        );
        // Band output peaks at 1/k = q at the centre.
        assert!((bp - 0.707).abs() < 0.03, "{bp}");
        let notch = gain_at(&mut Svf::new(SvfMode::Notch, 1000.0, 0.707, RATE), 1000.0);
        assert!(db(notch) < -30.0, "{notch}");
        for hz in [100.0, 1000.0, 10_000.0] {
            let ap = gain_at(&mut Svf::new(SvfMode::AllPass, 1000.0, 0.707, RATE), hz);
            assert!((ap - 1.0).abs() < 0.02, "allpass {hz}: {ap}");
        }
    }

    #[test]
    fn svf_survives_audio_rate_cutoff_modulation() {
        let mut f = Svf::new(SvfMode::LowPass, 1000.0, 4.0, RATE);
        for i in 0..48_000 {
            #[expect(clippy::cast_precision_loss, reason = "test")]
            let t = i as f32 / RATE;
            f.set(2000.0 + 1900.0 * (2.0 * PI * 300.0 * t).sin(), 4.0);
            let y = f.process((2.0 * PI * 110.0 * t).sin());
            assert!(y.is_finite() && y.abs() < 20.0);
        }
    }

    #[test]
    fn ladder_is_24db_per_octave_and_resonates() {
        let mk = |r: f32| Ladder::new(1000.0, r, RATE);
        let pass = db(gain_at(&mut mk(0.0), 100.0));
        let a = db(gain_at(&mut mk(0.0), 2000.0));
        let b = db(gain_at(&mut mk(0.0), 4000.0));
        assert!(pass.abs() < 2.0, "passband {pass}");
        assert!((a - b) > 18.0, "slope {a} {b}");
        // Small-signal: at full scale the tanh stages (correctly) compress
        // the resonant peak.
        let small = |r: f32| {
            let mut l = mk(r);
            let mut peak = 0.0f32;
            for i in 0..48_000 {
                #[expect(clippy::cast_precision_loss, reason = "test")]
                let x = 0.05 * (2.0 * PI * 1000.0 * i as f32 / RATE).sin();
                let y = l.process(x);
                if i > 24_000 {
                    peak = peak.max(y.abs());
                }
            }
            peak / 0.05
        };
        let (flat, res) = (small(0.0), small(0.85));
        assert!(
            (db(flat) + 12.0).abs() < 2.0,
            "four poles: -12 dB at cutoff, got {}",
            db(flat)
        );
        assert!(res > flat * 4.0, "resonance peak {flat} -> {res}");
    }

    #[test]
    fn ladder_self_oscillates_at_full_resonance() {
        let mut l = Ladder::new(800.0, 1.1, RATE);
        let mut last = 0.0;
        // A single impulse, then silence.
        for i in 0..48_000 {
            last = l.process(if i == 0 { 1.0 } else { 0.0 });
        }
        let mut peak = 0.0f32;
        for _ in 0..4800 {
            peak = peak.max(l.process(0.0).abs());
        }
        assert!(peak > 0.05, "rings forever: {peak} (last {last})");
    }

    #[test]
    fn one_pole_settles() {
        let mut p = OnePole::from_time(0.01, RATE);
        let mut y = 0.0;
        for _ in 0..480 {
            y = p.process(1.0);
        }
        assert!((y - 0.632).abs() < 0.01, "{y}");
    }

    #[test]
    fn allpass1_is_unity_gain_with_quarter_turn_at_its_frequency() {
        let a = Allpass1::coefficient(1000.0, RATE);
        for hz in [50.0, 1000.0, 15_000.0] {
            let g = gain_at(&mut Allpass1 { a, z: 0.0 }, hz);
            assert!((g - 1.0).abs() < 0.01, "{hz}: {g}");
        }
    }

    #[test]
    fn dc_blocker_removes_offset() {
        let mut d = DcBlocker::new(RATE);
        let mut y = 1.0;
        for _ in 0..48_000 {
            y = d.process(0.5);
        }
        assert!(y.abs() < 1e-3);
    }

    #[test]
    fn fractional_delay_interpolates_a_ramp_exactly() {
        let mut d = DelayLine::new(64);
        for i in 0..64 {
            #[expect(clippy::cast_precision_loss, reason = "test")]
            d.push(i as f32);
        }
        // Last pushed is 63 at d=1; d=10.5 → 63 − 9.5 = 53.5.
        assert!((d.tap_frac(10.5) - 53.5).abs() < 1e-4);
        assert!((d.tap(1) - 63.0).abs() < 1e-6);
    }
}
