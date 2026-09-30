//! Audio DSP — the processing blocks Web Audio, TouchDesigner's audio
//! CHOPs, openFrameworks' `ofSoundStream` + addons and Processing's
//! `minim`/`sound` libraries give a creative-coding patch (§2.10–§2.12).
//!
//! - [`Biquad`]: the RBJ cookbook filters (low/high/band pass, notch,
//!   peaking, shelves, all-pass) — Web Audio's `BiquadFilterNode`.
//! - [`Delay`]: a feedback delay line with a wet/dry mix.
//! - [`Compressor`]: feed-forward, with threshold, ratio, knee, attack,
//!   release and makeup — `DynamicsCompressorNode`.
//! - [`EnvelopeFollower`]: attack/release peak detector — the "make it
//!   pulse to the kick" signal.
//! - [`OnsetDetector`]: spectral-flux onsets with an adaptive threshold,
//!   and [`estimate_tempo`] from onset autocorrelation — beat detection.
//! - [`Processor`] chains them over a [`Samples`] buffer.

use std::f32::consts::PI;

use crate::analysis::Spectrum;
use crate::Samples;

/// An audio-rate processor.
pub trait Processor {
    fn process(&mut self, x: f32) -> f32;

    /// Process a whole buffer in place (per channel state is the caller's —
    /// run one processor per channel).
    fn run(&mut self, buf: &mut [f32]) {
        for s in buf {
            *s = self.process(*s);
        }
    }
}

/// RBJ biquad types.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilterKind {
    LowPass,
    HighPass,
    BandPass,
    Notch,
    AllPass,
    /// Gain in dB.
    Peaking(f32),
    LowShelf(f32),
    HighShelf(f32),
}

/// A second-order IIR filter (transposed direct form II).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    /// Design a filter at `freq` Hz, quality `q`, for `rate` Hz audio.
    #[must_use]
    pub fn new(kind: FilterKind, freq: f32, q: f32, rate: f32) -> Self {
        let w = 2.0 * PI * freq / rate;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q.max(1e-4));
        let (b0, b1, b2, a0, a1, a2) = match kind {
            FilterKind::LowPass => ((1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha),
            FilterKind::HighPass => ((1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha),
            FilterKind::BandPass => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * c, 1.0 - alpha),
            FilterKind::Notch => (1.0, -2.0 * c, 1.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha),
            FilterKind::AllPass => (1.0 - alpha, -2.0 * c, 1.0 + alpha, 1.0 + alpha, -2.0 * c, 1.0 - alpha),
            FilterKind::Peaking(db) => {
                let a = 10f32.powf(db / 40.0);
                (1.0 + alpha * a, -2.0 * c, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * c, 1.0 - alpha / a)
            }
            FilterKind::LowShelf(db) => {
                let a = 10f32.powf(db / 40.0);
                let k = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * c + k),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * c),
                    a * ((a + 1.0) - (a - 1.0) * c - k),
                    (a + 1.0) + (a - 1.0) * c + k,
                    -2.0 * ((a - 1.0) + (a + 1.0) * c),
                    (a + 1.0) + (a - 1.0) * c - k,
                )
            }
            FilterKind::HighShelf(db) => {
                let a = 10f32.powf(db / 40.0);
                let k = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * c + k),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
                    a * ((a + 1.0) + (a - 1.0) * c - k),
                    (a + 1.0) - (a - 1.0) * c + k,
                    2.0 * ((a - 1.0) - (a + 1.0) * c),
                    (a + 1.0) - (a - 1.0) * c - k,
                )
            }
        };
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// Magnitude response at `freq` (linear gain).
    #[must_use]
    pub fn magnitude(&self, freq: f32, rate: f32) -> f32 {
        let w = 2.0 * PI * freq / rate;
        let (c1, s1, c2, s2) = (w.cos(), w.sin(), (2.0 * w).cos(), (2.0 * w).sin());
        let nr = self.b0 + self.b1 * c1 + self.b2 * c2;
        let ni = -(self.b1 * s1 + self.b2 * s2);
        let dr = 1.0 + self.a1 * c1 + self.a2 * c2;
        let di = -(self.a1 * s1 + self.a2 * s2);
        ((nr * nr + ni * ni) / (dr * dr + di * di)).sqrt()
    }
}

