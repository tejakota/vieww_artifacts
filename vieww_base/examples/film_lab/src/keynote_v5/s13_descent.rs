//! S13 · THE DESCENT — say → rust, state held. 2:00–2:08.
//!
//! The reference's promise that vieww "computes and draws its own canvas"
//! extends to its own tooling: the say buffer *descends* to the generated
//! Rust — a scan line crosses the editor and every line flips to its
//! `// say: file:line`-annotated counterpart — while the preview never
//! stutters: the counter holds, the spring keeps swinging, the session
//! clock never rebuilds. A doorway, not a wall.
//!
//! Tap 5 (the descent, t≈0.45) fires here.

use vieww_foundation::{Color, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{alpha, clamp01, ease_in_out, ease_out_cubic, mix, tint, xywh, INK, MUTED, VIOLET, VIOLET_SOFT};

use super::studio;
use super::{Ctx};

/// The sweep's window (scene fraction).
const SWEEP_T0: f32 = 0.14;
const SWEEP_T1: f32 = 0.72;

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;
    let ladder = ctx.ladder;

    // The sweep — ease-in-out across the editor.
    let sweep = ease_in_out(clamp01((t - SWEEP_T0) / (SWEEP_T1 - SWEEP_T0)));

    // The tab flips at the half-point.
    let spec = studio::Spec {
        code: studio::Code::Rust { sweep },
        app: {
            let mut app = studio::App::new(1, super::tap_pulse(abs), abs);
            app.dial = 1.0;
            app.spring = 1.0;
            app
        },
        session_line: 1.0,
        ..Default::default()
    };

    let mut stack = Stack::new()
        .push(Positioned::fill().child(studio::studio(abs, ladder, spec)));

    // The state-held receipt — a chip pinned to the preview: the counter
    // and the spring's phase, both carried across the descent (their
    // values are read from the same clocks the app renders by).
    let receipt_a = clamp01((t - SWEEP_T0 - 0.10) / 0.14);
    if receipt_a > 0.0 {
        let counter = 1; // the witness — never reset through the morph
        let spring_phase = (abs * 2.4).sin();
        stack = stack.push(
            Positioned::new()
                .left(studio::PV_X0 + 70.0)
                .top(studio::TITLE_H + 120.0)
                .width(460.0)
                .height(44.0)
                .child(Opacity::new(receipt_a).child(super::chip(
                    format!("count {} · spring {:+.2} — held", counter, spring_phase),
                    15.0,
                    tint(VIOLET_SOFT, 0.15),
                ))),
        );
    }

    // The captions.
    stack = stack.push(super::caption(
        "say descends to rust — the same buffer, one codegen away",
        1000.0,
        clamp01((t - 0.06) / 0.12),
    ));
    stack = stack.push(super::caption(
        "a doorway, not a wall — state survives the switch",
        964.0,
        clamp01((t - 0.55) / 0.12),
    ));

    stack.into()
}
