//! Distortion and colour: waveshaping, bit crushing, saturation, an exciter,
//! and the oversampler they share.
//!
//! A non-linearity applied to a sine creates harmonics; applied to a sampled
//! signal, the harmonics above Nyquist fold back as *aliases* — inharmonic
//! grit that is the difference between "warm" and "broken". The standard
//! cure is to run the non-linearity at 2×–8× the sample rate between a pair
//! of steep low-pass filters, and [`Oversampler`] is that pair: cascaded
//! half-band FIR stages, Kaiser-designed to 90 dB. Every shaper here takes an
//! oversampling factor, as Web Audio's `WaveShaperNode.oversample` does.

use super::fir::{kaiser_beta, lowpass, Fir};
use crate::dsp::Processor;

/// A 2× stage: interpolate up, then (after processing) filter and decimate.
#[derive(Debug, Clone)]
struct HalfBand {
    up: Fir,
    down: Fir,
}

impl HalfBand {
    fn new() -> Self {
        let h = lowpass(63, 0.24, kaiser_beta(90.0));
        Self {
            up: Fir::new(&h),
            down: Fir::new(&h),
        }
    }
}

/// An `N`× oversampler (1, 2, 4 or 8) around any per-sample function.
#[derive(Debug, Clone)]
pub struct Oversampler {
    stages: Vec<HalfBand>,
}

impl Oversampler {
    /// `factor` is rounded down to 1, 2, 4 or 8.
    #[must_use]
    pub fn new(factor: usize) -> Self {
        let n = match factor {
            0 | 1 => 0,
            2 | 3 => 1,
            4..=7 => 2,
            _ => 3,
        };
        Self {
            stages: (0..n).map(|_| HalfBand::new()).collect(),
        }
    }

    /// The oversampling factor.
    #[must_use]
    pub fn factor(&self) -> usize {
        1 << self.stages.len()
    }

    /// Added latency at the base rate, in samples.
    #[must_use]
    pub fn latency(&self) -> f32 {
        // Each stage's up and down filters add 31 samples at that stage's
        // rate; at the base rate a stage-k filter's delay is 31 / 2^(k+1)
        // for each of the two.
        let mut l = 0.0;
        for k in 0..self.stages.len() {
            #[expect(clippy::cast_precision_loss, reason = "small")]
            let r = (1u32 << (k + 1)) as f32;
            l += 2.0 * 31.0 / r;
        }
        l
    }

    /// Run `f` on `x` at the oversampled rate. Allocation-free: the
    /// oversampled block lives on the stack (at most 8 samples).
    pub fn run(&mut self, x: f32, f: &mut impl FnMut(f32) -> f32) -> f32 {
        if self.stages.is_empty() {
            return f(x);
        }
        let mut buf = [0.0f32; 8];
        buf[0] = x;
        let mut len = 1;
        for st in &mut self.stages {
            let mut up = [0.0f32; 8];
            for i in 0..len {
                up[2 * i] = st.up.process(buf[i] * 2.0);
                up[2 * i + 1] = st.up.process(0.0);
            }
            buf = up;
            len *= 2;
        }
        for v in &mut buf[..len] {
            *v = f(*v);
        }
        for st in self.stages.iter_mut().rev() {
            let mut down = [0.0f32; 8];
            for i in 0..len / 2 {
                down[i] = st.down.process(buf[2 * i]);
                let _ = st.down.process(buf[2 * i + 1]);
            }
            buf = down;
            len /= 2;
        }
        buf[0]
    }
}

/// A transfer curve.
#[derive(Debug, Clone, PartialEq)]
pub enum Curve {
    /// `tanh(x)` — smooth, symmetric, odd harmonics.
    Soft,
    /// Clip at ±1.
    Hard,
    /// Asymmetric tube-like curve: even and odd harmonics.
    Tube,
    /// Fold back past ±1 (wavefolder).
    Fold,
    /// A Chebyshev polynomial `T_n`: a full-scale sine in comes out as its
    /// `n`th harmonic, exactly.
    Chebyshev(u32),
    /// An arbitrary table over `-1..=1`, linearly interpolated — Web Audio's
    /// `WaveShaperNode.curve`.
    Table(Vec<f32>),
}

