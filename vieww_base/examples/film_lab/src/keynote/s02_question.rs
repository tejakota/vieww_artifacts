//! S02 · THE QUESTION — Ki's hinge. 0:13–0:20.
//!
//! Black. Type-on: *"how long should it take to see what you built?"* —
//! then the sentence lifts: every glyph rides a deterministic flow field,
//! accelerates, and **condenses into one falling drop**. Typography
//! becoming weather becoming water — the typo plate's grammar (E-02's
//! type-on, the storm), landing the film's genesis image: one drop, about
//! to become a word.
//!
//! The cut in from S01 is the hard cut: silence, 60 fps, full black — the
//! cadence is the argument (K1). No caption; the question is the caption.

use std::sync::OnceLock;

use vieww_foundation::{Color, Gradient, Offset, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use crate::film_lib::{alpha, clamp01, mix, tint, INK, VIOLET, VIOLET_SOFT};

use super::{ease_out_type, Ctx};

/// The question, verbatim from the graph.
const QUESTION: &str = "how long should it take to see what you built?";

/// Where the line sits — centered, mid-frame.
const LINE_Y: f32 = 512.0;
const TYPE_SIZE: f32 = 42.0;

/// Where the glyphs condense — the drop's birthplace.
const CENTER: Offset = Offset::new(960.0, 396.0);

/// One glyph's storm state — its line origin and its phase pair.
struct Storm {
    starts: Vec<Offset>,
    phases: Vec<(f32, f32)>,
    rates: Vec<f32>,
}

fn hash01(i: usize) -> f32 {
    let h = (i as u64).wrapping_mul(0x9E3779B97F4A7C15).rotate_left(17);
    (h >> 40) as f32 / ((1u64 << 24) as f32)
}

fn storm() -> &'static Storm {
    static S: OnceLock<Storm> = OnceLock::new();
    S.get_or_init(|| {
        let n = QUESTION.chars().count();
        let adv = TYPE_SIZE * 0.60205;
        let total = adv * n as f32;
        let x0 = CENTER.dx - total * 0.5;
        let starts: Vec<Offset> = (0..n)
            .map(|i| Offset::new(x0 + adv * i as f32 + adv * 0.5, LINE_Y))
            .collect();
        let phases: Vec<(f32, f32)> = (0..n)
            .map(|i| {
                (
                    hash01(i * 7 + 1) * std::f32::consts::TAU,
                    hash01(i * 13 + 5) * std::f32::consts::TAU,
                )
            })
            .collect();
        let rates: Vec<f32> = (0..n).map(|i| 0.8 + hash01(i * 11 + 3) * 0.5).collect();
        Storm {
            starts,
            phases,
            rates,
        }
    })
}

