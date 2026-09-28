//! S15 · SAY TO RUST — the language, the compile, the carry.
//! 2:08–2:20.
//!
//! The film opens `counter.say` (the file-tree way — `open_path`) and
//! types the whole program in bursts: *keep a whole number called count
//! starting at 0 … a button "Add one" which when tapped: add 1 to
//! count.* Then the real Render: **say-codegen → counter.rs → studio
//! rustc → cdylib → dlopen → preview.** The studio shows its real
//! Compiling state for its scripted frames; the settle is off-clock;
//! the compiled counter mounts — and the film taps *Add one* through
//! the real pipeline: 0 → 1.
//!
//! Then the twist that makes the language honest: the film edits one
//! word of the program and renders again — the screen remounts, and the
//! `keep`ed count **survives the recompile**. The label still says
//! "Tapped 1 times".
//!
//! Witness taps 5 (Render, 134.2 s) and 6 (Add one, 136.62 s) fire here.

use vieww_widget::prelude::*;
use vieww_widget::WidgetNode;
use super::{caption, clamp01, spark, studio_chrome, tap_ring_at, tint, ACCENT, Ctx};

/// The script's moments, in absolute film seconds.
const OPEN_T: f32 = 129.0;
const RENDER_T: f32 = 134.2;
const SETTLE_T: f32 = 135.0;
const TAP_T: f32 = 136.62;
const CARRY_RENDER_T: f32 = 138.1;
const CARRY_SETTLE_T: f32 = 139.0;

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;

    let mut stack = studio_chrome(ctx);

    // The spark — rides the Render button's corner while the compile
    // runs (the light working), then settles into the preview.
    let compiling = abs >= RENDER_T && abs < SETTLE_T;
    let done = abs >= SETTLE_T;
    if compiling || done {
        let (sx, sy, r) = if compiling {
            (1794.0, 158.0, 9.0)
        } else {
            (1780.0, 150.0, 8.0)
        };
        let a = if compiling { 1.0 } else { 0.85 };
        let orb_abs = ctx.abs;
        let orb = super::Painting::sized(
            super::Size::new(110.0, 110.0),
            super::PaintWith::new(move |book: &mut super::Sketchbook, _s: super::Size| {
                spark(book, 55.0, 55.0, r, orb_abs, a, ACCENT);
            }),
        );
        stack = stack.push(
            super::Positioned::new()
                .left(sx - 55.0)
                .top(sy - 55.0)
                .width(110.0)
                .height(110.0)
                .child(orb),
        );
    }

    // The pipeline chips — the compile's receipts, staggered to arrive
    // with the Render and land as each stage completes (the film's
    // spelling of the studio's own pipeline).
    if abs >= RENDER_T - 0.02 {
        let appear = clamp01((abs - RENDER_T + 0.02) / 0.4);
        stack = stack.push(super::receipt_row(
            &[
                ("say-codegen", super::SYN_TYPE),
                ("studio rustc", super::SYN_MACRO),
                ("cdylib · dlopen", super::SYN_KEYWORD),
                ("preview", ACCENT),
            ],
            appear,
        ));
    }

    // The tap ring — the Add one tap's annotation.
    if abs >= TAP_T {
        stack = stack.push(tap_ring_at(super::script::TAP_ADD_ONE, abs - TAP_T));
    }

    // The carry badge — after the second render: the count survived.
    if abs >= CARRY_SETTLE_T + 0.1 {
        let pop = clamp01((abs - CARRY_SETTLE_T - 0.1) / 0.25);
        let badge = chip_carry(pop);
        stack = stack.push(
            super::Positioned::new()
                .left(1010.0)
                .top(848.0 - (1.0 - pop) * 14.0)
                .width(460.0)
                .height(42.0)
                .child(badge),
        );
    }

    // The captions — the compile's beats.
    stack = stack.push(caption(
        "say — the studio's English screen language, typed in bursts",
        1002.0,
        clamp01((abs - OPEN_T) / 0.30),
    ));
    if abs >= RENDER_T - 0.05 {
        stack = stack.push(caption(
            "render: say-codegen → rust → a real cdylib, loaded live",
            966.0,
            clamp01((abs - RENDER_T + 0.05) / 0.15),
        ));
    }
    if abs >= SETTLE_T + 0.05 {
        stack = stack.push(caption(
            "compiled and mounted — tapped: 0 → 1",
            930.0,
            clamp01((abs - SETTLE_T - 0.05) / 0.12),
        ));
    }
    if abs >= CARRY_RENDER_T {
        stack = stack.push(caption(
            "edit one word, render again — the kept count rides the remount",
            894.0,
            clamp01((abs - CARRY_RENDER_T) / 0.15),
        ));
    }

    stack.into()
}

/// The carry badge — the state's receipt: "Tapped 1 times", still.
fn chip_carry(pop: f32) -> vieww_widget::WidgetNode {
    use vieww_widget::prelude::*;
    vieww_widget::Opacity::new(pop.max(0.01)).child(
        Container::new()
            .color(super::alpha(super::SURFACE_2, 0.92))
            .radius(8.0)
            .border(vieww_foundation::Border::new(super::alpha(super::MINT, 0.4), 1.2))
            .padding(vieww_foundation::EdgeInsets::symmetric(8.0, 13.0))
            .child(
                Text::new("count = 1 · across the recompile")
                    .style(super::geist_mono(15.0).letter_spacing(1.2).color(tint(super::MINT, 0.12))),
            ),
    )
    .into()
}

