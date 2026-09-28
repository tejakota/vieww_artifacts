//! C06 · STATE CARRIES — the recompile that forgets nothing. 3:06–3:18.
//!
//! One word of the say program changes — `a heading "Counter"` becomes
//! `a heading "Counted"` — and Render fires again: a new compile, a new
//! cdylib, a *different* screen mounted. And the counter — the `keep`ed
//! value, the thing the user was mid-way through — survives the
//! recompile. The count is still 1. State that rides the edit loop is
//! the difference between a tool you sketch in and a tool you *work*
//! in; this scene is that difference, shown.
//!
//! The film marks the moment with a "still 1" receipt chip.

use vieww_foundation::TextAlign;
use vieww_widget::prelude::*;
use vieww_widget::WidgetNode;

use super::{ACCENT, Ctx, INK, LEDGER, MUTED, SYN_KEYWORD, W, alpha, caption, chip_row, clamp01, studio_chrome};

/// The second Render's film-time (the script's own).
const RENDER2_AT: f32 = 188.1;
/// The settle — the screen is back, at 189.0.
const BACK_AT: f32 = 189.0;

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let _sec = ctx.sec;

    let mut stack = studio_chrome(ctx);

    // The diff — the one-word change, quoted as a diff over the editor.
    let diff_a = clamp01((t - 0.04) / 0.12);
    if diff_a > 0.01 {
        for (i, (line, color)) in [
            ("- a heading \"Counter\"", SYN_KEYWORD),
            ("+ a heading \"Counted\"", LEDGER),
        ]
        .iter()
        .enumerate()
        {
            stack = stack.push(
                vieww_widget::Positioned::new()
                    .left(300.0)
                    .top(180.0 + i as f32 * 34.0)
                    .width(900.0)
                    .height(30.0)
                    .child(
                        vieww_widget::Opacity::new(diff_a).child(
                            vieww_widget::Text::new(line.to_string())
                                .style(
                                    super::geist_mono(18.0)
                                        .letter_spacing(1.0)
                                        .color(alpha(*color, 0.95)),
                                )
                                .align(TextAlign::Left),
                        ),
                    ),
            );
        }
    }

    // The rule — the say language's own words for what is happening.
    let rule_a = clamp01((t - 0.3) / 0.14);
    if rule_a > 0.01 {
        stack = stack.push(
            vieww_widget::Positioned::new()
                .left(300.0)
                .top(270.0)
                .width(1100.0)
                .height(56.0)
                .child(
                    vieww_widget::Opacity::new(rule_a).child(
                        vieww_widget::Text::new(
                            "a kept value is carried across the remount —\nthe user's state rides the recompile",
                        )
                        .style(super::geist_mono(15.0).letter_spacing(1.2).color(alpha(MUTED, 0.9)))
                        .align(TextAlign::Left),
                    ),
                ),
        );
    }

    // The compile state — held between the second Render and its settle.
    let compiling = ctx.abs >= RENDER2_AT && ctx.abs < BACK_AT;
    if compiling {
        stack = stack.push(
            vieww_widget::Positioned::new()
                .left(0.0)
                .top(560.0)
                .width(W)
                .height(30.0)
                .child(
                    vieww_widget::Text::new("recompiling — a different screen is being built")
                        .style(super::geist_mono(16.0).letter_spacing(1.6).color(alpha(super::SYN_FUNCTION, 0.95)))
                        .align(TextAlign::Center),
                ),
        );
    }

    // The receipt — the count, after the remount: the whole scene's
    // point, in one chip.
    if ctx.abs >= BACK_AT {
        let a = clamp01((ctx.abs - BACK_AT) / 0.3);
        stack = stack.push(
            vieww_widget::Positioned::new()
                .left(0.0)
                .top(560.0)
                .width(W)
                .height(30.0)
                .child(
                    vieww_widget::Opacity::new(a).child(
                        vieww_widget::Text::new("the screen is new. the count is still 1.")
                            .style(super::geist_mono(17.0).letter_spacing(1.8).color(alpha(LEDGER, 0.98)))
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }

    // The captions.
    stack = stack.push(caption(
        "change a word. render again. the state does not reset.",
        1002.0,
        clamp01((t - 0.05) / 0.12),
    ));
    stack = stack.push(caption(
        "the loop you work in, not the loop you sketch in",
        966.0,
        clamp01((t - 0.6) / 0.12),
    ));

    // The receipt chips.
    stack = stack.push(chip_row(
        &[
            ("keep: count", SYN_KEYWORD),
            ("remount: new screen", MUTED),
            ("count: still 1", LEDGER),
        ],
        // The editor's empty lower band — see `receipt_row`. Pinned
        // right, these rows rendered straight through the preview
        // pane's description paragraph in every studio scene.
        360.0,
        924.0,
        clamp01((ctx.sec - 1.0) / 0.5),
    ));

    let _ = INK;
    let _ = ACCENT;
    stack.into()
}
