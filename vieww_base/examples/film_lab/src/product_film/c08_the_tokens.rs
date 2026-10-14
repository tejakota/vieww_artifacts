//! C08 · THE TOKENS — one accent, the whole product re-themed. 3:30–3:40.
//!
//! The token editor: the studio's design tokens, live. The film swaps
//! the accent — **Teal** — and the *whole studio* re-themes: the
//! activity bar, the selection washes, the Render button's ramp, the
//! focus ring, the mark in the title bar. Then back to the brand's
//! **Purple**. One line to change it, because a token system that
//! needs nineteen edits is not a token system.
//!
//! The audience has now seen the studio: edit, preview, compile,
//! carry, platform, theme. The gap line has been spent. What is left
//! is the proof — and the invitation.

use vieww_foundation::TextAlign;
use vieww_widget::prelude::*;
use vieww_widget::WidgetNode;

use super::{alpha, caption, chip_row, clamp01, studio_chrome, xywh, Ctx, LEDGER, MUTED, W};

/// The accent swaps' film-times (the script's own).
const TEAL_AT: f32 = 214.5;
const PURPLE_AT: f32 = 216.2;

/// The accents, as the token editor sees them (the studio's own
/// `ACCENTS` table, quoted).
const ACCENTS: [(&str, [u8; 3], f32); 4] = [
    ("Purple", [0x7E, 0x5C, 0xE8], 0.0),
    ("Teal", [0x0E, 0x81, 0x74], TEAL_AT),
    ("Amber", [0xA0, 0x5F, 0x08], 0.0),
    ("Rose", [0xC0, 0x28, 53], 0.0),
];

pub(super) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = studio_chrome(ctx);

    // The headline.
    let head_a = clamp01((t - 0.04) / 0.12);
    if head_a > 0.01 {
        stack = stack.push(
            vieww_widget::Positioned::new()
                .left(0.0)
                .top(186.0)
                .width(W)
                .height(40.0)
                .child(
                    vieww_widget::Opacity::new(head_a).child(
                        vieww_widget::Text::new("one accent, the whole product re-themed")
                            .style(
                                super::geist(26.0)
                                    .letter_spacing(1.6)
                                    .color(alpha(super::INK, 0.95)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }

    // The swatch rail — the studio's own accents as swatches, the
    // current one ringed. The film's script swaps Teal in and Purple
    // back; the rail shows which is live.
    let rail_a = clamp01((t - 0.10) / 0.1);
    if rail_a > 0.01 {
        for (i, (name, rgb, at)) in ACCENTS.iter().enumerate() {
            let is_teal_event = ctx.abs >= *at && ctx.abs < PURPLE_AT && *at > 0.0;
            let is_current = if is_teal_event {
                name == &"Teal"
            } else {
                name == &"Purple"
            };
            let x = W * 0.5 - 2.0 * 110.0 + i as f32 * 110.0;
            let color = vieww_foundation::Color::rgb(rgb[0], rgb[1], rgb[2]);
            stack = stack.push(
                vieww_widget::Positioned::new()
                    .left(x)
                    .top(240.0)
                    .width(96.0)
                    .height(96.0)
                    .child(vieww_widget::Opacity::new(rail_a).child(Painting::sized(
                        Size::new(96.0, 96.0),
                        PaintWith::new(move |book: &mut vieww_foundation::Sketchbook, _s: Size| {
                            // The swatch — the accent's ramp, as the
                            // studio draws its own buttons.
                            book.rrect(
                                xywh(14.0, 14.0, 68.0, 68.0),
                                14.0,
                                vieww_foundation::Gradient::vertical()
                                    .with_stops(&[(0.0, color), (1.0, alpha(color, 0.75))]),
                            );
                            // The current one's ring.
                            if is_current {
                                book.ring(
                                    Offset::new(48.0, 48.0),
                                    44.0,
                                    2.4,
                                    alpha(super::INK, 0.9),
                                );
                            }
                        }),
                    ))),
            );
            // The name.
            stack = stack.push(
                vieww_widget::Positioned::new()
                    .left(x)
                    .top(344.0)
                    .width(96.0)
                    .height(24.0)
                    .child(
                        vieww_widget::Opacity::new(rail_a).child(
                            vieww_widget::Text::new(name.to_string())
                                .style(super::geist_mono(13.0).letter_spacing(1.4).color(alpha(
                                    if is_current { super::INK } else { MUTED },
                                    if is_current { 0.95 } else { 0.7 },
                                )))
                                .align(TextAlign::Center),
                        ),
                    ),
            );
        }
    }

    // The swap callout — fires with each scripted swap.
    for (at, text) in [
        (TEAL_AT, "accent → teal — the studio repaints itself"),
        (PURPLE_AT, "accent → purple — and home again"),
    ] {
        let since = ctx.abs - at;
        if (0.0..2.6).contains(&since) {
            let a = clamp01(since / 0.3) * (1.0 - clamp01((since - 1.8) / 0.8));
            stack = stack.push(
                vieww_widget::Positioned::new()
                    .left(0.0)
                    .top(400.0)
                    .width(W)
                    .height(30.0)
                    .child(
                        vieww_widget::Opacity::new(a).child(
                            vieww_widget::Text::new(text)
                                .style(
                                    super::geist_mono(16.0)
                                        .letter_spacing(1.6)
                                        .color(alpha(LEDGER, 0.95)),
                                )
                                .align(TextAlign::Center),
                        ),
                    ),
            );
        }
    }

    // The captions.
    stack = stack.push(caption(
        "the token editor is live: the studio re-themes as you pick",
        1002.0,
        clamp01((t - 0.05) / 0.12),
    ));
    stack = stack.push(caption(
        "one line to change it — that is the whole point of tokens",
        966.0,
        clamp01((t - 0.6) / 0.12),
    ));

    // The receipt chips.
    stack = stack.push(chip_row(
        &[
            ("19 surfaces, 1 edit", LEDGER),
            ("the mark itself", MUTED),
            ("the brand is back", super::ACCENT),
        ],
        // The editor's empty lower band — see `receipt_row`. Pinned
        // right, these rows rendered straight through the preview
        // pane's description paragraph in every studio scene.
        360.0,
        924.0,
        clamp01((sec - 1.0) / 0.5),
    ));

    stack.into()
}
