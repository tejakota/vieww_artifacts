//! S01 · THE WAIT — Ki's opening. 0:00–0:13.
//!
//! The world as it is: terminals, spinners, progress bars, a clock — the
//! dev-world's texture, rendered (procedural-first, per the risk register:
//! no licensing, no product named). Degraded-*crafted*: grain, dust,
//! defocus, and the **24-in-60 judder** — a 24 Hz clock sampled inside a
//! 60 Hz render; the judder is the mechanism, not a rate change. The
//! poverty of the image is a rendered effect through the same FilterChain
//! grammar the aurora plate proved (E-21) — which is itself the first
//! quiet demo, and the hard cut out of here (into silence and 60 fps) is
//! the argument.
//!
//! Nothing is named. The hour-counter's number is emitted by the overlay's
//! own clock — 1,852 hours and counting, on the wait's held cadence.

use vieww_effects::{Filter, FilterChain};
use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use crate::film_lib::{alpha, clamp01, ease_in_out, held_24_in_60, xywh, INK, MUTED};

use super::{dust, grain, vignette, Ctx};

/// A terminal window of the wait — chrome, log lines, a live cursor.
fn terminal(
    book: &mut Sketchbook,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    tq: f32,
    seed: u64,
    which: usize,
) {
    let mut rng = crate::film_lib::Rng::new(seed);
    // The window body.
    book.rrect(xywh(x, y, w, h), 10.0, alpha(Color::rgb(16, 17, 20), 0.96));
    book.stroke_rrect(xywh(x, y, w, h), 10.0, alpha(Color::WHITE, 0.07), 1.0);
    // Title bar.
    book.rect(
        xywh(x + 1.0, y + 1.0, w - 2.0, 30.0),
        alpha(Color::rgb(22, 23, 27), 0.95),
    );
    for (i, c) in [Color::rgb(70, 72, 78); 3].iter().enumerate() {
        book.circle(Offset::new(x + 20.0 + i as f32 * 17.0, y + 15.0), 4.5, *c);
    }
    // The title — a path, not a product: /var/build or ~/work.
    let _ = which;

    // Log lines — dim text rows with a few highlights, scrolling on the
    // held clock (the wait's own cadence).
    let lines = [
        "resolving workspace metadata",
        "compiling proc-macro v0.3.2",
        "waiting for file lock on package cache",
        "rebuilding 312/847 modules",
        "acquiring semver compatibility graph",
        "linking incremental objects",
    ];
    let scroll = ((tq * 13.0) * 2.4) as i32;
    for i in 0..6 {
        let li = ((i + scroll).rem_euclid(6)) as usize;
        let ly = y + 52.0 + i as f32 * 34.0;
        let bright = li == 3;
        let a = if bright {
            0.42
        } else {
            0.20 + rng.f01() * 0.06
        };
        let _ = a;
        let _ = (ly, lines[li]);
    }
    // (Text is drawn as widget nodes below; the painter keeps the chrome.)
}

