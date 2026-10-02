//! S09 · DAMAGE IN PIXELS — one write, measured. 1:40–1:51.
//!
//! One write lights one region (E-10): the button's own bounds glow as
//! the damage rect, its area computed from the same geometry the widget
//! lays out with — **ms · px** on screen, the ms from the census probe's
//! sampled medians at master resolution, the px from the probe rect.
//! Measured, not typed.
//!
//! The PerformanceOverlay strip runs above the preview: frame bars and a
//! damage-area history — the receipts, not the fireworks. Tap → **5**.

use vieww_widget::prelude::*;

use crate::film_lib::clamp01;

use super::studio::{button_rect, studio, App, Code, Spec};
use super::{caption, group_commas, Ctx};

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;

    // The write lands at 0.18 — one region lights.
    let write_t = 0.18f32;
    let lit = clamp01((t - write_t) / 0.05);

    // The damage area — computed from the button's own rect (derived).
    let r = button_rect();
    let area_px = (r.width() * r.height()).round() as u32;

    let mut app = App::new(ctx.ladder, super::tap_pulse(abs), abs);
    app.alive = 1.0;
    app.dial = 1.0;
    app.spring = 1.0;

    let spec = Spec {
        code: Code::Say {
            typed: 1.0,
            blink: ctx.sec,
        },
        app,
        damage: if lit > 0.0 {
            Some((
                r,
                format!("{} px · one write", group_commas(area_px as u64)),
            ))
        } else {
            None
        },
        perf_strip: true,
        ms: if ctx.probe.frame_ms > 0.0 {
            Some(ctx.probe.frame_ms)
        } else {
            None
        },
        px: Some(area_px),
        session_line: clamp01((abs - 26.0) / 99.0),
        ..Spec::default()
    };

    let mut stack = Stack::new().push(studio(abs, ctx.ladder, spec));

    // The damage rect's own pulse — a soft ring when the write lands.
    if (write_t..write_t + 0.4).contains(&t) {
        let k = clamp01((t - write_t) / 0.4);
        stack = stack.push(Positioned::fill().child(vieww_widget::Painting::sized(
            super::CANVAS,
            vieww_widget::PaintWith::new(
                move |book: &mut vieww_foundation::Sketchbook, _s: vieww_foundation::Size| {
                    let c = vieww_foundation::Offset::new(1209.0, 640.0);
                    book.ring(
                        c,
                        30.0 + 150.0 * k,
                        2.2,
                        crate::film_lib::alpha(super::C_DAMAGE, (1.0 - k) * 0.8),
                    );
                },
            ),
        )));
    }

    stack = stack.push(caption(
        "one write · one region",
        964.0,
        clamp01((t - 0.24) / 0.08),
    ));

    stack.into()
}
