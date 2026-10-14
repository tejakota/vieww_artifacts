//! A complex FFT in `f64`, shared by everything in this crate that needs a
//! transform at more precision than a spectrum display does.
//!
//! [`analysis`](crate::analysis) keeps its own `f32` transform: a spectrum
//! analyser's bars are read by eyes, and `f32` is what it hands them. The
//! callers *here* are different. A band-limited wavetable is built by
//! zeroing harmonics and transforming back, a convolution reverb multiplies
//! two spectra and transforms back, a linear-phase EQ designs its kernel in
//! the frequency domain — every one of them is a *round trip*, and a round
//! trip in `f32` leaves a noise floor around −140 dB that a long reverb tail
//! sums into something audible. `f64` puts the floor where nobody hears it.
//!
//! Radix-2, iterative, in place, with precomputed twiddles in [`Plan`] so a
//! block convolver that transforms the same size thousands of times computes
//! its sines once.

use std::f64::consts::PI;

/// A complex number. Deliberately minimal: what an FFT and a spectral
/// multiply need, and nothing a general numeric library would add.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Complex {
    /// Real part.
    pub re: f64,
    /// Imaginary part.
    pub im: f64,
}

impl Complex {
    /// Zero.
    pub const ZERO: Self = Self { re: 0.0, im: 0.0 };

    /// A complex number from its parts.
    #[must_use]
    pub const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    /// `e^{iθ}`.
    #[must_use]
    pub fn from_angle(theta: f64) -> Self {
        Self::new(theta.cos(), theta.sin())
    }

    /// The magnitude `|z|`.
    #[must_use]
    pub fn norm(self) -> f64 {
        self.re.hypot(self.im)
    }

    /// The conjugate.
    #[must_use]
    pub const fn conj(self) -> Self {
        Self::new(self.re, -self.im)
    }

    /// Scale by a real.
    #[must_use]
    pub fn scale(self, k: f64) -> Self {
        Self::new(self.re * k, self.im * k)
    }
}

impl std::ops::Add for Complex {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.re + o.re, self.im + o.im)
    }
}

impl std::ops::Sub for Complex {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.re - o.re, self.im - o.im)
    }
}

impl std::ops::Mul for Complex {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        Self::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
}

impl std::ops::AddAssign for Complex {
    fn add_assign(&mut self, o: Self) {
        self.re += o.re;
        self.im += o.im;
    }
}

/// Precomputed twiddles and bit-reversal for one transform size.
#[derive(Debug, Clone)]
pub struct Plan {
    n: usize,
    twiddles: Vec<Complex>,
    reversed: Vec<usize>,
}

impl Plan {
    /// A plan for size `n`.
    ///
    /// # Panics
    /// If `n` is not a power of two (the radix-2 butterfly's requirement).
    #[must_use]
    pub fn new(n: usize) -> Self {
        assert!(n.is_power_of_two(), "FFT size {n} is not a power of two");
        let bits = n.trailing_zeros();
        let reversed = (0..n)
            .map(|i| {
                if bits == 0 {
                    0
                } else {
                    i.reverse_bits() >> (usize::BITS - bits)
                }
            })
            .collect();
        #[expect(clippy::cast_precision_loss, reason = "FFT sizes are far below 2^52")]
        let twiddles = (0..n / 2)
            .map(|k| Complex::from_angle(-2.0 * PI * k as f64 / n as f64))
            .collect();
        Self {
            n,
            twiddles,
            reversed,
        }
    }

    /// The transform size.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.n
    }

    /// Always false — a plan has at least one point.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Forward transform in place: `X[k] = Σ x[n] e^{-2πikn/N}`.
    pub fn forward(&self, data: &mut [Complex]) {
        self.transform(data, false);
    }

    /// Inverse transform in place, *including* the `1/N` scale, so
    /// `inverse(forward(x)) == x`.
    pub fn inverse(&self, data: &mut [Complex]) {
        self.transform(data, true);
        #[expect(clippy::cast_precision_loss, reason = "FFT sizes are far below 2^52")]
        let k = 1.0 / self.n as f64;
        for z in data.iter_mut() {
            *z = z.scale(k);
        }
    }

    fn transform(&self, data: &mut [Complex], inverse: bool) {
        assert_eq!(data.len(), self.n, "buffer does not match the plan size");
        for i in 0..self.n {
            let j = self.reversed[i];
            if i < j {
                data.swap(i, j);
            }
        }
        let mut size = 2;
        while size <= self.n {
            let half = size / 2;
            let stride = self.n / size;
            for start in (0..self.n).step_by(size) {
                for k in 0..half {
                    let mut w = self.twiddles[k * stride];
                    if inverse {
                        w = w.conj();
                    }
                    let a = data[start + k];
                    let b = data[start + k + half] * w;
                    data[start + k] = a + b;
                    data[start + k + half] = a - b;
                }
            }
            size *= 2;
        }
    }
}

/// The inverse MDCT shared by the Vorbis and AAC decoders:
///
/// `y[n] = Σ_{k<N/2} X[k]·cos(2π/N·(n + n₀)(k + ½))`, `n₀ = (N/2 + 1)/2`,
/// for `n` in `0..N` — both specifications' kernel (AAC additionally
/// scales by `2/N`, which the caller applies). Computed through one
/// `N`-point complex FFT: `y[n] = Re(e^{iπ(n+n₀)/N} · Σ_k X[k]e^{i2πn₀k/N}
/// e^{i2πnk/N})`, so the cost is `O(N log N)`.
#[derive(Debug, Clone)]
pub struct Imdct {
    n: usize,
    plan: Plan,
    pre: Vec<Complex>,
    post: Vec<Complex>,
}

