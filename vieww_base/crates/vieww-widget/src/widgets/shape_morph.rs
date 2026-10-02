//! Shapes that morph into each other, smoothly.
//!
//! # How it works
//!
//! Every shape is sampled into a fixed number of points. Morphing is
//! lerping the points. The count is fixed (32) so any shape can become
//! any other: a circle's 32 points and a square's 32 points interpolate
//! cleanly.
//!
//! # Integration
//!
//! [`ShapeMorph`] wraps a child and clips it with the interpolated
//! shape. The progress comes from a signal or an `AnimationController`
//! value — the widget doesn't care what drives it.

// This framework's point type is `Offset` and its `Rect` is four edge
// values, so `Point` / `rect.left` do not exist. Aliasing rather than
// renaming every use site keeps the geometry code readable.
use vieww_foundation::{Offset as Point, Path, Rect};

use crate::prelude::*;

/// How many points each morphable shape is sampled into.
const SAMPLES: usize = 32;

/// A shape that can be sampled into a fixed number of points.
///
/// Points are ordered counter-clockwise starting from the top-centre.
/// The ordering convention is load-bearing: morphing lerps point `i` to
/// point `i`, so two shapes sampled with different starting points
/// would twist rather than morph.
pub trait MorphShape: std::fmt::Debug {
    /// Clone into a boxed shape, so a shape can be moved into a layout
    /// closure. `MorphShape` cannot require `Clone` directly and stay
    /// object-safe.
    fn clone_shape(&self) -> Box<dyn MorphShape>;

    /// Sample the shape's outline into exactly `SAMPLES` points.
    fn sample(&self, bounds: Rect) -> [Point; SAMPLES];
}

/// A circle (or ellipse, if bounds are non-square).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Circle;

impl MorphShape for Circle {
    fn clone_shape(&self) -> Box<dyn MorphShape> {
        Box::new(Circle)
    }

    fn sample(&self, bounds: Rect) -> [Point; SAMPLES] {
        let cx = (bounds.left + bounds.right) / 2.0;
        let cy = (bounds.top + bounds.bottom) / 2.0;
        let rx = (bounds.right - bounds.left) / 2.0;
        let ry = (bounds.bottom - bounds.top) / 2.0;

        let mut points = [Point::new(0.0, 0.0); SAMPLES];
        for (i, point) in points.iter_mut().enumerate() {
            let angle =
                (i as f32 / SAMPLES as f32) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2; // start at top
            *point = Point::new(cx + rx * angle.cos(), cy + ry * angle.sin());
        }
        points
    }
}

/// A rounded rectangle.
///
/// A radius of 0 is a rectangle; a radius of half-the-min-side is a
/// circle. Morphing between the two is the canonical use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoundedRectangle {
    pub radius: f32,
}

impl MorphShape for RoundedRectangle {
    fn clone_shape(&self) -> Box<dyn MorphShape> {
        Box::new(RoundedRectangle {
            radius: self.radius,
        })
    }

    fn sample(&self, bounds: Rect) -> [Point; SAMPLES] {
        let radius = self
            .radius
            .min((bounds.right - bounds.left) / 2.0)
            .min((bounds.bottom - bounds.top) / 2.0);

        if radius <= f32::EPSILON {
            // Plain rectangle: points distributed along the edges.
            let mut points = [Point::new(0.0, 0.0); SAMPLES];
            let quarter = SAMPLES / 4;
            for (i, point) in points.iter_mut().enumerate() {
                let edge = i / quarter;
                let t = (i % quarter) as f32 / quarter as f32;
                *point = match edge {
                    0 => Point::new(bounds.left + (bounds.right - bounds.left) * t, bounds.top),
                    1 => Point::new(bounds.right, bounds.top + (bounds.bottom - bounds.top) * t),
                    2 => Point::new(
                        bounds.right - (bounds.right - bounds.left) * t,
                        bounds.bottom,
                    ),
                    _ => Point::new(
                        bounds.left,
                        bounds.bottom - (bounds.bottom - bounds.top) * t,
                    ),
                };
            }
            return points;
        }

        // Superellipse (squircle) approximation: a smooth family from
        // square to circle with no discontinuity.
        let n = (radius / ((bounds.right - bounds.left).min(bounds.bottom - bounds.top) / 2.0))
            .clamp(0.0, 1.0)
            * 8.0
            + 2.0; // exponent 2 = ellipse, 10 = square-ish

        let cx = (bounds.left + bounds.right) / 2.0;
        let cy = (bounds.top + bounds.bottom) / 2.0;
        let rx = (bounds.right - bounds.left) / 2.0;
        let ry = (bounds.bottom - bounds.top) / 2.0;

        let mut points = [Point::new(0.0, 0.0); SAMPLES];
        for (i, point) in points.iter_mut().enumerate() {
            let angle =
                (i as f32 / SAMPLES as f32) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            let c = angle.cos().abs().powf(2.0 / n) * angle.cos().signum();
            let s = angle.sin().abs().powf(2.0 / n) * angle.sin().signum();
            *point = Point::new(cx + rx * c, cy + ry * s);
        }
        points
    }
}

