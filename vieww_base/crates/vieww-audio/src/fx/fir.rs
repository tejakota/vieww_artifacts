//! FIR filtering: Kaiser-windowed design, a direct-form filter, and a
//! uniformly partitioned FFT convolver for long kernels.
//!
//! Three effects need finite impulse responses: oversampling (a half-band
//! low-pass), linear-phase EQ (a kernel designed in the frequency domain),
//! and convolution reverb (a measured room, seconds long). Short kernels run
//! direct; long ones run through [`Convolver`] — uniformly partitioned
//! overlap-save, the standard real-time convolution scheme: the kernel is
//! cut into blocks, each block transformed once, and every input block is
//! multiplied against the stack of past input spectra. Cost per sample is
//! `O(log B + P)` instead of `O(len)`, so a three-second hall at 48 kHz costs
//! a few hundred multiply-adds per sample instead of 144,000.

use crate::dsp::Processor;
use crate::fft::{Complex, Plan};

/// The zeroth-order modified Bessel function of the first kind (series).
#[must_use]
pub fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let half = x / 2.0;
    for k in 1..64 {
        term *= half / f64::from(k);
        let t2 = term * term;
        sum += t2;
        if t2 < sum * 1e-17 {
            break;
        }
    }
    sum
}

/// A Kaiser window of length `n` with shape `beta`.
#[must_use]
pub fn kaiser(n: usize, beta: f64) -> Vec<f64> {
    if n == 1 {
        return vec![1.0];
    }
    let denom = bessel_i0(beta);
    #[expect(clippy::cast_precision_loss, reason = "window lengths")]
    let m = (n - 1) as f64;
    (0..n)
        .map(|i| {
            #[expect(clippy::cast_precision_loss, reason = "window lengths")]
            let r = 2.0 * i as f64 / m - 1.0;
            bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / denom
        })
        .collect()
}

/// Kaiser β for a stopband attenuation of `db` dB (Kaiser's formula).
#[must_use]
pub fn kaiser_beta(db: f64) -> f64 {
    if db > 50.0 {
        0.1102 * (db - 8.7)
    } else if db >= 21.0 {
        0.5842 * (db - 21.0).powf(0.4) + 0.07886 * (db - 21.0)
    } else {
        0.0
    }
}

/// A windowed-sinc low-pass: `taps` coefficients (odd for a centred,
/// symmetric kernel), cutoff `fc` as a fraction of the sample rate
/// (0..0.5), Kaiser β. Unity DC gain.
#[must_use]
pub fn lowpass(taps: usize, fc: f64, beta: f64) -> Vec<f64> {
    let w = kaiser(taps, beta);
    #[expect(clippy::cast_precision_loss, reason = "tap counts")]
    let mid = (taps - 1) as f64 / 2.0;
    let mut h: Vec<f64> = (0..taps)
        .map(|i| {
            #[expect(clippy::cast_precision_loss, reason = "tap counts")]
            let x = i as f64 - mid;
            let s = if x.abs() < 1e-12 {
                2.0 * fc
            } else {
                (2.0 * std::f64::consts::PI * fc * x).sin() / (std::f64::consts::PI * x)
            };
            s * w[i]
        })
        .collect();
    let sum: f64 = h.iter().sum();
    for v in &mut h {
        *v /= sum;
    }
    h
}

/// A direct-form FIR filter.
#[derive(Debug, Clone, PartialEq)]
pub struct Fir {
    taps: Vec<f32>,
    hist: Vec<f32>,
    pos: usize,
}

impl Fir {
    /// A filter with these coefficients.
    #[must_use]
    pub fn new(taps: &[f64]) -> Self {
        #[expect(clippy::cast_possible_truncation, reason = "f32 audio path")]
        let taps: Vec<f32> = taps.iter().map(|v| *v as f32).collect();
        let n = taps.len().max(1);
        Self {
            taps,
            hist: vec![0.0; n * 2],
            pos: 0,
        }
    }

