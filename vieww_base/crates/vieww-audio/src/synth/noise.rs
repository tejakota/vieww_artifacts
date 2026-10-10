//! Noise — the raw material of percussion, breath, wind and texture.
//!
//! Every colour is a seeded [`Rng`](crate::rng::Rng) shaped by a filter, so
//! a hi-hat rendered twice is the same hi-hat. The colours are named by their
//! spectral slope:
//!
//! | Colour | Slope | How |
//! |---|---|---|
//! | [`White`](Colour::White) | flat | uniform deviates |
//! | [`Pink`](Colour::Pink) | −3 dB/oct | Paul Kellet's refined seven-pole filter |
//! | [`Brown`](Colour::Brown) | −6 dB/oct | leaky integrator of white |
//! | [`Blue`](Colour::Blue) | +3 dB/oct | first difference of pink |
//! | [`Violet`](Colour::Violet) | +6 dB/oct | first difference of white |
//! | [`Velvet`](Colour::Velvet) | flat, sparse | ±1 impulses at a density — reverb tails, crackle |
//!
//! [`Noise::gain`] normalises each colour to roughly the same RMS so
//! switching colour does not also switch loudness.

use crate::rng::Rng;

/// A noise colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Colour {
    /// Flat spectrum.
    White,
    /// −3 dB/octave.
    Pink,
    /// −6 dB/octave (Brownian / red).
    Brown,
    /// +3 dB/octave.
    Blue,
    /// +6 dB/octave.
    Violet,
    /// Sparse random ±1 impulses, `density` per second.
    Velvet {
        /// Impulses per second.
        density: f32,
    },
}

/// A seeded noise generator.
#[derive(Debug, Clone)]
pub struct Noise {
    colour: Colour,
    rng: Rng,
    rate: f32,
    b: [f32; 7],
    brown: f32,
    prev: f32,
    gain: f32,
}

impl Noise {
    /// A noise source of `colour`, seeded, at `rate`.
    #[must_use]
    pub fn new(colour: Colour, seed: u64, rate: f32) -> Self {
        let gain = match colour {
            Colour::White => 1.0,
            Colour::Pink => 0.33,
            Colour::Brown => 3.5,
            Colour::Blue => 0.7,
            Colour::Violet => 0.5,
            Colour::Velvet { .. } => 1.0,
        };
        Self {
            colour,
            rng: Rng::new(seed),
            rate,
            b: [0.0; 7],
            brown: 0.0,
            prev: 0.0,
            gain,
        }
    }

    /// The normalising gain applied to this colour.
    #[must_use]
    pub const fn gain(&self) -> f32 {
        self.gain
    }

    fn pink(&mut self, white: f32) -> f32 {
        let b = &mut self.b;
        b[0] = 0.99886 * b[0] + white * 0.055_518;
        b[1] = 0.99332 * b[1] + white * 0.075_075_9;
        b[2] = 0.96900 * b[2] + white * 0.153_852;
        b[3] = 0.86650 * b[3] + white * 0.310_485_6;
        b[4] = 0.55000 * b[4] + white * 0.532_952_2;
        b[5] = -0.7616 * b[5] - white * 0.016_898_0;
        let out = b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + white * 0.5362;
        b[6] = white * 0.115_926;
        out
    }

    /// The next sample.
    pub fn tick(&mut self) -> f32 {
        let w = self.rng.bipolar();
        let v = match self.colour {
            Colour::White => w,
            Colour::Pink => self.pink(w),
            Colour::Brown => {
                self.brown = (self.brown + 0.02 * w) * 0.998;
                self.brown
            }
            Colour::Blue => {
                let p = self.pink(w);
                let d = p - self.prev;
                self.prev = p;
                d
            }
            Colour::Violet => {
                let d = w - self.prev;
                self.prev = w;
                d
            }
            Colour::Velvet { density } => {
                let p = (density / self.rate).clamp(0.0, 1.0);
                #[expect(clippy::cast_possible_truncation, reason = "probability")]
                let roll = self.rng.next_f64() as f32;
                if roll < p {
                    if self.rng.next_u64() & 1 == 0 {
                        1.0
                    } else {
                        -1.0
                    }
                } else {
                    0.0
                }
            }
        };
        v * self.gain
    }
}

impl super::Generator for Noise {
    fn next_sample(&mut self) -> f32 {
        self.tick()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::testing::rms;
    use crate::synth::Generator;

    const RATE: f32 = 48_000.0;

    /// Average power in an octave band, from many averaged FFT frames.
    fn band_power(colour: Colour, lo: f64, hi: f64) -> f64 {
        use crate::fft::{Complex, Plan};
        let n = 4096;
        let plan = Plan::new(n);
        let mut src = Noise::new(colour, 11, RATE);
        let mut acc = vec![0.0; n / 2];
        for _ in 0..64 {
            let mut buf: Vec<Complex> = (0..n)
                .map(|_| Complex::new(f64::from(src.tick()), 0.0))
                .collect();
            plan.forward(&mut buf);
            for (a, z) in acc.iter_mut().zip(&buf) {
                *a += z.norm().powi(2);
            }
        }
        #[expect(clippy::cast_precision_loss, reason = "test")]
        let bin = f64::from(RATE) / n as f64;
        let mut p = 0.0;
        let mut count = 0.0;
        for (k, a) in acc.iter().enumerate() {
            #[expect(clippy::cast_precision_loss, reason = "test")]
            let hz = k as f64 * bin;
            if hz >= lo && hz < hi {
                p += a;
                count += 1.0;
            }
        }
        p / count
    }

    /// dB change in per-bin power from one octave to the next.
    fn slope(colour: Colour) -> f64 {
        let a = band_power(colour, 500.0, 1000.0);
        let b = band_power(colour, 2000.0, 4000.0);
        10.0 * (b / a).log10() / 2.0
    }

    #[test]
    fn white_is_flat() {
        assert!(slope(Colour::White).abs() < 0.5, "{}", slope(Colour::White));
    }

    #[test]
    fn pink_falls_three_db_per_octave() {
        let s = slope(Colour::Pink);
        assert!((s + 3.0).abs() < 0.7, "{s}");
    }

    #[test]
    fn brown_falls_six_db_per_octave() {
        let s = slope(Colour::Brown);
        assert!((s + 6.0).abs() < 1.0, "{s}");
    }

    #[test]
    fn violet_rises_six_and_blue_three() {
        let v = slope(Colour::Violet);
        assert!((v - 6.0).abs() < 1.0, "{v}");
        let b = slope(Colour::Blue);
        assert!((b - 3.0).abs() < 1.2, "{b}");
    }

    #[test]
    fn colours_are_loudness_matched() {
        for c in [
            Colour::White,
            Colour::Pink,
            Colour::Brown,
            Colour::Blue,
            Colour::Violet,
        ] {
            let x = Noise::new(c, 3, RATE).render(96_000, 48_000).data;
            let r = rms(&x);
            assert!((0.25..0.9).contains(&r), "{c:?} rms {r}");
        }
    }

    #[test]
    fn velvet_density_is_respected() {
        let x = Noise::new(Colour::Velvet { density: 2000.0 }, 5, RATE)
            .render(48_000, 48_000)
            .data;
        let n = x.iter().filter(|v| **v != 0.0).count();
        assert!((1800..2200).contains(&n), "{n} impulses");
    }

    #[test]
    fn seeded_noise_is_reproducible() {
        let a = Noise::new(Colour::Pink, 9, RATE).render(1000, 48_000);
        let b = Noise::new(Colour::Pink, 9, RATE).render(1000, 48_000);
        assert_eq!(a, b);
    }
}
