//! S22 · THE LOOP — the fold-back. 3:34–3:39.
//!
//! The end card dissolves; the film's first image returns for one breath
//! — the terminal cursor, blinking in the dark of S01 — and then the
//! violet spark of S03/S04 takes its place. The loop closes: the old
//! world's cursor becomes the new world's light. Five seconds, the
//! quietest scene in the film, and the cut back to S01 (for those who
//! loop the film) lands on the exact same darkness.

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{alpha, clamp01, ease_in_out, ease_out_cubic, mix, tint, xywh, INK, MUTED, VIOLET, VIOLET_SOFT};

use super::{Ctx};

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = Stack::new();

    // The ground — fading to the film's first black.
    let fade = ease_in_out(clamp01(t / 0.5));
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(7, 7, 9)),
                    (0.65, crate::film_lib::BG_DEEP),
                    (1.0, Color::rgb(8, 8, 10)),
                ]),
            );
            // The stars, going out one by one.
            super::stars(book, w, h, 0xE2D1, 60, t, 0.09 * (1.0 - fade));
            super::vignette(book, w, h, 0.55);
        }),
    )));

    // The terminal cursor — S01's ghost, one blink.
    if t < 0.62 {
        let a = (1.0 - clamp01((t - 0.5) / 0.12)) * clamp01(t / 0.1);
        let on = (sec * 2.2).fract() < 0.55;
        if on && a > 0.0 {
            stack = stack.push(
                Positioned::new()
                    .left(960.0 - 7.0)
                    .top(520.0)
                    .width(14.0)
                    .height(28.0)
                    .child(Opacity::new(a).child(
                        Container::new().color(alpha(super::TERM_GREEN, 0.85)).radius(2.0),
                    )),
            );
        }
    }

    // The spark — takes the cursor's place.
    let spark_t = clamp01((t - 0.60) / 0.18);
    if spark_t > 0.0 {
        let breathe = 0.5 + 0.5 * (sec * 5.0).sin();
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let r = 3.0 + spark_t * 6.0 + breathe * 2.0;
                super::glow(book, 960.0, 534.0, 90.0 + spark_t * 120.0, VIOLET, (0.28 + 0.2 * breathe) * spark_t);
                book.circle(Offset::new(960.0, 534.0), r, alpha(tint(VIOLET_SOFT, 0.5), spark_t));
            }),
        )));
    }

    stack.into()
}
