//! S13 · LIVE COMPOSE — the damage overlay wakes, and every keystroke
//! is one small rect. 1:49–2:00.
//!
//! The studio's own inspector takes the stage: `show_damage`, the real
//! overlay the product ships. The film edits live.rs twice more — the
//! title edit and a row edit — and the damage rects bloom exactly where
//! the preview changed, live, per keystroke. This is S09's law
//! ("damage in pixels") demonstrated by the product itself: the mock
//! argued it; the studio proves it.
//!
//! Witness tap 3 (the compose edit, 114.0 s) fires here.

use super::{
    caption, chip, clamp01, studio_chrome, tint, Ctx, ACCENT, MINT, SYN_TYPE, VIOLET_SOFT,
};
use vieww_widget::WidgetNode;

/// The overlay's arrival (110.0 s → scene fraction 0.09).
const OVERLAY_T: f32 = 0.09;
/// The title edit — tap 3 (114.0 s → 0.45).
const EDIT_T: f32 = 0.45;
/// The row edit (116.5 s → 0.68).
const ROW_T: f32 = 0.68;

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;

    let mut stack = studio_chrome(ctx);

    // The captions — the law's beats, in order.
    stack = stack.push(caption(
        "the damage overlay — the studio's own inspector, switched on",
        1002.0,
        clamp01((t - (OVERLAY_T - 0.04)) / 0.10),
    ));
    if t > EDIT_T {
        stack = stack.push(caption(
            "one keystroke — one rect. the frame sits still where nothing changed",
            966.0,
            clamp01((t - EDIT_T) / 0.10),
        ));
    }
    if t > ROW_T {
        stack = stack.push(caption(
            "the row edit lands the same way: small, measured, live",
            930.0,
            clamp01((t - ROW_T) / 0.10),
        ));
    }

    // The receipts — the inspector's grammar, staggered.
    stack = stack.push(super::receipt_row(
        &[
            ("show_damage: on", SYN_TYPE),
            ("damage in pixels", VIOLET_SOFT),
            ("the real overlay", ACCENT),
        ],
        clamp01((ctx.sec - 2.4) / 0.5),
    ));

    // The edit-count chip — the scene's own arithmetic, riding the
    // caption band's right end: two edits, one overlay, zero rebuilds.
    if t > EDIT_T + 0.1 {
        let pop = clamp01((t - EDIT_T - 0.1) / 0.2);
        stack = stack.push(
            super::Positioned::new()
                .left(1010.0)
                .top(852.0 - (1.0 - pop) * 12.0)
                .width(420.0)
                .height(40.0)
                .child(super::Opacity::new(pop.max(0.01)).child(chip(
                    "2 edits · 0 rebuilds · live preview",
                    15.0,
                    tint(MINT, 0.1),
                ))),
        );
    }

    stack.into()
}