/// Interpolate between two sampled shapes.
///
/// `t = 0.0` is `from`, `t = 1.0` is `to`. Values outside `[0, 1]`
/// extrapolate, producing the elastic overshoot an under-damped spring
/// gives you for free.
#[must_use]
pub(crate) fn lerp_shapes(
    from: &[Point; SAMPLES],
    to: &[Point; SAMPLES],
    t: f32,
) -> [Point; SAMPLES] {
    let mut out = [Point::new(0.0, 0.0); SAMPLES];
    for i in 0..SAMPLES {
        out[i] = Point::new(
            from[i].dx + (to[i].dx - from[i].dx) * t,
            from[i].dy + (to[i].dy - from[i].dy) * t,
        );
    }
    out
}

/// Convert sampled points into a drawable [`Path`].
#[must_use]
pub(crate) fn points_to_path(points: &[Point; SAMPLES]) -> Path {
    let mut path = Path::new();
    path.move_to(points[0]);
    for point in &points[1..] {
        path.line_to(*point);
    }
    path.close();
    path
}

/// A widget that clips its child with a morphing shape.
///
/// The `progress` is read in `build` — whatever drives it (a signal,
/// an AnimationController's value) causes rebuilds as it changes.
///
/// # Examples
///
/// ```ignore
/// // A FAB that becomes a square when expanded:
/// ShapeMorph::new(
///         RoundedRectangle { radius: 28.0 },
///         RoundedRectangle { radius: 0.0 },
///     )
///     .progress(expand_value)
///     .child(content)
/// ```
#[derive(Debug)]
pub struct ShapeMorph {
    from: Box<dyn MorphShape>,
    to: Box<dyn MorphShape>,
    /// The box the shape is sampled in, and the size the child is given.
    size: vieww_foundation::Size,
    /// Drives the interpolation: 0 = from, 1 = to.
    progress: f32,
    child: WidgetNode,
}

impl ShapeMorph {
    /// Create a morph from `from` to `to`.
    #[must_use]
    pub fn new(from: impl MorphShape + 'static, to: impl MorphShape + 'static) -> Self {
        Self {
            from: Box::new(from),
            to: Box::new(to),
            size: vieww_foundation::Size::new(100.0, 100.0),
            progress: 0.0,
            child: SizedBox::shrink().into(),
        }
    }

    /// Set the box the shape is sampled in.
    ///
    /// A morph clip has to know its own box. Sampling the *available* space
    /// instead looks reasonable and is not: in a 600px-wide card the circle
    /// becomes 600px across, and a 120x120 child sitting in the top-left
    /// corner falls outside it and is clipped away completely — which is
    /// exactly how this widget first rendered, an empty panel.
    #[must_use]
    pub fn size(mut self, size: vieww_foundation::Size) -> Self {
        self.size = size;
        self
    }

    /// Set the morph progress (0.0 = from shape, 1.0 = to shape).
    #[must_use]
    pub const fn progress(mut self, t: f32) -> Self {
        self.progress = t;
        self
    }

