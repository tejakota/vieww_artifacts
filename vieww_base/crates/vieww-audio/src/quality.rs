//! Measurement: the numbers an audio analyser reports.
//!
//! * [`fit_sines`] — least-squares fit of sinusoids at known frequencies
//!   (IEEE 1057's approach, generalised to several tones), which is what
//!   makes the rest exact rather than window-limited: a fitted amplitude
//!   has no leakage and no scalloping loss.
//! * [`analyze_tone`] — THD, THD+N, SINAD, SNR and ENOB of a recorded tone.
//! * [`smpte_imd`] — SMPTE RP 120 intermodulation distortion (60 Hz +
//!   7 kHz at 4:1).
//! * [`dynamic_range`] — AES17 dynamic range: a −60 dBFS 997 Hz tone
//!   through the system under test, noise referred to full scale,
//!   optionally A-weighted.
//! * [`AWeighting`] — the IEC 61672 A-weighting curve as a digital filter
//!   (bilinear transform with pre-warped poles).
//! * [`log_sweep`] and [`impulse_response`] — Farina's exponential-sweep
//!   method: excite, record, deconvolve; the impulse response gives the
//!   frequency response ([`response_db`]) and, because harmonics of an
//!   exponential sweep land at fixed times before the linear response
//!   ([`harmonic_offset`]), the distortion products separated by order.
//! * [`latency`] — the delay between a stimulus and its recording, by
//!   cross-correlation with sub-sample interpolation.
//! * [`band_levels`] — fractional-octave band levels (IEC 61260 base-10
//!   centres), the real-time analyser's bars.
//! * [`null_depth_db`] — how well two renders cancel.

use crate::fft::{next_pow2, Complex, Plan};

/// The result of fitting sinusoids.
#[derive(Debug, Clone, PartialEq)]
pub struct SineFit {
    /// DC offset.
    pub dc: f64,
    /// Peak amplitude of each frequency.
    pub amplitudes: Vec<f64>,
    /// Phase of each (radians, sine phase at sample 0).
    pub phases: Vec<f64>,
    /// RMS of what the fit leaves.
    pub residual_rms: f64,
}

/// Solve `a x = b` in place (Gaussian elimination, partial pivoting).
fn solve(a: &mut [Vec<f64>], b: &mut [f64]) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let piv = (col..n).max_by(|x, y| a[*x][col].abs().total_cmp(&a[*y][col].abs()))?;
        if a[piv][col].abs() < 1e-300 {
            return None;
        }
        a.swap(col, piv);
        b.swap(col, piv);
        for row in col + 1..n {
            let f = a[row][col] / a[col][col];
            if f != 0.0 {
                let (top, rest) = a.split_at_mut(row);
                for (x, y) in rest[0][col..n].iter_mut().zip(&top[col][col..n]) {
                    *x -= f * y;
                }
                b[row] -= f * b[col];
            }
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let s: f64 = (row + 1..n).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - s) / a[row][row];
    }
    Some(x)
}

