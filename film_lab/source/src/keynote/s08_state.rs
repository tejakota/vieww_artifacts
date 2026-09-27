//! S08 · STATE SURVIVES — the quiet carry. 1:29–1:40.
//!
//! Edited mid-flight: a highlight sweeps the column line, the layout
//! re-flows — and the spring continues, the dial keeps its arc, the
//! counter holds. *No effect on purpose — the carry is the shot.* The
//! spring toy's phase comes from the session clock (absolute film time):
//! it cannot restart, because its phase was never the scene's to reset.
//!
//! Tap → **4**. The tether glow between buffer and preview is the only
//! ornament — and it is the point.

use vieww_foundation::{Color, Offset, Sketchbook, Size};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{alpha, clamp01, ease_in_out, xywh, VIOLET, VIOLET_SOFT};

use super::studio::{studio, App, Code, Spec};
use super::{caption, Ctx};

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;

    // The edit — a highlight band sweeps line 3 (the column's spacing),
    // and the layout re-flows: the app's column opens from 16 to 24.
    let edit = ease_in_out(clamp01((t - 0.22) / 0.16));

    let mut app = App::new(ctx.ladder, super::tap_pulse(abs), abs);
    app.alive = 1.0;
    app.dial = 1.0;
    app.spring = 1.0;
    app.spacing = edit;

    let spec = Spec {
        code: Code::Say { typed: 1.0, blink: ctx.sec },
        app,
        session_line: clamp01((abs - 26.0) / 99.0),
        ..Spec::default()
    };

    let mut stack = Stack::new().push(studio(abs, ctx.ladder, spec));

    // The edit highlight — a soft violet band over the column line.
    if edit > 0.0 && edit < 1.0 {
        let x = 118.0 + edit * 780.0;
        stack = stack.push(
            Positioned::new().left(0.0).top(94.0).width(940.0).height(940.0).child(
                Painting::sized(Size::new(940.0, 940.0), PaintWith::new(
                    move |book: &mut Sketchbook, _s: Size| {
                        let band_y = 10.0 + 3.0 * 37.0 + 8.0;
                        book.layer(1.0, 0.0, None, |g| {
                            g.rrect(xywh(x - 40.0, band_y, 130.0, 26.0), 6.0, alpha(VIOLET, 0.16));
                        });
                    },
                )),
            ),
        );
    }

    // The tether — the carry made visible: a thread from the swept line to
    // the preview's column, with a slow pulse riding it.
    {
        let pulse = (t * 2.4).fract();
        stack = stack.push(
            Positioned::fill().child(Painting::sized(
                super::CANVAS,
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    let a = Offset::new(906.0, 240.0);
                    let b = Offset::new(1300.0, 300.0);
                    let mut p = vieww_foundation::Path::new();
                    p.move_to(a);
                    p.line_to(Offset::new(1080.0, 208.0));
                    p.line_to(b);
                    book.stroke(p, alpha(VIOLET_SOFT, 0.35), 1.4);
                    // The riding pulse.
                    let px = a.dx + (b.dx - a.dx) * pulse;
                    let py = a.dy + (b.dy - a.dy) * pulse - 26.0 * (pulse * (1.0 - pulse) * 4.0);
                    book.circle(Offset::new(px, py), 4.0, alpha(VIOLET_SOFT, 0.9));
                }),
            )),
        );
    }

    stack = stack.push(caption(
        "the spring never restarted",
        964.0,
        clamp01((t - 0.16) / 0.08),
    ));

    stack.into()
}
