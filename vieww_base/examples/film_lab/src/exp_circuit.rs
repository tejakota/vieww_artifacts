//! exp_circuit — *the session line.* One path holds the whole film.
//!
//! E-17 is the film's continuity spine: a single vector path that draws
//! itself across three acts, gathering a node at every human touch, forking
//! ×3 at the world tour, and finally extending into **the axis** — E-20's
//! reveal trigger, the line's own final node recontextualized as a dimension.
//!
//! Four beats, one composition:
//!
//! - **A · the draw** (t 0.02–0.40): the path draws itself left → right —
//!   dash-phase progressive stroke (the proven E-17 technique), riding a
//!   **`Path::stroke_outline` glow**: the outline of a wide stroke, filled
//!   with a gradient that fades to nothing at its edges — an analytic glow
//!   with no blur pass. The draw head is a live spark.
//! - **B · the nodes** (as the front passes them): every touch the ladder
//!   records blooms as an `arc_ring` pulse settling to a node dot, with a
//!   real `Icon` glyph arriving on its own spring — `check`, `add`,
//!   `chevron_right`, the framework's own icon set as props.
//! - **C · the fork ×3** (t 0.50–0.78): at the world tour the line splits —
//!   desktop, browser, phone — each branch drawing itself, staggered, each
//!   ending in its own touch node.
//! - **D · the axis** (t 0.78–1.0): the phone branch's final node extends
//!   into a rising diagonal that leaves the composition — F2's "the axis,"
//!   with its caption: *what you watched, from outside.*
//!
//! Node vocabulary from the graph (§4.1–4.3): say · rust · compose · fix ·
//! receipt · carry · damage — then the fork: desktop · browser · phone.

use vieww_foundation::{Color, Dash, Gradient, Offset, Path, Rect, Size, Sketchbook, StrokeStyle, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{icons, Icon, Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, ease_out_expo, mix, spring_out, BG_DEEP, CANVAS, CANVAS_W, INK, MUTED,
    Rng, VIOLET, VIOLET_SOFT, CYAN_SOFT,
};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

/// Baseline of the line's world.
const BASE_Y: f32 = 438.0;

/// The session's touch nodes: (x-fraction, y-offset, label, icon).
const NODES: [(f32, f32, &str); 7] = [
    (0.085, 0.0, "say"),
    (0.175, -16.0, "rust"),
    (0.265, 10.0, "compose"),
    (0.352, -8.0, "fix"),
    (0.44, 18.0, "receipt"),
    (0.53, 0.0, "carry"),
    (0.615, -14.0, "damage"),
];

/// The fork point and its three branches.
const FORK_X: f32 = 0.685;
const BRANCHES: [(&str, f32); 3] = [("browser", -64.0), ("desktop", 0.0), ("phone", 64.0)];
/// Where branches end (x-fraction).
const BRANCH_END_X: f32 = 0.845;

fn node_px(x_frac: f32, y_off: f32) -> Offset {
    Offset::new(x_frac * CANVAS_W, BASE_Y + y_off)
}

/// A cubic through the nodes, sampled to a polyline for dash measurement.
fn session_polyline() -> Vec<Offset> {
    let pts: Vec<Offset> = NODES.iter().map(|&(x, y, _)| node_px(x, y)).collect();
    let fork = node_px(FORK_X, -6.0);
    let mut all = vec![pts[0]];
    // Gentle S-curves between consecutive nodes: control points on the
    // vertical midpoint, offset horizontally for the sweep.
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let dx = b.dx - a.dx;
        let c1 = Offset::new(a.dx + dx * 0.32, a.dy + (b.dy - a.dy) * 0.05);
        let c2 = Offset::new(a.dx + dx * 0.68, a.dy + (b.dy - a.dy) * 0.95);
        for i in 1..=20 {
            let s = i as f32 / 20.0;
            all.push(cubic(a, c1, c2, b, s));
        }
    }
    // Into the fork point.
    let last = *all.last().unwrap();
    let c1 = Offset::new(last.dx + 40.0, last.dy);
    let c2 = Offset::new(fork.dx - 40.0, fork.dy);
    for i in 1..=14 {
        let s = i as f32 / 14.0;
        all.push(cubic(last, c1, c2, fork, s));
    }
    all
}

/// One branch polyline, from the fork to its end node.
fn branch_polyline(y_off: f32) -> Vec<Offset> {
    let start = node_px(FORK_X, -6.0);
    let end = node_px(BRANCH_END_X, y_off - 6.0);
    let c1 = Offset::new(start.dx + 46.0, start.dy);
    let c2 = Offset::new(end.dx - 46.0, end.dy);
    (1..=18)
        .map(|i| {
            let s = i as f32 / 18.0;
            cubic(start, c1, c2, end, s)
        })
        .collect()
}

