//! Path operations: measuring, walking, trimming, hit-testing and combining.
//!
//! # What the comparison document asked for
//!
//! Every vector-animation tool in the document does more with a path than
//! fill it:
//!
//! | capability | who has it | here |
//! |---|---|---|
//! | line-drawing / "draw on" | GSAP DrawSVG, CSS `stroke-dashoffset`, After Effects *Trim Paths*, Manim `Create` | [`Path::trim`] |
//! | motion along a path | GSAP MotionPathPlugin, AE auto-orient, Blender *Follow Path* | [`PathMeasure::point_at`] |
//! | pixel-exact hit testing of arbitrary shapes | Konva's hit canvas, Fabric's `containsPoint`, PixiJS hit areas | [`Path::contains`] |
//! | boolean path operations | Paper.js `unite`/`intersect`/`subtract`/`exclude`, Figma, AE merge paths | [`Path::boolean`] |
//! | vector export | Fabric `toSVG`, Processing `PGraphicsPDF` | [`Path::to_svg_data`] (and `Sketchbook::to_svg`) |
//! | dash generation | every stroker | [`Path::dashed`] |
//!
//! `path.rs`' own header said "no boolean operations" and pointed at two
//! substitutes (todo-upgrades U-13). That was the right scope for a UI
//! toolkit and the wrong one for a framework measured against Paper.js and
//! After Effects, so this file closes it — while keeping `path.rs` the
//! vocabulary type and putting the geometry here.
//!
//! # Curves stay curves where they can
//!
//! Measuring needs arc length, and a cubic has no closed form for it, so each
//! cubic carries a small arc-length table (24 samples). [`Path::trim`] and
//! [`PathMeasure::segment`] use it to find the *parameter* at a distance and
//! then split the cubic by de Casteljau — the trimmed path is still cubics,
//! not a polyline, so a trimmed circle is still round at any zoom.
//!
//! Hit-testing and booleans flatten (to [`DEFAULT_TOLERANCE`]), because both
//! are questions about the *region*, and a region is exact enough at a
//! twentieth of a pixel.
//!
//! # How the booleans work — and why this way
//!
//! Textbook polygon clippers (Greiner–Hormann, Martinez–Rueda, Vatti) walk
//! intersection graphs and are famous for their degenerate cases: shared
//! edges, touching vertices, self-intersections. This one uses the
//! *arrangement* formulation instead, which has no such cases to special-case:
//!
//! 1. Flatten both operands and split every edge at every intersection with
//!    every other edge (including its own path's — self-intersections are
//!    just intersections).
//! 2. For each resulting sub-edge, probe a point a hair to its left and a hair
//!    to its right, and ask each operand "is this inside you?" under its fill
//!    rule. The boolean `op` of the two answers says whether the *result*
//!    covers that side.
//! 3. An edge is on the result's boundary exactly when the two sides differ.
//!    Keep those, oriented so the covered side is on the left.
//! 4. Link the kept edges into closed loops.
//!
//! Because every kept loop has the region on the same side, the output fills
//! correctly under the **nonzero** rule — holes come out wound the other way
//! automatically. Coincident edges from both operands produce two identical
//! kept edges; they are deduplicated. The cost is O(n²) in edge count, which
//! for the shapes a UI or a motion graphic combines (hundreds of edges) is
//! microseconds to a millisecond.

use crate::{Offset, Path, PathVerb, Rect};

/// Flattening tolerance used by [`Path::contains`] and [`Path::boolean`]:
/// the largest distance, in the path's units, between a curve and its chord.
pub const DEFAULT_TOLERANCE: f32 = 0.05;

/// Samples per cubic in the arc-length table.
const LUT: usize = 24;

/// Which points a fill covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum FillRule {
    /// Covered where the winding number is not zero. SVG's and the
    /// rasterizer's default.
    #[default]
    NonZero,
    /// Covered where an odd number of edges lie between the point and
    /// infinity.
    EvenOdd,
}

impl FillRule {
    /// Whether a point with this winding number is covered.
    #[must_use]
    pub const fn covers(self, winding: i32) -> bool {
        match self {
            Self::NonZero => winding != 0,
            Self::EvenOdd => winding % 2 != 0,
        }
    }
}

/// A boolean combination of two regions — Paper.js' four.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathOp {
    /// Covered by either (`unite`).
    Union,
    /// Covered by both (`intersect`).
    Intersect,
    /// Covered by the first and not the second (`subtract`).
    Difference,
    /// Covered by exactly one (`exclude`).
    Xor,
}

impl PathOp {
    const fn apply(self, a: bool, b: bool) -> bool {
        match self {
            Self::Union => a || b,
            Self::Intersect => a && b,
            Self::Difference => a && !b,
            Self::Xor => a != b,
        }
    }
}

/// One subpath flattened to points.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Contour {
    pub points: Vec<Offset>,
    /// Ended in `close()`. A fill treats every contour as closed regardless.
    pub closed: bool,
}

/// A position on a path, and which way the path is heading there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathPoint {
    pub position: Offset,
    /// The tangent's direction in radians, clockwise from +x in Y-down
    /// space — the rotation an object riding the path takes ("auto-orient").
    pub angle: f32,
}

fn add(a: Offset, b: Offset) -> Offset {
    Offset::new(a.dx + b.dx, a.dy + b.dy)
}
fn sub(a: Offset, b: Offset) -> Offset {
    Offset::new(a.dx - b.dx, a.dy - b.dy)
}
fn mul(a: Offset, s: f32) -> Offset {
    Offset::new(a.dx * s, a.dy * s)
}
fn lerp(a: Offset, b: Offset, t: f32) -> Offset {
    add(a, mul(sub(b, a), t))
}
fn cross(a: Offset, b: Offset) -> f32 {
    a.dx * b.dy - a.dy * b.dx
}

