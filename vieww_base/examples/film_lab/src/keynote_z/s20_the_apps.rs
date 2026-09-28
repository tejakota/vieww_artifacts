//! S20 · THE APPS — the world tour: real apps, shipped on vieww.
//! 3:02–3:12.
//!
//! Four vignettes of the four apps this repository actually ships —
//! SnapSearch, three, upxcale, vavlt — each drawn as its own card in
//! the film's hand (stylized, labelled, and carrying the app's own
//! receipt: the test counts the README states, nothing more). The
//! receipts: three 47 · upxcale 21 · SnapSearch 38 (47 with clip) ·
//! vavlt's own audit chain. The cards are the film's drawings of the
//! products, not the products' pixels — and the captions say so: this
//! scene is the tour, not the demo.

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{ease_out_back, ease_out_cubic, Rng};
use super::{
    ACCENT, ACCENT_DEEP, CANVAS, Ctx, FAINT, INK, MINT, MUTED, SYN_FUNCTION, SYN_KEYWORD,
    SYN_STRING, SYN_TYPE, VIOLET_SOFT, W, alpha, caption, clamp01, glow, ground, mix, stars,
    stars_parallax, tint, vignette, xywh,
};

/// One app card: name, tagline, test receipt, and a phone vignette.
struct AppCard {
    name: &'static str,
    what: &'static str,
    receipt: &'static str,
    accent: Color,
    /// The vignette's painter, in card-local coordinates (380×540).
    paint: fn(&mut Sketchbook, f32),
}

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let frame_i = (ctx.abs * 60.0) as u64;

    let cards = [
        AppCard {
            name: "SnapSearch",
            what: "find a photo by describing it",
            receipt: "38 tests · 47 with clip",
            accent: SYN_TYPE,
            paint: paint_search,
        },
        AppCard {
            name: "three",
            what: "temporal 3D capture",
            receipt: "47 tests",
            accent: ACCENT,
            paint: paint_three,
        },
        AppCard {
            name: "upxcale",
            what: "lanczos-3 photo upscaler",
            receipt: "21 tests",
            accent: SYN_STRING,
            paint: paint_upxcale,
        },
        AppCard {
            name: "vavlt",
            what: "consent-first vault",
            receipt: "the audit chain",
            accent: SYN_FUNCTION,
            paint: paint_vavlt,
        },
    ];

    let mut stack = Stack::new();

    // The ground — the world's register, gently moving (the tour's air).
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            ground(book, w, h);
            stars_parallax(book, w, h, 0x2070, 110, t, 0.11, 0.0, 30.0 * t);
            super::aurora(book, w, h, t * 0.6, 0x2020, 0.45);
            vignette(book, w, h, 0.5);
            super::grain(book, w, h, frame_i, 0.35);
        }),
    )));

    // The cards — four, staggered arrivals with a spring settle.
    let card_w = 380.0;
    let card_h = 540.0;
    let gap = (W - 4.0 * card_w) / 5.0;
    for (i, card) in cards.iter().enumerate() {
        let arrive = clamp01((t - 0.06 - i as f32 * 0.08) / 0.22);
        if arrive <= 0.01 {
            continue;
        }
        let settle = ease_out_back(arrive);
        let x = gap + i as f32 * (card_w + gap);
        let y = 250.0 + (1.0 - settle) * 40.0;

        // The card body — dark glass with the app's accent floor.
        let body = Painting::sized(
            Size::new(card_w, card_h),
            PaintWith::new({
                let accent = card.accent;
                let paint = card.paint;
                move |book: &mut Sketchbook, _s: Size| {
                book.rrect(xywh(0.0, 0.0, card_w, card_h), 18.0, alpha(Color::rgb(0x1B, 0x17, 0x15), 0.96));
                book.rrect(
                    xywh(0.0, card_h - 60.0, card_w, 60.0),
                    18.0,
                    Gradient::vertical().with_dither().with_stops(&[
                        (0.0, alpha(mix(Color::rgb(0x1B, 0x17, 0x15), accent, 0.0), 0.0)),
                        (1.0, alpha(mix(Color::rgb(0x1B, 0x17, 0x15), accent, 0.35), 0.96)),
                    ]),
                );
                book.rect(xywh(0.0, card_h - 70.0, card_w, 12.0), alpha(mix(Color::rgb(0x1B, 0x17, 0x15), accent, 0.35), 0.96));
                book.stroke_rrect(xywh(0.0, 0.0, card_w, card_h), 18.0, alpha(accent, 0.30), 1.2);
                // The phone frame inside the card — the app's home.
                let px = 55.0;
                let py = 66.0;
                let pw = card_w - 110.0;
                let ph = 300.0;
                book.rrect(xywh(px, py, pw, ph), 20.0, alpha(Color::rgb(0x12, 0x11, 0x0F), 0.98));
                book.stroke_rrect(xywh(px, py, pw, ph), 20.0, alpha(Color::WHITE, 0.14), 1.4);
                // The notch.
                book.rrect(xywh(px + pw * 0.5 - 30.0, py + 8.0, 60.0, 12.0), 6.0, alpha(Color::rgb(0x12, 0x11, 0x0F), 1.0));
                // The app's vignette, clipped by the frame's rect (drawn
                // in card-local space, inside the phone).
                paint(book, t);
                }
            }),
        );
        stack = stack.push(
            Positioned::new()
                .left(x)
                .top(y)
                .width(card_w)
                .height(card_h)
                .child(Opacity::new(ease_out_cubic(arrive)).child(body)),
        );

        // The card's labels — name, what it is, the receipt.
        for (text, size, color, ly) in [
            (card.name, 24.0, alpha(INK, 0.96), 396.0),
            (card.what, 13.5, alpha(MUTED, 0.9), 430.0),
            (card.receipt, 12.5, alpha(tint(card.accent, 0.2), 0.95), 486.0),
        ] {
            stack = stack.push(
                Positioned::new()
                    .left(x)
                    .top(y + ly)
                    .width(card_w)
                    .height(30.0)
                    .child(
                        Text::new(text)
                            .style(super::geist_mono(size).letter_spacing(1.4).color(color))
                            .align(TextAlign::Center),
                    ),
            );
        }
    }

    // The captions — the tour's beats.
    stack = stack.push(super::act_chip("IV", "THE SHIP", clamp01((sec - 0.3) / 0.5)));
    stack = stack.push(caption(
        "and the world ships — four apps, four stories, one runtime",
        1002.0,
        clamp01((t - 0.16) / 0.12),
    ));
    stack = stack.push(caption(
        "the cards are the film's drawings; the receipts are theirs",
        966.0,
        clamp01((t - 0.55) / 0.12),
    ));

    let _ = (FAINT, SYN_KEYWORD, VIOLET_SOFT, ACCENT_DEEP, MINT, Rng::new(0));

    stack.into()
}

