//! S12 · THE RECEIPTS — numbers arriving as files. 2:13–2:21.
//!
//! The benchmark card assembles in 3D around real widget props (E-13's
//! grammar): a frosted card whose body is a `Transform3`-projected quad
//! settling to flat, carrying a chip, a progress rail, a sparkline — and
//! the two numbers the CI artifacts own: **59.3 fps · 4.09 ms**, Redmi
//! Note 7 Pro (2019). The CI timeline draws itself as a path (E-19) —
//! jobs as ticks, the suite as one line.
//!
//! The numbers arrive as receipts (they are constants of the suite's own
//! measurement, printed by the device-suite and held verbatim by the lab
//! since round 3). Caption: *"that's a 2019 phone."*

use vieww_foundation::{Color, FontWeight, Offset, Rect, Size, Sketchbook, TextStyle, Transform3};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use crate::film_lib::{
    alpha, clamp01, mix, spring_out, tint, xywh, INK, MUTED, VIOLET, VIOLET_SOFT,
};

use super::{backdrop, caption, Ctx};

/// The device suite's own receipts — measured by `ci/mobile/device-suite.sh`
/// on the Redmi, held verbatim (kinetic's constants, the same source).
const FPS_MEDIAN: f32 = 59.3;
const MS_MEDIAN: f32 = 4.09;
const DEVICE: &str = "Redmi Note 7 Pro (2019)";

/// The card's rest rect — center stage (edges: left, top, right, bottom).
const CARD: Rect = Rect::new(460.0, 250.0, 1460.0, 830.0);

/// The card's 3D settle: edge-on to flat, spring-eased (the unfold's
/// grammar, one plane, no fan).
fn card_quad(t: f32) -> Option<vieww_foundation::Path> {
    let s = spring_out(clamp01((t - 0.05) / 0.38), 5.8, 0.76);
    let tilt = -0.30 * (1.0 - s);
    let xf = Transform3::translation(0.0, -540.0, 0.0)
        .then(Transform3::rotation_x(tilt))
        .then(Transform3::translation(0.0, 540.0, 0.0))
        .then(Transform3::perspective(1400.0));
    xf.project_rect(CARD)
}

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;

    let mut stack = Stack::new().push(Positioned::fill().child(backdrop(t, 0x7EC, 110)));

    // The card — projected quad, frosted fill, lit edge.
    if let Some(quad) = card_quad(t) {
        let settle = clamp01((t - 0.05) / 0.38);
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                // The floor shadow.
                book.layer(1.0, 26.0, None, |g| {
                    g.rrect(
                        xywh(
                            CARD.left + 60.0,
                            CARD.bottom - 26.0,
                            CARD.width() - 120.0,
                            60.0,
                        ),
                        30.0,
                        alpha(Color::BLACK, 0.35 * settle),
                    );
                });
                // The body.
                book.fill(
                    quad.clone(),
                    Gradient::vertical().with_dither().with_stops(&[
                        (0.0, alpha(mix(Color::WHITE, VIOLET_SOFT, 0.20), 0.075)),
                        (0.6, alpha(Color::WHITE, 0.040)),
                        (
                            1.0,
                            alpha(mix(Color::WHITE, Color::rgb(103, 232, 249), 0.25), 0.055),
                        ),
                    ]),
                );
                book.stroke(quad.clone(), alpha(VIOLET_SOFT, 0.4 * settle), 1.6);
            }),
        )));
    }

    // The card's content — real widget props, arriving with the settle.
    let settle = clamp01((t - 0.05) / 0.38);
    if settle > 0.2 {
        let a = clamp01((settle - 0.2) / 0.5);
        let content = Stack::new()
            // The device chip.
            .push(
                Positioned::new()
                    .left(524.0)
                    .top(316.0)
                    .width(520.0)
                    .height(38.0)
                    .child(Opacity::new(a).child(super::chip(
                        DEVICE,
                        16.0,
                        tint(VIOLET_SOFT, 0.3),
                    ))),
            )
            // The two numbers — count-ups on staggered springs (E-13).
            .push(
                Positioned::new()
                    .left(520.0)
                    .top(392.0)
                    .width(440.0)
                    .height(120.0)
                    .child(Opacity::new(a).child(count_up(FPS_MEDIAN, t, 0.30, 1))),
            )
            .push(
                Positioned::new()
                    .left(1000.0)
                    .top(392.0)
                    .width(440.0)
                    .height(120.0)
                    .child(Opacity::new(a).child(count_up(MS_MEDIAN, t, 0.38, 2))),
            )
            // The labels under the numbers.
            .push(
                Positioned::new()
                    .left(526.0)
                    .top(516.0)
                    .width(440.0)
                    .height(24.0)
                    .child(
                        Opacity::new(a).child(
                            Text::new("fps median · device suite").style(
                                TextStyle::new(14.0)
                                    .monospace()
                                    .letter_spacing(1.8)
                                    .color(alpha(MUTED, 0.9)),
                            ),
                        ),
                    ),
            )
            .push(
                Positioned::new()
                    .left(1006.0)
                    .top(516.0)
                    .width(440.0)
                    .height(24.0)
                    .child(
                        Opacity::new(a).child(
                            Text::new("ms median · frame").style(
                                TextStyle::new(14.0)
                                    .monospace()
                                    .letter_spacing(1.8)
                                    .color(alpha(MUTED, 0.9)),
                            ),
                        ),
                    ),
            );
        stack = stack.push(content);

        // The progress rail + the sparkline — widget props, painted.
        stack = stack.push(
            Positioned::new()
                .left(524.0)
                .top(576.0)
                .width(872.0)
                .height(130.0)
                .child(Opacity::new(a).child(Painting::sized(
                    Size::new(872.0, 130.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        // A linear progress — the suite passing.
                        let pw = 872.0 * clamp01((t - 0.42) / 0.3);
                        book.rrect(xywh(0.0, 0.0, 872.0, 10.0), 5.0, alpha(Color::WHITE, 0.08));
                        book.rrect(xywh(0.0, 0.0, pw, 10.0), 5.0, alpha(VIOLET_SOFT, 0.85));
                        // The sparkline — the frame-time trace, drawn from
                        // its own numbers (a calm trace around the median).
                        let mut p = vieww_foundation::Path::new();
                        for i in 0..48 {
                            let k = i as f32 / 47.0;
                            let x = k * 872.0;
                            let v = (k * 9.0).sin() * 0.5 + (k * 23.0).sin() * 0.3;
                            let y = 96.0 - (v * 14.0) - 26.0;
                            if i == 0 {
                                p.move_to(Offset::new(x, y));
                            } else {
                                p.line_to(Offset::new(x, y));
                            }
                        }
                        book.stroke(p, alpha(VIOLET, 0.75), 1.6);
                        // The median rule.
                        book.line(
                            Offset::new(0.0, 70.0),
                            Offset::new(872.0, 70.0),
                            alpha(Color::WHITE, 0.10),
                            1.0,
                        );
                    }),
                ))),
        );
    }

    // The CI timeline — the suite's jobs as a self-drawing path (E-19).
    stack = stack.push(ci_timeline(t));

    stack = stack.push(caption(
        "that's a 2019 phone",
        964.0,
        clamp01((t - 0.55) / 0.08),
    ));

    stack.into()
}

