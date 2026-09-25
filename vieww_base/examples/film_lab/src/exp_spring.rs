//! exp_spring — *E-03/E-04: the wordmark drop and the underline that
//! overshoots.* S02's arrival, at lab scale.
//!
//! The film's S02 moment: the wordmark **vieww** drops out of the dark on an
//! underdamped spring — one arrival, two visible bounces, a settle — and the
//! underline beneath it sweeps left→right on a **stiffer spring (higher ω)**,
//! overshooting its own length and snapping back. Two springs, two
//! characters: the weight of the mark, the discipline of the rule beneath.
//!
//! And — because no number is typed — the receipt is **the springs
//! themselves**: the right panel draws the live position curve of both
//! springs, with the ω/ζ constants that actually drive the motion printed
//! from the code's own values, and a dot riding each curve at the current
//! `t`. The graph is the proof of motion; the mark is the motion.
//!
//! Grammar notes, from the graph:
//! - E-03 "wordmark spring drop" — `vieww-animation · Spring`
//! - E-04 "underline overshoot" — `Spring · higher ω`
//!
//! The lab uses the analytic scrub-safe shadow (`spring_out`) — the real
//! `SpringAnimation` with interruptibility is proven in exp_mesh's
//! mid-flight retarget; here the closed form carries the render.

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextStyle, TextAlign};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, mix, spring_out, tint, BG_DEEP, CANVAS, CANVAS_W, FAINT, INK, MUTED, Rng,
    VIOLET, VIOLET_SOFT,
};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 10.0;

// ── The two springs — the receipt constants, printed nowhere by hand ───────

/// E-03: the drop. Slow enough to see the bounces, damped like a real mass.
const DROP_OMEGA: f32 = 6.2;
const DROP_ZETA: f32 = 0.52;

/// E-04: the underline. Higher ω, looser ζ — the overshoot IS the effect.
const LINE_OMEGA: f32 = 13.5;
const LINE_ZETA: f32 = 0.40;

/// When the drop starts (film-t) and how long the spring runs.
const DROP_T0: f32 = 0.08;
const DROP_SPAN: f32 = 0.62;

/// When the underline fires — just after first floor contact.
const LINE_T0: f32 = 0.30;
const LINE_SPAN: f32 = 0.40;

/// The wordmark, verbatim — lowercase, the wordmark's own voice.
const MARK: &str = "vieww";

/// The S02 tagline, under the rule.
const TAG: &str = "the ui runtime that renders its own film";

/// Left margin — the typographic grid.
const X: f32 = 120.0;

/// The mark's resting baseline (top of text box).
const MARK_TOP: f32 = 168.0;

/// How far above the frame the mark starts its drop.
const DROP_HEIGHT: f32 = 340.0;

// ── Motion ──────────────────────────────────────────────────────────────────

/// The drop's spring progress s(t) in `[0, 1]` — 1 is settled on the line.
fn drop_s(t: f32) -> f32 {
    spring_out(clamp01((t - DROP_T0) / DROP_SPAN), DROP_OMEGA, DROP_ZETA)
}

/// The underline's spring progress u(t).
fn line_s(t: f32) -> f32 {
    spring_out(clamp01((t - LINE_T0) / LINE_SPAN), LINE_OMEGA, LINE_ZETA)
}

// ── The wordmark band ───────────────────────────────────────────────────────

fn wordmark(t: f32) -> WidgetNode {
    let s = drop_s(t);

    // Position: starts DROP_HEIGHT above rest, spring lands it (can overshoot
    // below the line — the bounce — because spring_out exceeds 1).
    let y = MARK_TOP - DROP_HEIGHT * (1.0 - s);

    // Squash: at first contact (s near 1 from below OR overshoot) the mark
    // compresses briefly — mass meeting floor. Squash follows |velocity| of
    // the spring: strongest right at the crossing.
    let vel = (s - 1.0).abs().min(0.22);
    let squash = 1.0 - vel * 0.5;
    let stretch_w = 1.0 + vel * 0.35;

    let mark_size = 118.0;
    let box_h = 150.0 * squash;
    let box_w = 620.0 * stretch_w;

    // Contact flash: the frame the mass first crosses the line.
    let contact = ((s - 1.0) / 0.16 + 1.0).clamp(0.0, 1.0) * (1.0 - clamp01((s - 1.0) / 0.16));

    let mut band = Stack::new();

    // The ground shadow — an ellipse that firms and darkens as the mark nears.
    let near = clamp01(1.0 - (MARK_TOP - y) / DROP_HEIGHT);
    let shadow = Painting::sized(
        Size::new(box_w, 90.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            book.circle(
                Offset::new(box_w * 0.31, 45.0),
                60.0 + 130.0 * near,
                Gradient::radial_fill().with_dither().with_stops(&[
                    (0.0, alpha(Color::BLACK, 0.30 * near + 0.06 * contact)),
                    (1.0, alpha(Color::BLACK, 0.0)),
                ]),
            );
        }),
    );
    band = band.push(
        Positioned::new()
            .left(X + (box_w - 620.0) * 0.5)
            .top(MARK_TOP + 132.0)
            .width(box_w)
            .height(90.0)
            .child(shadow),
    );

    // Impact dust: at first contact a ring expands and fades — the landing.
    if contact > 0.01 {
        let ring = Painting::sized(
            Size::new(760.0, 120.0),
            PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                let grow = 1.0 - contact;
                book.ring(
                    Offset::new(230.0, 60.0),
                    30.0 + 210.0 * grow,
                    2.2,
                    alpha(VIOLET_SOFT, contact * 0.55),
                );
                book.ring(
                    Offset::new(230.0, 60.0),
                    18.0 + 130.0 * grow,
                    1.2,
                    alpha(MUTED, contact * 0.35),
                );
            }),
        );
        band = band.push(
            Positioned::new()
                .left(X - 60.0)
                .top(MARK_TOP + 96.0)
                .width(760.0)
                .height(120.0)
                .child(ring),
        );
    }

    // The mark itself. Regular weight — the wordmark is a lower, quieter thing
    // than a title; the drop carries the drama, not the weight.
    band = band.push(
        Positioned::new()
            .left(X + (box_w - 620.0) * 0.5)
            .top(y)
            .width(box_w.max(1.0))
            .height(box_h.max(1.0))
            .child(
                Text::new(MARK).style(
                    TextStyle::new(mark_size)
                        .weight(vieww_foundation::FontWeight::Regular)
                        .letter_spacing(6.0)
                        .color(INK),
                ),
            ),
    );

    band.into()
}