fn cubic_point(p0: Offset, c1: Offset, c2: Offset, p3: Offset, t: f32) -> Offset {
    let u = 1.0 - t;
    let a = u * u * u;
    let b = 3.0 * u * u * t;
    let c = 3.0 * u * t * t;
    let d = t * t * t;
    Offset::new(
        a * p0.dx + b * c1.dx + c * c2.dx + d * p3.dx,
        a * p0.dy + b * c1.dy + c * c2.dy + d * p3.dy,
    )
}

fn cubic_derivative(p0: Offset, c1: Offset, c2: Offset, p3: Offset, t: f32) -> Offset {
    let u = 1.0 - t;
    let a = mul(sub(c1, p0), 3.0 * u * u);
    let b = mul(sub(c2, c1), 6.0 * u * t);
    let c = mul(sub(p3, c2), 3.0 * t * t);
    add(add(a, b), c)
}

/// Split a cubic at `t`, returning both halves' control points.
fn split_cubic(
    p: [Offset; 4],
    t: f32,
) -> ([Offset; 4], [Offset; 4]) {
    let p01 = lerp(p[0], p[1], t);
    let p12 = lerp(p[1], p[2], t);
    let p23 = lerp(p[2], p[3], t);
    let a = lerp(p01, p12, t);
    let b = lerp(p12, p23, t);
    let mid = lerp(a, b, t);
    ([p[0], p01, a, mid], [mid, b, p23, p[3]])
}

/// The piece of a cubic between parameters `t0 < t1`.
fn sub_cubic(p: [Offset; 4], t0: f32, t1: f32) -> [Offset; 4] {
    let (left, _) = split_cubic(p, t1);
    if t1 <= 0.0 {
        return [p[0]; 4];
    }
    let (_, right) = split_cubic(left, (t0 / t1).clamp(0.0, 1.0));
    right
}

/// How many chords a cubic needs to stay within `tolerance` of the curve —
/// Wang's formula for degree three.
fn cubic_steps(p: [Offset; 4], tolerance: f32) -> usize {
    let d1 = add(sub(p[0], mul(p[1], 2.0)), p[2]);
    let d2 = add(sub(p[1], mul(p[2], 2.0)), p[3]);
    let m = d1.distance().max(d2.distance());
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (0.75 * m / tolerance.max(1e-4)).sqrt().ceil() as usize;
    n.clamp(1, 512)
}

#[derive(Debug, Clone)]
enum SegKind {
    Line,
    /// Cumulative arc length at `t = i / LUT`, `i = 0..=LUT`.
    Cubic { c1: Offset, c2: Offset, lut: Box<[f32; LUT + 1]> },
}

#[derive(Debug, Clone)]
struct Segment {
    from: Offset,
    to: Offset,
    kind: SegKind,
    /// Distance from the path's start to this segment's start.
    start: f32,
    length: f32,
    /// Which subpath this segment belongs to.
    contour: usize,
}

impl Segment {
    fn t_at(&self, local: f32) -> f32 {
        match &self.kind {
            SegKind::Line => {
                if self.length <= 0.0 {
                    0.0
                } else {
                    (local / self.length).clamp(0.0, 1.0)
                }
            }
            SegKind::Cubic { lut, .. } => {
                let local = local.clamp(0.0, self.length);
                let i = lut.partition_point(|&d| d < local).clamp(1, LUT);
                let (d0, d1) = (lut[i - 1], lut[i]);
                let frac = if d1 > d0 { (local - d0) / (d1 - d0) } else { 0.0 };
                #[allow(clippy::cast_precision_loss)]
                let t = ((i - 1) as f32 + frac) / LUT as f32;
                t.clamp(0.0, 1.0)
            }
        }
    }

    fn point(&self, t: f32) -> PathPoint {
        match &self.kind {
            SegKind::Line => {
                let d = sub(self.to, self.from);
                PathPoint {
                    position: lerp(self.from, self.to, t),
                    angle: d.dy.atan2(d.dx),
                }
            }
            SegKind::Cubic { c1, c2, .. } => {
                let mut d = cubic_derivative(self.from, *c1, *c2, self.to, t);
                if d.distance_squared() < 1e-12 {
                    // A cusp or coincident control point: look slightly inward.
                    let nudged = if t < 0.5 { t + 1e-3 } else { t - 1e-3 };
                    d = cubic_derivative(self.from, *c1, *c2, self.to, nudged);
                }
                PathPoint {
                    position: cubic_point(self.from, *c1, *c2, self.to, t),
                    angle: d.dy.atan2(d.dx),
                }
            }
        }
    }

    /// Append the part of this segment between local distances `a < b`.
    fn append_range(&self, a: f32, b: f32, out: &mut Path, needs_move: bool) {
        let (t0, t1) = (self.t_at(a), self.t_at(b));
        match &self.kind {
            SegKind::Line => {
                let p0 = lerp(self.from, self.to, t0);
                let p1 = lerp(self.from, self.to, t1);
                if needs_move {
                    out.move_to(p0);
                }
                out.line_to(p1);
            }
            SegKind::Cubic { c1, c2, .. } => {
                let piece = sub_cubic([self.from, *c1, *c2, self.to], t0, t1);
                if needs_move {
                    out.move_to(piece[0]);
                }
                out.cubic_to(piece[1], piece[2], piece[3]);
            }
        }
    }
}

