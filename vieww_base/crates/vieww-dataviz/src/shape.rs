//! Shape generators — `d3-shape`: lines and areas through data with a
//! choice of curve, arcs, and the pie and stack layouts.

use std::f32::consts::{FRAC_PI_2, TAU};

use vieww_foundation::{Offset, Path};

/// How a line passes through its points — D3's curve factories.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Curve {
    /// Straight segments (`curveLinear`).
    #[default]
    Linear,
    /// Horizontal then vertical, the step at the midpoint (`curveStep`).
    Step,
    /// Cubic, monotone in x — never overshoots between points
    /// (`curveMonotoneX`, Fritsch–Carlson).
    MonotoneX,
    /// Through every point with tension (`curveCardinal`; 0 is
    /// Catmull–Rom).
    Cardinal(f32),
    /// A uniform cubic B-spline — smooth, does not pass through interior
    /// points (`curveBasis`).
    Basis,
}

/// A path through `points` along `curve`.
#[must_use]
pub fn line(points: &[Offset], curve: Curve) -> Path {
    let mut p = Path::new();
    let Some(&first) = points.first() else {
        return p;
    };
    p.move_to(first);
    append(&mut p, points, curve);
    p
}

fn append(p: &mut Path, points: &[Offset], curve: Curve) {
    let n = points.len();
    if n < 2 {
        return;
    }
    match curve {
        Curve::Linear => {
            for q in &points[1..] {
                p.line_to(*q);
            }
        }
        Curve::Step => {
            for w in points.windows(2) {
                let mid = (w[0].dx + w[1].dx) / 2.0;
                p.line_to(Offset::new(mid, w[0].dy));
                p.line_to(Offset::new(mid, w[1].dy));
                p.line_to(w[1]);
            }
        }
        Curve::Cardinal(tension) => {
            let k = (1.0 - tension) / 6.0;
            for i in 0..n - 1 {
                let p0 = points[i.saturating_sub(1)];
                let p1 = points[i];
                let p2 = points[i + 1];
                let p3 = points[(i + 2).min(n - 1)];
                let c1 = p1 + (p2 - p0).scale(k);
                let c2 = p2 - (p3 - p1).scale(k);
                p.cubic_to(c1, c2, p2);
            }
        }
        Curve::MonotoneX => {
            // Secant slopes, then Fritsch–Carlson tangents.
            let d: Vec<f32> = points
                .windows(2)
                .map(|w| {
                    let h = w[1].dx - w[0].dx;
                    if h.abs() < 1e-9 {
                        0.0
                    } else {
                        (w[1].dy - w[0].dy) / h
                    }
                })
                .collect();
            let mut m = vec![0.0f32; n];
            m[0] = d[0];
            m[n - 1] = d[n - 2];
            for i in 1..n - 1 {
                m[i] = if d[i - 1] * d[i] <= 0.0 {
                    0.0
                } else {
                    (d[i - 1] + d[i]) / 2.0
                };
            }
            for i in 0..n - 1 {
                if d[i] == 0.0 {
                    m[i] = 0.0;
                    m[i + 1] = 0.0;
                    continue;
                }
                let a = m[i] / d[i];
                let b = m[i + 1] / d[i];
                let s = a * a + b * b;
                if s > 9.0 {
                    let t = 3.0 / s.sqrt();
                    m[i] = t * a * d[i];
                    m[i + 1] = t * b * d[i];
                }
            }
            for i in 0..n - 1 {
                let (a, b) = (points[i], points[i + 1]);
                let h = (b.dx - a.dx) / 3.0;
                p.cubic_to(
                    Offset::new(a.dx + h, a.dy + m[i] * h),
                    Offset::new(b.dx - h, b.dy - m[i + 1] * h),
                    b,
                );
            }
        }
        Curve::Basis => {
            // Clamped uniform B-spline: repeat the end points.
            let mut pts = Vec::with_capacity(n + 4);
            pts.push(points[0]);
            pts.push(points[0]);
            pts.extend_from_slice(points);
            pts.push(points[n - 1]);
            pts.push(points[n - 1]);
            for w in pts.windows(4) {
                let (p0, p1, p2, p3) = (w[0], w[1], w[2], w[3]);
                let c1 = Offset::new((2.0 * p1.dx + p2.dx) / 3.0, (2.0 * p1.dy + p2.dy) / 3.0);
                let c2 = Offset::new((p1.dx + 2.0 * p2.dx) / 3.0, (p1.dy + 2.0 * p2.dy) / 3.0);
                let end = Offset::new(
                    (p1.dx + 4.0 * p2.dx + p3.dx) / 6.0,
                    (p1.dy + 4.0 * p2.dy + p3.dy) / 6.0,
                );
                let _ = p0;
                p.cubic_to(c1, c2, end);
            }
        }
    }
}

