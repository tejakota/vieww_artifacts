//! S09 · THE CEILINGS — Act II's close: the flex wall. 1:21–1:28.
//!
//! Four live vignettes, each a mini-engine running at film speed, each
//! wearing its own receipt — quoted verbatim from the repository's own
//! metrics (sources: `film_lab/renders/{galaxy,mandel,hero4k,longplay}/
//! metrics.txt`, summarized in the root README's rounds 6–7 receipts):
//!
//! - **galaxy** — 60,527 shapes per frame, rastered in 101 ms
//! - **mandel** — 57,602 rects per frame, 69 ms
//! - **hero4k** — the worst frame at 3840×2160, 440 ms
//! - **longplay** — 256 frames, RSS flat at 38.3→39.0 MiB
//!
//! The point is the reference's own: "nothing in the film is limited by
//! the renderer — the constraint is taste."

use vieww_foundation::{Color, Offset, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use crate::film_lib::{
    alpha, clamp01, ease_out_back, mix, tint, xywh, Rng, AMBER, CYAN, CYAN_SOFT, INK, MAGENTA,
    MINT, MUTED, VIOLET, VIOLET_SOFT,
};

use super::Ctx;

/// The four vignette cards: (title, receipt, accent).
const CARDS: [(&str, &str, Color); 4] = [
    ("galaxy", "60,527 shapes / frame · 101 ms", VIOLET_SOFT),
    ("mandel", "57,602 rects / frame · 69 ms", CYAN_SOFT),
    ("hero4k", "3840 × 2160 · 440 ms", MINT),
    ("longplay", "256 frames · RSS 38.3 → 39.0 MiB", AMBER),
];

/// Card geometry — a 2×2 grid.
fn card_rect(i: usize) -> (f32, f32) {
    let x = 420.0 + (i % 2) as f32 * 580.0;
    let y = 240.0 + (i / 2) as f32 * 360.0;
    (x, y)
}

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = Stack::new();

    // The ground.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            super::ground(book, w, h);
            super::stars(book, w, h, 0x9E11, 80, t, 0.08);
            super::vignette(book, w, h, 0.5);
        }),
    )));

    // The cards.
    for i in 0..CARDS.len() {
        let (title, receipt, accent) = CARDS[i];
        let arrive = ease_out_back(clamp01((t - 0.04 - i as f32 * 0.08) / 0.20));
        if arrive <= 0.0 {
            continue;
        }
        let (x, y) = card_rect(i);
        let cw = 520.0;
        let ch = 300.0;
        let rise = (1.0 - arrive) * 60.0;
        let a = clamp01(arrive * 1.6);
        let i_f = i as f32;

        stack = stack.push(
            Positioned::new()
                .left(x)
                .top(y + rise)
                .width(cw)
                .height(ch)
                .child(Opacity::new(a).child(Painting::sized(
                    Size::new(cw, ch),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        book.rrect(
                            xywh(0.0, 0.0, cw, ch),
                            16.0,
                            alpha(Color::rgb(13, 14, 20), 0.95),
                        );
                        book.stroke_rrect(xywh(0.0, 0.0, cw, ch), 16.0, alpha(accent, 0.35), 1.3);
                        // The vignette plate — each card's own mini-engine.
                        let px0 = 24.0;
                        let py0 = 58.0;
                        let pw = cw - 48.0;
                        let ph = ch - 130.0;
                        match i_f as usize {
                            0 => plate_galaxy(book, px0, py0, pw, ph, sec),
                            1 => plate_mandel(book, px0, py0, pw, ph, sec),
                            2 => plate_hero4k(book, px0, py0, pw, ph, sec),
                            _ => plate_longplay(book, px0, py0, pw, ph, sec),
                        }
                        let _ = accent;
                    }),
                ))),
        );
        // The title + receipt.
        stack = stack.push(
            Positioned::new()
                .left(x + 24.0)
                .top(y + rise + 16.0)
                .width(300.0)
                .height(28.0)
                .child(
                    Opacity::new(a).child(
                        Text::new(title).style(
                            TextStyle::new(22.0)
                                .monospace()
                                .letter_spacing(2.4)
                                .color(alpha(INK, 0.95)),
                        ),
                    ),
                ),
        );
        stack = stack.push(
            Positioned::new()
                .left(x + 24.0)
                .top(y + rise + ch - 56.0)
                .width(cw - 48.0)
                .height(26.0)
                .child(
                    Opacity::new(clamp01((arrive - 0.4) * 2.0)).child(
                        Text::new(receipt).style(
                            TextStyle::new(16.5)
                                .monospace()
                                .letter_spacing(1.0)
                                .color(alpha(tint(accent, 0.05), 0.95)),
                        ),
                    ),
                ),
        );
    }

    // The headline.
    let head_a = clamp01(t / 0.14);
    stack = stack.push(
        Positioned::new()
            .left(0.0)
            .top(130.0)
            .width(1920.0)
            .height(44.0)
            .child(
                Opacity::new(head_a).child(
                    Text::new("the renderer's ceilings")
                        .style(
                            TextStyle::new(34.0)
                                .letter_spacing(1.5)
                                .color(alpha(INK, 0.96)),
                        )
                        .align(TextAlign::Center),
                ),
            ),
    );
    stack = stack.push(
        Positioned::new()
            .left(0.0)
            .top(182.0)
            .width(1920.0)
            .height(30.0)
            .child(
                Opacity::new(head_a).child(
                    Text::new("nothing here is limited by the renderer — the constraint is taste")
                        .style(
                            TextStyle::new(19.0)
                                .monospace()
                                .letter_spacing(1.8)
                                .color(alpha(MUTED, 0.9)),
                        )
                        .align(TextAlign::Center),
                ),
            ),
    );

    stack = stack.push(super::caption(
        "receipts quoted from the lab's own metrics files",
        1000.0,
        clamp01((t - 0.55) / 0.14),
    ));

    stack.into()
}