    /// Group delay in samples for a symmetric kernel.
    #[must_use]
    pub fn latency(&self) -> usize {
        self.taps.len().saturating_sub(1) / 2
    }
}

impl Processor for Fir {
    fn process(&mut self, x: f32) -> f32 {
        let n = self.taps.len();
        if n == 0 {
            return 0.0;
        }
        // Mirrored history: write twice so the dot product never wraps.
        self.pos = if self.pos == 0 { n - 1 } else { self.pos - 1 };
        self.hist[self.pos] = x;
        self.hist[self.pos + n] = x;
        crate::simd::dot(&self.hist[self.pos..self.pos + n], &self.taps)
    }
}

/// Uniformly partitioned overlap-save convolution.
#[derive(Debug, Clone)]
pub struct Convolver {
    block: usize,
    plan: Plan,
    parts: Vec<Vec<Complex>>,
    fdl: Vec<Vec<Complex>>,
    fdl_head: usize,
    prev: Vec<f64>,
    input: Vec<f64>,
    output: Vec<f64>,
    out_pos: usize,
    acc: Vec<Complex>,
    /// Scratch for the input FFT, allocated once — `run_block` used to
    /// collect a fresh `Vec` every block and drop the one it displaced
    /// from the frequency-delay line: an allocate-and-free per block in
    /// the audio path.
    fft_in: Vec<Complex>,
}

impl Convolver {
    /// Clear every buffer to silence, keeping the partitioned IR.
    pub fn clear(&mut self) {
        for p in &mut self.fdl {
            p.fill(crate::fft::Complex::ZERO);
        }
        self.prev.fill(0.0);
        self.input.fill(0.0);
        self.output.fill(0.0);
        self.acc.fill(crate::fft::Complex::ZERO);
    }

    /// A convolver for kernel `h` processing in blocks of `block` samples
    /// (a power of two; this is also its latency).
    ///
    /// # Panics
    /// If `block` is not a power of two.
    #[must_use]
    pub fn new(h: &[f32], block: usize) -> Self {
        assert!(block.is_power_of_two(), "block must be a power of two");
        let n = block * 2;
        let plan = Plan::new(n);
        let parts: Vec<Vec<Complex>> = h
            .chunks(block)
            .map(|c| {
                let mut buf = vec![Complex::ZERO; n];
                for (d, s) in buf.iter_mut().zip(c) {
                    *d = Complex::new(f64::from(*s), 0.0);
                }
                plan.forward(&mut buf);
                buf
            })
            .collect();
        let p = parts.len().max(1);
        Self {
            block,
            plan,
            fdl: vec![vec![Complex::ZERO; n]; p],
            parts,
            fdl_head: 0,
            prev: vec![0.0; block],
            input: Vec::with_capacity(block),
            output: vec![0.0; block],
            out_pos: 0,
            acc: vec![Complex::ZERO; n],
            fft_in: vec![Complex::ZERO; n],
        }
    }

    /// The latency in samples (one block).
    #[must_use]
    pub const fn latency(&self) -> usize {
        self.block
    }

    fn run_block(&mut self) {
        let n = self.block * 2;
        // The previous block's tail then this block's, into the scratch
        // that was allocated once — not a fresh Vec per block.
        for (d, s) in self.fft_in.iter_mut().zip(&self.prev) {
            *d = Complex::new(*s, 0.0);
        }
        for (d, s) in self.fft_in[self.block..].iter_mut().zip(&self.input) {
            *d = Complex::new(*s, 0.0);
        }
        self.plan.forward(&mut self.fft_in);
        let p = self.fdl.len();
        self.fdl_head = (self.fdl_head + p - 1) % p;
        self.fdl[self.fdl_head].copy_from_slice(&self.fft_in);
        self.acc.fill(Complex::ZERO);
        for (k, h) in self.parts.iter().enumerate() {
            let x = &self.fdl[(self.fdl_head + k) % p];
            for i in 0..n {
                self.acc[i] += x[i] * h[i];
            }
        }
        self.plan.inverse(&mut self.acc);
        for i in 0..self.block {
            self.output[i] = self.acc[self.block + i].re;
        }
        self.prev.copy_from_slice(&self.input);
        self.input.clear();
    }
}

