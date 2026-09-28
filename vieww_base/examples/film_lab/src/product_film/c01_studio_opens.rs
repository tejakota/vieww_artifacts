//! C01 · STUDIO OPENS — **the actual viewwstudio**, booted full-bleed on
//! the film's own driver. 2:08–2:18.
//!
//! Out of the engine's collapse, the cut — and there it is: the real
//! IDE. The real title bar, the real file tree with the workspace's
//! screens, the real editor opening `live.rs`, the real preview pane
//! waiting. The script has already switched the active tab; the bottom
//! panel closes for the film's wide frame. No flash over the product
//! (the honesty rule — the studio's pixels are the product's own).
//!
//! The overlay is the film's voice only: the act chip, the witness
//! (still 0 — nothing has been touched), the session clock, and the
//! captions. And the name — *viewwstudio* — held over the title bar
//! for one beat before dissolving into the chrome.

use vieww_foundation::TextAlign;
use vieww_widget::prelude::*;
use vieww_widget::WidgetNode;

use super::{ACCENT, Ctx, INK, SYN_TYPE, W, caption, chip_row, clamp01, studio_chrome, tint};

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = studio_chrome(ctx);

    // The title — the studio's name, in the film's own type, arriving
    // over the title bar area for one held beat before dissolving into
    // the chrome.
    let title_a = (clamp01((t - 0.04) / 0.10) * (1.0 - clamp01((t - 0.30) / 0.14))).clamp(0.0, 1.0);
    if title_a > 0.02 {
        let rise = (1.0 - clamp01((t - 0.04) / 0.10)) * 10.0;
        stack = stack.push(
            vieww_widget::Positioned::new()
                .left(0.0)
                .top(180.0 + rise)
                .width(W)
                .height(64.0)
                .child(
                    vieww_widget::Opacity::new(title_a).child(
                        vieww_widget::Text::new("viewwstudio")
                            .style(super::geist(44.0).letter_spacing(3.0).color(tint(INK, 0.0)))
                            .align(TextAlign::Center),
                    ),
                ),
        );
        stack = stack.push(
            vieww_widget::Positioned::new()
                .left(0.0)
                .top(250.0 + rise)
                .width(W)
                .height(30.0)
                .child(
                    vieww_widget::Opacity::new(title_a * 0.8).child(
                        vieww_widget::Text::new("the actual app — not a mock")
                            .style(super::geist_mono(16.0).letter_spacing(2.2).color(tint(SYN_TYPE, 0.15)))
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }

    // The captions — the arrival's beats.
    stack = stack.push(caption(
        "the studio opens — the real shell, the real workspace",
        1002.0,
        clamp01((t - 0.10) / 0.12),
    ));
    stack = stack.push(caption(
        "live.rs is the sketch: type, and the preview repaints",
        966.0,
        clamp01((t - 0.42) / 0.12),
    ));
    // The receipt chips — what the studio is, staggered.
    stack = stack.push(chip_row(
        &[
            ("the real Shell", ACCENT),
            ("the real editor", SYN_TYPE),
            ("the real preview", ACCENT),
        ],
        // The editor's empty lower band — see `receipt_row`. Pinned
        // right, these rows rendered straight through the preview
        // pane's description paragraph in every studio scene.
        360.0,
        924.0,
        clamp01((sec - 1.4) / 0.5),
    ));

    stack.into()
}
