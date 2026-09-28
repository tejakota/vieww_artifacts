//! S05 · THIRTY-SIX CRATES — Act II opens: the architecture. 0:39–0:50.
//!
//! The reference script's "modular block diagram" made kinetic: the 36
//! crates of `vieww_base` — the real list, counted from the array, never
//! typed as a figure — fly in from the dark and snap into six layered
//! tiers, connectors pulsing with light between them. The counter ticks as
//! they land: **36 crates · one dependency graph**. The layers rise into a
//! gentle 3D stack (the unfold's grammar, held vertical here): foundation
//! at the base, the meta-crate `vieww` at the crown.

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{alpha, clamp01, ease_in_out, ease_out_back, ease_out_cubic, mix, tint, xywh, FAINT, INK, MUTED, Rng, VIOLET, VIOLET_SOFT, CYAN, CYAN_SOFT, MINT, AMBER, MAGENTA};

use super::{Ctx};

/// The 36 crates — verbatim from `vieww_base/Cargo.toml`'s workspace
/// members (the crates/ directory, counted by the code that draws them).
const CRATES: [&str; 36] = [
    "vieww-foundation", "vieww-widget", "vieww-element", "vieww-paint",
    "vieww-hal", "vieww-text", "vieww-animation", "vieww-asset",
    "vieww-gestures", "vieww-render", "vieww-scene", "vieww-render-graph",
    "vieww-render-planner", "vieww-runtime", "vieww-gpu", "vieww-shaders",
    "vieww-reload", "vieww-platform", "vieww-platform-winit", "vieww-platform-web",
    "vieww-platform-web-dom", "vieww-hardware", "vieww-effects", "vieww-devtools",
    "vieww", "vieww-say-codegen", "vieww-test-harness", "vieww-image",
    "vieww-interaction", "vieww-scroll", "vieww-accessibility", "vieww-plugin",
    "vieww-plugin-macros", "vieww-widget-macros", "vieww-build", "vieww-cli",
];

/// The tier of a crate index — six tiers, bottom to top.
fn tier_of(i: usize) -> usize {
    match i {
        0 => 0,                        // foundation — alone at the base
        1..=5 => 1,                    // widget, element, paint, hal, text
        6..=12 => 2,                   // animation .. render-planner
        13..=22 => 3,                  // runtime .. hardware
        23..=24 => 4,                  // effects, devtools
        _ => 5,                        // the ecosystem crown
    }
}

/// The tier's accent.
fn tier_color(tier: usize) -> Color {
    match tier {
        0 => VIOLET,
        1 => CYAN,
        2 => MINT,
        3 => CYAN_SOFT,
        4 => MAGENTA,
        _ => VIOLET_SOFT,
    }
}

/// The block geometry: tiers are rows; within a tier, blocks are laid out
/// evenly. Returns (x, y, w, h) for crate `i` in the diagram's space.
fn block_rect(i: usize) -> (f32, f32, f32, f32) {
    let tier = tier_of(i);
    let members: Vec<usize> = (0..CRATES.len()).filter(|&j| tier_of(j) == tier).collect();
    let slot = members.iter().position(|&j| j == i).unwrap_or(0);
    let n = members.len().max(1);
    // Tier rows, bottom (0) at the diagram's base.
    let tier_y = 720.0 - tier as f32 * 118.0;
    let row_w = match n { 1 => 300.0, 2..=4 => 1240.0, _ => 1460.0 };
    let bw = match n { 1 => 300.0, 2..=4 => (row_w - (n as f32 - 1.0) * 20.0) / n as f32, _ => (row_w - (n as f32 - 1.0) * 12.0) / n as f32 };
    let x = 960.0 - row_w * 0.5 + slot as f32 * (bw + if n > 4 { 12.0 } else { 20.0 });
    (x, tier_y, bw, if tier == 5 { 74.0 } else { 62.0 })
}

