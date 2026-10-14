//! B04 · THE BUDGET — the engine's heartbeat. 1:52–2:08.
//!
//! The frame budget, met and *kept*: 16.6 ms per frame, the bars land
//! inside the line — the promise the old world broke, kept here. Two
//! receipts beside it: **damage** (a change repaints the one rect that
//! changed, not the window) and **signal** (a tap rebuilds the one
//! subtree that reads it, not the tree). The gap line shortens visibly
//! — the poles lean toward each other — and the distance readout falls
//! past 120 ms.
//!
//! This is the hinge of the film: the machine act ends with the
//! distance collapsed to *one frame*, and the studio act opens with a
//! place to spend it.

use vieww_foundation::{Color, Offset, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;

use super::{
    alpha, caption, clamp01, distance_chip, gap_line, glow, grain, ground, pole_caret, pole_screen,
    progress_rail, tint, vignette, xywh, Ctx, LEDGER, MUTED, TERM_GREEN, W,
};
use crate::film_lib::{ease_out_cubic, Rng};

/// The budget, in ms.
const BUDGET_MS: f32 = 16.6;

/// The budget bars' geometry — right half of the frame.
const BARS_X: f32 = 1030.0;
const BARS_Y: f32 = 560.0;
const N_BARS: usize = 24;
const BAR_W: f32 = 15.0;
const BAR_GAP: f32 = 22.0;

/// One in-budget frame cost — deterministic, comfortably inside.
fn frame_cost(i: usize) -> f32 {
    let mut rng = Rng::new(0xB4D6 ^ (i as u64).wrapping_mul(0x11));
    5.0 + rng.f01() * 6.0
}

/// The damage study's geometry — left half.
const DMG_X: f32 = 340.0;
const DMG_Y: f32 = 380.0;
const DMG_W: f32 = 500.0;
const DMG_H: f32 = 340.0;

pub(super) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let frame_i = (ctx.abs * 60.0) as u64;

    let room = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            ground(book, w, h);
            vignette(book, w, h, 0.55);
            grain(book, w, h, frame_i, 0.35);
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(room));

    // ── Left: the damage receipt ──
    let dmg_a = ease_out_cubic(clamp01((t - 0.05) / 0.2));
    if dmg_a > 0.01 {
        stack = stack.push(
            Positioned::new()
                .left(DMG_X)
                .top(DMG_Y)
                .width(DMG_W)
                .height(DMG_H)
                .child(super::Opacity::new(dmg_a).child(Painting::sized(
                    Size::new(DMG_W, DMG_H),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        // The window — a mock surface with content rows.
                        book.rrect(
                            xywh(0.0, 0.0, DMG_W, DMG_H),
                            12.0,
                            alpha(super::SURFACE, 0.95),
                        );
                        book.stroke_rrect(
                            xywh(0.0, 0.0, DMG_W, DMG_H),
                            12.0,
                            alpha(MUTED, 0.3),
                            1.4,
                        );
                        for r in 0..7 {
                            let ry = 24.0 + r as f32 * 44.0;
                            book.rrect(
                                xywh(24.0, ry, DMG_W - 48.0, 26.0),
                                5.0,
                                alpha(Color::WHITE, 0.05),
                            );
                        }
                        // The damaged rect — one row lights, the rest
                        // stay dark: only that region repaints. The
                        // rect breathes slightly (it is *live* damage).
                        let row = 3;
                        let dy = 24.0 + row as f32 * 44.0 + (sec * 2.0).sin() * 1.5;
                        let dr = xywh(20.0, dy - 4.0, DMG_W - 40.0, 34.0);
                        // The inspector's brackets — the film's own
                        // damage-rect grammar.
                        book.stroke_rrect(dr, 4.0, alpha(LEDGER, 0.85), 1.6);
                        for (x, y, sx, sy) in [
                            (dr.left, dr.top, 1.0, 1.0),
                            (dr.right, dr.top, -1.0, 1.0),
                            (dr.right, dr.bottom, -1.0, -1.0),
                            (dr.left, dr.bottom, 1.0, -1.0),
                        ] {
                            book.line(
                                Offset::new(x, y),
                                Offset::new(x + sx * 10.0, y),
                                alpha(LEDGER, 0.95),
                                3.0,
                            );
                            book.line(
                                Offset::new(x, y),
                                Offset::new(x, y + sy * 10.0),
                                alpha(LEDGER, 0.95),
                                3.0,
                            );
                        }
                        // The fill — the region being repainted.
                        book.rrect(dr, 4.0, alpha(LEDGER, 0.12));
                    }),
                ))),
        );
        // The damage label.
        stack = stack.push(
            Positioned::new()
                .left(DMG_X)
                .top(DMG_Y + DMG_H + 14.0)
                .width(DMG_W)
                .height(24.0)
                .child(
                    Text::new("damage — one rect repaints, the window does not")
                        .style(
                            TextStyle::new(14.0)
                                .monospace()
                                .letter_spacing(1.2)
                                .color(alpha(MUTED, 0.9)),
                        )
                        .align(TextAlign::Left),
                ),
        );
    }

    // ── The signal receipt — under the damage card ──
    let sig_a = ease_out_cubic(clamp01((t - 0.35) / 0.2));
    if sig_a > 0.01 {
        stack = stack.push(
            Positioned::new()
                .left(DMG_X)
                .top(DMG_Y + DMG_H + 52.0)
                .width(DMG_W)
                .height(120.0)
                .child(super::Opacity::new(sig_a).child(Painting::sized(
                    Size::new(DMG_W, 120.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        // The signal graph — a small tree where one
                        // leaf pulses and only its ancestors brighten.
                        let root = Offset::new(DMG_W * 0.5, 18.0);
                        let leaves: [Offset; 6] = [
                            Offset::new(70.0, 96.0),
                            Offset::new(150.0, 96.0),
                            Offset::new(230.0, 96.0),
                            Offset::new(DMG_W - 230.0, 96.0),
                            Offset::new(DMG_W - 150.0, 96.0),
                            Offset::new(DMG_W - 70.0, 96.0),
                        ];
                        // The wiring.
                        for (i, leaf) in leaves.iter().enumerate() {
                            let lit = i == 2 && (sec * 1.4).sin() > -0.2;
                            book.line(
                                root,
                                *leaf,
                                alpha(
                                    if lit { LEDGER } else { MUTED },
                                    if lit { 0.8 } else { 0.25 },
                                ),
                                if lit { 2.2 } else { 1.2 },
                            );
                            book.circle(
                                *leaf,
                                5.0,
                                alpha(
                                    if lit { LEDGER } else { MUTED },
                                    if lit { 0.95 } else { 0.4 },
                                ),
                            );
                        }
                        // The root — always lit when any leaf is.
                        book.circle(root, 6.0, alpha(LEDGER, 0.9));
                        glow(book, root.dx, root.dy, 20.0, LEDGER, 0.3);
                    }),
                ))),
        );
        stack = stack.push(
            Positioned::new()
                .left(DMG_X)
                .top(DMG_Y + DMG_H + 176.0)
                .width(DMG_W)
                .height(24.0)
                .child(
                    Text::new("signal — one tap rebuilds one subtree, not the tree")
                        .style(
                            TextStyle::new(14.0)
                                .monospace()
                                .letter_spacing(1.2)
                                .color(alpha(MUTED, 0.9)),
                        )
                        .align(TextAlign::Left),
                ),
        );
    }

    // ── Right: the budget, kept ──
    let bars_a = ease_out_cubic(clamp01((t - 0.15) / 0.2));
    if bars_a > 0.01 {
        let arrived = ((t - 0.15) / 0.75 * N_BARS as f32)
            .ceil()
            .clamp(0.0, N_BARS as f32) as usize;
        let full_h = 200.0;
        let scale_ms = 24.0;
        stack = stack.push(
            Positioned::new()
                .left(BARS_X)
                .top(BARS_Y - full_h - 30.0)
                .width(N_BARS as f32 * (BAR_W + BAR_GAP) + 40.0)
                .height(full_h + 60.0)
                .child(super::Opacity::new(bars_a).child(Painting::sized(
                    Size::new(N_BARS as f32 * (BAR_W + BAR_GAP) + 40.0, full_h + 60.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        // The budget line.
                        let by = full_h + 10.0 - (BUDGET_MS / scale_ms) * full_h;
                        let field_w = N_BARS as f32 * (BAR_W + BAR_GAP);
                        book.line(
                            Offset::new(0.0, by),
                            Offset::new(field_w, by),
                            alpha(TERM_GREEN, 0.6),
                            1.6,
                        );
                        // The bars — every one inside.
                        for i in 0..arrived.min(N_BARS) {
                            let cost = frame_cost(i);
                            let h = (cost / scale_ms * full_h).min(full_h);
                            let x = i as f32 * (BAR_W + BAR_GAP);
                            book.rrect(
                                xywh(x, full_h + 10.0 - h, BAR_W, h.max(3.0)),
                                3.0,
                                alpha(tint(LEDGER, 0.15), 0.7),
                            );
                        }
                        // The baseline.
                        book.line(
                            Offset::new(0.0, full_h + 10.0),
                            Offset::new(N_BARS as f32 * (BAR_W + BAR_GAP), full_h + 10.0),
                            alpha(Color::WHITE, 0.14),
                            1.0,
                        );
                    }),
                ))),
        );
        // The budget's label.
        stack = stack.push(
            Positioned::new()
                .left(BARS_X)
                .top(BARS_Y + 44.0)
                .width(500.0)
                .height(24.0)
                .child(
                    Text::new("the budget, kept — every frame inside 16.6 ms")
                        .style(
                            TextStyle::new(14.0)
                                .monospace()
                                .letter_spacing(1.2)
                                .color(alpha(tint(TERM_GREEN, 0.2), 0.95)),
                        )
                        .align(TextAlign::Left),
                ),
        );
    }

    // The poles — leaning in. The gap shortens: the screen pole slides
    // left, the caret slides right; both awake now.
    let lean = ease_out_cubic(clamp01((t - 0.2) / 0.6));
    let caret_x = 560.0 + lean * 180.0;
    let screen_x = 1360.0 - lean * 180.0;
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            let close = lean * 0.30;
            gap_line(
                book, 946.0, caret_x, screen_x, close, sec, 0.0, 0.0, LEDGER, 0.8,
            );
            pole_caret(book, caret_x, 946.0, sec, 0.95);
            pole_screen(
                book,
                screen_x,
                946.0,
                0.5 + 0.3 * (sec * 1.1).sin().abs(),
                0.95,
                LEDGER,
            );
        }),
    )));

    // The captions.
    stack = stack.push(super::act_chip("MOVEMENT II", "THE ENGINE", 1.0));
    stack = stack.push(caption(
        "damage, not repaint · signal, not tree · budget, kept",
        1002.0,
        clamp01((t - 0.05) / 0.12),
    ));
    stack = stack.push(caption(
        "the distance is now one frame — something should spend it",
        966.0,
        clamp01((t - 0.72) / 0.12),
    ));

    stack = stack.push(distance_chip(ctx.abs, clamp01(t / 0.1)));
    stack = stack.push(progress_rail(ctx.abs));

    let _ = W;
    let _ = vignette;
    stack.into()
}