fn cubic(a: Offset, c1: Offset, c2: Offset, b: Offset, s: f32) -> Offset {
    let u = 1.0 - s;
    Offset::new(
        u * u * u * a.dx + 3.0 * u * u * s * c1.dx + 3.0 * u * s * s * c2.dx + s * s * s * b.dx,
        u * u * u * a.dy + 3.0 * u * u * s * c1.dy + 3.0 * u * s * s * c2.dy + s * s * s * b.dy,
    )
}

/// Total polyline length.
fn poly_len(pts: &[Offset]) -> f32 {
    pts.windows(2).map(|w| (w[1].dx - w[0].dx).hypot(w[1].dy - w[0].dy)).sum()
}

/// Point at arc-length fraction `frac`.
fn point_at(pts: &[Offset], frac: f32) -> Offset {
    let total = poly_len(pts);
    let target = total * clamp01(frac);
    let mut acc = 0.0;
    for w in pts.windows(2) {
        let seg = (w[1].dx - w[0].dx).hypot(w[1].dy - w[0].dy);
        if acc + seg >= target {
            let s = if seg > 1e-6 { (target - acc) / seg } else { 0.0 };
            return Offset::new(
                w[0].dx + (w[1].dx - w[0].dx) * s,
                w[0].dy + (w[1].dy - w[0].dy) * s,
            );
        }
        acc += seg;
    }
    *pts.last().unwrap()
}

/// Build a `Path` from a polyline.
fn path_of(pts: &[Offset]) -> Path {
    let mut p = Path::new();
    if pts.is_empty() {
        return p;
    }
    p.move_to(pts[0]);
    for q in &pts[1..] {
        p.line_to(*q);
    }
    p
}

/// Progressive stroke — the E-17 mechanism: one dash whose draw length is
/// `frac` of the path, phase-advanced along it.
fn stroke_progressive(book: &mut Sketchbook, pts: &[Offset], frac: f32, color: Color, width: f32) {
    if frac <= 0.0 || pts.len() < 2 {
        return;
    }
    let total = poly_len(pts);
    let draw_len = total * clamp01(frac);
    let style = StrokeStyle::default().dash(Dash::new(vec![draw_len, total + 1.0]));
    book.stroke_styled(path_of(pts), color, width, style);
}

// ── The scene painter ───────────────────────────────────────────────────────