pub fn build(ctx: &Ctx) -> WidgetNode {
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
            super::stars(book, w, h, 0x51DE, 90, t, 0.09);
            // A cool engine-room glow behind the stack.
            super::glow(book, w * 0.5, h * 0.52, 560.0, CYAN, 0.08);
            super::vignette(book, w, h, 0.46);
        }),
    )));

    // The connectors — light rising between tiers, drawn behind the blocks.
    // Each connector lights when both its blocks have landed.
    let land = |i: usize| -> f32 {
        // Staggered by tier then slot: the base lands first.
        let tier = tier_of(i) as f32;
        let slot = {
            let tr = tier_of(i);
            (0..i).filter(|&j| tier_of(j) == tr).count() as f32
        };
        let t0 = 0.06 + tier * 0.085 + slot * 0.016;
        ease_out_back(clamp01((t - t0) / 0.16))
    };
    let connector_a = clamp01((t - 0.30) / 0.30);
    if connector_a > 0.0 {
        let mut lines: Vec<((f32, f32), (f32, f32))> = Vec::new();
        for i in 0..CRATES.len() {
            if tier_of(i) == 0 {
                continue;
            }
            let (x, y, w, h) = block_rect(i);
            // Connect to the tier below's nearest member (the parent's
            // center; the real graph is denser, the diagram is honest about
            // being a diagram).
            let below: Vec<usize> = (0..CRATES.len())
                .filter(|&j| tier_of(j) == tier_of(i) - 1)
                .collect();
            let pick = below[i % below.len()];
            let (px, py, pw, _ph) = block_rect(pick);
            lines.push(((px + pw * 0.5, py), (x + w * 0.5, y + h)));
        }
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                for (a, b) in &lines {
                    let pulse = 0.5 + 0.5 * (sec * 2.4 + a.0 * 0.01 + b.1 * 0.01).sin();
                    book.line(
                        Offset::new(a.0, a.1),
                        Offset::new(b.0, b.1),
                        alpha(CYAN, 0.10 + 0.10 * pulse * connector_a),
                        1.2,
                    );
                }
            }),
        )));
    }

    // The crate blocks themselves.
    for i in 0..CRATES.len() {
        let s = land(i);
        if s <= 0.0 {
            continue;
        }
        let (x, y, w, h) = block_rect(i);
        let tier = tier_of(i);
        let col = tier_color(tier);
        let name = CRATES[i];
        let settled = s >= 0.999;
        // Fly-in: from below and slightly scattered, springing to rest.
        let rise = (1.0 - s) * 140.0;
        let drift = (i as f32 % 7.0 - 3.0) * (1.0 - s) * 18.0;
        let a = clamp01(s * 2.0);
        stack = stack.push(
            Positioned::new()
                .left(x + drift)
                .top(y + rise)
                .width(w)
                .height(h)
                .child(Opacity::new(a).child(Painting::sized(
                    Size::new(w, h),
                    PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                        // The slab — dark glass with the tier's edge light.
                        book.rrect(xywh(0.0, 0.0, w, h), 9.0, alpha(Color::rgb(15, 16, 22), 0.94));
                        book.stroke_rrect(xywh(0.0, 0.0, w, h), 9.0, alpha(col, if settled { 0.42 } else { 0.75 }), 1.3);
                        // The top edge light — the tier's accent.
                        book.rrect(xywh(6.0, 3.0, (w - 12.0).max(2.0), 2.4), 1.2, alpha(col, 0.65));
                        // The crown tier gets the mark's glow.
                        if tier == 5 {
                            book.layer(1.0, 12.0, None, |g| {
                                g.circle(
                                    Offset::new(w * 0.5, h * 0.5),
                                    w * 0.5,
                                    Gradient::radial_fill().with_dither().with_stops(&[
                                        (0.0, alpha(VIOLET, 0.16)),
                                        (1.0, alpha(VIOLET, 0.0)),
                                    ]),
                                );
                            });
                        }
                    }),
                ))),
        );
        // The crate's name — mono, small, inside the slab.
        let font = if w > 240.0 { 19.0 } else { 12.5 };
        stack = stack.push(
            Positioned::new()
                .left(x + drift)
                .top(y + rise + h * 0.5 - 11.0)
                .width(w)
                .height(22.0)
                .child(Opacity::new(a).child(
                    Text::new(name)
                        .style(TextStyle::new(font).monospace().color(alpha(INK, 0.92)))
                        .align(TextAlign::Center),
                )),
        );
    }

    // The counter — crates landed, derived from the same land() the blocks
    // animate by; the total is CRATES.len(), counted, never typed.
    let landed = (0..CRATES.len()).filter(|&i| land(i) > 0.6).count();
    let count_a = clamp01((t - 0.10) / 0.2);
    if count_a > 0.0 {
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(140.0)
                .width(1920.0)
                .height(70.0)
                .child(Opacity::new(count_a).child(
                    Text::new(format!("{} crates · one dependency graph", landed))
                        .style(TextStyle::new(44.0).monospace().letter_spacing(3.0).color(alpha(INK, 0.96)))
                        .align(TextAlign::Center),
                )),
        );
    }
    // The tier legend, right side.
    let legend: [(&str, Color); 3] = [
        ("the base — foundation", VIOLET),
        ("the engine — paint · text · render · gpu", CYAN),
        ("the product — the vieww crate & friends", VIOLET_SOFT),
    ];
    for (i, (label, c)) in legend.iter().enumerate() {
        let la = clamp01((t - 0.42 - i as f32 * 0.08) / 0.2);
        if la <= 0.0 {
            continue;
        }
        stack = stack.push(
            Positioned::new()
                .left(1640.0)
                .top(190.0 + i as f32 * 34.0)
                .width(280.0)
                .height(26.0)
                .child(Opacity::new(la).child(
                    Text::new(*label)
                        .style(TextStyle::new(14.5).monospace().letter_spacing(1.2).color(alpha(INK, 0.85)))
                        .align(TextAlign::Left),
                )),
        );
    }

    stack = stack.push(super::act_chip("II", "THE FOUNDATION", clamp01((sec - 0.3) / 0.5)));
    stack = stack.push(super::caption(
        "thirty-six crates. one graph. zero orphan layers.",
        1000.0,
        clamp01((t - 0.60) / 0.14),
    ));

    stack.into()
}
