//! Gradient noise — the `noise()` of Processing, the Noise CHOP of
//! TouchDesigner, the texture source of Blender's procedural materials.
//!
//! # Why this crate owns it
//!
//! Every framework in the comparison document that does generative or
//! procedural work has a noise function, and none of them treat it as a
//! random-number generator. The distinction is the whole point: a random
//! number is a function of nothing (sample it twice, get two answers), while
//! noise is a **function of position** — the same coordinate always produces
//! the same value, and *nearby* coordinates produce nearby values. That
//! combination — reproducible, but spatially coherent — is what a smoke
//! drift, a water surface or a jitter that does not strobe needs, and a RNG
//! gives neither half.
//!
//! # The one rule
//!
//! Same seed, same coordinate, same value, every time. There is no clock in
//! here and no state a sample can mutate, so a frame rendered twice is a
//! frame rendered identically — the same contract the rest of this crate
//! makes, applied to a field rather than to a timeline.
//!
//! # What kind
//!
//! Perlin **gradient** noise rather than value noise. Value noise interpolates
//! a random number at each lattice corner, which visibly squares off around
//! the lattice — the eye finds the grid. Perlin noise interpolates a random
//! *gradient* at each corner, and because the field is flat (gradient zero)
//! at every lattice point, the grid hides: the eye cannot find a lattice it
//! cannot see. The fade curve is the quintic `6t⁵ − 15t⁴ + 10t³`, whose
//! first *and* second derivatives vanish at the ends, so joined segments
//! meet without a crease in the velocity either.
//!
//! ```
//! use vieww_animation::noise::Perlin;
//!
//! let noise = Perlin::from_seed(7);
//!
//! // Deterministic: the same ask, the same answer.
//! assert_eq!(noise.noise2(1.5, 2.5), noise.noise2(1.5, 2.5));
//!
//! // Coherent: a step of one lattice cell is a step of less than half.
//! let a = noise.noise2(3.0, 4.0);
//! let b = noise.noise2(3.25, 4.0);
//! assert!((a - b).abs() < 0.5, "neighbours, not strangers: {a} vs {b}");
//!
//! // Bounded: gradient noise with length-√2 gradients interpolates to at
//! // most g·√N/2 — exactly ±1 in 2D — so a caller can feed it straight into
//! // a `Color` or a `Curve` with no folklore constant in between.
//! assert!(noise.noise2(0.7, 9.1).abs() <= 1.0);
//! ```

/// Perlin gradient noise in one, two and three dimensions, plus the fractal
/// sum ([`fbm`](Perlin::fbm)) that stacks octaves of it.
///
/// Build once, sample as often as you like: the permutation table is shuffled
/// at construction and never touched again, so one `Perlin` is any number of
/// deterministic fields sharing a seed.
#[derive(Debug, Clone)]
pub struct Perlin {
    /// The shuffled 0..256 index table, doubled so every lookup can wrap with
    /// a mask instead of a modulo — the classic Perlin implementation trick,
    /// kept because it is one of the rare micro-optimisations that also makes
    /// the code *shorter* to read.
    perm: [u8; 512],
}

