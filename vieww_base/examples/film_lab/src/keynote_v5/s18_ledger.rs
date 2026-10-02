//! S18 · GREEN LEDGER — Act IV: the guarantee. 2:51–3:02.
//!
//! *"Developers hate alpha software that breaks on basic workflows."*
//! Before the release, the harness: a wall of test runs cascading green
//! across the crates — the counts quoted from the repository's own
//! receipts (518 + 237 + 13 tests across the workspace's suites — the
//! root README's own figures) — one cell flickers red and is caught
//! green before your eyes (the harness's whole job in one blink), and
//! the stamp lands: **0 open regressions**.

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use crate::film_lib::{alpha, clamp01, ease_in_out, spring_out, tint, xywh, INK, MINT, MUTED, RED};

use super::{group_commas, Ctx};

/// The wall's grid — 24 × 10 cells, each a test run going green.
const COLS: usize = 24;
const ROWS: usize = 10;

/// The workspace's test counts — quoted from the root README's receipts
/// (518 + 237 + 13 across the suites; the wall's cells are the cascade,
/// the receipts are the ledger's own numbers).
const TESTS: [u64; 3] = [518, 237, 13];

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let total_cells = (COLS * ROWS) as f32;
    // The cascade — fills over the first half, sweeping like a build matrix.
    let cascade = ease_in_out(clamp01((t - 0.10) / 0.42));
    // The one red flicker — cell 187, caught green at ~0.56.
    let flicker_cell = 187usize;
    let flicker_at = 0.52f32;
    let caught_at = 0.60f32;
    // The stamp — 0 open regressions.
    let stamp_s = spring_out(clamp01((t - 0.74) / 0.26), 9.0, 0.55);

    let mut stack = Stack::new();

    // The ground — the ledger register: deep, calm, faintly mint.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(8, 10, 11)),
                    (0.6, Color::rgb(9, 11, 12)),
                    (1.0, Color::rgb(10, 12, 13)),
                ]),
            );
            super::stars(book, w, h, 0x6EED, 40, t, 0.04);
            super::glow(book, w * 0.5, h * 0.5, 700.0, MINT, 0.045);
            super::vignette(book, w, h, 0.45);
        }),
    )));

    // The wall — the cascade of green cells.
    let grid_x = 300.0;
    let grid_y = 230.0;
    let grid_w = 1320.0;
    let grid_h = 560.0;
    let cw = grid_w / COLS as f32;
    let chh = grid_h / ROWS as f32;
    let cells_done = (total_cells * cascade).round() as usize;
    stack = stack.push(
        Positioned::new()
            .left(grid_x)
            .top(grid_y)
            .width(grid_w)
            .height(grid_h)
            .child(Painting::sized(
                Size::new(grid_w, grid_h),
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    for idx in 0..(COLS * ROWS) {
                        let gx = idx % COLS;
                        let gy = idx / COLS;
                        let x = gx as f32 * cw;
                        let y = gy as f32 * chh;
                        // The cascade sweeps diagonally — a build matrix's wave.
                        let diag = (gx + gy) as f32 / (COLS + ROWS - 2) as f32;
                        let done = cascade > diag
                            || (cascade * (COLS + ROWS - 2) as f32).ceil() >= (gx + gy) as f32;
                        let arriving = (cascade - diag).abs() < 0.06;
                        // The flicker — one cell goes red mid-cascade, caught.
                        let is_flicker = idx == flicker_cell;
                        let red_now = is_flicker && t > flicker_at && t < caught_at;
                        let col = if red_now {
                            RED
                        } else if done {
                            if arriving {
                                tint(MINT, 0.35)
                            } else {
                                MINT
                            }
                        } else {
                            alpha(MUTED, 0.35)
                        };
                        let a = if done || red_now { 0.75 } else { 0.3 };
                        // The cell — a small rounded square.
                        book.rrect(
                            xywh(x + 2.0, y + 2.0, cw - 4.0, chh - 4.0),
                            3.0,
                            alpha(col, a),
                        );
                        // A tick in the done ones — the check itself.
                        if done && !red_now && (idx % 3) == 0 {
                            let cx = x + cw * 0.5;
                            let cy = y + chh * 0.5;
                            let mut p = vieww_foundation::Path::new();
                            p.move_to(Offset::new(cx - 3.4, cy + 0.4));
                            p.line_to(Offset::new(cx - 0.8, cy + 3.0));
                            p.line_to(Offset::new(cx + 3.8, cy - 2.8));
                            book.stroke(p, alpha(Color::rgb(6, 18, 12), 0.85), 1.5);
                        }
                    }
                    let _ = cells_done;
                }),
            )),
    );

    // The counters — the tests, counting up as the cascade lands.
    let counter_a = clamp01((t - 0.14) / 0.12);
    if counter_a > 0.0 {
        let sum: u64 = TESTS.iter().sum();
        let shown = super::count_up(sum, clamp01((t - 0.14) / 0.42));
        let parts: Vec<String> = TESTS.iter().map(|n| group_commas(*n)).collect();
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(128.0)
                .width(1920.0)
                .height(70.0)
                .child(
                    Opacity::new(counter_a).child(
                        Text::new(format!("{} tests · {} crates · all green", shown, 36))
                            .style(
                                TextStyle::new(44.0)
                                    .monospace()
                                    .letter_spacing(2.0)
                                    .color(alpha(INK, 0.96)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(196.0)
                .width(1920.0)
                .height(28.0)
                .child(
                    Opacity::new(counter_a).child(
                        Text::new(parts.join(" + "))
                            .style(
                                TextStyle::new(17.0)
                                    .monospace()
                                    .letter_spacing(2.0)
                                    .color(alpha(MUTED, 0.85)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }

    // The flicker's story — the caught bug, captioned as it happens.
    if t > flicker_at && t < caught_at + 0.4 {
        let a = if t < caught_at {
            1.0
        } else {
            1.0 - (t - caught_at) / 0.4
        };
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(812.0)
                .width(1920.0)
                .height(36.0)
                .child(
                    Opacity::new(a).child(
                        Text::new("one regression tried to land. the harness caught it.")
                            .style(
                                TextStyle::new(21.0)
                                    .monospace()
                                    .letter_spacing(2.4)
                                    .color(alpha(tint(RED, 0.1), 0.95)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }

    // The stamp — zero open regressions, springing in.
    if stamp_s > 0.0 {
        let press = 1.0 + 0.10 * (1.0 - spring_out(clamp01((t - 0.74) / 0.2), 12.0, 0.5));
        let a = clamp01(stamp_s * 2.0);
        let w = 560.0 * press;
        let h = 150.0 * press;
        let x = 1920.0 - 340.0 - w * 0.5 + (1.0 - a) * 60.0;
        let y = 880.0 - h * 0.5;
        stack = stack.push(
            Positioned::new()
                .left(x - w * 0.5)
                .top(y - h * 0.5)
                .width(w)
                .height(h)
                .child(Opacity::new(a).child(Painting::sized(
                    Size::new(w, h),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        // The stamp — a bordered seal, slightly rotated.
                        book.transformed(
                            vieww_foundation::Transform::rotate_around(
                                Offset::new(w * 0.5, h * 0.5),
                                -0.06,
                            ),
                            |g| {
                                g.rrect(
                                    xywh(0.0, 0.0, w, h),
                                    14.0,
                                    alpha(Color::rgb(10, 14, 12), 0.92),
                                );
                                g.stroke_rrect(
                                    xywh(4.0, 4.0, w - 8.0, h - 8.0),
                                    10.0,
                                    alpha(tint(MINT, 0.1), 0.9),
                                    3.0,
                                );
                                g.stroke_rrect(
                                    xywh(14.0, 14.0, w - 28.0, h - 28.0),
                                    6.0,
                                    alpha(tint(MINT, 0.1), 0.5),
                                    1.2,
                                );
                            },
                        );
                    }),
                ))),
        );
        stack = stack.push(
            Positioned::new()
                .left(x - w * 0.5)
                .top(y - 22.0)
                .width(w)
                .height(44.0)
                .child(
                    Opacity::new(a).child(
                        Text::new("0 open regressions")
                            .style(
                                TextStyle::new(38.0)
                                    .monospace()
                                    .letter_spacing(3.0)
                                    .color(alpha(tint(MINT, 0.25), 1.0)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }

    stack = stack.push(super::act_chip(
        "IV",
        "THE LEDGER",
        clamp01((sec - 0.3) / 0.5),
    ));
    stack = stack.push(super::caption(
        "the harness ran before the product shipped — receipts, not promises",
        1000.0,
        clamp01((t - 0.30) / 0.14),
    ));

    stack.into()
}