/// A path prepared for distance queries: length, point-at-distance, and
/// extracting a stretch of it.
///
/// Build it once with [`Path::measure`] and query it every frame — the arc
/// length tables are the expensive part.
#[derive(Debug, Clone, Default)]
pub struct PathMeasure {
    segments: Vec<Segment>,
    total: f32,
}

impl PathMeasure {
    /// Total length of every subpath, closing segments included.
    #[must_use]
    pub const fn length(&self) -> f32 {
        self.total
    }

    /// The point `distance` along the path, clamped to its ends. `None` for
    /// an empty path.
    #[must_use]
    pub fn point_at(&self, distance: f32) -> Option<PathPoint> {
        if self.segments.is_empty() {
            return None;
        }
        let d = distance.clamp(0.0, self.total);
        let index = self
            .segments
            .partition_point(|s| s.start + s.length < d)
            .min(self.segments.len() - 1);
        let seg = &self.segments[index];
        Some(seg.point(seg.t_at(d - seg.start)))
    }

    /// The point at `fraction` (0..=1) of the total length.
    #[must_use]
    pub fn point_at_fraction(&self, fraction: f32) -> Option<PathPoint> {
        self.point_at(fraction.clamp(0.0, 1.0) * self.total)
    }

    /// The stretch of the path from `from` to `to` (distances), as a new
    /// path. Curves stay cubic. Crossing into another subpath starts a new
    /// subpath in the output, so a trimmed "i" does not grow a line between
    /// its stem and its dot.
    #[must_use]
    pub fn segment(&self, from: f32, to: f32) -> Path {
        let mut out = Path::new();
        let (from, to) = (from.max(0.0), to.min(self.total));
        if to <= from {
            return out;
        }
        let mut last_contour = None;
        let mut pen: Option<Offset> = None;
        for seg in &self.segments {
            let s0 = seg.start;
            let s1 = seg.start + seg.length;
            if s1 < from || s0 > to || seg.length <= 0.0 {
                continue;
            }
            let a = (from - s0).max(0.0);
            let b = (to - s0).min(seg.length);
            if b <= a {
                continue;
            }
            let start_point = seg.point(seg.t_at(a)).position;
            let needs_move = last_contour != Some(seg.contour)
                || pen.is_none_or(|p| sub(p, start_point).distance() > 1e-3);
            seg.append_range(a, b, &mut out, needs_move);
            last_contour = Some(seg.contour);
            pen = Some(seg.point(seg.t_at(b)).position);
        }
        out
    }
}

impl Path {
    /// A quadratic Bézier, stored as the cubic it is exactly equal to.
    pub fn quad_to(&mut self, control: Offset, end: Offset) -> &mut Self {
        let start = self.current_point().unwrap_or(control);
        let c1 = lerp(start, control, 2.0 / 3.0);
        let c2 = lerp(end, control, 2.0 / 3.0);
        self.cubic_to(c1, c2, end)
    }

    /// Where the pen is after the last verb — `None` for an empty path.
    #[must_use]
    pub fn current_point(&self) -> Option<Offset> {
        let mut start = None;
        let mut pen = None;
        for verb in self.verbs() {
            match *verb {
                PathVerb::MoveTo(p) => {
                    start = Some(p);
                    pen = Some(p);
                }
                PathVerb::LineTo(p) | PathVerb::CubicTo(_, _, p) => pen = Some(p),
                PathVerb::Close => pen = start,
            }
        }
        pen
    }

    /// Every subpath as a polyline, curves flattened to within `tolerance`.
    #[must_use]
    pub fn flatten(&self, tolerance: f32) -> Vec<Contour> {
        let mut out = Vec::new();
        let mut current = Contour::default();
        let mut pen = Offset::ZERO;
        let flush = |current: &mut Contour, out: &mut Vec<Contour>| {
            if current.points.len() > 1 {
                out.push(std::mem::take(current));
            } else {
                current.points.clear();
                current.closed = false;
            }
        };
        for verb in self.verbs() {
            match *verb {
                PathVerb::MoveTo(p) => {
                    flush(&mut current, &mut out);
                    current.points.push(p);
                    pen = p;
                }
                PathVerb::LineTo(p) => {
                    if current.points.is_empty() {
                        current.points.push(pen);
                    }
                    current.points.push(p);
                    pen = p;
                }
                PathVerb::CubicTo(c1, c2, p) => {
                    if current.points.is_empty() {
                        current.points.push(pen);
                    }
                    let n = cubic_steps([pen, c1, c2, p], tolerance);
                    for i in 1..=n {
                        #[allow(clippy::cast_precision_loss)]
                        let t = i as f32 / n as f32;
                        current.points.push(cubic_point(pen, c1, c2, p, t));
                    }
                    pen = p;
                }
                PathVerb::Close => {
                    current.closed = true;
                    if let Some(&first) = current.points.first() {
                        pen = first;
                    }
                    flush(&mut current, &mut out);
                    current.points.push(pen);
                }
            }
        }
        flush(&mut current, &mut out);
        out
    }

