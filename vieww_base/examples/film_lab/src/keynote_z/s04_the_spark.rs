//! S04 · THE SPARK — the film's protagonist is born, and says its name.
//! 0:26–0:37.
//!
//! Out of the break's flood: one point of violet light, alone in the
//! dark. It blooms — halo, ring, seven orbiting motes — god rays sweep
//! the room, an aurora unfurls behind it, and the stars come out for
//! the first time in the film (the world, warming). Then the wordmark:
//! **vieww**, five letters arriving one by one on springs, each with
//! its landing bloom, a hairline underline sweeping beneath. The
//! captions say what the light is; the chips say what it promises.
//!
//! The palette flips here and never flips back: violet/cyan from this
//! frame to the end. The spark settles above the wordmark — its post
//! until S05 dives into it.

use vieww_foundation::{Color, Offset, Size, Sketchbook, TextAlign, TextStyle, FontWeight};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{ease_out_cubic};
use super::{CANVAS, CYAN, CYAN_SOFT, Ctx, H, INK, MINT, MUTED, VIOLET, VIOLET_SOFT, W, alpha, aurora, bokeh, caption, chip_row, clamp01, draw_mark, glow, grain, ground, light_rays, mono, mono_w, spark, spring_out, stars, stars_parallax, tint, vignette, xywh};


