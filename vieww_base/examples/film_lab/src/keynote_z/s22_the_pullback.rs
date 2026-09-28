//! S22 · THE PULLBACK — the studio is one buffer on thirty-six crates.
//! 3:12–3:20.
//!
//! The reconciliation's geometry: the camera pulls back and the whole
//! studio — which the film has just lived inside — shrinks to a single
//! luminous card, one buffer among the architecture that made it. The
//! 36-crate city rises behind (the same blocks S06 built, now the
//! ground beneath everything), the spark travels from the studio's
//! preview down into the crates, and the witness line runs beneath it
//! all — the session the studio never lost.
//!
//! This is the claim the release actually makes: **viewwstudio is
//! built on vieww** — an editor and a device-framed preview, riding one
//! renderer, thirty-six crates down.

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{ease_in_out, ease_out_cubic};
use super::{
    ACCENT, CANVAS, CRATES, Ctx, FAINT, INK, MINT, MUTED, SYN_TYPE, VIOLET_SOFT, W, alpha,
    caption, clamp01, glow, ground, mix, session_rail, spark, stars_parallax, tint,
    vignette, xywh,
};

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let abs = ctx.abs;
    let frame_i = (ctx.abs * 60.0) as u64;

    // The pull-back — continuous, decelerating.
    let pull = ease_in_out(clamp01(t / 0.72));

    // The studio card's geometry: starts filling the frame (0.96 scale),
    // recedes to a small glowing card (0.22) among the crates.
    let studio_scale = 0.96 - 0.74 * pull;
    let studio_w = W * 0.62 * studio_scale;
    let studio_h = studio_w * (1080.0 / 1920.0);
    let studio_x = W * 0.5 - studio_w * 0.5;
    let studio_y = 300.0 - 130.0 * pull;

    let mut stack = Stack::new();

    // The ground — the architecture's register returning.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            ground(book, w, h);
            stars_parallax(book, w, h, 0x0A22, 120, t, 0.10, 0.0, -60.0 * pull);
            super::aurora(book, w, h, t * 0.5, 0x0660, 0.4);
            vignette(book, w, h, 0.5);
            super::grain(book, w, h, frame_i, 0.35);
        }),
    )));

    // THE ARCHITECTURE — the 36 crates as a distant glowing city rising
    // behind the receding studio: the same blocks S06 built, dimmed to
    // the background register. They arrive with the pull.
    if pull > 0.05 {
        let city = Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let a = clamp01((pull - 0.05) / 0.5);
                // The city's ground glow.
                glow(book, W * 0.5, 870.0, 800.0, ACCENT, 0.10 * a);
                let cols = 12;
                let rows = 3;
                let gw = W - 460.0;
                let cw = gw / cols as f32;
                for (i, name) in CRATES.iter().enumerate() {
                    let col = i % cols;
                    let row = i / cols;
                    // Only the first 36 (the list IS 36) — three rows.
                    if row >= rows {
                        break;
                    }
                    let x = 230.0 + col as f32 * cw;
                    let hgt = 70.0 + (i % 5) as f32 * 22.0;
                    let y = 870.0 - (row as f32 + 1.0) * (hgt + 14.0) + row as f32 * 30.0;
                    // The block — a dim crate.
                    book.rrect(
                        xywh(x, y, cw - 10.0, hgt),
                        7.0,
                        alpha(mix(Color::rgb(0x16, 0x13, 0x11), ACCENT, 0.10), a * 0.9),
                    );
                    book.stroke_rrect(xywh(x, y, cw - 10.0, hgt), 7.0, alpha(ACCENT, 0.14 + 0.06 * (i % 3) as f32 / 3.0 * a), 1.0);
                    // The crate's lit windows — tiny ticks.
                    for k in 0..3 {
                        book.rrect(
                            xywh(x + 8.0 + k as f32 * 14.0, y + 10.0, 6.0, 4.0),
                            1.5,
                            alpha(tint(SYN_TYPE, 0.1), a * (0.4 + 0.3 * ((i + k) % 3) as f32 / 3.0)),
                        );
                    }
                    let _ = name;
                }
            }),
        );
        stack = stack.push(Positioned::fill().child(city));
    }

    // THE STUDIO CARD — the whole product, receding: the title bar's
    // dots, the sidebar, the editor's lines, the preview's phone — the
    // shape of everything Act III showed, drawn small and held in light.
    {
        let card = Painting::sized(
            Size::new(studio_w.max(60.0), studio_h.max(40.0)),
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let w = studio_w.max(60.0);
                let h = studio_h.max(40.0);
                // The glass body.
                book.rrect(xywh(0.0, 0.0, w, h), 14.0, alpha(Color::rgb(0x14, 0x12, 0x10), 0.97));
                book.stroke_rrect(xywh(0.0, 0.0, w, h), 14.0, alpha(ACCENT, 0.4), 1.6);
                // The title bar dots.
                for (i, c) in [
                    Color::rgb(255, 95, 86),
                    Color::rgb(255, 189, 46),
                    Color::rgb(39, 201, 63),
                ].iter().enumerate() {
                    book.circle(Offset::new(18.0 + i as f32 * 16.0, 16.0), 4.0, alpha(*c, 0.8));
                }
                // The mark — the studio's own glyph.
                let ms = (w * 0.03).max(10.0);
                super::draw_mark(book, w * 0.5, h * 0.5 - h * 0.18, ms, VIOLET_SOFT, 0.9);
                // The sidebar block.
                book.rect(xywh(0.0, 30.0, w * 0.18, h - 30.0), alpha(Color::rgb(0x10, 0x0E, 0x0C), 0.9));
                // The editor's lines.
                for l in 0..6 {
                    book.rrect(
                        xywh(w * 0.22, 44.0 + l as f32 * (h * 0.075), w * 0.5 * (1.0 - l as f32 * 0.09), h * 0.03),
                        3.0,
                        alpha(Color::WHITE, 0.14),
                    );
                }
                // The preview's phone — glowing with the demo's light.
                let ph_w = w * 0.26;
                let ph_h = ph_w * 1.9;
                let ph_x = w - ph_w - w * 0.06;
                let ph_y = h * 0.22;
                book.rrect(xywh(ph_x, ph_y, ph_w, ph_h), 8.0, alpha(Color::rgb(0x0D, 0x0C, 0x0A), 1.0));
                book.stroke_rrect(xywh(ph_x, ph_y, ph_w, ph_h), 8.0, alpha(ACCENT, 0.5), 1.4);
                // The screen's light — the live demo, still alive in
                // miniature.
                glow(book, ph_x + ph_w * 0.5, ph_y + ph_h * 0.5, ph_w * 0.9, ACCENT, 0.5);
                // The status bar.
                book.rect(xywh(0.0, h - 8.0, w, 8.0), alpha(Color::rgb(0x18, 0x16, 0x14), 1.0));
            }),
        );
        stack = stack.push(
            Positioned::new()
                .left(studio_x)
                .top(studio_y)
                .width(studio_w.max(60.0))
                .height(studio_h.max(40.0))
                .child(card),
        );
        // The card's halo — one buffer held in light.
        stack = stack.push(Positioned::fill().child(Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                glow(book, studio_x + studio_w * 0.5, studio_y + studio_h * 0.5, studio_w.max(60.0) * 0.9, ACCENT, 0.22);
            }),
        )));
    }

    // THE SPARK — travels from the studio's preview down into the crates:
    // the product handing its light back to the engine beneath it.
    {
        let x0 = W * 0.62;
        let y0 = studio_y + studio_h * 0.4;
        let x1 = W * 0.5;
        let y1 = 850.0;
        let k = clamp01((t - 0.18) / 0.55);
        let sx = x0 + (x1 - x0) * k;
        let sy = y0 + (y1 - y0) * k;
        let trail = Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                // The trail — the light's path, fading behind it.
                for tk in 0..8 {
                    let tkk = (k - tk as f32 * 0.035).clamp(0.0, 1.0);
                    let tx = x0 + (x1 - x0) * tkk;
                    let ty = y0 + (y1 - y0) * tkk;
                    book.circle(
                        Offset::new(tx, ty),
                        3.0 - tk as f32 * 0.3,
                        alpha(ACCENT, 0.5 * (1.0 - tk as f32 / 8.0)),
                    );
                }
                spark(book, sx, sy, 9.0, abs, 1.0, ACCENT);
            }),
        );
        stack = stack.push(Positioned::fill().child(trail));
    }

    // THE WITNESS LINE — the session rail, running beneath everything.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            session_rail(book, 240.0, W - 240.0, 950.0, abs, 1.0);
        }),
    )));

    // The captions — the reconciliation's beats.
    stack = stack.push(caption(
        "pull back — the studio you just lived in",
        1002.0,
        clamp01((t - 0.06) / 0.10),
    ));
    if t > 0.30 {
        stack = stack.push(caption(
            "viewwstudio is built on vieww — one buffer on thirty-six crates",
            966.0,
            clamp01((t - 0.30) / 0.12),
        ));
    }
    if t > 0.62 {
        stack = stack.push(caption(
            "the light hands itself back to the engine — and the session never lost a beat",
            930.0,
            clamp01((t - 0.62) / 0.12),
        ));
    }

    let _ = (FAINT, INK, MINT, MUTED, SYN_TYPE, Rect::new(0.0, 0.0, 1.0, 1.0), sec);

    stack.into()
}
