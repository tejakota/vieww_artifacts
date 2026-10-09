//! D02 · THE PULLBACK — the studio in its constellation. 4:06–4:18.
//!
//! The camera pulls back: the studio window shrinks from full-bleed
//! toward one card in a field of cards — each card a screen drawn by
//! the same engine, each a device frame with a *different* kind of app
//! in it. No names are claimed (the film quotes only what it can): the
//! caption is the true one — *the studio is the largest application
//! built with the framework it edits. So is this page. So is this
//! film.* The constellation breathes; the studio card glows purple —
//! first among equals by size, not by kind.
//!
//! The engine act's claim, one last time, as geography.

use vieww_foundation::{Color, Offset, Sketchbook};
use vieww_widget::prelude::*;

use super::{
    alpha, caption, clamp01, distance_chip, glow, grain, ground, progress_rail, stars_deep,
    vignette, xywh, Ctx, ACCENT, INK, LEDGER, MUTED, W,
};
use crate::film_lib::{ease_out_cubic, Rng};

/// The pullback — 0 = the studio fills the frame, 1 = the constellation.
fn pullback(t: f32) -> f32 {
    ease_out_cubic(clamp01((t - 0.08) / 0.5))
}

/// One constellation card: a device frame with a tiny app inside —
/// abstract content, drawn generically (the film claims no names).
#[allow(clippy::too_many_arguments)]
fn card(
    book: &mut Sketchbook,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    seed: u64,
    t: f32,
    a: f32,
    color: Color,
) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.01 {
        return;
    }
    book.rrect(xywh(x, y, w, h), w * 0.09, alpha(super::SURFACE, 0.9 * a));
    book.stroke_rrect(xywh(x, y, w, h), w * 0.09, alpha(color, 0.35 * a), 1.3);
    // The tiny app — deterministic content rows, each card its own.
    let mut rng = Rng::new(seed);
    let pad = w * 0.12;
    let rows = (h / (w * 0.22)).floor() as usize;
    for r in 0..rows.min(5) {
        let ry = y + pad + r as f32 * w * 0.22;
        let rw = (w - pad * 2.0) * (0.4 + rng.f01() * 0.6);
        let breathe = 0.5 + 0.5 * (t * 1.2 + r as f32 + rng.f01() * 3.0).sin();
        book.rrect(
            xywh(x + pad, ry, rw, w * 0.1),
            w * 0.03,
            alpha(color, 0.22 * a * breathe),
        );
    }
    // The card's glow.
    glow(book, x + w * 0.5, y + h * 0.5, w * 0.9, color, 0.05 * a);
}