impl Processor for Biquad {
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// A feedback delay (echo).
#[derive(Debug, Clone, PartialEq)]
pub struct Delay {
    buf: Vec<f32>,
    pos: usize,
    pub feedback: f32,
    /// 0 = dry only, 1 = wet only.
    pub mix: f32,
}

impl Delay {
    #[must_use]
    pub fn new(seconds: f32, rate: f32, feedback: f32, mix: f32) -> Self {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = ((seconds * rate).round() as usize).max(1);
        Self {
            buf: vec![0.0; n],
            pos: 0,
            feedback,
            mix,
        }
    }
}

impl Processor for Delay {
    fn process(&mut self, x: f32) -> f32 {
        let d = self.buf[self.pos];
        self.buf[self.pos] = x + d * self.feedback;
        self.pos = (self.pos + 1) % self.buf.len();
        x * (1.0 - self.mix) + d * self.mix
    }
}

/// One-pole attack/release smoothing coefficient for a time constant.
fn coeff(seconds: f32, rate: f32) -> f32 {
    if seconds <= 0.0 {
        0.0
    } else {
        (-1.0 / (seconds * rate)).exp()
    }
}

/// Peak envelope follower.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnvelopeFollower {
    attack: f32,
    release: f32,
    pub level: f32,
}

impl EnvelopeFollower {
    #[must_use]
    pub fn new(attack: f32, release: f32, rate: f32) -> Self {
        Self {
            attack: coeff(attack, rate),
            release: coeff(release, rate),
            level: 0.0,
        }
    }
}

impl Processor for EnvelopeFollower {
    fn process(&mut self, x: f32) -> f32 {
        let a = x.abs();
        let k = if a > self.level { self.attack } else { self.release };
        self.level = a + k * (self.level - a);
        self.level
    }
}

/// A feed-forward compressor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Compressor {
    pub threshold_db: f32,
    pub ratio: f32,
    pub knee_db: f32,
    pub makeup_db: f32,
    attack: f32,
    release: f32,
    env_db: f32,
    /// Current gain reduction (dB, ≤ 0), for a meter.
    pub reduction_db: f32,
}

impl Compressor {
    #[must_use]
    pub fn new(threshold_db: f32, ratio: f32, attack: f32, release: f32, rate: f32) -> Self {
        Self {
            threshold_db,
            ratio,
            knee_db: 6.0,
            makeup_db: 0.0,
            attack: coeff(attack, rate),
            release: coeff(release, rate),
            env_db: -120.0,
            reduction_db: 0.0,
        }
    }

    /// Static curve: input dB → output dB (before makeup).
    #[must_use]
    pub fn curve(&self, x: f32) -> f32 {
        let (t, r, w) = (self.threshold_db, self.ratio.max(1.0), self.knee_db);
        if 2.0 * (x - t) < -w {
            x
        } else if 2.0 * (x - t).abs() <= w && w > 0.0 {
            x + (1.0 / r - 1.0) * (x - t + w / 2.0).powi(2) / (2.0 * w)
        } else {
            t + (x - t) / r
        }
    }
}

impl Processor for Compressor {
    fn process(&mut self, x: f32) -> f32 {
        let db = 20.0 * x.abs().max(1e-6).log10();
        let target = self.curve(db) - db;
        let k = if target < self.reduction_db { self.attack } else { self.release };
        self.reduction_db = target + k * (self.reduction_db - target);
        self.env_db = db;
        x * 10f32.powf((self.reduction_db + self.makeup_db) / 20.0)
    }
}

/// A chain of processors.
#[derive(Default)]
pub struct Chain(pub Vec<Box<dyn Processor>>);

impl std::fmt::Debug for Chain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Chain({} stages)", self.0.len())
    }
}

impl Chain {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn then(mut self, p: impl Processor + 'static) -> Self {
        self.0.push(Box::new(p));
        self
    }
}

impl Processor for Chain {
    fn process(&mut self, x: f32) -> f32 {
        self.0.iter_mut().fold(x, |v, p| p.process(v))
    }
}