/// A counted-up number — the kinetic band-4 grammar.
fn count_up(target: f32, t: f32, t0: f32, decimals: usize) -> WidgetNode {
    let v = target * spring_out(clamp01((t - t0) / 0.34), 12.0, 0.7);
    let text = format!("{:.*}", decimals, v);
    let settled = v >= target * 0.999;
    Text::new(text)
        .style(
            TextStyle::new(84.0)
                .monospace()
                .weight(if settled {
                    FontWeight::Bold
                } else {
                    FontWeight::Medium
                })
                .color(if settled { INK } else { alpha(MUTED, 0.9) }),
        )
        .into()
}

/// The CI timeline — one line, five job ticks, drawing itself.
fn ci_timeline(t: f32) -> WidgetNode {
    let draw = clamp01((t - 0.48) / 0.34);
    Painting::sized(
        Size::new(1920.0, 200.0),
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            let x0 = 220.0;
            let x1 = 1700.0;
            let y = 128.0;
            let jobs = ["checkout", "build", "bench", "artifact", "report"];
            let n = jobs.len() as f32;
            // The ghost.
            book.line(
                Offset::new(x0, y),
                Offset::new(x1, y),
                alpha(Color::WHITE, 0.06),
                1.0,
            );
            // The progressive stroke — one dash, phase-advanced.
            let total = x1 - x0;
            book.stroke_styled(
                {
                    let mut p = vieww_foundation::Path::new();
                    p.move_to(Offset::new(x0, y));
                    p.line_to(Offset::new(x1, y));
                    p
                },
                alpha(VIOLET_SOFT, 0.85),
                2.0,
                vieww_foundation::StrokeStyle::default()
                    .dash(vieww_foundation::Dash::new(vec![total * draw, total])),
            );
            // The job ticks — one per stage, lit as the line passes.
            for (i, job) in jobs.iter().enumerate() {
                let x = x0 + (i as f32 + 0.5) / n * total;
                let lit = draw >= (i as f32 + 0.5) / n;
                book.circle(
                    Offset::new(x, y),
                    if lit { 5.0 } else { 3.0 },
                    if lit {
                        alpha(VIOLET_SOFT, 0.95)
                    } else {
                        alpha(MUTED, 0.3)
                    },
                );
                let _ = job;
            }
        }),
    )
    .into()
}
