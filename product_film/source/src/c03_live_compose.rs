//! C03 · LIVE COMPOSE — the loop, closed. 2:30–2:42.
//!
//! The film types into the real editor — `title "My own inbox"` — and
//! the preview repaints as the buffer changes: the live sketch's
//! heading changes under the keystroke, with the studio's own damage
//! overlay switched on so the *one rect* that changed is the only rect
//! that lights. The row's text changes next. This is the studio's
//! whole reason to exist, shown working: **edit a line, see the
//! picture change, edit the next line** — the distance the film has
//! been collapsing, spent in one keystroke.
//!
//! The alive probe measures this scene's edit window: the census's
//! stopwatch around the keystroke → the next frame's raster.

use vieww_foundation::TextAlign;
use vieww_widget::prelude::*;
use vieww_widget::WidgetNode;

use super::{ACCENT, Ctx, INK, LEDGER, MUTED, W, alpha, caption, chip_row, clamp01, distance_chip, studio_chrome};

/// The edit's film-times (the script's own).
const EDIT_TITLE_AT: f32 = 154.0;
const EDIT_ROW_AT: f32 = 157.5;

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = studio_chrome(ctx);

    // The big callout — the loop, spelled, over the editor's left half.
    let loop_a = clamp01((t - 0.05) / 0.16);
    if loop_a > 0.01 {
        for (i, line) in [
            "edit a line,",
            "see the picture change,",
            "edit the next line.",
        ]
        .iter()
        .enumerate()
        {
            stack = stack.push(
                vieww_widget::Positioned::new()
                    .left(300.0)
                    .top(180.0 + i as f32 * 44.0)
                    .width(800.0)
                    .height(42.0)
                    .child(
                        vieww_widget::Opacity::new(loop_a).child(
                            vieww_widget::Text::new(line.to_string())
                                .style(
                                    super::geist(30.0)
                                        .letter_spacing(1.2)
                                        .color(alpha(INK, 0.95)),
                                )
                                .align(TextAlign::Left),
                        ),
                    ),
            );
        }
    }

    // The keystroke callout — a mono chip that fires with each scripted
    // edit, carrying the edit's own diff.
    for (at, text) in [
        (EDIT_TITLE_AT, "title \"My own inbox\""),
        (EDIT_ROW_AT, "row \"Release cut is ready\""),
    ] {
        let since = ctx.abs - at;
        if (-0.2..3.0).contains(&since) {
            let a = clamp01((since + 0.2) / 0.3) * (1.0 - clamp01((since - 2.0) / 1.0));
            stack = stack.push(
                vieww_widget::Positioned::new()
                    .left(300.0)
                    .top(360.0)
                    .width(700.0)
                    .height(36.0)
                    .child(
                        vieww_widget::Opacity::new(a).child(
                            vieww_widget::Container::new()
                                .color(alpha(super::WASH, 0.85))
                                .radius(8.0)
                                .border(vieww_foundation::Border::new(alpha(ACCENT, 0.3), 1.2))
                                .padding(vieww_foundation::EdgeInsets::symmetric(8.0, 14.0))
                                .child(
                                    vieww_widget::Text::new(text)
                                        .style(
                                            super::geist_mono(15.0)
                                                .letter_spacing(1.2)
                                                .color(alpha(LEDGER, 0.95)),
                                        )
                                        .align(TextAlign::Left),
                                ),
                        ),
                    ),
            );
        }
    }

    // The captions — the compose's story, including the damage overlay.
    stack = stack.push(caption(
        "the film types — the studio's real editor takes the keystroke",
        1002.0,
        clamp01((t - 0.10) / 0.12),
    ));
    stack = stack.push(caption(
        "the damage overlay is on: only the changed rect repaints",
        966.0,
        clamp01((t - 0.42) / 0.12),
    ));

    // The receipt chips.
    stack = stack.push(chip_row(
        &[
            ("damage: one rect", LEDGER),
            ("rebuild: one subtree", LEDGER),
            ("budget: kept", ACCENT),
        ],
        W - 640.0,
        936.0,
        clamp01((sec - 1.0) / 0.5),
    ));

    stack = stack.push(distance_chip(ctx.abs, clamp01(t / 0.1)));

    let _ = MUTED;
    stack.into()
}
