//! S10 · THE CADENCE — the metronome: sixty frames, every second.
//! 1:23–1:29.
//!
//! One second of time, stretched across the frame: two timeline strips.
//! The old world's strip — **24** — lights its blips on the held clock,
//! and the 2-3-2-3 pattern is visible as clustering. vieww's strip —
//! **60** — lights evenly, every blip a spark, the cadence the whole
//! film has been running on since frame one. A playhead sweeps both;
//! where it passes, light lands.
//!
//! The scene is short because the law is simple — and it hands the
//! studio its stage: the strip collapses into the session line S11
//! picks up.

use vieww_foundation::{Color, Offset, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use super::{
    alpha, caption, clamp01, tint, xywh, Ctx, CYAN_SOFT, INK, MUTED, VIOLET, VIOLET_SOFT, W,
};
use crate::film_lib::ease_out_cubic;

/// The strips' geometry.
const X0: f32 = 320.0;
const X1: f32 = 1600.0;
const Y24: f32 = 470.0;
const Y60: f32 = 620.0;

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let frame_i = (ctx.abs * 60.0) as u64;

    // The playhead — sweeps the second three times (every 2 s), each
    // sweep re-lighting the strips.
    let sweep = (sec / 2.0).fract();

    let mut stack = Stack::new();

    // The ground — neutral, focused: a metronome's room.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            super::ground(book, w, h);
            super::vignette(book, w, h, 0.55);
            super::grain(book, w, h, frame_i, 0.35);
        }),
    )));

    // THE STRIPS — the second, twice-annotated. The playhead lights
    // blips as it passes; each blip decays behind it.
    let strips = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            // The base lines.
            book.line(
                Offset::new(X0, Y24),
                Offset::new(X1, Y24),
                alpha(Color::WHITE, 0.10),
                1.0,
            );
            book.line(
                Offset::new(X0, Y60),
                Offset::new(X1, Y60),
                alpha(Color::WHITE, 0.10),
                1.0,
            );
            // The second's ticks — 12 major marks, both strips.
            for i in 0..=12 {
                let x = X0 + (X1 - X0) * i as f32 / 12.0;
                book.line(
                    Offset::new(x, Y24 - 6.0),
                    Offset::new(x, Y24 + 6.0),
                    alpha(Color::WHITE, 0.14),
                    1.0,
                );
                book.line(
                    Offset::new(x, Y60 - 6.0),
                    Offset::new(x, Y60 + 6.0),
                    alpha(Color::WHITE, 0.14),
                    1.0,
                );
            }
            // The playhead's position, in strip x.
            let px = X0 + (X1 - X0) * sweep;
            // THE 24 STRIP — blips at 1/24 spacing, held: a blip lights
            // only when the playhead has passed its time slot (the held
            // clock's tell: they arrive in visible clusters).
            for i in 0..24 {
                let slot = i as f32 / 24.0;
                let bx = X0 + (X1 - X0) * slot;
                let lit = sweep >= slot;
                let decay = ((sweep - slot) / 0.9).min(1.0);
                let a = if lit { 1.0 - decay * 0.6 } else { 0.10 };
                book.circle(Offset::new(bx, Y24), 4.0, alpha(MUTED, a));
                if lit && decay < 0.2 {
                    book.ring(
                        Offset::new(bx, Y24),
                        8.0,
                        1.0,
                        alpha(MUTED, 0.5 * (1.0 - decay * 5.0)),
                    );
                }
            }
            // THE 60 STRIP — blips at 1/60 spacing, every one a spark:
            // violet, hot, even. The cadence the film runs on.
            for i in 0..60 {
                let slot = i as f32 / 60.0;
                let bx = X0 + (X1 - X0) * slot;
                let lit = sweep >= slot;
                let decay = ((sweep - slot) / 0.9).min(1.0);
                let a = if lit { 1.0 - decay * 0.55 } else { 0.12 };
                book.circle(Offset::new(bx, Y60), 3.6, alpha(VIOLET_SOFT, a));
                if lit && decay < 0.15 {
                    super::glow(book, bx, Y60, 26.0, VIOLET, 0.30 * (1.0 - decay * 6.0));
                }
            }
            // The playhead — one line through both strips.
            book.line(
                Offset::new(px, Y24 - 26.0),
                Offset::new(px, Y60 + 26.0),
                alpha(tint(VIOLET_SOFT, 0.3), 0.8),
                1.6,
            );
            book.circle(Offset::new(px, Y60 + 30.0), 3.4, alpha(VIOLET_SOFT, 0.95));
            // The strip counts — live, right of the lines.
            let n24 = ((sweep * 24.0).floor() as u32).min(24);
            let n60 = ((sweep * 60.0).floor() as u32).min(60);
            book.rect(
                xywh(X1 + 8.0, Y24 - 12.0, 84.0, 24.0),
                alpha(Color::rgb(16, 16, 21), 0.8),
            );
            book.rect(
                xywh(X1 + 8.0, Y60 - 12.0, 84.0, 24.0),
                alpha(Color::rgb(16, 16, 21), 0.8),
            );
            let _ = (n24, n60);
        }),
    );
    stack = stack.push(Positioned::fill().child(strips));

    // The strip counts — as text (mono, right of each line).
    let n24 = ((sweep * 24.0).floor() as u32).min(24);
    let n60 = ((sweep * 60.0).floor() as u32).min(60);
    for (y, label, val, color) in [
        (Y24, "the old world · 24", n24, MUTED),
        (Y60, "vieww · 60", n60, VIOLET_SOFT),
    ] {
        stack = stack.push(
            Positioned::new()
                .left(X0)
                .top(y - 56.0)
                .width(500.0)
                .height(30.0)
                .child(
                    Text::new(label)
                        .style(
                            TextStyle::new(22.0)
                                .monospace()
                                .letter_spacing(2.6)
                                .color(alpha(color, 0.95)),
                        )
                        .align(TextAlign::Left),
                ),
        );
        stack = stack.push(
            Positioned::new()
                .left(X1 + 12.0)
                .top(y - 16.0)
                .width(120.0)
                .height(28.0)
                .child(
                    Text::new(format!("{:>2}", val))
                        .style(
                            TextStyle::new(22.0)
                                .monospace()
                                .letter_spacing(1.0)
                                .color(alpha(color, 0.95)),
                        )
                        .align(TextAlign::Left),
                ),
        );
    }

    // The title — the law, stated once.
    let title_a = clamp01((t - 0.04) / 0.10);
    stack = stack.push(
        Positioned::new()
            .left(0.0)
            .top(280.0)
            .width(W)
            .height(60.0)
            .child(
                Opacity::new(title_a).child(
                    Text::new("one second, twice-annotated")
                        .style(
                            TextStyle::new(34.0)
                                .weight(vieww_foundation::FontWeight::Medium)
                                .letter_spacing(2.0)
                                .color(alpha(INK, 0.96)),
                        )
                        .align(TextAlign::Center),
                ),
            ),
    );

    // The caption — the cadence's beat.
    stack = stack.push(caption(
        "the cadence is the product's own — this film runs on it",
        1002.0,
        clamp01((t - 0.14) / 0.10),
    ));

    // The hand-off — in the last beat, the 60 strip collapses toward a
    // single line (the session line S11 picks up): the blips ease toward
    // the center y as the scene closes.
    if t > 0.82 {
        let collapse = clamp01((t - 0.82) / 0.18);
        let merge = Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                // The strip's glow band, easing toward a hairline.
                let band_h = 40.0 * (1.0 - collapse) + 2.0;
                book.rect(
                    xywh(X0, Y60 - band_h * 0.5, X1 - X0, band_h),
                    alpha(VIOLET, 0.10 * (1.0 - collapse * 0.5)),
                );
            }),
        );
        stack = stack.push(Positioned::fill().child(merge));
    }

    let _ = (CYAN_SOFT, super::H, ease_out_cubic(t));

    stack.into()
}