// ── The vignettes — each app's signature screen, stylized ───────────────────

/// SnapSearch — the search dialog: a query field, the sparkle medallion,
/// and result rows with match badges.
fn paint_search(book: &mut Sketchbook, t: f32) {
    let px = 55.0;
    let py = 66.0;
    let pw = 270.0;
    // The search field.
    book.rrect(xywh(px + 18.0, py + 34.0, pw - 36.0, 34.0), 10.0, alpha(Color::WHITE, 0.08));
    book.stroke_rrect(xywh(px + 18.0, py + 34.0, pw - 36.0, 34.0), 10.0, alpha(SYN_TYPE, 0.5), 1.2);
    // The typing caret + query text glow.
    let caret = ((t * 2.2).fract() < 0.55).then(|| {
        book.rrect(xywh(px + pw - 52.0, py + 41.0, 2.5, 20.0), 1.2, alpha(SYN_TYPE, 0.9));
    });
    let _ = caret;
    // The medallion — a breathing sparkle ring.
    let breathe = 1.0 + 0.08 * (t * 2.0).sin();
    book.ring(Offset::new(px + 36.0, py + 51.0), 9.0 * breathe, 1.4, alpha(SYN_TYPE, 0.8));
    book.circle(Offset::new(px + 36.0, py + 51.0), 3.0, alpha(tint(SYN_TYPE, 0.4), 0.95));
    // Result rows with match badges.
    for r in 0..3 {
        let ry = py + 90.0 + r as f32 * 52.0;
        book.rrect(xywh(px + 18.0, ry, pw - 36.0, 40.0), 9.0, alpha(Color::WHITE, 0.05));
        // The thumbnail.
        book.rrect(xywh(px + 26.0, ry + 6.0, 28.0, 28.0), 6.0, alpha(mix(MUTED, SYN_TYPE, 0.2), 0.3));
        // The text bars.
        book.rrect(xywh(px + 62.0, ry + 10.0, 110.0 - r as f32 * 16.0, 7.0), 3.0, alpha(Color::WHITE, 0.22));
        book.rrect(xywh(px + 62.0, ry + 24.0, 70.0, 5.0), 2.5, alpha(Color::WHITE, 0.10));
        // The match badge.
        let badge_a = if r == 0 { 0.9 } else { 0.5 };
        book.rrect(xywh(px + pw - 58.0, ry + 12.0, 34.0, 16.0), 8.0, alpha(SYN_TYPE, badge_a));
    }
}