impl Curve {
    /// Apply the curve.
    #[must_use]
    pub fn apply(&self, x: f32) -> f32 {
        match self {
            Self::Soft => x.tanh(),
            Self::Hard => x.clamp(-1.0, 1.0),
            Self::Tube => {
                if x >= 0.0 {
                    1.0 - (-x).exp()
                } else {
                    -(1.0 - (x * 1.6).exp()) / 1.6
                }
            }
            Self::Fold => {
                // Triangle-fold into −1..1.
                let t = (x + 1.0).rem_euclid(4.0);
                if t < 2.0 {
                    t - 1.0
                } else {
                    3.0 - t
                }
            }
            Self::Chebyshev(n) => {
                let x = x.clamp(-1.0, 1.0);
                let (mut t0, mut t1) = (1.0f32, x);
                if *n == 0 {
                    return 1.0;
                }
                for _ in 1..*n {
                    let t2 = 2.0 * x * t1 - t0;
                    t0 = t1;
                    t1 = t2;
                }
                t1
            }
            Self::Table(t) => {
                if t.is_empty() {
                    return x;
                }
                #[expect(clippy::cast_precision_loss, reason = "table sizes")]
                let pos = (x.clamp(-1.0, 1.0) + 1.0) / 2.0 * (t.len() - 1) as f32;
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "clamped"
                )]
                let i = pos.floor() as usize;
                let j = (i + 1).min(t.len() - 1);
                let f = pos - pos.floor();
                t[i] + (t[j] - t[i]) * f
            }
        }
    }
}

/// A waveshaper: drive into a curve, oversampled, with output gain and mix.
#[derive(Debug, Clone)]
pub struct WaveShaper {
    /// The curve.
    pub curve: Curve,
    /// Input gain.
    pub drive: f32,
    /// Output gain.
    pub output: f32,
    /// Wet/dry 0..=1.
    pub mix: f32,
    os: Oversampler,
    dry_delay: super::filter::DelayLine,
}

impl WaveShaper {
    /// A shaper with `oversample` × (1, 2, 4, 8).
    #[must_use]
    pub fn new(curve: Curve, drive: f32, oversample: usize) -> Self {
        Self {
            curve,
            drive,
            output: 1.0,
            mix: 1.0,
            os: Oversampler::new(oversample),
            dry_delay: super::filter::DelayLine::new(64),
        }
    }

    /// Latency in samples.
    #[must_use]
    pub fn latency(&self) -> f32 {
        self.os.latency()
    }
}

impl Processor for WaveShaper {
    fn process(&mut self, x: f32) -> f32 {
        let drive = self.drive;
        let curve = &self.curve;
        let wet = self.os.run(x, &mut |v| curve.apply(v * drive)) * self.output;
        // Align the dry path with the oversampler's delay for the mix.
        self.dry_delay.push(x);
        let lat = self.os.latency();
        let dry = if lat > 0.0 {
            self.dry_delay.tap_frac(lat + 1.0)
        } else {
            x
        };
        dry + (wet - dry) * self.mix
    }
}

/// Bit depth and sample-rate reduction.
#[derive(Debug, Clone, PartialEq)]
pub struct Bitcrusher {
    /// Bits of resolution (1..=24; fractional allowed).
    pub bits: f32,
    /// Hold each sample this many samples (1 = no rate reduction).
    pub downsample: f32,
    counter: f32,
    held: f32,
}

impl Bitcrusher {
    /// A crusher.
    #[must_use]
    pub const fn new(bits: f32, downsample: f32) -> Self {
        Self {
            bits,
            downsample,
            counter: 0.0,
            held: 0.0,
        }
    }
}

impl Processor for Bitcrusher {
    fn process(&mut self, x: f32) -> f32 {
        if self.counter <= 0.0 {
            self.counter += self.downsample.max(1.0);
            let levels = 2f32.powf(self.bits.clamp(1.0, 24.0) - 1.0);
            self.held = (x * levels).round() / levels;
        }
        self.counter -= 1.0;
        self.held
    }
}

