//! S14 · THE FOLD-BACK — Ketsu opens. 2:35–2:39.
//!
//! The unfold in reverse, fast, spring-eased (F7): the planes fold down,
//! the grid lets go, the depth closes — and the world flattens to **one
//! surface: the studio, as it was in S04.** Everything you watched was
//! one thing. Kishōtenketsu's reconciliation begins by taking the world
//! apart again, quickly, and handing it back as a workbench.

use vieww_foundation::{Color, Rect, Size, Sketchbook, Transform3};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use crate::film_lib::{alpha, clamp01, ease_in_out, mix, spring_out, tint, BG_DEEP};

use super::studio::{studio, App, Code, Spec};
use super::{Ctx, C_DESC, C_GEOM, C_IDENT};

const FOCAL: f32 = 1500.0;
const SPINE_Y: f32 = 660.0;

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;

    // The fold — S13's reveal played backward, compressed to four
    // seconds. The planes stand back up and slide away; the depth closes.
    let fold = ease_in_out(clamp01(t / 0.62));
    let unfold = 1.0 - fold; // 1 → 0: rest tilt → edge-on → gone

    // The studio beneath — arriving as the planes leave, exactly as it
    // was in S04 (the same chrome, the same session, the counter at 7).
    let studio_a = clamp01((t - 0.30) / 0.34);
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
        session_line: 1.0,
        ..Spec::default()
    };

    let mut stack = Stack::new();

    if studio_a < 1.0 {
        stack = stack.push(
            Positioned::fill().child(
                Opacity::new(1.0 - studio_a).child(
                    Container::new()
                        .size(1920.0, 1080.0)
                        .color(Color::rgb(6, 6, 9)),
                ),
            ),
        );
    }
    if studio_a > 0.0 {
        stack = stack.push(
            Positioned::fill().child(Opacity::new(studio_a).child(studio(abs, ctx.ladder, spec))),
        );
    }

    // The folding planes — three quads rising back to edge-on.
    if unfold > 0.01 {
        let planes = [
            (
                Rect::new(468.0, 452.0, 828.0, SPINE_Y),
                C_DESC,
                0.0f32,
                -0.46f32,
            ),
            (
                Rect::new(852.0, 452.0, 1212.0, SPINE_Y),
                C_IDENT,
                0.06,
                -0.58,
            ),
            (
                Rect::new(1236.0, 452.0, 1596.0, SPINE_Y),
                C_GEOM,
                0.12,
                -0.70,
            ),
        ];
        let k = 0.82 + 0.18 * fold; // the world group returns too
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                for (rest, color, stagger, tilt_rest) in planes {
                    // The unfold's timeline reversed: where S13 ended
                    // (settled), S14 begins, and it folds to edge-on.
                    let tau = clamp01((unfold - stagger) / 0.5);
                    let s = spring_out(tau, 6.4, 0.8);
                    let tilt =
                        tilt_rest + (1.0 - s) * (-std::f32::consts::FRAC_PI_2 + 0.06 - tilt_rest);
                    let xf = Transform3::translation(0.0, -SPINE_Y, 0.0)
                        .then(Transform3::rotation_x(tilt))
                        .then(Transform3::translation(0.0, SPINE_Y, 0.0))
                        .then(Transform3::perspective(FOCAL));
                    let Some(quad) = xf.project_rect(rest) else {
                        continue;
                    };
                    book.layer(clamp01(tau * 1.2), 0.0, None, |g| {
                        g.fill(
                            quad.clone(),
                            Gradient::vertical().with_dither().with_stops(&[
                                (0.0, alpha(mix(color, BG_DEEP, 0.72), 0.30)),
                                (1.0, alpha(mix(color, BG_DEEP, 0.82), 0.22)),
                            ]),
                        );
                    });
                    book.stroke(quad, alpha(tint(color, 0.25), 0.7 * clamp01(tau)), 1.4);
                }
                let _ = k;
            }),
        )));
    }

    stack.into()
}