/// Least-squares fit of `dc + Σ A_k sin(2π f_k n / rate + φ_k)`.
/// Frequencies at or above Nyquist get amplitude 0.
#[must_use]
pub fn fit_sines(x: &[f32], rate: f64, freqs: &[f64]) -> SineFit {
    let usable: Vec<usize> = (0..freqs.len())
        .filter(|i| freqs[*i] > 0.0 && freqs[*i] < rate / 2.0)
        .collect();
    let m = 1 + 2 * usable.len();
    let mut ata = vec![vec![0.0; m]; m];
    let mut atb = vec![0.0; m];
    // Phasors advanced by recurrence, renormalised now and then.
    let steps: Vec<Complex> = usable
        .iter()
        .map(|i| Complex::from_angle(std::f64::consts::TAU * freqs[*i] / rate))
        .collect();
    let mut ph: Vec<Complex> = vec![Complex::new(1.0, 0.0); usable.len()];
    let mut basis = vec![0.0; m];
    for (n, &s) in x.iter().enumerate() {
        basis[0] = 1.0;
        for (k, p) in ph.iter().enumerate() {
            basis[1 + 2 * k] = p.im; // sin
            basis[2 + 2 * k] = p.re; // cos
        }
        let y = f64::from(s);
        for r in 0..m {
            atb[r] += basis[r] * y;
            for c in r..m {
                ata[r][c] += basis[r] * basis[c];
            }
        }
        for (p, st) in ph.iter_mut().zip(&steps) {
            *p = *p * *st;
        }
        if n % 1_024 == 1_023 {
            for p in &mut ph {
                let k = 1.0 / p.norm();
                *p = p.scale(k);
            }
        }
    }
    for r in 1..m {
        let (top, rest) = ata.split_at_mut(r);
        for (c, v) in rest[0][..r].iter_mut().enumerate() {
            *v = top[c][r];
        }
    }
    let coef = solve(&mut ata, &mut atb).unwrap_or_else(|| vec![0.0; m]);
    let mut amplitudes = vec![0.0; freqs.len()];
    let mut phases = vec![0.0; freqs.len()];
    for (k, &i) in usable.iter().enumerate() {
        let (s, c) = (coef[1 + 2 * k], coef[2 + 2 * k]);
        amplitudes[i] = s.hypot(c);
        phases[i] = c.atan2(s);
    }
    // Residual, recomputed directly for accuracy.
    let mut ph: Vec<Complex> = vec![Complex::new(1.0, 0.0); usable.len()];
    let mut sq = 0.0;
    for (n, &s) in x.iter().enumerate() {
        let mut model = coef[0];
        for (k, p) in ph.iter().enumerate() {
            model += coef[1 + 2 * k] * p.im + coef[2 + 2 * k] * p.re;
        }
        let e = f64::from(s) - model;
        sq += e * e;
        for (p, st) in ph.iter_mut().zip(&steps) {
            *p = *p * *st;
        }
        if n % 1_024 == 1_023 {
            for p in &mut ph {
                let k = 1.0 / p.norm();
                *p = p.scale(k);
            }
        }
    }
    #[expect(clippy::cast_precision_loss, reason = "sample counts")]
    let residual_rms = (sq / x.len().max(1) as f64).sqrt();
    SineFit {
        dc: coef[0],
        amplitudes,
        phases,
        residual_rms,
    }
}

/// Estimate a tone's frequency: FFT peak, parabolic interpolation on the
/// log magnitude, then a least-squares refinement.
#[must_use]
pub fn estimate_frequency(x: &[f32], rate: f64) -> f64 {
    let n = next_pow2(x.len()).min(1 << 20);
    let plan = Plan::new(n);
    #[expect(clippy::cast_precision_loss, reason = "window index")]
    let mut buf: Vec<Complex> = (0..n)
        .map(|i| {
            let v = x.get(i).copied().map_or(0.0, f64::from);
            let w = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos();
            Complex::new(v * w, 0.0)
        })
        .collect();
    plan.forward(&mut buf);
    let mag: Vec<f64> = buf[..n / 2]
        .iter()
        .map(|c| c.norm().max(1e-300).ln())
        .collect();
    let k = (1..n / 2 - 1)
        .max_by(|a, b| mag[*a].total_cmp(&mag[*b]))
        .unwrap_or(1);
    let (a, b, c) = (mag[k - 1], mag[k], mag[k + 1]);
    let d = 0.5 * (a - c) / (a - 2.0 * b + c);
    #[expect(clippy::cast_precision_loss, reason = "bin index")]
    let coarse = (k as f64 + d) * rate / n as f64;
    // Refine: the residual of a single-tone fit is minimal at the truth.
    let resid = |f: f64| fit_sines(x, rate, &[f]).residual_rms;
    #[expect(clippy::cast_precision_loss, reason = "bin width")]
    let (mut lo, mut hi) = (coarse - rate / n as f64, coarse + rate / n as f64);
    for _ in 0..40 {
        let m1 = lo + (hi - lo) / 3.0;
        let m2 = hi - (hi - lo) / 3.0;
        if resid(m1) < resid(m2) {
            hi = m2;
        } else {
            lo = m1;
        }
    }
    (lo + hi) / 2.0
}

/// Distortion and noise figures of a tone.
#[derive(Debug, Clone, PartialEq)]
pub struct Distortion {
    /// Fundamental peak amplitude.
    pub fundamental: f64,
    /// Harmonic peak amplitudes, from the 2nd up.
    pub harmonics: Vec<f64>,
    /// THD: harmonic RMS over fundamental RMS (ratio).
    pub thd: f64,
    /// THD+N: everything but the fundamental (and DC), over it (ratio).
    pub thd_n: f64,
    /// SINAD, dB.
    pub sinad_db: f64,
    /// SNR: fundamental over what remains after removing harmonics, dB.
    pub snr_db: f64,
    /// Effective number of bits, from SINAD (full-scale referred).
    pub enob: f64,
}

/// Ratio to dB.
#[must_use]
pub fn db(ratio: f64) -> f64 {
    20.0 * ratio.max(1e-300).log10()
}

