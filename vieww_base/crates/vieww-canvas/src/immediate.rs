//! Immediate mode — Processing / p5.js / openFrameworks' drawing model
//! (§2.21 L1–L2, §2.22 L1–L2, §5.3's "Immediate Mode" column): a sketch
//! with `setup()` and `draw()`, and a pen whose state — fill, stroke,
//! weight, the matrix — is changed by calls and saved by `push()`/`pop()`.
//!
//! Every call appends to a [`Sketchbook`], so an immediate-mode frame is
//! drawn by the same rasteriser as the retained tree — the two models meet
//! at the display list, which is how Processing's `PGraphics` sits on
//! Java2D and openFrameworks' `ofGraphics` on OpenGL.
//!
//! ```
//! use vieww_canvas::immediate::{P5, Mode};
//! use vieww_foundation::Color;
//! let mut p = P5::new(200.0, 200.0, 1);
//! p.background(Color::rgb(20, 20, 30));
//! p.fill(Color::rgb(255, 120, 0));
//! p.push();
//! p.translate(100.0, 100.0);
//! p.rotate(p.radians(45.0));
//! p.rect_mode(Mode::Center);
//! p.rect(0.0, 0.0, 50.0, 50.0);
//! p.pop();
//! assert!(p.frame().len() >= 2);
//! ```

use vieww_animation::noise::Perlin;
use vieww_foundation::{Brush, Color, FillRule, Offset, Path, Rect, Sketch, Sketchbook, StrokeStyle, Transform};

/// How `rect`/`ellipse` read their arguments (`rectMode`, `ellipseMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// x, y is the top-left corner (rect default).
    #[default]
    Corner,
    /// x, y is the centre (ellipse default).
    Center,
    /// x, y, w, h are two opposite corners.
    Corners,
    /// x, y is the centre, w, h are half extents.
    Radius,
}

/// `beginShape` kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShapeKind {
    #[default]
    Polygon,
    Points,
    Lines,
    Triangles,
    TriangleStrip,
    TriangleFan,
    Quads,
}

