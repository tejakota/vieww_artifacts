//! Vector fields and number lines — Manim's `VectorField`, `StreamLines`,
//! `ArrowVectorField`, `NumberLine` and `Axes` (§2.19 L2), Matplotlib's
//! `quiver`/`streamplot`.
//!
//! * [`arrows`] samples a field on a grid into arrow paths (shaft + head),
//!   length-normalised with the magnitude returned for colouring.
//! * [`streamline`] integrates a field with classical RK4 from a seed, both
//!   directions, stopping at the bounds, at stagnation, or when it would
//!   come within `separation` of an existing line (Jobard–Lefer evenly
//!   spaced streamlines when seeded by [`streamlines`]).
//! * [`NumberLine`] places ticks and labels along a segment, with
//!   `number_to_point`/`point_to_number`, Manim's API, for animating dots
//!   and braces onto numbers.

use vieww_foundation::{Offset, Path};

/// A 2-D vector field.
pub type FieldFn<'a> = &'a dyn Fn(f32, f32) -> (f32, f32);

/// One arrow: path and the field magnitude at its base.
#[derive(Debug, Clone, PartialEq)]
pub struct Arrow {
    pub path: Path,
    pub magnitude: f32,
}

/// Arrows on a `cols × rows` grid over `(x0, y0, x1, y1)`, each at most
/// `max_len` long (scaled by magnitude relative to the largest).
#[must_use]
pub fn arrows(field: FieldFn<'_>, bounds: (f32, f32, f32, f32), cols: usize, rows: usize, max_len: f32) -> Vec<Arrow> {
    let (x0, y0, x1, y1) = bounds;
    let mut samples = Vec::new();
    for j in 0..rows {
        for i in 0..cols {
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = (
                x0 + (x1 - x0) * (i as f32 + 0.5) / cols as f32,
                y0 + (y1 - y0) * (j as f32 + 0.5) / rows as f32,
            );
            let (u, v) = field(x, y);
            samples.push((x, y, u, v, (u * u + v * v).sqrt()));
        }
    }
    let mx = samples.iter().map(|s| s.4).fold(0.0f32, f32::max).max(1e-9);
    samples
        .into_iter()
        .map(|(x, y, u, v, m)| {
            let len = max_len * m / mx;
            let (dx, dy) = if m > 0.0 { (u / m * len, v / m * len) } else { (0.0, 0.0) };
            let tip = Offset::new(x + dx, y + dy);
            let mut p = Path::new();
            p.move_to(Offset::new(x, y));
            p.line_to(tip);
            if len > 1e-6 {
                let h = len.min(max_len) * 0.3;
                let (ux, uy) = (dx / len, dy / len);
                p.move_to(Offset::new(tip.dx - ux * h - uy * h * 0.5, tip.dy - uy * h + ux * h * 0.5));
                p.line_to(tip);
                p.line_to(Offset::new(tip.dx - ux * h + uy * h * 0.5, tip.dy - uy * h - ux * h * 0.5));
            }
            Arrow { path: p, magnitude: m }
        })
        .collect()
}

fn rk4(field: FieldFn<'_>, p: (f32, f32), h: f32) -> (f32, f32) {
    let unit = |x: f32, y: f32| {
        let (u, v) = field(x, y);
        let m = (u * u + v * v).sqrt();
        if m < 1e-9 {
            (0.0, 0.0)
        } else {
            (u / m, v / m)
        }
    };
    let k1 = unit(p.0, p.1);
    let k2 = unit(p.0 + k1.0 * h * 0.5, p.1 + k1.1 * h * 0.5);
    let k3 = unit(p.0 + k2.0 * h * 0.5, p.1 + k2.1 * h * 0.5);
    let k4 = unit(p.0 + k3.0 * h, p.1 + k3.1 * h);
    (
        p.0 + h / 6.0 * (k1.0 + 2.0 * k2.0 + 2.0 * k3.0 + k4.0),
        p.1 + h / 6.0 * (k1.1 + 2.0 * k2.1 + 2.0 * k3.1 + k4.1),
    )
}

/// Integrate from `seed` along the (unit-speed) field both ways with step
/// `h`, at most `steps` each way, stopping outside `bounds`, at a
/// stagnation point, or within `separation` of a point in `existing`.
#[must_use]
pub fn streamline(
    field: FieldFn<'_>,
    seed: (f32, f32),
    bounds: (f32, f32, f32, f32),
    h: f32,
    steps: usize,
    existing: &[(f32, f32)],
    separation: f32,
) -> Vec<(f32, f32)> {
    let inside = |p: (f32, f32)| p.0 >= bounds.0 && p.0 <= bounds.2 && p.1 >= bounds.1 && p.1 <= bounds.3;
    let free = |p: (f32, f32)| existing.iter().all(|q| (q.0 - p.0).powi(2) + (q.1 - p.1).powi(2) >= separation * separation);
    let mut halves = [Vec::new(), Vec::new()];
    for (dir, half) in [1.0f32, -1.0].iter().zip(halves.iter_mut()) {
        let mut p = seed;
        for _ in 0..steps {
            let q = rk4(field, p, h * dir);
            if !inside(q) || ((q.0 - p.0).abs() + (q.1 - p.1).abs()) < h * 1e-3 || !free(q) {
                break;
            }
            half.push(q);
            p = q;
        }
    }
    let [fwd, back] = halves;
    let mut line: Vec<(f32, f32)> = back.into_iter().rev().collect();
    line.push(seed);
    line.extend(fwd);
    line
}

