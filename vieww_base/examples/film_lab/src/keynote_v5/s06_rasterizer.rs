//! S06 · OWN RASTERIZER — the technical flex. 0:50–1:02.
//!
//! The reference script: "vieww doesn't just pass commands to native
//! platform controls. It computes and draws its own canvas natively,
//! managing bounds checks, deterministic shapes, and font shapers at zero
//! runtime cost." Three beats, one canvas each:
//!
//! 1. **4× supersampling** — a magnifier over a hairline: the pixel grid,
//!    and inside each pixel a 4×4 lattice of sample points lighting where
//!    the stroke covers (the rasterizer's own arithmetic, shown working).
//! 2. **Determinism** — the same path stamped twice; the diff bar reads
//!    0 bytes, because it is 0 bytes (byte-identical across runs — the
//!    repo's own receipt culture).
//! 3. **The font shaper** — an 'a' drawn as its own outline path,
//!    self-drawing, while the 28 blend modes orbit as a wheel of names.

use vieww_foundation::{Color, Offset, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, ease_out_cubic, mix, tint, xywh, CYAN, CYAN_SOFT, INK, MINT,
    MUTED, VIOLET, VIOLET_SOFT,
};

use super::Ctx;

/// The magnifier's lens, beat 1.
const LENS: Offset = Offset::new(700.0, 560.0);
const LENS_R: f32 = 190.0;

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let _sec = ctx.sec;

    let mut stack = Stack::new();

    // The ground — the engine room, cool.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            super::ground(book, w, h);
            // A fine blueprint grid — the engine room's graph paper.
            let mut x = 0.0;
            while x < w {
                book.line(
                    Offset::new(x, 0.0),
                    Offset::new(x, h),
                    alpha(CYAN, 0.028),
                    1.0,
                );
                x += 96.0;
            }
            let mut y = 0.0;
            while y < h {
                book.line(
                    Offset::new(0.0, y),
                    Offset::new(w, y),
                    alpha(CYAN, 0.028),
                    1.0,
                );
                y += 96.0;
            }
            super::stars(book, w, h, 0x6057, 40, t, 0.05);
            super::vignette(book, w, h, 0.5);
        }),
    )));

    // ── Beat 1 · the magnifier: a hairline at 24× zoom with its 4×4 samples.
    let b1 = clamp01(t / 0.34);
    if b1 > 0.0 {
        let zoom_in = ease_in_out(b1);
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                // The hairline, actual size, across the canvas — the thing
                // being magnified.
                let y = LENS.dy + 4.0;
                book.rect(xywh(240.0, y, 1440.0, 2.0), alpha(VIOLET_SOFT, 0.75));
                // The lens circle.
                book.circle(LENS, LENS_R + 8.0, alpha(Color::rgb(10, 12, 16), 0.96));
                book.stroke(
                    super::circle_path(LENS.dx, LENS.dy, LENS_R, 64),
                    alpha(CYAN_SOFT, 0.7),
                    2.0,
                );
                book.stroke(
                    super::circle_path(LENS.dx, LENS.dy, LENS_R + 14.0, 64),
                    alpha(CYAN, 0.18),
                    1.0,
                );
                // Inside the lens: the pixel grid at zoom — one big hairline
                // cell = one screen pixel, split into a 4×4 sample lattice.
                let cell = LENS_R / 2.6;
                let grid_n = 4;
                let gx0 = LENS.dx - cell * grid_n as f32 * 0.5;
                let gy0 = LENS.dy - cell * grid_n as f32 * 0.5;
                // The stroke's coverage inside the lens: a horizontal band
                // crossing the grid, 1.6 cells tall.
                let band_h = cell * 1.6;
                let band_y = LENS.dy - band_h * 0.5;
                for gy in 0..grid_n {
                    for gx in 0..grid_n {
                        let x = gx0 + gx as f32 * cell;
                        let y = gy0 + gy as f32 * cell;
                        // Pixel border.
                        book.stroke_rrect(xywh(x, y, cell, cell), 2.0, alpha(CYAN, 0.22), 1.0);
                        // The 4×4 sample lattice — each sample lights by the
                        // coverage of the band at that point.
                        for sy in 0..4 {
                            for sx in 0..4 {
                                let px = x + (sx as f32 + 0.5) / 4.0 * cell;
                                let py = y + (sy as f32 + 0.5) / 4.0 * cell;
                                let inside = py >= band_y && py <= band_y + band_h;
                                let edge = (py - band_y).abs() < cell * 0.25
                                    || (py - band_y - band_h).abs() < cell * 0.25;
                                let cov = if inside {
                                    if edge {
                                        0.5
                                    } else {
                                        1.0
                                    }
                                } else {
                                    0.0
                                };
                                let arrive =
                                    clamp01((zoom_in - (sx as f32 + sy as f32 * 4.0) * 0.02) / 0.3);
                                if cov > 0.0 && arrive > 0.0 {
                                    book.circle(
                                        Offset::new(px, py),
                                        3.1,
                                        alpha(VIOLET_SOFT, cov * arrive),
                                    );
                                } else {
                                    book.circle(
                                        Offset::new(px, py),
                                        1.6,
                                        alpha(MUTED, 0.25 * arrive),
                                    );
                                }
                            }
                        }
                    }
                }
                // The band itself, semi-transparent over the grid.
                book.rrect(
                    xywh(gx0 - cell * 0.4, band_y, cell * 4.8, band_h),
                    4.0,
                    alpha(VIOLET, 0.16),
                );
            }),
        )));
        // The receipts, under the lens.
        let r1a = clamp01((t - 0.10) / 0.2);
        if r1a > 0.0 {
            for (i, line) in [
                "4× supersampling — 16 samples per pixel",
                "coverage computed, never approximated",
            ]
            .iter()
            .enumerate()
            {
                stack = stack.push(
                    Positioned::new()
                        .left(0.0)
                        .top(170.0 + i as f32 * 36.0)
                        .width(1920.0)
                        .height(28.0)
                        .child(
                            Opacity::new(r1a).child(
                                Text::new(*line)
                                    .style(
                                        TextStyle::new(19.0)
                                            .monospace()
                                            .letter_spacing(2.0)
                                            .color(alpha(INK, 0.85)),
                                    )
                                    .align(TextAlign::Center),
                            ),
                        ),
                );
            }
        }
    }

    // ── Beat 2 · determinism — the same stamp, twice, and the zero diff.
    let b2 = clamp01((t - 0.36) / 0.28);
    if b2 > 0.0 {
        let p = ease_out_cubic(b2);
        let stamp = |ox: f32, delay: f32| {
            let _ = delay;
            let a = clamp01(p * 1.4);
            let build = clamp01((p - 0.1) * 2.2);
            Painting::sized(
                Size::new(340.0, 300.0),
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    // The stamp: a vieww-ish glyph — the window-mark — drawn by
                    // its own path, the same path both times. That's the point.
                    let cx = 170.0;
                    let cy = 150.0;
                    let sz = 92.0;
                    book.rrect(
                        xywh(cx - sz * 0.5, cy - sz * 0.5, sz, sz),
                        sz * 0.22,
                        alpha(Color::TRANSPARENT, 0.0),
                    );
                    let outline = crate::film_lib::clamp01(build);
                    book.stroke_rrect(
                        xywh(cx - sz * 0.5, cy - sz * 0.5, sz, sz),
                        sz * 0.22,
                        alpha(VIOLET_SOFT, 0.9 * a * outline),
                        5.0,
                    );
                    book.stroke_rrect(
                        xywh(cx - sz * 0.26, cy - sz * 0.26, sz * 0.52, sz * 0.52),
                        sz * 0.10,
                        alpha(CYAN_SOFT, 0.75 * a * outline),
                        3.6,
                    );
                    let mut beam = vieww_foundation::Path::new();
                    beam.move_to(Offset::new(cx + sz * 0.62, cy - sz * 0.62));
                    beam.line_to(Offset::new(cx - sz * 0.10, cy - sz * 0.10));
                    book.stroke(
                        beam,
                        alpha(tint(VIOLET_SOFT, 0.4), 0.95 * a * outline),
                        sz * 0.13,
                    );
                    let _ = ox;
                }),
            )
        };
        let card = |ox: f32| {
            Positioned::new()
                .left(ox)
                .top(430.0)
                .width(340.0)
                .height(300.0)
                .child(Painting::sized(
                    Size::new(340.0, 300.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        book.rrect(
                            xywh(0.0, 0.0, 340.0, 300.0),
                            16.0,
                            alpha(Color::rgb(13, 14, 20), 0.94),
                        );
                        book.stroke_rrect(
                            xywh(0.0, 0.0, 340.0, 300.0),
                            16.0,
                            alpha(Color::WHITE, 0.10),
                            1.2,
                        );
                    }),
                ))
        };
        stack = stack.push(card(500.0));
        stack = stack.push(card(1080.0));
        stack = stack.push(
            Positioned::new()
                .left(500.0)
                .top(430.0)
                .width(340.0)
                .height(300.0)
                .child(stamp(500.0, 0.0)),
        );
        stack = stack.push(
            Positioned::new()
                .left(1080.0)
                .top(430.0)
                .width(340.0)
                .height(300.0)
                .child(stamp(1080.0, 0.15)),
        );
        // Labels under each card.
        for (x, label) in [(500.0, "run a"), (1080.0, "run b")] {
            stack = stack.push(
                Positioned::new()
                    .left(x)
                    .top(740.0)
                    .width(340.0)
                    .height(28.0)
                    .child(
                        Text::new(label)
                            .style(
                                TextStyle::new(17.0)
                                    .monospace()
                                    .letter_spacing(2.0)
                                    .color(alpha(MUTED, 0.9)),
                            )
                            .align(TextAlign::Center),
                    ),
            );
        }
        // The diff receipt — a hairline between them, then the verdict.
        let diff_a = clamp01((t - 0.50) / 0.16);
        if diff_a > 0.0 {
            stack = stack.push(
                Positioned::new()
                    .left(0.0)
                    .top(790.0)
                    .width(1920.0)
                    .height(40.0)
                    .child(
                        Opacity::new(diff_a).child(
                            Text::new("diff: 0 bytes — byte-identical, run after run")
                                .style(
                                    TextStyle::new(23.0)
                                        .monospace()
                                        .letter_spacing(2.4)
                                        .color(alpha(tint(MINT, 0.15), 0.95)),
                                )
                                .align(TextAlign::Center),
                        ),
                    ),
            );
        }
    }

    // ── Beat 3 · the shaper + the blend wheel.
    let b3 = clamp01((t - 0.66) / 0.28);
    if b3 > 0.0 {
        let p = ease_out_cubic(b3);
        // The 'a' — a hand-shaped outline (the shaper's craft, one glyph
        // standing for the whole engine: bounds, joins, fills, hints).
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let cx = 620.0;
                let cy = 560.0;
                let sc = 1.7;
                // A lowercase 'a' as one closed path (single-storey, the
                // geometric spelling — clean joins, no hinting noise).
                let pts: [(f32, f32); 14] = [
                    (-26.0, 10.0),
                    (-26.0, -14.0),
                    (-18.0, -30.0),
                    (2.0, -38.0),
                    (24.0, -30.0),
                    (30.0, -18.0),
                    (30.0, 34.0),
                    (38.0, 34.0),
                    (38.0, 46.0),
                    (16.0, 46.0),
                    (10.0, 36.0),
                    (-4.0, 46.0),
                    (-22.0, 42.0),
                    (-26.0, 30.0),
                ];
                let mut path = vieww_foundation::Path::new();
                let n = pts.len();
                let drawn = (n as f32 * clamp01(p * 1.3)) as usize;
                for (i, (px, py)) in pts.iter().enumerate() {
                    if i > drawn {
                        break;
                    }
                    let pt = Offset::new(cx + px * sc, cy + py * sc);
                    if i == 0 {
                        path.move_to(pt);
                    } else {
                        path.line_to(pt);
                    }
                }
                if drawn >= n {
                    path.close();
                }
                book.fill(path.clone(), alpha(VIOLET, 0.14));
                book.stroke_styled(
                    path,
                    alpha(VIOLET_SOFT, 0.95),
                    3.2,
                    vieww_foundation::StrokeStyle::rounded().dash(vieww_foundation::Dash::new(
                        vec![900.0 * clamp01(p * 1.3), 900.0],
                    )),
                );
                // The bowl — the counter of the 'a', its own subpath.
                let bowl = super::circle_path(cx - 4.0 * sc, cy - 8.0 * sc, 15.0 * sc, 40);
                book.stroke(bowl, alpha(CYAN_SOFT, 0.65 * p), 2.4);
                // The baseline + x-height — the metrics the shaper keeps.
                book.line(
                    Offset::new(cx - 90.0, cy + 46.0 * sc),
                    Offset::new(cx + 110.0, cy + 46.0 * sc),
                    alpha(MUTED, 0.4),
                    1.0,
                );
                book.line(
                    Offset::new(cx - 90.0, cy - 30.0 * sc),
                    Offset::new(cx + 110.0, cy - 30.0 * sc),
                    alpha(MUTED, 0.25),
                    1.0,
                );

                // The blend wheel — 28 modes as arc ticks orbiting a core.
                let wx = 1300.0;
                let wy = 560.0;
                let wr = 170.0;
                book.circle(
                    Offset::new(wx, wy),
                    46.0,
                    alpha(Color::rgb(14, 15, 21), 0.95),
                );
                book.stroke(
                    super::circle_path(wx, wy, 46.0, 40),
                    alpha(VIOLET_SOFT, 0.6),
                    1.6,
                );
                let modes = 28;
                for m in 0..modes {
                    let a0 = m as f32 / modes as f32 * std::f32::consts::TAU
                        - std::f32::consts::FRAC_PI_2;
                    let lit = (m as f32) <= p * modes as f32;
                    let c = if lit {
                        mix(VIOLET, CYAN, m as f32 / modes as f32)
                    } else {
                        MUTED
                    };
                    book.line(
                        Offset::new(wx + a0.cos() * (wr - 22.0), wy + a0.sin() * (wr - 22.0)),
                        Offset::new(wx + a0.cos() * wr, wy + a0.sin() * wr),
                        alpha(c, if lit { 0.85 } else { 0.3 }),
                        if lit { 2.6 } else { 1.4 },
                    );
                }
                book.stroke(
                    super::circle_path(wx, wy, wr - 34.0, 64),
                    alpha(Color::WHITE, 0.07),
                    1.0,
                );
            }),
        )));
        // The labels.
        let l3 = clamp01((t - 0.72) / 0.16);
        if l3 > 0.0 {
            stack = stack.push(
                Positioned::new()
                    .left(430.0)
                    .top(800.0)
                    .width(400.0)
                    .height(28.0)
                    .child(
                        Opacity::new(l3).child(
                            Text::new("the font shaper — its own craft")
                                .style(
                                    TextStyle::new(17.0)
                                        .monospace()
                                        .letter_spacing(1.8)
                                        .color(alpha(MUTED, 0.9)),
                                )
                                .align(TextAlign::Center),
                        ),
                    ),
            );
            stack = stack.push(
                Positioned::new()
                    .left(1100.0)
                    .top(800.0)
                    .width(400.0)
                    .height(28.0)
                    .child(
                        Opacity::new(l3).child(
                            Text::new("28 blend modes — all native")
                                .style(
                                    TextStyle::new(17.0)
                                        .monospace()
                                        .letter_spacing(1.8)
                                        .color(alpha(MUTED, 0.9)),
                                )
                                .align(TextAlign::Center),
                        ),
                    ),
            );
        }
    }

    // The headline — the flex in one line.
    let head_a = clamp01((t - 0.06) / 0.2);
    stack = stack.push(
        Positioned::new()
            .left(0.0)
            .top(110.0)
            .width(1920.0)
            .height(50.0)
            .child(
                Opacity::new(head_a).child(
                    Text::new("it doesn't ask the platform to draw. it draws.")
                        .style(
                            TextStyle::new(34.0)
                                .letter_spacing(1.5)
                                .color(alpha(INK, 0.96)),
                        )
                        .align(TextAlign::Center),
                ),
            ),
    );

    stack = stack.push(super::caption(
        "bounds checks · deterministic shapes · font shaping — at zero runtime cost",
        1000.0,
        clamp01((t - 0.30) / 0.14),
    ));

    stack.into()
}
