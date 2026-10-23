//! kit — the film's own vocabulary: type, the mark, pills, cards, and
//! the vector flows (self-drawing strokes, dashes that march).
//!
//! Everything here is a pure function of the clock, the film's contract.
//! The mark is the app's own geometry (`wright::ui::logo::W`), imported
//! so the film and the product can never drift apart. Text goes through
//! the widget layer (the house rule the studio film set: a label is a
//! `Text` in a `Positioned`, never a canvas guess at a glyph's width);
//! the canvas is for everything that is not text.

use vieww_foundation::{
    Color, Dash, Offset, Path, Rect, Sketchbook, StrokeStyle, TextAlign, TextStyle,
};
use vieww_widget::prelude::*;
use vieww_widget::WidgetNode;

use crate::film::{clamp01, LINE, WAIT};

// ── colour helpers ──────────────────────────────────────────────────────────

/// `c` at opacity `a` (0..1).
pub(crate) fn alpha(c: Color, a: f32) -> Color {
    Color::rgba(c.r, c.g, c.b, (a * 255.0).clamp(0.0, 255.0) as u8)
}

// ── the mark ────────────────────────────────────────────────────────────────

/// The W, as the app draws it: the points on a 24-grid, stroke 2, round
/// caps. Drawn by the film at any size and any draw progress — the same
/// geometry `wright::ui::logo` owns, imported.
pub(crate) fn draw_mark(
    book: &mut Sketchbook,
    at: Offset,
    side: f32,
    progress: f32,
    color: Color,
    width: f32,
) {
    let pts = wright::ui::logo::W;
    let k = side / 24.0;
    let p: Vec<Offset> = pts
        .iter()
        .map(|(x, y)| Offset::new(at.dx + x * k, at.dy + y * k))
        .collect();
    // The stroke draws itself left to right — the dash arithmetic is the
    // framework's own (`Dash`), round caps round the growing tip.
    let mut path = Path::new();
    path.move_to(p[0]);
    for q in &p[1..] {
        path.line_to(*q);
    }
    let full: f32 = p
        .windows(2)
        .map(|w| (w[1].dx - w[0].dx).hypot(w[1].dy - w[0].dy))
        .sum();
    let drawn = (full * clamp01(progress)).max(0.01);
    book.stroke_styled(
        path,
        color,
        width,
        StrokeStyle::rounded().dash(Dash::new(vec![drawn, full + 1.0])),
    );
}

/// Draw the shared W mark on a canvas' horizontal centre line. Opening,
/// closing and social end cards use this helper so none can drift sideways
/// as their mark size or output resolution changes.
pub(crate) fn centered_mark(
    book: &mut Sketchbook,
    canvas_width: f32,
    top: f32,
    side: f32,
    progress: f32,
    color: Color,
    width: f32,
) {
    draw_mark(
        book,
        Offset::new((canvas_width - side) * 0.5, top),
        side,
        progress,
        color,
        width,
    );
}

// ── type ────────────────────────────────────────────────────────────────────

/// A display label at a place, one line, horizontally aligned in its
/// box — the studio film's own grammar, copied because it is right:
/// vertical centring only, the text aligns itself horizontally.
#[allow(clippy::too_many_arguments)]
pub(crate) fn dlabel(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    text: impl Into<String>,
    size: f32,
    color: Color,
    align: TextAlign,
    a: f32,
) -> WidgetNode {
    at_place(
        x,
        y,
        w,
        h,
        TextStyle::new(size).color(color).letter_spacing(0.3),
        text,
        align,
        a,
        true,
    )
}