/// The typing window, then the lift, then the condensation.
const TYPE_END: f32 = 0.40;
const LIFT_END: f32 = 0.78;

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let n = QUESTION.chars().count();
    let st = storm();

    // The typed prefix.
    let typed = (n as f32 * ease_out_type(clamp01(t / TYPE_END))).round() as usize;
    let typed = typed.min(n);
    let lift = clamp01((t - TYPE_END) / (LIFT_END - TYPE_END));
    let condense = clamp01((t - 0.66) / 0.24);

    let mut stack = Stack::new();

    // Phase A — the question, typed. The caret blinks at the wait's own
    // cadence, still; the cut has happened but the habit hasn't died.
    if lift <= 0.0 {
        let visible: String = QUESTION.chars().take(typed).collect();
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(LINE_Y - 32.0)
                .width(1920.0)
                .height(64.0)
                .child(
                    Text::new(visible)
                        .style(
                            TextStyle::new(TYPE_SIZE)
                                .monospace()
                                .letter_spacing(1.5)
                                .color(alpha(INK, 0.94)),
                        )
                        .align(TextAlign::Center),
                ),
        );
        if typed < n || t < TYPE_END {
            let blink = (ctx.sec * 4.8).sin() > -0.2;
            if blink {
                let adv = TYPE_SIZE * 0.60205;
                let cx = CENTER.dx + (typed as f32 - n as f32 * 0.5) * adv + adv * 0.5;
                stack = stack.push(
                    Positioned::new()
                        .left(cx)
                        .top(LINE_Y - 24.0)
                        .width(3.0)
                        .height(44.0)
                        .child(Container::new().color(alpha(VIOLET_SOFT, 0.85)).radius(1.5)),
                );
            }
        }
    }

    // Phase B — the lift. Every glyph still on screen (all of them now)
    // leaves the line and rides the field; convergence bends them toward
    // the condensation point as the field lets go.
    if lift > 0.0 {
        for (i, ch) in QUESTION.chars().enumerate() {
            let start = st.starts[i];
            let (p1, p2) = st.phases[i];
            let rate = st.rates[i];
            let u = (lift * rate).min(1.0);

            // The field: two crossing sines and a swirl — typo's flow,
            // closed-form (deterministic, no integration table needed).
            let field_x = (start.dy * 0.011 + p1).sin() * 46.0 + (start.dx * 0.009).cos() * 18.0
                - (start.dx - CENTER.dx) / 480.0 * 26.0 * u;
            let field_y = -(start.dx * 0.010 + p2).cos() * 40.0 * u
                + (start.dy - CENTER.dy) / 420.0 * 30.0 * u
                - u * u * 120.0;

            // The convergence — after the field has had its moment, the
            // glyphs are pulled to the center, shrinking as they go.
            let conv = condense * condense;
            let px = start.dx + field_x + (CENTER.dx - start.dx - field_x) * conv;
            let py = start.dy + field_y + (CENTER.dy - start.dy - field_y) * conv;

            let scale = 1.0 - 0.82 * conv;
            let alpha_g = 1.0 - 0.85 * conv;
            let tinted = mix(INK, tint(VIOLET_SOFT, 0.4), lift * 0.8);

            let size = TYPE_SIZE * scale;
            stack = stack.push(
                Positioned::new()
                    .left(px - size * 0.35)
                    .top(py - size * 0.55)
                    .width(size * 1.4)
                    .height(size * 1.4)
                    .child(
                        Opacity::new(alpha_g).child(
                            Text::new(ch.to_string())
                                .style(TextStyle::new(size).monospace().color(tinted))
                                .align(TextAlign::Center),
                        ),
                    ),
            );
        }
    }

    // Phase C — the drop. Born at the condensation point as the last
    // glyphs arrive; begins to fall at the very end (S03 inherits the fall).
    let drop_birth = clamp01((t - 0.80) / 0.10);
    if drop_birth > 0.0 {
        let fall = ((t - 0.955) / 0.045).max(0.0);
        let dy = CENTER.dy + fall * fall * 130.0;
        let dr = 7.0 + 9.0 * drop_birth;
        let dalpha = drop_birth;
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                // The drop — a teardrop body with a violet core.
                book.layer(1.0, 6.0, None, |g| {
                    g.circle(
                        Offset::new(CENTER.dx, dy),
                        dr * 3.4,
                        Gradient::radial_fill().with_dither().with_stops(&[
                            (0.0, alpha(VIOLET, 0.30 * dalpha)),
                            (1.0, alpha(VIOLET, 0.0)),
                        ]),
                    );
                });
                book.circle(
                    Offset::new(CENTER.dx, dy),
                    dr,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(tint(VIOLET_SOFT, 0.55), 0.98 * dalpha)),
                        (0.7, alpha(VIOLET, 0.85 * dalpha)),
                        (1.0, alpha(mix(VIOLET, Color::BLACK, 0.4), 0.9 * dalpha)),
                    ]),
                );
                // The highlight — the drop reads as liquid.
                book.circle(
                    Offset::new(CENTER.dx - dr * 0.32, dy - dr * 0.36),
                    dr * 0.22,
                    alpha(Color::WHITE, 0.75 * dalpha),
                );
            }),
        )));
    }

    // The ground — black, the deepest the film goes between beats.
    Stack::new()
        .push(
            Positioned::fill().child(
                Container::new()
                    .size(1920.0, 1080.0)
                    .color(Color::rgb(5, 5, 7)),
            ),
        )
        .push(stack)
        .into()
}