impl Imdct {
    /// An IMDCT producing `n` outputs from `n/2` coefficients.
    ///
    /// # Panics
    /// If `n` is not a power of two.
    #[must_use]
    pub fn new(n: usize) -> Self {
        #[expect(clippy::cast_precision_loss, reason = "transform sizes")]
        let nf = n as f64;
        let n0 = (nf / 2.0 + 1.0) / 2.0;
        #[expect(clippy::cast_precision_loss, reason = "transform sizes")]
        let pre = (0..n / 2)
            .map(|k| Complex::from_angle(2.0 * PI * n0 * k as f64 / nf))
            .collect();
        #[expect(clippy::cast_precision_loss, reason = "transform sizes")]
        let post = (0..n)
            .map(|i| Complex::from_angle(PI * (i as f64 + n0) / nf).scale(nf))
            .collect();
        Self {
            n,
            plan: Plan::new(n),
            pre,
            post,
        }
    }

    /// Output length.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.n
    }

    /// Always false.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Transform `x` (`n/2` values) into `y` (`n` values).
    pub fn run(&self, x: &[f32], y: &mut [f32]) {
        let mut buf = vec![Complex::ZERO; self.n];
        for (k, (b, v)) in buf.iter_mut().zip(x).enumerate().take(self.n / 2) {
            *b = self.pre[k].scale(f64::from(*v));
        }
        // Σ a[k] e^{+i2πnk/N} = N · inverse(a)[n]; `post` carries the N.
        self.plan.inverse(&mut buf);
        for (i, o) in y.iter_mut().enumerate().take(self.n) {
            #[expect(clippy::cast_possible_truncation, reason = "f32 audio")]
            {
                *o = (self.post[i] * buf[i]).re as f32;
            }
        }
    }
}

/// One-shot forward transform of a real signal, zero-padded to `n`.
#[must_use]
pub fn real_forward(signal: &[f64], n: usize) -> Vec<Complex> {
    let plan = Plan::new(n);
    let mut buf: Vec<Complex> = (0..n)
        .map(|i| Complex::new(signal.get(i).copied().unwrap_or(0.0), 0.0))
        .collect();
    plan.forward(&mut buf);
    buf
}

/// The next power of two at or above `n` (1 for 0).
#[must_use]
pub const fn next_pow2(n: usize) -> usize {
    if n <= 1 {
        1
    } else {
        n.next_power_of_two()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_is_identity() {
        let plan = Plan::new(64);
        let original: Vec<Complex> = (0..64)
            .map(|i| Complex::new((f64::from(i) * 0.37).sin(), (f64::from(i) * 0.11).cos()))
            .collect();
        let mut data = original.clone();
        plan.forward(&mut data);
        plan.inverse(&mut data);
        for (a, b) in data.iter().zip(&original) {
            assert!((a.re - b.re).abs() < 1e-12 && (a.im - b.im).abs() < 1e-12);
        }
    }

    #[test]
    fn a_cosine_lands_in_two_bins() {
        let n = 32;
        let sig: Vec<f64> = (0..n)
            .map(|i| (2.0 * PI * 3.0 * f64::from(i) / f64::from(n)).cos())
            .collect();
        let spec = real_forward(&sig, 32);
        assert!((spec[3].re - 16.0).abs() < 1e-9);
        assert!((spec[29].re - 16.0).abs() < 1e-9);
        for (k, z) in spec.iter().enumerate() {
            if k != 3 && k != 29 {
                assert!(z.norm() < 1e-9, "leak at {k}");
            }
        }
    }

    #[test]
    fn matches_a_direct_dft() {
        let n = 16;
        let x: Vec<f64> = (0..n).map(|i| f64::from(i * i % 7) - 3.0).collect();
        let fast = real_forward(&x, 16);
        for (k, got) in fast.iter().enumerate() {
            let mut want = Complex::ZERO;
            for (i, v) in x.iter().enumerate() {
                #[expect(clippy::cast_precision_loss, reason = "tiny test sizes")]
                let th = -2.0 * PI * (k * i) as f64 / f64::from(n);
                want += Complex::from_angle(th).scale(*v);
            }
            assert!((got.re - want.re).abs() < 1e-9 && (got.im - want.im).abs() < 1e-9);
        }
    }

    #[test]
    fn imdct_matches_its_definition() {
        for n in [16usize, 64, 256] {
            let m = Imdct::new(n);
            let x: Vec<f32> = (0..n / 2)
                .map(|k| ((k * 7 % 11) as f32 - 5.0) * 0.1)
                .collect();
            let mut y = vec![0.0; n];
            m.run(&x, &mut y);
            let nf = n as f64;
            let n0 = (nf / 2.0 + 1.0) / 2.0;
            for (i, got) in y.iter().enumerate() {
                let want: f64 = x
                    .iter()
                    .enumerate()
                    .map(|(k, v)| {
                        f64::from(*v) * (2.0 * PI / nf * (i as f64 + n0) * (k as f64 + 0.5)).cos()
                    })
                    .sum();
                assert!(
                    (f64::from(*got) - want).abs() < 1e-4,
                    "n={n} i={i}: {got} vs {want}"
                );
            }
        }
    }

    #[test]
    fn size_one_is_a_no_op() {
        let plan = Plan::new(1);
        let mut d = [Complex::new(2.5, -1.0)];
        plan.forward(&mut d);
        assert_eq!(d[0], Complex::new(2.5, -1.0));
    }
}