/// A mono label at a place — numbers, hashes, KDL, tags.
#[allow(clippy::too_many_arguments)]
pub(crate) fn mlabel(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    text: impl Into<String>,
    size: f32,
    color: Color,
    align: TextAlign,
    a: f32,
) -> WidgetNode {
    at_place(
        x,
        y,
        w,
        h,
        TextStyle::new(size)
            .color(color)
            .family(vieww_foundation::FontFamily::Monospace),
        text,
        align,
        a,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn at_place(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    style: TextStyle,
    text: impl Into<String>,
    align: TextAlign,
    a: f32,
    single_line: bool,
) -> WidgetNode {
    if a <= 0.01 {
        return Stack::new().into();
    }
    let text = Text::new(text.into()).style(style).align(align);
    // Film layout is authored against fixed rectangles. A long dynamic
    // label must not become a second line that collides with the next cue.
    // Typed copy intentionally keeps its multi-line reveal affordance.
    let text = if single_line {
        text.max_lines(1)
            .overflow(vieww_text::TextOverflow::Ellipsis)
    } else {
        text
    };
    Positioned::new()
        .left(x)
        .top(y)
        .width(w)
        .height(h)
        .child(
            Opacity::new(a.min(1.0)).child(
                Container::new()
                    .width(w)
                    .height(h)
                    .alignment(vieww_foundation::Alignment::CENTER_LEFT)
                    .child(text),
            ),
        )
        .into()
}

/// A label with a per-character reveal — the typing affordance, with the
/// caret the app's composer wears.
#[allow(clippy::too_many_arguments)]
pub(crate) fn typed_label(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    text: &str,
    size: f32,
    color: Color,
    shown: f32,
    a: f32,
) -> WidgetNode {
    let n = text.chars().count().max(1);
    let take = ((shown.clamp(0.0, 1.0)) * n as f32).round() as usize;
    let body: String = text.chars().take(take).collect();
    let caret = if take < n { "▌" } else { "" };
    let s = format!("{body}{caret}");
    at_place(
        x,
        y,
        w,
        h,
        TextStyle::new(size).color(color),
        s,
        TextAlign::Left,
        a,
        false,
    )
}

// ── the flows ───────────────────────────────────────────────────────────────

/// An S-curve thread between two points — the film's connector.
pub(crate) fn thread_pts(from: Offset, to: Offset, bend: f32) -> Vec<Offset> {
    let dx = to.dx - from.dx;
    let c1 = Offset::new(from.dx + dx * 0.42, from.dy + bend);
    let c2 = Offset::new(to.dx - dx * 0.42, to.dy - bend);
    cubic_pts(from, c1, c2, to, 44)
}

/// Flatten a cubic bezier into a polyline of `n` points.
pub(crate) fn cubic_pts(p0: Offset, c1: Offset, c2: Offset, p1: Offset, n: usize) -> Vec<Offset> {
    let n = n.max(2);
    (0..=n)
        .map(|i| {
            let u = i as f32 / n as f32;
            let v = 1.0 - u;
            let a = v * v * v;
            let b = 3.0 * v * v * u;
            let c = 3.0 * v * u * u;
            let d = u * u * u;
            Offset::new(
                a * p0.dx + b * c1.dx + c * c2.dx + d * p1.dx,
                a * p0.dy + b * c1.dy + c * c2.dy + d * p1.dy,
            )
        })
        .collect()
}

fn path_through(pts: &[Offset]) -> Path {
    let mut path = Path::new();
    if let Some(first) = pts.first() {
        path.move_to(*first);
        for p in &pts[1..] {
            path.line_to(*p);
        }
    }
    path
}

fn poly_len(pts: &[Offset]) -> f32 {
    pts.windows(2)
        .map(|w| (w[1].dx - w[0].dx).hypot(w[1].dy - w[0].dy))
        .sum()
}

/// A point `s` of the way along the polyline (by arc length).
pub(crate) fn point_at(pts: &[Offset], s: f32) -> Offset {
    if pts.is_empty() {
        return Offset::new(0.0, 0.0);
    }
    if pts.len() == 1 || s <= 0.0 {
        return pts[0];
    }
    let total = poly_len(pts);
    let target = total * s.clamp(0.0, 1.0);
    let mut acc = 0.0;
    for w in pts.windows(2) {
        let seg = (w[1].dx - w[0].dx).hypot(w[1].dy - w[0].dy);
        if acc + seg >= target {
            let u = if seg < 1.0e-6 {
                0.0
            } else {
                (target - acc) / seg
            };
            return Offset::new(
                w[0].dx + (w[1].dx - w[0].dx) * u,
                w[0].dy + (w[1].dy - w[0].dy) * u,
            );
        }
        acc += seg;
    }
    *pts.last().unwrap()
}

/// Dashes that march along a thread — the engine view's own grammar,
/// quoted: 5 on / 12 period in the app, the phase riding the clock.
pub(crate) fn march_thread(
    book: &mut Sketchbook,
    pts: &[Offset],
    phase: f32,
    color: Color,
    width: f32,
    a: f32,
) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.01 || pts.len() < 2 {
        return;
    }
    // The quiet carrier.
    book.stroke(path_through(pts), alpha(color, 0.22 * a), width * 0.8);
    // The marching dashes.
    book.stroke_styled(
        path_through(pts),
        alpha(color, 0.8 * a),
        width,
        StrokeStyle::rounded().dash(Dash::new(vec![5.0, 12.0]).offset(phase * 12.0)),
    );
}

/// A comet rider: a bright head with a fading trail — data in transit.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rider(book: &mut Sketchbook, pts: &[Offset], s: f32, color: Color, r: f32, a: f32) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.01 || pts.len() < 2 {
        return;
    }
    let trail = 8;
    for k in (1..=trail).rev() {
        let back = (s - k as f32 * 0.014).max(0.0);
        let p = point_at(pts, back);
        let fade = 1.0 - k as f32 / (trail + 1) as f32;
        book.circle(p, r * fade, alpha(color, 0.30 * fade * a));
    }
    let head = point_at(pts, s.clamp(0.0, 1.0));
    book.ring(head, r * 1.5, 1.1, alpha(color, 0.55 * a));
    book.circle(head, r, alpha(Color::WHITE, 0.9 * a));
}