/// A closed area between a top line and a baseline — `d3.area()`.
/// `top[i]` and `base[i]` share an x.
#[must_use]
pub fn area(top: &[Offset], base: &[Offset], curve: Curve) -> Path {
    let mut p = line(top, curve);
    if let Some(last) = base.last() {
        p.line_to(*last);
        let rev: Vec<Offset> = base.iter().rev().copied().collect();
        append(&mut p, &rev, curve);
        p.close();
    }
    p
}

/// An annular sector — `d3.arc()`. Angles in radians clockwise from
/// twelve o'clock (D3's convention); `pad` is an angular gap per side.
#[must_use]
pub fn arc(center: Offset, inner: f32, outer: f32, start: f32, end: f32, pad: f32) -> Path {
    let (s, e) = (start + pad / 2.0, end - pad / 2.0);
    if e <= s {
        return Path::new();
    }
    // Path::arc angles are from +x; D3's from −y.
    let s0 = s - FRAC_PI_2;
    let sweep = e - s;
    // `arc_ring` takes the outer radius and the band's width; a width
    // reaching the centre gives the pie wedge.
    Path::arc_ring(center, outer, outer - inner.max(0.0), s0, sweep)
}

/// One slice of a pie layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slice {
    /// Index into the input values.
    pub index: usize,
    pub value: f32,
    pub start: f32,
    pub end: f32,
}

/// `d3.pie()`: angles for values, largest first when `sort`, spanning
/// `[start, end]` with `pad` between slices.
#[must_use]
pub fn pie(values: &[f32], sort: bool, start: f32, end: f32, pad: f32) -> Vec<Slice> {
    let total: f32 = values.iter().map(|v| v.max(0.0)).sum();
    let mut order: Vec<usize> = (0..values.len()).collect();
    if sort {
        order.sort_by(|&a, &b| values[b].total_cmp(&values[a]));
    }
    #[allow(clippy::cast_precision_loss)]
    let pads = pad * values.len() as f32;
    let span = (end - start - pads).max(0.0);
    let mut angle = start;
    let mut out = vec![
        Slice {
            index: 0,
            value: 0.0,
            start: 0.0,
            end: 0.0
        };
        values.len()
    ];
    for i in order {
        let v = values[i].max(0.0);
        let a = if total > 0.0 { span * v / total } else { 0.0 } + pad;
        out[i] = Slice {
            index: i,
            value: v,
            start: angle,
            end: angle + a,
        };
        angle += a;
    }
    out
}

/// How stacked series are offset — `d3.stackOffset*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackOffset {
    /// Zero baseline.
    None,
    /// Normalised to 0..1 at every x.
    Expand,
    /// Centred around zero (a streamgraph's silhouette).
    Silhouette,
}

/// `d3.stack()`: for each series and each x, `(lower, upper)`.
/// `series[s][x]` is series `s` at column `x`.
#[must_use]
pub fn stack(series: &[Vec<f32>], offset: StackOffset) -> Vec<Vec<(f32, f32)>> {
    let cols = series.iter().map(Vec::len).max().unwrap_or(0);
    let mut out = vec![vec![(0.0, 0.0); cols]; series.len()];
    #[allow(clippy::needless_range_loop)]
    for x in 0..cols {
        let total: f32 = series
            .iter()
            .map(|s| s.get(x).copied().unwrap_or(0.0))
            .sum();
        let base = match offset {
            StackOffset::Silhouette => -total / 2.0,
            _ => 0.0,
        };
        let scale = if offset == StackOffset::Expand && total > 0.0 {
            1.0 / total
        } else {
            1.0
        };
        let mut acc = base;
        for (s, row) in series.iter().enumerate() {
            let v = row.get(x).copied().unwrap_or(0.0) * scale;
            out[s][x] = (acc, acc + v);
            acc += v;
        }
    }
    out
}

/// Where a label sits on an arc — `arc.centroid()`.
#[must_use]
pub fn arc_centroid(center: Offset, inner: f32, outer: f32, start: f32, end: f32) -> Offset {
    let r = (inner + outer) / 2.0;
    let a = (start + end) / 2.0 - FRAC_PI_2;
    Offset::new(center.dx + r * a.cos(), center.dy + r * a.sin())
}