    /// Prepare the path for length and point-at-distance queries.
    #[must_use]
    pub fn measure(&self) -> PathMeasure {
        let mut segments = Vec::new();
        let mut total = 0.0;
        let mut contour = 0usize;
        let mut pen = Offset::ZERO;
        let mut start = Offset::ZERO;
        let mut started = false;
        let mut push = |kind: SegKind, from: Offset, to: Offset, length: f32, contour: usize| {
            segments.push(Segment {
                from,
                to,
                kind,
                start: total,
                length,
                contour,
            });
            total += length;
        };
        for verb in self.verbs() {
            match *verb {
                PathVerb::MoveTo(p) => {
                    if started {
                        contour += 1;
                    }
                    started = true;
                    pen = p;
                    start = p;
                }
                PathVerb::LineTo(p) => {
                    push(SegKind::Line, pen, p, sub(p, pen).distance(), contour);
                    pen = p;
                }
                PathVerb::CubicTo(c1, c2, p) => {
                    let mut lut = Box::new([0.0f32; LUT + 1]);
                    let mut prev = pen;
                    for (i, slot) in lut.iter_mut().enumerate().skip(1) {
                        #[allow(clippy::cast_precision_loss)]
                        let t = i as f32 / LUT as f32;
                        // Sub-sample each table step so the table is honest
                        // about the length between its own entries.
                        let mut acc = 0.0;
                        for k in 1..=4 {
                            #[allow(clippy::cast_precision_loss)]
                            let tt = t - (1.0 / LUT as f32) * (1.0 - k as f32 / 4.0);
                            let q = cubic_point(pen, c1, c2, p, tt);
                            acc += sub(q, prev).distance();
                            prev = q;
                        }
                        *slot = acc;
                    }
                    for i in 1..=LUT {
                        lut[i] += lut[i - 1];
                    }
                    let length = lut[LUT];
                    push(SegKind::Cubic { c1, c2, lut }, pen, p, length, contour);
                    pen = p;
                }
                PathVerb::Close => {
                    let length = sub(start, pen).distance();
                    if length > 0.0 {
                        push(SegKind::Line, pen, start, length, contour);
                    }
                    pen = start;
                }
            }
        }
        PathMeasure { segments, total }
    }

    /// Total arc length. Build a [`PathMeasure`] instead when querying more
    /// than once.
    #[must_use]
    pub fn length(&self) -> f32 {
        self.measure().length()
    }

    /// The part of the path between `start` and `end`, each a fraction of the
    /// total length — After Effects' *Trim Paths*, and the draw-on effect
    /// (animate `end` from 0 to 1). `start > end` is an empty path.
    #[must_use]
    pub fn trim(&self, start: f32, end: f32) -> Self {
        let m = self.measure();
        let total = m.length();
        m.segment(start.clamp(0.0, 1.0) * total, end.clamp(0.0, 1.0) * total)
    }

    /// Trim Paths with *offset*: the window `[start, end]` shifted by
    /// `offset` (a fraction; 1.0 is one lap) and wrapped around the path's
    /// end, so a fixed-length dash can chase itself around a closed shape.
    #[must_use]
    pub fn trim_offset(&self, start: f32, end: f32, offset: f32) -> Self {
        let m = self.measure();
        let total = m.length();
        let span = (end - start).clamp(0.0, 1.0);
        if span >= 1.0 {
            return m.segment(0.0, total);
        }
        let a = (start + offset).rem_euclid(1.0);
        let b = a + span;
        if b <= 1.0 {
            m.segment(a * total, b * total)
        } else {
            let mut out = m.segment(a * total, total);
            out.extend(&m.segment(0.0, (b - 1.0) * total));
            out
        }
    }

    /// The dashes of `pattern` (on, off, on, …) starting `offset` into the
    /// pattern, as a path of open subpaths — the geometry `stroke-dasharray`
    /// strokes. An empty or all-zero pattern returns the path unchanged.
    #[must_use]
    pub fn dashed(&self, pattern: &[f32], offset: f32) -> Self {
        let period: f32 = pattern.iter().map(|v| v.max(0.0)).sum();
        if pattern.is_empty() || period <= 0.0 {
            return self.clone();
        }
        let m = self.measure();
        let mut out = Self::new();
        // Walk from the (negative) start of the pattern that covers 0.
        let mut d = -offset.rem_euclid(period);
        let mut index = 0usize;
        while d < m.length() {
            let len = pattern[index % pattern.len()].max(0.0);
            if index % 2 == 0 {
                out.extend(&m.segment(d.max(0.0), (d + len).min(m.length())));
            }
            d += len;
            index += 1;
            if len == 0.0 && index > pattern.len() * 4 && d <= 0.0 {
                break;
            }
        }
        out
    }

    /// The winding number of the path around `point` (curves flattened to
    /// [`DEFAULT_TOLERANCE`]); every subpath counts as closed, as in a fill.
    #[must_use]
    pub fn winding(&self, point: Offset) -> i32 {
        winding_of(&self.flatten(DEFAULT_TOLERANCE), point)
    }

    /// Whether a fill of this path under `rule` covers `point` — the
    /// geometric answer to Konva's hit canvas: exact for any shape, no
    /// offscreen buffer, no colour lookup.
    #[must_use]
    pub fn contains(&self, point: Offset, rule: FillRule) -> bool {
        if !self.bounds().inflate(DEFAULT_TOLERANCE).contains(point) {
            return false;
        }
        rule.covers(self.winding(point))
    }

    /// Whether a stroke of this path `width` wide passes over `point` —
    /// hit-testing a line, which a fill test cannot do.
    #[must_use]
    pub fn stroke_contains(&self, point: Offset, width: f32) -> bool {
        let r = width / 2.0;
        self.flatten(DEFAULT_TOLERANCE).iter().any(|c| {
            let n = c.points.len();
            let edges = if c.closed { n } else { n - 1 };
            (0..edges).any(|i| {
                let a = c.points[i];
                let b = c.points[(i + 1) % n];
                distance_to_segment(point, a, b) <= r
            })
        })
    }

