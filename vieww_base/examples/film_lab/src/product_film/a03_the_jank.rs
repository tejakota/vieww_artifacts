//! A03 · THE JANK — the third pain: the broken frame budget. 0:40–0:54.
//!
//! The old world's promise is 60 frames a second — 16.6 ms each — and
//! the old world breaks it. A timeline of frame bars: most bars land
//! inside the budget line, then one lands at 40, another at 70, and
//! the eye *feels* it before the number does. The gap line itself
//! stutters — a real 24-in-60 hold pattern, not a slow-down — the
//! judder of a compositor missing its vsync. The screen pole flickers
//! out of rhythm with the caret: the two ends of the distance, out of
//! phase.
//!
//! This is the deepest cut of the need: not just far — *unreliable*.

use vieww_foundation::{Color, Offset, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;

use super::{
    alpha, caption, clamp01, distance_chip, gap_line, grain, ground, pole_caret, pole_screen,
    progress_rail, tint, vignette, xywh, Ctx, BREAK_RED, MUTED, TERM_GREEN, W,
};
use crate::film_lib::{ease_out_cubic, held_24_in_60, Rng};

/// The budget, in ms — the promise.
const BUDGET_MS: f32 = 16.6;

/// The frame-bar timeline's geometry.
const TL_X: f32 = 360.0;
const TL_Y: f32 = 700.0;
const TL_W: f32 = 1200.0;
const BAR_W: f32 = 18.0;
const BAR_GAP: f32 = 30.0;
const N_BARS: usize = 36;

/// One frame's cost, deterministic — mostly in budget, then the break.
fn frame_cost(i: usize, sec: f32) -> f32 {
    let mut rng = Rng::new(0x3A7C ^ (i as u64).wrapping_mul(0x1F7));
    let base = 6.0 + rng.f01() * 7.0;
    // The break arrives at scene's third — bars 18..24 spike, hard.
    if (18..24).contains(&i) {
        let spike = 38.0 + rng.f01() * 34.0;
        // The spike's leading edge ramps — jank arrives, not switches on.
        let k = ((i - 18) as f32 / 6.0).min(1.0);
        base * (1.0 - k) + spike * k
    } else if i >= 24 {
        // The recovery — the jank passes, the fear stays.
        base + rng.f01() * 4.0 + (if i == 24 { 12.0 } else { 0.0 })
    } else {
        let _ = sec;
        base
    }
}

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
            vignette(book, w, h, 0.62);
            grain(book, w, h, frame_i, 0.45);
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(room));

    // The gap line — stuttering. The dashes' crawl is quantised to the
    // 24-in-60 hold, so the line itself judders like a dropped frame.
    let judder_t = held_24_in_60(sec);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            gap_line(
                book, 430.0, 560.0, 1360.0, 0.0, judder_t, 8.0, 0.0, MUTED, 0.9,
            );
            pole_caret(book, 560.0, 430.0, sec, 1.0);
            // The screen pole — lit *out of phase*: its flicker uses a
            // different hold, so the two ends visibly disagree.
            let lit = (held_24_in_60(sec * 1.31) * 2.0).fract() < 0.6;
            pole_screen(
                book,
                1360.0,
                430.0,
                if lit { 0.5 } else { 0.1 },
                0.85,
                BREAK_RED,
            );
        }),
    )));

    // The frame-budget timeline — bars march in, one per frame index,
    // and the break lands mid-scene.
    let arrived = ((t * 1.35) * N_BARS as f32).min(N_BARS as f32) as usize;
    let tl_a = ease_out_cubic(clamp01((t - 0.05) / 0.2));
    if tl_a > 0.01 {
        // The scale: 60 ms full height.
        let full_h = 180.0;
        let scale_ms = 60.0;
        stack = stack.push(
            Positioned::new()
                .left(TL_X)
                .top(TL_Y - full_h - 40.0)
                .width(TL_W)
                .height(full_h + 80.0)
                .child(super::Opacity::new(tl_a).child(Painting::sized(
                    Size::new(TL_W, full_h + 80.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        // The budget line — the promise, drawn first.
                        let by = full_h + 20.0 - (BUDGET_MS / scale_ms) * full_h;
                        book.line(
                            Offset::new(0.0, by),
                            Offset::new(TL_W, by),
                            alpha(TERM_GREEN, 0.55),
                            1.4,
                        );
                        // The bars — arrived ones only.
                        for i in 0..arrived.min(N_BARS) {
                            let cost = frame_cost(i, sec);
                            let h = (cost / scale_ms * full_h).min(full_h);
                            let x = i as f32 * (BAR_W + BAR_GAP);
                            let over = cost > BUDGET_MS;
                            let c = if over {
                                BREAK_RED
                            } else {
                                tint(TERM_GREEN, 0.25)
                            };
                            // A dropped frame is a *hole*: the bar
                            // drops below the timeline with a gap.
                            let gap = if over { 6.0 } else { 0.0 };
                            book.rrect(
                                xywh(x, full_h + 20.0 - h, BAR_W, h.max(3.0)),
                                3.0,
                                alpha(c, if over { 0.95 } else { 0.55 }),
                            );
                            if over {
                                book.line(
                                    Offset::new(x, full_h + 22.0 + gap),
                                    Offset::new(x + BAR_W, full_h + 22.0 + gap),
                                    alpha(BREAK_RED, 0.5),
                                    2.0,
                                );
                            }
                        }
                        // The baseline.
                        book.line(
                            Offset::new(0.0, full_h + 20.0),
                            Offset::new(TL_W, full_h + 20.0),
                            alpha(Color::WHITE, 0.14),
                            1.0,
                        );
                        // The budget label, at the line's right end.
                        book.circle(Offset::new(TL_W - 6.0, by), 3.0, alpha(TERM_GREEN, 0.8));
                    }),
                ))),
        );
        // The budget's label — the promise, spelled.
        stack = stack.push(
            Positioned::new()
                .left(TL_X)
                .top(TL_Y - 6.0)
                .width(400.0)
                .height(22.0)
                .child(
                    Text::new(format!("frame budget · {:.1} ms", BUDGET_MS))
                        .style(
                            TextStyle::new(14.0)
                                .monospace()
                                .letter_spacing(1.6)
                                .color(alpha(tint(TERM_GREEN, 0.15), 0.9)),
                        )
                        .align(TextAlign::Left),
                ),
        );
        // The worst frame's readout — the promise, broken, named.
        let worst = (0..arrived.min(N_BARS))
            .map(|i| frame_cost(i, sec))
            .fold(0.0f32, f32::max);
        if arrived > 18 {
            stack = stack.push(
                Positioned::new()
                    .left(TL_X + TL_W - 420.0)
                    .top(TL_Y - 6.0)
                    .width(420.0)
                    .height(22.0)
                    .child(
                        Text::new(format!("worst frame · {:.1} ms", worst))
                            .style(
                                TextStyle::new(14.0)
                                    .monospace()
                                    .letter_spacing(1.6)
                                    .color(alpha(BREAK_RED, 0.95)),
                            )
                            .align(TextAlign::Right),
                    ),
            );
        }
    }

    // The stutter stamp — when the break lands, the frame itself
    // shivers: a double-exposure offset, for eight frames, once.
    let break_at = 18.0 / (N_BARS as f32 * 1.35);
    let since_break = (t - break_at) / 0.05;
    if (0.0..8.0).contains(&since_break) {
        let k = 1.0 - since_break / 8.0;
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                let dx = (frame_i % 2) as f32 * 6.0 - 3.0;
                book.rrect(
                    xywh(dx, 0.0, s.width, s.height),
                    0.0,
                    alpha(BREAK_RED, 0.05 * k),
                );
                // The tear — one horizontal seam of pure red, one line
                // of pixels displaced: what a dropped frame feels like.
                let ty = 470.0 + (frame_i % 4) as f32 * 8.0;
                book.rect(xywh(0.0, ty, s.width, 2.0), alpha(BREAK_RED, 0.25 * k));
            }),
        )));
    }

    // The captions.
    stack = stack.push(super::act_chip("MOVEMENT I", "THE FAR", 1.0));
    stack = stack.push(caption(
        "sixty frames a second is a promise — the stack keeps breaking it",
        1002.0,
        clamp01((t - 0.05) / 0.12),
    ));
    stack = stack.push(caption(
        "the eye feels the number before the profiler does",
        966.0,
        clamp01((t - 0.55) / 0.12),
    ));

    stack = stack.push(distance_chip(ctx.abs, clamp01(t / 0.1)));
    stack = stack.push(progress_rail(ctx.abs));

    let _ = W;
    stack.into()
}
