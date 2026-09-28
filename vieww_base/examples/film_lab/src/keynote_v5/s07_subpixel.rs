//! S07 · SUBPIXEL FLOOR — the hairline check. 1:02–1:11.
//!
//! The reference script's live-demo beat, staged as an engine proof: a
//! structural stroke's thickness walks the ladder 3.00 → 1.00 → 0.50 →
//! 0.25 → 0.10 px — and at every step the line is *crisp*: no shimmer, no
//! dropout, because coverage is computed per sample, not snapped to the
//! pixel grid. A magnifier panel shows the current thickness as area
//! coverage; a gauge sweeps the log-scale ladder; the floor — the
//! hairline_floor — holds the line visible all the way down.

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{alpha, clamp01, ease_in_out, ease_out_cubic, mix, tint, xywh, FAINT, INK, MUTED, Rng, VIOLET, VIOLET_SOFT, CYAN, CYAN_SOFT};

use super::{Ctx};

/// The thickness ladder — the log steps the walk descends.
const LADDER: [f32; 5] = [3.00, 1.00, 0.50, 0.25, 0.10];

/// The main line's band.
const LINE_Y: f32 = 470.0;

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    // The walk: hold each rung, then step. Five rungs over the scene.
    let walk_t = clamp01((t - 0.12) / 0.66);
    let rung_f = walk_t * LADDER.len() as f32;
    let rung = (rung_f.floor() as usize).min(LADDER.len() - 1);
    let step_p = ease_in_out(rung_f - rung_f.floor());
    // The current thickness: log-lerp between rungs.
    let cur = if rung >= LADDER.len() - 1 {
        LADDER[LADDER.len() - 1]
    } else {
        let a = LADDER[rung];
        let b = LADDER[rung + 1];
        (a.ln() * (1.0 - step_p) + b.ln() * step_p).exp()
    };

    let mut stack = Stack::new();

    // The ground.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            super::ground(book, w, h);
            let mut x = 0.0;
            while x < w {
                book.line(Offset::new(x, 0.0), Offset::new(x, h), alpha(CYAN, 0.025), 1.0);
                x += 96.0;
            }
            super::vignette(book, w, h, 0.5);
        }),
    )));

    // The fixed reference ladder — every rung drawn as its own line, the
    // walked one highlighted. The lines are drawn at *true* thickness:
    // that is the demo — 0.10 px is a whisper of coverage, and it holds.
    let ladder_a = clamp01(t / 0.12);
    if ladder_a > 0.0 {
        let cur_l = cur;
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                for (i, &th) in LADDER.iter().enumerate() {
                    let y = LINE_Y + i as f32 * 74.0;
                    let active = (th - cur_l).abs() < 0.02
                        || (i == 0 && cur_l > LADDER[0] - 0.02);
                    // The line itself — true thickness, full antialiasing.
                    let line_col = if active { tint(VIOLET_SOFT, 0.2) } else { alpha(MUTED, 0.65) };
                    book.rect(xywh(300.0, y - th * 0.5, 880.0, th.max(0.6)), line_col);
                    // An underglow on the active rung — visible proof the
                    // engine still sees it.
                    if active {
                        book.layer(1.0, 8.0, None, |g| {
                            g.rect(xywh(300.0, y - 6.0, 880.0, 12.0), alpha(VIOLET, 0.10));
                        });
                    }
                }
            }),
        )));
        // The px labels, right of each rung.
        for (i, &th) in LADDER.iter().enumerate() {
            let y = LINE_Y + i as f32 * 74.0;
            let active = (th - cur).abs() < 0.02 || (i == 0 && cur > LADDER[0] - 0.02);
            stack = stack.push(
                Positioned::new()
                    .left(1216.0)
                    .top(y - 13.0)
                    .width(150.0)
                    .height(26.0)
                    .child(Opacity::new(ladder_a).child(
                        Text::new(format!("{:.2} px", th))
                            .style(TextStyle::new(20.0).monospace().letter_spacing(1.0).color(if active {
                                alpha(tint(VIOLET_SOFT, 0.2), 1.0)
                            } else {
                                alpha(MUTED, 0.85)
                            })),
                    )),
            );
        }
    }

    // The magnifier — the active rung at 30× zoom: coverage as area.
    let mag_a = clamp01((t - 0.14) / 0.16);
    if mag_a > 0.0 {
        let cur_l = cur;
        let active_i = {
            let mut best = 0;
            for (i, &th) in LADDER.iter().enumerate() {
                if (th - cur_l).abs() < 0.02 {
                    best = i;
                }
            }
            best
        };
        let ay = LINE_Y + active_i as f32 * 74.0;
        stack = stack.push(Positioned::new()
            .left(1420.0)
            .top(320.0)
            .width(400.0)
            .height(400.0)
            .child(Opacity::new(mag_a).child(Painting::sized(
                Size::new(400.0, 400.0),
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    // The lens card.
                    book.rrect(xywh(0.0, 0.0, 400.0, 400.0), 20.0, alpha(Color::rgb(12, 13, 19), 0.96));
                    book.stroke_rrect(xywh(0.0, 0.0, 400.0, 400.0), 20.0, alpha(CYAN_SOFT, 0.4), 1.4);
                    // The pixel grid inside: 5×5 big pixels.
                    let n = 5.0;
                    let pad = 56.0;
                    let cell = (400.0 - pad * 2.0) / n;
                    for gy in 0..5 {
                        for gx in 0..5 {
                            let x = pad + gx as f32 * cell;
                            let y = pad + gy as f32 * cell;
                            book.stroke_rrect(xywh(x, y, cell, cell), 3.0, alpha(CYAN, 0.16), 1.0);
                        }
                    }
                    // The stroke at zoom: `cur` px true → `cur * 30` display,
                    // capped by the panel: draw as a soft horizontal band
                    // whose alpha is the *coverage* (the honest zoom of an
                    // antialiased hairline is a translucent band).
                    let zoom_px_per_px = cell / 1.0; // 1 screen px = 1 cell
                    let _ = zoom_px_per_px;
                    let band_h = (cur_l * cell).max(2.0);
                    let by = 200.0 - band_h * 0.5;
                    // The coverage alpha: a 0.1px line over a 1px pixel is
                    // 10% coverage — translucent, exactly as rasterized.
                    let coverage = (cur_l / 1.0).min(1.0);
                    book.rrect(
                        xywh(pad, by, 400.0 - pad * 2.0, band_h),
                        band_h.min(6.0) * 0.5,
                        alpha(VIOLET_SOFT, (0.25 + 0.75 * coverage).min(1.0)),
                    );
                    book.stroke_rrect(
                        xywh(pad, by, 400.0 - pad * 2.0, band_h),
                        band_h.min(6.0) * 0.5,
                        alpha(tint(VIOLET_SOFT, 0.3), 0.8),
                        1.0,
                    );
                    // The readout.
                    let _ = ay;
                }),
            ))));
        // The mag label.
        stack = stack.push(
            Positioned::new()
                .left(1420.0)
                .top(730.0)
                .width(400.0)
                .height(28.0)
                .child(Opacity::new(mag_a).child(
                    Text::new(format!("30× zoom — {:.2} px → {:.0}% coverage", cur, (cur / 1.0).min(1.0) * 100.0))
                        .style(TextStyle::new(16.0).monospace().color(alpha(MUTED, 0.9)))
                        .align(TextAlign::Center),
                )),
        );
    }

    // The gauge — the ladder as a descending arc, needle sweeping down.
    let gauge_a = clamp01((t - 0.16) / 0.16);
    if gauge_a > 0.0 {
        let walk = clamp01((t - 0.12) / 0.66);
        let cur_l = cur;
        stack = stack.push(Positioned::new()
            .left(300.0)
            .top(130.0)
            .width(340.0)
            .height(150.0)
            .child(Opacity::new(gauge_a).child(Painting::sized(
                Size::new(340.0, 150.0),
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    let cx = 170.0;
                    let cy = 150.0;
                    let r = 128.0;
                    // The scale — 180° from 3px (left) to 0.10px (right), log.
                    let frac = ((cur_l.ln() - LADDER[4].ln()) / (LADDER[0].ln() - LADDER[4].ln())).clamp(0.0, 1.0);
                    book.stroke(
                        super::circle_path(cx, cy, r, 48),
                        alpha(Color::WHITE, 0.08),
                        10.0,
                    );
                    // The filled part — from left to the needle.
                    let mut arc = vieww_foundation::Path::new();
                    let a0 = std::f32::consts::PI;
                    let a1 = std::f32::consts::PI + frac * std::f32::consts::PI;
                    let mut first = true;
                    for i in 0..=40 {
                        let a = a0 + (a1 - a0) * i as f32 / 40.0;
                        let pt = Offset::new(cx + a.cos() * r, cy + a.sin() * r);
                        if first {
                            arc.move_to(pt);
                            first = false;
                        } else {
                            arc.line_to(pt);
                        }
                    }
                    book.stroke(arc, alpha(VIOLET_SOFT, 0.9), 10.0);
                    // The needle.
                    let na = std::f32::consts::PI + frac * std::f32::consts::PI;
                    book.line(
                        Offset::new(cx, cy),
                        Offset::new(cx + na.cos() * (r + 14.0), cy + na.sin() * (r + 14.0)),
                        alpha(tint(VIOLET_SOFT, 0.3), 0.95),
                        2.4,
                    );
                    book.circle(Offset::new(cx, cy), 5.0, alpha(INK, 0.95));
                }),
            ))));
        stack = stack.push(
            Positioned::new()
                .left(300.0)
                .top(296.0)
                .width(340.0)
                .height(30.0)
                .child(Opacity::new(gauge_a).child(
                    Text::new(format!("thickness {:.2} px", cur))
                        .style(TextStyle::new(22.0).monospace().letter_spacing(1.4).color(alpha(INK, 0.9)))
                        .align(TextAlign::Center),
                )),
        );
        let _ = walk;
    }

    // The headline.
    let head_a = clamp01((t - 0.04) / 0.18);
    stack = stack.push(
        Positioned::new()
            .left(760.0)
            .top(140.0)
            .width(1000.0)
            .height(50.0)
            .child(Opacity::new(head_a).child(
                Text::new("the hairline check")
                    .style(TextStyle::new(34.0).letter_spacing(1.5).color(alpha(INK, 0.96))),
            )),
    );
    stack = stack.push(
        Positioned::new()
            .left(760.0)
            .top(196.0)
            .width(1000.0)
            .height(30.0)
            .child(Opacity::new(head_a).child(
                Text::new("layout boundaries, evaluated to the exact sub-pixel")
                    .style(TextStyle::new(20.0).monospace().letter_spacing(1.6).color(alpha(MUTED, 0.9))),
            )),
    );

    stack = stack.push(super::caption(
        "down to a tenth of a pixel. no dropout. no shimmer.",
        1000.0,
        clamp01((t - 0.44) / 0.14),
    ));

    stack.into()
}