/// Tape/analog-style saturation: drive, bias (for even harmonics), a
/// pre-emphasis/de-emphasis pair so highs saturate less, and a mix.
#[derive(Debug, Clone)]
pub struct Saturation {
    /// Drive in dB.
    pub drive_db: f32,
    /// DC bias before the curve (0 = symmetric).
    pub bias: f32,
    /// Wet/dry.
    pub mix: f32,
    shaper: Oversampler,
    dc: super::filter::DcBlocker,
}

impl Saturation {
    /// A saturator at `rate`.
    #[must_use]
    pub fn new(drive_db: f32, bias: f32, rate: f32) -> Self {
        Self {
            drive_db,
            bias,
            mix: 1.0,
            shaper: Oversampler::new(2),
            dc: super::filter::DcBlocker::new(rate),
        }
    }
}

impl Processor for Saturation {
    fn process(&mut self, x: f32) -> f32 {
        let g = 10f32.powf(self.drive_db / 20.0);
        let bias = self.bias;
        let comp = 1.0 / g.tanh().max(1e-3);
        let wet = self
            .shaper
            .run(x, &mut |v| ((v * g + bias).tanh() - bias.tanh()) * comp);
        let wet = self.dc.process(wet);
        x + (wet - x) * self.mix
    }
}

/// An exciter: high-pass the input, saturate that band to generate new
/// upper harmonics, and blend it back — "air" and presence.
#[derive(Debug, Clone)]
pub struct Exciter {
    /// Amount of generated harmonics added.
    pub amount: f32,
    /// Drive into the harmonic generator.
    pub drive: f32,
    hp: super::filter::Svf,
    post: super::filter::Svf,
    os: Oversampler,
}

impl Exciter {
    /// An exciter acting above `freq` Hz.
    #[must_use]
    pub fn new(freq: f32, amount: f32, rate: f32) -> Self {
        use super::filter::{Svf, SvfMode};
        Self {
            amount,
            drive: 4.0,
            hp: Svf::new(SvfMode::HighPass, freq, 0.707, rate),
            post: Svf::new(SvfMode::HighPass, freq, 0.707, rate),
            os: Oversampler::new(2),
        }
    }
}

impl Processor for Exciter {
    fn process(&mut self, x: f32) -> f32 {
        let band = self.hp.process(x);
        let d = self.drive;
        let harm = self.os.run(band, &mut |v| (v * d).tanh() / d.tanh());
        // Keep only the new upper content (and the band) out of the shaper.
        let harm = self.post.process(harm - band);
        x + harm * self.amount
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::testing::{level_near, spectrum};

    const RATE: f32 = 48_000.0;

    fn sine(hz: f32, amp: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| {
                #[expect(clippy::cast_precision_loss, reason = "test")]
                let t = i as f32 / RATE;
                amp * (2.0 * std::f32::consts::PI * hz * t).sin()
            })
            .collect()
    }

    #[test]
    fn chebyshev_makes_exactly_its_harmonic() {
        let x = sine(375.0, 1.0, 16_384);
        let y: Vec<f32> = x.iter().map(|v| Curve::Chebyshev(3).apply(*v)).collect();
        let (m, bin) = spectrum(&y, RATE);
        assert!(level_near(&m, bin, 1125.0, 2) > 0.45);
        assert!(level_near(&m, bin, 375.0, 2) < 1e-3);
        assert!(level_near(&m, bin, 750.0, 2) < 1e-3);
    }

    #[test]
    fn symmetric_curves_make_odd_harmonics_tube_makes_even() {
        let x = sine(375.0, 0.9, 16_384);
        let h2 = |c: Curve| {
            let y: Vec<f32> = x.iter().map(|v| c.apply(*v * 2.0)).collect();
            let (m, bin) = spectrum(&y, RATE);
            level_near(&m, bin, 750.0, 2) / level_near(&m, bin, 375.0, 2)
        };
        assert!(h2(Curve::Soft) < 1e-3);
        assert!(h2(Curve::Tube) > 0.02);
    }