    /// SVG path data (`d` attribute) for this path — the inverse of
    /// [`parse_path_data`](crate::parse_path_data). Absolute commands,
    /// shortest round-tripping numbers.
    #[must_use]
    pub fn to_svg_data(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        let num = |v: f32| {
            let r = (v * 1000.0).round() / 1000.0;
            if r == 0.0 {
                "0".to_owned()
            } else {
                format!("{r}")
            }
        };
        for verb in self.verbs() {
            if !out.is_empty() {
                out.push(' ');
            }
            match *verb {
                PathVerb::MoveTo(p) => {
                    let _ = write!(out, "M{} {}", num(p.dx), num(p.dy));
                }
                PathVerb::LineTo(p) => {
                    let _ = write!(out, "L{} {}", num(p.dx), num(p.dy));
                }
                PathVerb::CubicTo(a, b, p) => {
                    let _ = write!(
                        out,
                        "C{} {} {} {} {} {}",
                        num(a.dx),
                        num(a.dy),
                        num(b.dx),
                        num(b.dy),
                        num(p.dx),
                        num(p.dy)
                    );
                }
                PathVerb::Close => out.push('Z'),
            }
        }
        out
    }

    /// Combine this path's region with `other`'s. Both are read under the
    /// nonzero rule; the result is closed polygons (curves flattened to
    /// [`DEFAULT_TOLERANCE`]) that fill correctly under nonzero.
    ///
    /// ```
    /// use vieww_foundation::{Offset, Path, PathOp, FillRule, Rect};
    ///
    /// let a = Path::rect(Rect::new(0.0, 0.0, 10.0, 10.0));
    /// let b = Path::rect(Rect::new(5.0, 5.0, 15.0, 15.0));
    /// let both = a.boolean(&b, PathOp::Intersect);
    /// assert!(both.contains(Offset::new(7.0, 7.0), FillRule::NonZero));
    /// assert!(!both.contains(Offset::new(2.0, 2.0), FillRule::NonZero));
    /// assert_eq!(both.bounds(), Rect::new(5.0, 5.0, 10.0, 10.0));
    /// ```
    #[must_use]
    pub fn boolean(&self, other: &Self, op: PathOp) -> Self {
        self.boolean_with(other, op, FillRule::NonZero, FillRule::NonZero)
    }

    /// [`boolean`](Self::boolean) with each operand read under its own rule.
    #[must_use]
    pub fn boolean_with(&self, other: &Self, op: PathOp, rule_a: FillRule, rule_b: FillRule) -> Self {
        let a = closed_rings(self.flatten(DEFAULT_TOLERANCE));
        let b = closed_rings(other.flatten(DEFAULT_TOLERANCE));
        boolean_rings(&a, &b, op, rule_a, rule_b)
    }
}

fn distance_to_segment(p: Offset, a: Offset, b: Offset) -> f32 {
    let ab = sub(b, a);
    let len2 = ab.distance_squared();
    let t = if len2 > 0.0 {
        ((p.dx - a.dx) * ab.dx + (p.dy - a.dy) * ab.dy) / len2
    } else {
        0.0
    };
    sub(p, lerp(a, b, t.clamp(0.0, 1.0))).distance()
}

fn closed_rings(contours: Vec<Contour>) -> Vec<Vec<Offset>> {
    contours
        .into_iter()
        .map(|c| {
            let mut pts = c.points;
            if pts.len() > 1 && pts.first() == pts.last() {
                pts.pop();
            }
            pts
        })
        .filter(|p| p.len() >= 3)
        .collect()
}

fn winding_of(contours: &[Contour], p: Offset) -> i32 {
    let mut w = 0;
    for c in contours {
        let n = c.points.len();
        if n < 2 {
            continue;
        }
        for i in 0..n {
            w += edge_winding(c.points[i], c.points[(i + 1) % n], p);
        }
    }
    w
}

fn rings_winding(rings: &[Vec<Offset>], p: Offset) -> i32 {
    let mut w = 0;
    for ring in rings {
        let n = ring.len();
        for i in 0..n {
            w += edge_winding(ring[i], ring[(i + 1) % n], p);
        }
    }
    w
}

/// The crossing-number contribution of one edge (Sunday's winding test).
fn edge_winding(a: Offset, b: Offset, p: Offset) -> i32 {
    if a.dy <= p.dy {
        if b.dy > p.dy && cross(sub(b, a), sub(p, a)) > 0.0 {
            return 1;
        }
    } else if b.dy <= p.dy && cross(sub(b, a), sub(p, a)) < 0.0 {
        return -1;
    }
    0
}

/// A point key for linking edges: coordinates snapped to 1/1024 of a unit.
fn key(p: Offset) -> (i64, i64) {
    #[allow(clippy::cast_possible_truncation)]
    ((f64::from(p.dx) * 1024.0).round() as i64, (f64::from(p.dy) * 1024.0).round() as i64)
}

