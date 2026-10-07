//! A01 · THE WAIT — the first pain: latency. 0:12–0:26.
//!
//! The old world's register: a terminal in the dark, compiling. The
//! caret blinks; a progress bar crawls toward 2% and never arrives; a
//! spinner turns at the speed of a clock, not of work. The gap line's
//! dashes crawl but the number holds: 45,000 ms — the edit-to-see
//! loop of a stack that has to rebuild, relink and reload to show you
//! one changed line. The wait is not a bug in any one tool; it is the
//! *design* of the distance.
//!
//! The need begins here.

use vieww_foundation::{Color, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;

use super::{
    Ctx, MUTED, TERM_GREEN, W, alpha, caption, clamp01, distance_chip, dust,
    gap_line, grain, ground, pole_caret, pole_screen, progress_rail, tint, vignette,
    xywh,
};
use crate::film_lib::{Rng, ease_out_cubic, held_24_in_60};

/// The terminal's build line, typed once, then the wait.
const BUILD_LINE: &str = "cargo build --release";

/// Where the old world happens — the terminal card's geometry.
const TERM_X: f32 = 660.0;
const TERM_Y: f32 = 640.0;
const TERM_W: f32 = 600.0;
const TERM_H: f32 = 240.0;

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let frame_i = (ctx.abs * 60.0) as u64;

    // The room — colder than the prologue: the pain acts lose the dust's
    // warmth and gain a scanline register.
    let room = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            ground(book, w, h);
            vignette(book, w, h, 0.68);
            // The scan drift — the old world's CRT cadence.
            let band_h = 90.0;
            let drift = (t * 110.0) % (band_h * 2.0);
            let mut y = -band_h * 2.0 + drift;
            let mut i = 0;
            while y < h {
                let fade = (1.0 - (y / h - 0.5).abs() * 1.4).clamp(0.0, 1.0);
                if i % 2 == 0 {
                    book.rect(xywh(0.0, y, w, band_h * 0.55), alpha(Color::BLACK, 0.30 * fade));
                } else {
                    book.rect(xywh(0.0, y, w, band_h * 0.35), alpha(TERM_GREEN, 0.04 * fade));
                }
                y += band_h;
                i += 1;
            }
            grain(book, w, h, frame_i, 0.5);
            dust(book, w, h, t, 0x0FF1, 0.5);
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(room));

    // The poles — still there, still far; the screen dims to sleeping.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            gap_line(book, 430.0, 560.0, 1360.0, 0.0, sec, 26.0, 0.0, MUTED, 0.9);
            pole_caret(book, 560.0, 430.0, sec, 1.0);
            pole_screen(book, 1360.0, 430.0, 0.0, 0.85, tint(MUTED, 0.1));
        }),
    )));

    // The terminal — the old world's window, assembling.
    let term_a = ease_out_cubic(clamp01((t - 0.04) / 0.16));
    if term_a > 0.01 {
        stack = stack.push(
            Positioned::new()
                .left(TERM_X)
                .top(TERM_Y)
                .width(TERM_W)
                .height(TERM_H)
                .child(
                    super::Opacity::new(term_a).child(Painting::sized(
                        Size::new(TERM_W, TERM_H),
                        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                            // The terminal card: a dark surface, a green rim.
                            book.rrect(xywh(0.0, 0.0, TERM_W, TERM_H), 12.0, alpha(Color::rgb(0x14, 0x18, 0x12), 0.96));
                            book.stroke_rrect(xywh(0.0, 0.0, TERM_W, TERM_H), 12.0, alpha(TERM_GREEN, 0.25), 1.4);
                            // The traffic lights — the old world is a GUI too.
                            for (i, c) in [
                                Color::rgb(0xE0, 0x6C, 0x60),
                                Color::rgb(0xE0, 0xA8, 0x4E),
                                Color::rgb(0x5C, 0xB8, 0x60),
                            ]
                            .iter()
                            .enumerate()
                            {
                                book.circle(Offset::new(26.0 + i as f32 * 26.0, 24.0), 6.5, alpha(*c, 0.8));
                            }
                        }),
                    )),
                ),
        );

        // The typed command + the compiling line + the progress bar.
        let type_p = clamp01((t - 0.10) / 0.14);
        let typed_n = (BUILD_LINE.chars().count() as f32 * ease_out_cubic(type_p)).round() as usize;
        let shown: String = BUILD_LINE.chars().take(typed_n).collect();
        if typed_n > 0 {
            stack = stack.push(
                Positioned::new()
                    .left(TERM_X + 24.0)
                    .top(TERM_Y + 56.0)
                    .width(TERM_W - 48.0)
                    .height(26.0)
                    .child(
                        Text::new(shown)
                            .style(
                                TextStyle::new(16.0)
                                    .monospace()
                                    .letter_spacing(1.0)
                                    .color(alpha(tint(TERM_GREEN, 0.25), 0.95)),
                            )
                            .align(TextAlign::Left),
                    ),
            );
        }

        // "Compiling..." — the word the whole act is about, after the line.
        let comp_a = clamp01((t - 0.30) / 0.10);
        if comp_a > 0.01 {
            stack = stack.push(
                Positioned::new()
                    .left(TERM_X + 24.0)
                    .top(TERM_Y + 92.0)
                    .width(TERM_W - 48.0)
                    .height(24.0)
                    .child(
                        super::Opacity::new(comp_a).child(
                            Text::new("   Compiling the app v0.3.0 (1,247 crates)")
                                .style(
                                    TextStyle::new(14.0)
                                        .monospace()
                                        .color(alpha(MUTED, 0.8)),
                                )
                                .align(TextAlign::Left),
                        ),
                    ),
            );
        }

        // The progress bar — crawling, never arriving. Held at 2%: the
        // honest shape of a rebuild's first seconds.
        let bar_a = clamp01((t - 0.38) / 0.12);
        if bar_a > 0.01 {
            stack = stack.push(
                Positioned::new()
                    .left(TERM_X + 24.0)
                    .top(TERM_Y + 140.0)
                    .width(TERM_W - 48.0)
                    .height(16.0)
                    .child(
                        super::Opacity::new(bar_a).child(Painting::sized(
                            Size::new(TERM_W - 48.0, 16.0),
                            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                                let bw = TERM_W - 48.0;
                                // 24-in-60 hold judder — the wait's cadence.
                                let judd = held_24_in_60(sec);
                                let creep = 0.5 + 0.5 * (judd * 0.4).sin();
                                let frac = 0.02 + creep * 0.004;
                                book.stroke_rrect(xywh(0.0, 2.0, bw, 10.0), 5.0, alpha(Color::WHITE, 0.12), 1.0);
                                book.rrect(xywh(2.0, 4.0, (bw - 4.0) * frac, 6.0), 3.0, alpha(TERM_GREEN, 0.75));
                            }),
                        )),
                    ),
            );
        }

        // The spinner — turning at a clock's speed, not a workload's.
        if bar_a > 0.01 {
            stack = stack.push(
                Positioned::new()
                    .left(TERM_X + 24.0)
                    .top(TERM_Y + 172.0)
                    .width(40.0)
                    .height(40.0)
                    .child(Painting::sized(
                        Size::new(40.0, 40.0),
                        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                            let spin = held_24_in_60(sec) * 2.4;
                            for i in 0..10 {
                                let ang = spin + i as f32 / 10.0 * std::f32::consts::TAU;
                                let a = (i as f32 / 10.0).powi(2);
                                book.line(
                                    Offset::new(20.0 + (ang - 0.22).cos() * 11.0, 20.0 + (ang - 0.22).sin() * 11.0),
                                    Offset::new(20.0 + (ang + 0.22).cos() * 11.0, 20.0 + (ang + 0.22).sin() * 11.0),
                                    alpha(TERM_GREEN, a * 0.7),
                                    2.4,
                                );
                            }
                        }),
                    )),
            );
            // The waiting clock — the seconds this single edit has cost.
            let elapsed = (sec * 10.0).round() / 10.0;
            stack = stack.push(
                Positioned::new()
                    .left(TERM_X + 72.0)
                    .top(TERM_Y + 182.0)
                    .width(240.0)
                    .height(24.0)
                    .child(
                        Text::new(format!("waiting… {elapsed:.1}s"))
                            .style(TextStyle::new(14.0).monospace().color(alpha(MUTED, 0.75)))
                            .align(TextAlign::Left),
                    ),
            );
        }
    }

    // The "meanwhile" clock tower top-right — the user's own clock,
    // ticking at the top of frame while the build ticks at the bottom.
    let user_a = clamp01((t - 0.5) / 0.2);
    if user_a > 0.01 {
        let total_wait = 6.0 + sec;
        stack = stack.push(
            Positioned::new()
                .left(W - 460.0)
                .top(120.0)
                .width(300.0)
                .height(90.0)
                .child(
                    super::Opacity::new(user_a).child(Painting::sized(
                        Size::new(300.0, 90.0),
                        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                            // The clock face.
                            let cx = 150.0;
                            let cy = 45.0;
                            book.circle(Offset::new(cx, cy), 26.0, alpha(Color::BLACK, 0.35));
                            book.ring(Offset::new(cx, cy), 26.0, 1.6, alpha(MUTED, 0.5));
                            // The second hand, sweeping with a terminal tick.
                            let tick = (sec.floor() / 60.0) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                            book.line(
                                Offset::new(cx, cy),
                                Offset::new(cx + tick.cos() * 20.0, cy + tick.sin() * 20.0),
                                alpha(TERM_GREEN, 0.8),
                                2.0,
                            );
                            // The dots — one per wasted second, accumulating.
                            let dots = (sec as usize).min(14);
                            let mut rng = Rng::new(0xBEA7);
                            for _ in 0..dots {
                                let dx = cx + rng.sym() * 60.0;
                                let dy = cy + rng.sym() * 26.0;
                                book.circle(Offset::new(dx, dy), 1.6, alpha(tint(TERM_GREEN, 0.2), 0.5));
                            }
                            let _ = total_wait;
                        }),
                    )),
                ),
        );
    }

    // The captions — the pain, named plainly.
    stack = stack.push(super::act_chip("MOVEMENT I", "THE FAR", clamp01((t - 0.04) / 0.10)));
    stack = stack.push(caption(
        "edit. wait. rebuild. wait. — the loop the old world gave you",
        1002.0,
        clamp01((t - 0.06) / 0.12),
    ));
    stack = stack.push(caption(
        "the distance is not a bug. it is the design",
        966.0,
        clamp01((t - 0.46) / 0.12),
    ));

    stack = stack.push(distance_chip(ctx.abs, clamp01(t / 0.1)));
    stack = stack.push(progress_rail(ctx.abs));

    stack.into()
}