// ── the state marks ─────────────────────────────────────────────────────────

/// A check in a ring — the receipt's tick, one stroke; the ring draws
/// itself first, the tick lands when the ring closes.
pub(crate) fn check_ring(book: &mut Sketchbook, at: Offset, r: f32, frac: f32, color: Color) {
    if frac <= 0.01 {
        return;
    }
    let sweep = std::f32::consts::TAU * clamp01(frac);
    book.arc(at, r, 2.2, -std::f32::consts::FRAC_PI_2, sweep, color);
    if frac >= 1.0 {
        let mut tick = Path::new();
        tick.move_to(Offset::new(at.dx - r * 0.42, at.dy + r * 0.02));
        tick.line_to(Offset::new(at.dx - r * 0.10, at.dy + r * 0.36));
        tick.line_to(Offset::new(at.dx + r * 0.46, at.dy - r * 0.30));
        book.stroke_styled(tick, color, 2.6, StrokeStyle::rounded());
    }
}

/// The waiting dot — a pulse, the status bar's own.
pub(crate) fn pulse_dot(book: &mut Sketchbook, at: Offset, r: f32, clock: f32, color: Color) {
    let p = 0.5 + 0.5 * (clock * std::f32::consts::TAU).sin();
    book.circle(at, r * (0.82 + 0.18 * p), color);
    book.ring(
        at,
        r * (1.25 + 0.5 * p),
        1.0,
        alpha(color, 0.35 * (1.0 - p)),
    );
}

/// A determinate progress line — the run's own, quoted.
pub(crate) fn progress_line(book: &mut Sketchbook, r: Rect, frac: f32, color: Color) {
    book.rect(r, alpha(LINE, 0.35));
    let k = clamp01(frac);
    if k > 0.005 {
        book.rect(
            Rect::new(r.left, r.top, r.left + r.width() * k, r.bottom),
            color,
        );
    }
}

/// The film's room: near black, a hairline horizon, the product's stage.
pub(crate) fn room(book: &mut Sketchbook, w: f32, h: f32, clock: f32) {
    book.rect(Rect::new(0.0, 0.0, w, h), crate::film::ROOM);
    book.line(
        Offset::new(0.0, h - 96.0),
        Offset::new(w, h - 96.0),
        alpha(LINE, 0.5),
        1.0,
    );
    pulse_dot(book, Offset::new(w - 72.0, 60.0), 5.0, clock * 0.5, WAIT);
}