fn boolean_rings(
    a: &[Vec<Offset>],
    b: &[Vec<Offset>],
    op: PathOp,
    rule_a: FillRule,
    rule_b: FillRule,
) -> Path {
    // Every edge of both operands.
    let mut edges: Vec<(Offset, Offset)> = Vec::new();
    for ring in a.iter().chain(b.iter()) {
        let n = ring.len();
        for i in 0..n {
            let (p, q) = (ring[i], ring[(i + 1) % n]);
            if key(p) != key(q) {
                edges.push((p, q));
            }
        }
    }

    // Split points per edge: (t, point), shared between the two edges of an
    // intersection so both sides agree on the exact coordinate.
    let mut splits: Vec<Vec<(f32, Offset)>> = vec![Vec::new(); edges.len()];
    #[allow(clippy::needless_range_loop)]
    for i in 0..edges.len() {
        let (p, q) = edges[i];
        let bb_i = Rect::new(p.dx.min(q.dx), p.dy.min(q.dy), p.dx.max(q.dx), p.dy.max(q.dy)).inflate(1e-3);
        for j in (i + 1)..edges.len() {
            let (r, s) = edges[j];
            let bb_j = Rect::new(r.dx.min(s.dx), r.dy.min(s.dy), r.dx.max(s.dx), r.dy.max(s.dy));
            if !bb_i.overlaps(bb_j.inflate(1e-3)) {
                continue;
            }
            intersect_edges((p, q), (r, s), &mut splits, i, j);
        }
    }

    // The sub-edges, classified.
    let scale = {
        let mut bounds = Rect::ZERO;
        let mut first = true;
        for (p, q) in &edges {
            let r = Rect::new(p.dx.min(q.dx), p.dy.min(q.dy), p.dx.max(q.dx), p.dy.max(q.dy));
            bounds = if first { r } else { bounds.union(r) };
            first = false;
        }
        bounds.width().max(bounds.height()).max(1.0)
    };
    let probe = (scale * 1e-5).max(1e-4);

    let mut kept: Vec<(Offset, Offset)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (index, &(p, q)) in edges.iter().enumerate() {
        let mut cuts = splits[index].clone();
        cuts.push((0.0, p));
        cuts.push((1.0, q));
        cuts.sort_by(|x, y| x.0.total_cmp(&y.0));
        for w in cuts.windows(2) {
            let (s, e) = (w[0].1, w[1].1);
            if key(s) == key(e) {
                continue;
            }
            let d = sub(e, s);
            let len = d.distance();
            let n = Offset::new(-d.dy / len, d.dx / len);
            let mid = lerp(s, e, 0.5);
            let eps = probe.min(len * 0.25);
            let left = add(mid, mul(n, eps));
            let right = sub(mid, mul(n, eps));
            let covered = |pt: Offset| {
                op.apply(
                    rule_a.covers(rings_winding(a, pt)),
                    rule_b.covers(rings_winding(b, pt)),
                )
            };
            let (l, r) = (covered(left), covered(right));
            if l == r {
                continue;
            }
            let (from, to) = if l { (s, e) } else { (e, s) };
            if seen.insert((key(from), key(to))) {
                kept.push((from, to));
            }
        }
    }

    link_loops(&kept)
}

fn intersect_edges(
    (p, q): (Offset, Offset),
    (r, s): (Offset, Offset),
    splits: &mut [Vec<(f32, Offset)>],
    i: usize,
    j: usize,
) {
    let d1 = sub(q, p);
    let d2 = sub(s, r);
    let denom = cross(d1, d2);
    let len1 = d1.distance();
    let len2 = d2.distance();
    if denom.abs() <= 1e-9 * len1 * len2 {
        // Parallel. If collinear, each edge is split where the other's
        // endpoints fall inside it.
        if cross(d1, sub(r, p)).abs() > 1e-4 * len1 {
            return;
        }
        let project = |pt: Offset, origin: Offset, dir: Offset, len2: f32| {
            ((pt.dx - origin.dx) * dir.dx + (pt.dy - origin.dy) * dir.dy) / len2
        };
        for pt in [r, s] {
            let t = project(pt, p, d1, len1 * len1);
            if t > 1e-6 && t < 1.0 - 1e-6 {
                splits[i].push((t, pt));
            }
        }
        for pt in [p, q] {
            let t = project(pt, r, d2, len2 * len2);
            if t > 1e-6 && t < 1.0 - 1e-6 {
                splits[j].push((t, pt));
            }
        }
        return;
    }
    let rp = sub(r, p);
    let t = cross(rp, d2) / denom;
    let u = cross(rp, d1) / denom;
    let eps = 1e-6;
    if t < -eps || t > 1.0 + eps || u < -eps || u > 1.0 + eps {
        return;
    }
    let t = t.clamp(0.0, 1.0);
    let u = u.clamp(0.0, 1.0);
    // Snap to an existing endpoint when the crossing is at one, so the
    // linker sees exactly one vertex there.
    let mut point = lerp(p, q, t);
    for end in [p, q, r, s] {
        if sub(end, point).distance() < 1e-4 {
            point = end;
        }
    }
    if t > eps && t < 1.0 - eps {
        splits[i].push((t, point));
    }
    if u > eps && u < 1.0 - eps {
        splits[j].push((u, point));
    }
}

fn link_loops(edges: &[(Offset, Offset)]) -> Path {
    use std::collections::HashMap;
    let mut outgoing: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, (from, _)) in edges.iter().enumerate() {
        outgoing.entry(key(*from)).or_default().push(i);
    }
    let mut used = vec![false; edges.len()];
    let mut out = Path::new();
    for start in 0..edges.len() {
        if used[start] {
            continue;
        }
        let mut ring = vec![edges[start].0];
        used[start] = true;
        let origin = key(edges[start].0);
        let mut current = start;
        loop {
            let end = edges[current].1;
            if key(end) == origin {
                break;
            }
            ring.push(end);
            let next = outgoing
                .get(&key(end))
                .and_then(|list| list.iter().copied().find(|&e| !used[e]));
            match next {
                Some(n) => {
                    used[n] = true;
                    current = n;
                }
                None => break,
            }
        }
        let ring = simplify(ring);
        if ring.len() >= 3 {
            out.move_to(ring[0]);
            for &p in &ring[1..] {
                out.line_to(p);
            }
            out.close();
        }
    }
    out
}

