//! The scrim behind every popup.
//!
//! Mirrors `prototype/screens/landing/backdrop-blur.css`.
//!
//! vieww mapping: `ModalBarrier` (`controls/navigator.rs`), over a
//! `vieww_effects::BackdropBlur` when the route is frosted.
//!
//! The prototype split "what a barrier is made of" from "where the modal sits",
//! and that split is worth keeping: the picker, the progress card and the
//! compare sheet all reuse this, and the only thing that changes between them is
//! how deep the wash is.
//!
//! # The blur is real now
//!
//! The prototype blurred the screen behind each popup with a CSS
//! `backdrop-filter`, and the original vieww could not follow: it had no
//! backdrop filter, and faking one meant rendering the body to an offscreen
//! layer, blurring that, and compositing it back under the barrier — three
//! extra passes over a full screen, every frame a popup was up, against a
//! 16.67ms budget. The honest trade then was a deeper scrim, which is what
//! the progress and compare routes still use deliberately: nothing behind
//! them is worth reading, so hiding beats frosting.
//!
//! The migrated framework ships the real thing — `vieww-effects`
//! `BackdropBlur` samples the destination pixels already painted beneath the
//! widget, blurs and tints *that* copy, and only then lets anything above
//! paint on top of the result. The picker takes it: the grid is what the user
//! is about to act on, and "legible but pushed back" is exactly what
//! `backdrop-blur.css` asked for. `blur: 0.0` keeps the plain-scrim path the
//! other two routes want, so this stays one widget with a dial rather than
//! two widgets.

use std::rc::Rc;

use vieww_effects::{BackdropBlur, BackdropFilter};
use vieww_foundation::Color;
use vieww_widget::prelude::*;
use vieww_widget::widget_node_from;

/// A tappable scrim that fills the route and dismisses it.
pub struct Backdrop {
    pub color: Color,
    /// Backdrop-blur sigma in logical pixels. `0.0` is the plain scrim the
    /// deep routes want; anything else frosts whatever the route beneath
    /// painted. The color still rides on top of the blur, so a frosted
    /// route keeps its wash — shallower than the plain one, because the
    /// blur is doing half the pushing-back now.
    pub blur: f32,
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
            blur: 0.0,
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
            blur: 0.0,
            on_dismiss: None,
        }
    }

    /// A dismissable scrim that also blurs whatever the route beneath it
    /// painted — the frosted glass the prototype's `backdrop-blur.css` asked
    /// for.
    ///
    /// `blur` is the sigma in logical pixels; 18–24 is the range a phone
    /// dialog usually wants. The `color` still paints over the blur, so pass
    /// a shallower wash than the plain path — the blur carries half the
    /// recession.
    #[must_use]
    pub fn frosted(color: Color, blur: f32, on_dismiss: Rc<dyn Fn()>) -> Self {
        Self {
            color,
            blur,
            on_dismiss: Some(on_dismiss),
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
        let barrier = match &self.on_dismiss {
            Some(dismiss) => {
                let dismiss = Rc::clone(dismiss);
                barrier.on_dismiss(move || dismiss()).into()
            }
            None => barrier.into(),
        };

        // The plain path, unchanged: one barrier, one wash.
        if self.blur <= 0.0 {
            return barrier;
        }

        // The frosted path. The blur sits *under* the barrier: it samples the
        // route beneath — the grid, mid-conversation — while the barrier
        // above it keeps the two jobs it already had, the wash and the tap.
        // Keeping them stacked this way round means the frosted route is the
        // plain one with a layer slid underneath, not a different widget —
        // the tap semantics and the scrim depth are decided in exactly one
        // place, `ModalBarrier`, for both.
        Stack::new()
            .fit(StackFit::Expand)
            .push(
                // A transparent tint: the wash is the barrier's job, and the
                // blur's job is only the blur. `BackdropFilter`'s own
                // `frosted`/`dark` presets would double-ink the wash.
                BackdropBlur::new(BackdropFilter {
                    blur: self.blur,
                    tint: Color::TRANSPARENT,
                }),
            )
            .push(barrier)
            .into()
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
