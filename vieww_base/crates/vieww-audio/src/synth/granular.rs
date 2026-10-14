//! Granular synthesis — sound as a cloud of tiny windowed fragments.
//!
//! A *grain* is 1–100 ms of a source, read from some position at some
//! pitch, faded in and out by a window. Hundreds or thousands per second,
//! overlapping, scattered in position, pitch and stereo, become textures no
//! oscillator makes: clouds, smears, frozen moments, and — because a
//! grain's *pitch* and the *speed* at which grain positions move through the
//! source are independent — time-stretching without pitch change and pitch
//! shifting without time change.
//!
//! [`Granulator`] runs a fixed pool of grains (no allocation while
//! rendering), synchronous (evenly spaced) or asynchronous (Poisson-spaced)
//! scheduling, cubic interpolation in the source, and the four grain windows
//! the literature uses: Hann, Gaussian, Tukey and trapezoid. Scatter is
//! seeded, so a cloud renders the same way twice.

use std::sync::Arc;

use crate::rng::Rng;

/// A grain's amplitude window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GrainWindow {
    /// Raised cosine — smooth, the default.
    Hann,
    /// Gaussian with this σ as a fraction of the half-length (0.3–0.5).
    Gauss(f32),
    /// Flat top with cosine tapers on this fraction of the length.
    Tukey(f32),
    /// Linear ramps on this fraction at each end.
    Trapezoid(f32),
}

impl GrainWindow {
    /// The window at `x` in `0..=1`.
    #[must_use]
    pub fn at(self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        match self {
            Self::Hann => 0.5 - 0.5 * (std::f32::consts::TAU * x).cos(),
            Self::Gauss(s) => {
                let d = (x - 0.5) / (0.5 * s.max(0.01));
                (-0.5 * d * d).exp()
            }
            Self::Tukey(a) => {
                let a = a.clamp(1e-3, 1.0);
                let edge = a / 2.0;
                if x < edge {
                    0.5 - 0.5 * (std::f32::consts::PI * x / edge).cos()
                } else if x > 1.0 - edge {
                    0.5 - 0.5 * (std::f32::consts::PI * (1.0 - x) / edge).cos()
                } else {
                    1.0
                }
            }
            Self::Trapezoid(a) => {
                let a = a.clamp(1e-3, 0.5);
                (x / a).min((1.0 - x) / a).min(1.0)
            }
        }
    }
}

/// Cloud parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cloud {
    /// Grain length in ms.
    pub size_ms: f32,
    /// Grains per second.
    pub density: f32,
    /// Synchronous (even spacing) vs asynchronous (random spacing).
    pub synchronous: bool,
    /// Read position in the source, 0..=1.
    pub position: f32,
    /// Random spread of the read position, ± fraction of the source.
    pub position_jitter: f32,
    /// How fast `position` advances, in source-seconds per second
    /// (1 = real time, 0.5 = twice as long, 0 = frozen).
    pub speed: f32,
    /// Grain pitch in semitones.
    pub pitch: f32,
    /// Random pitch spread, ± semitones.
    pub pitch_jitter: f32,
    /// Stereo spread 0..=1.
    pub spread: f32,
    /// Probability a grain plays backwards.
    pub reverse: f32,
    /// The window.
    pub window: GrainWindow,
}