/// Analyse a recorded tone at `f0` with `harmonics` harmonics (2nd up).
#[must_use]
pub fn analyze_tone(x: &[f32], rate: f64, f0: f64, harmonics: usize) -> Distortion {
    let fund = fit_sines(x, rate, &[f0]);
    let freqs: Vec<f64> = (1..=harmonics + 1).map(|k| f0 * k as f64).collect();
    let all = fit_sines(x, rate, &freqs);
    let a1 = all.amplitudes[0];
    let hs: Vec<f64> = all.amplitudes[1..].to_vec();
    let thd = hs.iter().map(|h| h * h).sum::<f64>().sqrt() / a1.max(1e-300);
    let fund_rms = fund.amplitudes[0] / std::f64::consts::SQRT_2;
    let thd_n = fund.residual_rms / fund_rms.max(1e-300);
    let sinad_db = -db(thd_n);
    // ENOB is referred to a full-scale tone (IEEE 1241).
    let full_scale_sinad = sinad_db - db(fund.amplitudes[0]);
    Distortion {
        fundamental: fund.amplitudes[0],
        harmonics: hs,
        thd,
        thd_n,
        sinad_db,
        snr_db: db(fund_rms / all.residual_rms.max(1e-300)),
        enob: (full_scale_sinad - 1.76) / 6.02,
    }
}

/// SMPTE RP 120 stimulus: 60 Hz and 7 kHz at 4:1, peak `amplitude`.
#[must_use]
pub fn smpte_signal(amplitude: f32, secs: f64, rate: f64) -> Vec<f32> {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "lengths"
    )]
    let n = (secs * rate) as usize;
    let a = f64::from(amplitude);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "audio"
    )]
    let v = (0..n)
        .map(|i| {
            let t = i as f64 / rate;
            (a * 0.8 * (std::f64::consts::TAU * 60.0 * t).sin()
                + a * 0.2 * (std::f64::consts::TAU * 7_000.0 * t).sin()) as f32
        })
        .collect();
    v
}

/// SMPTE intermodulation distortion of a recorded [`smpte_signal`]: the
/// RMS sum of the sidebands 7 kHz ± n·60 Hz (n = 1..=`orders`) over the
/// 7 kHz carrier (ratio).
#[must_use]
pub fn smpte_imd(x: &[f32], rate: f64, orders: usize) -> f64 {
    let mut freqs = vec![60.0, 120.0, 180.0, 7_000.0];
    for n in 1..=orders {
        #[expect(clippy::cast_precision_loss, reason = "order")]
        let d = 60.0 * n as f64;
        freqs.push(7_000.0 - d);
        freqs.push(7_000.0 + d);
    }
    let f = fit_sines(x, rate, &freqs);
    let side: f64 = f.amplitudes[4..].iter().map(|a| a * a).sum();
    side.sqrt() / f.amplitudes[3].max(1e-300)
}

/// A first-order section `(b1 s + b0) / (s + ω)` through the bilinear
/// transform.
#[derive(Debug, Clone, Copy, PartialEq)]
struct First {
    b: [f64; 2],
    a1: f64,
    z: f64,
}

impl First {
    fn new(b1: f64, b0: f64, w: f64, k: f64) -> Self {
        let a0 = k + w;
        Self {
            b: [(b1 * k + b0) / a0, (b0 - b1 * k) / a0],
            a1: (w - k) / a0,
            z: 0.0,
        }
    }

    fn tick(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z;
        self.z = self.b[1] * x - self.a1 * y;
        y
    }

    fn response(&self, w: f64) -> Complex {
        let z1 = Complex::from_angle(-w);
        let num = Complex::new(self.b[0], 0.0) + Complex::new(self.b[1], 0.0) * z1;
        let den = Complex::new(1.0, 0.0) + Complex::new(self.a1, 0.0) * z1;
        let d = den.re * den.re + den.im * den.im;
        let q = num * den.conj();
        Complex::new(q.re / d, q.im / d)
    }
}

/// The IEC 61672 A-weighting filter.
#[derive(Debug, Clone, PartialEq)]
pub struct AWeighting {
    secs: [First; 6],
    gain: f64,
    rate: f64,
}