pub(super) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let frame_i = (ctx.abs * 60.0) as u64;
    let pull = pullback(t);
    // The camera's pan, for the sky to work against — the pullback is the
    // film's one real scale move, and a flat sky would give it away.
    let pan = ctx.parallax(1.0);

    let room = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            ground(book, w, h);
            // The constellation's sky — arriving with the pullback.
            stars_deep(book, w, h, 0x1115, 110, t, 0.12 * pull, pan);
            vignette(book, w, h, 0.5);
            grain(book, w, h, frame_i, 0.3);

            // The studio card — full-bleed at pull=0, one card at pull=1.
            // It keeps the studio's own proportions (wide window).
            let sw = w * (1.0 - 0.82 * pull);
            let sh = h * (1.0 - 0.82 * pull);
            let sx = w * 0.5 - sw * 0.5;
            let sy = h * 0.52 - sh * 0.5;
            // The studio's surface.
            book.rrect(xywh(sx, sy, sw, sh), 18.0, alpha(super::MARK_GROUND, 0.96));
            book.stroke_rrect(xywh(sx, sy, sw, sh), 18.0, alpha(ACCENT, 0.4), 1.6);
            // The studio's own content — abstract: the title bar, the
            // activity rail, the editor, the preview, all as geometry.
            let pad = sw * 0.03;
            // Title bar.
            book.rrect(
                xywh(sx + pad, sy + pad, sw - pad * 2.0, sh * 0.05),
                6.0,
                alpha(Color::rgb(0x1E, 0x21, 0x26), 1.0),
            );
            for (i, c) in [
                Color::rgb(0xE0, 0x6C, 0x60),
                Color::rgb(0xE0, 0xA8, 0x4E),
                Color::rgb(0x5C, 0xB8, 0x60),
            ]
            .iter()
            .enumerate()
            {
                let d = sh * 0.02;
                book.circle(
                    Offset::new(sx + pad * 2.0 + i as f32 * d * 2.2, sy + pad + sh * 0.025),
                    d * 0.5,
                    alpha(*c, 0.85),
                );
            }
            // The activity rail.
            book.rrect(
                xywh(sx + pad, sy + pad * 3.0 + sh * 0.05, sw * 0.035, sh * 0.78),
                6.0,
                alpha(Color::rgb(0x18, 0x1B, 0x20), 1.0),
            );
            // The editor + the preview.
            let body_y = sy + pad * 3.0 + sh * 0.05;
            let body_h = sh * 0.78;
            book.rrect(
                xywh(sx + pad * 3.0 + sw * 0.035, body_y, sw * 0.5, body_h),
                8.0,
                alpha(Color::rgb(0x14, 0x16, 0x1A), 1.0),
            );
            // The preview — the accent panel, with a screen inside.
            let pv_x = sx + pad * 3.0 + sw * 0.035 + sw * 0.5 + pad;
            let pv_w = sw - (pv_x - sx) - pad;
            book.rrect(
                xywh(pv_x, body_y, pv_w, body_h),
                8.0,
                alpha(Color::rgb(0x14, 0x16, 0x1A), 1.0),
            );
            // The preview's device — the phone, purple-lit.
            let pw = pv_w * 0.34;
            let ph = body_h * 0.72;
            let px = pv_x + (pv_w - pw) * 0.5;
            let py = body_y + (body_h - ph) * 0.5;
            book.rrect(
                xywh(px, py, pw, ph),
                pw * 0.18,
                alpha(Color::rgb(0x0A, 0x0A, 0x0C), 1.0),
            );
            book.stroke_rrect(xywh(px, py, pw, ph), pw * 0.18, alpha(ACCENT, 0.7), 1.4);
            glow(book, px + pw * 0.5, py + ph * 0.5, pw * 1.1, ACCENT, 0.16);
            // The phone's content rows.
            for r in 0..4 {
                let ry = py + ph * 0.12 + r as f32 * ph * 0.18;
                let rw = pw * 0.6 * (0.5 + (r % 3) as f32 * 0.25);
                book.rrect(
                    xywh(px + pw * 0.12, ry, rw, ph * 0.07),
                    3.0,
                    alpha(MUTED, 0.35),
                );
            }
            // The editor's lines — mono-ish rows with syntax colours.
            for r in 0..7 {
                let ry = body_y + body_h * 0.08 + r as f32 * body_h * 0.12;
                let cols = [
                    Color::rgb(0xEF, 0xA3, 0xFF),
                    Color::rgb(0x48, 0xD7, 0xFE),
                    Color::rgb(0x59, 0xD3, 0x8C),
                    Color::rgb(0xFF, 0xBB, 0x6D),
                ];
                let c = cols[r % cols.len()];
                let rw = sw * 0.4 * (0.4 + ((r * 31) % 7) as f32 / 10.0);
                book.rrect(
                    xywh(
                        sx + pad * 3.0 + sw * 0.035 + sw * 0.02,
                        ry,
                        rw,
                        body_h * 0.05,
                    ),
                    2.0,
                    alpha(c, 0.5),
                );
            }

            // The constellation — the other cards, arriving with the pull.
            if pull > 0.25 {
                let field_a = (pull - 0.25) / 0.75;
                // Eight cards around the studio — deterministic layout.
                let positions: [(f32, f32, f32, f32, u64, Color); 8] = [
                    (150.0, 180.0, 240.0, 160.0, 0xA1, MUTED),
                    (1540.0, 150.0, 260.0, 170.0, 0xA2, LEDGER),
                    (180.0, 760.0, 250.0, 170.0, 0xA3, LEDGER),
                    (1520.0, 740.0, 270.0, 180.0, 0xA4, MUTED),
                    (480.0, 130.0, 200.0, 140.0, 0xA5, MUTED),
                    (1290.0, 120.0, 190.0, 130.0, 0xA6, LEDGER),
                    (460.0, 830.0, 210.0, 150.0, 0xA7, MUTED),
                    (1300.0, 840.0, 200.0, 140.0, 0xA8, LEDGER),
                ];
                for (x, y, cw, ch, seed, color) in positions {
                    card(book, x, y, cw, ch, seed, sec, field_a, color);
                }
            }
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(room));

    // The captions — the claim, in its true spelling.
    stack = stack.push(super::act_chip("MOVEMENT IV", "THE PROOF", 1.0));
    stack = stack.push(caption(
        "the studio is the largest application built with the framework it edits",
        1002.0,
        clamp01((t - 0.05) / 0.12),
    ));
    stack = stack.push(caption(
        "so is the product page. so is this film — one engine, everywhere",
        966.0,
        clamp01((t - 0.55) / 0.12),
    ));

    stack = stack.push(distance_chip(ctx.abs, clamp01(t / 0.1)));
    stack = stack.push(progress_rail(ctx.abs));

    let _ = INK;
    let _ = W;
    stack.into()
}
