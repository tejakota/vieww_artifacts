//! S04 · THE REVEAL — the title beat, Act I's close. 0:28–0:39.
//!
//! The spark breathes, then blooms: one shockwave ring turns the cage to
//! dust, the palette floods back, and — for the first time in the film —
//! the motion runs at the product's own 60 fps cadence, which is the point:
//! the judder of S01–S02 was the old world's, and this cut is the first
//! demo. The wordmark writes itself under a sweeping light; the underline
//! springs past its rest and settles; **this is vieww**.
//!
//! K1: the wait is obsolete.

use vieww_foundation::{
    Color, FontWeight, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle,
};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, ease_out_cubic, mix, spring_out, tint, xywh, Rng, CYAN_SOFT, INK,
    MUTED, VIOLET, VIOLET_SOFT,
};

use super::Ctx;

/// The bloom's origin (S03's hub).
const CX: f32 = 960.0;
const CY: f32 = 520.0;
/// When the bloom fires (scene fraction).
const BLOOM_T: f32 = 0.16;
/// The wordmark's rest top.
const MARK_TOP: f32 = 400.0;
const MARK_SIZE: f32 = 176.0;

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = Stack::new();

    // The bloom's phases: breathe (0 → BLOOM_T), shockwave + dust (after).
    let breathe = clamp01(t / BLOOM_T);
    let since_bloom = (t - BLOOM_T).max(0.0);

    // The ground — the film's own palette arrives with the bloom.
    let palette_in = ease_out_cubic(clamp01(since_bloom / 0.22));
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (
                        0.0,
                        mix(Color::rgb(7, 7, 9), Color::rgb(9, 9, 13), palette_in),
                    ),
                    (
                        0.6,
                        mix(Color::rgb(6, 6, 9), crate::film_lib::BG_DEEP, palette_in),
                    ),
                    (
                        1.0,
                        mix(Color::rgb(8, 7, 10), Color::rgb(12, 11, 18), palette_in),
                    ),
                ]),
            );
            let star_a = palette_in;
            super::stars(book, w, h, 0x4B10, 110, t, 0.11 * star_a + 0.02);
            // The horizon glow — violet, then warm as the mark lands.
            super::glow(
                book,
                CX,
                h * 0.80,
                w * 0.34,
                VIOLET,
                0.10 + 0.10 * palette_in,
            );
            super::vignette(book, w, h, 0.50 - 0.08 * palette_in);
        }),
    )));

    // The breathing spark, pre-bloom.
    if t < BLOOM_T {
        let pulse = 0.5 + 0.5 * (sec * 6.0).sin();
        let grow = ease_in_out(breathe);
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let r = 4.0 + grow * 26.0 + pulse * 5.0;
                super::glow(
                    book,
                    CX,
                    CY,
                    140.0 + grow * 420.0,
                    VIOLET,
                    0.30 + 0.4 * grow * pulse,
                );
                book.circle(Offset::new(CX, CY), r, alpha(tint(VIOLET_SOFT, 0.55), 1.0));
                book.circle(Offset::new(CX, CY), r * 0.45, alpha(Color::WHITE, 0.9));
            }),
        )));
    }

    // The shockwave + the cage's dust, post-bloom.
    if since_bloom > 0.0 {
        let wave = clamp01(since_bloom / 0.5);
        let dust_a = (1.0 - wave).max(0.0);
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                // Two rings — the hard edge and its echo.
                let r1 = 60.0 + wave * 1250.0;
                let r2 = r1 * 0.72;
                book.ring(
                    Offset::new(CX, CY),
                    r1,
                    3.4 * (1.0 - wave) + 0.8,
                    alpha(tint(VIOLET_SOFT, 0.3), 0.85 * (1.0 - wave)),
                );
                book.ring(
                    Offset::new(CX, CY),
                    r2,
                    1.8,
                    alpha(CYAN_SOFT, 0.5 * (1.0 - wave)),
                );
                // The bars' dust — 220 motes blown outward on the wave.
                let mut rng = Rng::new(0xB100);
                for _ in 0..220 {
                    let ang = rng.f01() * std::f32::consts::TAU;
                    let r0 = 260.0 + rng.f01() * 260.0;
                    let speed = 0.6 + rng.f01() * 1.8;
                    let dist = r0 + wave * 700.0 * speed;
                    let fall = wave * wave * 160.0 * rng.f01();
                    let size = 0.8 + rng.f01() * 2.4;
                    let c = if rng.f01() > 0.7 {
                        CYAN_SOFT
                    } else {
                        VIOLET_SOFT
                    };
                    book.circle(
                        Offset::new(CX + ang.cos() * dist, CY + ang.sin() * dist * 0.7 + fall),
                        size,
                        alpha(c, dust_a * (0.25 + rng.f01() * 0.5)),
                    );
                }
                // Volumetric shafts — the bloom's light, Plus-blended.
                book.layer(1.0, 30.0, None, |g| {
                    for i in 0..7 {
                        let ang = -std::f32::consts::FRAC_PI_2 + (i as f32 - 3.0) * 0.38;
                        let len = 900.0 * (1.0 - wave * 0.4);
                        let tip = Offset::new(CX + ang.cos() * len, CY + ang.sin() * len);
                        let mut beam = vieww_foundation::Path::new();
                        let perp = Offset::new(-ang.sin(), ang.cos());
                        beam.move_to(Offset::new(CX + perp.dx * 26.0, CY + perp.dy * 26.0));
                        beam.line_to(tip);
                        beam.line_to(Offset::new(CX - perp.dx * 26.0, CY - perp.dy * 26.0));
                        beam.close();
                        g.fill(
                            beam,
                            Gradient::linear(Offset::new(CX, CY), tip)
                                .with_dither()
                                .with_stops(&[
                                    (0.0, alpha(VIOLET, 0.20 * (1.0 - wave * 0.8))),
                                    (1.0, alpha(VIOLET, 0.0)),
                                ]),
                        );
                    }
                });
            }),
        )));
    }

    // The wordmark — springs up out of the bloom, revealed by a sweep of
    // light crossing left→right; the glint rides the reveal edge.
    let mark_s = spring_out(clamp01((t - (BLOOM_T + 0.10)) / 0.42), 6.4, 0.55);
    if mark_s > 0.0 {
        let y = MARK_TOP - 300.0 * (1.0 - mark_s);
        let vel = (mark_s - 1.0).abs().min(0.22);
        let squash = 1.0 - vel * 0.5;
        let box_h = 220.0 * squash;
        let box_w = 1080.0 * (1.0 + vel * 0.25);
        // The reveal sweep — the light line crossing the mark.
        let reveal = ease_in_out(clamp01((t - (BLOOM_T + 0.14)) / 0.30));
        let sweep_x = (CX - box_w * 0.5) + box_w * reveal;

        // The ground shadow — firms as the mark nears.
        let near = clamp01(1.0 - (MARK_TOP - y) / 300.0);
        stack = stack.push(
            Positioned::new()
                .left(CX - box_w * 0.5)
                .top(726.0)
                .width(box_w)
                .height(80.0)
                .child(Painting::sized(
                    Size::new(box_w, 80.0),
                    PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                        book.circle(
                            Offset::new(box_w * 0.5, 30.0),
                            120.0 + 240.0 * near,
                            Gradient::radial_fill().with_dither().with_stops(&[
                                (0.0, alpha(Color::BLACK, 0.30 * near + 0.05)),
                                (1.0, alpha(Color::BLACK, 0.0)),
                            ]),
                        );
                    }),
                )),
        );

        // The mark — revealed by brightness, not clipping: a soft alpha ramp
        // that rises with the sweep, like a scan developing the word.
        stack = stack.push(
            Positioned::new()
                .left(CX - box_w * 0.5)
                .top(y)
                .width(box_w)
                .height(box_h.max(1.0))
                .child(
                    Text::new("vieww")
                        .style(
                            TextStyle::new(MARK_SIZE)
                                .weight(FontWeight::Regular)
                                .letter_spacing(11.0)
                                .color(alpha(INK, 0.10 + 0.90 * reveal)),
                        )
                        .align(TextAlign::Center),
                ),
        );
        // The sweep bar — a bright hairline with bloom, riding the reveal.
        if reveal > 0.01 && reveal < 0.995 {
            stack = stack.push(
                Positioned::new()
                    .left(sweep_x - 2.0)
                    .top(y - 30.0)
                    .width(4.0)
                    .height(box_h + 60.0)
                    .child(Painting::sized(
                        Size::new(4.0, box_h + 60.0),
                        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                            book.rrect(
                                xywh(0.0, 0.0, 3.2, s.height),
                                1.6,
                                alpha(tint(VIOLET_SOFT, 0.5), 0.95),
                            );
                        }),
                    )),
            );
        }
    }

    // The underline — overshoots on a stiffer spring.
    let u = spring_out(clamp01((t - (BLOOM_T + 0.26)) / 0.34), 13.5, 0.42);
    if u > 0.001 {
        let rest_w = 780.0;
        let w = rest_w * u;
        stack = stack.push(
            Positioned::new()
                .left(CX - 390.0)
                .top(738.0)
                .width(w.max(2.0) + 4.0)
                .height(12.0)
                .child(Painting::sized(
                    Size::new(w.max(2.0) + 4.0, 12.0),
                    PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                        book.rrect(
                            xywh(0.0, 0.0, w, 5.0),
                            2.5,
                            Gradient::horizontal().with_dither().with_stops(&[
                                (0.0, alpha(VIOLET, 0.95)),
                                (0.55, alpha(VIOLET_SOFT, 0.85)),
                                (1.0, alpha(VIOLET_SOFT, 0.12)),
                            ]),
                        );
                    }),
                )),
        );
    }

    // The declaration — the film's first raised voice.
    let say_a = clamp01((t - (BLOOM_T + 0.34)) / 0.14);
    if say_a > 0.0 {
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(812.0)
                .width(1920.0)
                .height(60.0)
                .child(
                    Opacity::new(say_a).child(
                        Text::new("this is vieww")
                            .style(
                                TextStyle::new(40.0)
                                    .monospace()
                                    .letter_spacing(7.0)
                                    .color(alpha(INK, 0.96)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }
    // The quiet subtitle — what it is.
    let sub_a = clamp01((t - (BLOOM_T + 0.46)) / 0.14);
    if sub_a > 0.0 {
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(872.0)
                .width(1920.0)
                .height(36.0)
                .child(
                    Opacity::new(sub_a).child(
                        Text::new("the ui runtime, written in rust")
                            .style(
                                TextStyle::new(21.0)
                                    .monospace()
                                    .letter_spacing(4.0)
                                    .color(alpha(MUTED, 0.9)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }

    stack = stack.push(super::caption(
        "no engine bundled. no bindings. no truncation.",
        1000.0,
        clamp01((t - (BLOOM_T + 0.52)) / 0.14),
    ));

    stack.into()
}