impl AWeighting {
    /// A-weighting at `rate`, normalised to 0 dB at 1 kHz.
    ///
    /// The three low poles go through the bilinear transform (at their
    /// frequencies its warping is negligible). The double pole at 12.2 kHz
    /// is mapped by matched-z instead, with the zero of each section placed
    /// — solved here, per rate — to minimise the worst deviation from the
    /// analog curve between 1 kHz and 20 kHz (or 0.45·rate). At 48 kHz
    /// that is within 0.06 dB at 4 kHz and 0.31 dB at 10 kHz, against the
    /// bilinear transform's 0.3 dB and 1.2 dB.
    #[must_use]
    pub fn new(rate: f64) -> Self {
        let k = 2.0 * rate;
        let w = |f: f64| std::f64::consts::TAU * f;
        let (w1, w2, w3) = (w(20.598_997), w(107.652_65), w(737.862_23));
        let p = (-w(12_194.217) / rate).exp();
        let build = |c: f64| {
            let lp = First {
                b: [1.0, c],
                a1: -p,
                z: 0.0,
            };
            let mut a = Self {
                secs: [
                    First::new(1.0, 0.0, w1, k),
                    First::new(1.0, 0.0, w1, k),
                    First::new(1.0, 0.0, w2, k),
                    First::new(1.0, 0.0, w3, k),
                    lp,
                    lp,
                ],
                gain: 1.0,
                rate,
            };
            a.gain = 1.0 / a.magnitude(1_000.0);
            a
        };
        let top = (rate * 0.45).min(20_000.0);
        let probes: Vec<f64> = (0..24)
            .map(|i| 1_000.0 * (top / 1_000.0).powf(f64::from(i) / 23.0))
            .collect();
        let worst = |a: &Self| {
            probes
                .iter()
                .map(|f| (db(a.magnitude(*f)) - db(Self::analog(*f))).abs())
                .fold(0.0, f64::max)
        };
        let (mut lo, mut hi) = (-0.5, 0.8);
        for _ in 0..60 {
            let m1 = lo + (hi - lo) / 3.0;
            let m2 = hi - (hi - lo) / 3.0;
            if worst(&build(m1)) < worst(&build(m2)) {
                hi = m2;
            } else {
                lo = m1;
            }
        }
        build((lo + hi) / 2.0)
    }

    /// The analog A-weighting curve (linear, 0 dB at 1 kHz) — the
    /// standard's definition, for comparison.
    #[must_use]
    pub fn analog(hz: f64) -> f64 {
        let f2 = hz * hz;
        let ra = |f2: f64| {
            12_194.217f64.powi(2) * f2 * f2
                / ((f2 + 20.598_997f64.powi(2))
                    * ((f2 + 107.652_65f64.powi(2)) * (f2 + 737.862_23f64.powi(2))).sqrt()
                    * (f2 + 12_194.217f64.powi(2)))
        };
        ra(f2) / ra(1.0e6)
    }

    /// Linear magnitude at `hz`.
    #[must_use]
    pub fn magnitude(&self, hz: f64) -> f64 {
        let w = std::f64::consts::TAU * hz / self.rate;
        self.secs
            .iter()
            .fold(Complex::new(self.gain, 0.0), |acc, s| acc * s.response(w))
            .norm()
    }

    /// Filter one sample.
    pub fn tick(&mut self, x: f32) -> f32 {
        let y = self
            .secs
            .iter_mut()
            .fold(f64::from(x) * self.gain, |v, s| s.tick(v));
        #[expect(clippy::cast_possible_truncation, reason = "audio")]
        let y = y as f32;
        y
    }

    /// Filter a buffer.
    #[must_use]
    pub fn filter(&mut self, x: &[f32]) -> Vec<f32> {
        x.iter().map(|v| self.tick(*v)).collect()
    }
}

/// AES17 dynamic range, dB: a −60 dBFS 997 Hz tone through `dut`, the
/// residual (noise and distortion) referred to a full-scale sine.
pub fn dynamic_range(mut dut: impl FnMut(&[f32]) -> Vec<f32>, rate: f64, a_weighted: bool) -> f64 {
    let amp = 10f64.powf(-60.0 / 20.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "lengths"
    )]
    let n = (rate * 2.0) as usize;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "audio"
    )]
    let stim: Vec<f32> = (0..n)
        .map(|i| (amp * (std::f64::consts::TAU * 997.0 * i as f64 / rate).sin()) as f32)
        .collect();
    let mut y = dut(&stim);
    if a_weighted {
        y = AWeighting::new(rate).filter(&y);
    }
    let steady = &y[y.len() / 4..];
    let fit = fit_sines(steady, rate, &[997.0]);
    db(std::f64::consts::FRAC_1_SQRT_2 / fit.residual_rms.max(1e-300))
}

/// An exponential (Farina) sine sweep from `f1` to `f2` Hz over `secs`.
#[must_use]
pub fn log_sweep(f1: f64, f2: f64, secs: f64, rate: f64) -> Vec<f32> {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "lengths"
    )]
    let n = (secs * rate) as usize;
    let r = (f2 / f1).ln();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "audio"
    )]
    let v = (0..n)
        .map(|i| {
            let t = i as f64 / rate;
            (std::f64::consts::TAU * f1 * secs / r * ((t * r / secs).exp() - 1.0)).sin() as f32
        })
        .collect();
    v
}