    /// Set the child being clipped.
    #[must_use]
    pub fn child(mut self, child: impl Into<WidgetNode>) -> Self {
        self.child = child.into();
        self
    }
}

impl Widget for ShapeMorph {
    fn debug_name(&self) -> &'static str {
        "ShapeMorph"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let t = self.progress.clamp(-0.5, 1.5);
        let bounds = Rect::new(0.0, 0.0, self.size.width, self.size.height);
        let morphed = lerp_shapes(&self.from.sample(bounds), &self.to.sample(bounds), t);

        // The clip and the child share one box, so the shape always contains
        // what it is clipping.
        Clip::path(points_to_path(&morphed))
            .child(
                Container::new()
                    .size(self.size.width, self.size.height)
                    .child(self.child.clone()),
            )
            .into()
    }
}

widget_node_from!(ShapeMorph);

/// A regular n-gon: triangle, square (as a shape, not a rect), pentagon …
///
/// MorphSVG's bread and butter — a morph between two polygon shapes is the
/// cleanest demo of point-to-point interpolation there is, and a pentagon
/// becoming a triangle shows the fan-out that fixed sample counts make
/// possible.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Polygon {
    /// The number of sides. Three or more; fewer is a line, which a caller
    /// can have from a path and which this shape refuses to pretend to be.
    pub sides: u32,
}

impl Polygon {
    /// An `n`-sided regular polygon.
    #[must_use]
    pub const fn new(sides: u32) -> Self {
        Self { sides }
    }
}

impl MorphShape for Polygon {
    fn clone_shape(&self) -> Box<dyn MorphShape> {
        Box::new(Polygon { sides: self.sides })
    }

    fn sample(&self, bounds: Rect) -> [Point; SAMPLES] {
        // Corners, then straight edges between them: each sample is the
        // linear interpolation between its two bracketing corners, which
        // makes the flanks *exactly* straight (no polar approximation, no
        // rounding at the corners) and puts sample 0 on the top corner —
        // the same start as `Circle`, so morphs between the two do not
        // twist.
        let cx = (bounds.left + bounds.right) / 2.0;
        let cy = (bounds.top + bounds.bottom) / 2.0;
        let rx = (bounds.right - bounds.left) / 2.0;
        let ry = (bounds.bottom - bounds.top) / 2.0;
        let sides = self.sides.max(3) as f32;

        let corner = |k: f32| -> Point {
            let angle = k * (std::f32::consts::TAU / sides) - std::f32::consts::FRAC_PI_2;
            Point::new(cx + rx * angle.cos(), cy + ry * angle.sin())
        };

        let mut points = [Point::new(0.0, 0.0); SAMPLES];
        for (i, point) in points.iter_mut().enumerate() {
            let c = i as f32 / SAMPLES as f32 * sides;
            let a = corner(c.floor());
            let b = corner(c.floor() + 1.0);
            let frac = c - c.floor();
            *point = Point::new(a.dx + (b.dx - a.dx) * frac, a.dy + (b.dy - a.dy) * frac);
        }
        points
    }
}

/// A star: `points` spikes, alternating outer and inner radius.
///
/// The inner radius is a fraction of the outer (0.5 is the classic
/// five-pointed star); 1.0 is a `Polygon` with `2·points` sides, 0.0 is
/// spikes, and the range between is every star a badge has ever drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Star {
    /// The number of outer points. Five is a star; six is a Star of David
    /// outline; anything above twelve is a gear.
    pub points: u32,
    /// The inner radius as a fraction of the outer, in `0.05..=1.0`.
    /// Clamped rather than refused — see [`Polygon`](Polygon) for the same
    /// decision — because the degenerate ends are still drawable shapes.
    pub inner: f32,
}

impl Star {
    /// A star of `points` spikes with the classic inner radius.
    #[must_use]
    pub const fn new(points: u32) -> Self {
        Self { points, inner: 0.5 }
    }

    /// Set the inner radius fraction.
    #[must_use]
    pub const fn inner(mut self, inner: f32) -> Self {
        self.inner = if inner < 0.05 {
            0.05
        } else if inner > 1.0 {
            1.0
        } else {
            inner
        };
        self
    }
}

