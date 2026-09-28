//! C04 · THE TAP — a real touch, through the real pipeline. 2:42–2:50.
//!
//! The film's finger lands on the live demo's *Open settings* button —
//! a real `PointerEvent::down`/`up` pair through `handle_pointer`, the
//! real gesture disambiguation, the real route change: the preview
//! navigates from Home to Settings, the nav bar's selection moves, the
//! toggle and the badge counter appear. The tap ring blooms where the
//! finger landed; the witness counter ticks to 3. Nothing about this is
//! simulated except the finger — and the film says so.
//!
//! A short scene, deliberately: the tap is one beat, and the beat is
//! the proof.

use vieww_foundation::TextAlign;
use vieww_widget::prelude::*;
use vieww_widget::WidgetNode;

use super::{Ctx, LEDGER, W, alpha, caption, chip_row, clamp01, studio_chrome, tap_ring_at};
use super::script::TAP_LIVE_ROW;

/// The tap's film-time (the script's own: up at 166.5).
const TAP_AT: f32 = 166.5;

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let _sec = ctx.sec;

    let mut stack = studio_chrome(ctx);

    // The callout — before the tap, the frame names what it is about to
    // do; after it, what it did.
    let pre = clamp01((t - 0.05) / 0.12) * (1.0 - clamp01((t - 0.44) / 0.10));
    if pre > 0.01 {
        stack = stack.push(
            vieww_widget::Positioned::new()
                .left(0.0)
                .top(190.0)
                .width(W)
                .height(40.0)
                .child(
                    vieww_widget::Opacity::new(pre).child(
                        vieww_widget::Text::new("a real tap, through the real input pipeline")
                            .style(super::geist_mono(22.0).letter_spacing(2.0).color(alpha(super::INK, 0.95)))
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }
    let post = clamp01((t - 0.52) / 0.10);
    if post > 0.01 {
        stack = stack.push(
            vieww_widget::Positioned::new()
                .left(0.0)
                .top(190.0)
                .width(W)
                .height(40.0)
                .child(
                    vieww_widget::Opacity::new(post).child(
                        vieww_widget::Text::new("the route changed — the demo navigated, for real")
                            .style(super::geist_mono(22.0).letter_spacing(2.0).color(alpha(LEDGER, 0.95)))
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }

    // The tap ring — the film's annotation of the real interaction,
    // blooming where the (scripted) finger landed.
    let since = (ctx.abs - TAP_AT) / 1.2;
    if (0.0..1.0).contains(&since) {
        stack = stack.push(tap_ring_at(TAP_LIVE_ROW, since));
    }

    // The captions.
    stack = stack.push(caption(
        "pointer down, pointer up — the gesture engine decides it was a tap",
        1002.0,
        clamp01((t - 0.08) / 0.10),
    ));
    stack = stack.push(caption(
        "the preview navigates: Home → Settings, one subtree rebuilt",
        966.0,
        clamp01((t - 0.58) / 0.12),
    ));

    // The receipt chips.
    stack = stack.push(chip_row(
        &[
            ("real PointerEvent", LEDGER),
            ("real gesture arena", LEDGER),
            ("real route", super::ACCENT),
        ],
        W - 640.0,
        936.0,
        clamp01((ctx.sec - 0.8) / 0.4),
    ));

    stack.into()
}