/// The hour-counter — the wait's own receipt, ticking on held steps.
fn hour_counter(tq_sec: f32) -> WidgetNode {
    let held = held_24_in_60(tq_sec);
    // The wait has been going on a long time: 1,852 hours, plus this
    // scene's own held minutes — emitted by the clock, not typed.
    let minute = 14 + ((held / 0.6) as i32).min(9);
    let text = format!("1,852 h {:02} m", minute);
    Positioned::new()
        .left(1436.0)
        .top(946.0)
        .width(420.0)
        .height(64.0)
        .child(
            Text::new(text)
                .style(
                    TextStyle::new(44.0)
                        .monospace()
                        .letter_spacing(1.5)
                        .color(alpha(MUTED, 0.85)),
                )
                .align(TextAlign::Right),
        )
        .into()
}

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    // The wait's clock: everything moves on the held 24 Hz step. The
    // judder is the mechanism (E-01's cadence, E-21's crafted poverty).
    let tq = held_24_in_60(ctx.sec);
    let t = tq / 13.0;

    // The world — gray, desaturated, three windows deep.
    let sec = ctx.sec;
    let world = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;
            // The ground — flatter and colder than the film's usual violet.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(13, 13, 15)),
                    (0.7, Color::rgb(9, 9, 11)),
                    (1.0, Color::rgb(11, 11, 13)),
                ]),
            );

            // Window drift — slow, mechanical, on the held clock.
            let d1 = (t * 1.1).sin() * 14.0;
            let d2 = (t * 0.8 + 2.0).sin() * 18.0;
            let d3 = (t * 0.9 + 4.0).sin() * 12.0;

            // The far window — defocused (the wait's eyes).
            book.layer(1.0, 5.5, None, |g| {
                terminal(g, 150.0 + d1, 128.0, 640.0, 380.0, t, 0x51, 0);
            });
            // The mid window.
            book.layer(1.0, 2.2, None, |g| {
                terminal(g, 560.0 + d2, 332.0, 780.0, 430.0, t, 0x52, 1);
            });
            // The near window.
            terminal(book, 1080.0 + d3, 168.0, 640.0, 400.0, t, 0x53, 2);

            // A progress bar — the long one, crawling, resetting.
            let px = 1150.0;
            let py = 640.0;
            let pw = 500.0;
            let prog = (t * 0.55).fract();
            book.rrect(xywh(px, py, pw, 10.0), 5.0, alpha(Color::WHITE, 0.06));
            book.rrect(xywh(px, py, pw * prog, 10.0), 5.0, alpha(MUTED, 0.55));
            book.rect(xywh(px, py + 26.0, pw, 1.0), alpha(Color::WHITE, 0.05));

            // Dust — the wait's air, thick.
            dust(book, w, h, t, 0xD057, 1.0);
            // Grain — per-frame, on the frame index the clock derives.
            let frame_i = (sec * 60.0).round() as u64;
            grain(book, w, h, frame_i, 1.0);
            // The heavy vignette — the world closes in at the edges.
            vignette(book, w, h, 0.62);
        }),
    );

    // E-21's ramp: saturation bleeds, brightness falls — but crafted:
    // this is the *most* degraded the film ever allows itself to look.
    let degraded = FilterChain::new()
        .filter(Filter::saturation(0.42))
        .filter(Filter::brightness(0.82))
        .child(world);

    // The log text — the terminals' words, dim, on the held clock.
    let mut stack = Stack::new().push(Positioned::fill().child(degraded));

    // Terminal 3's log (the near window) — real text, scrolling.
    let scroll = ((t * 13.0) * 1.6) as i32;
    let lines = [
        "resolving workspace metadata",
        "compiling proc-macro v0.3.2",
        "waiting for file lock on package cache",
        "rebuilding 312/847 modules",
        "acquiring semver compatibility graph",
        "linking incremental objects",
    ];
    for i in 0..5 {
        let li = ((i + scroll).rem_euclid(6)) as usize;
        let bright = li == 3;
        let a = if bright { 0.5 } else { 0.26 };
        stack = stack.push(
            Positioned::new()
                .left(1116.0)
                .top(236.0 + i as f32 * 34.0)
                .width(580.0)
                .height(26.0)
                .child(
                    Text::new(lines[li])
                        .style(TextStyle::new(17.0).monospace().color(alpha(INK, a))),
                ),
        );
    }
    // The cursor — a block, blinking on the held cadence.
    let blink_on = (held_24_in_60(ctx.sec) * 2.2).fract() < 0.55;
    if blink_on {
        stack = stack.push(
            Positioned::new()
                .left(1116.0)
                .top(402.0)
                .width(11.0)
                .height(22.0)
                .child(Container::new().color(alpha(MUTED, 0.55)).radius(1.5)),
        );
    }

    // The spinner — a dashed ring, marching. The wait's icon.
    stack = stack.push(
        Positioned::new()
            .left(1130.0)
            .top(500.0)
            .width(72.0)
            .height(72.0)
            .child(Painting::sized(
                Size::new(72.0, 72.0),
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    let phase = tq * 260.0;
                    book.stroke_styled(
                        super::circle_path(36.0, 36.0, 22.0, 40),
                        alpha(MUTED, 0.5),
                        3.0,
                        vieww_foundation::StrokeStyle::rounded()
                            .dash(vieww_foundation::Dash::even(9.0).offset(phase)),
                    );
                }),
            )),
    );

    // The hour-counter — the wait's receipt.
    stack = stack.push(hour_counter(ctx.sec));

    // A fade-up from black — the film's first breath (transition grammar I).
    let fade = ease_in_out(clamp01(ctx.t / 0.035));
    if fade < 1.0 {
        stack = stack.push(
            Positioned::fill().child(
                Opacity::new(1.0 - fade)
                    .child(Container::new().size(1920.0, 1080.0).color(Color::BLACK)),
            ),
        );
    }

    stack.into()
}
