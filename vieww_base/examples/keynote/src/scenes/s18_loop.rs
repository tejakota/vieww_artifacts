//! **S18 · THE LOOP** — the last ten seconds, and the first.
//!
//! The end card's light narrows to a seam, the seam holds, and the frame
//! resolves into the shape S01 opened on: a dark room with one rectangle of
//! light in it. The film is built to run on a loop at a launch, so its last
//! frame is engineered to be indistinguishable from its first — same black,
//! same vignette, same one bright horizontal in the middle of the frame.
//!
//! One thing survives the fold: the witness counter, at **7**, held for two
//! seconds in the centre of an otherwise empty frame. It is the last image,
//! and it is the film's argument compressed to a single glyph — the session
//! never restarted, and neither does this.
//!
//! Then black, and the loop point.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Rect, Size, Sketchbook};
use vieww_widget::prelude::*;

use crate::film::Ctx;
use crate::kit::{
    alpha, caption, clamp01, dust, ease_in_cubic, ease_out_cubic, grain, ground, mix, painter, seg,
    smoothstep, vignette, xywh, Type, CYAN, H, INK, INK_SOFT, MUTED, VIOLET, VIOLET_SOFT, VOID, W,
};
use crate::studio::compose;

/// The beats, in scene-seconds.
const COLLAPSE_T: f32 = 0.60;
const HOLD_T: f32 = 3.20;
const FOLD_T: f32 = 6.40;

pub fn frame(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let frame_i = ctx.frame;

    // The collapse: the end card's width narrows to a seam.
    let collapse = ease_out_cubic(seg(sec, COLLAPSE_T, COLLAPSE_T + 1.6));
    // The counter, held.
    let hold = smoothstep(seg(sec, HOLD_T, HOLD_T + 0.7)) * (1.0 - smoothstep(seg(sec, FOLD_T, FOLD_T + 0.9)));
    // The fold: the seam becomes the terminal's own rectangle of light, the
    // shape S01 opens on.
    let fold = ease_in_cubic(seg(sec, FOLD_T, FOLD_T + 2.4));
    let black = smoothstep(seg(sec, 9.0, 9.9));

    let bg = painter(move |book: &mut Sketchbook, size: Size| {
        let (w, h) = (size.width, size.height);
        ground(book, size, sec, 0.9 * (1.0 - fold * 0.7));
        dust(book, size, sec, 26, VIOLET_SOFT, 0.4 * (1.0 - fold));

        // The seam: the film's whole light, narrowed to a line.
        let seam_w = w * (1.0 - collapse * 0.90) * (1.0 - fold * 0.55);
        let seam_h = 3.0 + 2.0 * (1.0 - collapse);
        if seam_w > 2.0 && black < 0.98 {
            let x = w * 0.5 - seam_w * 0.5;
            let y = h * 0.5 - seam_h * 0.5;
            book.blended_layer(1.0, 26.0, BlendMode::Plus, None, |g| {
                g.rrect(
                    xywh(x, y - 3.0, seam_w, seam_h + 6.0),
                    (seam_h + 6.0) * 0.5,
                    Gradient::horizontal().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.0)),
                        (0.5, alpha(mix(VIOLET_SOFT, Color::WHITE, 0.45), 0.55 * (1.0 - black))),
                        (1.0, alpha(CYAN, 0.0)),
                    ]),
                );
            });
            book.rrect(
                xywh(x, y, seam_w, seam_h),
                seam_h * 0.5,
                Gradient::horizontal().with_dither().with_stops(&[
                    (0.0, alpha(VIOLET, 0.0)),
                    (0.42, alpha(VIOLET_SOFT, 0.85 * (1.0 - black))),
                    (0.62, alpha(Color::WHITE, 0.90 * (1.0 - black))),
                    (1.0, alpha(CYAN, 0.0)),
                ]),
            );
        }

        // The fold: the seam opens back out into a rectangle — the terminal
        // S01 begins in. The film's last shape is its first.
        if fold > 0.02 && black < 0.98 {
            let e = ease_out_cubic(fold);
            let tw = 520.0 * e;
            let th = 320.0 * e;
            let r = xywh(w * 0.5 - tw * 0.5, h * 0.5 - th * 0.5, tw, th);
            book.rect(
                r,
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, alpha(Color::rgb(13, 13, 15), 0.92 * e * (1.0 - black))),
                    (1.0, alpha(Color::rgb(9, 9, 11), 0.92 * e * (1.0 - black))),
                ]),
            );
            book.stroke_rrect(r, 0.0, alpha(Color::rgb(46, 44, 42), 0.35 * e * (1.0 - black)), 1.0);
            // Three faint lines of a log that has not started yet.
            for i in 0..3 {
                book.rect(
                    xywh(r.left + 24.0, r.top + 32.0 + i as f32 * 20.0, (r.width() - 48.0) * (0.7 - i as f32 * 0.18), 4.0),
                    alpha(Color::rgb(60, 58, 56), 0.5 * e * (1.0 - black)),
                );
            }
        }

        vignette(book, size, 1.0 + 0.35 * fold);
        grain(book, size, frame_i, 0.016 * (1.0 - black), 380);
        if black > 0.004 {
            book.rect(Rect::new(0.0, 0.0, w, h), alpha(VOID, black));
        }
    });

    let mut nodes: Vec<WidgetNode> = Vec::new();

    // The counter, held alone, at seven.
    if hold > 0.004 {
        nodes.push(
            Type::new(format!("{}", ctx.spine.witness))
                .size(150.0)
                .bold()
                .color(alpha(INK, 0.97 * hold))
                .center()
                .banner(H * 0.5 - 168.0)
                .into(),
        );
        nodes.push(
            Type::new("seven touches · one session · never restarted")
                .mono()
                .size(15.0)
                .track(3.2)
                .color(alpha(VIOLET_SOFT, 0.9 * hold))
                .center()
                .banner(H * 0.5 + 76.0)
                .into(),
        );
    }

    // The loop point, named — so a room running this on repeat knows the
    // film meant to come back round.
    let loop_a = smoothstep(seg(sec, 8.0, 8.8)) * (1.0 - smoothstep(seg(sec, 9.2, 9.8)));
    if loop_a > 0.004 {
        nodes.push(
            Type::new("↻")
                .size(26.0)
                .color(alpha(MUTED, 0.7 * loop_a))
                .center()
                .banner(H * 0.5 - 16.0)
                .into(),
        );
    }

    nodes.push(caption(
        "the last frame is the first",
        smoothstep(seg(sec, 1.2, 2.0)) * (1.0 - smoothstep(seg(sec, 7.2, 8.0))) * 0.8,
    ));
    let _ = (INK_SOFT, clamp01, W, Size::new(0.0, 0.0), Offset::ZERO, Rect::ZERO);
    compose(bg, nodes)
}