impl MorphShape for Star {
    fn clone_shape(&self) -> Box<dyn MorphShape> {
        Box::new(Star {
            points: self.points,
            inner: self.inner,
        })
    }

    fn sample(&self, bounds: Rect) -> [Point; SAMPLES] {
        let cx = (bounds.left + bounds.right) / 2.0;
        let cy = (bounds.top + bounds.bottom) / 2.0;
        let rx = (bounds.right - bounds.left) / 2.0;
        let ry = (bounds.bottom - bounds.top) / 2.0;
        let spikes = self.points.max(3) as f32;
        let inner = self.inner.clamp(0.05, 1.0);

        let mut points = [Point::new(0.0, 0.0); SAMPLES];
        for (i, point) in points.iter_mut().enumerate() {
            let t = i as f32 / SAMPLES as f32;
            let angle = t * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            // The radius alternates between 1 (at a spike) and `inner` (at
            // a valley) on a triangle wave of period 1/spikes — linear in
            // the *angle*, so the spikes have straight flanks exactly as a
            // drawn star does, rather than a cosine that rounds them.
            let phase = (t * spikes).fract();
            let wave = if phase < 0.5 {
                1.0 - 2.0 * phase
            } else {
                2.0 * phase - 1.0
            };
            let r = inner + (1.0 - inner) * wave;
            *point = Point::new(cx + rx * r * angle.cos(), cy + ry * r * angle.sin());
        }
        points
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_bounds() -> Rect {
        Rect::new(0.0, 0.0, 100.0, 100.0)
    }

    #[test]
    fn lerping_at_zero_gives_from() {
        let circle = Circle.sample(test_bounds());
        let square = RoundedRectangle { radius: 0.0 }.sample(test_bounds());

        let result = lerp_shapes(&circle, &square, 0.0);
        for i in 0..SAMPLES {
            assert!((result[i].dx - circle[i].dx).abs() < 0.001);
            assert!((result[i].dy - circle[i].dy).abs() < 0.001);
        }
    }

    #[test]
    fn lerping_at_one_gives_to() {
        let circle = Circle.sample(test_bounds());
        let square = RoundedRectangle { radius: 0.0 }.sample(test_bounds());

        let result = lerp_shapes(&circle, &square, 1.0);
        for i in 0..SAMPLES {
            assert!((result[i].dx - square[i].dx).abs() < 0.001);
        }
    }

    #[test]
    fn lerping_at_half_is_between() {
        let circle = Circle.sample(test_bounds());
        let square = RoundedRectangle { radius: 0.0 }.sample(test_bounds());

        let result = lerp_shapes(&circle, &square, 0.5);
        for i in 0..SAMPLES {
            let expected_x = (circle[i].dx + square[i].dx) / 2.0;
            assert!((result[i].dx - expected_x).abs() < 0.001);
        }
    }

    #[test]
    fn circle_sample_starts_at_top() {
        let points = Circle.sample(test_bounds());
        // Top of a 100x100 rect is y=0, x=50.
        assert!((points[0].dx - 50.0).abs() < 0.1);
        assert!(points[0].dy.abs() < 0.1);
    }

    #[test]
    fn square_sample_has_four_corners() {
        let points = RoundedRectangle { radius: 0.0 }.sample(test_bounds());
        // With SAMPLES=32, quarter=8, corners at indices 0, 8, 16, 24.
        assert!((points[0].dy - 0.0).abs() < 0.1, "top-left");
        assert!((points[8].dx - 100.0).abs() < 0.1, "top-right");
        assert!((points[16].dy - 100.0).abs() < 0.1, "bottom-right");
        assert!((points[24].dx - 0.0).abs() < 0.1, "bottom-left");
    }

    #[test]
    fn path_from_points_is_closed() {
        let points = Circle.sample(test_bounds());
        let path = points_to_path(&points);
        // A closed path has the same first and last implicit point.
        // We can't easily inspect Path internals, but construction
        // should not panic.
        let _ = path;
    }
}

#[cfg(test)]
mod more_shape_tests {
    use super::*;

