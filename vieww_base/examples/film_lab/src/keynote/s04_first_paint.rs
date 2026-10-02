//! S04 · FIRST PAINT — Ki's close, and the door. 0:26–0:40.
//!
//! The studio arrives: a buffer `counter.say`, three lines of say typed
//! before your eyes. The preview blooms alive — frosted glass, the counter
//! app, real springs. **First shown code of a Rust framework: `say`** —
//! that's K0, and it needs no caption; the buffer tab says it.
//!
//! The caption the honesty ledger fought for: **"alive in N seconds"** —
//! N measured by the census probe on this bench (the say pipeline compiles
//! — codegen → cdylib → dlopen — the studio escapes *your project's*
//! build, not compilation itself; the dropped phrase stays dropped).
//!
//! Tap → the counter reads **1**. The session chip and the session line
//! are born here and never leave.

use vieww_foundation::{Offset, Size, Sketchbook};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use crate::film_lib::{alpha, clamp01, smoothstep, xywh, VIOLET};

use super::studio::{studio, App, Code, Spec};
use super::{caption, Ctx};

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;

    // The studio's entrance — a door opening: light spills in from behind
    // the IDE as it fades up. The wait ends not with an argument but with
    // an entrance.
    let enter = smoothstep(clamp01(t / 0.055));

    // The buffer types: say, three lines and then the rest, at a human
    // rhythm (E-02's bursts-and-pauses).
    let typed = clamp01((t - 0.04) / 0.50);
    let code = Code::Say {
        typed,
        blink: ctx.sec,
    };

    // The preview blooms alive at 0.30 — the film's first product shot.
    let alive = smoothstep(clamp01((t - 0.30) / 0.11));

    let mut app = App::new(ctx.ladder, super::tap_pulse(abs), abs);
    app.alive = alive;

    let spec = Spec {
        code,
        app,
        session_line: clamp01((abs - 26.0) / 99.0),
        ..Spec::default()
    };

    let mut stack = Stack::new().push(Opacity::new(enter).child(studio(abs, ctx.ladder, spec)));

    // The light spill — the door, as light: a wide violet wash behind the
    // studio that recedes as the IDE becomes the world.
    if enter < 1.0 {
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let a = (1.0 - enter) * 0.5;
                book.layer(1.0, 60.0, None, |g| {
                    g.rect(xywh(0.0, 0.0, 1920.0, 1080.0), alpha(VIOLET, a * 0.30));
                });
            }),
        )));
    }

    // The caption — N measured, never typed (ledger 11).
    let n = ctx.probe.alive_seconds;
    let caption_txt = if n > 0.0 {
        format!("alive in {:.2} seconds", n)
    } else {
        "alive in … seconds".to_string()
    };
    stack = stack.push(caption(&caption_txt, 964.0, clamp01((t - 0.36) / 0.10)));

    // The tap moment — one human touch, the counter is born. The button
    // wears its own receipt: a bloom from the preview's counter node.
    if ctx.ladder >= 1 {
        let pulse = super::tap_pulse(abs);
        if pulse > 0.01 {
            stack = stack.push(Positioned::fill().child(Painting::sized(
                super::CANVAS,
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    let c = Offset::new(1405.0, 660.0);
                    super::glow(
                        book,
                        c.dx,
                        c.dy,
                        160.0 + 200.0 * (1.0 - pulse),
                        VIOLET,
                        0.35 * pulse,
                    );
                }),
            )));
        }
    }

    stack.into()
}
