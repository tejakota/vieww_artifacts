//! S20 · BUILT ON VIEWW — the pull-back reveal. 3:13–3:23.
//!
//! Everything you watched was one thing. The studio floats center-frame
//! and goes to glass; beneath it the 36-crate constellation materializes
//! and holds it up with shafts of light; behind everything, the film's
//! own strata hover faintly — the terminal, the amputated tree, the
//! caged gear, the engine room — the whole story in one picture.
//!
//! The title lands: **viewwstudio — built on vieww.**

use vieww_foundation::{
    Color, Gradient, Offset, Size, Sketchbook, TextAlign, TextStyle, Transform,
};
use vieww_widget::prelude::*;
use vieww_widget::Transformed;
use vieww_widget::{Opacity, PaintWith, Painting};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, ease_out_back, ease_out_cubic, mix, xywh, Rng, CYAN, INK, MUTED,
    VIOLET, VIOLET_SOFT,
};

use super::studio;
use super::Ctx;

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;
    let ladder = ctx.ladder;

    // The studio settles small (0 → 0.26), goes to glass (0.30 → 0.55),
    // the constellation rises beneath (0.34 → 0.70), the strata breathe
    // behind (0.40+), the title lands (0.66 → 0.82).
    let settle = ease_in_out(clamp01(t / 0.26));
    let glass = ease_in_out(clamp01((t - 0.30) / 0.25));
    let constel = ease_out_back(clamp01((t - 0.34) / 0.30));
    let title_a = clamp01((t - 0.66) / 0.16);

    let spec = studio::Spec {
        code: studio::Code::Say {
            typed: 1.0,
            blink: ctx.sec,
        },
        app: {
            let mut app = studio::App::new(1, super::tap_pulse(abs), abs);
            app.dial = 1.0;
            app.spring = 1.0;
            app
        },
        session_line: 1.0,
        ..Default::default()
    };

    let mut stack = Stack::new();

    // The ground — the film's deepest register.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            super::ground(book, w, h);
            super::stars(book, w, h, 0x8A17, 120, t, 0.10);
            super::vignette(book, w, h, 0.5);
        }),
    )));

    // The strata — the film's earlier images, faint and deep, stacked like
    // sediment behind the studio.
    let strata_a = clamp01((t - 0.40) / 0.20) * 0.5;
    if strata_a > 0.01 {
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let a = strata_a;
                // The terminal's ghost — a dim rectangle, top-left deep.
                book.stroke_rrect(
                    xywh(140.0, 150.0, 320.0, 190.0),
                    10.0,
                    alpha(Color::WHITE, 0.05 * a * 2.0),
                    1.2,
                );
                for i in 0..4 {
                    book.rect(
                        xywh(
                            164.0,
                            190.0 + i as f32 * 30.0,
                            180.0 + (i % 3) as f32 * 40.0,
                            3.0,
                        ),
                        alpha(Color::WHITE, 0.05 * a * 2.0),
                    );
                }
                // The tree's ghost — top-right deep.
                let mut rng = Rng::new(0x5702);
                for _ in 0..26 {
                    let x = 1500.0 + rng.f01() * 300.0;
                    let y = 160.0 + rng.f01() * 220.0;
                    book.circle(Offset::new(x, y), 2.2, alpha(VIOLET, 0.10 * a * 2.0));
                }
                // The gear's ghost — bottom-left deep.
                book.stroke(
                    super::circle_path(300.0, 820.0, 80.0, 40),
                    alpha(CYAN, 0.08 * a * 2.0),
                    2.0,
                );
                book.stroke(
                    super::circle_path(300.0, 820.0, 44.0, 32),
                    alpha(CYAN, 0.06 * a * 2.0),
                    1.4,
                );
                // The engine room's ghost — bottom-right deep.
                for i in 0..5 {
                    book.rrect(
                        xywh(1460.0 + i as f32 * 72.0, 800.0, 60.0, 34.0),
                        5.0,
                        alpha(VIOLET_SOFT, 0.08 * a * 2.0),
                    );
                }
            }),
        )));
    }

    // The constellation — the 36 crates as a lattice of glowing slabs,
    // rising beneath the studio and holding it up. Six rows of six.
    if constel > 0.0 {
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let base_y = 620.0 + (1.0 - constel) * 180.0;
                let rows = 6;
                let cols = 6;
                for r in 0..rows {
                    for c in 0..cols {
                        let idx = r * cols + c;
                        // Staggered arrival — the base first.
                        let arrive = clamp01((constel * 1.5) - (r as f32 * 0.10 + c as f32 * 0.03));
                        if arrive <= 0.0 {
                            continue;
                        }
                        let x = 660.0 + c as f32 * 100.0 + (r % 2) as f32 * 26.0;
                        let y = base_y + r as f32 * 54.0;
                        let pulse = 0.5 + 0.5 * (abs * 2.0 + idx as f32 * 0.7).sin();
                        let col = if r == 0 {
                            VIOLET
                        } else if r < 3 {
                            CYAN
                        } else {
                            VIOLET_SOFT
                        };
                        book.rrect(
                            xywh(x, y, 88.0, 26.0),
                            6.0,
                            alpha(mix(Color::rgb(20, 22, 32), col, 0.18), 0.96 * arrive),
                        );
                        book.stroke_rrect(
                            xywh(x, y, 88.0, 26.0),
                            6.0,
                            alpha(col, (0.55 + 0.3 * pulse) * arrive),
                            1.4,
                        );
                        // The slab's own edge light — visible weight.
                        book.rrect(
                            xywh(x + 8.0, y + 3.0, 72.0, 2.6),
                            1.3,
                            alpha(col, 0.5 * arrive),
                        );
                        // The light connecting upward — toward the studio.
                        if r < rows - 1 {
                            book.line(
                                Offset::new(x + 44.0, y),
                                Offset::new(x + 44.0 + 26.0, y - 54.0),
                                alpha(col, 0.16 + 0.14 * pulse),
                                1.2,
                            );
                        }
                    }
                }
                // The shafts — light rising from the constellation into the
                // studio above (beams' grammar, three shafts).
                book.layer(1.0, 26.0, None, |g| {
                    for i in 0..3 {
                        let x = 810.0 + i as f32 * 150.0;
                        let mut beam = vieww_foundation::Path::new();
                        beam.move_to(Offset::new(x - 20.0, 640.0));
                        beam.line_to(Offset::new(x + 6.0, 380.0));
                        beam.line_to(Offset::new(x + 40.0, 380.0));
                        beam.line_to(Offset::new(x + 14.0, 640.0));
                        beam.close();
                        g.fill(
                            beam,
                            Gradient::vertical().with_dither().with_stops(&[
                                (0.0, alpha(VIOLET, 0.0)),
                                (1.0, alpha(VIOLET, 0.16)),
                            ]),
                        );
                    }
                });
            }),
        )));
    }

    // The studio — small, centered, going to glass.
    if settle > 0.0 {
        let k = 0.30 + 0.70 * (1.0 - settle) * 0.0 + 0.0; // settles at 0.30
        let _ = k;
        let scale = 1.0 - 0.70 * settle; // 1.0 → 0.30
        let scale_about = |cx: f32, cy: f32, s: f32| {
            Transform::translate(Offset::new(cx * (1.0 - s), cy * (1.0 - s)))
                .then(Transform::scale(s, s))
        };
        stack = stack.push(
            Positioned::fill().child(
                Opacity::new(1.0 - 0.35 * glass).child(
                    Transformed::new(scale_about(960.0, 380.0, scale))
                        .child(studio::studio(abs, ladder, spec)),
                ),
            ),
        );
    }

    // The title — lands beneath the studio, above the constellation.
    if title_a > 0.0 {
        let a = ease_out_cubic(title_a);
        let rise = (1.0 - a) * 20.0;
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(470.0 + rise)
                .width(1920.0)
                .height(70.0)
                .child(
                    Opacity::new(a).child(
                        Text::new("viewwstudio — built on vieww")
                            .style(
                                TextStyle::new(48.0)
                                    .letter_spacing(3.0)
                                    .color(alpha(INK, 0.97)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(548.0 + rise)
                .width(1920.0)
                .height(34.0)
                .child(
                    Opacity::new(a * 0.9).child(
                        Text::new("the studio is the proof. the engine is the claim.")
                            .style(
                                TextStyle::new(20.0)
                                    .monospace()
                                    .letter_spacing(3.0)
                                    .color(alpha(MUTED, 0.9)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }

    stack = stack.push(super::caption(
        "the hero, and the ground it stands on",
        1000.0,
        clamp01((t - 0.70) / 0.14),
    ));

    stack.into()
}
