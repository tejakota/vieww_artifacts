//! Metering: loudness, true peak, and the classic ballistic meters.
//!
//! [`LoudnessMeter`] is ITU-R BS.1770-4 as EBU R128 uses it: each channel
//! is K-weighted (the standard's two biquads, re-derived for any sample
//! rate the way the standard's 48 kHz coefficients were made), squared,
//! weighted by its position (1.0 front, 1.41 surround, LFE excluded) and
//! summed in 100 ms sub-blocks. From those come
//!
//! * **momentary** loudness — the last 400 ms;
//! * **short-term** loudness — the last 3 s;
//! * **integrated** loudness — every 400 ms block (75 % overlap) gated at
//!   −70 LUFS absolute, then at −10 LU relative to the absolute-gated mean;
//! * **loudness range** (EBU Tech 3342) — the short-term values gated at
//!   −70 LUFS and −20 LU relative, from the 10th to the 95th percentile.
//!
//! Gated quantities are kept in fixed 0.01 LU histograms (each bin holding
//! the exact energy of its blocks), so a meter that has run for hours holds
//! no more memory than one that started a second ago, and pushing audio
//! never allocates. The tests are the EBU Tech 3341/3342 conformance
//! signals: a 1 kHz sine at −23 dBFS on both channels reads −23.0 LUFS.
//!
//! [`TruePeak`] is BS.1770 Annex 2: 4× oversampling through a windowed-sinc
//! interpolator, the peak of the oversampled signal in dBTP.
//!
//! The ballistic meters are [`PeakMeter`] (instant attack, hold, linear
//! dB fall), [`RmsMeter`] (sliding window), [`VuMeter`] (IEC 60268-17: a
//! critically damped second-order average-responding movement reaching
//! 99 % in 300 ms, calibrated so a sine reads its RMS), [`Ppm`]
//! (IEC 60268-10 quasi-peak, Type I and Type II integration and fall
//! times), [`Correlation`] (phase correlation, −1..+1) and [`Goniometer`]
//! (the mid/side vectorscope's points).

use crate::channels::Layout;
use crate::fx::fir;

/// Loudness of a mean-square energy, in LUFS (BS.1770's −0.691 offset).
#[must_use]
pub fn energy_to_lufs(e: f64) -> f64 {
    if e <= 0.0 {
        f64::NEG_INFINITY
    } else {
        -0.691 + 10.0 * e.log10()
    }
}

/// The energy a loudness corresponds to.
#[must_use]
pub fn lufs_to_energy(l: f64) -> f64 {
    10f64.powf((l + 0.691) / 10.0)
}

/// Linear amplitude to dB (−∞ for silence).
#[must_use]
pub fn to_db(x: f64) -> f64 {
    if x <= 0.0 {
        f64::NEG_INFINITY
    } else {
        20.0 * x.log10()
    }
}

