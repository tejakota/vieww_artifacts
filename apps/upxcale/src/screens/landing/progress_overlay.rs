//! The rendering overlay.
//!
//! Mirrors `prototype/screens/landing/progress-overlay.css`.
//!
//! vieww mapping: `CircularProgress` (indeterminate) + `LinearProgress`
//! (determinate) + `Container` + `Flex` + `Text`, over a blocking
//! [`Backdrop`](super::backdrop).
//!
//! # Two indicators, and each says something different
//!
//! The ring is **indeterminate** and the bar is **determinate**, and that is not
//! decoration. The ring says "the worker is alive"; the bar says "this much of
//! the work is done". They come from different facts, so they are different
//! widgets.
//!
//! The number under the bar is real. `RenderState` counts stages the worker has
//! actually reported finishing — two per photograph, the resample and the
//! sharpen — so the bar cannot reach 90% and sit there, and it cannot reach 100%
//! before the pixels exist. The prototype's bar was a `setInterval` adding a
//! random amount, which is the correct thing for a prototype and a lie in an
//! app.
//!
//! vieww makes determinate and indeterminate the same widget with and without a
//! value, so a screen that starts one way and ends the other does not swap
//! widgets to say so.

use vieww_foundation::{Alignment, Constraints, EdgeInsets, Size};
use vieww_widget::prelude::*;
use vieww_widget::widget_node_from;

use crate::state::RenderState;
use crate::theme;

/// The widest the card may be. The prototype used `min(80%, 280px)`; at the
/// 414pt surface this app targets, 280 is the binding half of that.
const CARD_WIDTH: f32 = 280.0;

/// The centred card.
#[derive(Debug)]
pub struct ProgressOverlay {
    pub state: RenderState,
}

impl Widget for ProgressOverlay {
    fn debug_name(&self) -> &'static str {
        "ProgressOverlay"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        let theme_data = *ThemeData::of(ctx);
        let gap = theme_data.metrics.gap;
        let corner = theme_data.metrics.corner * 2.8;
        let state = &self.state;

        let card = Container::new()
            .decoration(
                BoxDecoration::new()
                    .color(theme_data.colors.surface_variant)
                    .radius(corner)
                    // The picker's `CARD_SHADOW`, verbatim: both cards float,
                    // so both cast the same shadow. One elevation language —
                    // see `theme.rs`.
                    .shadow(theme::CARD_SHADOW),
            )
            .padding(EdgeInsets::symmetric(gap * 3.0, gap * 4.0))
            .child(
                Flex::column()
                    .main_axis_size(MainAxisSize::Min)
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .spacing(gap * 2.0)
                    .children(children![
                        // Alive, not progressing — the ring never claims to
                        // know how far along the current photograph is.
                        CircularProgress::indeterminate()
                            .size(52.0)
                            .thickness(4.0)
                            .label("Upscaling"),
                        Text::new("Upscaling")
                            .style(theme_data.text.title)
                            .color(theme_data.colors.on_surface)
                            .bold(),
                        // No `.align(TextAlign::Center)` here. The column
                        // already centres its children on the cross axis, and
                        // adding the text-level alignment on top gives the
                        // `Text` a full-width box that it then centres *within*
                        // — the two centrings compose into a visible rightward
                        // offset. The other two labels in this card do not set
                        // it, and they sit correctly; this one did, and did not.
                        Text::new(state.status.clone())
                            .style(theme_data.text.body)
                            .color(theme_data.colors.on_surface_variant),
                        // The bar wants a bounded width; the card's padding
                        // gives it one, but `SizedBox` states it rather than
                        // relying on the column's cross-axis stretch.
                        SizedBox::from_size(Size::new(CARD_WIDTH - gap * 8.0, 4.0)).child(
                            LinearProgress::new(state.fraction())
                                .thickness(4.0)
                                .label("Upscaling progress")
                        ),
                        Text::new(format!("{}%", state.percent()))
                            .style(theme_data.text.label)
                            .color(theme_data.colors.on_surface_variant)
                            .size(12.0)
                            .bold(),
                    ]),
            );

        // The card must be given a maximum width, not left to shrink-wrap. Its
        // widest child is the status line, whose text changes with the batch —
        // unbounded, a long status makes the card grow past the screen and the
        // text paints outside it. Bounding here is also what lets the text
        // wrap, since wrapping needs a width to wrap *to*.
        Align::new(Alignment::CENTER)
            .child(
                Constrained::new(Constraints::new(0.0, CARD_WIDTH, 0.0, f32::INFINITY)).child(
                    Semantics::new()
                        .role(SemanticRole::ProgressBar)
                        .label("Upscaling photos")
                        .value(format!("{} percent", state.percent()))
                        .live(SemanticLiveness::Polite)
                        .child(card),
                ),
            )
            .into()
    }
}

widget_node_from!(ProgressOverlay);
