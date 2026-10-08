//! Free drawing — Fabric.js' `isDrawingMode` brushes (`PencilBrush`,
//! `CircleBrush`, `SprayBrush`, `PatternBrush`'s stroke) and the
//! pressure-sensitive ink of note apps, as path builders.
//!
//! A brush receives pointer samples (`down`, `move_to`, `up`) and produces
//! vector output that lands on a [`Stage`](crate::Stage) as ordinary shapes,
//! so drawn strokes are selectable, transformable and serialisable like
//! anything else.
//!
//! * [`PencilBrush`] smooths the raw samples (a moving-average plus a
//!   minimum spacing — Fabric's `decimate`) and fits a quadratic-midpoint
//!   curve through them (Fabric's own construction), so a shaky hand gives
//!   a clean line.
//! * [`InkBrush`] makes a **filled outline** whose width follows pressure
//!   and speed (fast = thin), with round caps — the calligraphic look.
//! * [`SprayBrush`] scatters dots in a radius per sample, deterministically
//!   seeded.
//! * [`CircleBrush`] stamps a circle per sample.

use vieww_foundation::{Offset, Path};

/// A pointer sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub at: Offset,
    /// 0..1 (1 for a mouse).
    pub pressure: f32,
    /// Seconds.
    pub time: f32,
}

fn dist(a: Offset, b: Offset) -> f32 {
    ((a.dx - b.dx).powi(2) + (a.dy - b.dy).powi(2)).sqrt()
}

fn mid(a: Offset, b: Offset) -> Offset {
    Offset::new((a.dx + b.dx) * 0.5, (a.dy + b.dy) * 0.5)
}

/// Fabric's pencil.
#[derive(Debug, Clone, PartialEq)]
pub struct PencilBrush {
    /// Drop samples closer than this to the last kept one.
    pub decimate: f32,
    /// Moving-average window (samples).
    pub smoothing: usize,
    pub width: f32,
    points: Vec<Offset>,
}

impl PencilBrush {
    #[must_use]
    pub fn new(width: f32) -> Self {
        Self {
            decimate: 2.0,
            smoothing: 3,
            width,
            points: Vec::new(),
        }
    }

    pub fn down(&mut self, p: Offset) {
        self.points.clear();
        self.points.push(p);
    }

    pub fn move_to(&mut self, p: Offset) {
        if self.points.last().is_none_or(|l| dist(*l, p) >= self.decimate) {
            self.points.push(p);
        }
    }

    /// Finish the stroke; the smoothed curve.
    pub fn up(&mut self) -> Path {
        let pts = std::mem::take(&mut self.points);
        smooth_path(&moving_average(&pts, self.smoothing))
    }

    /// The live stroke so far (for drawing while the pointer is down).
    #[must_use]
    pub fn preview(&self) -> Path {
        smooth_path(&self.points)
    }
}

/// Moving average with the endpoints pinned.
#[must_use]
pub fn moving_average(pts: &[Offset], window: usize) -> Vec<Offset> {
    if pts.len() < 3 || window < 2 {
        return pts.to_vec();
    }
    let half = window / 2;
    let mut out = Vec::with_capacity(pts.len());
    for i in 0..pts.len() {
        if i == 0 || i + 1 == pts.len() {
            out.push(pts[i]);
            continue;
        }
        let lo = i.saturating_sub(half);
        let hi = (i + half).min(pts.len() - 1);
        let n = (hi - lo + 1) as f32;
        let (sx, sy) = pts[lo..=hi].iter().fold((0.0, 0.0), |a, p| (a.0 + p.dx, a.1 + p.dy));
        out.push(Offset::new(sx / n, sy / n));
    }
    out
}

/// Quadratic midpoint smoothing: each sample is a control point, the curve
/// passes through the midpoints between samples (expressed as cubics).
#[must_use]
pub fn smooth_path(pts: &[Offset]) -> Path {
    let mut path = Path::new();
    match pts.len() {
        0 => return path,
        1 => {
            path.move_to(pts[0]);
            path.line_to(Offset::new(pts[0].dx + 0.01, pts[0].dy));
            return path;
        }
        2 => {
            path.move_to(pts[0]);
            path.line_to(pts[1]);
            return path;
        }
        _ => {}
    }
    path.move_to(pts[0]);
    let mut from = pts[0];
    for i in 1..pts.len() - 1 {
        let ctrl = pts[i];
        let to = mid(pts[i], pts[i + 1]);
        // Quadratic (from, ctrl, to) as a cubic.
        let c1 = Offset::new(from.dx + (ctrl.dx - from.dx) * 2.0 / 3.0, from.dy + (ctrl.dy - from.dy) * 2.0 / 3.0);
        let c2 = Offset::new(to.dx + (ctrl.dx - to.dx) * 2.0 / 3.0, to.dy + (ctrl.dy - to.dy) * 2.0 / 3.0);
        path.cubic_to(c1, c2, to);
        from = to;
    }
    path.line_to(pts[pts.len() - 1]);
    path
}