/// BS.1770 channel weights for a layout (LFE excluded, surrounds +1.5 dB).
#[must_use]
pub fn channel_weights(layout: Layout) -> Vec<f64> {
    match layout {
        Layout::Mono => vec![1.0],
        Layout::Stereo => vec![1.0, 1.0],
        Layout::Quad => vec![1.0, 1.0, 1.41, 1.41],
        Layout::Surround51 => vec![1.0, 1.0, 1.0, 0.0, 1.41, 1.41],
        Layout::Surround71 => vec![1.0, 1.0, 1.0, 0.0, 1.41, 1.41, 1.41, 1.41],
        Layout::Discrete(n) => vec![1.0; n],
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    s: [f64; 2],
}

impl Biquad {
    fn tick(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.s[0];
        self.s[0] = self.b[1] * x - self.a[0] * y + self.s[1];
        self.s[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// The K-weighting filter for one channel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KWeighting {
    shelf: Biquad,
    hp: Biquad,
}

impl KWeighting {
    /// K-weighting at `rate`. At 48 kHz this reproduces the coefficients
    /// printed in BS.1770 to the precision they are printed.
    #[must_use]
    pub fn new(rate: f64) -> Self {
        use std::f64::consts::PI;
        // Stage 1: the head's acoustic effect, a high shelf.
        let f0 = 1_681.974_450_955_533;
        let g = 3.999_843_853_973_347;
        let q = 0.707_175_236_955_419_6;
        let k = (PI * f0 / rate).tan();
        let vh = 10f64.powf(g / 20.0);
        let vb = vh.powf(0.499_666_774_154_541_6);
        let a0 = 1.0 + k / q + k * k;
        let shelf = Biquad {
            b: [
                (vh + vb * k / q + k * k) / a0,
                2.0 * (k * k - vh) / a0,
                (vh - vb * k / q + k * k) / a0,
            ],
            a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
            s: [0.0; 2],
        };
        // Stage 2: the RLB high-pass.
        let f0 = 38.135_470_876_024_44;
        let q = 0.500_327_037_323_877_3;
        let k = (PI * f0 / rate).tan();
        let a0 = 1.0 + k / q + k * k;
        let hp = Biquad {
            b: [1.0, -2.0, 1.0],
            a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
            s: [0.0; 2],
        };
        Self { shelf, hp }
    }

    /// Filter one sample.
    pub fn tick(&mut self, x: f64) -> f64 {
        self.hp.tick(self.shelf.tick(x))
    }

    /// The coefficients `(b, a)` of each stage, `a` without its leading 1.
    #[must_use]
    pub fn coefficients(&self) -> [([f64; 3], [f64; 2]); 2] {
        [(self.shelf.b, self.shelf.a), (self.hp.b, self.hp.a)]
    }
}

const HIST_LO: f64 = -70.0;
const HIST_STEP: f64 = 0.01;
const HIST_BINS: usize = 8_000; // −70 .. +10 LUFS

/// A histogram of block loudnesses that keeps each bin's exact energy.
#[derive(Debug, Clone, PartialEq)]
struct Histogram {
    count: Vec<u64>,
    energy: Vec<f64>,
    total: u64,
}

impl Histogram {
    fn new() -> Self {
        Self {
            count: vec![0; HIST_BINS],
            energy: vec![0.0; HIST_BINS],
            total: 0,
        }
    }

    fn bin(l: f64) -> usize {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to the histogram"
        )]
        let i = ((l - HIST_LO) / HIST_STEP).floor().max(0.0) as usize;
        i.min(HIST_BINS - 1)
    }

    fn bin_lufs(i: usize) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "bin index")]
        let v = HIST_LO + (i as f64 + 0.5) * HIST_STEP;
        v
    }

    /// Add a block (callers apply the absolute gate).
    fn add(&mut self, energy: f64) {
        let i = Self::bin(energy_to_lufs(energy));
        self.count[i] += 1;
        self.energy[i] += energy;
        self.total += 1;
    }

    /// Mean energy of the blocks from bin `from` up.
    fn mean_from(&self, from: usize) -> Option<f64> {
        let (n, e) = (from..HIST_BINS).fold((0u64, 0.0), |(n, e), i| {
            (n + self.count[i], e + self.energy[i])
        });
        #[expect(clippy::cast_precision_loss, reason = "block counts")]
        let m = (n > 0).then(|| e / n as f64);
        m
    }

    fn clear(&mut self) {
        self.count.fill(0);
        self.energy.fill(0.0);
        self.total = 0;
    }
}

/// 4× oversampling true-peak detection for one channel (BS.1770 Annex 2).
#[derive(Debug, Clone, PartialEq)]
pub struct TruePeak {
    phases: [[f32; TP_TAPS]; 4],
    hist: [f32; TP_TAPS],
    pos: usize,
    peak: f32,
}

const TP_TAPS: usize = 24;

impl Default for TruePeak {
    fn default() -> Self {
        Self::new()
    }
}

impl TruePeak {
    /// A detector (sample-rate independent: the interpolator is relative).
    #[must_use]
    pub fn new() -> Self {
        // Cutoff at the original Nyquist (0.125 of the 4× rate), ~90 dB
        // stopband; ×4 restores the gain zero-stuffing removes.
        let h = fir::lowpass(4 * TP_TAPS, 0.125, fir::kaiser_beta(90.0));
        let mut phases = [[0.0f32; TP_TAPS]; 4];
        for (p, phase) in phases.iter_mut().enumerate() {
            for (k, t) in phase.iter_mut().enumerate() {
                #[expect(clippy::cast_possible_truncation, reason = "coefficients")]
                let v = (h[p + 4 * k] * 4.0) as f32;
                *t = v;
            }
        }
        Self {
            phases,
            hist: [0.0; TP_TAPS],
            pos: 0,
            peak: 0.0,
        }
    }

    /// Feed one sample; returns the largest of its four interpolated values.
    pub fn tick(&mut self, x: f32) -> f32 {
        self.pos = (self.pos + TP_TAPS - 1) % TP_TAPS;
        self.hist[self.pos] = x;
        let mut m = x.abs();
        for phase in &self.phases {
            let mut acc = 0.0f32;
            for (k, c) in phase.iter().enumerate() {
                acc += c * self.hist[(self.pos + k) % TP_TAPS];
            }
            m = m.max(acc.abs());
        }
        self.peak = self.peak.max(m);
        m
    }

    /// The highest true peak seen (linear).
    #[must_use]
    pub const fn peak(&self) -> f32 {
        self.peak
    }

    /// The highest true peak seen, in dBTP.
    #[must_use]
    pub fn peak_dbtp(&self) -> f64 {
        to_db(f64::from(self.peak))
    }

    /// Forget the maximum.
    pub fn reset(&mut self) {
        self.peak = 0.0;
        self.hist = [0.0; TP_TAPS];
    }
}

