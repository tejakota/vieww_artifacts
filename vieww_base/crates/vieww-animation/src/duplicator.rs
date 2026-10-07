//! Duplicators — Cavalry's procedural core (and After Effects' Repeater).
//!
//! A [`Duplicator`] lays out `n` copies of something by a [`Distribution`]
//! (linear, grid, radial, spiral, along a polyline, or a seeded random scatter
//! with a minimum spacing) and then lets **behaviours** shape them, each
//! scaled by a [`Falloff`] — the copy's index, its distance from a point, or
//! its position along an axis, eased. Every behaviour reads the copy's
//! normalised index and writes into an [`Instance`]: position, rotation,
//! scale, opacity and a free `value` channel the caller can map to colour.
//!
//! The whole thing is a pure function of its parameters (and `time`, which
//! the `Wave` behaviour reads), so it animates by being re-evaluated.
//!
//! ```
//! use vieww_animation::duplicator::{Behaviour, Distribution, Duplicator, Falloff};
//!
//! let rows = Duplicator::new(Distribution::Grid { cols: 4, rows: 3, spacing: (10.0, 10.0) })
//!     .behaviour(Behaviour::Scale(0.5), Falloff::Index { from: 0.0, to: 1.0 })
//!     .evaluate(0.0);
//! assert_eq!(rows.len(), 12);
//! assert!(rows[11].scale > rows[0].scale);
//! ```

use crate::noise::Perlin;
use crate::Curve;

/// Where the copies go.
#[derive(Debug, Clone)]
pub enum Distribution {
    /// `count` copies from `start`, stepping by `step`.
    Linear {
        count: usize,
        start: (f32, f32),
        step: (f32, f32),
    },
    /// Row-major, centred on the origin.
    Grid {
        cols: usize,
        rows: usize,
        spacing: (f32, f32),
    },
    /// Around a circle; copies face outward when `orient` is set.
    Radial {
        count: usize,
        radius: f32,
        orient: bool,
    },
    /// Phyllotaxis — the golden-angle spiral of a sunflower head.
    Spiral { count: usize, spacing: f32 },
    /// Evenly spaced by arc length along a polyline, oriented to its tangent.
    Path {
        count: usize,
        points: Vec<(f32, f32)>,
    },
    /// Poisson-disc scatter inside `size` (dart throwing, seeded).
    Scatter {
        count: usize,
        size: (f32, f32),
        min_distance: f32,
        seed: u64,
    },
}

/// One copy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Instance {
    pub index: usize,
    /// `index / (n − 1)`.
    pub t: f32,
    pub position: (f32, f32),
    /// Radians.
    pub rotation: f32,
    pub scale: f32,
    pub opacity: f32,
    /// A free channel (colour ramp position, z, …).
    pub value: f32,
}

/// How strongly a behaviour applies to a copy.
#[derive(Debug, Clone, Copy)]
pub enum Falloff {
    /// Full strength everywhere.
    None,
    /// Ramp over the normalised index.
    Index { from: f32, to: f32 },
    /// 1 at `center`, 0 beyond `radius`, eased.
    Radial {
        center: (f32, f32),
        radius: f32,
        curve: Curve,
    },
    /// Along an axis: 0 at `start`, 1 at `end` (projected).
    Linear { start: (f32, f32), end: (f32, f32) },
}

impl Falloff {
    fn weight(&self, inst: &Instance) -> f32 {
        match *self {
            Self::None => 1.0,
            Self::Index { from, to } => from + (to - from) * inst.t,
            Self::Radial {
                center,
                radius,
                curve,
            } => {
                let d = ((inst.position.0 - center.0).powi(2)
                    + (inst.position.1 - center.1).powi(2))
                .sqrt();
                curve.transform((1.0 - d / radius.max(1e-6)).clamp(0.0, 1.0))
            }
            Self::Linear { start, end } => {
                let (dx, dy) = (end.0 - start.0, end.1 - start.1);
                let l2 = (dx * dx + dy * dy).max(1e-12);
                (((inst.position.0 - start.0) * dx + (inst.position.1 - start.1) * dy) / l2)
                    .clamp(0.0, 1.0)
            }
        }
    }
}