/// Run a (mono) processor over a buffer, returning a new buffer. For
/// stereo, each channel goes through its own clone of `make()`.
pub fn apply<P: Processor>(samples: &Samples, mut make: impl FnMut() -> P) -> Samples {
    let ch = usize::from(samples.channels.max(1));
    let mut procs: Vec<P> = (0..ch).map(|_| make()).collect();
    let data = samples.data.iter().enumerate().map(|(i, &x)| procs[i % ch].process(x)).collect();
    Samples {
        data,
        channels: samples.channels,
        rate: samples.rate,
    }
}

/// Spectral-flux onset detection.
#[derive(Debug, Clone, PartialEq)]
pub struct OnsetDetector {
    pub frame: usize,
    pub hop: usize,
    /// Threshold = median of the last `window` flux values × `multiplier` +
    /// `delta`.
    pub window: usize,
    pub multiplier: f32,
    pub delta: f32,
}

impl Default for OnsetDetector {
    fn default() -> Self {
        Self {
            frame: 1024,
            hop: 512,
            window: 8,
            multiplier: 1.5,
            delta: 0.01,
        }
    }
}

impl OnsetDetector {
    /// Flux per hop (half-wave rectified magnitude increase).
    #[must_use]
    pub fn flux(&self, mono: &[f32], rate: u32) -> Vec<f32> {
        let mut prev: Option<Vec<f32>> = None;
        let mut out = Vec::new();
        let mut i = 0;
        while i + self.frame <= mono.len() {
            let spec = Spectrum::analyze(&Samples::mono(mono[i..i + self.frame].to_vec(), rate), crate::analysis::Window::Hann);
            let mags: Vec<f32> = spec.magnitudes().to_vec();
            let f = prev.as_ref().map_or(0.0, |p| mags.iter().zip(p).map(|(m, q)| (m - q).max(0.0)).sum());
            out.push(f);
            prev = Some(mags);
            i += self.hop;
        }
        out
    }

    /// Onset times in seconds.
    #[must_use]
    pub fn detect(&self, samples: &Samples) -> Vec<f32> {
        let ch = usize::from(samples.channels.max(1));
        #[allow(clippy::cast_precision_loss)]
        let mono: Vec<f32> = samples.data.chunks(ch).map(|c| c.iter().sum::<f32>() / ch as f32).collect();
        let flux = self.flux(&mono, samples.rate);
        let peak = flux.iter().copied().fold(0.0f32, f32::max).max(1e-9);
        let flux: Vec<f32> = flux.iter().map(|f| f / peak).collect();
        let mut out = Vec::new();
        for i in 1..flux.len().saturating_sub(1) {
            let lo = i.saturating_sub(self.window);
            let mut w: Vec<f32> = flux[lo..=i].to_vec();
            w.sort_by(f32::total_cmp);
            let thresh = w[w.len() / 2] * self.multiplier + self.delta;
            if flux[i] > thresh && flux[i] >= flux[i - 1] && flux[i] > flux[i + 1] {
                #[allow(clippy::cast_precision_loss)]
                out.push((i * self.hop + self.frame / 2) as f32 / samples.rate as f32);
            }
        }
        out
    }
}