// ── The underline — E-04, the overshoot at full display ────────────────────

fn underline(t: f32) -> WidgetNode {
    let u = line_s(t);
    if u <= 0.001 {
        return Stack::new().into();
    }

    // Rest width: the mark's width + a breath. The spring overshoots past it.
    let rest_w = 566.0;
    let w = (rest_w * u).max(0.0);

    let bar = Painting::sized(
        Size::new(w.max(2.0) + 2.0, 10.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            book.rrect(
                Rect::new(0.0, 0.0, w, 4.5),
                2.0,
                Gradient::horizontal().with_dither().with_stops(&[
                    (0.0, alpha(VIOLET, 0.95)),
                    (0.55, alpha(VIOLET_SOFT, 0.85)),
                    (1.0, alpha(VIOLET_SOFT, 0.12)),
                ]),
            );
        }),
    );

    Stack::new()
        .push(
            Positioned::new()
                .left(X)
                .top(MARK_TOP + 148.0)
                .width(w.max(2.0) + 2.0)
                .height(10.0)
                .child(bar),
        )
        .into()
}

// ── The tagline — arrives only after both springs settle ────────────────────

fn tagline(t: f32) -> WidgetNode {
    let a = clamp01((t - 0.62) / 0.20);
    if a <= 0.01 {
        return Stack::new().into();
    }
    let spacing = 6.0 - 4.2 * a;

    Stack::new()
        .push(
            Positioned::new()
                .left(X)
                .top(MARK_TOP + 186.0)
                .width(820.0)
                .height(26.0)
                .child(
                    Opacity::new(a).child(
                        Text::new(TAG).style(
                            TextStyle::new(15.0)
                                .monospace()
                                .letter_spacing(spacing)
                                .color(alpha(MUTED, 0.85)),
                        ),
                    ),
                ),
        )
        .into()
}

// ── The receipt panel — both springs, drawn live ────────────────────────────

/// Panel geometry — right side, the lab's instrument voice.
const P_X: f32 = 872.0;
const P_Y: f32 = 168.0;
const P_W: f32 = 330.0;
const P_H: f32 = 208.0;

