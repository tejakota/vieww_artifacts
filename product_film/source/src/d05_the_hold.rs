//! D05 · THE HOLD — the film's last breath. 4:50–5:00.
//!
//! The mark breathes once, the final line holds, and the frame fades
//! to the ground it came from. The distance readout reads its floor —
//! `0.0 ms` — and dissolves with everything else. What remains on the
//! black, for the last second, is the caret: the point the film began
//! with, still blinking, still ready. The loop is the invitation:
//! *your thought is the next one.*

use vieww_foundation::{Color, Gradient, Rect, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;

use super::{
    ACCENT, Ctx, INK, MUTED, W, alpha, brand_mark, clamp01, glow, tint, xywh,
};
use crate::film_lib::ease_out_cubic;

/// The fade — everything leaves except the caret.
fn fade(t: f32) -> f32 {
    1.0 - ease_out_cubic(clamp01((t - 0.55) / 0.35))
}

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let f = fade(t);

    let mut stack = Stack::new();

    // The ground — deep, quiet, the film's first register one last time.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(8, 8, 11)),
                    (0.7, super::BG_DEEP),
                    (1.0, Color::rgb(6, 6, 9)),
                ]),
            );
        }),
    )));

    // The mark — centered, breathing once, fading with the hold.
    if f > 0.01 {
        let breathe = 1.0 + 0.02 * (sec * 1.4).sin();
        let side = 96.0 * breathe;
        stack = stack.push(
            Positioned::new()
                .left(W * 0.5 - side * 0.5)
                .top(360.0)
                .width(side)
                .height(side)
                .child(
                    super::Opacity::new(f).child(brand_mark(side, 1.0, 1.0)),
                ),
        );
    }

    // The final line — the film's thesis, held, then gone.
    if f > 0.01 {
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(512.0)
                .width(W)
                .height(40.0)
                .child(
                    super::Opacity::new(f).child(
                        Text::new("the distance is one frame.")
                            .style(
                                TextStyle::new(26.0)
                                    .monospace()
                                    .letter_spacing(3.0)
                                    .color(alpha(INK, 0.95)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }

    // The sub-line — the invitation, quiet.
    if f > 0.5 {
        let a = (f - 0.5) * 2.0;
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(566.0)
                .width(W)
                .height(30.0)
                .child(
                    super::Opacity::new(a).child(
                        Text::new("your thought is the next one.")
                            .style(
                                TextStyle::new(16.0)
                                    .monospace()
                                    .letter_spacing(2.4)
                                    .color(alpha(MUTED, 0.85)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }

    // The caret — the last thing on the frame. The film's first point,
    // returned: blinking at its terminal rest, center-frame, on the
    // black. It outlives the fade.
    let caret_a = clamp01((t - 0.30) / 0.25) * (if t > 0.55 { 1.0 } else { 0.35 });
    if caret_a > 0.01 {
        let cx = W * 0.5;
        let cy = 660.0;
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let on = (sec * 2.2).fract() < 0.55;
                glow(book, cx, cy, 100.0, ACCENT, 0.14);
                if on {
                    book.rrect(xywh(cx - 2.5, cy - 22.0, 5.0, 44.0), 2.0, alpha(tint(ACCENT, 0.4), 1.0));
                    book.rrect(xywh(cx - 1.5, cy - 18.0, 3.0, 36.0), 1.5, alpha(Color::WHITE, 0.85));
                }
            }),
        )));
    }

    stack.into()
}
