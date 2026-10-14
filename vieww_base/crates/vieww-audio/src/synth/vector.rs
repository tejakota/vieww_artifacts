//! Vector synthesis — four sources morphed by a 2-D joystick.
//!
//! The Prophet VS and the Korg Wavestation put four oscillators at the
//! corners of a diamond (A left, B top, C right, D bottom) and mixed them by
//! where a joystick sat; recording the joystick's path as an *envelope* made
//! the timbre travel on its own. [`Vector`] is that mixer for any four
//! [`Generator`]s, and [`VectorPath`] is the travelling joystick.
//!
//! The weights are the VS's: with the joystick at `(x, y)` in `[-1, 1]²`,
//! A gets `(1−x)/2 · (1−|y|)`, C gets `(1+x)/2 · (1−|y|)`, B gets `max(y,0)`
//! and D gets `max(−y,0)` — every point's weights sum to one, so moving the
//! stick changes the timbre and never the level.

use super::Generator;

/// A joystick position.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    /// −1 is A (left), +1 is C (right).
    pub x: f32,
    /// +1 is B (top), −1 is D (bottom).
    pub y: f32,
}

/// The four corner weights at `p`, in order A, B, C, D.
#[must_use]
pub fn weights(p: Point) -> [f32; 4] {
    let x = p.x.clamp(-1.0, 1.0);
    let y = p.y.clamp(-1.0, 1.0);
    let horizontal = 1.0 - y.abs();
    [
        (1.0 - x) / 2.0 * horizontal,
        y.max(0.0),
        (1.0 + x) / 2.0 * horizontal,
        (-y).max(0.0),
    ]
}

/// A path the joystick follows: points at times, linearly interpolated,
/// optionally looping between two points.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VectorPath {
    /// `(seconds, point)` in time order.
    pub points: Vec<(f32, Point)>,
    /// Loop between these point indices.
    pub loop_range: Option<(usize, usize)>,
}

impl VectorPath {
    /// The joystick at time `t`.
    #[must_use]
    pub fn at(&self, mut t: f32) -> Point {
        let Some(first) = self.points.first() else {
            return Point::default();
        };
        if let Some((a, b)) = self.loop_range {
            let (ta, tb) = (self.points[a].0, self.points[b].0);
            if tb > ta && t > tb {
                t = ta + (t - ta).rem_euclid(tb - ta);
            }
        }
        if t <= first.0 {
            return first.1;
        }
        for w in self.points.windows(2) {
            let (t0, p0) = w[0];
            let (t1, p1) = w[1];
            if t <= t1 {
                let f = if t1 > t0 { (t - t0) / (t1 - t0) } else { 1.0 };
                return Point {
                    x: p0.x + (p1.x - p0.x) * f,
                    y: p0.y + (p1.y - p0.y) * f,
                };
            }
        }
        self.points[self.points.len() - 1].1
    }
}

/// Four generators at the corners of a joystick.
pub struct Vector {
    /// The corners, A B C D.
    pub sources: [Box<dyn Generator + Send>; 4],
    /// The current joystick position (overridden by a path, if set).
    pub position: Point,
    /// A path to follow instead of `position`.
    pub path: Option<VectorPath>,
    rate: f32,
    elapsed: usize,
}

impl std::fmt::Debug for Vector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vector")
            .field("position", &self.position)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl Vector {
    /// A vector mixer over four sources.
    #[must_use]
    pub fn new(sources: [Box<dyn Generator + Send>; 4], rate: f32) -> Self {
        Self {
            sources,
            position: Point::default(),
            path: None,
            rate,
            elapsed: 0,
        }
    }

    /// Restart the path from its beginning.
    pub const fn restart(&mut self) {
        self.elapsed = 0;
    }
}

impl Generator for Vector {
    fn next_sample(&mut self) -> f32 {
        #[expect(clippy::cast_precision_loss, reason = "sample counts")]
        let t = self.elapsed as f32 / self.rate;
        self.elapsed += 1;
        let p = self.path.as_ref().map_or(self.position, |path| path.at(t));
        let w = weights(p);
        // Every corner advances every sample, heard or not, so a corner
        // fading back in is in phase with where it would have been.
        let mut sum = 0.0;
        for (src, wi) in self.sources.iter_mut().zip(w) {
            sum += src.next_sample() * wi;
        }
        sum
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::Constant;

    #[test]
    fn weights_sum_to_one_everywhere() {
        for xi in -4..=4 {
            for yi in -4..=4 {
                #[expect(clippy::cast_precision_loss, reason = "test")]
                let p = Point {
                    x: xi as f32 / 4.0,
                    y: yi as f32 / 4.0,
                };
                let s: f32 = weights(p).iter().sum();
                assert!((s - 1.0).abs() < 1e-6, "{p:?}");
            }
        }
    }

    #[test]
    fn corners_select_one_source() {
        assert_eq!(weights(Point { x: -1.0, y: 0.0 }), [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(weights(Point { x: 0.0, y: 1.0 }), [0.0, 1.0, 0.0, 0.0]);
        assert_eq!(weights(Point { x: 1.0, y: 0.0 }), [0.0, 0.0, 1.0, 0.0]);
        assert_eq!(weights(Point { x: 0.0, y: -1.0 }), [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn path_moves_the_mix_over_time() {
        let srcs: [Box<dyn Generator + Send>; 4] = [
            Box::new(Constant(1.0)),
            Box::new(Constant(2.0)),
            Box::new(Constant(3.0)),
            Box::new(Constant(4.0)),
        ];
        let mut v = Vector::new(srcs, 100.0);
        v.path = Some(VectorPath {
            points: vec![
                (0.0, Point { x: -1.0, y: 0.0 }),
                (1.0, Point { x: 1.0, y: 0.0 }),
            ],
            loop_range: None,
        });
        let x: Vec<f32> = (0..101).map(|_| v.next_sample()).collect();
        assert!((x[0] - 1.0).abs() < 1e-6);
        assert!(
            (x[50] - 2.0).abs() < 1e-6,
            "midway is A/C average: {}",
            x[50]
        );
        assert!((x[100] - 3.0).abs() < 1e-6);
    }

    #[test]
    fn looping_path_wraps() {
        let path = VectorPath {
            points: vec![
                (0.0, Point { x: 0.0, y: 0.0 }),
                (1.0, Point { x: 1.0, y: 0.0 }),
                (2.0, Point { x: -1.0, y: 0.0 }),
            ],
            loop_range: Some((1, 2)),
        };
        let a = path.at(1.5);
        let b = path.at(2.5);
        assert!((a.x - b.x).abs() < 1e-6);
    }
}
