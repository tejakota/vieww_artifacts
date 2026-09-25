//! exp_hero4k — *the budget line at 3840×2160.*
//!
//! The hero frame's own tree, root-scaled ×2 through `Transformed`, rendered
//! at true 4K — the canvas-size tolerance axis, and the receipt that answers
//! "can the master go bigger?". Every geometric edge lands on a 4K pixel
//! grid; the supersampling runs at device resolution; the ms/frame number
//! in the metrics is the honest cost of quadruple the pixels.

use vieww_foundation::Transform;
use vieww_widget::prelude::*;
use vieww_widget::Transformed;

use crate::exp_hero;

/// Film-time this experiment spans (same as the hero).
pub const SECONDS: f32 = exp_hero::SECONDS;

/// The frame: the hero tree, doubled.
pub fn frame(t: f32) -> WidgetNode {
    Transformed::new(Transform::scale(2.0, 2.0))
        .child(exp_hero::frame(t))
        .into()
}