impl Default for Cloud {
    fn default() -> Self {
        Self {
            size_ms: 60.0,
            density: 40.0,
            synchronous: true,
            position: 0.0,
            position_jitter: 0.0,
            speed: 1.0,
            pitch: 0.0,
            pitch_jitter: 0.0,
            spread: 0.0,
            reverse: 0.0,
            window: GrainWindow::Hann,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Grain {
    pos: f64,
    inc: f64,
    age: usize,
    len: usize,
    gl: f32,
    gr: f32,
    active: bool,
}

/// A granular engine over a mono source.
#[derive(Debug, Clone)]
pub struct Granulator {
    source: Arc<Vec<f32>>,
    source_rate: f32,
    rate: f32,
    /// The cloud's parameters (change any time).
    pub cloud: Cloud,
    grains: Vec<Grain>,
    rng: Rng,
    until_next: f64,
    scan: f64,
    started: u64,
}

impl Granulator {
    /// A granulator over `source` (sampled at `source_rate`), rendering at
    /// `rate`, with room for `max_grains` simultaneous grains.
    #[must_use]
    pub fn new(
        source: Arc<Vec<f32>>,
        source_rate: f32,
        rate: f32,
        max_grains: usize,
        seed: u64,
    ) -> Self {
        Self {
            source,
            source_rate,
            rate,
            cloud: Cloud::default(),
            grains: vec![Grain::default(); max_grains.max(1)],
            rng: Rng::new(seed),
            until_next: 0.0,
            scan: 0.0,
            started: 0,
        }
    }

    /// Grains started so far.
    #[must_use]
    pub const fn grains_started(&self) -> u64 {
        self.started
    }

    /// Grains currently sounding.
    #[must_use]
    pub fn active(&self) -> usize {
        self.grains.iter().filter(|g| g.active).count()
    }

    fn read(&self, pos: f64) -> f32 {
        let n = self.source.len();
        if n < 4 {
            return 0.0;
        }
        #[expect(clippy::cast_precision_loss, reason = "source lengths")]
        let p = pos.clamp(1.0, (n - 3) as f64);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped"
        )]
        let i = p.floor() as usize;
        #[expect(clippy::cast_possible_truncation, reason = "fraction")]
        let f = (p - p.floor()) as f32;
        let s = &self.source;
        let (y0, y1, y2, y3) = (s[i - 1], s[i], s[i + 1], s[i + 2]);
        let c1 = 0.5 * (y2 - y0);
        let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
        let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
        ((c3 * f + c2) * f + c1) * f + y1
    }

    fn spawn(&mut self) {
        let Some(slot) = self.grains.iter().position(|g| !g.active) else {
            return; // pool full: drop the grain rather than allocate
        };
        let c = self.cloud;
        #[expect(clippy::cast_precision_loss, reason = "source lengths")]
        let n = self.source.len() as f64;
        let jitter = self.rng.range(-1.0, 1.0) * f64::from(c.position_jitter);
        let base = f64::from(c.position) + self.scan / n;
        let start = (base + jitter).rem_euclid(1.0) * n;
        let semis = c.pitch + c.pitch_jitter * self.rng.bipolar();
        let ratio =
            f64::from(2f32.powf(semis / 12.0)) * f64::from(self.source_rate) / f64::from(self.rate);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "grain length"
        )]
        let len = ((c.size_ms * 0.001 * self.rate) as usize).max(2);
        #[expect(clippy::cast_possible_truncation, reason = "probability")]
        let reverse = (self.rng.next_f64() as f32) < c.reverse;
        let pan = c.spread * self.rng.bipolar();
        let (gl, gr) = super::equal_power(pan);
        self.grains[slot] = Grain {
            pos: start,
            inc: if reverse { -ratio } else { ratio },
            age: 0,
            len,
            gl,
            gr,
            active: true,
        };
        self.started += 1;
    }

    /// The next stereo frame.
    pub fn next_stereo(&mut self) -> (f32, f32) {
        let c = self.cloud;
        // Scan position at `speed` source-seconds per output second.
        self.scan += f64::from(c.speed) * f64::from(self.source_rate) / f64::from(self.rate);
        self.until_next -= 1.0;
        while self.until_next <= 0.0 {
            self.spawn();
            let mean = f64::from(self.rate) / f64::from(c.density.max(0.01));
            let gap = if c.synchronous {
                mean
            } else {
                // Exponential inter-onset times: a Poisson process.
                -mean * (1.0 - self.rng.next_f64()).max(1e-12).ln()
            };
            self.until_next += gap.max(1.0);
        }
        let mut l = 0.0;
        let mut r = 0.0;
        for i in 0..self.grains.len() {
            let g = self.grains[i];
            if !g.active {
                continue;
            }
            #[expect(clippy::cast_precision_loss, reason = "grain lengths")]
            let x = g.age as f32 / (g.len - 1) as f32;
            let w = c.window.at(x);
            let v = self.read(g.pos) * w;
            l += v * g.gl;
            r += v * g.gr;
            let gm = &mut self.grains[i];
            gm.pos += gm.inc;
            gm.age += 1;
            if gm.age >= gm.len {
                gm.active = false;
            }
        }
        // Normalise for overlap: expected simultaneous grains.
        let overlap = (c.density * c.size_ms * 0.001).max(1.0);
        let k = 1.0 / overlap.sqrt();
        (l * k, r * k)
    }
}