    #[test]
    fn oversampling_cuts_aliasing_of_a_hard_clip() {
        // 5 kHz clipped: harmonics at 15, 25 (→23), 35 (→13) kHz… count
        // energy that is not on the 5 kHz grid.
        let x = sine(5_000.0, 3.0, 32_768);
        let off_grid = |os: usize| {
            let mut w = WaveShaper::new(Curve::Hard, 1.0, os);
            let y: Vec<f32> = x.iter().map(|v| w.process(*v)).collect();
            let (m, bin) = spectrum(&y[4096..4096 + 16_384], RATE);
            let mut e = 0.0;
            for (k, v) in m.iter().enumerate() {
                #[expect(clippy::cast_precision_loss, reason = "test")]
                let hz = k as f64 * bin;
                let near = (hz / 5000.0).round() * 5000.0;
                if (hz - near).abs() > 50.0 && hz < 20_000.0 {
                    e += v * v;
                }
            }
            e
        };
        let none = off_grid(1);
        let eight = off_grid(8);
        assert!(eight < none * 0.01, "{none} -> {eight}");
    }

    #[test]
    fn table_curve_interpolates() {
        let c = Curve::Table(vec![-1.0, 0.0, 1.0]);
        assert!((c.apply(0.5) - 0.5).abs() < 1e-6);
        let c = Curve::Table(vec![1.0, 0.0, 1.0]);
        assert!((c.apply(-0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn fold_stays_in_range() {
        for i in -100..100 {
            #[expect(clippy::cast_precision_loss, reason = "test")]
            let v = Curve::Fold.apply(i as f32 * 0.1);
            assert!((-1.0..=1.0).contains(&v));
        }
        assert!((Curve::Fold.apply(1.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn bitcrusher_quantises_and_holds() {
        let mut b = Bitcrusher::new(3.0, 4.0);
        let x: Vec<f32> = (0..64).map(|i| (i as f32 / 32.0) - 1.0).collect();
        let y: Vec<f32> = x.iter().map(|v| b.process(*v)).collect();
        let mut levels: Vec<i32> = y.iter().map(|v| (v * 4.0).round() as i32).collect();
        levels.dedup();
        assert!(levels.len() <= 9);
        assert!(y.chunks(4).all(|c| c.iter().all(|v| *v == c[0])));
    }

    #[test]
    fn saturation_adds_harmonics_and_keeps_level() {
        let mut s = Saturation::new(12.0, 0.2, RATE);
        let x = sine(375.0, 0.5, 32_768);
        let y: Vec<f32> = x.iter().map(|v| s.process(*v)).collect();
        let (m, bin) = spectrum(&y[8192..8192 + 16_384], RATE);
        let f = level_near(&m, bin, 375.0, 2);
        assert!(level_near(&m, bin, 750.0, 2) > f * 0.01, "even from bias");
        assert!(level_near(&m, bin, 1125.0, 2) > f * 0.01, "odd from tanh");
        assert!(f > 0.2 && f < 0.6, "{f}");
    }

    #[test]
    fn exciter_adds_upper_harmonics_only_above_its_band() {
        let mut e = Exciter::new(2000.0, 1.0, RATE);
        let x = sine(3000.0, 0.5, 32_768);
        let y: Vec<f32> = x.iter().map(|v| e.process(*v)).collect();
        let (m, bin) = spectrum(&y[8192..8192 + 16_384], RATE);
        assert!(level_near(&m, bin, 9000.0, 3) > 1e-3);
        let mut e = Exciter::new(2000.0, 1.0, RATE);
        let lo = sine(200.0, 0.5, 32_768);
        let y: Vec<f32> = lo.iter().map(|v| e.process(*v)).collect();
        let (m, bin) = spectrum(&y[8192..8192 + 16_384], RATE);
        assert!(level_near(&m, bin, 600.0, 3) < 1e-3, "lows untouched");
    }
}
