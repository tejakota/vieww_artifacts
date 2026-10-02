//! S01 · THE BLANK — the film's first breath: a dark room, drifting dust,
//! and one blinking caret. 0:00–0:09.
//!
//! Nothing else. The question types itself below the point — *how far is
//! a thought from a screen?* — and the caret's dormant glow breathes
//! stronger as the scene closes, handing its light to the old world's
//! terminal (S02). The spark is not born yet; this is the dark it is
//! born into.
//!
//! The loop's bookend: S24 ends on this exact frame — the light collapses
//! back into the blinking point, and the film becomes its own ouroboros.

use vieww_foundation::{Color, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;

use super::{alpha, caption, clamp01, mono_w, tint, xywh, Ctx, MUTED, VIOLET, VIOLET_SOFT, W};
use crate::film_lib::ease_out_cubic;

/// The question — typed, never pasted.
const QUESTION: &str = "how far is a thought from a screen?";

/// The caret's blink period, seconds (≈2.2 Hz — a terminal at rest).
const BLINK_HZ: f32 = 2.2;

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let frame_i = (ctx.abs * 60.0) as u64;

    // The room — deep ground, slow dust, a heavy vignette, faint grain.
    // Nothing else moves: the wait before the wait.
    let dust_a = 0.5 + 0.5 * (t * 2.4).sin();
    let room = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            super::ground(book, w, h);
            super::dust(book, w, h, t, 0x8121, 0.9);
            super::vignette(book, w, h, 0.62);
            super::grain(book, w, h, frame_i, 0.55);
            let _ = dust_a;
        }),
    );

    // The caret — center frame, blinking at a terminal's rest. This exact
    // point, this exact blink, is where S24 will land the collapsing
    // spark: the loop's two ends meet here.
    let on = (sec * BLINK_HZ).fract() < 0.55;
    let caret = Painting::sized(
        Size::new(24.0, 64.0),
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            // The dormant glow — the spark asleep inside the caret. It
            // breathes on a slow harmonic of its own, and swells gently
            // through the scene (the hand-off to S02's cursor).
            let swell = 0.05 + 0.045 * t + 0.02 * (sec * 0.8).sin();
            super::glow(book, 12.0, 32.0, 110.0 + 30.0 * t, VIOLET, swell);
            if on {
                book.rrect(
                    xywh(10.0, 6.0, 4.0, 52.0),
                    2.0,
                    alpha(tint(VIOLET_SOFT, 0.4), 0.98),
                );
                book.rrect(xywh(10.5, 8.0, 3.0, 48.0), 1.5, alpha(Color::WHITE, 0.85));
            }
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(room)).push(
        Positioned::new()
            .left(W * 0.5 - 12.0)
            .top(440.0)
            .width(24.0)
            .height(64.0)
            .child(caret),
    );

    // The question — types itself under the caret, mid-scene, in the
    // instrument voice; its own caret rides the type-on.
    let type_p = clamp01((t - 0.30) / 0.42);
    let typed_n = (QUESTION.chars().count() as f32 * ease_out_cubic(type_p)).round() as usize;
    let shown: String = QUESTION.chars().take(typed_n).collect();
    if typed_n > 0 {
        let y = 556.0;
        let x0 = W * 0.5 - mono_w(24.0, QUESTION.chars().count()) * 0.5;
        stack = stack.push(
            Positioned::new()
                .left(x0)
                .top(y)
                .width(mono_w(24.0, QUESTION.chars().count()) + 30.0)
                .height(36.0)
                .child(
                    Text::new(shown)
                        .style(
                            TextStyle::new(24.0)
                                .monospace()
                                .letter_spacing(1.5)
                                .color(alpha(MUTED, 0.85)),
                        )
                        .align(TextAlign::Left),
                ),
        );
        // The question's own caret — a second, dimmer point of light.
        let q_on = (sec * BLINK_HZ + 0.5).fract() < 0.55;
        if q_on && typed_n < QUESTION.chars().count() {
            let cx = x0 + mono_w(24.0, typed_n) + 3.0;
            stack = stack.push(
                Positioned::new()
                    .left(cx)
                    .top(y + 5.0)
                    .width(10.0)
                    .height(28.0)
                    .child(Container::new().color(alpha(VIOLET_SOFT, 0.75)).radius(1.5)),
            );
        }
    }

    // The captions — the film's voice, bottom-left, staggered.
    stack = stack.push(caption(
        "every interface begins as a blinking point",
        1002.0,
        clamp01((t - 0.08) / 0.14),
    ));
    stack = stack.push(caption(
        "this is the dark before the light — hold the frame",
        966.0,
        clamp01((t - 0.66) / 0.12),
    ));

    // The pre-spark — one mote of light, far stage right, almost missed:
    // the film's protagonist, waiting in the wings for S04.
    if t > 0.5 {
        let mote_a = clamp01((t - 0.5) / 0.2) * 0.55;
        let mote_abs = ctx.abs;
        let mote = Painting::sized(
            Size::new(60.0, 60.0),
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                super::spark(book, 30.0, 30.0, 7.0, mote_abs, mote_a, VIOLET_SOFT);
            }),
        );
        let drift = (sec * 6.0).sin() * 8.0;
        stack = stack.push(
            Positioned::new()
                .left(1560.0 + drift)
                .top(300.0)
                .width(60.0)
                .height(60.0)
                .child(mote),
        );
    }

    stack.into()
}
