//! S16 · THE LOOP — the final recursion. 2:52–3:00.
//!
//! The camera pulls back from the end card — and the end card is **one
//! more buffer open in the studio**: the tab says `endcard`, the session
//! chip reads **3:00** (the session's own total, emitted by the scene
//! table), and the session never ended — it rendered the film.
//!
//! The film closes by fading on the wordmark's reflection: the genesis
//! image (a drop, a word) returned as a mirror in the studio's floor.
//! The tool that escaped the wait is the tool that rendered the film
//! about escaping it.

use vieww_foundation::{Color, Offset, Size, Sketchbook, TextAlign, TextStyle, Transform, FontWeight};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};
use vieww_widget::Transformed;

use crate::film_lib::{alpha, clamp01, ease_in_out, mix, smoothstep, tint, xywh, INK, MUTED, VIOLET_SOFT};

use super::studio::{studio, App, Code, Spec};
use super::{caption, Ctx};

/// Scale about a point.
fn scale_about(cx: f32, cy: f32, s: f32) -> Transform {
    Transform::translate(Offset::new(cx * (1.0 - s), cy * (1.0 - s)))
        .then(Transform::scale(s, s))
}

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;

    // The studio — the session's last frame. The chip reads the session's
    // own total (the scene table's sum — emitted), which is 3:00.
    let mut app = App::new(ctx.ladder, super::tap_pulse(abs), abs);
    app.alive = 1.0;
    app.dial = 1.0;
    app.spring = 1.0;
    let spec = Spec {
        code: Code::Say { typed: 1.0, blink: ctx.sec },
        app,
        session_line: 1.0,
        tab: Some("endcard".to_string()),
        ..Spec::default()
    };

    let mut stack = Stack::new().push(studio(abs, ctx.ladder, spec));

    // The end card, receding — the same card S15 held, pulling back into
    // the studio's preview until it is one pane among panes.
    let recede = ease_in_out(clamp01(t / 0.55));
    let k = 1.0 + 1.1 * recede;
    let card_a = 1.0 - smoothstep(clamp01((t - 0.30) / 0.30));

    if card_a > 0.01 {
        let card = Stack::new()
            .push(
                Positioned::new().left(0.0).top(360.0).width(1920.0).height(170.0).child(
                    Text::new("vieww")
                        .style(
                            TextStyle::new(128.0)
                                .weight(FontWeight::Regular)
                                .letter_spacing(18.0)
                                .color(alpha(INK, 0.95)),
                        )
                        .align(TextAlign::Center),
                ),
            )
            .push(
                Positioned::new().left(0.0).top(540.0).width(1920.0).height(30.0).child(
                    Text::new("this film was rendered with vieww")
                        .style(TextStyle::new(28.0).monospace().letter_spacing(4.0).color(alpha(tint(VIOLET_SOFT, 0.2), 0.9)))
                        .align(TextAlign::Center),
                ),
            );
        stack = stack.push(Positioned::fill().child(
            Opacity::new(card_a).child(Transformed::new(scale_about(960.0, 470.0, k)).child(card)),
        ));
    }

    // The caption — the loop, named once.
    stack = stack.push(caption(
        "the session never ended",
        964.0,
        clamp01((t - 0.52) / 0.10),
    ));

    // The wordmark's reflection — in the studio's floor, fading: the last
    // image of the film. A mirrored mark under a floor line, dissolving.
    let refl_a = clamp01((t - 0.55) / 0.15) * (1.0 - clamp01((t - 0.90) / 0.10));
    if refl_a > 0.01 {
        let floor_y = 968.0;
        stack = stack.push(
            Positioned::fill().child(Painting::sized(
                super::CANVAS,
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    // The floor line.
                    book.line(
                        Offset::new(560.0, floor_y),
                        Offset::new(1360.0, floor_y),
                        alpha(Color::WHITE, 0.10),
                        1.0,
                    );
                    // The reflection's glow — soft, sinking.
                    super::glow(book, 960.0, floor_y + 40.0, 300.0, VIOLET_SOFT, 0.12 * refl_a);
                }),
            )),
        );
        // The mirrored mark — a text node flipped about the floor line.
        let mark_h = 120.0;
        let flip = Transform::translate(Offset::new(0.0, 2.0 * (floor_y + 14.0)))
            .then(Transform::scale(1.0, -1.0));
        stack = stack.push(
            Positioned::fill().child(
                Opacity::new(refl_a * 0.30).child(
                    Transformed::new(flip).child(
                        Positioned::new().left(0.0).top(floor_y + 14.0).width(1920.0).height(mark_h).child(
                            Text::new("vieww")
                                .style(
                                    TextStyle::new(96.0)
                                        .weight(FontWeight::Regular)
                                        .letter_spacing(14.0)
                                        .color(INK),
                                )
                                .align(TextAlign::Center),
                        ),
                    ),
                ),
            ),
        );
    }

    // The final fade — the film's last breath, on the reflection.
    let fade = 1.0 - ease_in_out(clamp01((t - 0.93) / 0.07));
    if fade < 1.0 {
        stack = stack.push(
            Positioned::fill().child(
                Opacity::new(1.0 - fade).child(
                    Container::new().size(1920.0, 1080.0).color(Color::rgb(4, 4, 6)),
                ),
            ),
        );
    }

    stack.into()
}
