//! exp_rackfocus — *E-16: rack focus.* The blur ramp between **buffer**
//! and **preview** — the film grammar that says which layer is the subject.
//!
//! S06's shot: the same session seen two ways, stacked in the frame —
//!
//! - **The buffer, behind**: the raw world — glyph atlas cells, scan rows,
//!   mono command text, the machine's own view of the frame. Cold cyan,
//!   green ticks, low contrast.
//! - **The preview, front**: the same session rendered — the frosted card,
//!   the stat, the sparkline, the author's view.
//!
//! A rack focus runs the whole experiment: focus **pulls forward** (the
//! preview sharpens, the buffer melts), holds, then **pulls back** (the
//! buffer sharpens — the film looks at the machine — while the preview
//! softens). The focal bar at the bottom slides between the two labels;
//! its position is the actual blur parameter driving the two `Filtered`
//! layers, printed live (px blur, both layers).
//!
//! This is `vieww-effects`' blur ramp on layers — E-16's own mechanism —
//! staged as cinematography: *what the film chooses to look at.*

use vieww_foundation::{Color, FontWeight, Gradient, Offset, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, mix, smoothstep, spring_out, tint, xywh, BG_DEEP, CANVAS, FAINT,
    INK, MUTED, Rng, VIOLET, VIOLET_SOFT, CYAN, CYAN_SOFT, MINT,
};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

/// Left margin.
const X: f32 = 120.0;

// ── The focus state machine ─────────────────────────────────────────────────
//
// focal(t) in [0, 1]: 0 = focus on the BUFFER (back), 1 = focus on the
// PREVIEW (front). The ramp: pull forward, hold, pull back, hold on the
// buffer — the machine gets the last word in this experiment.

fn focal(t: f32) -> f32 {
    // Pull forward: t 0.08 → 0.34.
    let forward = ease_in_out(clamp01((t - 0.08) / 0.26));
    // Hold.
    // Pull back: t 0.58 → 0.84 — sprung at the ends, a lens, not a lerp.
    let back = ease_in_out(clamp01((t - 0.58) / 0.26));
    forward * (1.0 - back)
}

/// Max blur, px — the defocus depth of the whole rack.
const MAX_BLUR: f32 = 13.0;

/// Blur on the preview (front) at `t` — 0 when focused.
fn blur_front(t: f32) -> f32 {
    MAX_BLUR * (1.0 - focal(t))
}

/// Blur on the buffer (back) at `t`.
fn blur_back(t: f32) -> f32 {
    MAX_BLUR * focal(t)
}

// ── The buffer layer — the machine's view ───────────────────────────────────

