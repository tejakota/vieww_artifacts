//! S10 · STUDIO OPENS — Act III, the hero arrives. 1:28–1:38.
//!
//! From the bloom of Act II's engine, the product assembles: the
//! viewwstudio mark draws on, the splash lifts, and the IDE builds itself
//! — title bar, activity rail, file tree, editor, preview glass — every
//! pane arriving on its own spring (the springs are the studio's own
//! animation crate, and the film's). The command palette flashes once,
//! the way a real opening does; the session chip ticks to life; the
//! buffer waits, empty, caret breathing.
//!
//! Caption: **this is where you'll build.**

use vieww_foundation::{Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, ease_out_back, xywh, INK, MUTED, VIOLET, VIOLET_SOFT,
};

use super::studio;
use super::Ctx;

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;
    let ladder = ctx.ladder;

    // The splash (0 → 0.34) then the assembly (0.30 → 0.78) then life.
    let splash_a = clamp01(t / 0.06);
    let splash_out = ease_in_out(clamp01((t - 0.30) / 0.18));
    let draw_mark = clamp01((t - 0.03) / 0.14);
    let name_a = clamp01((t - 0.13) / 0.10);

    // The studio spec — everything arriving.
    let chrome_in = ease_out_back(clamp01((t - 0.36) / 0.26));
    let spec = studio::Spec {
        code: studio::Code::Say {
            typed: 0.0,
            blink: ctx.sec,
        },
        app: {
            let mut app = studio::App::new(0, 0.0, abs);
            app.alive = 0.0;
            app
        },
        editor_blur: (1.0 - chrome_in) * 6.0,
        preview_blur: (1.0 - chrome_in) * 5.0,
        palette: clamp01((t - 0.62) / 0.06) * (1.0 - clamp01((t - 0.78) / 0.08)),
        rail_lit: (chrome_in * 3.0) as usize,
        tree: clamp01((t - 0.44) / 0.22),
        session_line: clamp01((t - 0.70) / 0.20),
        ..Default::default()
    };

    let mut stack = Stack::new();

    // The studio itself — arrives with the assembly.
    let studio_a = clamp01(chrome_in * 2.0);
    if studio_a > 0.0 {
        let rise = (1.0 - chrome_in) * 90.0;
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(rise)
                .width(1920.0)
                .height(1080.0)
                .child(Opacity::new(studio_a).child(studio::studio(abs, ladder, spec))),
        );
    } else {
        // Pre-assembly: the deep ground alone.
        stack = stack.push(Positioned::fill().child(super::backdrop(t, 0xA11C, 60)));
    }

    // The splash — the mark + the name, center frame, lifting away.
    if splash_out < 1.0 {
        let a = splash_a * (1.0 - splash_out);
        let scale = 1.0 + splash_out * 0.5;
        let y = 400.0 - splash_out * 120.0;
        stack = stack.push(
            Positioned::fill().child(
                Opacity::new(a).child(
                    Positioned::new()
                        .left(0.0)
                        .top(y)
                        .width(1920.0)
                        .height(320.0)
                        .child(Painting::sized(
                            Size::new(1920.0, 320.0),
                            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                                // The mark — stroke-on, at splash scale.
                                let cx = 960.0;
                                let cy = 90.0;
                                let sz = 120.0 * scale;
                                studio::draw_mark(book, cx, cy, sz, VIOLET_SOFT, draw_mark);
                                // The splash glow behind it.
                                super::glow(book, cx, cy, 320.0 * scale, VIOLET, 0.20);
                            }),
                        )),
                ),
            ),
        );
        // The name.
        stack = stack.push(
            Positioned::fill().child(
                Opacity::new(a).child(
                    Positioned::new()
                        .left(0.0)
                        .top(y + 190.0)
                        .width(1920.0)
                        .height(60.0)
                        .child(
                            Text::new("viewwstudio")
                                .style(
                                    TextStyle::new(56.0)
                                        .letter_spacing(6.0)
                                        .color(alpha(INK, name_a)),
                                )
                                .align(TextAlign::Center),
                        ),
                ),
            ),
        );
        // The tagline.
        stack = stack.push(
            Positioned::fill().child(
                Opacity::new(a).child(
                    Positioned::new()
                        .left(0.0)
                        .top(y + 262.0)
                        .width(1920.0)
                        .height(34.0)
                        .child(
                            Text::new("build the picture. see the picture.")
                                .style(
                                    TextStyle::new(20.0)
                                        .monospace()
                                        .letter_spacing(3.0)
                                        .color(alpha(MUTED, name_a)),
                                )
                                .align(TextAlign::Center),
                        ),
                ),
            ),
        );
    }

    // The welcome breath — a light wash crossing the assembled studio.
    let wash = clamp01((t - 0.80) / 0.16);
    if wash > 0.0 && wash < 1.0 {
        let x = -300.0 + wash * 2400.0;
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                book.layer(1.0, 40.0, None, |g| {
                    g.rect(xywh(x - 200.0, 0.0, 400.0, 1080.0), alpha(VIOLET, 0.06));
                });
            }),
        )));
    }

    stack = stack.push(super::act_chip(
        "III",
        "THE STUDIO",
        clamp01((t - 0.40) / 0.4),
    ));
    stack = stack.push(super::caption(
        "viewwstudio. this is where you build.",
        1000.0,
        clamp01((t - 0.72) / 0.14),
    ));

    stack.into()
}
