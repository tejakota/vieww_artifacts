//! A04 · THE DRIFT — the fourth pain: the device gap. 0:54–1:10.
//!
//! The design looked right in the window. Then it shipped, and the
//! phone rendered it wrong: the text wraps where the desktop wrapped
//! it differently, the safe area eats the header, the tap target is a
//! different size, the theme inverts. The right pole splits into three
//! device outlines — desktop, phone, tablet — each showing the *same*
//! screen, each showing it *differently*. The gap line frays: one
//! thought, many screens, no single distance any more.
//!
//! The need's peak — and the act's last word: *mockup is not device.*

use vieww_foundation::{Color, Offset, Rect, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;

use super::{
    ACCENT, BREAK_RED, Ctx, GROUND, MUTED, SYN_COMMENT, W, alpha, caption, clamp01,
    distance_chip, gap_line, grain, ground, pole_caret, pole_screen, progress_rail, spring_out,
    tint, vignette, xywh,
};
use crate::film_lib::ease_out_cubic;

/// The same screen, three ways — each device's version of one layout,
/// and how it drifts. (The content is one line: the same words, wrapped
/// three different ways.)
const SCREEN_WORDS: &str = "the same screen";

/// The three devices' geometry: (x, y, w, h, corner, label).
const DEVICES: [(f32, f32, f32, f32, f32, &str); 3] = [
    (700.0, 560.0, 300.0, 190.0, 8.0, "desktop · 1440"),
    (1030.0, 540.0, 150.0, 300.0, 20.0, "phone · 393"),
    (1260.0, 545.0, 210.0, 260.0, 12.0, "tablet · 834"),
];

pub fn build(ctx: &Ctx) -> WidgetNode {
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
            grain(book, w, h, frame_i, 0.45);
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(room));

    // The fraying deepens through the scene as the devices diverge.
    let fray = 0.1 + 0.7 * ease_out_cubic(clamp01((t - 0.25) / 0.5));

    // The poles — the screen pole splits at scene's start.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            gap_line(book, 430.0, 560.0, 1360.0, 0.0, sec, 6.0, fray, BREAK_RED, 0.85);
            pole_caret(book, 560.0, 430.0, sec, 1.0);
            // The original screen pole ghosts away — replaced by three.
            let ghost = 1.0 - clamp01((t - 0.06) / 0.2);
            if ghost > 0.01 {
                pole_screen(book, 1360.0, 430.0, 0.0, ghost * 0.6, MUTED);
            }
        }),
    )));

    // The three devices — each drifts in on its own spring, each draws
    // THE SAME SCREEN, and each gets it wrong in its own way.
    for (di, (x, y, dw, dh, corner, label)) in DEVICES.iter().enumerate() {
        let (x, y, dw, dh, corner, label) = (*x, *y, *dw, *dh, *corner, *label);
        let arrive = spring_out(clamp01((t - 0.08 - di as f32 * 0.12) / 0.6), 8.0, 0.62);
        if arrive <= 0.02 {
            continue;
        }
        let scale = 0.7 + 0.3 * arrive;
        let w2 = dw * scale;
        let h2 = dh * scale;
        let x2 = x - (w2 - dw) * 0.5;
        let y2 = y + (dh - h2) * 0.5;
        let device_a = ease_out_cubic(clamp01((t - 0.08 - di as f32 * 0.12) / 0.3));
        let drift_k = ease_out_cubic(clamp01((t - 0.30 - di as f32 * 0.1) / 0.4));
        // Splay about the frame's centre: the outer devices turn away, the
        // middle one stays nearly square to the lens.
        let from_centre = (x + dw * 0.5 - super::W * 0.5) / (super::W * 0.5);
        let yaw = -from_centre * 0.34 * (0.25 + 0.75 * drift_k);
        let pitch = -0.045 * (0.3 + 0.7 * drift_k);

        // The drift: each device's *content* diverges from the master.
        // Device 0 (desktop): the text wraps short — lines break early.
        // Device 1 (phone): the safe-area notch eats the header.
        // Device 2 (tablet): the layout is stretched and pale.
        stack = stack.push(
            Positioned::new()
                .left(x2)
                .top(y2 - h2 - 40.0)
                .width(w2)
                .height(h2 + 40.0)
                .child(
                    super::Opacity::new(device_a).child(Painting::sized(
                        Size::new(w2, h2 + 40.0),
                        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                            // Standing in space, not lying on the page.
                            //
                            // The splay opens with the drift: square-on
                            // while the three screens still agree, turning
                            // away from each other as they stop agreeing.
                            // The angle *is* the story here, so it is
                            // driven by `drift_k` rather than the clock.
                            super::panel_3d(
                                book,
                                xywh(0.0, 0.0, w2, h2 + 40.0),
                                yaw,
                                pitch,
                                1500.0,
                                1.0,
                                move |book: &mut Sketchbook| {
                                // The device frame.
                                let body = xywh(0.0, 40.0, w2, h2);
                                book.stroke_rrect(body, corner, alpha(MUTED, 0.7), 2.2);
                                // The screen's surface.
                                book.rrect(
                                    xywh(0.0 + 4.0, 40.0 + 4.0, w2 - 8.0, h2 - 8.0),
                                    (corner - 3.0).max(2.0),
                                    alpha(Color::rgb(0x17, 0x13, 0x11), 0.9),
                                );
                                // The header bar — the same screen's chrome.
                                let hdr_h = 26.0;
                                book.rrect(
                                    xywh(6.0, 46.0, w2 - 12.0, hdr_h),
                                    4.0,
                                    alpha(ACCENT, (0.35 - drift_k * 0.2).max(0.12)),
                                );
                                // The same words — wrapped per device, the
                                // drift made visible in the line breaks.
                                let words: Vec<&str> = SCREEN_WORDS.split(' ').collect();
                                let (line1, line2): (&str, &str) = match di {
                                    0 => ("the same", "screen"),          // desktop: early break
                                    1 => ("the same screen", ""),          // phone: one long line, clipped
                                    _ => ("the same screen", ""),           // tablet: stretched spacing
                                };
                                let _ = words;
                                let fs = if di == 2 { 17.0 } else { 13.0 };
                                let ty = 46.0 + hdr_h + 14.0;
                                let ink = alpha(MUTED, 0.85);
                                // Line 1 — as glyph-ish bars (the words'
                                // shapes, not their letters: this is a screen
                                // seen from across the room).
                                let bar_w = |text: &str| fs * 0.55 * text.chars().count() as f32;
                                let spacing = if di == 2 { 14.0 * (1.0 + drift_k) } else { 8.0 };
                                book.rrect(
                                    xywh(12.0, ty, (bar_w(line1) * (if di == 2 { 1.4 } else { 1.0 })).min(w2 - 24.0), 7.0),
                                    3.0,
                                    ink,
                                );
                                if !line2.is_empty() {
                                    book.rrect(
                                        xywh(12.0 + spacing, ty + 14.0, (bar_w(line2) * (if di == 2 { 1.4 } else { 1.0 })).min(w2 - 24.0), 7.0),
                                        3.0,
                                        alpha(MUTED, 0.6),
                                    );
                                }
                                // The phone's notch — the safe area the
                                // design forgot, eating the header.
                                if di == 1 && drift_k > 0.3 {
                                    book.rrect(
                                        xywh(w2 * 0.5 - 26.0, 42.0, 52.0, 12.0),
                                        6.0,
                                        alpha(BREAK_RED, 0.9 * drift_k),
                                    );
                                }
                                // The drift mark — a red rim, deepening.
                                if drift_k > 0.05 {
                                    book.stroke_rrect(
                                        xywh(0.0, 40.0, w2, h2),
                                        corner,
                                        alpha(BREAK_RED, 0.6 * drift_k),
                                        2.0,
                                    );
                                }
                                },
                            );
                        }),
                    )),
                ),
        );
        // The device's label.
        stack = stack.push(
            Positioned::new()
                .left(x2)
                // `y2` is the device's *bottom*, so the label went a whole
                // device-height below the thing it names — two to three
                // hundred pixels adrift, and at three different heights
                // because each device is a different height. It reads as a
                // dropped list rather than three captions.
                .top(y2 + 14.0)
                .width(w2)
                .height(22.0)
                .child(
                    super::Opacity::new(device_a).child(
                        Text::new(label)
                            .style(
                                TextStyle::new(12.5)
                                    .monospace()
                                    .letter_spacing(1.4)
                                    .color(alpha(MUTED, 0.8)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }

    // The verdict chips — the drift, named, right of the devices.
    stack = stack.push(super::chip_row(
        &[
            ("same code", MUTED),
            ("three screens", MUTED),
            ("three truths", BREAK_RED),
        ],
        // Under the devices, not across them. At y=300 this row landed on
        // top of the phone and the tablet — three labels about the screens
        // covering the screens — while the whole lower half of the frame
        // sat empty.
        W * 0.5 - 300.0,
        700.0,
        clamp01((t - 0.62) / 0.3),
    ));

    // The captions.
    stack = stack.push(super::act_chip("MOVEMENT I", "THE FAR", 1.0));
    stack = stack.push(caption(
        "it looked right in the window. it shipped wrong on the phone",
        1002.0,
        clamp01((t - 0.05) / 0.12),
    ));
    stack = stack.push(caption(
        "a mockup is not a device — and neither forgives you",
        966.0,
        clamp01((t - 0.68) / 0.12),
    ));

    stack = stack.push(distance_chip(ctx.abs, clamp01(t / 0.1)));
    stack = stack.push(progress_rail(ctx.abs));

    let _ = Rect::new(0.0, 0.0, 0.0, 0.0);
    let _ = SYN_COMMENT;
    let _ = tint(MUTED, 0.2);
    stack.into()
}