/// Drop vertices on a straight line between their neighbours.
fn simplify(ring: Vec<Offset>) -> Vec<Offset> {
    if ring.len() < 4 {
        return ring;
    }
    let n = ring.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let prev = ring[(i + n - 1) % n];
        let here = ring[i];
        let next = ring[(i + 1) % n];
        let a = sub(here, prev);
        let b = sub(next, here);
        let straight = cross(a, b).abs() <= 1e-6 * a.distance() * b.distance()
            && (a.dx * b.dx + a.dy * b.dy) > 0.0;
        if !straight {
            out.push(here);
        }
    }
    out
}

/// Signed area of a path's flattened contours (positive = clockwise in
/// Y-down space). Useful for asserting a boolean's result.
#[must_use]
pub fn signed_area(path: &Path) -> f32 {
    path.flatten(DEFAULT_TOLERANCE)
        .iter()
        .map(|c| {
            let n = c.points.len();
            (0..n)
                .map(|i| cross(c.points[i], c.points[(i + 1) % n]))
                .sum::<f32>()
                / 2.0
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn square(x: f32, y: f32, s: f32) -> Path {
        Path::rect(Rect::new(x, y, x + s, y + s))
    }

    fn circle(c: Offset, r: f32) -> Path {
        Path::arc(c, r, 0.0, 2.0 * PI)
    }

    #[test]
    fn a_squares_length_is_its_perimeter() {
        assert!((square(0.0, 0.0, 10.0).length() - 40.0).abs() < 1e-4);
    }

    #[test]
    fn a_circles_length_is_two_pi_r() {
        let len = circle(Offset::new(0.0, 0.0), 50.0).length();
        assert!((len - 2.0 * PI * 50.0).abs() < 0.2, "{len}");
    }

    #[test]
    fn point_at_walks_the_perimeter_with_the_heading() {
        let m = square(0.0, 0.0, 10.0).measure();
        let p = m.point_at(15.0).unwrap();
        assert!((p.position.dx - 10.0).abs() < 1e-4 && (p.position.dy - 5.0).abs() < 1e-4);
        assert!((p.angle - PI / 2.0).abs() < 1e-4, "heading down the right side");
        let end = m.point_at(1e9).unwrap();
        assert!(sub(end.position, Offset::ZERO).distance() < 1e-4, "clamped to the end");
    }

    #[test]
    fn trim_keeps_curves_cubic_and_the_right_length() {
        let c = circle(Offset::new(0.0, 0.0), 40.0);
        let half = c.trim(0.0, 0.5);
        assert!(half.verbs().iter().any(|v| matches!(v, PathVerb::CubicTo(..))));
        assert!((half.length() - PI * 40.0).abs() < 0.3, "{}", half.length());
        assert!(c.trim(0.6, 0.4).is_empty());
        assert!((c.trim(0.0, 1.0).length() - c.length()).abs() < 0.2);
    }

    #[test]
    fn trim_offset_wraps_around_the_end() {
        let s = square(0.0, 0.0, 10.0);
        let t = s.trim_offset(0.0, 0.25, 0.875);
        assert!((t.length() - 10.0).abs() < 1e-3, "{}", t.length());
        let starts = t.verbs().iter().filter(|v| matches!(v, PathVerb::MoveTo(_))).count();
        assert_eq!(starts, 2, "the wrapped window is two pieces");
    }

    #[test]
    fn trimming_across_subpaths_does_not_bridge_them() {
        let mut p = Path::new();
        p.move_to(Offset::new(0.0, 0.0)).line_to(Offset::new(10.0, 0.0));
        p.move_to(Offset::new(0.0, 20.0)).line_to(Offset::new(10.0, 20.0));
        let t = p.trim(0.25, 0.75);
        assert!((t.length() - 10.0).abs() < 1e-4);
        assert_eq!(t.verbs().iter().filter(|v| matches!(v, PathVerb::MoveTo(_))).count(), 2);
    }

    #[test]
    fn dashes_cover_the_on_fraction() {
        let line = {
            let mut p = Path::new();
            p.move_to(Offset::ZERO).line_to(Offset::new(100.0, 0.0));
            p
        };
        let d = line.dashed(&[10.0, 10.0], 0.0);
        assert!((d.length() - 50.0).abs() < 1e-3);
        let shifted = line.dashed(&[10.0, 10.0], 5.0);
        assert!((shifted.length() - 50.0).abs() < 1e-3, "{}", shifted.length());
    }

    #[test]
    fn contains_is_exact_for_curves_and_honours_fill_rules() {
        let c = circle(Offset::new(0.0, 0.0), 10.0);
        assert!(c.contains(Offset::new(9.9, 0.0), FillRule::NonZero));
        assert!(!c.contains(Offset::new(7.2, 7.2), FillRule::NonZero), "outside the curve, inside the box");
        // A ring drawn as two same-direction circles: nonzero fills the hole,
        // even-odd does not.
        let mut ring = circle(Offset::ZERO, 10.0);
        ring.extend(&circle(Offset::ZERO, 5.0));
        assert!(ring.contains(Offset::new(1.0, 0.0), FillRule::NonZero));
        assert!(!ring.contains(Offset::new(1.0, 0.0), FillRule::EvenOdd));
    }

    #[test]
    fn stroke_contains_hits_lines() {
        let mut p = Path::new();
        p.move_to(Offset::ZERO).line_to(Offset::new(100.0, 0.0));
        assert!(p.stroke_contains(Offset::new(50.0, 1.5), 4.0));
        assert!(!p.stroke_contains(Offset::new(50.0, 3.0), 4.0));
    }

    #[test]
    fn svg_data_round_trips_through_the_parser() {
        let mut p = Path::new();
        p.move_to(Offset::new(1.5, 2.0))
            .line_to(Offset::new(10.0, 2.0))
            .cubic_to(Offset::new(12.0, 4.0), Offset::new(12.0, 8.0), Offset::new(10.0, 10.0))
            .close();
        let d = p.to_svg_data();
        assert_eq!(d, "M1.5 2 L10 2 C12 4 12 8 10 10 Z");
        let back = crate::parse_path_data(&d).unwrap();
        assert_eq!(back.to_svg_data(), d);
    }

    #[test]
    fn quad_to_is_the_equivalent_cubic() {
        let mut p = Path::new();
        p.move_to(Offset::ZERO).quad_to(Offset::new(10.0, 10.0), Offset::new(20.0, 0.0));
        let mid = p.measure().point_at_fraction(0.5).unwrap().position;
        assert!((mid.dx - 10.0).abs() < 0.05 && (mid.dy - 5.0).abs() < 0.05, "{mid:?}");
    }

    fn area(p: &Path) -> f32 {
        signed_area(p).abs()
    }

    #[test]
    fn booleans_of_overlapping_squares_have_the_right_areas() {
        let a = square(0.0, 0.0, 10.0);
        let b = square(5.0, 5.0, 10.0);
        assert!((area(&a.boolean(&b, PathOp::Union)) - 175.0).abs() < 0.01);
        assert!((area(&a.boolean(&b, PathOp::Intersect)) - 25.0).abs() < 0.01);
        assert!((area(&a.boolean(&b, PathOp::Difference)) - 75.0).abs() < 0.01);
        let x = a.boolean(&b, PathOp::Xor);
        // Xor is two L-shapes: union minus intersection.
        let cover = |pt: Offset| x.contains(pt, FillRule::NonZero);
        assert!(cover(Offset::new(2.0, 2.0)) && cover(Offset::new(12.0, 12.0)));
        assert!(!cover(Offset::new(7.0, 7.0)));
    }

    #[test]
    fn a_difference_can_make_a_hole_that_nonzero_respects() {
        let outer = square(0.0, 0.0, 30.0);
        let inner = square(10.0, 10.0, 10.0);
        let framed = outer.boolean(&inner, PathOp::Difference);
        assert!(framed.contains(Offset::new(5.0, 5.0), FillRule::NonZero));
        assert!(!framed.contains(Offset::new(15.0, 15.0), FillRule::NonZero));
        assert!((signed_area(&framed).abs() - 800.0).abs() < 0.01);
    }

    #[test]
    fn shared_edges_and_disjoint_shapes_are_handled() {
        let a = square(0.0, 0.0, 10.0);
        let b = square(10.0, 0.0, 10.0);
        let u = a.boolean(&b, PathOp::Union);
        assert!((area(&u) - 200.0).abs() < 0.01);
        assert!(u.contains(Offset::new(10.0, 5.0), FillRule::NonZero), "the seam is filled");
        let far = square(50.0, 50.0, 5.0);
        assert!(a.boolean(&far, PathOp::Intersect).is_empty());
        assert!((area(&a.boolean(&far, PathOp::Union)) - 125.0).abs() < 0.01);
    }

    #[test]
    fn circle_booleans_approximate_the_lens_area() {
        let r = 20.0;
        let a = circle(Offset::new(0.0, 0.0), r);
        let b = circle(Offset::new(r, 0.0), r);
        let lens = area(&a.boolean(&b, PathOp::Intersect));
        // Lens of two radius-r circles at distance r: r²(2π/3 − √3/2).
        let expected = r * r * (2.0 * PI / 3.0 - 3f32.sqrt() / 2.0);
        assert!((lens - expected).abs() / expected < 0.01, "{lens} vs {expected}");
    }

    #[test]
    fn a_self_intersecting_bowtie_is_resolved_by_its_rule() {
        let mut bow = Path::new();
        bow.move_to(Offset::new(0.0, 0.0))
            .line_to(Offset::new(10.0, 10.0))
            .line_to(Offset::new(10.0, 0.0))
            .line_to(Offset::new(0.0, 10.0))
            .close();
        let clean = bow.boolean(&Path::new(), PathOp::Union);
        assert!((area(&clean) - 50.0).abs() < 0.01, "{}", area(&clean));
    }

    #[test]
    fn arc_ring_band_and_wedge_have_straight_sides() {
        use std::f32::consts::PI;
        let c = Offset::new(0.0, 0.0);
        // A half band, radius 100, width 40: in the band, not in the hole.
        let band = Path::arc_ring(c, 100.0, 40.0, 0.0, PI);
        assert!(band.contains(Offset::new(0.0, 80.0), FillRule::NonZero));
        assert!(!band.contains(Offset::new(0.0, 40.0), FillRule::NonZero));
        assert!(!band.contains(Offset::new(0.0, -80.0), FillRule::NonZero));
        // A quarter wedge from the centre covers near its bisector.
        let wedge = Path::arc_ring(c, 100.0, 100.0, 0.0, PI / 2.0);
        assert!(wedge.contains(Offset::new(20.0, 20.0), FillRule::NonZero));
        assert!(!wedge.contains(Offset::new(-20.0, 20.0), FillRule::NonZero));
        // The inner arc of the band starts on the inner circle, not at the
        // outer arc's end: the band's end is a straight edge.
        let lines = band.verbs().iter().filter(|v| matches!(v, crate::PathVerb::LineTo(_))).count();
        assert!(lines >= 1, "{:?}", band.verbs());
        let first_inner = band.verbs().iter().find_map(|v| match v {
            crate::PathVerb::LineTo(p) => Some(*p),
            _ => None,
        });
        assert!((first_inner.unwrap().distance() - 60.0).abs() < 1e-3);
    }
}