fn ltwh(l: f32, t: f32, w: f32, h: f32) -> Rect {
    Rect::new(l, t, l + w, t + h)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Style {
    fill: Option<Color>,
    stroke: Option<Color>,
    weight: f32,
    rect_mode: Mode,
    ellipse_mode: Mode,
    matrix: Transform,
}

#[derive(Debug, Clone, PartialEq)]
enum Vert {
    Plain(Offset),
    Bezier(Offset, Offset, Offset),
    Curve(Offset),
}

/// The pen. See the [module docs](self).
#[derive(Debug)]
pub struct P5 {
    pub width: f32,
    pub height: f32,
    pub frame_count: u64,
    style: Style,
    stack: Vec<Style>,
    book: Sketchbook,
    shape: Option<(ShapeKind, Vec<Vert>, Vec<Vec<Vert>>)>,
    noise: Perlin,
    rng: u64,
}

impl P5 {
    /// A `width × height` sketch; `seed` fixes `random` and `noise`.
    #[must_use]
    pub fn new(width: f32, height: f32, seed: u64) -> Self {
        Self {
            width,
            height,
            frame_count: 0,
            style: Style {
                fill: Some(Color::WHITE),
                stroke: Some(Color::BLACK),
                weight: 1.0,
                rect_mode: Mode::Corner,
                ellipse_mode: Mode::Center,
                matrix: Transform::IDENTITY,
            },
            stack: Vec::new(),
            book: Sketchbook::new(),
            shape: None,
            noise: Perlin::from_seed(seed),
            rng: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
        }
    }

    /// The frame drawn so far.
    #[must_use]
    pub fn frame(&self) -> &[Sketch] {
        self.book.items()
    }

    /// Take the frame and start the next (`draw()` returning).
    pub fn end_frame(&mut self) -> Sketchbook {
        self.frame_count += 1;
        self.stack.clear();
        self.style.matrix = Transform::IDENTITY;
        std::mem::take(&mut self.book)
    }

    // ── state ────────────────────────────────────────────────────────────

    pub fn fill(&mut self, c: Color) {
        self.style.fill = Some(c);
    }
    pub fn no_fill(&mut self) {
        self.style.fill = None;
    }
    pub fn stroke(&mut self, c: Color) {
        self.style.stroke = Some(c);
    }
    pub fn no_stroke(&mut self) {
        self.style.stroke = None;
    }
    pub fn stroke_weight(&mut self, w: f32) {
        self.style.weight = w.max(0.0);
    }
    pub fn rect_mode(&mut self, m: Mode) {
        self.style.rect_mode = m;
    }
    pub fn ellipse_mode(&mut self, m: Mode) {
        self.style.ellipse_mode = m;
    }
    pub fn push(&mut self) {
        self.stack.push(self.style);
    }
    pub fn pop(&mut self) {
        if let Some(s) = self.stack.pop() {
            self.style = s;
        }
    }
    pub fn translate(&mut self, x: f32, y: f32) {
        self.style.matrix = Transform::translate(Offset::new(x, y)).then(self.style.matrix);
    }
    pub fn rotate(&mut self, radians: f32) {
        self.style.matrix = Transform::rotate(radians).then(self.style.matrix);
    }
    pub fn scale(&mut self, sx: f32, sy: f32) {
        self.style.matrix = Transform::scale(sx, sy).then(self.style.matrix);
    }
    pub fn reset_matrix(&mut self) {
        self.style.matrix = Transform::IDENTITY;
    }
    /// The current matrix (`getMatrix`).
    #[must_use]
    pub fn matrix(&self) -> Transform {
        self.style.matrix
    }

    // ── maths (`map`, `lerp`, `constrain`, `noise`, `random`) ─────────────

    #[must_use]
    pub fn radians(&self, deg: f32) -> f32 {
        deg.to_radians()
    }
    #[must_use]
    pub fn map(v: f32, a0: f32, a1: f32, b0: f32, b1: f32) -> f32 {
        b0 + (b1 - b0) * (v - a0) / (a1 - a0)
    }
    #[must_use]
    pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
        a + (b - a) * t
    }
    #[must_use]
    pub fn constrain(v: f32, lo: f32, hi: f32) -> f32 {
        v.clamp(lo, hi)
    }
    /// Processing's `noise()`: 0..1, smooth.
    #[must_use]
    pub fn noise(&self, x: f32, y: f32, z: f32) -> f32 {
        (self.noise.noise3(x, y, z) * 0.5 + 0.5).clamp(0.0, 1.0)
    }
    /// `random(lo, hi)`, seeded.
    pub fn random(&mut self, lo: f32, hi: f32) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        #[allow(clippy::cast_precision_loss)]
        let u = (self.rng >> 40) as f32 / (1u64 << 24) as f32;
        lo + (hi - lo) * u
    }
    /// HSB colour (`colorMode(HSB)`), all in 0..1.
    #[must_use]
    pub fn hsb(h: f32, s: f32, b: f32, a: f32) -> Color {
        let h = h.rem_euclid(1.0) * 6.0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let i = h.floor() as u32;
        let f = h - h.floor();
        let (p, q, t) = (b * (1.0 - s), b * (1.0 - s * f), b * (1.0 - s * (1.0 - f)));
        let (r, g, bl) = match i % 6 {
            0 => (b, t, p),
            1 => (q, b, p),
            2 => (p, b, t),
            3 => (p, q, b),
            4 => (t, p, b),
            _ => (b, p, q),
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let u = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Color::rgba(u(r), u(g), u(bl), u(a))
    }

    // ── drawing ──────────────────────────────────────────────────────────

    pub fn background(&mut self, c: Color) {
        self.book.rect(ltwh(0.0, 0.0, self.width, self.height), c);
    }

    /// Paint `path` (in the current matrix) with the current fill and stroke.
    pub fn draw_path(&mut self, path: &Path, closed: bool) {
        let p = path.transformed(self.style.matrix);
        if closed {
            if let Some(f) = self.style.fill {
                self.book.fill(p.clone(), f);
            }
        }
        if let Some(s) = self.style.stroke {
            if self.style.weight > 0.0 {
                // Strokes scale with the matrix, as in Processing.
                let k = {
                    let a = self.style.matrix.apply(Offset::ZERO);
                    let b = self.style.matrix.apply(Offset::new(1.0, 0.0));
                    ((b.dx - a.dx).powi(2) + (b.dy - a.dy).powi(2)).sqrt()
                };
                self.book.stroke_styled(p, Brush::Solid(s), self.style.weight * k, StrokeStyle::rounded());
            }
        }
    }

    fn resolve(mode: Mode, x: f32, y: f32, w: f32, h: f32) -> Rect {
        match mode {
            Mode::Corner => ltwh(x, y, w, h),
            Mode::Center => ltwh(x - w * 0.5, y - h * 0.5, w, h),
            Mode::Corners => ltwh(x.min(w), y.min(h), (w - x).abs(), (h - y).abs()),
            Mode::Radius => ltwh(x - w, y - h, w * 2.0, h * 2.0),
        }
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let r = Self::resolve(self.style.rect_mode, x, y, w, h);
        self.draw_path(&Path::rect(r), true);
    }

    pub fn rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32) {
        let r = Self::resolve(self.style.rect_mode, x, y, w, h);
        self.draw_path(&Path::rounded_rect(r, radius), true);
    }

    pub fn ellipse(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let r = Self::resolve(self.style.ellipse_mode, x, y, w, h);
        let c = Offset::new(r.left + r.width() * 0.5, r.top + r.height() * 0.5);
        let unit = Path::arc(Offset::ZERO, 1.0, 0.0, std::f32::consts::TAU);
        let p = unit.transformed(Transform::scale(r.width() * 0.5, r.height() * 0.5).then(Transform::translate(c)));
        self.draw_path(&p, true);
    }

    pub fn circle(&mut self, x: f32, y: f32, d: f32) {
        let m = self.style.ellipse_mode;
        self.style.ellipse_mode = Mode::Center;
        self.ellipse(x, y, d, d);
        self.style.ellipse_mode = m;
    }

    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        let mut p = Path::new();
        p.move_to(Offset::new(x1, y1));
        p.line_to(Offset::new(x2, y2));
        self.draw_path(&p, false);
    }

    /// A stroke-coloured dot of the stroke weight.
    pub fn point(&mut self, x: f32, y: f32) {
        if let Some(s) = self.style.stroke {
            let c = self.style.matrix.apply(Offset::new(x, y));
            self.book.circle(c, (self.style.weight * 0.5).max(0.5), s);
        }
    }

    pub fn triangle(&mut self, a: (f32, f32), b: (f32, f32), c: (f32, f32)) {
        self.polygon(&[a, b, c]);
    }

    pub fn quad(&mut self, a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)) {
        self.polygon(&[a, b, c, d]);
    }

    fn polygon(&mut self, pts: &[(f32, f32)]) {
        let mut p = Path::new();
        p.move_to(Offset::new(pts[0].0, pts[0].1));
        for q in &pts[1..] {
            p.line_to(Offset::new(q.0, q.1));
        }
        p.close();
        self.draw_path(&p, true);
    }

    /// `arc(x, y, w, h, start, stop)` — a pie (filled to the centre) when
    /// filled, an open arc when only stroked.
    pub fn arc(&mut self, x: f32, y: f32, w: f32, h: f32, start: f32, stop: f32) {
        let r = Self::resolve(self.style.ellipse_mode, x, y, w, h);
        let c = Offset::new(r.left + r.width() * 0.5, r.top + r.height() * 0.5);
        let k = Transform::scale(r.width() * 0.5, r.height() * 0.5).then(Transform::translate(c));
        let arc = Path::arc(Offset::ZERO, 1.0, start, stop - start).transformed(k);
        if self.style.fill.is_some() {
            let mut pie = Path::new();
            pie.move_to(c);
            pie.line_to(k.apply(Offset::new(start.cos(), start.sin())));
            pie.extend(&arc);
            pie.line_to(c);
            pie.close();
            let stroke = self.style.stroke.take();
            self.draw_path(&pie, true);
            self.style.stroke = stroke;
        }
        let fill = self.style.fill.take();
        self.draw_path(&arc, false);
        self.style.fill = fill;
    }

    pub fn begin_shape(&mut self, kind: ShapeKind) {
        self.shape = Some((kind, Vec::new(), Vec::new()));
    }
    pub fn vertex(&mut self, x: f32, y: f32) {
        if let Some((_, v, _)) = &mut self.shape {
            v.push(Vert::Plain(Offset::new(x, y)));
        }
    }
    pub fn bezier_vertex(&mut self, c1: (f32, f32), c2: (f32, f32), to: (f32, f32)) {
        if let Some((_, v, _)) = &mut self.shape {
            v.push(Vert::Bezier(Offset::new(c1.0, c1.1), Offset::new(c2.0, c2.1), Offset::new(to.0, to.1)));
        }
    }
    /// Catmull–Rom vertex (`curveVertex`): the first and last are guides.
    pub fn curve_vertex(&mut self, x: f32, y: f32) {
        if let Some((_, v, _)) = &mut self.shape {
            v.push(Vert::Curve(Offset::new(x, y)));
        }
    }
    /// Start a hole (`beginContour`) — ends at `end_contour`.
    pub fn begin_contour(&mut self) {
        if let Some((_, v, holes)) = &mut self.shape {
            holes.push(std::mem::take(v));
        }
    }
    pub fn end_contour(&mut self) {
        if let Some((_, v, holes)) = &mut self.shape {
            if let Some(outer) = holes.pop() {
                let hole = std::mem::replace(v, outer);
                holes.push(hole);
            }
        }
    }

    fn verts_to_path(verts: &[Vert], close: bool) -> Path {
        let mut p = Path::new();
        let curves: Vec<Offset> = verts.iter().filter_map(|v| if let Vert::Curve(o) = v { Some(*o) } else { None }).collect();
        if curves.len() == verts.len() && curves.len() >= 4 {
            // Catmull–Rom through points 1..n-2.
            p.move_to(curves[1]);
            for i in 1..curves.len() - 2 {
                let (p0, p1, p2, p3) = (curves[i - 1], curves[i], curves[i + 1], curves[i + 2]);
                let c1 = Offset::new(p1.dx + (p2.dx - p0.dx) / 6.0, p1.dy + (p2.dy - p0.dy) / 6.0);
                let c2 = Offset::new(p2.dx - (p3.dx - p1.dx) / 6.0, p2.dy - (p3.dy - p1.dy) / 6.0);
                p.cubic_to(c1, c2, p2);
            }
        } else {
            let mut first = true;
            for v in verts {
                match v {
                    Vert::Plain(o) | Vert::Curve(o) => {
                        if first {
                            p.move_to(*o);
                            first = false;
                        } else {
                            p.line_to(*o);
                        }
                    }
                    Vert::Bezier(a, b, c) => {
                        if first {
                            p.move_to(*a);
                            first = false;
                        }
                        p.cubic_to(*a, *b, *c);
                    }
                }
            }
        }
        if close {
            p.close();
        }
        p
    }

    /// `endShape(close)`.
    pub fn end_shape(&mut self, close: bool) {
        let Some((kind, verts, holes)) = self.shape.take() else { return };
        let pts: Vec<Offset> = verts
            .iter()
            .map(|v| match v {
                Vert::Plain(o) | Vert::Curve(o) | Vert::Bezier(_, _, o) => *o,
            })
            .collect();
        match kind {
            ShapeKind::Polygon => {
                let mut path = Self::verts_to_path(&verts, close);
                for h in &holes {
                    path.extend(&Self::verts_to_path(h, true).reversed());
                }
                self.draw_path(&path, close || self.style.fill.is_some());
            }
            ShapeKind::Points => {
                for p in pts {
                    self.point(p.dx, p.dy);
                }
            }
            ShapeKind::Lines => {
                for pair in pts.chunks_exact(2) {
                    self.line(pair[0].dx, pair[0].dy, pair[1].dx, pair[1].dy);
                }
            }
            ShapeKind::Triangles => {
                for t in pts.chunks_exact(3) {
                    self.polygon(&[(t[0].dx, t[0].dy), (t[1].dx, t[1].dy), (t[2].dx, t[2].dy)]);
                }
            }
            ShapeKind::TriangleStrip => {
                for i in 2..pts.len() {
                    let (a, b, c) = (pts[i - 2], pts[i - 1], pts[i]);
                    self.polygon(&[(a.dx, a.dy), (b.dx, b.dy), (c.dx, c.dy)]);
                }
            }
            ShapeKind::TriangleFan => {
                for i in 2..pts.len() {
                    let (a, b, c) = (pts[0], pts[i - 1], pts[i]);
                    self.polygon(&[(a.dx, a.dy), (b.dx, b.dy), (c.dx, c.dy)]);
                }
            }
            ShapeKind::Quads => {
                for q in pts.chunks_exact(4) {
                    self.polygon(&[(q[0].dx, q[0].dy), (q[1].dx, q[1].dy), (q[2].dx, q[2].dy), (q[3].dx, q[3].dy)]);
                }
            }
        }
    }
}