/// A variable-width ink stroke.
#[derive(Debug, Clone, PartialEq)]
pub struct InkBrush {
    pub min_width: f32,
    pub max_width: f32,
    /// Speed (px/s) at which the width reaches its minimum.
    pub thinning_speed: f32,
    samples: Vec<Sample>,
}

impl InkBrush {
    #[must_use]
    pub fn new(min_width: f32, max_width: f32) -> Self {
        Self {
            min_width,
            max_width,
            thinning_speed: 1500.0,
            samples: Vec::new(),
        }
    }

    pub fn down(&mut self, s: Sample) {
        self.samples.clear();
        self.samples.push(s);
    }

    pub fn move_to(&mut self, s: Sample) {
        if self.samples.last().is_none_or(|l| dist(l.at, s.at) >= 1.0) {
            self.samples.push(s);
        }
    }

    /// The widths the stroke will use, per kept sample.
    #[must_use]
    pub fn widths(&self) -> Vec<f32> {
        let n = self.samples.len();
        let mut w = Vec::with_capacity(n);
        let mut smoothed = self.max_width;
        for i in 0..n {
            let speed = if i == 0 {
                0.0
            } else {
                let dt = (self.samples[i].time - self.samples[i - 1].time).max(1e-3);
                dist(self.samples[i].at, self.samples[i - 1].at) / dt
            };
            let thin = (speed / self.thinning_speed).clamp(0.0, 1.0);
            let target = self.min_width + (self.max_width - self.min_width) * (1.0 - thin) * self.samples[i].pressure.clamp(0.0, 1.0);
            smoothed = smoothed * 0.6 + target * 0.4;
            w.push(smoothed.max(self.min_width));
        }
        w
    }

    /// Finish: the filled outline (left side out, round cap, right side
    /// back, round cap), fill with nonzero.
    pub fn up(&mut self) -> Path {
        let widths = self.widths();
        let pts: Vec<Offset> = self.samples.iter().map(|s| s.at).collect();
        self.samples.clear();
        let mut path = Path::new();
        if pts.is_empty() {
            return path;
        }
        if pts.len() == 1 {
            return Path::arc(pts[0], widths[0] * 0.5, 0.0, std::f32::consts::TAU);
        }
        let normal = |i: usize| {
            let a = pts[i.saturating_sub(1)];
            let b = pts[(i + 1).min(pts.len() - 1)];
            let (dx, dy) = (b.dx - a.dx, b.dy - a.dy);
            let l = (dx * dx + dy * dy).sqrt().max(1e-6);
            Offset::new(-dy / l, dx / l)
        };
        let left: Vec<Offset> = (0..pts.len())
            .map(|i| {
                let n = normal(i);
                Offset::new(pts[i].dx + n.dx * widths[i] * 0.5, pts[i].dy + n.dy * widths[i] * 0.5)
            })
            .collect();
        let right: Vec<Offset> = (0..pts.len())
            .map(|i| {
                let n = normal(i);
                Offset::new(pts[i].dx - n.dx * widths[i] * 0.5, pts[i].dy - n.dy * widths[i] * 0.5)
            })
            .collect();
        path.move_to(left[0]);
        for p in &left[1..] {
            path.line_to(*p);
        }
        // End cap: half circle around the last point.
        let last = pts.len() - 1;
        cap(&mut path, pts[last], widths[last] * 0.5, normal(last), 8);
        for p in right.iter().rev() {
            path.line_to(*p);
        }
        let n0 = normal(0);
        cap(&mut path, pts[0], widths[0] * 0.5, Offset::new(-n0.dx, -n0.dy), 8);
        path.close();
        path
    }
}

fn cap(path: &mut Path, c: Offset, r: f32, from_normal: Offset, steps: usize) {
    let a0 = from_normal.dy.atan2(from_normal.dx);
    for k in 1..=steps {
        let a = a0 - std::f32::consts::PI * k as f32 / steps as f32;
        path.line_to(Offset::new(c.dx + r * a.cos(), c.dy + r * a.sin()));
    }
}