/// The wordmark's letters — positions derived from the mono advance.
const WORD: &str = "vieww";

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let frame_i = (ctx.abs * 60.0) as u64;

    // The beats: the point (0–0.16) · the bloom (0.16–0.42) · the word
    // (0.38–0.68) · the promises (0.66–1).
    let point = clamp01(t / 0.16);
    let bloom = clamp01((t - 0.14) / 0.28);
    let word = clamp01((t - 0.36) / 0.30);
    let promises = clamp01((t - 0.64) / 0.22);

    // The camera — a slow push-in begins as the word lands (the dive
    // into S05 starts here, in the knees).
    let cam_y = 0.0 + 40.0 * clamp01((t - 0.55) / 0.45);

    // The sky — the film's home register, arriving with the bloom:
    // ground, parallax stars, aurora, bokeh, vignette, grain.
    let sky = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            super::ground(book, w, h);
            // The stars — out for the first time, riding the camera.
            super::stars_parallax(book, w, h, 0xE2D1, 130, t, 0.12 * bloom, 0.0, cam_y);
            // The aurora — two calm curtains, far behind everything.
            if bloom > 0.05 {
                super::aurora(book, w, h, t, 0xA4C1, 0.8 * bloom);
            }
            // The bokeh — the room's warm air, defocused.
            if bloom > 0.2 {
                super::bokeh(book, w, h, t, 0x30C4, 14, 0.08 * bloom, VIOLET);
            }
            super::vignette(book, w, h, 0.5);
            super::grain(book, w, h, frame_i, 0.45);
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(sky));

    // THE SPARK — the point of light. Grows from a mote to its full size
    // with the bloom; god rays arrive behind it; it settles upward to
    // its post above the wordmark once the word begins to land.
    let spark_r = 6.0 + 22.0 * ease_out_cubic(bloom);
    let spark_y = 470.0 - 120.0 * clamp01((t - 0.38) / 0.24); // settles upward
    let ray_a = clamp01((bloom - 0.25) / 0.5) * (1.0 - 0.5 * clamp01((t - 0.6) / 0.4));
    let light = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let _ = s;
            // The rays — the room, lit by one point.
            if ray_a > 0.02 {
                super::light_rays(book, W * 0.5, spark_y, spark_r, 1000.0, sec * 0.10, sec, ray_a * 0.8, VIOLET_SOFT);
            }
            // The spark itself — the film's protagonist, in full costume.
            super::spark(book, W * 0.5, spark_y, spark_r, sec, point.max(0.2), VIOLET_SOFT);
            // Its ground-glow — the floor catching the light.
            super::glow(book, W * 0.5, H - 140.0, 600.0, VIOLET, 0.10 * bloom);
        }),
    );
    stack = stack.push(Positioned::fill().child(light));

    // THE WORDMARK — five letters, one per beat, each landing on a spring
    // with a bloom beneath it. Mono at hero scale: the runtime's name in
    // the instrument voice.
    let size = 148.0;
    let tracking = 14.0;
    let word_w = mono_w(size, WORD.chars().count()) + tracking * (WORD.chars().count() - 1) as f32;
    let x0 = W * 0.5 - word_w * 0.5;
    let base_y = 560.0;
    for (i, ch) in WORD.chars().enumerate() {
        // Staggered arrival — 0.09 of the word window per letter.
        let li = clamp01((word - i as f32 * 0.10) / 0.5);
        if li <= 0.01 {
            continue;
        }
        let drop = spring_out(li, 11.0, 0.5);
        let y = base_y - (1.0 - drop) * 60.0;
        let a = ease_out_cubic(li);
        let cx = x0 + i as f32 * (mono_w(size, 1) + tracking);
        // The landing bloom — the letter's own light, flashing as it
        // settles (heavier for the doubled w's: the word ends in an
        // echo).
        let bloom_a = (1.0 - li).max(0.0) * 0.5;
        let glyph = Painting::sized(
            Size::new(mono_w(size, 1) + 40.0, 200.0),
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                if bloom_a > 0.02 {
                    super::glow(book, 30.0, 100.0, 130.0, VIOLET, bloom_a);
                }
            }),
        );
        stack = stack.push(
            Positioned::new()
                .left(cx - 20.0)
                .top(y - 40.0)
                .width(mono_w(size, 1) + 40.0)
                .height(200.0)
                .child(glyph),
        );
        stack = stack.push(
            Positioned::new()
                .left(cx)
                .top(y)
                .width(mono_w(size, 1) + 8.0)
                .height(160.0)
                .child(Opacity::new(a).child(
                    Text::new(ch.to_string())
                        .style(
                            TextStyle::new(size)
                                .monospace()
                                .weight(FontWeight::Medium)
                                .letter_spacing(0.0)
                                .color(alpha(INK, 0.97)),
                        )
                        .align(TextAlign::Left),
                )),
        );
    }

    // The underline — springs across beneath the word once it has landed.
    let ul = spring_out(clamp01((t - 0.62) / 0.26), 12.0, 0.45);
    if ul > 0.02 {
        let w = word_w * ul.min(1.0);
        stack = stack.push(
            Positioned::new()
                .left(W * 0.5 - w * 0.5)
                .top(base_y + 148.0)
                .width(w.max(2.0))
                .height(6.0)
                .child(Painting::sized(
                    Size::new(w.max(2.0), 6.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        book.rrect(xywh(0.0, 0.0, w.max(2.0), 3.0), 1.5, alpha(tint(VIOLET_SOFT, 0.2), 0.95));
                    }),
                )),
        );
    }

    // The subline — what the light is, in one breath.
    let sub_a = clamp01((t - 0.70) / 0.12);
    if sub_a > 0.01 {
        let rise = (1.0 - ease_out_cubic(sub_a)) * 10.0;
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(750.0 + rise)
                .width(W)
                .height(36.0)
                .child(Opacity::new(sub_a).child(
                    Text::new("the runtime that renders what you imagine")
                        .style(TextStyle::new(24.0).monospace().letter_spacing(3.2).color(alpha(MUTED, 0.95)))
                        .align(TextAlign::Center),
                )),
        );
    }

    // The mark — the studio's own glyph, ghosted behind the spark: the
    // product the spark will become, foreshadowed in the light.
    if promises > 0.3 {
        let mark_a = clamp01((promises - 0.3) / 0.4) * 0.30;
        let mark = Painting::sized(
            Size::new(300.0, 300.0),
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                super::draw_mark(book, 150.0, 150.0, 120.0, VIOLET_SOFT, mark_a);
            }),
        );
        stack = stack.push(
            Positioned::new().left(W * 0.5 - 150.0).top(spark_y - 150.0).width(300.0).height(300.0).child(mark),
        );
    }

    // The promises — the receipt chips, staggered (the Sequence rule).
    stack = stack.push(super::chip_row(
        &[
            ("60 fps", VIOLET_SOFT),
            ("its own rasterizer", CYAN_SOFT),
            ("byte-identical", MINT),
            ("nothing added in post", CYAN),
        ],
        W * 0.5 - 480.0,
        848.0,
        promises,
    ));

    // The captions — the reveal's beats.
    stack = stack.push(caption(
        "this is vieww",
        1002.0,
        clamp01((t - 0.34) / 0.12),
    ));
    stack = stack.push(caption(
        "the spark stays — scenes cut, the light doesn't",
        966.0,
        clamp01((t - 0.72) / 0.12),
    ));

    // The dive's beginning — the frame brightens at the very bottom edge
    // of the scene, handing S05 its entry speed.
    if t > 0.9 {
        let dive_a = clamp01((t - 0.9) / 0.1) * 0.16;
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                // A violet horizon rising from the bottom — the machine
                // beneath, glowing up through the floor.
                book.rect(
                    xywh(0.0, s.height - 180.0, s.width, 180.0),
                    vieww_foundation::Gradient::vertical().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.0)),
                        (1.0, alpha(VIOLET, dive_a)),
                    ]),
                );
            }),
        )));
    }

    let _ = (Offset::new(0.0, 0.0), H, sec);

    stack.into()
}