/// An EBU R128 / BS.1770-4 loudness meter.
#[derive(Debug, Clone, PartialEq)]
pub struct LoudnessMeter {
    weights: Vec<f64>,
    filters: Vec<KWeighting>,
    peaks: Vec<TruePeak>,
    sample_peak: Vec<f32>,
    sub_len: usize,
    sub_fill: usize,
    sub_acc: f64,
    subs: [f64; 30],
    sub_count: u64,
    integrated: Histogram,
    range: Histogram,
    max_momentary: f64,
    max_short_term: f64,
}

impl LoudnessMeter {
    /// A meter for interleaved audio of `layout` at `rate` Hz.
    #[must_use]
    pub fn new(layout: Layout, rate: u32) -> Self {
        let weights = channel_weights(layout);
        let n = weights.len();
        Self {
            filters: vec![KWeighting::new(f64::from(rate)); n],
            peaks: vec![TruePeak::new(); n],
            sample_peak: vec![0.0; n],
            weights,
            sub_len: usize::try_from(rate.div_ceil(10).max(1)).unwrap_or(4_800),
            sub_fill: 0,
            sub_acc: 0.0,
            subs: [0.0; 30],
            sub_count: 0,
            integrated: Histogram::new(),
            range: Histogram::new(),
            max_momentary: f64::NEG_INFINITY,
            max_short_term: f64::NEG_INFINITY,
        }
    }

    /// Channels expected per frame.
    #[must_use]
    pub fn channels(&self) -> usize {
        self.weights.len()
    }

    /// Meter interleaved frames. Never allocates.
    pub fn push(&mut self, interleaved: &[f32]) {
        let ch = self.weights.len();
        for frame in interleaved.chunks_exact(ch) {
            let mut e = 0.0;
            for (c, &x) in frame.iter().enumerate() {
                self.peaks[c].tick(x);
                self.sample_peak[c] = self.sample_peak[c].max(x.abs());
                if self.weights[c] != 0.0 {
                    let y = self.filters[c].tick(f64::from(x));
                    e += self.weights[c] * y * y;
                }
            }
            self.sub_acc += e;
            self.sub_fill += 1;
            if self.sub_fill == self.sub_len {
                self.close_sub_block();
            }
        }
    }

    fn close_sub_block(&mut self) {
        #[expect(clippy::cast_precision_loss, reason = "block length")]
        let mean = self.sub_acc / self.sub_len as f64;
        self.subs.rotate_left(1);
        self.subs[29] = mean;
        self.sub_acc = 0.0;
        self.sub_fill = 0;
        self.sub_count += 1;
        if self.sub_count >= 4 {
            let e = self.window_energy(4);
            if energy_to_lufs(e) >= HIST_LO {
                self.integrated.add(e);
            }
            self.max_momentary = self.max_momentary.max(energy_to_lufs(e));
        }
        if self.sub_count >= 30 {
            let e = self.window_energy(30);
            if energy_to_lufs(e) >= HIST_LO {
                self.range.add(e);
            }
            self.max_short_term = self.max_short_term.max(energy_to_lufs(e));
        }
    }

    fn window_energy(&self, n: usize) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "window length")]
        let m = self.subs[30 - n..].iter().sum::<f64>() / n as f64;
        m
    }

    /// Momentary loudness (last 400 ms), LUFS; −∞ before 400 ms have passed.
    #[must_use]
    pub fn momentary(&self) -> f64 {
        if self.sub_count < 4 {
            return f64::NEG_INFINITY;
        }
        energy_to_lufs(self.window_energy(4))
    }

    /// Short-term loudness (last 3 s), LUFS; −∞ before 3 s have passed.
    #[must_use]
    pub fn short_term(&self) -> f64 {
        if self.sub_count < 30 {
            return f64::NEG_INFINITY;
        }
        energy_to_lufs(self.window_energy(30))
    }

    /// The loudest momentary value so far.
    #[must_use]
    pub const fn max_momentary(&self) -> f64 {
        self.max_momentary
    }

    /// The loudest short-term value so far.
    #[must_use]
    pub const fn max_short_term(&self) -> f64 {
        self.max_short_term
    }

    /// Integrated (programme) loudness, LUFS; −∞ if nothing passed the gate.
    #[must_use]
    pub fn integrated(&self) -> f64 {
        let Some(abs_mean) = self.integrated.mean_from(0) else {
            return f64::NEG_INFINITY;
        };
        let rel = energy_to_lufs(abs_mean) - 10.0;
        self.integrated
            .mean_from(Histogram::bin(rel))
            .map_or(f64::NEG_INFINITY, energy_to_lufs)
    }

    /// Loudness range (EBU Tech 3342), LU; 0 with too little programme.
    #[must_use]
    pub fn range(&self) -> f64 {
        let Some(abs_mean) = self.range.mean_from(0) else {
            return 0.0;
        };
        let from = Histogram::bin(energy_to_lufs(abs_mean) - 20.0);
        let n: u64 = self.range.count[from..].iter().sum();
        if n == 0 {
            return 0.0;
        }
        let percentile = |p: f64| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss,
                reason = "rank within the gated count"
            )]
            let rank = ((n - 1) as f64 * p).round() as u64;
            let mut seen = 0;
            for i in from..HIST_BINS {
                seen += self.range.count[i];
                if seen > rank {
                    return Histogram::bin_lufs(i);
                }
            }
            Histogram::bin_lufs(HIST_BINS - 1)
        };
        percentile(0.95) - percentile(0.10)
    }

    /// The highest true peak across channels, dBTP.
    #[must_use]
    pub fn true_peak(&self) -> f64 {
        self.peaks
            .iter()
            .map(TruePeak::peak_dbtp)
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// The highest true peak per channel, dBTP.
    #[must_use]
    pub fn true_peaks(&self) -> Vec<f64> {
        self.peaks.iter().map(TruePeak::peak_dbtp).collect()
    }

    /// The highest sample peak across channels, dBFS.
    #[must_use]
    pub fn sample_peak(&self) -> f64 {
        to_db(f64::from(
            self.sample_peak.iter().copied().fold(0.0, f32::max),
        ))
    }

    /// Start a new programme.
    pub fn reset(&mut self) {
        let rate_filters: Vec<KWeighting> = self
            .filters
            .iter()
            .map(|f| {
                let mut f = *f;
                f.shelf.s = [0.0; 2];
                f.hp.s = [0.0; 2];
                f
            })
            .collect();
        self.filters = rate_filters;
        self.peaks.iter_mut().for_each(TruePeak::reset);
        self.sample_peak.fill(0.0);
        self.sub_fill = 0;
        self.sub_acc = 0.0;
        self.subs = [0.0; 30];
        self.sub_count = 0;
        self.integrated.clear();
        self.range.clear();
        self.max_momentary = f64::NEG_INFINITY;
        self.max_short_term = f64::NEG_INFINITY;
    }
}

