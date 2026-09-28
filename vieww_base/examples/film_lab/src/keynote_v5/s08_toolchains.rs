//! S08 · ONE CLICK — the toolchains. 1:11–1:21.
//!
//! The reference: "One-Click Toolchains — automatic target generation for
//! Android (.apk) and iOS (.ipa); cargo matrix native build steps." The
//! studio's own behaviour, staged as the engine's grammar: four target
//! rows verify their tools one by one (each tool checked before a build
//! starts — the studio's documented rule), the pipeline draws itself
//! left→right (compile → link → bundle → sign), and two packages stamp
//! out with springs: `.apk` and `.ipa`.

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle, FontWeight};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{alpha, clamp01, ease_in_out, ease_out_back, ease_out_cubic, mix, spring_out, tint, xywh, FAINT, INK, MUTED, Rng, VIOLET, VIOLET_SOFT, CYAN, CYAN_SOFT, MINT, AMBER};

use super::{Ctx};

/// The four targets — the studio's own Build-and-Run menu, verbatim.
const TARGETS: [(&str, &str); 4] = [
    ("Desktop", "the machine you're on"),
    ("Windows", "cross-compiled .exe"),
    ("Android", "debug-signed .apk"),
    ("iOS", "the .ipa"),
];

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = Stack::new();

    // The ground.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            super::ground(book, w, h);
            let mut x = 0.0;
            while x < w {
                book.line(Offset::new(x, 0.0), Offset::new(x, h), alpha(CYAN, 0.025), 1.0);
                x += 96.0;
            }
            super::stars(book, w, h, 0x71C4, 50, t, 0.06);
            super::vignette(book, w, h, 0.48);
        }),
    )));

    // The tool-verification panel — the targets checking their tools.
    let panel_a = clamp01(t / 0.12);
    if panel_a > 0.0 {
        // The panel card.
        stack = stack.push(
            Positioned::new()
                .left(280.0)
                .top(180.0)
                .width(640.0)
                .height(560.0)
                .child(Opacity::new(panel_a).child(Painting::sized(
                    Size::new(640.0, 560.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        book.rrect(xywh(0.0, 0.0, 640.0, 560.0), 18.0, alpha(Color::rgb(13, 14, 20), 0.95));
                        book.stroke_rrect(xywh(0.0, 0.0, 640.0, 560.0), 18.0, alpha(Color::WHITE, 0.10), 1.2);
                        // The header rail.
                        book.rect(xywh(1.0, 1.0, 638.0, 52.0), alpha(Color::rgb(16, 17, 24), 0.9));
                        book.rect(xywh(0.0, 52.0, 640.0, 1.0), alpha(Color::WHITE, 0.06));
                    }),
                ))),
        );
        stack = stack.push(
            Positioned::new()
                .left(312.0)
                .top(196.0)
                .width(400.0)
                .height(26.0)
                .child(Opacity::new(panel_a).child(
                    Text::new("build & run — targets")
                        .style(TextStyle::new(16.0).monospace().letter_spacing(2.2).color(alpha(MUTED, 0.9))),
                )),
        );
        // The rows — each verifies (a spinner), then ticks.
        for (i, (name, note)) in TARGETS.iter().enumerate() {
            let t0 = 0.10 + i as f32 * 0.11;
            let verify_p = clamp01((t - t0) / 0.10);
            let done_p = clamp01((t - t0 - 0.10) / 0.08);
            if verify_p <= 0.0 {
                continue;
            }
            let y = 268.0 + i as f32 * 108.0;
            let tick_a = ease_out_back(done_p);
            stack = stack.push(
                Positioned::new()
                    .left(312.0)
                    .top(y)
                    .width(576.0)
                    .height(92.0)
                    .child(Painting::sized(
                        Size::new(576.0, 92.0),
                        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                            book.rrect(xywh(0.0, 0.0, 576.0, 92.0), 12.0, alpha(Color::rgb(17, 18, 25), 0.9));
                            book.stroke_rrect(
                                xywh(0.0, 0.0, 576.0, 92.0),
                                12.0,
                                alpha(if done_p >= 1.0 { MINT } else { MUTED }, 0.3),
                                1.0,
                            );
                            // The status: spinner while verifying, tick when done.
                            let c = Offset::new(500.0, 46.0);
                            if done_p <= 0.0 {
                                let spin = sec * 7.0 + i as f32;
                                book.arc(c, 16.0, 3.0, spin, 4.4, alpha(CYAN_SOFT, 0.85));
                            } else if tick_a > 0.0 {
                                // The tick — a check drawn as one stroke.
                                let mut p = vieww_foundation::Path::new();
                                p.move_to(Offset::new(c.dx - 12.0, c.dy + 1.0));
                                p.line_to(Offset::new(c.dx - 3.0, c.dy + 10.0));
                                p.line_to(Offset::new(c.dx + 13.0, c.dy - 10.0));
                                book.stroke_styled(
                                    p,
                                    alpha(tint(MINT, 0.1), 0.95),
                                    4.0,
                                    vieww_foundation::StrokeStyle::rounded().dash(vieww_foundation::Dash::new(vec![
                                        40.0 * tick_a,
                                        40.0,
                                    ])),
                                );
                                // The tick's bloom.
                                book.layer(1.0, 8.0, None, |g| {
                                    g.circle(c, 26.0, Gradient::radial_fill().with_dither().with_stops(&[
                                        (0.0, alpha(MINT, 0.20 * tick_a)),
                                        (1.0, alpha(MINT, 0.0)),
                                    ]));
                                });
                            }
                        }),
                    )),
            );
            stack = stack.push(
                Positioned::new()
                    .left(338.0)
                    .top(y + 16.0)
                    .width(300.0)
                    .height(30.0)
                    .child(
                        Text::new(*name)
                            .style(TextStyle::new(24.0).weight(FontWeight::Medium).color(alpha(INK, 0.95))),
                    ),
            );
            stack = stack.push(
                Positioned::new()
                    .left(338.0)
                    .top(y + 52.0)
                    .width(420.0)
                    .height(24.0)
                    .child(
                        Text::new(*note)
                            .style(TextStyle::new(15.5).monospace().color(alpha(MUTED, 0.85))),
                    ),
            );
        }
    }

    // The pipeline — compile → link → bundle → sign, self-drawing.
    let pipe_a = clamp01((t - 0.34) / 0.12);
    if pipe_a > 0.0 {
        let stages = ["compile", "link", "bundle", "sign"];
        let draw = ease_in_out(clamp01((t - 0.36) / 0.30));
        stack = stack.push(Positioned::new()
            .left(1000.0)
            .top(220.0)
            .width(730.0)
            .height(200.0)
            .child(Opacity::new(pipe_a).child(Painting::sized(
                Size::new(730.0, 200.0),
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    // The line — one horizontal rail with four nodes.
                    let y = 100.0;
                    let x0 = 60.0;
                    let x1 = 670.0;
                    book.line(Offset::new(x0, y), Offset::new(x1, y), alpha(Color::WHITE, 0.08), 2.0);
                    // The progressive stroke.
                    let total = x1 - x0;
                    let mut p = vieww_foundation::Path::new();
                    p.move_to(Offset::new(x0, y));
                    p.line_to(Offset::new(x0 + total * draw, y));
                    book.stroke_styled(
                        p,
                        alpha(CYAN_SOFT, 0.9),
                        2.4,
                        vieww_foundation::StrokeStyle::rounded(),
                    );
                    // The marching dash on the drawn part — cargo's conveyor.
                    if draw > 0.02 {
                        let mut m = vieww_foundation::Path::new();
                        m.move_to(Offset::new(x0, y));
                        m.line_to(Offset::new(x0 + total * draw, y));
                        book.stroke_styled(
                            m,
                            alpha(tint(CYAN, 0.3), 0.9),
                            5.0,
                            vieww_foundation::StrokeStyle::rounded()
                                .dash(vieww_foundation::Dash::even(3.0).offset(-sec * 40.0)),
                        );
                    }
                    // The nodes.
                    for (i, _) in stages.iter().enumerate() {
                        let x = x0 + total * i as f32 / (stages.len() - 1) as f32;
                        let lit = draw >= i as f32 / (stages.len() - 1) as f32 - 0.001;
                        book.circle(
                            Offset::new(x, y),
                            if lit { 8.0 } else { 5.0 },
                            if lit { alpha(tint(CYAN_SOFT, 0.2), 1.0) } else { alpha(MUTED, 0.4) },
                        );
                        if lit {
                            book.ring(Offset::new(x, y), 14.0, 1.2, alpha(CYAN, 0.35));
                        }
                    }
                }),
            ))));
        // The stage labels.
        for (i, name) in ["compile", "link", "bundle", "sign"].iter().enumerate() {
            let x = 1000.0 + 60.0 + 610.0 * i as f32 / 3.0;
            let lit = ease_in_out(clamp01((t - 0.36) / 0.30)) >= i as f32 / 3.0 - 0.001;
            stack = stack.push(
                Positioned::new()
                    .left(x - 70.0)
                    .top(330.0)
                    .width(140.0)
                    .height(26.0)
                    .child(
                        Text::new(*name)
                            .style(TextStyle::new(16.0).monospace().letter_spacing(1.6).color(if lit {
                                alpha(tint(CYAN_SOFT, 0.1), 0.95)
                            } else {
                                alpha(MUTED, 0.6)
                            }))
                            .align(TextAlign::Center),
                    ),
            );
        }
        // The cargo receipt.
        let cargo_a = clamp01((t - 0.52) / 0.14);
        if cargo_a > 0.0 {
            stack = stack.push(
                Positioned::new()
                    .left(1000.0)
                    .top(470.0)
                    .width(730.0)
                    .height(30.0)
                    .child(Opacity::new(cargo_a).child(
                        Text::new("cargo matrix · native build steps · no glue scripts")
                            .style(TextStyle::new(19.0).monospace().letter_spacing(1.6).color(alpha(INK, 0.85))),
                    )),
            );
        }
    }

    // The packages — .apk and .ipa stamp out, springing.
    let pkg_s = |t0: f32| spring_out(clamp01((t - t0) / 0.34), 8.0, 0.55);
    for (i, (ext, col, label)) in [
        (".apk", MINT, "android package"),
        (".ipa", CYAN_SOFT, "ios package"),
    ]
    .iter()
    .enumerate()
    {
        let s = pkg_s(0.58 + i as f32 * 0.14);
        if s <= 0.0 {
            continue;
        }
        let x = 1052.0 + i as f32 * 330.0;
        let y = 600.0 - (1.0 - s) * 160.0;
        let press = 1.0 - 0.08 * (1.0 - s);
        stack = stack.push(
            Positioned::new()
                .left(x)
                .top(y)
                .width(280.0 * press)
                .height(150.0 * press)
                .child(Painting::sized(
                    Size::new(280.0, 150.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        // The package — a parcel with a wax seal.
                        book.rrect(xywh(0.0, 0.0, 280.0, 150.0), 14.0, alpha(Color::rgb(16, 18, 24), 0.97));
                        book.stroke_rrect(xywh(0.0, 0.0, 280.0, 150.0), 14.0, alpha(*col, 0.55), 1.6);
                        // The tape — a cross of packing tape.
                        book.rrect(xywh(126.0, 0.0, 28.0, 150.0), 3.0, alpha(*col, 0.14));
                        // The seal.
                        book.circle(Offset::new(140.0, 78.0), 26.0, alpha(*col, 0.85));
                        book.circle(Offset::new(140.0, 78.0), 18.0, alpha(Color::rgb(12, 13, 18), 0.9));
                        book.stroke(super::circle_path(140.0, 78.0, 18.0, 28), alpha(*col, 0.8), 1.4);
                        // The stamp shadow.
                        book.shadow(
                            xywh(6.0, 10.0, 268.0, 150.0),
                            14.0,
                            vieww_foundation::Shadow::new(alpha(Color::BLACK, 0.5), Offset::new(0.0, 18.0), 40.0),
                        );
                    }),
                )),
        );
        stack = stack.push(
            Positioned::new()
                .left(x + 30.0)
                .top(y + 30.0)
                .width(220.0)
                .height(34.0)
                .child(
                    Text::new(*ext)
                        .style(TextStyle::new(28.0).monospace().weight(FontWeight::Medium).color(alpha(INK, 0.97))),
                ),
        );
        stack = stack.push(
            Positioned::new()
                .left(x + 30.0)
                .top(y + 66.0)
                .width(220.0)
                .height(24.0)
                .child(
                    Text::new(*label)
                        .style(TextStyle::new(14.5).monospace().color(alpha(MUTED, 0.85))),
                ),
        );
    }

    // The headline.
    let head_a = clamp01((t - 0.04) / 0.16);
    stack = stack.push(
        Positioned::new()
            .left(0.0)
            .top(120.0)
            .width(1920.0)
            .height(44.0)
            .child(Opacity::new(head_a).child(
                Text::new("one click. every target.")
                    .style(TextStyle::new(34.0).letter_spacing(1.5).color(alpha(INK, 0.96)))
                    .align(TextAlign::Center),
            )),
    );

    stack = stack.push(super::caption(
        "every tool checked before a build starts — never a linker error mid-flight",
        1000.0,
        clamp01((t - 0.62) / 0.14),
    ));

    stack.into()
}
