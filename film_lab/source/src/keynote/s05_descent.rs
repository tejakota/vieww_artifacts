//! S05 · THE DESCENT — Shō opens. 0:40–0:51.
//!
//! The same buffer, now as Rust: a scan-line sweep crosses the editor and
//! the code resolves (E-06) — say on one side, generated Rust on the
//! other, annotated `// say: …` the way codegen annotates. State holds
//! across the switch: the counter never resets, the session chip never
//! blinks. A doorway, not a wall.
//!
//! Tap → **2**. The descent is also a claim about depth: everything you
//! are about to watch in Act II works the same way one layer down.

use vieww_widget::prelude::*;

use crate::film_lib::clamp01;

use super::studio::{studio, App, Code, Spec};
use super::{caption, Ctx};

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;

    // The sweep crosses between 0.14 and 0.48; the buffer is say before,
    // Rust after, line by line as the scan passes.
    let sweep = clamp01((t - 0.14) / 0.34);
    let code = if t < 0.14 {
        Code::Say { typed: 1.0, blink: ctx.sec }
    } else {
        Code::Rust { sweep }
    };

    let mut app = App::new(ctx.ladder, super::tap_pulse(abs), abs);
    // The preview holds the whole time — state carries through the morph.
    app.alive = 1.0;

    let spec = Spec {
        code,
        app,
        session_line: clamp01((abs - 26.0) / 99.0),
        ..Spec::default()
    };

    let mut stack = Stack::new()
        .push(studio(abs, ctx.ladder, spec));

    stack = stack.push(caption(
        "start in say · go as deep as you want",
        964.0,
        clamp01((t - 0.55) / 0.10),
    ));

    stack.into()
}