/// Measure a whole buffer at once: `(integrated LUFS, LRA LU, dBTP)`.
#[must_use]
pub fn measure(interleaved: &[f32], layout: Layout, rate: u32) -> (f64, f64, f64) {
    let mut m = LoudnessMeter::new(layout, rate);
    m.push(interleaved);
    (m.integrated(), m.range(), m.true_peak())
}

/// The gain, in dB, that brings a buffer to `target` LUFS integrated.
#[must_use]
pub fn normalization_gain(interleaved: &[f32], layout: Layout, rate: u32, target: f64) -> f64 {
    let (i, _, _) = measure(interleaved, layout, rate);
    if i.is_finite() {
        target - i
    } else {
        0.0
    }
}

/// A peak meter: instant attack, a hold, then a linear fall in dB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeakMeter {
    level: f32,
    held: f32,
    hold_left: u32,
    hold: u32,
    fall_per_sample: f32,
    clipped: bool,
}

impl PeakMeter {
    /// `hold` seconds of hold, then a fall of `fall_db_per_s`.
    #[must_use]
    pub fn new(hold: f32, fall_db_per_s: f32, rate: f32) -> Self {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "sample counts"
        )]
        let hold = (hold * rate).max(0.0) as u32;
        Self {
            level: 0.0,
            held: 0.0,
            hold_left: 0,
            hold,
            fall_per_sample: 10f32.powf(-fall_db_per_s / 20.0 / rate),
            clipped: false,
        }
    }

    /// Feed one sample.
    pub fn tick(&mut self, x: f32) {
        let a = x.abs();
        if a >= 1.0 {
            self.clipped = true;
        }
        if a >= self.level {
            self.level = a;
        } else {
            self.level *= self.fall_per_sample;
        }
        if a >= self.held {
            self.held = a;
            self.hold_left = self.hold;
        } else if self.hold_left > 0 {
            self.hold_left -= 1;
        } else {
            self.held *= self.fall_per_sample;
        }
    }

    /// Feed a buffer.
    pub fn push(&mut self, xs: &[f32]) {
        xs.iter().for_each(|x| self.tick(*x));
    }

    /// The bar's level, dBFS.
    #[must_use]
    pub fn db(&self) -> f32 {
        20.0 * self.level.max(1e-12).log10()
    }

    /// The held peak marker, dBFS.
    #[must_use]
    pub fn held_db(&self) -> f32 {
        20.0 * self.held.max(1e-12).log10()
    }

    /// Whether a sample reached full scale since the last [`clear_clip`](Self::clear_clip).
    #[must_use]
    pub const fn clipped(&self) -> bool {
        self.clipped
    }

    /// Reset the clip light.
    pub fn clear_clip(&mut self) {
        self.clipped = false;
    }
}

/// A sliding-window RMS meter.
#[derive(Debug, Clone, PartialEq)]
pub struct RmsMeter {
    window: Vec<f64>,
    pos: usize,
    sum: f64,
}

