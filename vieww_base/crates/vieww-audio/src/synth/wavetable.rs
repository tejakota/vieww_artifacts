//! Wavetable synthesis and periodic waves.
//!
//! A [`Wavetable`] is a stack of single-cycle *frames*; an oscillator reads
//! one frame, or crossfades between neighbours as its *position* moves — the
//! evolving timbres Serum, Vital and the PPG Wave are known for. Web Audio's
//! `PeriodicWave` is the one-frame case built from Fourier coefficients
//! instead of samples, and [`Wavetable::from_harmonics`] is exactly that.
//!
//! # Band-limiting
//!
//! A frame read at a high pitch contains harmonics above Nyquist, which alias.
//! Each frame is therefore stored as a *mip chain*: level `k` keeps only the
//! harmonics below `max_harmonics >> k`, rebuilt through an inverse FFT with
//! everything above zeroed. [`WavetableOsc`] picks, per sample, the richest
//! level whose top harmonic stays below Nyquist at the current pitch, so a
//! sweep from a sub-bass to a whistle stays clean — the standard scheme, and
//! the reason a wavetable synth's CPU cost does not depend on pitch.

use crate::fft::{Complex, Plan};

/// Samples per stored cycle.
pub const TABLE_SIZE: usize = 2048;

/// One frame's mip chain: `levels[k]` keeps harmonics `1..=(TABLE_SIZE/2) >> k`.
#[derive(Debug, Clone)]
struct Frame {
    levels: Vec<Vec<f32>>,
}

/// A wavetable: frames, each band-limited at every octave.
#[derive(Debug, Clone)]
pub struct Wavetable {
    frames: Vec<Frame>,
}

/// Build a frame's mip chain from its spectrum (bins `0..=TABLE_SIZE/2`).
fn frame_from_spectrum(spectrum: &[Complex], normalise: bool) -> Frame {
    let plan = Plan::new(TABLE_SIZE);
    let top = TABLE_SIZE / 2;
    let mut levels = Vec::new();
    let mut max_h = top - 1;
    let mut peak_ref: Option<f64> = None;
    loop {
        let mut buf = vec![Complex::ZERO; TABLE_SIZE];
        for h in 1..=max_h.min(spectrum.len() - 1) {
            buf[h] = spectrum[h];
            buf[TABLE_SIZE - h] = spectrum[h].conj();
        }
        plan.inverse(&mut buf);
        let samples: Vec<f64> = buf.iter().map(|z| z.re).collect();
        // Normalise every level by the *full* level's peak, so octaves do
        // not jump in loudness as the oscillator crosses them.
        let peak = *peak_ref.get_or_insert_with(|| {
            samples
                .iter()
                .fold(0.0f64, |m, v| m.max(v.abs()))
                .max(1e-12)
        });
        let k = if normalise { 1.0 / peak } else { 1.0 };
        #[expect(clippy::cast_possible_truncation, reason = "tables are f32")]
        levels.push(samples.iter().map(|v| (v * k) as f32).collect());
        if max_h <= 1 {
            break;
        }
        max_h /= 2;
    }
    Frame { levels }
}

impl Wavetable {
    /// Frames from raw single cycles of any length (resampled to
    /// [`TABLE_SIZE`] through their spectrum). Each frame is normalised to
    /// peak 1.
    ///
    /// # Panics
    /// If `cycles` is empty or any cycle is empty.
    #[must_use]
    pub fn from_cycles(cycles: &[Vec<f32>]) -> Self {
        assert!(!cycles.is_empty(), "a wavetable needs at least one frame");
        let frames = cycles
            .iter()
            .map(|c| {
                assert!(!c.is_empty(), "empty cycle");
                // DFT the cycle at its own length, keep harmonics that fit.
                let n = c.len();
                let max_h = (n / 2).min(TABLE_SIZE / 2);
                let mut spec = vec![Complex::ZERO; TABLE_SIZE / 2 + 1];
                for (h, slot) in spec.iter_mut().enumerate().take(max_h + 1).skip(1) {
                    let mut acc = Complex::ZERO;
                    for (i, v) in c.iter().enumerate() {
                        #[expect(clippy::cast_precision_loss, reason = "cycle lengths")]
                        let th = -std::f64::consts::TAU * (h * i) as f64 / n as f64;
                        acc += Complex::from_angle(th).scale(f64::from(*v));
                    }
                    #[expect(clippy::cast_precision_loss, reason = "cycle lengths")]
                    let k = TABLE_SIZE as f64 / n as f64;
                    *slot = acc.scale(k);
                }
                frame_from_spectrum(&spec, true)
            })
            .collect();
        Self { frames }
    }

