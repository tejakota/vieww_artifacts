//! S11 · THE MIRROR — Ten opens. 2:05–2:13.
//!
//! Devtools turns on the studio itself (K3): the inspector slides in, and
//! the IDE you've been watching since S04 turns out to be a vieww app —
//! its own damage lights up as the panel opens (E-12: the opening *was* a
//! write), and its tree is laid out in the reveal's three colors before
//! the reveal proper says what they mean.
//!
//! One twist, eight seconds: everything the film has shown you was the
//! product drawing itself.

use vieww_foundation::{Color, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use crate::film_lib::{alpha, clamp01, ease_out_back, tint, xywh, INK, VIOLET_SOFT};

use super::studio::{studio, App, Code, Spec};
use super::{caption, Ctx, C_DAMAGE, C_DESC, C_GEOM, C_IDENT, C_SIGNAL};

/// The inspector panel's rect — full-frame coordinates. Its own opening
/// is the write whose damage lights (E-12).
fn panel_rect() -> Rect {
    xywh(1188.0, 78.0, 672.0, 880.0)
}

/// One tree row of the studio's own widget tree.
struct Row {
    depth: usize,
    label: &'static str,
    color: Color,
}

fn tree_rows() -> Vec<Row> {
    vec![
        Row {
            depth: 0,
            label: "viewwstudio",
            color: C_DESC,
        },
        Row {
            depth: 1,
            label: "TitleBar",
            color: C_DESC,
        },
        Row {
            depth: 1,
            label: "ActivityBar",
            color: C_DESC,
        },
        Row {
            depth: 1,
            label: "EditorPane",
            color: C_GEOM,
        },
        Row {
            depth: 2,
            label: "buffer counter.say",
            color: C_IDENT,
        },
        Row {
            depth: 2,
            label: "Text · say code",
            color: C_IDENT,
        },
        Row {
            depth: 1,
            label: "PreviewPane",
            color: C_GEOM,
        },
        Row {
            depth: 2,
            label: "column",
            color: C_GEOM,
        },
        Row {
            depth: 3,
            label: "heading \"Counter\"",
            color: C_DESC,
        },
        Row {
            depth: 3,
            label: "label \"Tapped …\"",
            color: C_DESC,
        },
        Row {
            depth: 3,
            label: "button \"Add one\"",
            color: C_GEOM,
        },
        Row {
            depth: 2,
            label: "Signal COUNT",
            color: C_SIGNAL,
        },
    ]
}

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;

    // The studio, still live, still the same session.
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

    let mut stack = Stack::new().push(studio(abs, ctx.ladder, spec));

    // The inspector — slides in from the right, sprung, and the studio's
    // own damage lights: opening the panel was a write (E-12).
    let slide = ease_out_back(clamp01(t / 0.22));
    if slide > 0.0 {
        let r = panel_rect();
        let x = r.left + (1.0 - slide) * 700.0;

        // The damage — the panel's own footprint, outlined in the reveal's
        // damage color the moment it lands.
        let damage_a = clamp01((t - 0.10) / 0.06) * (1.0 - clamp01((t - 0.45) / 0.20));

        stack = stack.push(
            Positioned::new()
                .left(x)
                .top(r.top)
                .width(r.width())
                .height(r.height())
                .child(Painting::sized(
                    Size::new(r.width(), r.height()),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        // The panel body — frosted, over the studio.
                        book.rrect(
                            xywh(0.0, 0.0, 672.0, 880.0),
                            14.0,
                            alpha(Color::rgb(14, 14, 19), 0.965),
                        );
                        book.stroke_rrect(
                            xywh(0.0, 0.0, 672.0, 880.0),
                            14.0,
                            alpha(Color::WHITE, 0.10),
                            1.0,
                        );
                        // The header rule.
                        book.rect(xywh(0.0, 52.0, 672.0, 1.0), alpha(Color::WHITE, 0.07));
                    }),
                )),
        );

        // The tree rows — the studio's own widget tree, the reveal's palette.
        let rows = tree_rows();
        for (i, row) in rows.iter().enumerate() {
            let appear = clamp01((t - 0.12 - i as f32 * 0.018) / 0.10);
            if appear <= 0.0 {
                continue;
            }
            let ry = 78.0 + 66.0 + i as f32 * 56.0;
            let indent = 26.0 + row.depth as f32 * 34.0;
            stack = stack.push(
                Positioned::new()
                    .left(x + indent)
                    .top(ry)
                    .width(600.0 - indent)
                    .height(30.0)
                    .child(
                        Opacity::new(appear).child(
                            Text::new(row.label)
                                .style(TextStyle::new(19.0).monospace().color(alpha(INK, 0.88))),
                        ),
                    ),
            );
            // The node glyph — a small rounded rect in the row's layer color.
            let glyph_color = row.color;
            let g_a = appear;
            stack = stack.push(
                Positioned::new()
                    .left(x + indent - 24.0)
                    .top(ry + 7.0)
                    .width(14.0)
                    .height(14.0)
                    .child(Painting::sized(
                        Size::new(14.0, 14.0),
                        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                            book.rrect(
                                xywh(0.0, 0.0, 14.0, 14.0),
                                4.0,
                                alpha(glyph_color, 0.9 * g_a),
                            );
                        }),
                    )),
            );
        }

        // The panel header + LIVE chip.
        stack = stack
            .push(
                Positioned::new()
                    .left(x + 22.0)
                    .top(96.0)
                    .width(420.0)
                    .height(28.0)
                    .child(
                        Text::new("inspector · viewwstudio (self)").style(
                            TextStyle::new(20.0)
                                .monospace()
                                .letter_spacing(1.4)
                                .color(alpha(INK, 0.92)),
                        ),
                    ),
            )
            .push(
                Positioned::new()
                    .left(x + 560.0)
                    .top(94.0)
                    .width(90.0)
                    .height(30.0)
                    .child(super::chip("live", 14.0, tint(VIOLET_SOFT, 0.25))),
            );

        // The damage outline — the studio inspecting itself.
        if damage_a > 0.0 {
            stack = stack.push(
                Positioned::new()
                    .left(r.left - 3.0)
                    .top(r.top - 3.0)
                    .width(r.width() + 6.0)
                    .height(r.height() + 6.0)
                    .child(Painting::sized(
                        Size::new(r.width() + 6.0, r.height() + 6.0),
                        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                            book.stroke_rrect(
                                xywh(3.0, 3.0, 672.0, 880.0),
                                14.0,
                                alpha(C_DAMAGE, damage_a * 0.85),
                                1.6,
                            );
                        }),
                    )),
            );
        }
    }

    // K3's caption — the twist, said once, center-adjacent.
    stack = stack.push(caption(
        "the studio is a vieww app",
        964.0,
        clamp01((t - 0.38) / 0.08),
    ));

    stack.into()
}
