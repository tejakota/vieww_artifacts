//! P01 · THE TWO POINTS — the film's question. 0:00–0:12.
//!
//! A dark room, and two points of light in it: on the left, a caret —
//! a thought, blinking at a terminal's rest; on the right, a screen
//! outline — a person, waiting to see it. Between them, a dashed axis
//! crawls: the distance, measured, held at 45,000 ms. The question
//! types itself between the poles — *how far is a thought from a
//! screen?* — and the distance chip arrives as the film's instrument.
//!
//! This is the curiosity beat: nothing hurts yet, nothing is sold.
//! Just the question, the dark, and the two lights.

use vieww_foundation::{Color, Offset, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;

use super::{
    ACCENT, Ctx, GROUND, INK, MUTED, W, alpha, caption, clamp01, distance_chip, dust,
    gap_line, glow, grain, ground, pole_caret, pole_screen, progress_rail, vignette,
};
use crate::film_lib::ease_out_cubic;

/// The question — typed, never pasted.
const QUESTION: &str = "how far is a thought from a screen?";

/// The poles' positions — the film's whole geography, set here.
/// Every later scene inherits these two x positions.
pub const CARET_X: f32 = 560.0;
pub const SCREEN_X: f32 = 1360.0;
pub const POLE_Y: f32 = 430.0;

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let frame_i = (ctx.abs * 60.0) as u64;

    // The room — deep ground, slow dust, a heavy vignette, faint grain.
    let room = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            ground(book, w, h);
            dust(book, w, h, t, 0x8121, 0.9);
            vignette(book, w, h, 0.62);
            grain(book, w, h, frame_i, 0.55);
        }),
    );

    // The two points arrive — the caret first, the screen answering it.
    let caret_a = ease_out_cubic(clamp01((t - 0.06) / 0.14));
    let screen_a = ease_out_cubic(clamp01((t - 0.22) / 0.14));

    // The axis between them — dashes crawling once both poles exist.
    let line_a = clamp01((t - 0.36) / 0.16);

    let mut stack = Stack::new().push(Positioned::fill().child(room));

    // The poles and the axis, one painting.
    let line_a2 = line_a;
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            gap_line(book, POLE_Y, CARET_X, SCREEN_X, 0.0, sec, 0.0, 0.0, ACCENT, line_a2);
            pole_caret(book, CARET_X, POLE_Y, sec, caret_a);
            pole_screen(book, SCREEN_X, POLE_Y, 0.0, screen_a, MUTED);
            // The measurement halo — the room registers the distance.
            if line_a2 > 0.5 {
                let breathe = 0.5 + 0.5 * (sec * 0.7).sin();
                glow(book, (CARET_X + SCREEN_X) * 0.5, POLE_Y, 220.0, GROUND, 0.0);
                let _ = breathe;
            }
        }),
    )));

    // The labels — the two roles, named once.
    let label_a = ease_out_cubic(clamp01((t - 0.40) / 0.14));
    if label_a > 0.01 {
        for (text, x) in [("a thought", CARET_X), ("a screen", SCREEN_X)] {
            stack = stack.push(
                Positioned::new()
                    .left(x - 180.0)
                    .top(POLE_Y + 92.0)
                    .width(360.0)
                    .height(26.0)
                    .child(
                        Text::new(text)
                            .style(
                                TextStyle::new(15.0)
                                    .monospace()
                                    .letter_spacing(2.4)
                                    .color(alpha(MUTED, 0.9 * label_a)),
                            )
                            .align(TextAlign::Center),
                    ),
            );
        }
    }

    // The question — types itself between the poles, mid-scene.
    let type_p = clamp01((t - 0.34) / 0.34);
    // One anchor, one tracked measurement — see `type_on`. The anchor here
    // was already right; the drift was `gmono_w` ignoring the 1.6 tracking,
    // so the caret fell a pixel and a half behind per character and was
    // most of a word adrift by the end of the line.
    stack = stack.push(super::type_on(
        QUESTION,
        super::TypeAt::CenteredOn((W * 0.5) as i32),
        620.0,
        TextStyle::new(26.0)
            .monospace()
            .letter_spacing(1.6)
            .color(alpha(INK, 0.92)),
        type_p,
        sec,
    ));

    // The captions — the question's framing.
    stack = stack.push(caption(
        "every screen you have ever shipped began as one of these",
        1002.0,
        clamp01((t - 0.10) / 0.12),
    ));
    stack = stack.push(caption(
        "this film is the story of the distance between them",
        966.0,
        clamp01((t - 0.58) / 0.12),
    ));

    // The distance chip — the instrument arrives with the question.
    stack = stack.push(distance_chip(ctx.abs, clamp01((t - 0.52) / 0.14)));
    stack = stack.push(progress_rail(ctx.abs));

    let _ = Color::WHITE;
    let _ = Offset::new(0.0, 0.0);
    stack.into()
}