    /// A one-frame table from Fourier coefficients — Web Audio's
    /// `createPeriodicWave(real, imag)`: `x(t) = Σ real[k]·cos(2πkt) +
    /// imag[k]·sin(2πkt)` for `k ≥ 1` (index 0, DC, is ignored as in the
    /// spec). `normalise` matches the spec's default of peak 1.
    #[must_use]
    pub fn from_harmonics(real: &[f32], imag: &[f32], normalise: bool) -> Self {
        let n = real.len().max(imag.len()).min(TABLE_SIZE / 2);
        let mut spec = vec![Complex::ZERO; TABLE_SIZE / 2 + 1];
        #[expect(clippy::cast_precision_loss, reason = "table size")]
        let half = TABLE_SIZE as f64 / 2.0;
        for (k, slot) in spec.iter_mut().enumerate().take(n).skip(1) {
            let a = f64::from(real.get(k).copied().unwrap_or(0.0));
            let b = f64::from(imag.get(k).copied().unwrap_or(0.0));
            // a cos + b sin  ⇔  X[k] = N/2 (a − ib)
            *slot = Complex::new(a * half, -b * half);
        }
        Self {
            frames: vec![frame_from_spectrum(&spec, normalise)],
        }
    }

    /// A table that morphs through the four classic shapes:
    /// sine → triangle → saw → square.
    #[must_use]
    pub fn classic() -> Self {
        let h = TABLE_SIZE / 2;
        let mut sine = vec![0.0; h];
        sine[1] = 1.0;
        let mut tri = vec![0.0; h];
        let mut saw = vec![0.0; h];
        let mut sq = vec![0.0; h];
        for k in 1..h {
            #[expect(clippy::cast_precision_loss, reason = "harmonic index")]
            let kf = k as f32;
            saw[k] = 2.0 / (std::f32::consts::PI * kf);
            if k % 2 == 1 {
                sq[k] = 4.0 / (std::f32::consts::PI * kf);
                let sign = if (k / 2) % 2 == 0 { 1.0 } else { -1.0 };
                tri[k] = sign * 8.0 / (std::f32::consts::PI * std::f32::consts::PI * kf * kf);
            }
        }
        let zero = vec![0.0; h];
        let frames = [sine, tri, saw, sq]
            .iter()
            .map(|imag| Self::from_harmonics(&zero, imag, true).frames.remove(0))
            .collect();
        Self { frames }
    }

    /// The number of frames.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.frames.len()
    }

    /// Mip levels per frame.
    #[must_use]
    pub fn levels(&self) -> usize {
        self.frames[0].levels.len()
    }

    /// Read frame `f`, mip level `level`, at phase `t` (turns), linearly
    /// interpolated.
    fn read(&self, f: usize, level: usize, t: f64) -> f32 {
        let lv = &self.frames[f].levels[level.min(self.frames[f].levels.len() - 1)];
        #[expect(clippy::cast_precision_loss, reason = "table size")]
        let x = t.rem_euclid(1.0) * TABLE_SIZE as f64;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "in range"
        )]
        let i = x as usize % TABLE_SIZE;
        #[expect(clippy::cast_possible_truncation, reason = "fraction")]
        let fr = (x - x.floor()) as f32;
        let a = lv[i];
        let b = lv[(i + 1) % TABLE_SIZE];
        a + (b - a) * fr
    }

    /// The mip level for a phase increment `inc` (cycles per sample).
    fn level_for(&self, inc: f64) -> usize {
        // Level k holds harmonics up to (TABLE_SIZE/2 − 1) >> k; the top one
        // must stay below 0.5 cycles/sample.
        let top = TABLE_SIZE / 2 - 1;
        let mut k = 0;
        while k + 1 < self.levels() {
            #[expect(clippy::cast_precision_loss, reason = "small")]
            let max_h = (top >> k) as f64;
            if max_h * inc < 0.5 {
                break;
            }
            k += 1;
        }
        k
    }

    /// Sample the table at `position` (frames, fractional) and phase `t`,
    /// band-limited for increment `inc`.
    #[must_use]
    pub fn sample(&self, position: f32, t: f64, inc: f64) -> f32 {
        let level = self.level_for(inc);
        #[expect(clippy::cast_precision_loss, reason = "frame counts")]
        let max = (self.frames.len() - 1) as f32;
        let p = position.clamp(0.0, max);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped"
        )]
        let f0 = p.floor() as usize;
        let f1 = (f0 + 1).min(self.frames.len() - 1);
        let w = p - p.floor();
        let a = self.read(f0, level, t);
        if w == 0.0 || f0 == f1 {
            a
        } else {
            a + (self.read(f1, level, t) - a) * w
        }
    }
}

/// An oscillator reading a [`Wavetable`].
#[derive(Debug, Clone)]
pub struct WavetableOsc {
    table: std::sync::Arc<Wavetable>,
    rate: f64,
    phase: f64,
    inc: f64,
    /// Fractional frame position, `0..frames-1`.
    pub position: f32,
}

impl WavetableOsc {
    /// An oscillator at `hz`.
    #[must_use]
    pub fn new(table: std::sync::Arc<Wavetable>, hz: f32, rate: f32) -> Self {
        let mut o = Self {
            table,
            rate: f64::from(rate),
            phase: 0.0,
            inc: 0.0,
            position: 0.0,
        };
        o.set_frequency(hz);
        o
    }

