//! S02 · THE WAIT — the old world answers the question with a terminal.
//! 0:09–0:19.
//!
//! A dependency avalanche, streaming in the machine's own voice: the
//! receipts are the reference script's own (quoted, never invented) —
//! a 40.2 MB engine "for breakfast", 1,847 lines of C++ bindings, a
//! 1.1 GB system framework, **41.7 MB before your first pixel**. The
//! progress bar and the spinner run at the old world's cadence: a real
//! 24 Hz hold sampled inside the 60 Hz render (the judder is the honest
//! 2-3-2-3 pattern, not a slowed clock). Then the bar reaches 99 and
//! stops. And stays.
//!
//! The palette holds the old world's register — terminal green, amber
//! warnings, red totals — until S03 breaks it and S04 replaces it.

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use super::{alpha, caption, clamp01, mix, mono_w, tint, xywh, Ctx, AMBER, MUTED, RED, W};
use crate::film_lib::ease_out_cubic;

/// The terminal's inner origin.
const TX: f32 = 300.0;
const TY: f32 = 248.0;
/// The terminal's line height.
const LH: f32 = 30.0;
/// How many lines fit before the scroll.
const MAX_LINES: usize = 9;

/// One dependency line: text, tone, and the film-second it lands.
fn dep_lines() -> Vec<(&'static str, Color, f32)> {
    let _green = alpha(Color::rgb(63, 185, 80), 0.9);
    vec![
        ("$ create-app everything", Color::rgb(230, 232, 234), 0.5),
        ("resolving dependencies…", alpha(MUTED, 0.85), 1.15),
        ("heavy-js-engine        40.2 MB", AMBER, 1.9),
        ("cxx-bindings       1,847 lines", AMBER, 2.6),
        ("layout-compat-shim", alpha(MUTED, 0.7), 3.3),
        ("gesture-polyfill", alpha(MUTED, 0.7), 3.9),
        ("legacy-ui-kit", alpha(MUTED, 0.7), 4.4),
        ("sys-framework         1.1 GB", AMBER, 5.0),
        ("fetching…", alpha(MUTED, 0.7), 5.7),
        ("compiling the world…", alpha(MUTED, 0.85), 6.3),
        ("41.7 MB before your first pixel", tint(RED, 0.25), 7.2),
    ]
}

