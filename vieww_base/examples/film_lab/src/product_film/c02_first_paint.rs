//! C02 · FIRST PAINT — the screen() contract, and the live preview's
//! first frame. 2:18–2:30.
//!
//! The studio's core rule arrives as text over the real editor: a
//! buffer needs exactly one thing to be previewable — *a function
//! named `screen` that returns something to draw*. The film runs the
//! real **Live Preview** command (a real caution dialog, really
//! accepted), and the demo mounts in the real preview pane: the
//! studio's own `live.rs` sketch — the Inbox, three rows, a nav —
//! drawn by the studio, live, at the size it will be seen at.
//!
//! The distance chip reads the *measured* number from here on: the
//! census's edit→pixels latency, quoted beside the narration. The
//! relief begins: the loop the studio exists to shorten, shortened.

use vieww_foundation::TextAlign;
use vieww_widget::prelude::*;
use vieww_widget::WidgetNode;

use super::{
    ACCENT, Ctx, INK, LEDGER, MUTED, SYN_TYPE, W, alpha, caption, chip_row, clamp01,
    distance_chip, studio_chrome, tap_ring_at,
};
use super::script::TAP_LIVE_ROW;
use crate::film_lib::ease_out_cubic;

/// The acceptance moment — the tap lands at 140.2 (film seconds).
const ACCEPT_AT: f32 = 140.2;

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = studio_chrome(ctx);

    // The code contract — the one rule, quoted over the editor, typed
    // on with a caret. This is the studio's own doc, first page.
    let rule = "one file, one screen()";
    let type_p = clamp01((t - 0.06) / 0.2);
    let typed_n = (rule.chars().count() as f32 * ease_out_cubic(type_p)).round() as usize;
    let shown: String = rule.chars().take(typed_n).collect();
    if typed_n > 0 {
        stack = stack.push(
            vieww_widget::Positioned::new()
                .left(0.0)
                .top(190.0)
                .width(W)
                .height(56.0)
                .child(
                    vieww_widget::Text::new(shown)
                        .style(
                            super::geist_mono(30.0)
                                .letter_spacing(2.0)
                                .color(alpha(SYN_TYPE, 0.95)),
                        )
                        .align(TextAlign::Center),
                ),
        );
        // The type-on caret.
        let on = (sec * 2.6).fract() < 0.55;
        if on && type_p < 1.0 {
            let w = super::gmono_w(30.0, typed_n);
            stack = stack.push(
                vieww_widget::Positioned::new()
                    .left(W * 0.5 - super::gmono_w(30.0, rule.chars().count()) * 0.5 + w + 6.0)
                    .top(196.0)
                    .width(14.0)
                    .height(34.0)
                    .child(
                        vieww_widget::Container::new().color(alpha(SYN_TYPE, 0.9)).radius(2.0),
                    ),
            );
        }
    }

    // The sub-rule — the contract's explanation, held under the rule.
    let sub_a = clamp01((t - 0.34) / 0.14);
    if sub_a > 0.01 {
        stack = stack.push(
            vieww_widget::Positioned::new()
                .left(0.0)
                .top(254.0)
                .width(W)
                .height(30.0)
                .child(
                    vieww_widget::Opacity::new(sub_a).child(
                        vieww_widget::Text::new(
                            "a function named screen, returning something to draw — that is the whole contract",
                        )
                        .style(super::geist_mono(16.0).letter_spacing(1.4).color(alpha(MUTED, 0.9)))
                        .align(TextAlign::Center),
                    ),
                ),
        );
    }

    // The captions — the live preview's story.
    stack = stack.push(caption(
        "run Live Preview — a real dialog, really accepted",
        1002.0,
        clamp01((t - 0.28) / 0.12),
    ));
    stack = stack.push(caption(
        "the preview pane mounts the sketch: edit a line, see the picture change",
        966.0,
        clamp01((t - 0.55) / 0.12),
    ));

    // The receipt chips — the preview's own honesty, staggered.
    stack = stack.push(chip_row(
        &[
            ("the real live.rs", SYN_TYPE),
            ("the real layout", LEDGER),
            ("the real paint", ACCENT),
        ],
        W - 640.0,
        936.0,
        clamp01((sec - 1.0) / 0.5),
    ));

    // The tap ring — the acceptance, annotated where it landed (the
    // dialog's accept position, annotated at the pane's center-right).
    let since = (ctx.abs - ACCEPT_AT) / 1.0;
    if (0.0..1.0).contains(&since) {
        stack = stack.push(tap_ring_at(TAP_LIVE_ROW, since));
    }

    // The distance chip — now quoting the measured number: the census's
    // alive_seconds, printed beside the narration's.
    let probe = ctx.probe;
    if probe.alive_seconds > 0.0 {
        let ms = (probe.alive_seconds * 1000.0).round();
        stack = stack.push(
            vieww_widget::Positioned::new()
                .left(0.0)
                .top(880.0)
                .width(W)
                .height(26.0)
                .child(
                    vieww_widget::Opacity::new(clamp01((t - 0.6) / 0.2)).child(
                        vieww_widget::Text::new(format!(
                            "measured, this bench: edit → pixels in {} ms",
                            ms
                        ))
                        .style(super::geist_mono(14.0).letter_spacing(1.4).color(alpha(LEDGER, 0.9)))
                        .align(TextAlign::Center),
                    ),
                ),
        );
    }

    let _ = INK;
    stack.into()
}
