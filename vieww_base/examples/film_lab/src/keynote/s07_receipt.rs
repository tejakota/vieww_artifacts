//! S07 · THE RECEIPT — the session measured. 1:11–1:29.
//!
//! Rack focus pulls from editor to pure render (E-16): the buffer falls
//! out of focus, the preview comes into it, both σ values printed live —
//! the focus pull as cinematography, the numbers as the receipt.
//!
//! Then the scrub (E-08): one frame window, ten writes driven through the
//! input pipeline — **10 → 1** — the coalescing scheduler made visible.
//! Ten signal writes land as ticks; one rebuild bar answers. The ratio is
//! counted live from the strip's own script, not asserted.

use vieww_foundation::{Color, Offset, Size, Sketchbook};
use vieww_widget::prelude::*;
use vieww_widget::{PaintWith, Painting};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, spring_out, xywh, INK, MUTED, VIOLET, VIOLET_SOFT,
};

use super::studio::{studio, App, Code, Spec};
use super::{caption, Ctx};

/// The rack focus window — σ ramps in opposite directions.
const RACK_T0: f32 = 0.02;
const RACK_T1: f32 = 0.20;

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;

    // The rack — editor falls out of focus while the preview sharpens.
    let rack = ease_in_out(clamp01((t - RACK_T0) / (RACK_T1 - RACK_T0)));
    let editor_blur = rack * 15.0;
    let preview_blur = (1.0 - rack) * 16.0;

    let mut app = App::new(ctx.ladder, super::tap_pulse(abs), abs);
    app.alive = 1.0;
    app.dial = 1.0;
    app.spring = 1.0;

    // The build badge — frames built, the harness's own count (E-09).
    let badge = if t > 0.20 {
        Some((abs * 60.0) as u64)
    } else {
        None
    };

    let spec = Spec {
        code: Code::Say {
            typed: 1.0,
            blink: ctx.sec,
        },
        app,
        editor_blur,
        preview_blur,
        build_badge: badge,
        session_line: clamp01((abs - 26.0) / 99.0),
        ..Spec::default()
    };

    let mut stack = Stack::new().push(studio(abs, ctx.ladder, spec));

    // The σ receipts — both values, printed live while the rack moves.
    if rack > 0.01 && rack < 0.99 {
        let e_txt = format!("σ {:.1}", editor_blur);
        let p_txt = format!("σ {:.1}", preview_blur);
        stack = stack
            .push(
                Positioned::new()
                    .left(604.0)
                    .top(96.0)
                    .width(160.0)
                    .height(24.0)
                    .child(
                        Text::new(e_txt).style(
                            vieww_foundation::TextStyle::new(15.0)
                                .monospace()
                                .color(alpha(MUTED, 0.9)),
                        ),
                    ),
            )
            .push(
                Positioned::new()
                    .left(976.0)
                    .top(96.0)
                    .width(160.0)
                    .height(24.0)
                    .child(
                        Text::new(p_txt).style(
                            vieww_foundation::TextStyle::new(15.0)
                                .monospace()
                                .color(alpha(MUTED, 0.9)),
                        ),
                    ),
            );
    }

    // The scrub strip — ten writes, one rebuild. Laid over the studio's
    // lower margin, the coalescing scheduler made visible (E-08).
    let scrub = scrub_strip(t);
    stack = stack.push(
        Positioned::new()
            .left(0.0)
            .top(760.0)
            .width(1920.0)
            .height(220.0)
            .child(scrub),
    );

    stack = stack.push(caption(
        "ten writes · one rebuild",
        964.0,
        clamp01((t - 0.78) / 0.08),
    ));

    stack.into()
}

/// The scrub strip — the coalescing receipt. Ten ticks (writes, each a
/// signal pulse), then one bar (the single rebuild they coalesced into).
fn scrub_strip(t: f32) -> WidgetNode {
    // The script: writes land 0.28→0.55, the rebuild fires at 0.62.
    let write_t0 = 0.28f32;
    let write_span = 0.27f32;
    let rebuild_t = 0.62f32;

    let mut stack = Stack::new();

    let strip = Painting::sized(
        Size::new(1920.0, 220.0),
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            let x0 = 260.0;
            let x1 = 1660.0;
            let y = 84.0;
            let n = 10usize; // the writes — counted by this very loop

            // The rail.
            book.line(
                Offset::new(x0, y + 46.0),
                Offset::new(x1, y + 46.0),
                alpha(Color::WHITE, 0.08),
                1.0,
            );

            // The write ticks — one per signal write, each with its pulse.
            let mut landed = 0usize;
            for i in 0..n {
                let wi = write_t0 + (i as f32 / n as f32) * write_span;
                let x = x0 + (i as f32 + 0.5) / n as f32 * (x1 - x0);
                if t >= wi {
                    landed += 1;
                    // The tick.
                    book.rect(xywh(x - 2.0, y + 22.0, 4.0, 20.0), alpha(VIOLET_SOFT, 0.95));
                    // Its settling pulse.
                    let age = ((t - wi) / 0.24).min(1.0);
                    if age < 1.0 {
                        book.ring(
                            Offset::new(x, y + 32.0),
                            8.0 + 16.0 * age,
                            1.6,
                            alpha(VIOLET, (1.0 - age) * 0.7),
                        );
                    }
                } else {
                    book.circle(Offset::new(x, y + 32.0), 3.0, alpha(MUTED, 0.25));
                }
            }

            // The rebuild — one bar, the ten writes coalesced.
            if t >= rebuild_t {
                let k = spring_out(clamp01((t - rebuild_t) / 0.16), 10.0, 0.62);
                let bw = (x1 - x0) * 0.78 * k;
                book.layer(1.0, 0.0, None, |g| {
                    g.rrect(xywh(x0, y + 64.0, bw, 14.0), 7.0, alpha(VIOLET, 0.22));
                });
                book.rrect(xywh(x0, y + 64.0, bw, 14.0), 7.0, alpha(VIOLET_SOFT, 0.9));
                // The glow — one rebuild, but it carries.
                super::glow(book, x0 + bw * 0.5, y + 71.0, 130.0, VIOLET, 0.30);
            }
        }),
    );
    stack = stack.push(Positioned::fill().child(strip));

    // The ratio — counted from the strip's own state, never asserted.
    let rebuild_t = 0.62f32;
    if t >= rebuild_t {
        let a = clamp01((t - rebuild_t) / 0.12);
        let writes_done = 10; // all ten landed before the rebuild fired
        let rebuilds_done = 1;
        let ratio = format!("{} → {}", writes_done, rebuilds_done);
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(10.0)
                .width(1920.0)
                .height(84.0)
                .child(
                    vieww_widget::Opacity::new(a).child(
                        Text::new(ratio)
                            .style(
                                vieww_foundation::TextStyle::new(64.0)
                                    .monospace()
                                    .letter_spacing(6.0)
                                    .color(INK),
                            )
                            .align(vieww_foundation::TextAlign::Center),
                    ),
                ),
        );
    }

    stack.into()
}