impl Processor for Convolver {
    fn process(&mut self, x: f32) -> f32 {
        let y = self.output[self.out_pos];
        self.input.push(f64::from(x));
        self.out_pos += 1;
        if self.input.len() == self.block {
            self.run_block();
            self.out_pos = 0;
        }
        #[expect(clippy::cast_possible_truncation, reason = "f32 audio path")]
        let out = y as f32;
        out
    }
}

/// Direct convolution (reference and short kernels): `len(x) + len(h) − 1`.
#[must_use]
pub fn convolve(x: &[f32], h: &[f32]) -> Vec<f32> {
    if x.is_empty() || h.is_empty() {
        return Vec::new();
    }
    let mut y = vec![0.0f64; x.len() + h.len() - 1];
    for (i, a) in x.iter().enumerate() {
        for (j, b) in h.iter().enumerate() {
            y[i + j] += f64::from(*a) * f64::from(*b);
        }
    }
    #[expect(clippy::cast_possible_truncation, reason = "f32 audio path")]
    y.into_iter().map(|v| v as f32).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    #[test]
    fn bessel_i0_known_values() {
        assert!((bessel_i0(0.0) - 1.0).abs() < 1e-15);
        assert!((bessel_i0(1.0) - 1.266_065_877_752_008).abs() < 1e-12);
        assert!((bessel_i0(5.0) - 27.239_871_823_604_45).abs() < 1e-9);
    }

    #[test]
    fn lowpass_passes_dc_and_stops_the_stopband() {
        let h = lowpass(127, 0.2, kaiser_beta(80.0));
        let resp = |f: f64| {
            let mut re = 0.0;
            let mut im = 0.0;
            for (i, v) in h.iter().enumerate() {
                #[expect(clippy::cast_precision_loss, reason = "test")]
                let th = -2.0 * std::f64::consts::PI * f * i as f64;
                re += v * th.cos();
                im += v * th.sin();
            }
            (re * re + im * im).sqrt()
        };
        assert!((resp(0.0) - 1.0).abs() < 1e-9);
        assert!((resp(0.1) - 1.0).abs() < 1e-3);
        assert!(20.0 * resp(0.3).log10() < -75.0, "{}", resp(0.3));
    }

    #[test]
    fn convolver_matches_direct_convolution() {
        let mut r = Rng::new(4);
        let x: Vec<f32> = (0..3000).map(|_| r.bipolar()).collect();
        let h: Vec<f32> = (0..700)
            .map(|i| r.bipolar() * (-(i as f32) / 200.0).exp())
            .collect();
        let want = convolve(&x, &h);
        let mut c = Convolver::new(&h, 64);
        let mut got = Vec::new();
        for v in x.iter().chain(std::iter::repeat_n(&0.0, 64 + 700)) {
            got.push(c.process(*v));
        }
        let lat = c.latency();
        for i in 0..want.len() {
            assert!(
                (got[i + lat] - want[i]).abs() < 1e-4,
                "{i}: {} vs {}",
                got[i + lat],
                want[i]
            );
        }
    }

    #[test]
    fn fir_is_a_direct_convolution() {
        let h = [0.5, -0.25, 0.125];
        let mut f = Fir::new(&h);
        let x = [1.0, 2.0, 0.0, -1.0];
        let y: Vec<f32> = x.iter().map(|v| f.process(*v)).collect();
        #[expect(clippy::cast_possible_truncation, reason = "test")]
        let want = convolve(&x, &h.map(|v| v as f32));
        for i in 0..4 {
            assert!((y[i] - want[i]).abs() < 1e-6);
        }
    }
}
