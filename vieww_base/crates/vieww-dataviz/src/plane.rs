//! Manim's coordinate mobjects (§2.19 L2: `Axes`, `NumberPlane`, `Vector`)
//! and its signature move — `plane.apply_function(f)`, `apply_complex_function`
//! — where a grid is bent by a function and every line stays smooth because
//! it was subdivided before it was moved.
//!
//! A [`NumberPlane`] is lines in *math* space (x right, y up), each sampled
//! densely; [`NumberPlane::warped`] maps every sample through any function
//! (and [`complex`] gives the classic ones: z², eᶻ, 1/z, Joukowski), and
//! [`NumberPlane::morph`] interpolates between the straight and the warped
//! grid — Manim's `ApplyComplexFunction` animation, as a pure function of
//! `t`. [`to_screen`] maps math coordinates into a pixel rectangle.
//!
//! [`ValueTracker`] is Manim's animatable scalar with updaters: dependent
//! values recomputed whenever it is set.

/// A polyline in math coordinates.
pub type Line = Vec<(f64, f64)>;

/// What kind of line (for styling: axes bold, majors normal, minors faint).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Axis,
    Major,
    Minor,
}

/// The grid.
#[derive(Debug, Clone, PartialEq)]
pub struct NumberPlane {
    pub x_range: (f64, f64),
    pub y_range: (f64, f64),
    pub lines: Vec<(LineKind, Line)>,
}

impl NumberPlane {
    /// Lines every `step` (majors) with `minors` subdivisions between, each
    /// sampled every `resolution` units so warps bend smoothly.
    #[must_use]
    pub fn new(x_range: (f64, f64), y_range: (f64, f64), step: f64, minors: u32, resolution: f64) -> Self {
        let mut lines = Vec::new();
        let res = resolution.max(1e-3);
        let sub = step / f64::from(minors + 1);
        let sample = |a: f64, b: f64| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let n = (((b - a) / res).ceil() as usize).max(1);
            #[allow(clippy::cast_precision_loss)]
            (0..=n).map(move |i| a + (b - a) * i as f64 / n as f64)
        };
        let kind = |v: f64| {
            if v.abs() < 1e-9 {
                LineKind::Axis
            } else if ((v / step).round() * step - v).abs() < 1e-9 {
                LineKind::Major
            } else {
                LineKind::Minor
            }
        };
        let mut x = (x_range.0 / sub).ceil() * sub;
        while x <= x_range.1 + 1e-9 {
            lines.push((kind(x), sample(y_range.0, y_range.1).map(|y| (x, y)).collect()));
            x += sub;
        }
        let mut y = (y_range.0 / sub).ceil() * sub;
        while y <= y_range.1 + 1e-9 {
            lines.push((kind(y), sample(x_range.0, x_range.1).map(|xx| (xx, y)).collect()));
            y += sub;
        }
        // Axes last so they draw on top.
        lines.sort_by_key(|(k, _)| match k {
            LineKind::Minor => 0,
            LineKind::Major => 1,
            LineKind::Axis => 2,
        });
        Self { x_range, y_range, lines }
    }

    /// Every sample through `f` (`apply_function`).
    #[must_use]
    pub fn warped(&self, f: &dyn Fn((f64, f64)) -> (f64, f64)) -> Self {
        Self {
            lines: self
                .lines
                .iter()
                .map(|(k, l)| (*k, l.iter().map(|p| f(*p)).collect()))
                .collect(),
            ..self.clone()
        }
    }

    /// The grid partway to `f`'s image: each sample moves on a straight path
    /// from where it is to where `f` sends it (Manim's default path).
    #[must_use]
    pub fn morph(&self, f: &dyn Fn((f64, f64)) -> (f64, f64), t: f64) -> Self {
        let t = t.clamp(0.0, 1.0);
        self.warped(&|p| {
            let q = f(p);
            (p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t)
        })
    }
}

/// Classic complex maps, as functions of (re, im).
pub mod complex {
    /// z².
    #[must_use]
    pub fn square(z: (f64, f64)) -> (f64, f64) {
        (z.0 * z.0 - z.1 * z.1, 2.0 * z.0 * z.1)
    }
    /// eᶻ.
    #[must_use]
    pub fn exp(z: (f64, f64)) -> (f64, f64) {
        let r = z.0.exp();
        (r * z.1.cos(), r * z.1.sin())
    }
    /// 1/z (∞ at 0 is mapped to a large finite point).
    #[must_use]
    pub fn reciprocal(z: (f64, f64)) -> (f64, f64) {
        let d = (z.0 * z.0 + z.1 * z.1).max(1e-9);
        (z.0 / d, -z.1 / d)
    }
    /// The Joukowski map z + 1/z — the airfoil transform.
    #[must_use]
    pub fn joukowski(z: (f64, f64)) -> (f64, f64) {
        let r = reciprocal(z);
        (z.0 + r.0, z.1 + r.1)
    }
}

