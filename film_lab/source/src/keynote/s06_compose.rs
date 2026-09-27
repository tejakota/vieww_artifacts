//! S06 · COMPOSE LIVE — Shō's long middle. 0:51–1:11.
//!
//! Keystrokes land per-keystroke in the preview: the buffer grows a dial
//! line and a spring line, and the app grows a dial and a spring —
//! composed as you type, the live-compose promise made visible.
//!
//! At ~1:03 a deliberate error: the word runs one step too far, and the
//! preview renders it as a **boundary, not a crash** (E-07) — a card, a
//! precise message, the last good frame held underneath. Fixed in three
//! keystrokes; the fix lands with a pop.
//!
//! Tap → **3**. Twenty seconds, one breath, no cuts — Shō deepens.

use vieww_foundation::{Color, Offset, Sketchbook, Size};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{alpha, clamp01, smoothstep, xywh, MUTED, VIOLET_SOFT};

use super::studio::{studio, App, Code, Seg, Spec, CODE_PLAIN};
use super::{caption, ease_out_type, Ctx};

/// The dial line's clean text.
const DIAL: &str = "        a dial showing count";
/// The dial line's error text — the word runs one step too far.
const DIAL_ERR: &str = "        a dial showing counting";
/// The spring line's clean text.
const SPRING: &str = "        a spring below it";

/// One line as segments, sliced to `n` visible chars.
fn seg_line(text: &'static str, n: usize) -> Vec<Seg> {
    let n = n.min(text.chars().count());
    let s: String = text.chars().take(n).collect();
    vec![("        ", MUTED), (leak(s), CODE_PLAIN)]
}

/// Static strings for the segments (the render is one process, bounded).
fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;

    // The composition script — typed, errored, fixed (times in-scene):
    //   0.08–0.30  the dial line types      0.32–0.48  the spring line types
    //   0.55–0.60  the error arrives        0.65–0.72  three keystrokes fix it
    //   0.80       tap → 3
    let dial_full = DIAL.chars().count();
    let spring_full = SPRING.chars().count();
    let dial_n = (dial_full as f32 * ease_out_type(clamp01((t - 0.08) / 0.22))).round() as usize;
    let spring_n = (spring_full as f32 * ease_out_type(clamp01((t - 0.32) / 0.16))).round() as usize;

    let err_in = smoothstep(clamp01((t - 0.55) / 0.05));
    let fix_step = clamp01((t - 0.65) / 0.07);
    // During the fix, three backspaces take the error word back to "count".
    let backspaced = (3.0 * fix_step).round() as usize;

    // The buffer's visible extras, by phase.
    let mut extras: Vec<Vec<Seg>> = Vec::new();
    let mut err_token = None;
    if t >= 0.08 {
        if err_in > 0.0 && t < 0.72 {
            // The line shows the full error text, minus what the fix removed.
            let vis = DIAL_ERR.chars().count() - backspaced;
            extras.push(seg_line(DIAL_ERR, vis));
            // The underline rides under the runaway word — its offset is
            // the word's own position in the line (derived, not typed).
            let word_at = DIAL_ERR.find("counting").unwrap_or(23);
            err_token = Some((8, word_at));
        } else {
            extras.push(seg_line(DIAL, dial_n.min(dial_full)));
        }
    }
    if t >= 0.32 {
        extras.push(seg_line(SPRING, spring_n));
    }

    let code = Code::SayExt { blink: ctx.sec, lines: extras };

    // The app grows with the typing — per-keystroke compose.
    let mut app = App::new(ctx.ladder, super::tap_pulse(abs), abs);
    app.alive = 1.0;
    app.dial = clamp01(dial_n as f32 / dial_full as f32);
    app.spring = clamp01(spring_n as f32 / spring_full as f32);
    app.error = if t < 0.65 { err_in } else { 1.0 - fix_step };

    let spec = Spec {
        code,
        app,
        err_token,
        session_line: clamp01((abs - 26.0) / 99.0),
        ..Spec::default()
    };

    let mut stack = Stack::new().push(studio(abs, ctx.ladder, spec));

    // The boundary caption — the E-07 story, said once.
    stack = stack.push(caption(
        "a boundary · not a crash",
        964.0,
        clamp01((t - 0.58) / 0.08) * (1.0 - clamp01((t - 0.74) / 0.06)),
    ));

    // The fix lands with a pop — a brief ring at the preview's label.
    if (0.72..0.82).contains(&t) {
        let pop = clamp01((t - 0.72) / 0.08);
        stack = stack.push(
            Positioned::fill().child(Painting::sized(
                super::CANVAS,
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    let c = Offset::new(1160.0, 330.0);
                    book.ring(c, 20.0 + 60.0 * pop, 2.0, alpha(VIOLET_SOFT, 0.6 * (1.0 - pop)));
                }),
            )),
        );
    }

    stack.into()
}
