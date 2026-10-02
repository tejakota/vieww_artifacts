//! A02 · THE TOLLS — the second pain: the middleman. 0:26–0:40.
//!
//! Between the thought and the screen, the stack stacks: DOM, CSS,
//! JavaScript, the browser, the engine, the compositor, the garbage
//! collector. Each slab lands with weight — the gap line sags under
//! them — and each one is a *toll* on the way from intent to pixel:
//! a translation, a layout pass, a style resolution, a paint, a
//! composite. Nobody chose the tower; it accreted. The film drops the
//! slabs one by one, and the line's sag deepens with every landing.

use vieww_foundation::{Color, Offset, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;

use super::{
    alpha, caption, clamp01, distance_chip, gap_line, grain, ground, pole_caret, pole_screen,
    progress_rail, spring_out, tint, vignette, xywh, Ctx, BREAK_RED, GROUND, MUTED, SYN_COMMENT,
    SYN_FUNCTION, SYN_KEYWORD, SYN_NUMBER, SYN_PUNCT, SYN_TYPE, W,
};
use crate::film_lib::ease_out_cubic;

/// The tower's layers — the old world's middlemen, top of tower first.
/// The colors are the syntax ramp's: each layer reads as a *language*
/// the thought must be translated into on its way to the screen.
const LAYERS: [(&str, Color); 7] = [
    ("DOM", SYN_KEYWORD),
    ("CSS", SYN_TYPE),
    ("JS", SYN_FUNCTION),
    ("WASM", SYN_MACRO2),
    ("BROWSER", SYN_STRING2),
    ("ENGINE", SYN_NUMBER),
    ("COMPOSITOR", SYN_PUNCT),
];

// Local aliases so the table above stays one-token-per-cell.
const SYN_MACRO2: Color = SYN_FUNCTION;
const SYN_STRING2: Color = SYN_TYPE;

/// Where the tower stands — the middle of the gap.
const TOWER_X: f32 = 960.0;
const TOWER_Y: f32 = 560.0;
const SLAB_W: f32 = 320.0;
const SLAB_H: f32 = 52.0;

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
            vignette(book, w, h, 0.6);
            grain(book, w, h, frame_i, 0.4);
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(room));

    // The sag deepens as slabs land — the count that have arrived.
    let landed = LAYERS
        .iter()
        .enumerate()
        .filter(|(i, _)| sec > 0.8 + *i as f32 * 1.5)
        .count();
    let sag = 6.0 + landed as f32 * 9.0;

    // The poles + the sagging line, under the tower.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            gap_line(book, 430.0, 560.0, 1360.0, 0.0, sec, sag, 0.0, MUTED, 0.9);
            pole_caret(book, 560.0, 430.0, sec, 1.0);
            pole_screen(book, 1360.0, 430.0, 0.0, 0.85, MUTED);
        }),
    )));

    // The tower — each slab drops in on a spring, lands with weight,
    // and its label fades up. The tower grows *downward* from the line.
    for (i, (label, color)) in LAYERS.iter().enumerate() {
        let land_t = clamp01((sec - (0.8 + i as f32 * 1.5)) / 0.5);
        if land_t <= 0.0 {
            continue;
        }
        // The spring drop — overshoot reads as weight.
        let drop = spring_out(land_t, 9.0, 0.55);
        let y = TOWER_Y - SLAB_H * drop - i as f32 * SLAB_H * 1.06;
        let slab_a = ease_out_cubic(clamp01(land_t * 1.4));
        let color = *color;
        let label = *label;
        let judd_y = if land_t < 1.0 {
            0.0
        } else {
            (sec * 2.0 + i as f32).sin() * 0.6
        };

        stack = stack.push(
            Positioned::new()
                .left(TOWER_X - SLAB_W * 0.5)
                .top(y + judd_y)
                .width(SLAB_W)
                .height(SLAB_H)
                .child(super::Opacity::new(slab_a).child(Painting::sized(
                    Size::new(SLAB_W, SLAB_H),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        // The slab — a language bar: heavy body, top rim.
                        book.rrect(xywh(0.0, 0.0, SLAB_W, SLAB_H), 8.0, alpha(GROUND, 0.92));
                        book.stroke_rrect(
                            xywh(0.0, 0.0, SLAB_W, SLAB_H),
                            8.0,
                            alpha(color, 0.55),
                            1.6,
                        );
                        book.rrect(xywh(1.0, 1.0, SLAB_W - 2.0, 3.0), 2.0, alpha(color, 0.30));
                        // The tick — a toll paid.
                        book.line(
                            Offset::new(18.0, SLAB_H * 0.5),
                            Offset::new(34.0, SLAB_H * 0.5),
                            alpha(color, 0.85),
                            2.6,
                        );
                    }),
                ))),
        );
        // The label.
        stack = stack.push(
            Positioned::new()
                .left(TOWER_X - SLAB_W * 0.5 + 46.0)
                .top(y + judd_y + 15.0)
                .width(SLAB_W - 60.0)
                .height(24.0)
                .child(
                    super::Opacity::new(slab_a).child(
                        Text::new(label)
                            .style(
                                TextStyle::new(17.0)
                                    .monospace()
                                    .letter_spacing(3.0)
                                    .color(alpha(color, 0.95)),
                            )
                            .align(TextAlign::Left),
                    ),
                ),
        );
    }

    // The toll count — one per layer, top-right, counting with the drops.
    let toll_a = clamp01((t - 0.10) / 0.2);
    if toll_a > 0.01 {
        stack = stack.push(
            Positioned::new()
                .left(W - 460.0)
                .top(140.0)
                .width(300.0)
                .height(56.0)
                .child(super::Opacity::new(toll_a).child(Painting::sized(
                    Size::new(300.0, 56.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        book.rrect(
                            xywh(0.0, 0.0, 300.0, 50.0),
                            9.0,
                            alpha(super::SURFACE_2, 0.9),
                        );
                        book.stroke_rrect(
                            xywh(0.0, 0.0, 300.0, 50.0),
                            9.0,
                            alpha(BREAK_RED, 0.30),
                            1.1,
                        );
                    }),
                ))),
        );
        stack = stack.push(super::chrome(
            Positioned::new()
                .left(W - 460.0 + 20.0)
                .top(152.0)
                .width(260.0)
                .height(26.0)
                .child(
                    super::Opacity::new(toll_a).child(
                        Text::new(format!("translations: {} / {}", landed, LAYERS.len()))
                            .style(
                                TextStyle::new(15.0)
                                    .monospace()
                                    .letter_spacing(1.6)
                                    .color(alpha(tint(BREAK_RED, 0.3), 1.0)),
                            )
                            .align(TextAlign::Left),
                    ),
                )
                .into(),
        ));
    }

    // The captions.
    stack = stack.push(super::act_chip("MOVEMENT I", "THE FAR", 1.0));
    stack = stack.push(caption(
        "every layer between intent and pixel is a toll booth",
        1002.0,
        clamp01((t - 0.05) / 0.12),
    ));
    let late = clamp01((t - 0.78) / 0.14);
    if late > 0.01 {
        stack = stack.push(caption("nobody chose the tower. it accreted", 966.0, late));
    }

    stack = stack.push(distance_chip(ctx.abs, clamp01(t / 0.1)));
    stack = stack.push(progress_rail(ctx.abs));

    let _ = SYN_COMMENT;
    stack.into()
}