fn scene(book: &mut Sketchbook, t: f32) {
    let w = CANVAS_W;
    let h = CANVAS.height;

    // The ground.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::vertical().with_dither().with_stops(&[
            (0.0, Color::rgb(8, 8, 12)),
            (0.6, BG_DEEP),
            (1.0, Color::rgb(11, 10, 17)),
        ]),
    );

    // Sparse stars.
    let mut rng = Rng::new(0x5E55);
    for _ in 0..40 {
        let x = rng.f01() * w;
        let y = rng.f01() * h;
        let r = 0.4 + rng.f01() * 0.9;
        let tw = 0.5 + 0.5 * (t * 3.5 + rng.f01() * 9.0).sin();
        book.circle(Offset::new(x, y), r, alpha(Color::WHITE, 0.03 + 0.07 * tw));
    }

    // The time ruler under the baseline — session ticks, barely there.
    for i in 0..=40 {
        let x = 84.0 + i as f32 / 40.0 * (w - 168.0);
        let tall = i % 5 == 0;
        book.rect(
            Rect::new(x, BASE_Y + 58.0, 1.0, if tall { 7.0 } else { 3.0 }),
            alpha(Color::WHITE, 0.045),
        );
    }

    let draw_t = ease_out_expo(clamp01((t - 0.02) / 0.38));

    // ── A · the session line, with its stroke_outline glow ──────────────────
    let poly = session_polyline();
    let _total = poly_len(&poly);

    if draw_t > 0.0 {
        // The analytic glow: the outline of a WIDE stroke, filled with a
        // gradient that fades vertically to nothing over the shape's own
        // bounds. No blur, one fill.
        let glow_width = 11.0 * (0.6 + 0.4 * draw_t);
        let outline = path_of(&poly).stroke_outline(glow_width);
        book.fill(
            outline,
            Gradient::vertical().with_stops(&[
                (0.0, alpha(VIOLET, 0.0)),
                (0.38, alpha(VIOLET, 0.15 * draw_t)),
                (0.62, alpha(VIOLET, 0.15 * draw_t)),
                (1.0, alpha(VIOLET, 0.0)),
            ]),
        );
        // The line itself.
        stroke_progressive(book, &poly, draw_t, alpha(VIOLET_SOFT, 0.9), 2.4);

        // The draw head: a live spark at the front.
        let head = point_at(&poly, draw_t);
        book.layer(1.0, 8.0, None, |inner| {
            inner.circle(head, 14.0, Gradient::radial_fill().with_stops(&[
                (0.0, alpha(CYAN_SOFT, 0.5)),
                (1.0, alpha(VIOLET, 0.0)),
            ]));
        });
        book.circle(head, 2.6, Color::WHITE);
    }

    // ── B · the nodes bloom as the front passes ────────────────────────────
    for (i, &(x, y, _)) in NODES.iter().enumerate() {
        let node_t = clamp01((draw_t - x - 0.012) / 0.02);
        if node_t <= 0.0 {
            continue;
        }
        let spring = spring_out(node_t, 11.0, 0.55);
        let px = node_px(x, y);
        // The pulse ring, expanding and fading.
        let ring_r = 6.0 + 16.0 * spring;
        book.ring(px, ring_r, 1.6, alpha(VIOLET, (1.0 - node_t).max(0.0) * 0.55));
        // The node itself.
        book.circle(px, 4.4 + 1.2 * (spring - 1.0), mix(VIOLET_SOFT, Color::WHITE, 0.3));
        // The small label tick — labels themselves are widgets (crisper).
        book.line(
            Offset::new(px.dx, px.dy + 8.0),
            Offset::new(px.dx, px.dy + 15.0),
            alpha(Color::WHITE, 0.14 * node_t),
            1.0,
        );
        let _ = i;
    }

    // ── C · the fork ×3 ────────────────────────────────────────────────────
    let fork_t = clamp01((t - 0.50) / 0.28);
    if fork_t > 0.0 {
        for (bi, &(label, y_off)) in BRANCHES.iter().enumerate() {
            let branch_draw = ease_out_expo(clamp01((fork_t - bi as f32 * 0.13) / 0.6));
            let bpoly = branch_polyline(y_off);
            let bcolor = match label {
                "browser" => CYAN_SOFT,
                "phone" => VIOLET_SOFT,
                _ => INK,
            };
            stroke_progressive(book, &bpoly, branch_draw, alpha(bcolor, 0.85), 2.0);
            // Branch glow — lighter than the trunk's.
            if branch_draw > 0.0 {
                let outline = path_of(&bpoly).stroke_outline(7.0);
                book.fill(outline, alpha(bcolor, 0.07));
            }
            // The branch's end node: the touch that reaches that surface.
            if branch_draw > 0.96 {
                let end = node_px(BRANCH_END_X, y_off - 6.0);
                let settle = clamp01((fork_t - bi as f32 * 0.13 - 0.6) / 0.25);
                let spring = spring_out(settle, 12.0, 0.5);
                book.ring(end, 5.0 + 10.0 * (1.0 - settle), 1.4, alpha(bcolor, (1.0 - settle) * 0.5));
                book.circle(end, 4.2 + 1.0 * (spring - 1.0), bcolor);
            }
        }
    }

    // ── D · the axis — the final node becomes a dimension ─────────────────
    let axis_t = clamp01((t - 0.78) / 0.2);
    if axis_t > 0.0 {
        let rise = spring_out(axis_t, 8.0, 0.7);
        let from = node_px(BRANCH_END_X, 64.0 - 6.0);
        let to = Offset::new(w - 74.0, 96.0);
        let tip = Offset::new(
            from.dx + (to.dx - from.dx) * rise,
            from.dy + (to.dy - from.dy) * rise,
        );
        // The axis line: long, straight, brighter than the session line —
        // it is not a beat, it is a dimension.
        let mut axis = Path::new();
        axis.move_to(from);
        axis.line_to(tip);
        let len = (tip.dx - from.dx).hypot(tip.dy - from.dy);
        let style = StrokeStyle::default().dash(Dash::new(vec![len, len + 1.0]));
        book.stroke_styled(axis.clone(), alpha(INK, 0.92), 2.6, style);
        // Its glow — a soft vertical-wedge gradient fill of a wide outline.
        let outline = axis.clone().stroke_outline(16.0);
        book.fill(outline, alpha(VIOLET_SOFT, 0.10));
        // The head: bright, small, deliberate.
        if rise > 0.2 {
            book.layer(1.0, 10.0, None, |inner| {
                inner.circle(tip, 16.0, Gradient::radial_fill().with_stops(&[
                    (0.0, alpha(Color::WHITE, 0.55)),
                    (1.0, alpha(VIOLET_SOFT, 0.0)),
                ]));
            });
            book.circle(tip, 3.2, Color::WHITE);
        }
    }

    // The vignette.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::radial(Offset::new(0.5, 0.5), 0.8).with_dither().with_stops(&[
            (0.6, alpha(Color::BLACK, 0.0)),
            (1.0, alpha(Color::BLACK, 0.42)),
        ]),
    );
}