fn buffer_layer(t: f32) -> WidgetNode {
    // The buffer board: glyph cells, scan rows, command stream.
    let board = Painting::sized(
        Size::new(900.0, 330.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            // The panel — colder, flatter than the preview's glass.
            book.rrect(
                Rect::new(0.0, 0.0, 900.0, 330.0),
                12.0,
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, alpha(Color::rgb(13, 17, 22), 0.94)),
                    (1.0, alpha(Color::rgb(10, 13, 17), 0.96)),
                ]),
            );
            book.stroke_rrect(
                Rect::new(0.5, 0.5, 899.0, 329.0),
                12.0,
                alpha(CYAN, 0.14),
                1.0,
            );

            // The glyph atlas — a 10×4 grid of cells, some holding glyph
            // ghosts (the rasterizer's alphabet in its drawer).
            let mut rng = Rng::new(0x8F);
            for gy in 0..4 {
                for gx in 0..10 {
                    let cx = 26.0 + gx as f32 * 34.0;
                    let cy = 26.0 + gy as f32 * 34.0;
                    book.stroke_rrect(
                        xywh(cx, cy, 30.0, 30.0),
                        3.0,
                        alpha(CYAN, 0.10),
                        1.0,
                    );
                    // Glyph ghosts: runes in a handful of cells.
                    let pick = rng.f01();
                    if pick > 0.62 {
                        book.rrect(
                            xywh(cx + 8.0, cy + 8.0 + rng.f01() * 6.0, 14.0, 6.0),
                            2.0,
                            alpha(CYAN_SOFT, 0.16),
                        );
                        book.rrect(
                            xywh(cx + 8.0, cy + 18.0, 10.0 + rng.f01() * 6.0, 4.0),
                            2.0,
                            alpha(CYAN, 0.10),
                        );
                    }
                }
            }

            // Scan rows — the raster's own progress, ticking.
            for row in 0..5 {
                let ry = 190.0 + row as f32 * 26.0;
                let scan = clamp01((t * 1.4 + row as f32 * 0.13).fract());
                book.line(
                    Offset::new(26.0, ry),
                    Offset::new(26.0 + 240.0, ry),
                    alpha(Color::WHITE, 0.05),
                    1.0,
                );
                book.line(
                    Offset::new(26.0, ry),
                    Offset::new(26.0 + 240.0 * scan, ry),
                    alpha(CYAN_SOFT, 0.22),
                    1.2,
                );
            }

            // The command stream — mono rows of Push/Fill/Pop, faint.
            let cmds = [
                "PushLayer { blur: 9.0, alpha: 1.0 }",
                "Fill rrect { r: 12.0 } brush grad",
                "GlyphRun 6 faces 12 px",
                "PopLayer",
                "Fill circle { r: 78.0 }",
                "Stroke line { w: 2.0 }",
                "PushLayer { blur: 13.0 }",
                "GlyphRun 1 face 44 px",
            ];
            for (i, cmd) in cmds.iter().enumerate() {
                let cy = 196.0 + i as f32 * 16.0;
                let _ = cy;
                let _ = cmd;
            }
            // (The command text is drawn by Text widgets over this board —
            // the painting keeps the rows' rules.)
            for i in 0..8 {
                let ry = 196.0 + i as f32 * 16.0;
                book.rect(
                    xywh(290.0, ry + 3.0, 320.0 + (i % 3) as f32 * 40.0, 1.0),
                    alpha(Color::WHITE, 0.04),
                );
            }

            // A coverage bar — alpha ramp across the bottom.
            book.rect(
                xywh(26.0, 300.0, 848.0, 6.0),
                alpha(Color::WHITE, 0.05),
            );
            book.rect(
                xywh(26.0, 300.0, 848.0 * 0.62, 6.0),
                Gradient::horizontal().with_dither().with_stops(&[
                    (0.0, alpha(MINT, 0.5)),
                    (1.0, alpha(CYAN, 0.3)),
                ]),
            );
        }),
    );

    // The command stream text — mono, cold.
    let cmds = [
        "PushLayer { blur: 9.0, alpha: 1.0 }",
        "Fill rrect { r: 12.0 } brush grad",
        "GlyphRun 6 faces 12 px",
        "PopLayer",
        "Fill circle { r: 78.0 }",
        "Stroke line { w: 2.0 }",
        "PushLayer { blur: 13.0 }",
        "GlyphRun 1 face 44 px",
    ];
    let stream = (0..cmds.len())
        .fold(Stack::new(), |acc, i| {
            acc.push(
                Positioned::new()
                    .left(302.0)
                    .top(192.0 + i as f32 * 16.0)
                    .width(560.0)
                    .height(15.0)
                    .child(
                        Text::new(cmds[i]).style(
                            TextStyle::new(11.0)
                                .monospace()
                                .color(alpha(CYAN_SOFT, 0.42)),
                        ),
                    ),
            )
        });

    let body = Stack::new()
        .push(Positioned::fill().child(board))
        .push(stream);

    // The label, always crisp (labels ride outside the blur).
    Stack::new()
        .push(
            Positioned::new()
                .left(660.0)
                .top(96.0)
                .width(420.0)
                .height(18.0)
                .child(
                    Text::new("THE BUFFER · THE MACHINE'S VIEW").style(
                        TextStyle::new(12.0)
                            .monospace()
                            .letter_spacing(2.2)
                            .color(alpha(FAINT, 0.9)),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(660.0)
                .top(120.0)
                .width(900.0)
                .height(330.0)
                .child(
                    Filtered::blur(blur_back(t))
                        .child(body),
                ),
        )
        .into()
}

// ── The preview layer — the author's view ───────────────────────────────────

fn preview_layer(t: f32) -> WidgetNode {
    let settle = smoothstep(clamp01((t - 0.02) / 0.18));

    // The glass card — the film's own aesthetic, front layer.
    let card = Stack::new()
        .push(
            Positioned::new()
                .left(34.0)
                .top(30.0)
                .width(320.0)
                .height(44.0)
                .child(
                    Text::new("184.2k").style(
                        TextStyle::new(40.0)
                            .weight(FontWeight::Medium)
                            .color(alpha(INK, 0.92 * settle + 0.08)),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(34.0)
                .top(82.0)
                .width(320.0)
                .height(16.0)
                .child(
                    Text::new("ELEMENTS COMPOSED · SESSION 1").style(
                        TextStyle::new(11.0)
                            .monospace()
                            .letter_spacing(2.0)
                            .color(alpha(MUTED, 0.8)),
                    ),
                ),
        );

    let sparkline = Painting::sized(
        Size::new(360.0, 120.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            let mut rng = Rng::new(0x5F9);
            let mut p = vieww_foundation::Path::new();
            for i in 0..24 {
                let px = 12.0 + i as f32 * 15.0;
                let py = 100.0 - 70.0 * (i as f32 / 23.0).powi(2) - rng.f01() * 10.0;
                if i == 0 { p.move_to(Offset::new(px, py)); } else { p.line_to(Offset::new(px, py)); }
            }
            book.stroke(p, alpha(VIOLET_SOFT, 0.8), 2.0);
            // The head dot.
            book.circle(Offset::new(12.0 + 23.0 * 15.0, 100.0 - 70.0 - 5.0), 4.0, alpha(tint(VIOLET, 0.4), 0.9));
        }),
    );

    let body = Stack::new()
        .push(Positioned::fill().child(
            Painting::sized(
                Size::new(430.0, 230.0),
                PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                    book.rrect(
                        Rect::new(0.0, 0.0, 430.0, 230.0),
                        12.0,
                        Gradient::vertical().with_dither().with_stops(&[
                            (0.0, alpha(Color::rgb(26, 24, 38), 0.55)),
                            (1.0, alpha(Color::rgb(16, 15, 24), 0.6)),
                        ]),
                    );
                    book.stroke_rrect(
                        Rect::new(0.5, 0.5, 429.0, 229.0),
                        12.0,
                        alpha(VIOLET_SOFT, 0.22),
                        1.2,
                    );
                }),
            ),
        ))
        .push(card)
        .push(
            Positioned::new()
                .left(30.0)
                .top(108.0)
                .width(360.0)
                .height(120.0)
                .child(sparkline),
        );

    Stack::new()
        .push(
            Positioned::new()
                .left(X)
                .top(300.0)
                .width(420.0)
                .height(18.0)
                .child(
                    Text::new("THE PREVIEW · THE AUTHOR'S VIEW").style(
                        TextStyle::new(12.0)
                            .monospace()
                            .letter_spacing(2.2)
                            .color(alpha(FAINT, 0.9)),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(X)
                .top(324.0)
                .width(430.0)
                .height(230.0)
                .child(
                    Filtered::blur(blur_front(t))
                        .child(body),
                ),
        )
        .into()
}

// ── The focal bar — the rack's own instrument ───────────────────────────────

const F_Y: f32 = 610.0;

fn focal_bar(t: f32) -> WidgetNode {
    let f = focal(t);

        let bar = Painting::sized(
        Size::new(1040.0, 60.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            // The rail.
            book.rrect(
                xywh(0.0, 24.0, 1040.0, 4.0),
                2.0,
                alpha(Color::WHITE, 0.07),
            );
            // The focus region fill — from buffer (left) to current f.
            let fx = 60.0 + f * 920.0;
            book.rrect(
                xywh(fx.min(60.0 + 920.0), 24.0, (f * 920.0).abs().max(4.0), 4.0),
                2.0,
                Gradient::horizontal().with_dither().with_stops(&[
                    (0.0, alpha(CYAN, 0.4)),
                    (1.0, alpha(VIOLET, 0.6)),
                ]),
            );
            // The focal indicator — a triangle-tick riding f.
            book.ring(Offset::new(fx, 26.0), 8.0, 2.0, alpha(VIOLET_SOFT, 0.9));
            book.circle(Offset::new(fx, 26.0), 3.0, tint(VIOLET, 0.45));
        }),
    );

    Stack::new()
        .push(
            Positioned::new()
                .left(X)
                .top(F_Y - 28.0)
                .width(700.0)
                .height(18.0)
                .child(
                    Text::new("FOCAL PLANE · BUFFER ↔ PREVIEW").style(
                        TextStyle::new(12.0)
                            .monospace()
                            .letter_spacing(2.2)
                            .color(alpha(FAINT, 0.9)),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(X)
                .top(F_Y)
                .width(1040.0)
                .height(60.0)
                .child(bar),
        )
        .push(
            Positioned::new()
                .left(X - 10.0)
                .top(F_Y - 6.0)
                .width(160.0)
                .height(16.0)
                .child(
                    Text::new("BUFFER").style(
                        TextStyle::new(12.0)
                            .monospace()
                            .letter_spacing(2.0)
                            .color(alpha(CYAN_SOFT, 0.65)),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(X + 950.0)
                .top(F_Y - 6.0)
                .width(160.0)
                .height(16.0)
                .child(
                    Text::new("PREVIEW").style(
                        TextStyle::new(12.0)
                            .monospace()
                            .letter_spacing(2.0)
                            .color(alpha(VIOLET_SOFT, 0.65)),
                    ),
                ),
        )
        // The live blur readout — both layers, printed from the parameters.
        .push(
            Positioned::new()
                .left(X + 380.0)
                .top(F_Y + 36.0)
                .width(400.0)
                .height(16.0)
                .child(
                    Text::new(format!(
                        "buffer blur {:.1} px · preview blur {:.1} px",
                        blur_back(t),
                        blur_front(t)
                    ))
                    .style(
                        TextStyle::new(12.0)
                            .monospace()
                            .color(alpha(MUTED, 0.9)),
                    ),
                ),
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

            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(9, 9, 13)),
                    (0.6, BG_DEEP),
                    (1.0, Color::rgb(12, 12, 17)),
                ]),
            );

            // The stars — split by temperature: cyan stars behind the buffer
            // (right), violet stars behind the preview (left). The world
            // itself knows which side it's on.
            let mut rng = Rng::new(0xFA1);
            for _ in 0..56 {
                let x = rng.f01() * w;
                let y = rng.f01() * h;
                let r = 0.4 + rng.f01() * 0.9;
                let tw = 0.5 + 0.5 * (t * 3.0 + rng.f01() * 10.0).sin();
                let side_c = if x > w * 0.5 { alpha(CYAN, 0.05 + 0.05 * tw) } else { alpha(VIOLET, 0.05 + 0.06 * tw) };
                book.circle(Offset::new(x, y), r, side_c);
            }

            // The twin glows — one per layer, answering each other.
            book.layer(1.0, 38.0, None, |inner| {
                inner.circle(
                    Offset::new(w * 0.30, h * 0.62),
                    w * 0.24,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.10)),
                        (1.0, alpha(VIOLET, 0.0)),
                    ]),
                );
            });
            book.layer(1.0, 34.0, None, |inner| {
                inner.circle(
                    Offset::new(w * 0.72, h * 0.30),
                    w * 0.22,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(CYAN, 0.07)),
                        (1.0, alpha(CYAN, 0.0)),
                    ]),
                );
            });

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
        .push(buffer_layer(t))
        .push(preview_layer(t))
        .push(focal_bar(t))
        .into()
}