/// three — the feed: avatars, moments, the gradient ring of a profile.
fn paint_three(book: &mut Sketchbook, t: f32) {
    let px = 55.0;
    let py = 66.0;
    let pw = 270.0;
    // The profile ring — a gradient arc, rotating slowly.
    let sweep = std::f32::consts::TAU * 0.75;
    let start = t * 0.6;
    let mut p = vieww_foundation::Path::arc_ring(Offset::new(px + 40.0, py + 52.0), 16.0, 3.5, start, sweep);
    book.fill(p, Gradient::sweep(Offset::new(px + 40.0, py + 52.0), start, start + sweep).with_dither().with_stops(&[
        (0.0, ACCENT),
        (0.5, SYN_KEYWORD),
        (1.0, ACCENT),
    ]));
    // The avatar.
    book.circle(Offset::new(px + 40.0, py + 52.0), 12.0, alpha(mix(MUTED, ACCENT, 0.3), 0.6));
    // Feed rows — capture moments.
    for r in 0..3 {
        let ry = py + 92.0 + r as f32 * 62.0;
        book.rrect(xywh(px + 18.0, ry, pw - 36.0, 50.0), 10.0, alpha(Color::WHITE, 0.045));
        // The moment's thumbnail — a depth gradient.
        book.rrect(
            xywh(px + 26.0, ry + 8.0, 48.0, 34.0),
            7.0,
            Gradient::linear(Offset::new(px + 26.0, ry + 8.0), Offset::new(px + 74.0, ry + 42.0))
                .with_dither()
                .with_stops(&[(0.0, alpha(ACCENT_DEEP, 0.5)), (1.0, alpha(SYN_TYPE, 0.35))]),
        );
        book.rrect(xywh(px + 84.0, ry + 14.0, 120.0 - r as f32 * 22.0, 7.0), 3.0, alpha(Color::WHITE, 0.2));
        book.rrect(xywh(px + 84.0, ry + 28.0, 80.0, 5.0), 2.5, alpha(Color::WHITE, 0.10));
        // The remix count.
        book.circle(Offset::new(px + pw - 34.0, ry + 25.0), 8.0, alpha(SYN_KEYWORD, 0.35));
    }
}