/// A full turn, for pie layouts.
pub const FULL: f32 = TAU;

#[cfg(test)]
mod tests {
    use super::*;
    use vieww_foundation::{signed_area, FillRule};

    fn pts(v: &[(f32, f32)]) -> Vec<Offset> {
        v.iter().map(|&(x, y)| Offset::new(x, y)).collect()
    }

    #[test]
    fn every_curve_passes_through_its_ends() {
        let p = pts(&[(0.0, 0.0), (10.0, 5.0), (20.0, -3.0), (30.0, 8.0)]);
        for c in [
            Curve::Linear,
            Curve::Step,
            Curve::MonotoneX,
            Curve::Cardinal(0.0),
            Curve::Basis,
        ] {
            let path = line(&p, c);
            let m = path.measure();
            let end = m.point_at_fraction(1.0).unwrap().position;
            assert!(
                (end - Offset::new(30.0, 8.0)).distance() < 1e-3,
                "{c:?} ends at {end:?}"
            );
        }
    }

    #[test]
    fn monotone_never_overshoots() {
        let p = pts(&[
            (0.0, 0.0),
            (1.0, 10.0),
            (2.0, 10.0),
            (3.0, 11.0),
            (4.0, 0.0),
        ]);
        let path = line(&p, Curve::MonotoneX);
        for c in path.flatten(0.01) {
            for q in c.points {
                assert!(q.dy <= 11.0 + 1e-3 && q.dy >= -1e-3, "{q:?}");
            }
        }
        // Cardinal does overshoot here (the contrast D3's docs draw).
        let card = line(&p, Curve::Cardinal(0.0));
        let max = card
            .flatten(0.01)
            .iter()
            .flat_map(|c| c.points.clone())
            .map(|q| q.dy)
            .fold(f32::MIN, f32::max);
        assert!(max > 11.0);
    }

    #[test]
    fn areas_are_closed_regions() {
        let top = pts(&[(0.0, 0.0), (10.0, 0.0)]);
        let base = pts(&[(0.0, 10.0), (10.0, 10.0)]);
        let a = area(&top, &base, Curve::Linear);
        assert!((signed_area(&a).abs() - 100.0).abs() < 1e-3);
    }

    #[test]
    fn pies_sum_to_the_span_and_pad() {
        let s = pie(&[1.0, 3.0, 2.0], true, 0.0, FULL, 0.0);
        assert!((s[1].start - 0.0).abs() < 1e-6, "largest first");
        let total: f32 = s.iter().map(|x| x.end - x.start).sum();
        assert!((total - FULL).abs() < 1e-5);
        assert!((s[1].end - s[1].start - FULL / 2.0).abs() < 1e-5);
        let padded = pie(&[1.0, 1.0], false, 0.0, FULL, 0.1);
        assert!((padded[1].end - FULL).abs() < 1e-5);
    }

    #[test]
    fn arcs_cover_the_right_region() {
        let c = Offset::new(0.0, 0.0);
        // First quarter from 12 o'clock clockwise: the upper-right quadrant.
        let a = arc(c, 20.0, 50.0, 0.0, FRAC_PI_2, 0.0);
        assert!(a.contains(Offset::new(25.0, -25.0), FillRule::NonZero));
        assert!(!a.contains(Offset::new(-25.0, -25.0), FillRule::NonZero));
        assert!(
            !a.contains(Offset::new(5.0, -5.0), FillRule::NonZero),
            "the hole"
        );
        let wedge = arc(c, 0.0, 50.0, 0.0, FRAC_PI_2, 0.0);
        assert!(wedge.contains(Offset::new(5.0, -5.0), FillRule::NonZero));
        let cen = arc_centroid(c, 20.0, 50.0, 0.0, FRAC_PI_2);
        assert!(cen.dx > 0.0 && cen.dy < 0.0);
    }

    #[test]
    fn stacks_accumulate_expand_and_centre() {
        let s = vec![vec![1.0, 2.0], vec![3.0, 2.0]];
        let z = stack(&s, StackOffset::None);
        assert_eq!(z[1][0], (1.0, 4.0));
        let e = stack(&s, StackOffset::Expand);
        assert_eq!(e[1][1], (0.5, 1.0));
        let m = stack(&s, StackOffset::Silhouette);
        assert_eq!(m[0][0].0, -2.0);
    }
}
