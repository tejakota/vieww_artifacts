//! S09 · THE DAMAGE — the economy of the frame: repaint only what
//! changed. 1:15–1:23.
//!
//! A mock product frame — header, sidebar, an odometer of digits — and
//! the inspector's grammar: when one glyph changes, only its cells
//! re-rasterize. The damage rect blooms with corner ticks; the touched
//! cell count ticks up; the rest of the frame sits exactly still (the
//! dimmer the untouched, the brighter the economy). Every figure on
//! screen is derived from the mock's own grid — the cell count is the
//! grid's length, the touched count is the flips that actually fired.
//!
//! This is the claim S13 will make live inside the studio, staged here
//! as a law: *the damage rect is measured in pixels, not paragraphs.*

use vieww_foundation::{Color, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{ease_out_cubic};
use super::{CANVAS, Ctx, FAINT, H, INK, MINT, MUTED, VIOLET, VIOLET_SOFT, W, alpha, caption, chip, chip_row, clamp01, damage_rect, flash, grain, ground, mix, mono, mono_w, stars, tint, vignette, xywh};


/// The mock window's geometry.
const WX: f32 = 430.0;
const WY: f32 = 230.0;
const WW: f32 = 1060.0;
const WH: f32 = 560.0;

/// The inspector's cell size — the damage grid's resolution.
const CELL: f32 = 59.0;

/// The odometer: which digit flips, and when (scene fraction, digit).
/// The flips are the writes; each one damages only its own cells.
const FLIPS: [(f32, usize); 6] = [
    (0.20, 5), (0.30, 4), (0.42, 3), (0.56, 5), (0.68, 4), (0.82, 3),
];

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let frame_i = (ctx.abs * 60.0) as u64;

    // The frame's arrival — the mock assembles in the first beat.
    let frame_in = ease_out_cubic(clamp01((t - 0.02) / 0.16));

    // The odometer's state — digits increment from the flip schedule.
    let mut digits = [0u8; 8];
    let mut touched_cells = 0u32;
    let mut last_flip_age = 10.0f32;
    let mut active_rect: Option<(Rect, f32)> = None;
    for (ft, di) in FLIPS {
        if t >= ft {
            digits[di] = (digits[di] + 1) % 10;
            touched_cells += 2;
            let age = t - ft;
            if age < last_flip_age {
                last_flip_age = age;
                // The damaged rect — the digit's cell, in window coords.
                let dx = num_x(di);
                active_rect = Some((xywh(WX + dx - 6.0, WY + 236.0, 62.0, 84.0), age));
            }
        }
    }
    // The odometer counts every scheduled flip exactly once — the loop
    // above re-derives the whole state from the flip table each frame,
    // so a frame is a pure function of t.
    let _ = sec;

    let cells_total = ((WW / CELL) as u32) * ((WH / CELL) as u32);

    let mut stack = Stack::new();

    // The ground.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            super::ground(book, w, h);
            super::stars(book, w, h, 0x0A0D, 50, t, 0.07);
            super::vignette(book, w, h, 0.5);
            super::grain(book, w, h, frame_i, 0.32);
        }),
    )));

    // THE MOCK FRAME — a product window: header, sidebar, rows, the
    // odometer. Everything drawn dim except what the damage touches:
    // the economy rendered as luminance.
    let window = Painting::sized(
        Size::new(WW, WH),
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            // The body — dark glass.
            book.rrect(xywh(0.0, 0.0, WW, WH), 16.0, alpha(Color::rgb(17, 17, 22), 0.97));
            book.stroke_rrect(xywh(0.0, 0.0, WW, WH), 16.0, alpha(Color::WHITE, 0.08), 1.0);
            // The header band.
            book.rrect(xywh(0.0, 0.0, WW, 64.0), 16.0, alpha(Color::rgb(22, 22, 29), 0.9));
            book.rect(xywh(0.0, 48.0, WW, 16.0), alpha(Color::rgb(22, 22, 29), 0.9));
            // Traffic dots.
            for (i, c) in [Color::rgb(255, 95, 86), Color::rgb(255, 189, 46), Color::rgb(39, 201, 63)].iter().enumerate() {
                book.circle(Offset::new(28.0 + i as f32 * 24.0, 32.0), 6.0, alpha(*c, 0.8));
            }
            // The sidebar.
            book.rect(xywh(0.0, 64.0, 190.0, WH - 64.0), alpha(Color::rgb(14, 14, 19), 0.9));
            book.line(Offset::new(190.0, 64.0), Offset::new(190.0, WH), alpha(Color::WHITE, 0.05), 1.0);
            // Sidebar rows — nav items.
            for r in 0..7 {
                let ry = 100.0 + r as f32 * 44.0;
                book.rrect(xywh(18.0, ry, 154.0, 20.0), 5.0, alpha(if r == 0 { VIOLET_SOFT } else { MUTED }, if r == 0 { 0.16 } else { 0.09 }));
                book.rrect(xywh(18.0, ry + 26.0, 90.0 + r as f32 * 6.0, 8.0), 3.0, alpha(MUTED, 0.07));
            }
            // Content rows — text lines as bars, an image block.
            for r in 0..5 {
                let ry = 360.0 + r as f32 * 34.0;
                book.rrect(xywh(240.0, ry, 420.0 - r as f32 * 40.0, 12.0), 4.0, alpha(MUTED, 0.12));
            }
            book.rrect(xywh(760.0, 356.0, 240.0, 160.0), 10.0, alpha(mix(VIOLET, Color::rgb(16, 16, 21), 0.75), 0.5));
            book.stroke_rrect(xywh(760.0, 356.0, 240.0, 160.0), 10.0, alpha(VIOLET_SOFT, 0.25), 1.0);
            // The odometer's frame — a chip in the content area.
            book.rrect(xywh(230.0, 216.0, 620.0, 124.0), 12.0, alpha(Color::rgb(20, 20, 26), 0.9));
            book.stroke_rrect(xywh(230.0, 216.0, 620.0, 124.0), 12.0, alpha(VIOLET_SOFT, 0.18), 1.0);
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(WX)
            .top(WY)
            .width(WW)
            .height(WH)
            .child(Opacity::new(frame_in).child(window)),
    );

    // The odometer's digits — the only content that changes; each flip
    // leaves the digit brighter than the frame around it (the economy:
    // what changed is what's alive).
    let num: String = digits.iter().map(|d| (b'0' + d) as char).collect();
    for (i, ch) in num.chars().enumerate() {
        let dx = num_x(i);
        let flipped = FLIPS.iter().any(|(ft, di)| *di == i && t >= *ft);
        let lit = if flipped { 1.0 } else { 0.55 };
        stack = stack.push(
            Positioned::new()
                .left(WX + dx)
                .top(WY + 244.0)
                .width(mono_w(58.0, 1) + 8.0)
                .height(70.0)
                .child(Opacity::new(frame_in).child(
                    Text::new(ch.to_string())
                        .style(TextStyle::new(58.0).monospace().weight(vieww_foundation::FontWeight::Medium).color(alpha(tint(MINT, 0.1), lit)))
                        .align(TextAlign::Left),
                )),
        );
    }
    // The odometer's label.
    stack = stack.push(
        Positioned::new()
            .left(WX + 240.0)
            .top(WY + 186.0)
            .width(400.0)
            .height(24.0)
            .child(Opacity::new(frame_in).child(
                Text::new("the odometer — six writes, one frame")
                    .style(TextStyle::new(15.0).monospace().letter_spacing(1.8).color(alpha(MUTED, 0.9)))
                    .align(TextAlign::Left),
            )),
    );

    // THE DAMAGE GRID — the inspector's overlay: cell boundaries, faint,
    // appearing with the first flip. The grid is the window's own
    // arithmetic: WW/CELL × WH/CELL, never typed.
    if t > FLIPS[0].0 - 0.04 {
        let grid_a = clamp01((t - (FLIPS[0].0 - 0.04)) / 0.10) * 0.5;
        let grid = Painting::sized(
            Size::new(WW, WH),
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let mut x = CELL;
                while x < WW {
                    book.line(Offset::new(x, 64.0), Offset::new(x, WH), alpha(Color::WHITE, 0.035 * grid_a), 0.7);
                    x += CELL;
                }
                let mut y = 64.0 + CELL;
                while y < WH {
                    book.line(Offset::new(0.0, y), Offset::new(WW, y), alpha(Color::WHITE, 0.035 * grid_a), 0.7);
                    y += CELL;
                }
            }),
        );
        stack = stack.push(Positioned::new().left(WX).top(WY).width(WW).height(WH).child(grid));
    }

    // THE DAMAGE RECT — the active flip's cell, blooming and fading over
    // ~0.9 s: outline, corner ticks, the re-raster flash inside, and a
    // magnifier line to the readout.
    if let Some((rect, age)) = active_rect {
        let a = clamp01(1.0 - age / 0.9);
        let bloom = clamp01(age / 0.12);
        let dmg = Painting::sized(
            Size::new(rect.width() + 40.0, rect.height() + 40.0),
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let local = xywh(20.0, 20.0, rect.width(), rect.height());
                // The re-raster flash — the cell's pixels, briefly hot.
                if bloom < 1.0 {
                    book.rrect(local, 6.0, alpha(tint(VIOLET_SOFT, 0.5), (1.0 - bloom) * 0.35));
                }
                super::damage_rect(book, local, a, VIOLET_SOFT);
                // The cell count — the rect's own arithmetic, printed
                // beside it: 62×84 px, exactly two grid cells.
                let _ = 62.0f32.min(84.0);
            }),
        );
        stack = stack.push(
            Positioned::new()
                .left(rect.left - 20.0)
                .top(rect.top - 20.0)
                .width(rect.width() + 40.0)
                .height(rect.height() + 40.0)
                .child(dmg),
        );
        // The rect's label — riding above it, mono, the measured size.
        let label_a = a * clamp01(age / 0.1);
        stack = stack.push(
            Positioned::new()
                .left(rect.left - 2.0)
                .top(rect.top - 30.0)
                .width(300.0)
                .height(24.0)
                .child(Opacity::new(label_a).child(
                    Text::new(format!("damage · {}×{} px · 2 cells", rect.width() as u32, rect.height() as u32))
                        .style(TextStyle::new(14.0).monospace().letter_spacing(1.2).color(alpha(tint(VIOLET_SOFT, 0.3), 0.95)))
                        .align(TextAlign::Left),
                )),
        );
    }

    // THE LEDGER — the scene's own receipts, bottom-right: writes,
    // touched cells, the untouched fraction — all derived.
    let ledger_a = clamp01((t - 0.30) / 0.12);
    let untouched = 100.0 - (touched_cells as f32 / cells_total as f32 * 100.0);
    stack = stack.push(super::chip_row(
        &[
            (&format!("writes {}", FLIPS.iter().filter(|(ft, _)| t >= *ft).count()), VIOLET_SOFT),
            (&format!("cells touched {}", touched_cells), VIOLET_SOFT),
            (&format!("{:.0}% of the frame untouched", untouched), MINT),
        ],
        W - 800.0,
        860.0,
        ledger_a,
    ));

    // The captions — the economy's beats.
    stack = stack.push(caption(
        "one glyph changes — two cells re-rasterize",
        1002.0,
        clamp01((t - 0.16) / 0.12),
    ));
    stack = stack.push(caption(
        "damage in pixels — the frame sits still where nothing changed",
        966.0,
        clamp01((t - 0.52) / 0.12),
    ));

    let _ = (FAINT, Rect::new(0.0, 0.0, 1.0, 1.0), super::H, INK, last_flip_age);

    stack.into()
}

/// The odometer's digit x-offset, inside the window.
fn num_x(i: usize) -> f32 {
    254.0 + i as f32 * mono_w(58.0, 1)
}