/// What to do to the copies.
#[derive(Debug, Clone, Copy)]
pub enum Behaviour {
    /// Add to position.
    Offset(f32, f32),
    /// Add radians.
    Rotate(f32),
    /// Add to scale.
    Scale(f32),
    /// Multiply opacity by `1 − amount·w`.
    Fade(f32),
    /// Add seeded noise to position (amplitude, frequency).
    Jitter { amplitude: f32, frequency: f32, seed: u64 },
    /// A travelling sine wave in y: amplitude, wavelength in copies, speed.
    Wave {
        amplitude: f32,
        wavelength: f32,
        speed: f32,
    },
    /// Write into `value`.
    Value(f32),
}

/// A distribution plus behaviours.
#[derive(Debug, Clone)]
pub struct Duplicator {
    distribution: Distribution,
    behaviours: Vec<(Behaviour, Falloff)>,
}

impl Duplicator {
    #[must_use]
    pub fn new(distribution: Distribution) -> Self {
        Self {
            distribution,
            behaviours: Vec::new(),
        }
    }

    #[must_use]
    pub fn behaviour(mut self, b: Behaviour, f: Falloff) -> Self {
        self.behaviours.push((b, f));
        self
    }

    fn base(&self) -> Vec<((f32, f32), f32)> {
        match &self.distribution {
            Distribution::Linear { count, start, step } => (0..*count)
                .map(|i| {
                    #[allow(clippy::cast_precision_loss)]
                    let k = i as f32;
                    ((start.0 + step.0 * k, start.1 + step.1 * k), 0.0)
                })
                .collect(),
            Distribution::Grid { cols, rows, spacing } => {
                let mut v = Vec::with_capacity(cols * rows);
                #[allow(clippy::cast_precision_loss)]
                let (cx, cy) = (
                    (*cols as f32 - 1.0) * 0.5 * spacing.0,
                    (*rows as f32 - 1.0) * 0.5 * spacing.1,
                );
                for r in 0..*rows {
                    for c in 0..*cols {
                        #[allow(clippy::cast_precision_loss)]
                        v.push((
                            (c as f32 * spacing.0 - cx, r as f32 * spacing.1 - cy),
                            0.0,
                        ));
                    }
                }
                v
            }
            Distribution::Radial {
                count,
                radius,
                orient,
            } => (0..*count)
                .map(|i| {
                    #[allow(clippy::cast_precision_loss)]
                    let a = std::f32::consts::TAU * i as f32 / (*count).max(1) as f32;
                    (
                        (radius * a.cos(), radius * a.sin()),
                        if *orient { a } else { 0.0 },
                    )
                })
                .collect(),
            Distribution::Spiral { count, spacing } => {
                let golden = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
                (0..*count)
                    .map(|i| {
                        #[allow(clippy::cast_precision_loss)]
                        let k = i as f32;
                        let r = spacing * k.sqrt();
                        let a = k * golden;
                        ((r * a.cos(), r * a.sin()), a)
                    })
                    .collect()
            }
            Distribution::Path { count, points } => along_path(points, *count),
            Distribution::Scatter {
                count,
                size,
                min_distance,
                seed,
            } => scatter(*count, *size, *min_distance, *seed),
        }
    }