impl super::Generator for Granulator {
    fn next_sample(&mut self) -> f32 {
        let (l, r) = self.next_stereo();
        (l + r) * std::f32::consts::FRAC_1_SQRT_2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::testing::{level_near, spectrum};
    use crate::synth::Generator;

    const RATE: f32 = 48_000.0;

    fn sine_source(hz: f32, seconds: f32) -> Arc<Vec<f32>> {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "test"
        )]
        let n = (seconds * RATE) as usize;
        Arc::new(
            (0..n)
                .map(|i| {
                    #[expect(clippy::cast_precision_loss, reason = "test")]
                    let t = i as f32 / RATE;
                    (std::f32::consts::TAU * hz * t).sin()
                })
                .collect(),
        )
    }

    #[test]
    fn windows_start_and_end_at_zero_and_peak_in_the_middle() {
        for w in [
            GrainWindow::Hann,
            GrainWindow::Tukey(0.5),
            GrainWindow::Trapezoid(0.2),
        ] {
            assert!(w.at(0.0).abs() < 1e-6, "{w:?}");
            assert!(w.at(1.0).abs() < 1e-6, "{w:?}");
            assert!((w.at(0.5) - 1.0).abs() < 1e-6, "{w:?}");
        }
        assert!((GrainWindow::Gauss(0.4).at(0.5) - 1.0).abs() < 1e-6);
        // A Gaussian never reaches zero: at σ = 0.4 its edge is e^−3.125.
        assert!((GrainWindow::Gauss(0.4).at(0.0) - (-3.125f32).exp()).abs() < 1e-6);
        assert!(
            (GrainWindow::Tukey(0.5).at(0.4) - 1.0).abs() < 1e-6,
            "flat top"
        );
    }

    #[test]
    fn density_sets_grains_per_second() {
        let mut g = Granulator::new(sine_source(220.0, 2.0), RATE, RATE, 256, 1);
        g.cloud.density = 200.0;
        for _ in 0..48_000 {
            g.next_stereo();
        }
        assert!(
            (199..=201).contains(&g.grains_started()),
            "{}",
            g.grains_started()
        );
        let mut a = Granulator::new(sine_source(220.0, 2.0), RATE, RATE, 256, 1);
        a.cloud.density = 200.0;
        a.cloud.synchronous = false;
        for _ in 0..48_000 {
            a.next_stereo();
        }
        let n = a.grains_started();
        assert!((160..=240).contains(&n), "Poisson ~200: {n}");
    }

    #[test]
    fn stretching_keeps_pitch_and_pitch_shifting_keeps_time() {
        let src = sine_source(500.0, 3.0);
        let peak_hz = |speed: f32, pitch: f32| {
            let mut g = Granulator::new(src.clone(), RATE, RATE, 64, 3);
            g.cloud.speed = speed;
            g.cloud.pitch = pitch;
            g.cloud.density = 80.0;
            g.cloud.size_ms = 50.0;
            let x = g.render(32_768, 48_000).data;
            let (m, bin) = spectrum(&x[8_192..8_192 + 16_384], RATE);
            let at500 = level_near(&m, bin, 500.0, 4);
            let at1000 = level_near(&m, bin, 1000.0, 4);
            (at500, at1000)
        };
        let (a, b) = peak_hz(0.25, 0.0);
        assert!(a > 10.0 * b, "quarter speed stays at 500 Hz: {a} {b}");
        let (a, b) = peak_hz(1.0, 12.0);
        assert!(b > 10.0 * a, "an octave up lands at 1 kHz: {a} {b}");
    }

    #[test]
    fn spread_widens_the_image_and_output_is_bounded() {
        let mut g = Granulator::new(sine_source(300.0, 2.0), RATE, RATE, 128, 5);
        g.cloud.spread = 1.0;
        g.cloud.density = 150.0;
        g.cloud.position_jitter = 0.3;
        g.cloud.pitch_jitter = 3.0;
        g.cloud.reverse = 0.5;
        let mut diff = 0.0f32;
        for _ in 0..48_000 {
            let (l, r) = g.next_stereo();
            assert!(l.is_finite() && l.abs() < 4.0 && r.abs() < 4.0);
            diff += (l - r).abs();
        }
        assert!(diff > 100.0, "{diff}");
    }

    #[test]
    fn full_pool_drops_grains_instead_of_allocating() {
        let mut g = Granulator::new(sine_source(300.0, 1.0), RATE, RATE, 4, 2);
        g.cloud.density = 2_000.0;
        g.cloud.size_ms = 100.0;
        for _ in 0..4_800 {
            g.next_stereo();
            assert!(g.active() <= 4);
        }
    }

    #[test]
    fn seeded_clouds_repeat() {
        let mk = || {
            let mut g = Granulator::new(sine_source(300.0, 1.0), RATE, RATE, 32, 77);
            g.cloud.synchronous = false;
            g.cloud.position_jitter = 0.5;
            g.render(4_800, 48_000)
        };
        assert_eq!(mk(), mk());
    }
}