/// upxcale — the compare: one photo sharpening under the ramp.
fn paint_upxcale(book: &mut Sketchbook, t: f32) {
    let px = 55.0;
    let py = 66.0;
    let pw = 270.0;
    let split = 0.35 + 0.3 * (t * 0.5).sin();
    // The photo — before (blurred, left) and after (sharp, right),
    // drawn as the same abstract scene: a horizon and a sun.
    let photo_x = px + 18.0;
    let photo_y = py + 40.0;
    let photo_w = pw - 36.0;
    let photo_h = 200.0;
    // The split line.
    let split_x = photo_x + photo_w * split;
    // The "sharp" half — clean gradient sky.
    book.rect(
        xywh(split_x, photo_y, photo_x + photo_w - split_x, photo_h),
        Gradient::vertical().with_dither().with_stops(&[
            (0.0, Color::rgb(0x1E, 0x2A, 0x38)),
            (1.0, Color::rgb(0x0E, 0x14, 0x1C)),
        ]),
    );
    // The sun on the sharp side — crisp.
    book.circle(Offset::new(split_x + 50.0, photo_y + 60.0), 16.0, alpha(SYN_STRING, 0.85));
    // The "blurred" half — a soft layer over the same scene.
    book.layer(1.0, 6.0, None, |g| {
        g.rect(
            xywh(photo_x, photo_y, split_x - photo_x, photo_h),
            Gradient::vertical().with_dither().with_stops(&[
                (0.0, alpha(Color::rgb(0x1E, 0x2A, 0x38), 0.85)),
                (1.0, alpha(Color::rgb(0x0E, 0x14, 0x1C), 0.85)),
            ]),
        );
        g.circle(Offset::new(split_x - 60.0, photo_y + 60.0), 18.0, alpha(SYN_STRING, 0.5));
    });
    // The horizon line — sharp side only.
    book.line(
        Offset::new(split_x, photo_y + photo_h * 0.66),
        Offset::new(photo_x + photo_w, photo_y + photo_h * 0.66),
        alpha(Color::WHITE, 0.18),
        1.0,
    );
    // The ramp handle — a grip on the split.
    book.rrect(xywh(split_x - 2.0, photo_y + photo_h * 0.5 - 16.0, 4.0, 32.0), 2.0, alpha(INK, 0.95));
    // The AFTER chip.
    book.rrect(xywh(photo_x + photo_w - 64.0, photo_y + 8.0, 46.0, 18.0), 9.0, alpha(SYN_STRING, 0.9));
    // Fresh-tile markers — small success ticks below.
    for k in 0..4 {
        let ty = photo_y + photo_h + 18.0;
        book.rrect(xywh(photo_x + 6.0 + k as f32 * 62.0, ty, 52.0, 30.0), 6.0, alpha(Color::WHITE, 0.06));
        if k < 3 {
            book.line(
                Offset::new(photo_x + 18.0 + k as f32 * 62.0, ty + 15.0),
                Offset::new(photo_x + 26.0 + k as f32 * 62.0, ty + 22.0),
                alpha(SYN_STRING, 0.8),
                2.0,
            );
            book.line(
                Offset::new(photo_x + 26.0 + k as f32 * 62.0, ty + 22.0),
                Offset::new(photo_x + 40.0 + k as f32 * 62.0, ty + 8.0),
                alpha(SYN_STRING, 0.8),
                2.0,
            );
        }
    }
}

/// vavlt — the vault: locked tiles, the consent toggle, the audit mark.
fn paint_vavlt(book: &mut Sketchbook, t: f32) {
    let px = 55.0;
    let py = 66.0;
    let pw = 270.0;
    // The vault grid — 3x2 locked tiles.
    for r in 0..2 {
        for c in 0..3 {
            let tx = px + 18.0 + c as f32 * 80.0;
            let ty = py + 44.0 + r as f32 * 80.0;
            book.rrect(xywh(tx, ty, 68.0, 68.0), 10.0, alpha(Color::WHITE, 0.05));
            book.stroke_rrect(xywh(tx, ty, 68.0, 68.0), 10.0, alpha(SYN_FUNCTION, 0.25), 1.0);
            // The lock glyph — a shackle and a body.
            book.rrect(xywh(tx + 26.0, ty + 30.0, 16.0, 14.0), 3.0, alpha(SYN_FUNCTION, 0.7));
            book.stroke_rrect(xywh(tx + 29.0, ty + 22.0, 10.0, 10.0), 5.0, alpha(SYN_FUNCTION, 0.5), 2.0);
        }
    }
    // The consent toggle — one switch, on.
    let on = true;
    let knob = if on { 20.0 } else { 4.0 };
    book.rrect(xywh(px + 18.0, py + 216.0, 44.0, 24.0), 12.0, alpha(SYN_STRING, 0.8));
    book.circle(Offset::new(px + 18.0 + knob + 4.0, py + 228.0), 8.5, alpha(INK, 0.95));
    // The audit line — zebra ticks.
    for k in 0..6 {
        let ay = py + 254.0 + k as f32 * 9.0;
        book.rect(
            xywh(px + 18.0, ay, if k % 2 == 0 { 120.0 } else { 90.0 }, 4.0),
            alpha(MUTED, 0.28),
        );
    }
    // The deep tier's warning glyph — a breathing triangle.
    let breathe = 0.6 + 0.3 * (t * 2.0).sin();
    let mut tri = vieww_foundation::Path::new();
    tri.move_to(Offset::new(px + pw - 40.0, py + 254.0));
    tri.line_to(Offset::new(px + pw - 24.0, py + 254.0));
    tri.line_to(Offset::new(px + pw - 32.0, py + 240.0));
    tri.close();
    book.fill(tri, alpha(SYN_FUNCTION, breathe));
}

// keep Rect imported for future receipts
#[allow(unused)]
fn _r(r: Rect) -> Rect { r }