impl RmsMeter {
    /// RMS over the last `seconds`.
    #[must_use]
    pub fn new(seconds: f32, rate: f32) -> Self {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "window length"
        )]
        let n = (seconds * rate).round().max(1.0) as usize;
        Self {
            window: vec![0.0; n],
            pos: 0,
            sum: 0.0,
        }
    }

    /// Feed one sample.
    pub fn tick(&mut self, x: f32) {
        let sq = f64::from(x) * f64::from(x);
        self.sum += sq - self.window[self.pos];
        self.window[self.pos] = sq;
        self.pos = (self.pos + 1) % self.window.len();
        if self.pos == 0 {
            // Re-sum once per window so rounding never accumulates.
            self.sum = self.window.iter().sum();
        }
    }

    /// Feed a buffer.
    pub fn push(&mut self, xs: &[f32]) {
        xs.iter().for_each(|x| self.tick(*x));
    }

    /// The RMS level (linear).
    #[must_use]
    pub fn rms(&self) -> f32 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss,
            reason = "meter reading"
        )]
        let r = (self.sum.max(0.0) / self.window.len() as f64).sqrt() as f32;
        r
    }

    /// The RMS level, dBFS (a full-scale sine reads −3.01).
    #[must_use]
    pub fn db(&self) -> f32 {
        20.0 * self.rms().max(1e-12).log10()
    }
}

/// A VU meter (IEC 60268-17).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VuMeter {
    a: f64,
    s1: f64,
    s2: f64,
    reference_dbfs: f32,
}

impl VuMeter {
    /// A VU meter reading 0 VU at an RMS of `reference_dbfs` (−18 is the
    /// common digital alignment).
    #[must_use]
    pub fn new(reference_dbfs: f32, rate: f32) -> Self {
        // Two critically damped poles: 1 − (1 + t/τ)e^{−t/τ} = 0.99 at
        // t = 300 ms gives t/τ = 6.638.
        let tau = 0.3 / 6.638_352;
        Self {
            a: (-1.0 / (tau * f64::from(rate))).exp(),
            s1: 0.0,
            s2: 0.0,
            reference_dbfs,
        }
    }

    /// Feed one sample.
    pub fn tick(&mut self, x: f32) {
        // Average-responding, scaled so a sine reads its RMS: π / (2√2).
        let v = f64::from(x.abs()) * std::f64::consts::PI / (2.0 * std::f64::consts::SQRT_2);
        self.s1 = v + self.a * (self.s1 - v);
        self.s2 = self.s1 + self.a * (self.s2 - self.s1);
    }

    /// Feed a buffer.
    pub fn push(&mut self, xs: &[f32]) {
        xs.iter().for_each(|x| self.tick(*x));
    }

    /// The needle, in VU (dB relative to the reference).
    #[must_use]
    pub fn vu(&self) -> f32 {
        #[expect(clippy::cast_possible_truncation, reason = "meter reading")]
        let db = to_db(self.s2).max(-200.0) as f32;
        db - self.reference_dbfs
    }
}

/// A quasi-peak programme meter (IEC 60268-10).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ppm {
    attack: f32,
    fall_per_sample: f32,
    level: f32,
}

impl Ppm {
    /// A PPM whose *integration time* (the burst length that reads 2 dB
    /// under the steady reading) is `integration` seconds, falling
    /// `fall_db` in `fall_s` seconds.
    #[must_use]
    pub fn new(integration: f32, fall_db: f32, fall_s: f32, rate: f32) -> Self {
        // 1 − e^{−T/τ} = 10^{−2/20}  ⇒  τ = T / 1.5782.
        let tau = integration / -(1.0 - 10f32.powf(-0.1)).ln();
        Self {
            attack: 1.0 - (-1.0 / (tau * rate)).exp(),
            fall_per_sample: 10f32.powf(-fall_db / fall_s / 20.0 / rate),
            level: 0.0,
        }
    }

    /// Type I (DIN 45406): 5 ms integration, 20 dB fall in 1.7 s.
    #[must_use]
    pub fn type_i(rate: f32) -> Self {
        Self::new(0.005, 20.0, 1.7, rate)
    }

    /// Type II (BBC / EBU): 10 ms integration, 24 dB fall in 2.8 s.
    #[must_use]
    pub fn type_ii(rate: f32) -> Self {
        Self::new(0.010, 24.0, 2.8, rate)
    }

    /// Feed one sample.
    pub fn tick(&mut self, x: f32) {
        let a = x.abs();
        if a > self.level {
            self.level += (a - self.level) * self.attack;
        } else {
            self.level *= self.fall_per_sample;
        }
    }

    /// Feed a buffer.
    pub fn push(&mut self, xs: &[f32]) {
        xs.iter().for_each(|x| self.tick(*x));
    }

    /// The reading, dBFS.
    #[must_use]
    pub fn db(&self) -> f32 {
        20.0 * self.level.max(1e-12).log10()
    }
}

/// A phase-correlation meter: +1 mono-compatible, 0 unrelated, −1 inverted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Correlation {
    a: f64,
    lr: f64,
    ll: f64,
    rr: f64,
}