/// The shuffle behind [`Perlin::from_seed`].
///
/// splitmix64 rather than rand: three lines, no dependency, and the quality
/// that matters here is only "does a different seed give a different shuffle"
/// — not the statistical purity a simulation would demand.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Perlin {
    /// A field from a seed: the same seed is the same field, forever, on
    /// every machine.
    pub fn from_seed(seed: u64) -> Self {
        let mut perm: [u8; 256] = {
            let mut table = [0u8; 256];
            for (i, slot) in table.iter_mut().enumerate() {
                *slot = u8::try_from(i).expect("256 entries fit a byte");
            }
            table
        };
        // Fisher–Yates under splitmix64: every seed a different permutation,
        // and no seed an identity permutation, which would make every lattice
        // point its own gradient and turn the field into stripes.
        let mut state = seed;
        for i in (1..perm.len()).rev() {
            let j = (splitmix64(&mut state) % (i as u64 + 1)) as usize;
            perm.swap(i, j);
        }
        let mut doubled = [0u8; 512];
        doubled[..256].copy_from_slice(&perm);
        doubled[256..].copy_from_slice(&perm);
        Self { perm: doubled }
    }

    /// One-dimensional noise — a coherent signal along a line. The time axis
    /// of a drift, the x of a horizon.
    ///
    /// Reuses [`noise2`](Self::noise2) with y pinned, because a 1D Perlin and
    /// a 2D Perlin restricted to a line are the same field and maintaining two
    /// implementations of one gradient table would be a place for them to
    /// disagree.
    pub fn noise1(&self, x: f32) -> f32 {
        self.noise2(x, 0.0)
    }

    /// Two-dimensional noise — the workhorse: terrain, smoke, a water surface.
    pub fn noise2(&self, x: f32, y: f32) -> f32 {
        // Lattice cell and position inside it.
        let xi = x.floor() as i32;
        let yi = y.floor() as i32;
        let xf = x - x.floor();
        let yf = y - y.floor();

        // Corner gradients, hashed from the permutation table.
        let hash = |gx: i32, gy: i32| -> usize {
            let px = (gx & 0xFF) as usize;
            let py = (gy & 0xFF) as usize;
            // The doubled table is what makes this a plain add: `perm[px]`
            // is a `u8` (≤ 255) and `py` ≤ 255, so the index is ≤ 510 and
            // the 512-entry table covers it without a wrapping pass.
            self.perm[self.perm[px] as usize + py] as usize
        };
        let g00 = gradient2(hash(xi, yi), xf, yf);
        let g10 = gradient2(hash(xi + 1, yi), xf - 1.0, yf);
        let g01 = gradient2(hash(xi, yi + 1), xf, yf - 1.0);
        let g11 = gradient2(hash(xi + 1, yi + 1), xf - 1.0, yf - 1.0);

        let u = fade(xf);
        let v = fade(yf);

        // Bilinear blend of the four corner contributions.
        let x0 = g00 + u * (g10 - g00);
        let x1 = g01 + u * (g11 - g01);
        let mixed = x0 + v * (x1 - x0);

        // Already inside ±1. The gradients are the four diagonals (±1, ±1),
        // length √2, and the interpolated maximum of N-dimensional gradient
        // noise with gradient length g is g·√N/2 — here √2·√2/2 = 1 exactly.
        // Scaling by anything would be a constant nobody asked for; the
        // 1-D case (g = 1, max 1·√1/2 = 0.5, the lerp(2t(1−t)) shape)
        // checks the formula.
        mixed
    }

    /// Three-dimensional noise — a volume, or a 2D field animated coherently
    /// through time (the third coordinate is the clock, and this is exactly
    /// how every "flowing" 2D shader does it).
    pub fn noise3(&self, x: f32, y: f32, z: f32) -> f32 {
        let xi = x.floor() as i32;
        let yi = y.floor() as i32;
        let zi = z.floor() as i32;
        let xf = x - x.floor();
        let yf = y - y.floor();
        let zf = z - z.floor();

        let hash = |gx: i32, gy: i32, gz: i32| -> usize {
            let px = (gx & 0xFF) as usize;
            let py = (gy & 0xFF) as usize;
            let pz = (gz & 0xFF) as usize;
            self.perm[self.perm[self.perm[px] as usize + py] as usize + pz] as usize
        };

        let corners = |dx: f32, dy: f32| -> [f32; 2] {
            [
                gradient3(hash(xi, yi, zi), xf + dx, yf + dy, zf),
                gradient3(hash(xi, yi, zi + 1), xf + dx, yf + dy, zf - 1.0),
            ]
        };
        let c00 = corners(0.0, 0.0);
        let c10 = corners(-1.0, 0.0);
        let c01 = corners(0.0, -1.0);
        let c11 = corners(-1.0, -1.0);

        let u = fade(xf);
        let v = fade(yf);
        let w = fade(zf);

        let mix = |pair: [f32; 2]| pair[0] + w * (pair[1] - pair[0]);
        let x0 = mix(c00) + u * (mix(c10) - mix(c00));
        let x1 = mix(c01) + u * (mix(c11) - mix(c01));
        let mixed = x0 + v * (x1 - x0);

        // ±√6/2 normalised to ±1 — same g·√N/2 bound as the 2D case above,
        // with N = 3: √2·√3/2.
        mixed * (2.0 / (6.0_f32).sqrt())
    }

    /// Fractal Brownian motion — Perlin's own second act, and the reason most
    /// callers reach for noise at all.
    ///
    /// Sums `octaves` copies of the field, each at twice the frequency
    /// (lacunarity) and `gain` of the amplitude, so the result has detail at
    /// every scale: a coastline, a cloud, a mountain range — none of which one
    /// octave of anything can look like. The value stays inside ±1 because
    /// the amplitude series is geometric and is renormalised here rather than
    /// left for every caller to do the same.
    ///
    /// `detail` is the number of octaves: 1 is plain noise, 2–4 is terrain,
    /// 5+ is the fuzz only a zoomed-in view can find. Values of 0 make the
    /// answer identically zero — there are no octaves, so there is no field —
    /// and are clamped to 1 rather than special-cased, because a caller who
    /// writes 0 means "no detail" and plain noise is the least surprising
    /// reading of that.
    pub fn fbm(&self, x: f32, y: f32, octaves: u32, gain: f32, lacunarity: f32) -> f32 {
        let octaves = octaves.max(1);
        let mut sum = 0.0;
        let mut amplitude = 1.0;
        let mut total = 0.0;
        let mut frequency = 1.0;
        for _ in 0..octaves {
            sum += amplitude * self.noise2(x * frequency, y * frequency);
            total += amplitude;
            amplitude *= gain;
            frequency *= lacunarity;
        }
        sum / total
    }
}