/// Fabric's spray: `density` dots per sample within `radius`.
#[derive(Debug, Clone, PartialEq)]
pub struct SprayBrush {
    pub radius: f32,
    pub density: usize,
    pub dot: (f32, f32),
    seed: u64,
    dots: Vec<(Offset, f32)>,
}

impl SprayBrush {
    #[must_use]
    pub fn new(radius: f32, density: usize, seed: u64) -> Self {
        Self {
            radius,
            density,
            dot: (0.5, 1.5),
            seed: seed | 1,
            dots: Vec::new(),
        }
    }

    fn next(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        #[allow(clippy::cast_precision_loss)]
        let v = (self.seed >> 40) as f32 / (1u64 << 24) as f32;
        v
    }

    pub fn spray(&mut self, at: Offset) {
        for _ in 0..self.density {
            let a = self.next() * std::f32::consts::TAU;
            let r = self.radius * self.next().sqrt();
            let size = self.dot.0 + (self.dot.1 - self.dot.0) * self.next();
            self.dots.push((Offset::new(at.dx + r * a.cos(), at.dy + r * a.sin()), size));
        }
    }

    /// The dots so far, as (centre, radius).
    #[must_use]
    pub fn dots(&self) -> &[(Offset, f32)] {
        &self.dots
    }

    /// Finish: one path of all the dots.
    pub fn up(&mut self) -> Path {
        let mut p = Path::new();
        for (c, r) in self.dots.drain(..) {
            p.extend(&Path::arc(c, r, 0.0, std::f32::consts::TAU));
        }
        p
    }
}

/// Fabric's circle brush: a circle per sample, sized by pressure.
#[must_use]
pub fn circle_stamps(samples: &[Sample], radius: f32) -> Path {
    let mut p = Path::new();
    for s in samples {
        p.extend(&Path::arc(s.at, radius * s.pressure.clamp(0.1, 1.0), 0.0, std::f32::consts::TAU));
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pencil_decimates_and_smooths_a_jittery_line() {
        let mut b = PencilBrush::new(2.0);
        b.down(Offset::new(0.0, 0.0));
        for i in 1..200 {
            let x = i as f32 * 0.3;
            let jitter = if i % 2 == 0 { 0.6 } else { -0.6 };
            b.move_to(Offset::new(x, jitter));
        }
        let kept = b.points.len();
        assert!(kept < 100, "{kept}");
        let path = b.up();
        let bounds = path.bounds();
        assert!(bounds.height() < 1.2, "{bounds:?}");
        assert!(bounds.width() > 55.0);
    }

    #[test]
    fn moving_average_pins_endpoints() {
        let pts = vec![Offset::new(0.0, 0.0), Offset::new(1.0, 5.0), Offset::new(2.0, 0.0), Offset::new(3.0, 5.0)];
        let s = moving_average(&pts, 3);
        assert_eq!(s[0], pts[0]);
        assert_eq!(s[3], pts[3]);
        assert!(s[1].dy < 5.0 && s[1].dy > 0.0);
    }

    #[test]
    fn ink_is_thinner_when_fast_and_its_outline_encloses_the_path() {
        let mut ink = InkBrush::new(1.0, 10.0);
        ink.down(Sample { at: Offset::new(0.0, 0.0), pressure: 1.0, time: 0.0 });
        for i in 1..20 {
            ink.move_to(Sample { at: Offset::new(i as f32 * 2.0, 0.0), pressure: 1.0, time: i as f32 * 0.1 });
        }
        for i in 20..40 {
            ink.move_to(Sample { at: Offset::new(i as f32 * 20.0, 0.0), pressure: 1.0, time: 2.0 + (i - 19) as f32 * 0.005 });
        }
        let w = ink.widths();
        assert!(w[15] > w[38] * 2.0, "{} {}", w[15], w[38]);
        let outline = ink.up();
        assert!(outline.contains(Offset::new(20.0, 0.0), vieww_foundation::FillRule::NonZero));
        assert!(!outline.contains(Offset::new(20.0, 30.0), vieww_foundation::FillRule::NonZero));
    }

    #[test]
    fn spray_is_deterministic_and_stays_in_its_radius() {
        let mut a = SprayBrush::new(10.0, 20, 9);
        let mut b = SprayBrush::new(10.0, 20, 9);
        a.spray(Offset::new(50.0, 50.0));
        b.spray(Offset::new(50.0, 50.0));
        assert_eq!(a.dots(), b.dots());
        assert_eq!(a.dots().len(), 20);
        assert!(a.dots().iter().all(|(c, _)| dist(*c, Offset::new(50.0, 50.0)) <= 10.0 + 1e-4));
        assert!(!a.up().is_empty());
    }
}
