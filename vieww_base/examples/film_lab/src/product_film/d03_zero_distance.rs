//! D03 · ZERO DISTANCE — the poles meet. 4:18–4:32.
//!
//! The prologue returns, verbatim: the two points, the dashed axis,
//! the question's furniture — and then the answer. The caret slides
//! right; the screen leans left; the dashes accelerate as the distance
//! counter falls through its last orders of magnitude; and the two
//! points touch. The contact blooms — and out of the bloom, the
//! viewwstudio mark: the studio's own code-drawn logo, its two panels
//! (the editor, the preview) revealing on their own clocks, exactly as
//! the product's splash screen does it.
//!
//! The question the film asked in its first twelve seconds is answered
//! in these fourteen: *how far is a thought from a screen?* — one
//! frame.

use vieww_foundation::{Offset, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;

use super::{
    alpha, brand_mark, caption, clamp01, distance_chip, gap_line, glow, grain, ground, pole_caret,
    pole_screen, progress_rail, spring_out, vignette, xywh, Ctx, INK, MUTED, W,
};
use crate::film_lib::ease_out_cubic;

/// Where the meeting happens.
const MEET_X: f32 = 960.0;
const MEET_Y: f32 = 430.0;

pub(super) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let frame_i = (ctx.abs * 60.0) as u64;

    // The approach — the caret slides right, the screen slides left.
    let approach = ease_out_cubic(clamp01((t - 0.10) / 0.45));
    let caret_x = 560.0 + (MEET_X - 560.0) * approach;
    let screen_x = 1360.0 + (MEET_X - 1360.0) * approach;
    // The merge — the contact's bloom, and the mark's reveal.
    let merge = clamp01((t - 0.55) / 0.20);
    // The mark's panels — the studio's own reveal clock.
    let mark_a = clamp01((t - 0.62) / 0.10);
    let editor = spring_out(clamp01((t - 0.62) / 0.3), 9.0, 0.6);
    let preview = spring_out(clamp01((t - 0.72) / 0.3), 9.0, 0.6);

    let room = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            ground(book, w, h);
            vignette(book, w, h, 0.6);
            grain(book, w, h, frame_i, 0.45);
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(room));

    // The poles and the axis — the question's furniture, returning. As
    // the poles approach, the dashes speed up (the crawl multiplier
    // rises with the approach).
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            if merge < 0.99 {
                gap_line(
                    book,
                    MEET_Y,
                    560.0,
                    1360.0,
                    approach * 0.85,
                    sec,
                    0.0,
                    0.0,
                    MUTED,
                    1.0 - merge * 0.8,
                );
                pole_caret(book, caret_x, MEET_Y, sec, 1.0 - merge * 0.6);
                pole_screen(book, screen_x, MEET_Y, approach, 1.0 - merge * 0.6, MUTED);
            }
            // The contact's bloom — the moment itself.
            if merge > 0.0 {
                let bloom = spring_out(merge, 11.0, 0.5);
                glow(
                    book,
                    MEET_X,
                    MEET_Y,
                    320.0 * bloom,
                    super::ACCENT,
                    0.5 * (1.0 - merge * 0.4),
                );
                // The shockwave ring.
                book.ring(
                    Offset::new(MEET_X, MEET_Y),
                    20.0 + bloom * 180.0,
                    2.4,
                    alpha(super::ACCENT, (1.0 - merge) * 0.8),
                );
                // The sparks — twelve motes thrown outward.
                for i in 0..12 {
                    let ang = i as f32 / 12.0 * std::f32::consts::TAU;
                    let d = 30.0 + bloom * 190.0;
                    let mx = MEET_X + ang.cos() * d;
                    let my = MEET_Y + ang.sin() * d * 0.7;
                    let ma = (1.0 - merge).max(0.0) * 0.8;
                    book.circle(Offset::new(mx, my), 2.6, alpha(super::ACCENT, ma));
                }
            }
        }),
    )));

    // The mark — the studio's own logo, revealed by the studio's own
    // code (`viewwstudio::ui::brand::revealed`), at the meeting point.
    if mark_a > 0.01 {
        let side = 132.0;
        let rise = (1.0 - ease_out_cubic(clamp01((t - 0.62) / 0.3))) * 26.0;
        stack = stack.push(
            Positioned::new()
                .left(MEET_X - side * 0.5)
                .top(MEET_Y - side * 0.5 + rise - 6.0)
                .width(side)
                .height(side)
                .child(super::Opacity::new(mark_a).child(
                    // The real mark — the studio's own drawing of it.
                    brand_mark(side, editor, preview),
                )),
        );
    }

    // The answer — the question, restated, then answered. The question
    // fades as the answer arrives.
    let q_a = clamp01((t - 0.04) / 0.1) * (1.0 - clamp01((t - 0.5) / 0.1));
    if q_a > 0.01 {
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(570.0)
                .width(W)
                .height(34.0)
                .child(
                    super::Opacity::new(q_a).child(
                        Text::new("how far is a thought from a screen?")
                            .style(
                                TextStyle::new(24.0)
                                    .monospace()
                                    .letter_spacing(2.0)
                                    .color(alpha(MUTED, 0.9)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }
    let answer_a = clamp01((t - 0.62) / 0.16);
    if answer_a > 0.01 {
        let rise = (1.0 - ease_out_cubic(answer_a)) * 14.0;
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(628.0 + rise)
                .width(W)
                .height(56.0)
                .child(
                    super::Opacity::new(answer_a).child(
                        Text::new("one frame.")
                            .style(
                                super::geist(46.0)
                                    .letter_spacing(2.0)
                                    .color(alpha(INK, 0.97)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }

    // The captions — the closing narration.
    stack = stack.push(super::act_chip(
        "MOVEMENT V",
        "ZERO",
        clamp01((t - 0.04) / 0.10),
    ));
    stack = stack.push(caption(
        "the question the film opened with — answered",
        1002.0,
        clamp01((t - 0.10) / 0.12),
    ));

    // The distance chip — falling to its floor, and out.
    stack = stack.push(distance_chip(ctx.abs, clamp01(t / 0.1)));
    stack = stack.push(progress_rail(ctx.abs));

    let _ = xywh;
    stack.into()
}
