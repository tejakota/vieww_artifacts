//! S14 · THE TAP — a real tap, through the real input pipeline.
//! 2:00–2:08.
//!
//! The camera holds on the demo while the script's pointer goes down at
//! the row and comes up 120 ms later — through `handle_pointer`, the
//! real gesture disambiguation, the real hit-test, the real route
//! signal. The demo navigates from Home to Detail exactly as a user's
//! finger would drive it, and the film's overlay draws its bloom ring
//! at the tap point: the witness's grammar annotating the product's own
//! ripple.
//!
//! Witness tap 4 (the route changes, 124.5 s) fires here.

use vieww_widget::prelude::*;
use vieww_widget::WidgetNode;
use super::{clamp01, script, studio_chrome, tap_ring_at, caption, Ctx};

/// The tap: down at 124.38 s, up at 124.50 s (scene fractions 0.548/0.563).
const DOWN_T: f32 = 124.38;
const UP_T: f32 = 124.50;

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;
    let scene_start = abs - ctx.sec;

    let mut stack = studio_chrome(ctx);

    // The tap's annotation — a press dot while the pointer is down, and
    // the bloom ring expanding after the up.
    if abs >= DOWN_T - 0.06 {
        let press = clamp01((DOWN_T - abs) / 0.06).abs(); // 1 just before
        let _ = press;
        let since_up = abs - UP_T;
        if since_up >= 0.0 {
            stack = stack.push(tap_ring_at(script::TAP_LIVE_ROW, since_up));
        } else if abs >= DOWN_T - 0.03 {
            // The press dot — a small spark at the touch point, the
            // finger's weight.
            let dot = super::Painting::sized(
                super::Size::new(60.0, 60.0),
                super::PaintWith::new(move |book: &mut super::Sketchbook, _s: super::Size| {
                    book.circle(
                        vieww_foundation::Offset::new(30.0, 30.0),
                        9.0,
                        super::alpha(super::ACCENT, 0.85),
                    );
                    super::glow(book, 30.0, 30.0, 60.0, super::ACCENT, 0.4);
                }),
            );
            stack = stack.push(
                super::Positioned::new()
                    .left(script::TAP_LIVE_ROW.dx - 30.0)
                    .top(script::TAP_LIVE_ROW.dy - 30.0)
                    .width(60.0)
                    .height(60.0)
                    .child(dot),
            );
        }
    }

    // The captions — the tap's beats.
    stack = stack.push(caption(
        "a real tap — through the real input pipeline",
        1002.0,
        clamp01((t - 0.08) / 0.10),
    ));
    if abs >= UP_T + 0.05 {
        stack = stack.push(caption(
            "the hit-test, the gesture arena, the route signal — all the product's",
            966.0,
            clamp01((abs - UP_T - 0.05) / 0.12),
        ));
    }
    let _ = scene_start;

    stack.into()
}