/// The old world's cadence, as a clock: film seconds quantized to the
/// 24 Hz hold — the honest 2-3-2-3 pattern, never a slowed render.
fn held24(sec: f32) -> f32 {
    (sec * 24.0).floor() / 24.0
}

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let frame_i = (ctx.abs * 60.0) as u64;

    // Everything that moves in the old world moves on the held clock.
    let held = held24(sec);

    // The terminal panel — dark glass, a red warning floor.
    let panel_x = 260.0;
    let panel_y = 170.0;
    let panel_w = 1400.0;
    let panel_h = 660.0;

    let mut stack = Stack::new().push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            // The old world's air: a colder, flatter ground, no stars.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(11, 11, 12)),
                    (0.7, Color::rgb(9, 10, 10)),
                    (1.0, Color::rgb(13, 11, 11)),
                ]),
            );
            // The panel's shadow pool.
            super::glow(
                book,
                panel_x + panel_w * 0.5,
                panel_y + panel_h + 30.0,
                500.0,
                RED,
                0.05,
            );
            super::vignette(book, w, h, 0.55);
            super::scanbands(book, w, h, t, 0.35, Color::rgb(63, 185, 80));
            super::grain(book, w, h, frame_i, 0.5);
        }),
    )));

    // The panel body.
    stack = stack.push(
        Positioned::new()
            .left(panel_x)
            .top(panel_y)
            .width(panel_w)
            .height(panel_h)
            .child(
                Container::new()
                    .color(alpha(Color::rgb(13, 14, 13), 0.97))
                    .radius(16.0)
                    .border(vieww_foundation::Border::new(alpha(AMBER, 0.25), 1.2)),
            ),
    );

    // The title bar — three lights and a name.
    let titlebar = Painting::sized(
        Size::new(panel_w, 46.0),
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            // The dots — the old world's macaroni.
            let dots = [RED, AMBER, Color::rgb(63, 185, 80)];
            for (i, c) in dots.iter().enumerate() {
                book.circle(
                    Offset::new(26.0 + i as f32 * 26.0, 23.0),
                    7.0,
                    alpha(*c, 0.85),
                );
            }
            // The bar's underline.
            book.line(
                Offset::new(0.0, 45.5),
                Offset::new(panel_w, 45.5),
                alpha(Color::WHITE, 0.06),
                1.0,
            );
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(panel_x)
            .top(panel_y)
            .width(panel_w)
            .height(46.0)
            .child(titlebar),
    );
    stack = stack.push(
        Positioned::new()
            .left(panel_x + 96.0)
            .top(panel_y + 13.0)
            .width(600.0)
            .height(24.0)
            .child(
                Text::new("terminal — the old world").style(
                    TextStyle::new(15.0)
                        .monospace()
                        .letter_spacing(2.2)
                        .color(alpha(MUTED, 0.8)),
                ),
            ),
    );

    // The streaming lines — landing on their scheduled seconds, scrolling
    // up once the buffer is full. Each arrives with a soft flicker (the
    // CRT's phosphor memory) and stays.
    let lines = dep_lines();
    let visible: Vec<(&'static str, Color, f32, usize)> = lines
        .iter()
        .enumerate()
        .filter(|(_, (_, _, at))| sec >= *at)
        .map(|(i, (text, color, at))| (*text, *color, *at, i))
        .collect();
    let scroll = visible.len().saturating_sub(MAX_LINES);
    for (text, color, at, idx) in visible.iter().take(MAX_LINES) {
        let age = (sec - at).min(0.4) / 0.4;
        let flick = 0.75 + 0.25 * (age * 8.0).sin().abs() * (1.0 - age);
        let a = ease_out_cubic(age) * flick;
        let y = TY + (idx - scroll) as f32 * LH;
        stack = stack.push(
            Positioned::new()
                .left(TX)
                .top(y)
                .width(1300.0)
                .height(26.0)
                .child(
                    Opacity::new(a.max(0.05)).child(
                        Text::new(*text)
                            .style(
                                TextStyle::new(20.0)
                                    .monospace()
                                    .letter_spacing(0.6)
                                    .color(alpha(*color, 0.95)),
                            )
                            .align(TextAlign::Left),
                    ),
                ),
        );
    }

    // The prompt caret — blinking after the last landed line, 24-held.
    if let Some((_, _, _, idx)) = visible.last() {
        let row = (idx - scroll) as f32;
        let y = TY + row * LH;
        let last_w = mono_w(20.0, visible.last().unwrap().0.chars().count());
        let on = (held * 2.2).fract() < 0.55;
        if on {
            stack = stack.push(
                Positioned::new()
                    .left(TX + last_w + 8.0)
                    .top(y + 4.0)
                    .width(11.0)
                    .height(22.0)
                    .child(
                        Container::new()
                            .color(alpha(Color::rgb(63, 185, 80), 0.9))
                            .radius(1.5),
                    ),
            );
        }
    }

    // The size bar — filling toward 41.7 MB as the deps land, at the old
    // world's held cadence. It reaches 99% and stays: the stall that
    // breaks in S03.
    let bar_y = 760.0;
    let bar_x = TX;
    let bar_w = 1300.0;
    // The fill: a quadratic crawl to 99% on the held clock — and there
    // it stays: the stall S03 breaks.
    let bar_p = if held < 6.5 {
        0.99 * (held / 6.5).powi(2).min(1.0)
    } else {
        0.99
    };
    let mb = 41.7 * bar_p;
    let bar = Painting::sized(
        Size::new(bar_w, 26.0),
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            book.stroke_rrect(
                xywh(0.0, 0.0, bar_w, 12.0),
                6.0,
                alpha(Color::WHITE, 0.14),
                1.2,
            );
            let fill_w = (bar_w * bar_p).max(4.0);
            book.rrect(
                xywh(0.0, 0.0, fill_w, 12.0),
                6.0,
                Gradient::horizontal().with_dither().with_stops(&[
                    (0.0, alpha(AMBER, 0.85)),
                    (0.7, alpha(mix(AMBER, RED, 0.55), 0.9)),
                    (1.0, alpha(RED, 0.95)),
                ]),
            );
            // The 99 marker — the stall's headstone.
            let mx = bar_w * 0.99;
            book.line(
                Offset::new(mx, -3.0),
                Offset::new(mx, 15.0),
                alpha(RED, 0.7),
                1.4,
            );
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(bar_x)
            .top(bar_y)
            .width(bar_w)
            .height(26.0)
            .child(bar),
    );
    stack = stack.push(
        Positioned::new()
            .left(bar_x)
            .top(bar_y + 30.0)
            .width(900.0)
            .height(24.0)
            .child(
                Text::new(format!(
                    "{:5.1} MB · compiling the world · {}%",
                    mb,
                    (bar_p * 100.0) as u32
                ))
                .style(
                    TextStyle::new(18.0)
                        .monospace()
                        .letter_spacing(1.0)
                        .color(alpha(tint(RED, 0.25), 0.9)),
                ),
            ),
    );

    // The spinner — an arc at the old world's held 24, top-right of the
    // panel. In the stall it trembles: its sweep shortens and its period
    // quickens, the machine grinding against 99.
    let stall = clamp01((sec - 6.5) / 1.2);
    let spin_period = 1.1 - 0.5 * stall;
    let spin = (held / spin_period).fract();
    let sweep = std::f32::consts::TAU * (0.62 - 0.30 * stall);
    let spinner = Painting::sized(
        Size::new(64.0, 64.0),
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            let ang = spin * std::f32::consts::TAU;
            book.arc(
                Offset::new(32.0, 32.0),
                15.0,
                3.0,
                ang,
                sweep,
                alpha(AMBER, 0.85),
            );
            book.arc(
                Offset::new(32.0, 32.0),
                15.0,
                1.2,
                ang + std::f32::consts::PI,
                sweep * 0.4,
                alpha(MUTED, 0.5),
            );
        }),
    );
    let tremble = stall * (sec * 40.0).sin() * 2.5;
    stack = stack.push(
        Positioned::new()
            .left(panel_x + panel_w - 96.0 + tremble)
            .top(panel_y + 70.0)
            .width(64.0)
            .height(64.0)
            .child(spinner),
    );

    // The captions — the old world's receipts, in order.
    stack = stack.push(super::act_chip(
        "I",
        "THE SPARK",
        clamp01((sec - 0.4) / 0.5),
    ));
    stack = stack.push(caption(
        "the old world's price — 41.7 MB before your first pixel",
        1000.0,
        clamp01((t - 0.28) / 0.12),
    ));
    stack = stack.push(caption(
        "and every pixel judders — a real 24 Hz hold inside 60",
        964.0,
        clamp01((t - 0.62) / 0.12),
    ));

    // The receipt chip row, bottom-right — the numbers' provenance.
    stack = stack.push(super::chip_row(
        &[
            ("engine 40.2 MB", AMBER),
            ("bindings 1,847 lines", AMBER),
            ("sys-framework 1.1 GB", AMBER),
        ],
        W - 640.0,
        928.0,
        clamp01((t - 0.70) / 0.16),
    ));

    stack.into()
}
