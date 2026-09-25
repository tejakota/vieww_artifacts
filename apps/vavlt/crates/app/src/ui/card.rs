//! Surfaces: the card, the tint, the rule, the chip.
//!
//! All four are one `Container` with different numbers, and they are named
//! separately because the *decision* is which one to reach for. A tinted card
//! is a notice; a plain card is a group; a rule is what you use when neither
//! space nor a surface change will do.

use vieww::prelude::*;

use crate::theme::VavltTheme;

/// A group of rows on its own surface.
pub fn card(theme: &VavltTheme, child: impl Into<WidgetNode>) -> WidgetNode {
    Container::new()
        .color(theme.colors.card)
        .radius(20.0)
        .child(child)
        .into()
}

/// A notice that carries a semantic colour — a picker error, a caveat.
///
/// The padding is baked in because a tinted card is always a paragraph, and the
/// one at the call site was wrong on two screens.
pub fn tinted_card(
    theme: &VavltTheme,
    tint: Color,
    text: impl Into<String>,
    ink: Color,
) -> WidgetNode {
    Container::new()
        .color(tint)
        .radius(20.0)
        .padding(EdgeInsets::all(theme.metrics.sp_2))
        .child(Text::new(text).style(theme.caption().color(ink)))
        .into()
}

/// One hairline. The last resort, after space and after a surface change.
pub fn divider(theme: &VavltTheme) -> WidgetNode {
    Container::new().height(1.0).color(theme.colors.line).into()
}

/// What a chip is saying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipTone {
    Neutral,
    Accent,
    Warning,
    Destructive,
    Success,
}

/// A pill. Used for a file's state and its class, and nothing else — a chip
/// that is not a status is a button that forgot to look like one.
pub fn chip(theme: &VavltTheme, label: impl Into<String>, tone: ChipTone) -> WidgetNode {
    let colors = theme.colors;
    let (fill, ink) = match tone {
        ChipTone::Accent => (colors.accent_tint, colors.accent_txt),
        ChipTone::Warning => (colors.warning_tint, colors.warning_txt),
        ChipTone::Destructive => (colors.destructive_tint, colors.destructive_txt),
        ChipTone::Success => (colors.hover, colors.success_txt),
        ChipTone::Neutral => (colors.hover, colors.dim),
    };

    Container::new()
        .height(28.0)
        .color(fill)
        // A stadium, not a rounded rectangle: the radius is half the height, so
        // a chip that grows a line still reads as a pill.
        .radius(14.0)
        .padding(EdgeInsets::symmetric(11.0, 0.0))
        .alignment(Alignment::CENTER)
        .child(Text::new(label).style(theme.meta().color(ink)))
        .into()
}
