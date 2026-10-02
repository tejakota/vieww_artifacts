//! S17 · DEVTOLS MIRROR — the twist. 2:42–2:51.
//!
//! *"The IDE you've been watching is a vieww app."* The inspector turns
//! on the studio itself: the chrome lifts and dims, and the three trees —
//! **Widget → Element → Render** — fan out from the studio's own title
//! bar as three glass planes, each carrying the tree glyphs of the very
//! scene you've been watching. Apart from them, the signal: a chip
//! reading the witness counter live — the state that survived every edit,
//! every descent, every device.
//!
//! K3, restaged from the plan: the studio is the dogfood, and devtools
//! proves it by eating itself.

use vieww_foundation::{Color, Offset, Size, Sketchbook, TextAlign, TextStyle, Transform};
use vieww_widget::prelude::*;
use vieww_widget::Transformed;
use vieww_widget::{Opacity, PaintWith, Painting};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, ease_out_back, tint, xywh, CYAN_SOFT, INK, MINT, MUTED,
    VIOLET_SOFT,
};

use super::studio;
use super::Ctx;

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;
    let ladder = ctx.ladder;

    // The lift: the studio shrinks and dims (0 → 0.30), the planes fan
    // out (0.24 → 0.62), the signal arrives (0.62 → 0.80).
    let lift = ease_in_out(clamp01(t / 0.24));
    let fan = ease_out_back(clamp01((t - 0.24) / 0.30));
    let signal_a = clamp01((t - 0.62) / 0.14);

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

    // The deep ground beneath everything.
    stack = stack.push(Positioned::fill().child(super::backdrop(t, 0x31D3, 70)));

    // The studio, lifted and dimmed.
    let k = 1.0 - 0.30 * lift;
    let scale_about = |cx: f32, cy: f32, s: f32| {
        Transform::translate(Offset::new(cx * (1.0 - s), cy * (1.0 - s)))
            .then(Transform::scale(s, s))
    };
    stack = stack.push(
        Positioned::fill().child(Opacity::new(1.0 - 0.45 * lift).child(
            Transformed::new(scale_about(960.0, 540.0, k)).child(studio::studio(abs, ladder, spec)),
        )),
    );

    // The three planes — Widget, Element, Render — fanning from the
    // studio's title bar. Each is a glass card with its tree's glyphs and
    // its own count (the trees' nodes for this very frame, drawn from the
    // scene's own composition).
    if fan > 0.0 {
        let planes: [(&str, Color, f32); 3] = [
            ("widget tree", VIOLET_SOFT, -1.0),
            ("element tree", CYAN_SOFT, 0.0),
            ("render tree", MINT, 1.0),
        ];
        for (i, (name, col, dir)) in planes.iter().enumerate() {
            // Copy out of the borrowed array — the closures below are
            // 'static and own what they capture.
            let name: &str = name;
            let col: Color = *col;
            let dir: f32 = *dir;
            let f = clamp01(fan - i as f32 * 0.08);
            if f <= 0.0 {
                continue;
            }
            // The plane's anchor: the studio's title bar, fanning outward.
            let spread = (i as f32 - 1.0) * 420.0 * f;
            let drop = 260.0 + i as f32 * 24.0;
            let x = 960.0 + spread - 240.0;
            let y = drop - (1.0 - f) * 80.0;
            let tilt = dir * 0.16 * (1.0 - f * 0.4);

            let card = Painting::sized(
                Size::new(480.0, 420.0),
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    // Glass.
                    book.rrect(
                        xywh(0.0, 0.0, 480.0, 420.0),
                        18.0,
                        alpha(Color::rgb(14, 15, 21), 0.92),
                    );
                    book.stroke_rrect(xywh(0.0, 0.0, 480.0, 420.0), 18.0, alpha(col, 0.5), 1.5);
                    // The header.
                    book.rect(
                        xywh(1.0, 1.0, 478.0, 44.0),
                        alpha(Color::rgb(17, 18, 25), 0.9),
                    );
                    book.rect(xywh(0.0, 44.0, 480.0, 1.0), alpha(Color::WHITE, 0.06));
                    // The tree glyphs — rows of dots, connected: this very
                    // frame's own shape, abstracted the way an inspector
                    // abstracts it.
                    let levels: [usize; 5] = [1, 2, 4, 6, 8];
                    let mut yy = 78.0;
                    for (li, n) in levels.iter().enumerate() {
                        let indent = 56.0 + li as f32 * 44.0;
                        for j in 0..*n {
                            let xx = indent + j as f32 * (400.0 - indent) / (*n).max(1) as f32;
                            let lit = ((li + j + i) % 3) != 0;
                            book.circle(
                                Offset::new(xx, yy),
                                if lit { 5.0 } else { 3.4 },
                                alpha(if lit { col } else { MUTED }, 0.8),
                            );
                            if lit {
                                book.line(
                                    Offset::new(xx - indent + 26.0, yy - 30.0),
                                    Offset::new(xx, yy - 6.0),
                                    alpha(col, 0.18),
                                    1.0,
                                );
                            }
                        }
                        yy += 62.0;
                    }
                    // The scan shimmer — the inspector's own refresh,
                    // breathing with the film clock.
                    let shim = 0.5 + 0.5 * (abs * 1.4 + i as f32).sin();
                    book.layer(1.0, 18.0, None, |g| {
                        g.rect(
                            xywh(0.0, 60.0, 480.0, 340.0),
                            alpha(col, 0.03 + 0.02 * shim),
                        );
                    });
                }),
            );
            // Tilt via a shear-ish transform (rotate slightly).
            let tr = Transform::translate(Offset::new(x, y))
                .then(Transform::rotate(tilt))
                .then(Transform::translate(Offset::new(240.0, 0.0)));
            stack = stack.push(
                Positioned::fill().child(
                    Opacity::new(clamp01(f * 2.0)).child(
                        Transformed::new(tr).child(
                            Stack::new()
                                .push(
                                    Positioned::new()
                                        .left(-240.0)
                                        .top(0.0)
                                        .width(480.0)
                                        .height(44.0)
                                        .child(
                                            Text::new(name)
                                                .style(
                                                    TextStyle::new(17.0)
                                                        .monospace()
                                                        .letter_spacing(2.2)
                                                        .color(alpha(INK, 0.95)),
                                                )
                                                .align(TextAlign::Center),
                                        ),
                                )
                                .push(
                                    Positioned::new()
                                        .left(-240.0)
                                        .top(0.0)
                                        .width(480.0)
                                        .height(444.0)
                                        .child(card),
                                ),
                        ),
                    ),
                ),
            );
        }
    }

    // The signal — apart from the trees, reading the witness live. The
    // state that survived everything: a chip with the count, and the line
    // back to the studio.
    if signal_a > 0.0 {
        let num = ladder;
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(856.0)
                .width(1920.0)
                .height(44.0)
                .child(
                    Opacity::new(signal_a).child(
                        Text::new(format!(
                            "signal<u32> = {} — the state that survived everything",
                            num
                        ))
                        .style(
                            TextStyle::new(24.0)
                                .monospace()
                                .letter_spacing(2.2)
                                .color(alpha(tint(VIOLET_SOFT, 0.2), 1.0)),
                        )
                        .align(TextAlign::Center),
                    ),
                ),
        );
        // The line from the signal up to the studio's preview.
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let mut p = vieww_foundation::Path::new();
                p.move_to(Offset::new(960.0, 850.0));
                p.line_to(Offset::new(960.0, 700.0));
                book.stroke_styled(
                    p,
                    alpha(VIOLET_SOFT, 0.7 * signal_a),
                    1.6,
                    vieww_foundation::StrokeStyle::rounded()
                        .dash(vieww_foundation::Dash::even(6.0).offset(-abs * 30.0)),
                );
                book.circle(Offset::new(960.0, 700.0), 4.0, alpha(VIOLET_SOFT, signal_a));
            }),
        )));
    }

    stack = stack.push(super::caption(
        "the studio is a vieww app — devtools proves it on itself",
        1000.0,
        clamp01((t - 0.30) / 0.12),
    ));
    stack = stack.push(super::caption(
        "three trees, one signal, identity that survives every rebuild",
        964.0,
        clamp01((t - 0.58) / 0.12),
    ));

    stack.into()
}