/// Estimate tempo (bpm, in `min..max`) from onset times, by scoring each
/// candidate period by how many onset intervals land on its multiples.
#[must_use]
pub fn estimate_tempo(onsets: &[f32], min_bpm: f32, max_bpm: f32) -> Option<f32> {
    if onsets.len() < 3 {
        return None;
    }
    let mut best = (0.0f32, None);
    let mut bpm = min_bpm;
    while bpm <= max_bpm {
        let period = 60.0 / bpm;
        let mut score = 0.0;
        for (i, a) in onsets.iter().enumerate() {
            for b in &onsets[i + 1..] {
                let k = (b - a) / period;
                let err = (k - k.round()).abs();
                if k.round() >= 1.0 && k.round() <= 4.0 {
                    score += (1.0 - err * 4.0).max(0.0) / k.round();
                }
            }
        }
        if score > best.0 + 1e-6 {
            best = (score, Some(bpm));
        }
        bpm += 0.5;
    }
    best.1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| (2.0 * PI * freq * i as f32 / rate).sin()).collect()
    }

    fn rms(v: &[f32]) -> f32 {
        (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt()
    }

    #[test]
    fn low_pass_passes_lows_and_cuts_highs() {
        let rate = 48_000.0;
        let f = Biquad::new(FilterKind::LowPass, 1000.0, 0.707, rate);
        assert!((f.magnitude(100.0, rate) - 1.0).abs() < 0.01);
        assert!((f.magnitude(1000.0, rate) - 0.707).abs() < 0.01);
        assert!(f.magnitude(10_000.0, rate) < 0.02);
        let mut lp = f;
        let mut hi = sine(8000.0, rate, 4800);
        lp.run(&mut hi);
        assert!(rms(&hi[2400..]) < 0.03);
    }

    #[test]
    fn peaking_and_shelves_hit_their_gain() {
        let rate = 48_000.0;
        let pk = Biquad::new(FilterKind::Peaking(6.0), 2000.0, 1.0, rate);
        assert!((20.0 * pk.magnitude(2000.0, rate).log10() - 6.0).abs() < 0.05);
        let ls = Biquad::new(FilterKind::LowShelf(-12.0), 200.0, 0.707, rate);
        assert!((20.0 * ls.magnitude(10.0, rate).log10() + 12.0).abs() < 0.2);
        assert!(20.0 * ls.magnitude(10_000.0, rate).log10().abs() < 0.2);
        let n = Biquad::new(FilterKind::Notch, 1000.0, 5.0, rate);
        assert!(n.magnitude(1000.0, rate) < 1e-3);
        let ap = Biquad::new(FilterKind::AllPass, 1000.0, 1.0, rate);
        assert!((ap.magnitude(3000.0, rate) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn delay_echoes_with_decaying_feedback() {
        let mut d = Delay::new(0.01, 1000.0, 0.5, 1.0);
        let mut x = vec![0.0; 40];
        x[0] = 1.0;
        d.run(&mut x);
        assert_eq!((x[10], x[20], x[30]), (1.0, 0.5, 0.25));
    }

    #[test]
    fn compressor_curve_and_steady_state() {
        let mut c = Compressor::new(-20.0, 4.0, 0.001, 0.05, 48_000.0);
        c.knee_db = 0.0;
        assert_eq!(c.curve(-30.0), -30.0);
        assert!((c.curve(0.0) - (-15.0)).abs() < 1e-4);
        let mut loud = vec![1.0f32; 4800];
        c.run(&mut loud);
        assert!((20.0 * loud[4799].log10() + 15.0).abs() < 0.1, "{}", 20.0 * loud[4799].log10());
    }

    #[test]
    fn envelope_follower_attacks_fast_releases_slow() {
        let mut e = EnvelopeFollower::new(0.001, 0.1, 1000.0);
        for _ in 0..5 {
            e.process(1.0);
        }
        assert!(e.level > 0.99);
        for _ in 0..10 {
            e.process(0.0);
        }
        assert!(e.level > 0.85);
    }

    #[test]
    fn onsets_and_tempo_of_a_click_track() {
        let rate = 22_050u32;
        let bpm = 120.0;
        let mut data = vec![0.0f32; rate as usize * 4];
        let mut seed = 1u32;
        for beat in 0..8 {
            let at = (beat as f32 * 60.0 / bpm * rate as f32) as usize + 2000;
            for k in 0..600 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (seed >> 9) as f32 / (1u32 << 23) as f32 * 2.0 - 1.0;
                if at + k < data.len() {
                    data[at + k] = noise * (-(k as f32) / 150.0).exp();
                }
            }
        }
        let s = Samples::mono(data, rate);
        let on = OnsetDetector::default().detect(&s);
        assert_eq!(on.len(), 8, "{on:?}");
        for (i, t) in on.iter().enumerate() {
            let expect = i as f32 * 0.5 + 2000.0 / rate as f32;
            assert!((t - expect).abs() < 0.03, "{t} vs {expect}");
        }
        let tempo = estimate_tempo(&on, 80.0, 160.0).unwrap();
        assert!((tempo - 120.0).abs() < 2.0, "{tempo}");
    }

    #[test]
    fn chain_and_stereo_apply() {
        let s = Samples::stereo(vec![1.0, -1.0, 0.0, 0.0, 0.0, 0.0], 1000);
        let out = apply(&s, || Chain::new().then(Delay::new(0.001, 1000.0, 0.0, 1.0)));
        assert_eq!(out.data, vec![0.0, 0.0, 1.0, -1.0, 0.0, 0.0]);
    }
}