// ── Widgets over the paint: icons, labels, the caption ─────────────────────

fn node_label(text: &str, x: f32, y: f32, lit: f32) -> WidgetNode {
    Positioned::new()
        .left(x - 46.0)
        .top(y)
        .width(92.0)
        .height(16.0)
        .child(
            Text::new(text).style(
                TextStyle::new(13.0)
                    .monospace()
                    .letter_spacing(1.0)
                    .color(mix(alpha(MUTED, 0.5), alpha(INK, 0.9), lit)),
            ),
        )
        .into()
}

pub fn frame(t: f32) -> WidgetNode {
    let draw_t = ease_out_expo(clamp01((t - 0.02) / 0.38));

    // Node labels + icons: real widgets riding the painted line.
    let mut overlays = Stack::new();
    for &(x, y, label) in NODES.iter() {
        let node_t = clamp01((draw_t - x - 0.012) / 0.02);
        if node_t <= 0.0 {
            continue;
        }
        let spring = spring_out(node_t, 11.0, 0.55);
        let px = node_px(x, y);
        overlays = overlays
            .push(node_label(label, px.dx, px.dy + 20.0, node_t))
            .push(
                Positioned::new()
                    .left(px.dx - 24.0 + 12.0 * (1.0 - spring))
                    .top(px.dy - 44.0 - 10.0 * (1.0 - spring))
                    .width(48.0)
                    .height(20.0)
                    .child(
                        Icon::new(match label {
                            "say" => icons::chevron_right(),
                            "fix" => icons::add(),
                            _ => icons::check(),
                        })
                        .size(15.0)
                        .color(alpha(VIOLET_SOFT, 0.82 + 0.13 * node_t)),
                    ),
            );
    }

    // Branch labels at the fork.
    let fork_t = clamp01((t - 0.50) / 0.28);
    if fork_t > 0.2 {
        for (bi, &(label, y_off)) in BRANCHES.iter().enumerate() {
            let appear = clamp01((fork_t - 0.2 - bi as f32 * 0.13) / 0.25);
            if appear <= 0.0 {
                continue;
            }
            let end = node_px(BRANCH_END_X, y_off - 6.0);
            let color = match label {
                "browser" => CYAN_SOFT,
                "phone" => VIOLET_SOFT,
                _ => MUTED,
            };
            overlays = overlays
                .push(
                    Positioned::new()
                        .left(end.dx - 60.0)
                        .top(end.dy + 14.0)
                        .width(120.0)
                        .height(17.0)
                        .child(
                            Text::new(label).style(
                                TextStyle::new(13.0)
                                    .monospace()
                                    .letter_spacing(1.5)
                                    .color(alpha(color, 0.55 + 0.45 * appear)),
                            ),
                        ),
                )
                .push(
                    Positioned::new()
                        .left(end.dx - 60.0)
                        .top(end.dy - 34.0)
                        .width(120.0)
                        .height(18.0)
                        .child(
                            Icon::new(match label {
                                "browser" => icons::chevron_right(),
                                "phone" => icons::chevron_right(),
                                _ => icons::check(),
                            })
                            .size(12.0)
                            .color(alpha(color, 0.9 * appear)),
                        ),
                );
        }
    }

    // F2's caption — the axis's own words, blooming late.
    let cap_t = clamp01((t - 0.86) / 0.14);
    if cap_t > 0.0 {
        overlays = overlays.push(
            Positioned::new()
                .left(CANVAS_W - 74.0 - 320.0)
                .top(56.0)
                .width(320.0)
                .height(24.0)
                .child(
                    Text::new("what you watched, from outside.")
                        .style(
                            TextStyle::new(17.0)
                                .weight(vieww_foundation::FontWeight::Medium)
                                .color(alpha(INK, cap_t)),
                        )
                        .align(vieww_foundation::TextAlign::Right),
                ),
        );
    }

    let paint = Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            scene(book, t);
            let _ = size;
        }),
    );

    Stack::new()
        .push(Positioned::fill().child(paint))
        .push(overlays)
        .into()
}
