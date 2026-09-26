//! The floating **Render** button.
//!
//! Mirrors `prototype/screens/landing/render-button.css`.
//!
//! vieww mapping: an *extended* `FloatingActionButton` — composed here from
//! `Pressable` + `Container` + `Flex` + `Icon` + `Text`, because
//! `vieww_widget::FloatingActionButton` is a 56pt circle carrying one icon and
//! has no labelled variant. Its `label()` is the screen-reader string and is
//! never painted, which is the right design for the control it is and the wrong
//! one for this button: "Render" is the screen's primary action and a bare
//! sparkle would not say so.
//!
//! Composing rather than extending is what vieww asks for here — `controls/`
//! exists so an application can write its own control the same way the built-in
//! ones are written, and this is that.
//!
//! # Behaviour
//!
//! The button is visible as soon as the screen has a grid, not gated on a
//! selection. That is the change from the original prototype flow: the app opens
//! to photographs with the button already waiting, and tapping it is what opens
//! the picker. Selection happens *inside* the picker, so this button never
//! carries a count and has no badge.
//!
//! While a render is in flight it is dimmed by `DISABLED_ALPHA` and takes no
//! gestures — vieww's rule is that a control with no handler registers no
//! recogniser, and this mirrors it by not attaching one.

use std::rc::Rc;

use vieww_foundation::{Alignment, EdgeInsets};
use vieww_widget::prelude::*;
use vieww_widget::widget_node_from;

use crate::icons;
use crate::theme;

/// Height of the extended button. Taller than `touch_target` because it is the
/// screen's primary action and reads as a bar rather than as a chip.
const HEIGHT: f32 = 52.0;

/// The button.
pub struct RenderButton {
    /// Dimmed and inert while true.
    pub busy: bool,
    pub on_pressed: Rc<dyn Fn()>,
}

impl Widget for RenderButton {
    fn debug_name(&self) -> &'static str {
        "RenderButton"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        let theme_data = *ThemeData::of(ctx);
        let busy = self.busy;
        let on_pressed = Rc::clone(&self.on_pressed);

        let surface = Pressable::themed(ctx, move |press| {
            // vieww's `pressed_fill` washes `on_primary` into the fill at a
            // fraction of PRESSED_ALPHA. `ColorScheme::pressed` is that exact
            // arithmetic, so the button darkens the same way every other
            // control in the app does rather than by an ad-hoc scale.
            let fill = ColorScheme::pressed(
                theme_data.colors.primary,
                theme_data.colors.on_primary,
                press,
            );

            Container::new()
                .decoration(
                    BoxDecoration::new()
                        .color(fill)
                        .stadium()
                        // The accent-tinted shadow is what lifts it off a grid
                        // of photographs; a neutral one disappears against a
                        // dark tile. Lives in `theme.rs` as `RENDER_SHADOW`
                        // beside the cards' neutral `CARD_SHADOW`, so the
                        // app's whole elevation language is readable in one
                        // place.
                        .shadow(theme::RENDER_SHADOW),
                )
                .height(HEIGHT)
                .padding(EdgeInsets::symmetric(24.0, 0.0))
                .alignment(Alignment::CENTER)
                .child(
                    Flex::row()
                        .main_axis_size(MainAxisSize::Min)
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .spacing(theme_data.metrics.gap)
                        .children(children![
                            Icon::new(icons::sparkles())
                                .size(20.0)
                                .color(theme_data.colors.on_primary),
                            Text::new("Render")
                                .style(theme_data.text.title)
                                .color(theme_data.colors.on_primary)
                                .size(16.0)
                                .bold(),
                        ]),
                )
                .into()
        });

        // No handler while busy — the same "no handler, no recogniser" rule
        // vieww's own `Button` follows, rather than attaching one that returns
        // early.
        let surface = if busy {
            surface
        } else {
            surface.on_tap(move || on_pressed())
        };

        let labelled = Semantics::new()
            .label(if busy {
                "Rendering, please wait"
            } else {
                "Render selected photos"
            })
            .role(SemanticRole::Button)
            .child(surface);

        if busy {
            Opacity::new(theme::DISABLED_OPACITY).child(labelled).into()
        } else {
            labelled.into()
        }
    }
}

widget_node_from!(RenderButton);

// `Widget` requires `Debug`, and these structs hold `Rc<dyn Fn>` callbacks,
// which are not. Hand-written rather than derived, and each prints the fields
// that identify *which* instance this is — a tree dump saying `PhotoTile` a
// dozen times is no use, one saying `PhotoTile { id: "a4", upscaled: true }`
// is.
impl std::fmt::Debug for RenderButton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderButton")
            .field("busy", &self.busy)
            .finish()
    }
}