    /// Every copy at `time` (seconds).
    #[must_use]
    pub fn evaluate(&self, time: f32) -> Vec<Instance> {
        let base = self.base();
        let n = base.len();
        let mut out: Vec<Instance> = base
            .into_iter()
            .enumerate()
            .map(|(i, (p, r))| Instance {
                index: i,
                #[allow(clippy::cast_precision_loss)]
                t: if n > 1 { i as f32 / (n - 1) as f32 } else { 0.0 },
                position: p,
                rotation: r,
                scale: 1.0,
                opacity: 1.0,
                value: 0.0,
            })
            .collect();
        for (b, f) in &self.behaviours {
            // Weights read the layout *before* this behaviour moves anything.
            let weights: Vec<f32> = out.iter().map(|i| f.weight(i)).collect();
            for (inst, w) in out.iter_mut().zip(weights) {
                match *b {
                    Behaviour::Offset(x, y) => {
                        inst.position.0 += x * w;
                        inst.position.1 += y * w;
                    }
                    Behaviour::Rotate(a) => inst.rotation += a * w,
                    Behaviour::Scale(s) => inst.scale += s * w,
                    Behaviour::Fade(a) => inst.opacity *= 1.0 - a * w,
                    Behaviour::Jitter {
                        amplitude,
                        frequency,
                        seed,
                    } => {
                        let p = Perlin::from_seed(seed);
                        #[allow(clippy::cast_precision_loss)]
                        let k = inst.index as f32 * 1.618 + time * frequency;
                        inst.position.0 += p.noise1(k) * amplitude * w;
                        inst.position.1 += p.noise1(k + 43.0) * amplitude * w;
                    }
                    Behaviour::Wave {
                        amplitude,
                        wavelength,
                        speed,
                    } => {
                        #[allow(clippy::cast_precision_loss)]
                        let ph = std::f32::consts::TAU
                            * (inst.index as f32 / wavelength.max(1e-3) - time * speed);
                        inst.position.1 += amplitude * ph.sin() * w;
                    }
                    Behaviour::Value(v) => inst.value += v * w,
                }
            }
        }
        out
    }
}

fn along_path(points: &[(f32, f32)], count: usize) -> Vec<((f32, f32), f32)> {
    if points.len() < 2 || count == 0 {
        return points.iter().take(count).map(|p| (*p, 0.0)).collect();
    }
    let seg: Vec<f32> = points
        .windows(2)
        .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
        .collect();
    let total: f32 = seg.iter().sum();
    (0..count)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let mut d = if count > 1 {
                total * i as f32 / (count - 1) as f32
            } else {
                0.0
            };
            for (k, l) in seg.iter().enumerate() {
                if d <= *l || k == seg.len() - 1 {
                    let u = if *l > 0.0 { (d / l).min(1.0) } else { 0.0 };
                    let (a, b) = (points[k], points[k + 1]);
                    let p = (a.0 + (b.0 - a.0) * u, a.1 + (b.1 - a.1) * u);
                    return (p, (b.1 - a.1).atan2(b.0 - a.0));
                }
                d -= l;
            }
            (points[0], 0.0)
        })
        .collect()
}