    /// Change pitch, phase-continuous.
    pub fn set_frequency(&mut self, hz: f32) {
        self.inc = (f64::from(hz) / self.rate).clamp(0.0, 0.5);
    }

    /// The next sample, with a phase offset in turns (for PM from outside).
    pub fn next_pm(&mut self, offset: f64) -> f32 {
        let v = self
            .table
            .sample(self.position, self.phase + offset, self.inc);
        self.phase = (self.phase + self.inc).rem_euclid(1.0);
        v
    }
}

impl super::Generator for WavetableOsc {
    fn next_sample(&mut self) -> f32 {
        self.next_pm(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::testing::{level_near, spectrum};
    use crate::synth::Generator;
    use std::sync::Arc;

    const RATE: f32 = 48_000.0;

    #[test]
    fn periodic_wave_matches_its_coefficients() {
        // 1·sin(1) + 0.5·cos(3): un-normalised, the table *is* the formula.
        let real = [0.0, 0.0, 0.0, 0.5];
        let imag = [0.0, 1.0];
        let wt = Wavetable::from_harmonics(&real, &imag, false);
        for i in 0..32 {
            let t = f64::from(i) / 32.0;
            let want =
                (std::f64::consts::TAU * t).sin() + 0.5 * (3.0 * std::f64::consts::TAU * t).cos();
            let got = f64::from(wt.sample(0.0, t, 0.0));
            assert!((got - want).abs() < 1e-3, "t={t}: {got} vs {want}");
        }
    }

    #[test]
    fn normalised_periodic_wave_peaks_at_one() {
        let wt = Wavetable::from_harmonics(&[0.0], &[0.0, 3.0, 2.0], true);
        let peak = (0..TABLE_SIZE)
            .map(|i| {
                #[expect(clippy::cast_precision_loss, reason = "test")]
                let t = i as f64 / TABLE_SIZE as f64;
                wt.sample(0.0, t, 0.0).abs()
            })
            .fold(0.0, f32::max);
        assert!((peak - 1.0).abs() < 1e-3, "{peak}");
    }

    #[test]
    fn high_notes_drop_harmonics_above_nyquist() {
        let wt = Arc::new(Wavetable::classic());
        let mut o = WavetableOsc::new(wt, 5_000.0, RATE);
        o.position = 2.0; // saw
        let x = o.render(16_384, 48_000).data;
        let (mags, bin) = spectrum(&x, RATE);
        // Harmonic 5 (25 kHz) would alias to 23 kHz; it must be absent,
        // while harmonic 4 (20 kHz) may remain.
        let alias = level_near(&mags, bin, 23_000.0, 3);
        let fund = level_near(&mags, bin, 5_000.0, 3);
        assert!(alias < fund * 1e-3, "alias {alias} vs fundamental {fund}");
    }

    #[test]
    fn position_morphs_from_sine_to_square() {
        let wt = Arc::new(Wavetable::classic());
        let third = |pos: f32| {
            let mut o = WavetableOsc::new(wt.clone(), 375.0, RATE);
            o.position = pos;
            let x = o.render(16_384, 48_000).data;
            let (mags, bin) = spectrum(&x, RATE);
            level_near(&mags, bin, 1125.0, 2) / level_near(&mags, bin, 375.0, 2)
        };
        assert!(third(0.0) < 1e-3, "sine has no third harmonic");
        assert!(
            (third(3.0) - 1.0 / 3.0).abs() < 0.03,
            "square: {}",
            third(3.0)
        );
        let mid = third(2.5);
        assert!(mid > third(0.0) && mid < 0.4, "a blend: {mid}");
    }

    #[test]
    fn from_cycles_round_trips_a_drawn_wave() {
        // A drawn 64-sample saw-ish cycle.
        let cyc: Vec<f32> = (0..64).map(|i| 1.0 - 2.0 * (i as f32 / 64.0)).collect();
        let wt = Wavetable::from_cycles(&[cyc]);
        assert_eq!(wt.frames(), 1);
        // Low pitch: shape preserved (normalised peak ≈ 1, falling).
        let a = wt.sample(0.0, 0.25, 0.001);
        let b = wt.sample(0.0, 0.75, 0.001);
        assert!(a > 0.3 && b < -0.3, "{a} {b}");
    }

    #[test]
    fn mip_levels_are_loudness_continuous() {
        let wt = Wavetable::classic();
        // Saw frame, compare level 0 and level 1 RMS.
        let rms = |lv: usize| {
            let s: f64 = (0..TABLE_SIZE)
                .map(|i| {
                    #[expect(clippy::cast_precision_loss, reason = "test")]
                    let t = i as f64 / TABLE_SIZE as f64;
                    f64::from(wt.read(2, lv, t)).powi(2)
                })
                .sum();
            #[expect(clippy::cast_precision_loss, reason = "test")]
            (s / TABLE_SIZE as f64).sqrt()
        };
        let r0 = rms(0);
        let r1 = rms(1);
        assert!((r0 - r1).abs() / r0 < 0.02, "{r0} {r1}");
    }
}