/// Where the `k`-th harmonic's impulse response lands relative to the
/// linear one, for a [`log_sweep`]: `secs · ln k / ln(f2/f1)` seconds
/// *earlier* (so, circularly, that far from the end of the response).
#[must_use]
pub fn harmonic_offset(k: u32, f1: f64, f2: f64, secs: f64) -> f64 {
    secs * f64::from(k).ln() / (f2 / f1).ln()
}

/// Deconvolve: the impulse response that turns `stimulus` into
/// `recorded` (regularised spectral division; frequencies the stimulus
/// does not contain come out as zero). Length is a power of two at least
/// `recorded.len() + stimulus.len()`.
#[must_use]
pub fn impulse_response(recorded: &[f32], stimulus: &[f32]) -> Vec<f32> {
    let n = next_pow2(recorded.len() + stimulus.len());
    let plan = Plan::new(n);
    let load = |s: &[f32]| -> Vec<Complex> {
        (0..n)
            .map(|i| Complex::new(s.get(i).copied().map_or(0.0, f64::from), 0.0))
            .collect()
    };
    let mut y = load(recorded);
    let mut x = load(stimulus);
    plan.forward(&mut y);
    plan.forward(&mut x);
    let peak = x
        .iter()
        .map(|c| c.re * c.re + c.im * c.im)
        .fold(0.0, f64::max);
    let eps = peak * 1e-8;
    let mut h: Vec<Complex> = y
        .iter()
        .zip(&x)
        .map(|(y, x)| {
            let p = x.re * x.re + x.im * x.im;
            if p < eps * 100.0 {
                Complex::new(0.0, 0.0)
            } else {
                (*y * x.conj()).scale(1.0 / (p + eps))
            }
        })
        .collect();
    plan.inverse(&mut h);
    #[expect(clippy::cast_possible_truncation, reason = "audio")]
    let v = h.iter().map(|c| c.re as f32).collect();
    v
}

/// The magnitude of an impulse response at `hz`, dB.
#[must_use]
pub fn response_db(ir: &[f32], hz: f64, rate: f64) -> f64 {
    let step = Complex::from_angle(-std::f64::consts::TAU * hz / rate);
    let mut p = Complex::new(1.0, 0.0);
    let mut acc = Complex::new(0.0, 0.0);
    for &v in ir {
        acc += p.scale(f64::from(v));
        p = p * step;
    }
    db(acc.norm())
}

/// The delay of `recorded` behind `reference`, in samples (sub-sample by
/// parabolic interpolation of the cross-correlation peak).
#[must_use]
pub fn latency(reference: &[f32], recorded: &[f32]) -> f64 {
    let n = next_pow2(reference.len() + recorded.len());
    let plan = Plan::new(n);
    let load = |s: &[f32]| -> Vec<Complex> {
        (0..n)
            .map(|i| Complex::new(s.get(i).copied().map_or(0.0, f64::from), 0.0))
            .collect()
    };
    let mut a = load(reference);
    let mut b = load(recorded);
    plan.forward(&mut a);
    plan.forward(&mut b);
    let mut r: Vec<Complex> = b.iter().zip(&a).map(|(b, a)| *b * a.conj()).collect();
    plan.inverse(&mut r);
    let v: Vec<f64> = r.iter().map(|c| c.re).collect();
    let k = (0..n).max_by(|x, y| v[*x].total_cmp(&v[*y])).unwrap_or(0);
    let (a, b, c) = (v[(k + n - 1) % n], v[k], v[(k + 1) % n]);
    let den = a - 2.0 * b + c;
    let d = if den.abs() < 1e-30 {
        0.0
    } else {
        0.5 * (a - c) / den
    };
    #[expect(clippy::cast_precision_loss, reason = "lag")]
    let lag = if k > n / 2 {
        k as f64 - n as f64
    } else {
        k as f64
    };
    lag + d
}

/// Band centre frequencies (IEC 61260, base 10) for `1/fraction` octave
/// bands between `lo` and `hi` Hz.
#[must_use]
pub fn band_centres(fraction: u32, lo: f64, hi: f64) -> Vec<f64> {
    let b = f64::from(fraction.max(1));
    let g = 10f64.powf(0.3); // the base-10 octave ratio
    let x_lo = (b * (lo / 1_000.0).log10() / g.log10()).ceil();
    let x_hi = (b * (hi / 1_000.0).log10() / g.log10()).floor();
    #[expect(clippy::cast_possible_truncation, reason = "band indices")]
    let (a, z) = (x_lo as i32, x_hi as i32);
    (a..=z)
        .map(|x| 1_000.0 * g.powf(f64::from(x) / b))
        .collect()
}