impl Correlation {
    /// Averaging with time constant `tau` seconds.
    #[must_use]
    pub fn new(tau: f32, rate: f32) -> Self {
        Self {
            a: (-1.0 / (f64::from(tau) * f64::from(rate))).exp(),
            lr: 0.0,
            ll: 0.0,
            rr: 0.0,
        }
    }

    /// Feed one stereo frame.
    pub fn tick(&mut self, l: f32, r: f32) {
        let (l, r) = (f64::from(l), f64::from(r));
        let b = 1.0 - self.a;
        self.lr = self.a * self.lr + b * l * r;
        self.ll = self.a * self.ll + b * l * l;
        self.rr = self.a * self.rr + b * r * r;
    }

    /// Feed interleaved stereo.
    pub fn push(&mut self, interleaved: &[f32]) {
        for f in interleaved.chunks_exact(2) {
            self.tick(f[0], f[1]);
        }
    }

    /// The reading, −1..+1 (0 in silence).
    #[must_use]
    pub fn value(&self) -> f32 {
        let d = (self.ll * self.rr).sqrt();
        #[expect(clippy::cast_possible_truncation, reason = "meter reading")]
        let v = if d < 1e-20 { 0.0 } else { (self.lr / d) as f32 };
        v.clamp(-1.0, 1.0)
    }
}

/// A goniometer (vectorscope): the recent stereo frames as mid/side points.
#[derive(Debug, Clone, PartialEq)]
pub struct Goniometer {
    points: Vec<(f32, f32)>,
    pos: usize,
    filled: bool,
}

