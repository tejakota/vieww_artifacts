//! S19 · DEVTOOLS MIRROR — the studio, inspected by itself.
//! 2:49–2:57.
//!
//! The close of the act: the script reopens the live demo, switches on
//! the studio's own inspectors — `show_damage`, `show_semantics` — and
//! edits the buffer live. The damage rects bloom on the real edits; the
//! semantics overlay outlines the real tree; the inspector's grammar
//! eats the studio's own chrome. The product auditing the product: the
//! film's whole honesty discipline, running inside the thing it
//! films.

use vieww_widget::prelude::*;
use vieww_widget::WidgetNode;
use super::{caption, clamp01, studio_chrome, Ctx, ACCENT, SYN_TYPE};

/// The script's moments, absolute film seconds.
const MIRROR_T: f32 = 170.0;
const DAMAGE_T: f32 = 171.5;
const SEMANTICS_T: f32 = 173.0;
const EDIT_T: f32 = 174.2;

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;

    let mut stack = studio_chrome(ctx);

    // The inspector chips — which overlays are on, live.
    let damage_on = abs >= DAMAGE_T && abs < 175.5;
    let semantics_on = abs >= SEMANTICS_T && abs < 176.2;
    let damage_label = format!("show_damage: {}", if damage_on { "on" } else { "off" });
    let semantics_label = format!("show_semantics: {}", if semantics_on { "on" } else { "off" });
    stack = stack.push(super::receipt_row(
        &[
            (damage_label.as_str(), if damage_on { ACCENT } else { super::MUTED }),
            (semantics_label.as_str(), if semantics_on { SYN_TYPE } else { super::MUTED }),
        ],
        clamp01((abs - DAMAGE_T) / 0.3),
    ));

    // The captions — the mirror's beats.
    stack = stack.push(caption(
        "the mirror — the studio, inspected by itself",
        1002.0,
        clamp01((abs - MIRROR_T) / 0.20),
    ));
    if abs >= DAMAGE_T {
        stack = stack.push(caption(
            "the damage overlay again — this time on the live sketch",
            966.0,
            clamp01((abs - DAMAGE_T) / 0.15),
        ));
    }
    if abs >= SEMANTICS_T {
        stack = stack.push(caption(
            "and the semantics tree — the UI, outlined as meaning",
            930.0,
            clamp01((abs - SEMANTICS_T) / 0.15),
        ));
    }
    if abs >= EDIT_T {
        stack = stack.push(caption(
            "an edit lands. the rects bloom. the product audits the product",
            894.0,
            clamp01((abs - EDIT_T) / 0.15),
        ));
    }

    stack.into()
}
