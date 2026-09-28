//! S17 · THE TOKENS — the palette, live: the token editor and the one
//! accent that re-themes everything. 2:30–2:38.
//!
//! The studio's own Tokens view — the token list somebody can read,
//! change and export. The film's script flips the accent to Teal and
//! watches the whole chrome re-theme in one frame (the token editor's
//! whole argument), then flips it back to **Purple — the brand's ramp,
//! the same pair the product page carries** (`#7E5CE8 → #B491FF`,
//! quoted from `viewwsite`). The film's receipt chips are the palette
//! itself, hex by hex.

use vieww_widget::prelude::*;
use vieww_widget::WidgetNode;
use super::{caption, clamp01, studio_chrome, ACCENT, ACCENT_DEEP, Ctx, SYN_TYPE};

/// The script's moments, absolute film seconds.
const TOKENS_T: f32 = 151.0;
const TEAL_T: f32 = 154.5;
const PURPLE_T: f32 = 156.2;

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;

    let mut stack = studio_chrome(ctx);

    // The palette receipt — the brand's own hexes, riding the caption
    // band. The accent flip's annotation: two swatch chips that swap
    // when the script does.
    let flip = abs >= TEAL_T && abs < PURPLE_T;
    let appear = clamp01((abs - TOKENS_T - 0.4) / 0.4);
    if appear > 0.01 {
        let near = if flip { super::TEAL_SWATCH } else { ACCENT_DEEP };
        let far = if flip { super::TEAL_SWATCH_2 } else { ACCENT };
        let near_hex = if flip { "#0E8174" } else { "#7E5CE8" };
        let far_hex = if flip { "#2FBFAE" } else { "#B491FF" };
        let swatch = super::Painting::sized(
            super::Size::new(560.0, 46.0),
            super::PaintWith::new(move |book: &mut super::Sketchbook, _s: super::Size| {
                book.rrect(super::xywh(0.0, 2.0, 560.0, 40.0), 9.0, super::alpha(super::SURFACE_2, 0.94));
                book.stroke_rrect(super::xywh(0.0, 2.0, 560.0, 40.0), 9.0, super::alpha(ACCENT, 0.30), 1.2);
                // The swatches.
                book.rrect(super::xywh(12.0, 12.0, 20.0, 20.0), 5.0, near);
                book.rrect(super::xywh(38.0, 12.0, 20.0, 20.0), 5.0, far);
                // The ramp between them.
                book.rect(
                    super::xywh(64.0, 20.0, 40.0, 4.0),
                    vieww_foundation::Gradient::horizontal().with_dither().with_stops(&[
                        (0.0, near),
                        (1.0, far),
                    ]),
                );
            }),
        );
        stack = stack.push(
            super::Positioned::new()
                .left(1010.0)
                .top(852.0)
                .width(560.0)
                .height(46.0)
                .child(super::Opacity::new(appear).child(swatch)),
        );
        stack = stack.push(
            super::Positioned::new()
                .left(1132.0)
                .top(860.0)
                .width(430.0)
                .height(26.0)
                .child(
                    vieww_widget::Text::new(format!("{near_hex} → {far_hex} · the brand ramp, from viewwsite"))
                        .style(super::geist_mono(13.5).letter_spacing(1.0).color(super::alpha(super::MUTED, 0.95)))
                        .align(vieww_foundation::TextAlign::Left),
                ),
        );
    }

    // The captions — the palette's beats.
    stack = stack.push(caption(
        "the token editor — every colour the studio draws, as a list",
        1002.0,
        clamp01((abs - TOKENS_T) / 0.25),
    ));
    if abs >= TEAL_T {
        stack = stack.push(caption(
            "one accent, one frame — the whole chrome re-themes",
            966.0,
            clamp01((abs - TEAL_T) / 0.15),
        ));
    }
    if abs >= PURPLE_T {
        stack = stack.push(caption(
            "…and back to the brand's — the pair the product page wears",
            930.0,
            clamp01((abs - PURPLE_T) / 0.15),
        ));
    }

    stack.into()
}
