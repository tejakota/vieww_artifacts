//! The scrim behind every popup.
//!
//! Mirrors `prototype/screens/landing/backdrop-blur.css`.
//!
//! vieww mapping: `ModalBarrier` (`controls/navigator.rs`).
//!
//! The prototype split "what a barrier is made of" from "where the modal sits",
//! and that split is worth keeping: the picker, the progress card and the
//! compare sheet all reuse this, and the only thing that changes between them is
//! how deep the wash is.
//!
//! # The blur is gone, and that is deliberate
//!
//! The prototype blurred the screen behind each popup with a CSS
//! `backdrop-filter`. vieww has no backdrop filter: `vieww-effects` can blur a
//! *subtree it owns*, but a barrier that blurs whatever happens to be painted
//! behind it is a compositor feature, and the framework's `docs/AIMS.md` does
//! not claim one.
//!
//! Faking it would mean rendering the body to an offscreen layer, blurring that,
//! and compositing it back under the barrier — three extra passes over a full
//! screen, every frame a popup is up, on a device with a 16.67ms budget. The
//! honest trade at this point is a deeper scrim instead: the picker's wash is
//! set so the grid still reads as *there* without competing with the card.
//!
//! If vieww grows a real `BackdropFilter`, this is the one file that changes.

use std::rc::Rc;

use vieww_foundation::Color;
use vieww_widget::prelude::*;
use vieww_widget::widget_node_from;

/// A tappable scrim that fills the route and dismisses it.
pub struct Backdrop {
    pub color: Color,
    /// `None` makes the barrier non-dismissable — what the progress overlay
    /// wants, because there is nothing sensible to do with a half-finished
    /// render.
    pub on_dismiss: Option<Rc<dyn Fn()>>,
}

impl Backdrop {
    #[must_use]
    pub fn dismissable(color: Color, on_dismiss: Rc<dyn Fn()>) -> Self {
        Self {
            color,
            on_dismiss: Some(on_dismiss),
        }
    }

    /// A scrim that absorbs taps and does nothing with them.
    ///
    /// Still a barrier rather than a plain `ColoredBox`: the point is that a tap
    /// aimed at the grid underneath does not reach it.
    #[must_use]
    pub const fn blocking(color: Color) -> Self {
        Self {
            color,
            on_dismiss: None,
        }
    }
}

impl Widget for Backdrop {
    fn debug_name(&self) -> &'static str {
        "Backdrop"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let barrier = ModalBarrier::new().color(self.color);
        match &self.on_dismiss {
            Some(dismiss) => {
                let dismiss = Rc::clone(dismiss);
                barrier.on_dismiss(move || dismiss()).into()
            }
            None => barrier.into(),
        }
    }
}

widget_node_from!(Backdrop);

// `Widget` requires `Debug`, and these structs hold `Rc<dyn Fn>` callbacks,
// which are not. Hand-written rather than derived, and each prints the fields
// that identify *which* instance this is — a tree dump saying `PhotoTile` a
// dozen times is no use, one saying `PhotoTile { id: "a4", upscaled: true }`
// is.
impl std::fmt::Debug for Backdrop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Backdrop")
            .field("color", &self.color)
            .field("dismissable", &self.on_dismiss.is_some())
            .finish()
    }
}