/// The quintic fade, and why quintic: the cubic `3t² − 2t³` that Perlin's
/// 1985 paper used has a *first* derivative of zero at both ends but a
/// non-zero second derivative, which shows up as a faint crease where cells
/// join. The quintic zeroes both, and the crease goes away. It costs two
/// multiplies more; the eye is worth two multiplies.
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// One of the twelve 3D corner gradients Perlin specified, dotted with the
/// offset from its lattice corner — the "gradient" in gradient noise.
///
/// The 3D table restricted to (±1, ±1, 0) and its permutations is also the 2D
/// table, which is why [`Perlin::noise2`](Perlin::noise2) can call this: one
/// gradient set, two dimensions, no second implementation to drift.
fn gradient3(hash: usize, x: f32, y: f32, z: f32) -> f32 {
    match hash % 12 {
        0 => x + y,
        1 => -x + y,
        2 => x - y,
        3 => -x - y,
        4 => x + z,
        5 => -x + z,
        6 => x - z,
        7 => -x - z,
        8 => y + z,
        9 => -y + z,
        10 => y - z,
        _ => -y - z,
    }
}

/// The 2D face of [`gradient3`] — the four (±1, ±1) diagonals, dotted with
/// the corner offset, matching what `gradient3` does for the z = 0 slice.
fn gradient2(hash: usize, x: f32, y: f32) -> f32 {
    match hash % 4 {
        0 => x + y,
        1 => -x + y,
        2 => x - y,
        _ => -x - y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-6;

    #[test]
    fn the_same_ask_gets_the_same_answer() {
        let noise = Perlin::from_seed(42);
        for x in [-2.0, -0.5, 0.0, 0.13, 7.7] {
            for y in [-3.0, 0.0, 4.2] {
                assert!((noise.noise2(x, y) - noise.noise2(x, y)).abs() < EPS);
            }
        }
    }

    #[test]
    fn lattice_points_are_zero() {
        // The field is flat at every integer coordinate — the property that
        // hides the grid from the eye, and the fastest fingerprint that the
        // gradient (not value) noise was implemented.
        let noise = Perlin::from_seed(1);
        for gx in 0..8 {
            for gy in 0..8 {
                let x = gx as f32;
                let y = gy as f32;
                assert!(
                    noise.noise2(x, y).abs() < 1e-4,
                    "field is not flat at ({x}, {y}): {}",
                    noise.noise2(x, y)
                );
            }
        }
    }

    #[test]
    fn different_seeds_are_different_fields() {
        let a = Perlin::from_seed(1);
        let b = Perlin::from_seed(2);
        let mut disagreements = 0;
        for i in 0..64 {
            let x = i as f32 * 0.37;
            if (a.noise2(x, 0.5) - b.noise2(x, 0.5)).abs() > 1e-3 {
                disagreements += 1;
            }
        }
        // 64 samples of two different fields agreeing everywhere is not a
        // coincidence, it is a bug; anything above a handful is healthy.
        assert!(disagreements > 8, "only {disagreements} of 64 differ");
    }

    #[test]
    fn the_field_is_bounded() {
        let noise = Perlin::from_seed(9);
        let mut worst = 0.0f32;
        for i in 0..2000 {
            let x = (i % 67) as f32 * 0.71;
            let y = (i / 67) as f32 * 0.53;
            worst = worst.max(noise.noise2(x, y).abs());
            worst = worst.max(noise.noise3(x, y, x * 0.3).abs());
        }
        assert!(worst <= 1.0 + 1e-4, "worst sample {worst} left ±1");
    }

    #[test]
    fn neighbours_are_neighbours() {
        let noise = Perlin::from_seed(3);
        for (x, y) in [(1.2, 3.4), (7.7, 0.1), (0.5, 0.5)] {
            let here = noise.noise2(x, y);
            let there = noise.noise2(x + 0.1, y);
            assert!(
                (here - there).abs() < 0.4,
                "a small step moved {here} to {there}"
            );
        }
    }

    #[test]
    fn one_dimension_is_a_line_through_two() {
        let noise = Perlin::from_seed(11);
        for x in [-1.0, 0.25, 2.6] {
            assert!((noise.noise1(x) - noise.noise2(x, 0.0)).abs() < EPS);
        }
    }

    #[test]
    fn fbm_is_deterministic_and_bounded() {
        let noise = Perlin::from_seed(5);
        let a = noise.fbm(1.7, 2.3, 4, 0.5, 2.0);
        let b = noise.fbm(1.7, 2.3, 4, 0.5, 2.0);
        assert!((a - b).abs() < EPS);
        for i in 0..500 {
            let x = (i % 31) as f32 * 0.41;
            let y = (i / 31) as f32 * 0.29;
            assert!(noise.fbm(x, y, 5, 0.55, 2.1).abs() <= 1.0 + 1e-4);
        }
    }

    #[test]
    fn fbm_octaves_add_detail() {
        // More octaves must change the value — the sum has more terms — and
        // the classic failure this catches is an octave loop that recomputes
        // the same frequency every time.
        let noise = Perlin::from_seed(13);
        let one = noise.fbm(0.8, 0.9, 1, 0.5, 2.0);
        let two = noise.fbm(0.8, 0.9, 2, 0.5, 2.0);
        assert!((one - two).abs() > 1e-3, "{one} vs {two}");
    }

    #[test]
    fn zero_octaves_is_plain_noise() {
        // Documented: 0 means "no detail", and plain noise is the reading.
        let noise = Perlin::from_seed(17);
        let zero = noise.fbm(0.4, 1.2, 0, 0.5, 2.0);
        let one = noise.fbm(0.4, 1.2, 1, 0.5, 2.0);
        assert!((zero - one).abs() < EPS);
    }
}