/// RMS level of each fractional-octave band, dBFS (a full-scale sine
/// reads −3.01 in its band). Computed from the power spectrum, so the
/// bands' powers sum to the signal's.
#[must_use]
pub fn band_levels(x: &[f32], rate: f64, centres: &[f64], fraction: u32) -> Vec<f64> {
    let n = next_pow2(x.len());
    let plan = Plan::new(n);
    let mut buf: Vec<Complex> = (0..n)
        .map(|i| Complex::new(x.get(i).copied().map_or(0.0, f64::from), 0.0))
        .collect();
    plan.forward(&mut buf);
    #[expect(clippy::cast_precision_loss, reason = "lengths")]
    let (nf, len) = (n as f64, x.len().max(1) as f64);
    let half_band = 10f64.powf(0.3 / (2.0 * f64::from(fraction.max(1))));
    centres
        .iter()
        .map(|fc| {
            let (lo, hi) = (fc / half_band, fc * half_band);
            let mut p = 0.0;
            for (k, c) in buf.iter().enumerate().take(n / 2 + 1).skip(1) {
                #[expect(clippy::cast_precision_loss, reason = "bin index")]
                let f = k as f64 * rate / nf;
                if f >= lo && f < hi {
                    let w = if k == n / 2 { 1.0 } else { 2.0 };
                    p += w * (c.re * c.re + c.im * c.im);
                }
            }
            // Parseval: Σ|x|² = Σ|X|²/N; mean square over the signal's length.
            10.0 * (p / nf / len).max(1e-300).log10()
        })
        .collect()
}