// ── The four mini-engines ───────────────────────────────────────────────────

/// galaxy — a spiral of 700 stars, density falling with radius.
fn plate_galaxy(book: &mut Sketchbook, x: f32, y: f32, w: f32, h: f32, sec: f32) {
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    let spin = sec * 0.4;
    let mut rng = Rng::new(0x6A1A);
    let arms = 2.0;
    for _ in 0..700 {
        let r = rng.f01().sqrt() * w.min(h) * 0.62;
        let arm = (rng.f01() * arms).floor();
        let spread = rng.sym() * 0.55;
        let a = spin + arm / arms * std::f32::consts::TAU + spread + r * 0.012;
        let px = cx + a.cos() * r * 1.25;
        let py = cy + a.sin() * r * 0.62;
        let warm = rng.f01();
        let c = if warm > 0.8 {
            AMBER
        } else if warm > 0.5 {
            Color::WHITE
        } else {
            VIOLET_SOFT
        };
        book.circle(
            Offset::new(px, py),
            0.5 + rng.f01() * 1.1,
            alpha(c, 0.25 + rng.f01() * 0.6),
        );
    }
    super::glow(book, cx, cy, w * 0.4, VIOLET, 0.14);
}

/// mandel — a rect-mosaic zoom of the boundary.
fn plate_mandel(book: &mut Sketchbook, x: f32, y: f32, w: f32, h: f32, sec: f32) {
    let cols = 44;
    let rows = 26;
    let cw = w / cols as f32;
    let chh = h / rows as f32;
    let zoom = 1.0 + (sec * 0.22).sin() * 0.4;
    let ox = -0.7435;
    let oy = 0.1318;
    let scale = 2.6 / zoom;
    for gy in 0..rows {
        for gx in 0..cols {
            let u = (gx as f32 + 0.5) / cols as f32 - 0.5;
            let v = (gy as f32 + 0.5) / rows as f32 - 0.5;
            let cx = ox + u * scale * 1.6;
            let cy = oy + v * scale;
            // A tiny escape-time iteration — the real arithmetic, scaled
            // down so the plate stays a plate.
            let (mut zx, mut zy, mut it) = (0.0f32, 0.0f32, 0u32);
            let max_it = 24;
            while zx * zx + zy * zy < 4.0 && it < max_it {
                let nx = zx * zx - zy * zy + cx;
                zy = 2.0 * zx * zy + cy;
                zx = nx;
                it += 1;
            }
            let k = it as f32 / max_it as f32;
            let c = if k >= 1.0 {
                alpha(Color::rgb(8, 8, 12), 0.9)
            } else {
                mix(MAGENTA, CYAN, k)
            };
            book.rect(
                xywh(x + gx as f32 * cw, y + gy as f32 * chh, cw + 0.6, chh + 0.6),
                alpha(c, 0.85),
            );
        }
    }
}

/// hero4k — a 4K grid of cells, a sweep counting them off.
fn plate_hero4k(book: &mut Sketchbook, x: f32, y: f32, w: f32, h: f32, sec: f32) {
    let cols = 64;
    let rows = 36;
    let cw = w / cols as f32;
    let chh = h / rows as f32;
    let sweep = ((sec * 0.5) % 1.0) * (cols + rows) as f32;
    for gy in 0..rows {
        for gx in 0..cols {
            let d = (gx + gy) as f32;
            let past = d < sweep;
            let c = if past {
                MINT
            } else {
                alpha(Color::WHITE, 0.16)
            };
            let a = if past {
                0.14 + 0.10 * ((gx * 7 + gy * 13) % 5) as f32 / 5.0
            } else {
                0.14
            };
            book.rect(
                xywh(x + gx as f32 * cw, y + gy as f32 * chh, cw - 1.0, chh - 1.0),
                alpha(c, a),
            );
        }
    }
    // The sweep line.
    let sy = (sweep / (cols + rows) as f32) * h;
    book.rect(xywh(x, y + sy, w, 1.4), alpha(tint(MINT, 0.3), 0.9));
}

/// longplay — an RSS time series, flat as the claim.
fn plate_longplay(book: &mut Sketchbook, x: f32, y: f32, w: f32, h: f32, sec: f32) {
    // 38.3 → 39.0 MiB over 256 frames — the receipt's own numbers as a
    // line. Flat means flat.
    let base = 38.3f32;
    let rise = 0.7f32;
    let n = 128;
    let scroll = (sec * 18.0) as usize;
    let mut p = vieww_foundation::Path::new();
    for i in 0..=n {
        let k = i as f32 / n as f32;
        let frame = ((k * 256.0) as usize + scroll) % 256;
        let v = base + rise * (frame as f32 / 256.0) + ((frame % 17) as f32 * 0.012).sin() * 0.05;
        // Map 38→40 MiB to the plate's height.
        let yy = y + h - ((v - 38.0) / 2.0) * h;
        let xx = x + k * w;
        if i == 0 {
            p.move_to(Offset::new(xx, yy));
        } else {
            p.line_to(Offset::new(xx, yy));
        }
    }
    book.stroke(p, alpha(tint(AMBER, 0.1), 0.95), 2.0);
    // The band edges.
    book.line(
        Offset::new(x, y + h - 0.15 * h),
        Offset::new(x + w, y + h - 0.15 * h),
        alpha(MUTED, 0.25),
        1.0,
    );
    book.line(
        Offset::new(x, y + h - 0.5 * h),
        Offset::new(x + w, y + h - 0.5 * h),
        alpha(MUTED, 0.25),
        1.0,
    );
}
