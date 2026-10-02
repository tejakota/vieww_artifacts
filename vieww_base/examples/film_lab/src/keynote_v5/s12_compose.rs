//! S12 · LIVE COMPOSE — per-keystroke, and the damage economy. 1:49–2:00.
//!
//! The reference's "Step 2": change a text element, modify a structural
//! value — the preview updates *immediately*, without a manual reload
//! cycle, and the damage overlay lights exactly one region per write.
//! Two edits: the heading "Counter" → "viewwstudio" (a text change, the
//! word scrambling under the keystrokes) and the spacing 16 → 24 (a
//! structural re-flow the whole column breathes around). The receipts the
//! engine keeps live: **10 writes · 1 rebuild** — the scheduler's
//! coalescing, made visible (the scrub plate's grammar).
//!
//! Taps 3 (heading, t≈0.30) and 4 (spacing, t≈0.62) fire here.

use vieww_foundation::{Color, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use crate::film_lib::{alpha, clamp01, ease_in_out, tint, xywh, INK, MINT, VIOLET_SOFT};

use super::studio;
use super::Ctx;

/// The heading edit window (scene fraction).
const HEAD_T0: f32 = 0.30;
const HEAD_T1: f32 = 0.50;
/// The spacing edit window.
const SPACE_T0: f32 = 0.62;
const SPACE_T1: f32 = 0.74;

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let abs = ctx.abs;

    // The two edits' progress.
    let ladder = ctx.ladder;
    let head_p = ease_in_out(clamp01((t - HEAD_T0) / (HEAD_T1 - HEAD_T0)));
    let space_p = ease_in_out(clamp01((t - SPACE_T0) / (SPACE_T1 - SPACE_T0)));

    // The live compose overlay — an edit region highlight riding the line
    // being changed (the studio's own editor grammar).
    let mut err: Option<(usize, usize)> = None;
    let _ = &mut err;

    let mut spec = studio::Spec {
        code: studio::Code::Say {
            typed: 1.0,
            blink: ctx.sec,
        },
        app: {
            let mut app = studio::App::new(1, super::tap_pulse(abs), abs);
            app.heading = head_p;
            app.spacing = space_p;
            app.dial = clamp01((t - 0.10) / 0.10);
            app.spring = clamp01((t - 0.16) / 0.10);
            app
        },
        session_line: 1.0,
        build_badge: Some(build_count(t)),
        ..Default::default()
    };

    // The damage overlay — one region per write, exactly the widget's own
    // geometry: the heading's rect during the text edit, the column's
    // during the re-flow.
    let (damage, label) = if head_p > 0.0 && head_p < 1.0 {
        let wobble = 0.5 + 0.5 * (sec * 10.0).sin();
        (
            Rect::new(96.0 - 6.0, 80.0, 96.0 + 620.0 + wobble * 10.0, 150.0),
            "damage · heading text".to_string(),
        )
    } else if space_p > 0.0 && space_p < 1.0 {
        (
            Rect::new(80.0, 70.0, 880.0, 700.0),
            "damage · column spacing".to_string(),
        )
    } else {
        (Rect::new(0.0, 0.0, 0.0, 0.0), String::new())
    };
    if !label.is_empty() {
        spec.damage = Some((damage, label));
    }

    let mut stack = Stack::new().push(Positioned::fill().child(studio::studio(abs, ladder, spec)));

    // The coalescing receipt — writes vs rebuilds, live: a small strip
    // under the editor showing the last ten writes folding into one build
    // (the scheduler's own arithmetic, animated by the scene's clock).
    let strip_a = clamp01((t - 0.18) / 0.12);
    if strip_a > 0.0 {
        let writes = write_events(t);
        stack = stack.push(
            Positioned::new()
                .left(studio::ED_X0 + 40.0)
                .top(918.0)
                .width(560.0)
                .height(84.0)
                .child(Opacity::new(strip_a).child(Painting::sized(
                    Size::new(560.0, 84.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        book.rrect(
                            xywh(0.0, 0.0, 560.0, 84.0),
                            10.0,
                            alpha(Color::rgb(14, 14, 19), 0.92),
                        );
                        book.stroke_rrect(
                            xywh(0.0, 0.0, 560.0, 84.0),
                            10.0,
                            alpha(Color::WHITE, 0.08),
                            1.0,
                        );
                        // The write ticks — ten, landing in the strip.
                        for (i, wt) in writes.iter().enumerate() {
                            let age = (sec - wt).max(0.0);
                            let a = (1.0 - age * 0.9).clamp(0.25, 1.0);
                            let x = 20.0 + i as f32 * 34.0;
                            book.rrect(xywh(x, 16.0, 8.0, 22.0), 2.0, alpha(VIOLET_SOFT, a));
                        }
                        // The one rebuild they fold into.
                        let built = build_count(t);
                        book.rrect(
                            xywh(20.0, 52.0, 220.0 * (built as f32 / 3.0).min(1.0), 14.0),
                            7.0,
                            alpha(tint(MINT, 0.05), 0.9),
                        );
                        let _ = built;
                    }),
                ))),
        );
        let wr = write_events(t).len();
        stack = stack.push(
            Positioned::new()
                .left(studio::ED_X0 + 260.0)
                .top(946.0)
                .width(340.0)
                .height(26.0)
                .child(
                    Opacity::new(strip_a).child(
                        Text::new(format!("{} writes · {} rebuild", wr, build_count(t))).style(
                            TextStyle::new(15.0)
                                .monospace()
                                .letter_spacing(1.2)
                                .color(alpha(INK, 0.85)),
                        ),
                    ),
                ),
        );
    }

    // The captions.
    stack = stack.push(super::caption(
        "per-keystroke. the preview is the build.",
        1000.0,
        clamp01((t - 0.06) / 0.12),
    ));
    if head_p > 0.02 {
        stack = stack.push(super::caption(
            "one write lights one region — nothing else repaints",
            964.0,
            clamp01((t - HEAD_T0 - 0.06) / 0.12),
        ));
    }

    stack.into()
}

/// The write events — the ten keystrokes the scheduler coalesces, as film
/// seconds (the scrub plate's grammar: writes cluster, the build fires once).
fn write_events(t: f32) -> Vec<f32> {
    let mut v = Vec::new();
    for i in 0..10 {
        let wt = 0.20 + i as f32 * 0.052 + (i % 3) as f32 * 0.01;
        if wt <= t {
            v.push(wt);
        }
    }
    v
}

/// How many rebuilds the ten writes collapsed into — one per completed
/// edit cluster (the honest count for this scene: two edits, one settle
/// each, plus the initial build).
fn build_count(t: f32) -> u64 {
    let mut n = 1;
    if t > HEAD_T1 {
        n += 1;
    }
    if t > SPACE_T1 {
        n += 1;
    }
    n
}