/// How deeply `b` cancels `a`: RMS of the difference relative to `a`, dB
/// (−∞ for bit-identical).
#[must_use]
pub fn null_depth_db(a: &[f32], b: &[f32]) -> f64 {
    let (mut d, mut r) = (0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        let e = f64::from(*x) - f64::from(*y);
        d += e * e;
        r += f64::from(*x) * f64::from(*x);
    }
    if d == 0.0 {
        f64::NEG_INFINITY
    } else {
        10.0 * (d / r.max(1e-300)).log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;

    fn tone(f: f64, a: f64, n: usize) -> Vec<f64> {
        (0..n)
            .map(|i| a * (std::f64::consts::TAU * f * i as f64 / RATE + 0.3).sin())
            .collect()
    }

    fn f32s(v: &[f64]) -> Vec<f32> {
        v.iter().map(|x| *x as f32).collect()
    }

    #[test]
    fn fit_recovers_amplitudes_phases_and_dc() {
        let x: Vec<f32> = (0..9_000)
            .map(|i| {
                let t = std::f64::consts::TAU * i as f64 / RATE;
                (0.1 + 0.5 * (1_000.3 * t + 0.4).sin() + 0.01 * (3_333.0 * t - 1.0).sin()) as f32
            })
            .collect();
        let f = fit_sines(&x, RATE, &[1_000.3, 3_333.0, 30_000.0]);
        assert!((f.dc - 0.1).abs() < 1e-6);
        assert!((f.amplitudes[0] - 0.5).abs() < 1e-6);
        assert!((f.phases[0] - 0.4).abs() < 1e-5);
        assert!((f.amplitudes[1] - 0.01).abs() < 1e-6);
        assert!((f.phases[1] + 1.0).abs() < 1e-3);
        assert_eq!(f.amplitudes[2], 0.0, "above Nyquist");
        assert!(f.residual_rms < 1e-7);
    }

    #[test]
    fn thd_of_known_polynomials() {
        // y = x + 0.1x²: H2 = 0.05 for a unit sine.
        let x = tone(997.0, 1.0, 24_000);
        let y: Vec<f32> = f32s(&x.iter().map(|v| v + 0.1 * v * v).collect::<Vec<_>>());
        let d = analyze_tone(&y, RATE, 997.0, 5);
        assert!((d.harmonics[0] - 0.05).abs() < 1e-5);
        assert!((d.thd - 0.05).abs() < 1e-4, "{}", d.thd);
        // y = x + 0.01x³: H3 = 0.0025, fundamental 1.0075.
        let y: Vec<f32> = f32s(&x.iter().map(|v| v + 0.01 * v * v * v).collect::<Vec<_>>());
        let d = analyze_tone(&y, RATE, 997.0, 5);
        assert!((d.fundamental - 1.007_5).abs() < 1e-5);
        assert!((d.thd - 0.0025 / 1.0075).abs() < 1e-5, "{}", d.thd);
        assert!(d.harmonics[0] < 1e-6);
    }

    #[test]
    fn band_limited_square_wave_thd() {
        // Odd harmonics 1/k up to the 9th: √(1/9 + 1/25 + 1/49 + 1/81).
        let n = 24_000;
        let x: Vec<f32> = (0..n)
            .map(|i| {
                let t = std::f64::consts::TAU * 500.0 * i as f64 / RATE;
                [1.0, 3.0, 5.0, 7.0, 9.0]
                    .iter()
                    .map(|k: &f64| (k * t).sin() / k)
                    .sum::<f64>() as f32
            })
            .collect();
        let d = analyze_tone(&x, RATE, 500.0, 9);
        let want = (1.0f64 / 9.0 + 1.0 / 25.0 + 1.0 / 49.0 + 1.0 / 81.0).sqrt();
        assert!((d.thd - want).abs() < 1e-5, "{} vs {want}", d.thd);
    }

    #[test]
    fn sixteen_bit_quantisation_figures() {
        // A −6 dBFS tone quantised to 16 bits: SINAD ≈ 98.08 − 6.02, ENOB ≈ 16.
        let x = tone(997.0, 0.5, 48_000);
        let q: Vec<f32> = x
            .iter()
            .map(|v| ((v * 32_768.0).round() / 32_768.0) as f32)
            .collect();
        let d = analyze_tone(&q, RATE, 997.0, 9);
        assert!((d.sinad_db - 92.06).abs() < 1.0, "{}", d.sinad_db);
        assert!((d.enob - 16.0).abs() < 0.2, "{}", d.enob);
        assert!(d.snr_db >= d.sinad_db - 0.1);
    }

    #[test]
    fn frequency_estimation_is_sub_hertz() {
        let x = f32s(&tone(1_234.567, 0.7, 8_192));
        let f = estimate_frequency(&x, RATE);
        assert!((f - 1_234.567).abs() < 1e-3, "{f}");
    }

    #[test]
    fn smpte_imd_of_a_clean_and_a_nonlinear_path() {
        let s = smpte_signal(1.0, 0.5, RATE);
        assert!(smpte_imd(&s, RATE, 3) < 1e-6);
        // y = x + 0.1x²: sidebands at 7k ± 60 of 2·0.1·0.8·0.2/2 = 0.016,
        // relative to the 0.2 carrier: 8 % each, 11.3 % together.
        let y: Vec<f32> = s.iter().map(|v| v + 0.1 * v * v).collect();
        let imd = smpte_imd(&y, RATE, 3);
        assert!(
            (imd - 0.08 * std::f64::consts::SQRT_2).abs() < 1e-3,
            "{imd}"
        );
    }

    #[test]
    fn a_weighting_matches_iec_61672() {
        let a = AWeighting::new(RATE);
        for (hz, want, tol) in [
            (31.5, -39.4, 0.2),
            (100.0, -19.1, 0.1),
            (1_000.0, 0.0, 1e-9),
            (2_000.0, 1.2, 0.1),
            (4_000.0, 1.0, 0.1),
            (10_000.0, -2.5, 0.35),
        ] {
            let got = db(a.magnitude(hz));
            assert!((got - want).abs() <= tol, "{hz} Hz: {got} vs {want}");
        }
        for f in [500.0, 3_000.0, 8_000.0] {
            assert!((db(a.magnitude(f)) - db(AWeighting::analog(f))).abs() < 0.25);
        }
        // The time-domain filter agrees with the response.
        let mut f = AWeighting::new(RATE);
        let x = f32s(&tone(100.0, 1.0, 48_000));
        let y = f.filter(&x);
        let fit = fit_sines(&y[24_000..], RATE, &[100.0]);
        assert!((db(fit.amplitudes[0]) + 19.1).abs() < 0.15);
    }

    #[test]
    fn dynamic_range_of_16_and_24_bit_paths() {
        let quantise = |bits: i32| {
            move |x: &[f32]| -> Vec<f32> {
                let s = 2f64.powi(bits - 1);
                x.iter()
                    .map(|v| ((f64::from(*v) * s).round() / s) as f32)
                    .collect()
            }
        };
        let dr16 = dynamic_range(quantise(16), RATE, false);
        assert!((dr16 - 98.08).abs() < 1.0, "{dr16}");
        // A-weighting removes some of the (white) quantisation noise.
        let dr16a = dynamic_range(quantise(16), RATE, true);
        assert!(dr16a > dr16, "{dr16a} vs {dr16}");
        let dr20 = dynamic_range(quantise(20), RATE, false);
        assert!((dr20 - dr16 - 24.08).abs() < 1.5, "{dr20}");
    }

    #[test]
    fn sweep_deconvolution_measures_a_filter() {
        use crate::dsp::{Biquad, FilterKind, Processor};
        let sweep = log_sweep(20.0, 20_000.0, 2.0, RATE);
        let mut lp = Biquad::new(FilterKind::LowPass, 2_000.0, 0.707, RATE as f32);
        let mut rec: Vec<f32> = sweep.iter().map(|x| lp.process(*x)).collect();
        rec.extend((0..4_800).map(|_| lp.process(0.0)));
        let ir = impulse_response(&rec, &sweep);
        let lp_ref = Biquad::new(FilterKind::LowPass, 2_000.0, 0.707, RATE as f32);
        for hz in [100.0, 1_000.0, 2_000.0, 5_000.0, 10_000.0] {
            let got = response_db(&ir[..8_192], hz, RATE);
            let want = db(f64::from(lp_ref.magnitude(hz as f32, RATE as f32)));
            assert!((got - want).abs() < 0.3, "{hz}: {got} vs {want}");
        }
    }

    #[test]
    fn sweep_separates_harmonics_by_time() {
        let (f1, f2, secs) = (50.0, 16_000.0, 2.0);
        let sweep = log_sweep(f1, f2, secs, RATE);
        let rec: Vec<f32> = sweep.iter().map(|v| v + 0.1 * v * v).collect();
        let ir = impulse_response(&rec, &sweep);
        let n = ir.len();
        let linear_peak = ir[..64].iter().map(|v| v.abs()).fold(0.0, f32::max);
        let off = (harmonic_offset(2, f1, f2, secs) * RATE).round() as usize;
        let h2 = ir[n - off - 64..n - off + 64]
            .iter()
            .map(|v| v.abs())
            .fold(0.0, f32::max);
        assert!(linear_peak > 0.5, "{linear_peak}");
        assert!(
            h2 > 0.005,
            "second-harmonic response found at its offset: {h2}"
        );
        // And nowhere near it otherwise.
        let quiet = ir[n / 2..n / 2 + 4_096]
            .iter()
            .map(|v| v.abs())
            .fold(0.0, f32::max);
        assert!(quiet < h2 / 5.0, "{quiet}");
    }

    #[test]
    fn latency_is_found_to_a_fraction_of_a_sample() {
        let x: Vec<f32> = {
            let mut r = crate::rng::Rng::new(9);
            (0..8_000).map(|_| r.bipolar()).collect()
        };
        let mut y = vec![0.0; 137];
        y.extend(&x);
        assert!((latency(&x, &y) - 137.0).abs() < 0.05);
        // Half-sample delay by linear interpolation of a smooth signal.
        let s = f32s(&tone(300.0, 1.0, 4_000));
        let half: Vec<f32> = (0..4_000)
            .map(|i| if i == 0 { 0.0 } else { (s[i] + s[i - 1]) / 2.0 })
            .collect();
        let l = latency(&s, &half);
        assert!((l - 0.5).abs() < 0.1, "{l}");
    }

    #[test]
    fn band_levels_put_a_tone_in_its_band() {
        let centres = band_centres(3, 25.0, 20_000.0);
        assert_eq!(centres.len(), 30);
        assert!(
            (centres[15] - 794.3).abs() < 0.1 || centres.iter().any(|c| (c - 1_000.0).abs() < 1e-9)
        );
        let x = f32s(&tone(1_000.0, 1.0, 48_000));
        let lv = band_levels(&x, RATE, &centres, 3);
        let k = centres
            .iter()
            .position(|c| (c - 1_000.0).abs() < 1e-9)
            .unwrap();
        assert!((lv[k] + 3.01).abs() < 0.05, "{}", lv[k]);
        assert!(lv[k - 2] < -40.0 && lv[k + 2] < -40.0);
        let octaves = band_centres(1, 31.0, 16_000.0);
        assert_eq!(octaves.len(), 10);
    }

    #[test]
    fn null_depth() {
        let a = f32s(&tone(440.0, 0.5, 1_000));
        assert_eq!(null_depth_db(&a, &a), f64::NEG_INFINITY);
        let b: Vec<f32> = a.iter().map(|v| v * 0.999).collect();
        assert!((null_depth_db(&a, &b) + 60.0).abs() < 0.01);
    }
}
