//! Deterministic randomness for noise, sample-and-hold, grain scatter and
//! round-robin — seeded, so a mixdown that uses any of them is still
//! byte-for-byte reproducible.
//!
//! SplitMix64: a 64-bit state, one add and three multiply-xorshift rounds per
//! value, and statistical quality far beyond what audio noise can reveal.
//! [`hash`] is the same mixer used statelessly, which is what lets a
//! sample-and-hold LFO be a *pure function of its phase* (cycle `k`'s value
//! is `hash(seed, k)`) rather than a stream whose value depends on how many
//! times it was asked.

/// A seeded SplitMix64 generator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// A generator from a seed. Equal seeds give equal streams.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next 64 random bits.
    pub const fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.state)
    }

    /// Uniform in `[0, 1)`.
    pub fn next_f64(&mut self) -> f64 {
        unit(self.next_u64())
    }

    /// Uniform in `[-1, 1)`.
    #[expect(clippy::cast_possible_truncation, reason = "audio-rate noise is f32")]
    pub fn bipolar(&mut self) -> f32 {
        (self.next_f64() * 2.0 - 1.0) as f32
    }

    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }

    /// A standard normal deviate (Box–Muller).
    pub fn gaussian(&mut self) -> f64 {
        let u1 = self.next_f64().max(f64::MIN_POSITIVE);
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }

    /// An index in `0..n` (0 when `n` is 0).
    #[expect(
        clippy::cast_possible_truncation,
        reason = "n is a usize to begin with"
    )]
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }
}

const fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[expect(clippy::cast_precision_loss, reason = "53 bits is the f64 mantissa")]
fn unit(bits: u64) -> f64 {
    (bits >> 11) as f64 / (1u64 << 53) as f64
}

/// A stateless hash of `(seed, index)` to `[0, 1)`.
#[must_use]
pub fn hash(seed: u64, index: u64) -> f64 {
    unit(mix(
        seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ index.wrapping_add(0xD1B5_4A32_D192_ED03)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_seeds_equal_streams() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn uniform_mean_and_range() {
        let mut r = Rng::new(1);
        let n = 100_000;
        let mut sum = 0.0;
        for _ in 0..n {
            let v = r.next_f64();
            assert!((0.0..1.0).contains(&v));
            sum += v;
        }
        assert!((sum / f64::from(n) - 0.5).abs() < 0.01);
    }

    #[test]
    fn gaussian_has_unit_variance() {
        let mut r = Rng::new(3);
        let n = 50_000;
        let xs: Vec<f64> = (0..n).map(|_| r.gaussian()).collect();
        let mean = xs.iter().sum::<f64>() / f64::from(n);
        let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / f64::from(n);
        assert!(
            mean.abs() < 0.02 && (var - 1.0).abs() < 0.03,
            "{mean} {var}"
        );
    }

    #[test]
    fn hash_is_stable_and_spread() {
        assert_eq!(hash(5, 9), hash(5, 9));
        assert_ne!(hash(5, 9), hash(5, 10));
        assert_ne!(hash(5, 9), hash(6, 9));
    }
}