fn spring_graph(t: f32) -> WidgetNode {
    let drop_s_now = drop_s(t);
    let line_s_now = line_s(t);

    let graph = Painting::sized(
        Size::new(P_W, P_H),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            let gx = 16.0;
            let gy = 38.0;
            let gw = P_W - 30.0;
            let gh = P_H - 74.0;

            // The panel card.
            book.rrect(
                Rect::new(0.0, 0.0, P_W, P_H),
                10.0,
                alpha(Color::rgb(16, 16, 21), 0.88),
            );
            book.stroke_rrect(
                Rect::new(0.0, 0.0, P_W, P_H),
                10.0,
                alpha(Color::WHITE, 0.08),
                1.0,
            );

            // Axes: y from 0 (bottom) to 1.15 (headroom for the overshoot).
            let y_of = |v: f32| gy + gh - gh * (v / 1.15).clamp(0.0, 1.0);
            let x_of = |u: f32| gx + gw * u;

            book.line(
                Offset::new(gx, y_of(0.0)),
                Offset::new(gx + gw, y_of(0.0)),
                alpha(Color::WHITE, 0.10),
                1.0,
            );
            book.line(
                Offset::new(gx, y_of(1.0)),
                Offset::new(gx + gw, y_of(1.0)),
                alpha(Color::WHITE, 0.05),
                1.0,
            );
            // The 1.0 line — where "settled" lives. The drop crosses it.

            // The curves — 64 samples each, the actual functions.
            let mut draw_curve = |omega: f32, zeta: f32, t0: f32, span: f32, color: Color| {
                let mut p = vieww_foundation::Path::new();
                let mut started = false;
                for i in 0..=64 {
                    let u = i as f32 / 64.0;
                    let v = spring_out(clamp01((u - t0) / span), omega, zeta);
                    let pt = Offset::new(x_of(u), y_of(v));
                    if !started {
                        p.move_to(pt);
                        started = true;
                    } else {
                        p.line_to(pt);
                    }
                }
                book.stroke(p, color, 1.8);
            };
            draw_curve(DROP_OMEGA, DROP_ZETA, DROP_T0, DROP_SPAN, alpha(INK, 0.85));
            draw_curve(LINE_OMEGA, LINE_ZETA, LINE_T0, LINE_SPAN, alpha(VIOLET_SOFT, 0.9));

            // The riders — dots at the current t on both curves.
            book.circle(
                Offset::new(x_of(t), y_of(drop_s_now)),
                4.0,
                alpha(tint(VIOLET, 0.4), 0.95),
            );
            if line_s_now > 0.0 {
                book.circle(
                    Offset::new(x_of(t), y_of(line_s_now)),
                    4.0,
                    VIOLET_SOFT,
                );
                book.ring(
                    Offset::new(x_of(t), y_of(line_s_now)),
                    7.5,
                    1.2,
                    alpha(VIOLET, 0.5),
                );
            }

            // The t cursor — a hairline through both riders.
            book.line(
                Offset::new(x_of(t), gy + 4.0),
                Offset::new(x_of(t), gy + gh - 2.0),
                alpha(Color::WHITE, 0.10),
                1.0,
            );
        }),
    );

    // Labels — the constants that actually drive the motion.
    let labels = Stack::new()
        .push(
            Positioned::new()
                .left(P_X + 16.0)
                .top(P_Y - 24.0)
                .width(330.0)
                .height(18.0)
                .child(
                    Text::new("E-03 / E-04 · THE SPRINGS · RECEIPT").style(
                        TextStyle::new(12.0)
                            .monospace()
                            .letter_spacing(2.4)
                            .color(alpha(FAINT, 0.9)),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(P_X + 16.0)
                .top(P_Y + P_H - 32.0)
                .width(310.0)
                .height(16.0)
                .child(
                    Text::new(format!(
                        "drop  ω {:.1} · ζ {:.2}      line  ω {:.1} · ζ {:.2}",
                        DROP_OMEGA, DROP_ZETA, LINE_OMEGA, LINE_ZETA
                    ))
                    .style(
                        TextStyle::new(12.0)
                            .monospace()
                            .color(alpha(MUTED, 0.9)),
                    ),
                ),
        );

    Stack::new()
        .push(labels)
        .push(
            Positioned::new()
                .left(P_X)
                .top(P_Y)
                .width(P_W)
                .height(P_H)
                .child(graph),
        )
        .into()
}

// ── The board ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let bg = Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground: deep, quiet — S02 is an interior beat.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(9, 9, 13)),
                    (0.6, BG_DEEP),
                    (1.0, Color::rgb(12, 11, 18)),
                ]),
            );

            // Sparse stars — calmer than S01; the wait is over.
            let mut rng = Rng::new(0x502);
            for _ in 0..54 {
                let x = rng.f01() * w;
                let y = rng.f01() * h;
                let r = 0.4 + rng.f01() * 0.9;
                let tw = 0.5 + 0.5 * (t * 3.0 + rng.f01() * 9.0).sin();
                book.circle(Offset::new(x, y), r, alpha(Color::WHITE, 0.03 + 0.07 * tw));
            }

            // The horizon glow — under the mark, wide and low.
            book.layer(1.0, 40.0, None, |inner| {
                inner.circle(
                    Offset::new(w * 0.30, h * 0.86),
                    w * 0.34,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.10)),
                        (1.0, alpha(VIOLET, 0.0)),
                    ]),
                );
            });

            // The drop line hint — before the mark falls, the floor it aims at.
            let pre = 1.0 - clamp01((t - DROP_T0) / 0.10);
            if pre > 0.0 {
                book.line(
                    Offset::new(X, MARK_TOP + 152.0),
                    Offset::new(X + 560.0 * pre, MARK_TOP + 152.0),
                    alpha(FAINT, 0.28 * pre),
                    1.0,
                );
            }

            // The vignette.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.5), 0.80).with_dither().with_stops(&[
                    (0.55, alpha(Color::BLACK, 0.0)),
                    (1.0, alpha(Color::BLACK, 0.45)),
                ]),
            );
        }),
    );

    Stack::new()
        .push(Positioned::fill().child(bg))
        .push(wordmark(t))
        .push(underline(t))
        .push(tagline(t))
        .push(spring_graph(t))
        .into()
}