impl Goniometer {
    /// Keep the last `capacity` points.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            points: vec![(0.0, 0.0); capacity.max(1)],
            pos: 0,
            filled: false,
        }
    }

    /// The display point for a frame: `x` is side (left is negative), `y`
    /// is mid — so mono is a vertical line, and a hard-left signal leans
    /// left at 45°.
    #[must_use]
    pub fn point(l: f32, r: f32) -> (f32, f32) {
        let s = std::f32::consts::FRAC_1_SQRT_2;
        ((r - l) * s, (l + r) * s)
    }

    /// Feed interleaved stereo.
    pub fn push(&mut self, interleaved: &[f32]) {
        for f in interleaved.chunks_exact(2) {
            self.points[self.pos] = Self::point(f[0], f[1]);
            self.pos = (self.pos + 1) % self.points.len();
            if self.pos == 0 {
                self.filled = true;
            }
        }
    }

    /// The points, oldest first.
    pub fn points(&self) -> impl Iterator<Item = (f32, f32)> + '_ {
        let (start, n) = if self.filled {
            (self.pos, self.points.len())
        } else {
            (0, self.pos)
        };
        (0..n).map(move |i| self.points[(start + i) % self.points.len()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stereo sine of `dbfs` peak level on both channels.
    fn tone(hz: f64, dbfs: f64, secs: f64, rate: u32) -> Vec<f32> {
        let a = 10f64.powf(dbfs / 20.0);
        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = (secs * f64::from(rate)) as usize;
        (0..n)
            .flat_map(|i| {
                let v =
                    (a * (std::f64::consts::TAU * hz * i as f64 / f64::from(rate)).sin()) as f32;
                [v, v]
            })
            .collect()
    }

    #[test]
    fn k_weighting_matches_the_printed_48k_coefficients() {
        let k = KWeighting::new(48_000.0);
        let [(b1, a1), (b2, a2)] = k.coefficients();
        let close = |x: f64, y: f64| (x - y).abs() < 1e-9;
        assert!(close(b1[0], 1.535_124_859_586_97));
        assert!(close(b1[1], -2.691_696_189_406_38));
        assert!(close(b1[2], 1.198_392_810_852_85));
        assert!(close(a1[0], -1.690_659_293_182_41));
        assert!(close(a1[1], 0.732_480_774_215_85));
        assert_eq!(b2, [1.0, -2.0, 1.0]);
        assert!(close(a2[0], -1.990_047_454_833_98));
        assert!(close(a2[1], 0.990_072_250_366_21));
    }

    #[test]
    fn ebu_3341_cases_1_and_2_steady_tones() {
        for rate in [44_100, 48_000] {
            for level in [-23.0, -33.0] {
                let x = tone(1_000.0, level, 20.0, rate);
                let mut m = LoudnessMeter::new(Layout::Stereo, rate);
                m.push(&x);
                for got in [m.momentary(), m.short_term(), m.integrated()] {
                    assert!((got - level).abs() <= 0.1, "{rate} {level}: {got}");
                }
            }
        }
    }

    #[test]
    fn ebu_3341_case_3_and_4_the_relative_gate() {
        let rate = 48_000;
        let mut x = tone(1_000.0, -36.0, 10.0, rate);
        x.extend(tone(1_000.0, -23.0, 60.0, rate));
        x.extend(tone(1_000.0, -36.0, 10.0, rate));
        let (i, _, _) = measure(&x, Layout::Stereo, rate);
        assert!((i + 23.0).abs() <= 0.1, "{i}");
        // Case 4: −72 / −36 / −23 / −36 / −72, the absolute gate too.
        let mut y = tone(1_000.0, -72.0, 10.0, rate);
        y.extend(&x);
        y.extend(tone(1_000.0, -72.0, 10.0, rate));
        let (i, _, _) = measure(&y, Layout::Stereo, rate);
        assert!((i + 23.0).abs() <= 0.1, "{i}");
    }

    #[test]
    fn ebu_3341_case_5_gate_threshold_steps() {
        let rate = 48_000;
        let mut x = tone(1_000.0, -26.0, 20.0, rate);
        x.extend(tone(1_000.0, -20.0, 20.1, rate));
        x.extend(tone(1_000.0, -26.0, 20.0, rate));
        let (i, _, _) = measure(&x, Layout::Stereo, rate);
        assert!((i + 23.0).abs() <= 0.1, "{i}");
    }

    #[test]
    fn ebu_3342_loudness_range() {
        let rate = 48_000;
        // Case 1: 20 s at −20 then 20 s at −30 → LRA 10 ± 1.
        let mut x = tone(1_000.0, -20.0, 20.0, rate);
        x.extend(tone(1_000.0, -30.0, 20.0, rate));
        let (_, lra, _) = measure(&x, Layout::Stereo, rate);
        assert!((lra - 10.0).abs() <= 1.0, "{lra}");
        // Case 3: −40 / −20 — the lower part falls under the relative gate.
        let mut y = tone(1_000.0, -40.0, 20.0, rate);
        y.extend(tone(1_000.0, -20.0, 20.0, rate));
        let (_, lra, _) = measure(&y, Layout::Stereo, rate);
        assert!((lra - 20.0).abs() <= 1.0, "{lra}");
        // A steady tone has (nearly) no range.
        let (_, lra, _) = measure(&tone(1_000.0, -23.0, 20.0, rate), Layout::Stereo, rate);
        assert!(lra < 0.1, "{lra}");
    }

    #[test]
    fn surround_channels_are_weighted_and_lfe_ignored() {
        let rate = 48_000;
        let mono = tone(1_000.0, -23.0, 5.0, rate);
        let frames = mono.len() / 2;
        let mut sur = vec![0.0f32; frames * 6];
        for f in 0..frames {
            sur[f * 6 + 4] = mono[f * 2]; // SL
            sur[f * 6 + 3] = mono[f * 2]; // LFE — must not count
        }
        let mut m = LoudnessMeter::new(Layout::Surround51, rate);
        m.push(&sur);
        // One channel at −23 dBFS: −26.0 (one of two) + 1.49 dB (×1.41).
        let want = -23.0 - 10.0 * 2f64.log10() + 10.0 * 1.41f64.log10();
        assert!((m.integrated() - want).abs() < 0.1, "{}", m.integrated());
    }

    #[test]
    fn silence_and_short_input_are_minus_infinity() {
        let mut m = LoudnessMeter::new(Layout::Stereo, 48_000);
        assert_eq!(m.momentary(), f64::NEG_INFINITY);
        m.push(&vec![0.0; 48_000 * 2]);
        assert_eq!(m.integrated(), f64::NEG_INFINITY);
        assert_eq!(m.short_term(), f64::NEG_INFINITY);
        assert_eq!(m.range(), 0.0);
    }

    #[test]
    fn true_peak_finds_the_peak_between_samples() {
        // fs/4 at 45°: every sample is ±0.707, the waveform peaks at 1.0.
        let x: Vec<f32> = (0..4_800)
            .map(|i| {
                (std::f64::consts::FRAC_PI_2 * f64::from(i) + std::f64::consts::FRAC_PI_4).sin()
                    as f32
            })
            .flat_map(|v| [v, v])
            .collect();
        let mut m = LoudnessMeter::new(Layout::Stereo, 48_000);
        m.push(&x);
        assert!((m.sample_peak() + 3.01).abs() < 0.05, "{}", m.sample_peak());
        assert!(m.true_peak().abs() < 0.2, "{}", m.true_peak());
    }

    #[test]
    fn true_peak_of_a_low_tone_equals_its_sample_peak() {
        let x = tone(997.0, -6.0, 1.0, 48_000);
        let (_, _, tp) = measure(&x, Layout::Stereo, 48_000);
        assert!((tp + 6.0).abs() < 0.05, "{tp}");
    }

    #[test]
    fn normalization_hits_the_target() {
        let x = tone(1_000.0, -30.0, 10.0, 48_000);
        let g = normalization_gain(&x, Layout::Stereo, 48_000, -23.0);
        assert!((g - 7.0).abs() < 0.1, "{g}");
    }

    #[test]
    fn pushing_never_allocates_more_memory_over_time() {
        // The histograms are fixed: an hour costs what a second costs.
        let mut m = LoudnessMeter::new(Layout::Stereo, 8_000);
        let x = tone(1_000.0, -23.0, 60.0, 8_000);
        for _ in 0..10 {
            m.push(&x);
        }
        assert_eq!(m.integrated.count.len(), HIST_BINS);
        assert!(m.integrated.total > 5_000);
        assert!((m.integrated() + 23.0).abs() < 0.2, "{}", m.integrated());
    }

    #[test]
    fn peak_meter_holds_then_falls() {
        let mut p = PeakMeter::new(0.5, 20.0, 1_000.0);
        p.tick(0.5);
        for _ in 0..400 {
            p.tick(0.0);
        }
        assert!((p.held_db() + 6.02).abs() < 0.01);
        assert!(p.db() < -6.02 - 7.0);
        for _ in 0..600 {
            p.tick(0.0);
        }
        // 0.5 s hold then 0.5 s at 20 dB/s.
        assert!(
            (p.held_db() - (-6.02 - 10.0)).abs() < 0.3,
            "{}",
            p.held_db()
        );
        assert!(!p.clipped());
        p.tick(-1.0);
        assert!(p.clipped());
    }

    #[test]
    fn rms_of_a_full_scale_sine_is_minus_3() {
        let mut r = RmsMeter::new(0.3, 48_000.0);
        for i in 0..48_000 {
            r.tick((std::f32::consts::TAU * 1_000.0 * i as f32 / 48_000.0).sin());
        }
        assert!((r.db() + 3.01).abs() < 0.02, "{}", r.db());
    }

    #[test]
    fn vu_reaches_99_percent_in_300_ms_and_reads_rms() {
        let rate = 48_000.0;
        let reference = -18.0;
        // A sine whose RMS is the reference level.
        let amp = 10f32.powf(reference / 20.0) * std::f32::consts::SQRT_2;
        let mut vu = VuMeter::new(reference, rate);
        let sine = |i: usize| amp * (std::f32::consts::TAU * 1_000.0 * i as f32 / rate).sin();
        for i in 0..(0.3 * rate) as usize {
            vu.tick(sine(i));
        }
        let at_300 = vu.vu();
        for i in 0..rate as usize {
            vu.tick(sine(i));
        }
        assert!(vu.vu().abs() < 0.05, "steady {}", vu.vu());
        // 99 % of the reading is −0.087 dB.
        assert!((at_300 + 0.087).abs() < 0.05, "at 300 ms {at_300}");
    }

    #[test]
    fn ppm_integration_time_is_the_minus_2_db_burst() {
        for (mut p, t) in [
            (Ppm::type_i(48_000.0), 0.005),
            (Ppm::type_ii(48_000.0), 0.010),
        ] {
            for _ in 0..(t * 48_000.0) as usize {
                p.tick(0.5);
            }
            assert!((p.db() - (-6.02 - 2.0)).abs() < 0.05, "{}", p.db());
        }
        // Type II falls 24 dB in 2.8 s.
        let mut p = Ppm::type_ii(1_000.0);
        for _ in 0..1_000 {
            p.tick(1.0);
        }
        let top = p.db();
        for _ in 0..2_800 {
            p.tick(0.0);
        }
        assert!((top - p.db() - 24.0).abs() < 0.1);
    }

    #[test]
    fn correlation_reads_mono_inverted_and_unrelated() {
        let rate = 48_000.0;
        let s = |i: usize| (i as f32 * 0.05).sin();
        let mut c = Correlation::new(0.1, rate);
        (0..20_000).for_each(|i| c.tick(s(i), s(i)));
        assert!((c.value() - 1.0).abs() < 1e-4);
        let mut c = Correlation::new(0.1, rate);
        (0..20_000).for_each(|i| c.tick(s(i), -s(i)));
        assert!((c.value() + 1.0).abs() < 1e-4);
        let mut rng = crate::rng::Rng::new(3);
        let mut c = Correlation::new(0.3, rate);
        for _ in 0..48_000 {
            c.tick(rng.bipolar(), rng.bipolar());
        }
        assert!(c.value().abs() < 0.1, "{}", c.value());
        assert_eq!(Correlation::new(0.1, rate).value(), 0.0);
    }

    #[test]
    fn goniometer_points() {
        let (x, y) = Goniometer::point(0.5, 0.5);
        assert!(x.abs() < 1e-7 && y > 0.7);
        let (x, y) = Goniometer::point(1.0, 0.0);
        assert!(x < 0.0 && (x + y).abs() < 1e-6);
        let mut g = Goniometer::new(3);
        g.push(&[1.0, 1.0, 0.0, 1.0]);
        assert_eq!(g.points().count(), 2);
        g.push(&[0.0, 0.0, 0.0, 0.0]);
        let pts: Vec<_> = g.points().collect();
        assert_eq!(pts.len(), 3);
        assert_eq!(pts[2], (0.0, 0.0));
        assert!(pts[0].0 > 0.0);
    }
}
