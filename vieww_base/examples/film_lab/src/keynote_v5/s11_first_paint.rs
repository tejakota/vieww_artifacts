//! S11 · FIRST PAINT — the say program, and the preview wakes. 1:38–1:49.
//!
//! The reference's "Step 1 + 2": the clean IDE, a simple view component,
//! and the live render shift. The buffer types itself in **say** — the
//! studio's English-facing screen language, verbatim grammar from
//! `apps/viewwstudio/docs/06-say.md` — and at the last keystroke the
//! preview blooms alive with no reload, no recompile of the world: the
//! skeleton dissolves into the counter, and the receipt prints **alive in
//! N seconds**, N measured by the census probe on this bench (never
//! typed). Then *Add one* is tapped, and the witness counter is born.
//!
//! Taps 1 (first paint, t≈0.58) and 2 (the button, t≈0.86) fire here.

use vieww_foundation::{Offset, Size, Sketchbook};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use crate::film_lib::{alpha, clamp01, ease_out_cubic, spring_out, tint, MINT, VIOLET_SOFT};

use super::studio;
use super::Ctx;

/// The type-on window (scene fraction).
const TYPE_T0: f32 = 0.08;
const TYPE_T1: f32 = 0.56;
/// The first paint (scene fraction) — tap 1.
const PAINT_T: f32 = 0.58;
/// The button tap — tap 2.
const TAP_T: f32 = 0.86;

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;
    let ladder = ctx.ladder;

    // The typing progress — bursts and pauses (the keystroke rhythm).
    let typed = crate::film_lib::clamp01((t - TYPE_T0) / (TYPE_T1 - TYPE_T0));
    let typed_eased = super::ease_out_type(typed);

    // The alive curve: blooms at PAINT_T over ~0.5s.
    let alive = ease_out_cubic(clamp01((t - PAINT_T) / 0.10));
    // The tap flash.
    let flash = super::tap_pulse(abs);

    // The alive receipt — measured by the census (pass 2); in pass 1 the
    // probe is zero and the chip prints a placeholder, honestly.
    let alive_txt = if ctx.probe.alive_seconds > 0.0 {
        format!("alive in {:.2} s", ctx.probe.alive_seconds)
    } else {
        "alive in — s".to_string()
    };

    let mut spec = studio::Spec {
        code: studio::Code::Say {
            typed: typed_eased,
            blink: ctx.sec,
        },
        app: {
            let mut app = studio::App::new(if t >= TAP_T { 1 } else { 0 }, flash, abs);
            app.alive = alive;
            app
        },
        session_line: clamp01((t - 0.04) / 0.30),
        // The receipts — measured by the census (frame_ms) and derived from
        // the button's own geometry (px) — never typed.
        ms: (alive >= 1.0 && ctx.probe.frame_ms > 0.0).then_some(ctx.probe.frame_ms),
        px: (alive >= 1.0).then_some({
            let br = studio::button_rect();
            (br.width() * br.height()) as u32
        }),
        ..Default::default()
    };

    // The alive receipt chip — pops with the paint.
    if alive >= 1.0 && t < PAINT_T + 0.42 {
        let pop = spring_out(clamp01((t - PAINT_T - 0.06) / 0.30), 10.0, 0.55);
        let a = clamp01((t - (PAINT_T + 0.34)) / 0.10);
        let rise = (1.0 - pop) * 30.0;
        spec = studio::Spec {
            overlay: Some(
                Stack::new()
                    .push(
                        Positioned::new()
                            .left(studio::PV_X0 + 70.0)
                            .top(studio::TITLE_H + 120.0 + rise - (1.0 - a) * 20.0)
                            .width(340.0)
                            .height(44.0)
                            .child(Opacity::new(a.max(0.01)).child(super::chip(
                                alive_txt,
                                16.0,
                                tint(MINT, 0.1),
                            ))),
                    )
                    .into(),
            ),
            ..spec
        };
    }

    let mut stack = Stack::new().push(Positioned::fill().child(studio::studio(abs, ladder, spec)));

    // The button-tap flourish — a ring blooming from the button when the
    // witness counter is born (tap 2).
    if t >= TAP_T {
        let since = (t - TAP_T) / (1.0 - TAP_T);
        let ring_a = (1.0 - since * 1.4).max(0.0);
        if ring_a > 0.0 {
            let br = studio::button_rect();
            let cx = studio::PV_X0 + br.left + br.width() * 0.5;
            let cy = studio::TITLE_H + 64.0 + br.top - 40.0 + br.height() * 0.5;
            let r = 20.0 + since * 160.0;
            stack = stack.push(Positioned::fill().child(Painting::sized(
                super::CANVAS,
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    book.ring(
                        Offset::new(cx, cy),
                        r,
                        2.4,
                        alpha(VIOLET_SOFT, ring_a * 0.8),
                    );
                }),
            )));
        }
    }

    // The captions — say's two beats.
    stack = stack.push(super::caption(
        "say what you see — the studio's own screen language",
        1000.0,
        clamp01((t - 0.12) / 0.12),
    ));
    if t > PAINT_T + 0.05 {
        stack = stack.push(super::caption(
            "no reload. no rebuild of the world. it paints.",
            964.0,
            clamp01((t - PAINT_T - 0.10) / 0.12),
        ));
    }

    stack.into()
}