fn scatter(count: usize, size: (f32, f32), min_d: f32, seed: u64) -> Vec<((f32, f32), f32)> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut rand = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        #[allow(clippy::cast_precision_loss)]
        let r = (state >> 40) as f32 / (1u64 << 24) as f32;
        r
    };
    let mut out: Vec<((f32, f32), f32)> = Vec::with_capacity(count);
    let mut tries = 0;
    while out.len() < count && tries < count * 60 {
        tries += 1;
        let p = (rand() * size.0, rand() * size.1);
        if out
            .iter()
            .all(|(q, _)| (q.0 - p.0).powi(2) + (q.1 - p.1).powi(2) >= min_d * min_d)
        {
            out.push((p, 0.0));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_is_centred() {
        let g = Duplicator::new(Distribution::Grid {
            cols: 3,
            rows: 3,
            spacing: (2.0, 2.0),
        })
        .evaluate(0.0);
        assert_eq!(g[4].position, (0.0, 0.0));
        assert_eq!(g[0].position, (-2.0, -2.0));
    }

    #[test]
    fn radial_copies_sit_on_the_circle_and_face_out() {
        let r = Duplicator::new(Distribution::Radial {
            count: 8,
            radius: 5.0,
            orient: true,
        })
        .evaluate(0.0);
        for i in &r {
            let d = (i.position.0.powi(2) + i.position.1.powi(2)).sqrt();
            assert!((d - 5.0).abs() < 1e-4);
        }
        assert!((r[2].rotation - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
    }

    #[test]
    fn spiral_radius_grows_with_sqrt_index() {
        let s = Duplicator::new(Distribution::Spiral {
            count: 101,
            spacing: 1.0,
        })
        .evaluate(0.0);
        let r = |i: &Instance| (i.position.0.powi(2) + i.position.1.powi(2)).sqrt();
        assert!((r(&s[100]) - 10.0).abs() < 1e-3);
    }

    #[test]
    fn path_spacing_is_by_arc_length() {
        let p = Duplicator::new(Distribution::Path {
            count: 5,
            points: vec![(0.0, 0.0), (1.0, 0.0), (1.0, 3.0)],
        })
        .evaluate(0.0);
        assert!((p[1].position.0 - 1.0).abs() < 1e-4 && p[1].position.1.abs() < 1e-4);
        assert!((p[4].position.1 - 3.0).abs() < 1e-4);
        assert!((p[3].rotation - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
    }

    #[test]
    fn scatter_respects_min_distance_and_seed() {
        let mk = |seed| {
            Duplicator::new(Distribution::Scatter {
                count: 60,
                size: (100.0, 100.0),
                min_distance: 8.0,
                seed,
            })
            .evaluate(0.0)
        };
        let a = mk(3);
        assert_eq!(a.len(), 60);
        for i in 0..a.len() {
            for j in i + 1..a.len() {
                let d = ((a[i].position.0 - a[j].position.0).powi(2)
                    + (a[i].position.1 - a[j].position.1).powi(2))
                .sqrt();
                assert!(d >= 8.0);
            }
        }
        assert_eq!(a, mk(3));
        assert_ne!(a, mk(4));
    }

    #[test]
    fn radial_falloff_weakens_with_distance() {
        let d = Duplicator::new(Distribution::Linear {
            count: 11,
            start: (0.0, 0.0),
            step: (1.0, 0.0),
        })
        .behaviour(
            Behaviour::Scale(1.0),
            Falloff::Radial {
                center: (0.0, 0.0),
                radius: 10.0,
                curve: Curve::Linear,
            },
        )
        .evaluate(0.0);
        assert!((d[0].scale - 2.0).abs() < 1e-4);
        assert!((d[5].scale - 1.5).abs() < 1e-4);
        assert!((d[10].scale - 1.0).abs() < 1e-4);
    }

    #[test]
    fn wave_travels_with_time() {
        let d = Duplicator::new(Distribution::Linear {
            count: 8,
            start: (0.0, 0.0),
            step: (1.0, 0.0),
        })
        .behaviour(
            Behaviour::Wave {
                amplitude: 2.0,
                wavelength: 8.0,
                speed: 1.0,
            },
            Falloff::None,
        );
        let a = d.evaluate(0.0);
        let b = d.evaluate(0.125);
        // One copy per 1/8 s: the wave moved by one copy.
        assert!((a[2].position.1 - b[3].position.1).abs() < 1e-4);
    }

    #[test]
    fn linear_falloff_and_fade() {
        let d = Duplicator::new(Distribution::Linear {
            count: 3,
            start: (0.0, 0.0),
            step: (5.0, 0.0),
        })
        .behaviour(
            Behaviour::Fade(1.0),
            Falloff::Linear {
                start: (0.0, 0.0),
                end: (10.0, 0.0),
            },
        )
        .evaluate(0.0);
        assert_eq!(d[0].opacity, 1.0);
        assert!((d[1].opacity - 0.5).abs() < 1e-5);
        assert!(d[2].opacity.abs() < 1e-5);
    }
}