/// Math → screen: `(x_range, y_range)` onto the pixel rect `(l, t, w, h)`,
/// y flipped.
#[must_use]
pub fn to_screen(p: (f64, f64), x_range: (f64, f64), y_range: (f64, f64), rect: (f64, f64, f64, f64)) -> (f64, f64) {
    let (l, t, w, h) = rect;
    (
        l + (p.0 - x_range.0) / (x_range.1 - x_range.0) * w,
        t + (1.0 - (p.1 - y_range.0) / (y_range.1 - y_range.0)) * h,
    )
}

/// Manim's `ValueTracker` with updaters.
pub struct ValueTracker {
    value: f64,
    updaters: Vec<Box<dyn Fn(f64) -> f64>>,
    derived: Vec<f64>,
}

impl std::fmt::Debug for ValueTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValueTracker").field("value", &self.value).field("derived", &self.derived).finish()
    }
}

impl ValueTracker {
    #[must_use]
    pub fn new(value: f64) -> Self {
        Self { value, updaters: Vec::new(), derived: Vec::new() }
    }

    /// Register an updater; returns its index into [`derived`](Self::derived).
    pub fn add_updater(&mut self, f: impl Fn(f64) -> f64 + 'static) -> usize {
        self.derived.push(f(self.value));
        self.updaters.push(Box::new(f));
        self.updaters.len() - 1
    }

    pub fn set(&mut self, v: f64) {
        self.value = v;
        for (d, f) in self.derived.iter_mut().zip(&self.updaters) {
            *d = f(v);
        }
    }

    #[must_use]
    pub fn get(&self) -> f64 {
        self.value
    }

    #[must_use]
    pub fn derived(&self, i: usize) -> f64 {
        self.derived[i]
    }

    /// `tracker.animate.set_value(to)` at progress `t` from `from`.
    pub fn animate_to(&mut self, from: f64, to: f64, t: f64) {
        self.set(from + (to - from) * t.clamp(0.0, 1.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plane_has_axes_majors_and_minors() {
        let p = NumberPlane::new((-2.0, 2.0), (-1.0, 1.0), 1.0, 1, 0.1);
        let axes = p.lines.iter().filter(|(k, _)| *k == LineKind::Axis).count();
        let majors = p.lines.iter().filter(|(k, _)| *k == LineKind::Major).count();
        let minors = p.lines.iter().filter(|(k, _)| *k == LineKind::Minor).count();
        assert_eq!(axes, 2);
        assert_eq!(majors, 4 + 2);
        assert_eq!(minors, 4 + 2);
        assert_eq!(p.lines.last().expect("line").0, LineKind::Axis);
        // Subdivided: a vertical line over 2 units at 0.1 has 21 samples.
        assert_eq!(p.lines.iter().find(|(k, l)| *k == LineKind::Axis && l[0].0 == 0.0).expect("y axis").1.len(), 21);
    }

    #[test]
    fn morph_ends_at_the_warp_and_starts_at_the_grid() {
        let p = NumberPlane::new((-1.0, 1.0), (-1.0, 1.0), 0.5, 0, 0.05);
        let w = p.warped(&complex::square);
        let near = |a: &NumberPlane, b: &NumberPlane| {
            a.lines.iter().zip(&b.lines).all(|((_, la), (_, lb))| {
                la.iter().zip(lb).all(|(p, q)| (p.0 - q.0).abs() < 1e-9 && (p.1 - q.1).abs() < 1e-9)
            })
        };
        assert!(near(&p.morph(&complex::square, 0.0), &p));
        assert!(near(&p.morph(&complex::square, 1.0), &w));
        // z² sends i to −1.
        assert_eq!(complex::square((0.0, 1.0)), (-1.0, 0.0));
        let e = complex::exp((0.0, std::f64::consts::PI));
        assert!((e.0 + 1.0).abs() < 1e-12 && e.1.abs() < 1e-12);
        let j = complex::joukowski((1.0, 0.0));
        assert!((j.0 - 2.0).abs() < 1e-12);
    }

    #[test]
    fn screen_mapping_flips_y() {
        let s = to_screen((0.0, 1.0), (-1.0, 1.0), (-1.0, 1.0), (0.0, 0.0, 200.0, 100.0));
        assert_eq!(s, (100.0, 0.0));
    }

    #[test]
    fn trackers_drive_their_updaters() {
        let mut t = ValueTracker::new(1.0);
        let sq = t.add_updater(|v| v * v);
        t.animate_to(1.0, 3.0, 0.5);
        assert!((t.get() - 2.0).abs() < 1e-12);
        assert!((t.derived(sq) - 4.0).abs() < 1e-12);
    }
}