/// A Processing sketch: `setup` once, `draw` per frame.
pub trait SketchProgram {
    fn setup(&mut self, p: &mut P5);
    fn draw(&mut self, p: &mut P5);
}

/// Run `program` for `frames` frames, returning each frame's display list.
pub fn run(program: &mut dyn SketchProgram, width: f32, height: f32, frames: usize, seed: u64) -> Vec<Sketchbook> {
    let mut p = P5::new(width, height, seed);
    program.setup(&mut p);
    let mut setup_book = p.end_frame();
    p.frame_count = 0;
    let mut out = Vec::with_capacity(frames);
    for i in 0..frames {
        program.draw(&mut p);
        let mut b = p.end_frame();
        if i == 0 {
            // Whatever `setup` drew is under the first frame.
            let mut first = std::mem::take(&mut setup_book).into_items();
            first.extend(b.into_items());
            b = Sketchbook::new();
            for s in first {
                b.push(s);
            }
        }
        out.push(b);
    }
    out
}

/// Hit-test helper for immediate sketches: is `point` inside the polygon
/// the current matrix would draw for `pts`?
#[must_use]
pub fn hit_polygon(p: &P5, pts: &[(f32, f32)], point: Offset) -> bool {
    let mut path = Path::new();
    path.move_to(p.matrix().apply(Offset::new(pts[0].0, pts[0].1)));
    for q in &pts[1..] {
        path.line_to(p.matrix().apply(Offset::new(q.0, q.1)));
    }
    path.close();
    path.contains(point, FillRule::NonZero)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_pop_restore_style_and_matrix() {
        let mut p = P5::new(100.0, 100.0, 1);
        p.fill(Color::RED);
        p.push();
        p.translate(10.0, 20.0);
        p.fill(Color::BLUE);
        assert_eq!(p.matrix().apply(Offset::ZERO), Offset::new(10.0, 20.0));
        p.pop();
        assert_eq!(p.matrix().apply(Offset::ZERO), Offset::ZERO);
        assert_eq!(p.style.fill, Some(Color::RED));
    }

    #[test]
    fn transforms_compose_in_processing_order() {
        let mut p = P5::new(100.0, 100.0, 1);
        p.translate(50.0, 50.0);
        p.rotate(std::f32::consts::FRAC_PI_2);
        p.scale(2.0, 2.0);
        let q = p.matrix().apply(Offset::new(10.0, 0.0));
        assert!((q.dx - 50.0).abs() < 1e-4 && (q.dy - 70.0).abs() < 1e-4, "{q:?}");
        assert!(hit_polygon(&p, &[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)], Offset::new(40.0, 60.0)));
    }

    #[test]
    fn modes_resolve_like_processing() {
        assert_eq!(P5::resolve(Mode::Center, 10.0, 10.0, 4.0, 6.0), ltwh(8.0, 7.0, 4.0, 6.0));
        assert_eq!(P5::resolve(Mode::Corners, 10.0, 10.0, 4.0, 6.0), ltwh(4.0, 6.0, 6.0, 4.0));
        assert_eq!(P5::resolve(Mode::Radius, 10.0, 10.0, 4.0, 6.0), ltwh(6.0, 4.0, 8.0, 12.0));
    }

    #[test]
    fn shapes_emit_display_items_and_stroke_only_shapes_have_no_fill() {
        let mut p = P5::new(100.0, 100.0, 1);
        p.no_fill();
        p.begin_shape(ShapeKind::Polygon);
        for (x, y) in [(0.0, 0.0), (10.0, 0.0), (20.0, 10.0), (30.0, 0.0)] {
            p.curve_vertex(x, y);
        }
        p.end_shape(false);
        assert_eq!(p.frame().len(), 1);
        p.fill(Color::WHITE);
        p.begin_shape(ShapeKind::Triangles);
        for (x, y) in [(0.0, 0.0), (10.0, 0.0), (5.0, 9.0), (20.0, 0.0), (30.0, 0.0), (25.0, 9.0)] {
            p.vertex(x, y);
        }
        p.end_shape(true);
        // Two triangles × (fill + stroke).
        assert_eq!(p.frame().len(), 1 + 4);
    }

    #[test]
    fn noise_and_random_are_seeded() {
        let mut a = P5::new(1.0, 1.0, 5);
        let mut b = P5::new(1.0, 1.0, 5);
        assert_eq!(a.random(0.0, 1.0), b.random(0.0, 1.0));
        let n = a.noise(0.3, 1.2, 4.5);
        assert!((0.0..=1.0).contains(&n));
        assert_eq!(n, b.noise(0.3, 1.2, 4.5));
        assert_eq!(P5::hsb(0.0, 1.0, 1.0, 1.0), Color::rgba(255, 0, 0, 255));
        assert!((P5::map(5.0, 0.0, 10.0, 100.0, 200.0) - 150.0).abs() < 1e-5);
    }

    struct Orbit;
    impl SketchProgram for Orbit {
        fn setup(&mut self, p: &mut P5) {
            p.background(Color::BLACK);
        }
        fn draw(&mut self, p: &mut P5) {
            #[allow(clippy::cast_precision_loss)]
            let t = p.frame_count as f32 * 0.1;
            p.circle(50.0 + 20.0 * t.cos(), 50.0 + 20.0 * t.sin(), 5.0);
        }
    }

    #[test]
    fn the_setup_draw_loop_produces_one_list_per_frame() {
        let frames = run(&mut Orbit, 100.0, 100.0, 5, 1);
        assert_eq!(frames.len(), 5);
        assert!(frames[0].len() > frames[1].len());
        assert_ne!(format!("{:?}", frames[1].items()), format!("{:?}", frames[2].items()));
    }
}