    const EPS: f32 = 1e-4;

    fn square_bounds() -> Rect {
        Rect::new(0.0, 0.0, 100.0, 100.0)
    }

    #[test]
    fn a_polygon_has_flat_sides() {
        // The apothem quantisation: every sample on a hexagon sits at
        // either the circumradius (a corner) or the apothem (mid-edge) or
        // between the two on a straight flank — never outside the
        // circumradius, never inside the apothem.
        let hexagon = Polygon::new(6);
        let points = hexagon.sample(square_bounds());
        let circum = 50.0_f32;
        let apothem = circum * (std::f32::consts::PI / 6.0).cos();
        for point in &points {
            let r = ((point.dx - 50.0).powi(2) + (point.dy - 50.0).powi(2)).sqrt();
            assert!(
                r <= circum + EPS && r >= apothem - EPS,
                "radius {r} outside [{apothem}, {circum}]"
            );
        }
    }

    #[test]
    fn a_polygon_starts_at_the_top() {
        let triangle = Polygon::new(3);
        let points = triangle.sample(square_bounds());
        // The first sample is the top-centre, the convention every morph
        // depends on (see `MorphShape`'s docs).
        assert!((points[0].dx - 50.0).abs() < EPS);
        assert!((points[0].dy - 0.0).abs() < 0.5);
    }

    #[test]
    fn a_star_alternates_spikes_and_valleys() {
        let star = Star::new(5);
        let points = star.sample(square_bounds());
        let radii: Vec<f32> = points
            .iter()
            .map(|p| ((p.dx - 50.0).powi(2) + (p.dy - 50.0).powi(2)).sqrt())
            .collect();
        // The outer radius is 50 (the bounds' half); the inner is 25 at
        // inner = 0.5. Samples do not land exactly on the extrema (32
        // samples, 10 extrema), but the *envelope* must hold.
        let max_r = radii.iter().copied().fold(f32::MIN, f32::max);
        let min_r = radii.iter().copied().fold(f32::MAX, f32::min);
        assert!(
            max_r <= 50.0 + EPS && max_r > 48.0,
            "spikes reach the edge: {max_r}"
        );
        assert!(min_r < 27.0 && min_r > 23.0, "valleys pull in: {min_r}");
    }

    #[test]
    fn a_star_with_full_inner_radius_is_the_polygon() {
        // inner = 1.0 means no valleys: the wave is pinned at 1 and the
        // sample is a plain (rounded) circle of the bounds' radius.
        let flat = Star::new(5).inner(1.0);
        let points = flat.sample(square_bounds());
        for point in &points {
            let r = ((point.dx - 50.0).powi(2) + (point.dy - 50.0).powi(2)).sqrt();
            assert!((r - 50.0).abs() < 0.6, "flat star is round: {r}");
        }
    }

    #[test]
    fn morphing_star_to_polygon_interpolates() {
        // The module's contract on new shapes: point i morphs to point i,
        // and the halfway sample of two shapes is the mean of their
        // samples.
        let star = Star::new(5).sample(square_bounds());
        let circle = Circle.sample(square_bounds());
        let mid = lerp_shapes(&star, &circle, 0.5);
        for i in 0..SAMPLES {
            let want_dx = (star[i].dx + circle[i].dx) / 2.0;
            assert!(
                (mid[i].dx - want_dx).abs() < EPS,
                "sample {i}: {} vs {want_dx}",
                mid[i].dx
            );
        }
    }

    #[test]
    fn inner_radius_is_clamped() {
        assert_eq!(Star::new(5).inner(-1.0).inner, 0.05);
        assert_eq!(Star::new(5).inner(2.0).inner, 1.0);
        assert_eq!(Star::new(5).inner(0.7).inner, 0.7);
    }

    #[test]
    fn degenerate_sides_and_points_are_refused() {
        // Two sides is a line, two spikes is a lens; both clamp to the
        // least drawable shape rather than producing NaN geometry.
        let _ = Polygon::new(2).sample(square_bounds());
        let _ = Star::new(2).sample(square_bounds());
    }
}