/// Evenly spaced streamlines (seeded beside existing ones, Jobard–Lefer).
#[must_use]
pub fn streamlines(field: FieldFn<'_>, bounds: (f32, f32, f32, f32), separation: f32, h: f32) -> Vec<Vec<(f32, f32)>> {
    let mut lines: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut all: Vec<(f32, f32)> = Vec::new();
    let mut seeds = vec![((bounds.0 + bounds.2) * 0.5, (bounds.1 + bounds.3) * 0.5)];
    let mut k = 0;
    while k < seeds.len() && lines.len() < 400 {
        let s = seeds[k];
        k += 1;
        if all.iter().any(|q| (q.0 - s.0).powi(2) + (q.1 - s.1).powi(2) < separation * separation) {
            continue;
        }
        let l = streamline(field, s, bounds, h, 2000, &all, separation * 0.5);
        if l.len() < 3 {
            continue;
        }
        for w in l.windows(2).step_by(3) {
            let (a, b) = (w[0], w[1]);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let m = (dx * dx + dy * dy).sqrt().max(1e-9);
            seeds.push((a.0 - dy / m * separation, a.1 + dx / m * separation));
            seeds.push((a.0 + dy / m * separation, a.1 - dx / m * separation));
        }
        all.extend(&l);
        lines.push(l);
    }
    lines
}

/// Manim's `NumberLine`: a segment carrying a numeric range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NumberLine {
    pub start: Offset,
    pub end: Offset,
    pub min: f64,
    pub max: f64,
    pub step: f64,
}

impl NumberLine {
    #[must_use]
    pub fn number_to_point(&self, x: f64) -> Offset {
        #[allow(clippy::cast_possible_truncation)]
        let t = ((x - self.min) / (self.max - self.min)) as f32;
        Offset::new(self.start.dx + (self.end.dx - self.start.dx) * t, self.start.dy + (self.end.dy - self.start.dy) * t)
    }

    #[must_use]
    pub fn point_to_number(&self, p: Offset) -> f64 {
        let (dx, dy) = (self.end.dx - self.start.dx, self.end.dy - self.start.dy);
        let t = ((p.dx - self.start.dx) * dx + (p.dy - self.start.dy) * dy) / (dx * dx + dy * dy).max(1e-12);
        self.min + f64::from(t) * (self.max - self.min)
    }

    /// Tick values (inclusive of both ends when they fall on the step).
    #[must_use]
    pub fn ticks(&self) -> Vec<f64> {
        if self.step <= 0.0 {
            return vec![self.min, self.max];
        }
        let first = (self.min / self.step).ceil() * self.step;
        let mut v = Vec::new();
        let mut x = first;
        while x <= self.max + self.step * 1e-9 {
            v.push((x / self.step).round() * self.step);
            x += self.step;
        }
        v
    }

    /// The line and its tick marks (`tick` long, perpendicular).
    #[must_use]
    pub fn path(&self, tick: f32) -> Path {
        let mut p = Path::new();
        p.move_to(self.start);
        p.line_to(self.end);
        let (dx, dy) = (self.end.dx - self.start.dx, self.end.dy - self.start.dy);
        let m = (dx * dx + dy * dy).sqrt().max(1e-9);
        let (nx, ny) = (-dy / m * tick * 0.5, dx / m * tick * 0.5);
        for t in self.ticks() {
            let q = self.number_to_point(t);
            p.move_to(Offset::new(q.dx - nx, q.dy - ny));
            p.line_to(Offset::new(q.dx + nx, q.dy + ny));
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_field_streamlines_are_circles() {
        let f = |x: f32, y: f32| (-y, x);
        let l = streamline(&f, (3.0, 0.0), (-5.0, -5.0, 5.0, 5.0), 0.05, 300, &[], 0.0);
        for p in &l {
            assert!(((p.0 * p.0 + p.1 * p.1).sqrt() - 3.0).abs() < 1e-3);
        }
    }

    #[test]
    fn streamlines_stop_at_a_sink_and_at_the_border() {
        let sink = |x: f32, y: f32| (-x, -y);
        let l = streamline(&sink, (2.0, 1.0), (-3.0, -3.0, 3.0, 3.0), 0.1, 1000, &[], 0.0);
        let first = l[0];
        assert!(first.0.abs() > 2.9 || first.1.abs() > 2.9, "backward runs to the border");
        let last = *l.last().unwrap();
        assert!(last.0.hypot(last.1) < 0.2, "forward ends at the sink");
    }

    #[test]
    fn even_spacing_fills_the_domain() {
        let f = |x: f32, y: f32| (1.0, (x * 0.5).sin() + y * 0.0);
        let ls = streamlines(&f, (0.0, 0.0, 20.0, 10.0), 1.0, 0.1);
        assert!(ls.len() >= 6, "{}", ls.len());
    }

    #[test]
    fn arrows_scale_with_magnitude() {
        let f = |x: f32, _y: f32| (x, 0.0);
        let a = arrows(&f, (0.0, 0.0, 10.0, 1.0), 5, 1, 2.0);
        assert_eq!(a.len(), 5);
        assert!(a[4].magnitude > a[0].magnitude);
    }

    #[test]
    fn number_line_maps_both_ways() {
        let nl = NumberLine { start: Offset::new(0.0, 0.0), end: Offset::new(100.0, 0.0), min: -5.0, max: 5.0, step: 1.0 };
        assert_eq!(nl.number_to_point(0.0).dx, 50.0);
        assert!((nl.point_to_number(Offset::new(75.0, 3.0)) - 2.5).abs() < 1e-6);
        assert_eq!(nl.ticks().len(), 11);
        assert!(!nl.path(4.0).is_empty());
    }
}
