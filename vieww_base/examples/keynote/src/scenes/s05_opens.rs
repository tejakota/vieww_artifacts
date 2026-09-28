//! **S05 · THE STUDIO OPENS** — 承, the session begins.
//!
//! The hero arrives. The window does not fade up: it *assembles*, panel by
//! panel, each one flying in from the edge it belongs to — the rail from the
//! left, the tree after it, the tab strip down from the title bar, the
//! preview in from the right, the status bar up from the floor. The order is
//! the order a person's attention travels, and the stagger is what makes a
//! screenshot into a machine.
//!
//! Two things are true from the first frame and neither is said out loud
//! yet: the window is drawn by vieww, and the chrome you are watching is a
//! widget tree, not an image. The film comes back for both (S11, S13).
//!
//! Nothing is typed here. The buffer is empty, the caret blinks, and the
//! preview says *waiting for a first paint* — which is the last time in the
//! film anything waits for anything.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Rect, Size, Sketchbook};
use vieww_widget::prelude::*;

use crate::film::Ctx;
use crate::kit::{
    act_card, alpha, caption, clamp01, dust, ease_out_cubic, ease_out_expo, grain, ground, horizon,
    mix, painter, seg, smoothstep, spring, vignette, xywh, Type, CYAN, H, INK, INK_SOFT, MUTED,
    VIOLET, VIOLET_SOFT, W,
};
use crate::studio::{self, compose, project_tree, status_line, FileKind, FocusOn, Layout, Shell};

/// The panels, and the beat each one lands on (scene-seconds).
const PANELS: [(f32, f32, f32); 6] = [
    // (arrive, from-x, from-y) — the edge each panel comes in from.
    (1.10, -340.0, 0.0),  // rail
    (1.32, -340.0, 0.0),  // sidebar
    (1.54, 0.0, -160.0),  // tabs
    (1.74, 0.0, 120.0),   // editor
    (1.96, 420.0, 0.0),   // preview
    (2.20, 0.0, 120.0),   // status
];

pub fn frame(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let t = ctx.t;
    let frame_i = ctx.frame;

    // The window's own arrival: a spring from slightly small, so the frame
    // has weight before the panels start.
    let win = spring(seg(sec, 0.35, 2.0), 12.0, 0.42);
    let assembly = ease_out_cubic(seg(sec, 0.35, 1.0));

    // Each panel's settle, on its own beat.
    let panel_p: Vec<f32> = PANELS
        .iter()
        .map(|(at, _, _)| spring(seg(sec, *at, at + 0.9), 15.0, 0.5))
        .collect();

    let l = Layout::new(64.0, 0.36, true);

    // The chrome's state: the project open, nothing typed.
    let mut sh = Shell {
        title: "counter — viewwstudio".into(),
        files: project_tree("main.say"),
        tabs: vec![("main.say".into(), FileKind::Say, true, false)],
        code: Vec::new(),
        caret: Some(0),
        caret_on: crate::kit::caret_on(ctx.abs, false),
        status: status_line("desktop · native", "60 fps", &[("no build running", MUTED)]),
        preview_label: "preview · desktop".into(),
        preview_chip: "waiting".into(),
        elapsed: Some(crate::kit::clock_mmss(ctx.spine.elapsed)),
        assembly,
        focus: (sec > 3.2).then_some(FocusOn::Code),
        panel: crate::studio::Panel {
            lines: crate::studio::output_lines(),
            ..crate::studio::Panel::default()
        },
        ..Shell::default()
    };
    // The tree reveals itself row by row as the sidebar lands.
    let rows = (sh.files.len() as f32 * ease_out_cubic(seg(sec, 1.5, 3.0))).ceil() as usize;
    sh.files.truncate(rows);

    let title_out = smoothstep(seg(t, 0.90, 1.0));
    let shell_for_paint = sh.clone();
    let p = panel_p.clone();

    let bg = painter(move |book: &mut Sketchbook, size: Size| {
        ground(book, size, sec, 0.8);
        horizon(book, size, VIOLET, 0.7);
        dust(book, size, sec, 34, VIOLET_SOFT, 0.55);

        // The window, drawn through the arrival transform: the whole studio
        // scales up from 0.93 on a spring. One transform, applied to the
        // chrome's own drawing, so nothing is composited after the fact.
        let k = 0.93 + 0.07 * win;
        let cx = size.width * 0.5;
        let cy = size.height * 0.5;
        book.transformed(
            vieww_foundation::Transform::scale_around(Offset::new(cx, cy), k, k),
            |g| {
                studio::back(g, &l, &shell_for_paint);

                // Each panel's own fly-in is drawn as a *mask* that slides
                // off it: the panel is already in place and a plate of the
                // window's ground covers what has not arrived. It is the
                // cheapest honest assembly — no panel is ever drawn twice.
                // Each card of the shell, in the order the eye travels.
                let covers: [(Rect, usize); 6] = [
                    (l.rail, 0),
                    (l.sidebar, 1),
                    (l.editor, 2),
                    (l.preview, 3),
                    (l.panel, 4),
                    (l.status, 5),
                ];
                for (r, i) in covers {
                    let done = p[i];
                    if done >= 0.999 {
                        continue;
                    }
                    let (_, fx, fy) = PANELS[i];
                    let off_x = fx * (1.0 - done);
                    let off_y = fy * (1.0 - done);
                    // The ground plate, where the card has not yet reached —
                    // the window's own colour, so an unarrived region reads as
                    // empty shell rather than as a hole.
                    g.rrect(
                        Rect::new(r.left - 1.0, r.top - 1.0, r.right + 1.0, r.bottom + 1.0),
                        crate::studio::CARD_CORNER,
                        alpha(crate::studio::WINDOW, (1.0 - done).min(1.0)),
                    );
                    // The panel's leading edge — a bright seam travelling
                    // with it, which is what makes the motion readable at
                    // 60 fps rather than a smear.
                    if done > 0.02 && done < 0.98 {
                        let seam = if fx.abs() > 1.0 {
                            xywh(
                                if fx < 0.0 { r.left + r.width() * done } else { r.right - r.width() * done },
                                r.top,
                                2.0,
                                r.height(),
                            )
                        } else {
                            xywh(
                                r.left,
                                if fy < 0.0 { r.top + r.height() * done } else { r.bottom - r.height() * done },
                                r.width(),
                                2.0,
                            )
                        };
                        g.rect(seam, alpha(VIOLET_SOFT, 0.55 * (1.0 - done)));
                    }
                    let _ = (off_x, off_y);
                }

                studio::front(g, &l, &shell_for_paint);
            },
        );

        // The window's own light on the ground beneath it — one blurred
        // group, and the reason the studio sits in a room rather than on a
        // slide.
        book.blended_layer(1.0, 70.0, BlendMode::Plus, None, |g| {
            g.rrect(
                xywh(l.window.left + 60.0, l.window.bottom - 40.0, l.window.width() - 120.0, 120.0),
                60.0,
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, alpha(VIOLET, 0.10 * win)),
                    (1.0, alpha(VIOLET, 0.0)),
                ]),
            );
        });

        grain(book, size, frame_i, 0.012, 300);
        vignette(book, size, 0.85);
        if title_out > 0.004 {
            book.rect(
                Rect::new(0.0, 0.0, size.width, size.height),
                alpha(Color::BLACK, title_out * 0.35),
            );
        }
    });

    // The studio's glyphs, drawn through the same arrival scale so the type
    // travels with its panel.
    let k = 0.93 + 0.07 * win;
    let mut nodes: Vec<WidgetNode> = Vec::new();
    let text_nodes = studio::text(&l, &sh);
    let mut inner = Stack::new();
    for n in text_nodes {
        inner = inner.push(Positioned::fill().child(n));
    }
    nodes.push(
        vieww_widget::Transformed::new(vieww_foundation::Transform::scale_around(
            Offset::new(W * 0.5, H * 0.5),
            k,
            k,
        ))
        .child(inner)
        .into(),
    );

    // The preview's own placeholder: the last wait in the film, and it is
    // one line long.
    let wait_a = ease_out_expo(seg(sec, 2.6, 3.4)) * (1.0 - smoothstep(seg(sec, 14.4, 15.4)));
    nodes.push(
        Type::new("waiting for a first paint")
            .mono()
            .size(15.0)
            .track(2.4)
            .color(alpha(MUTED, 0.7 * wait_a))
            .center()
            .at(l.preview_body.left, l.preview_body.top + l.preview_body.height() * 0.5 - 10.0)
            .width(l.preview_body.width())
            .into(),
    );

    // The name, once, over the assembled window — then it leaves and does
    // not come back until the end card.
    let name_a = smoothstep(seg(sec, 4.4, 5.6)) * (1.0 - smoothstep(seg(sec, 10.0, 11.4)));
    if name_a > 0.004 {
        nodes.push(
            Type::new("viewwstudio")
                .size(64.0)
                .medium()
                .track(1.5)
                .color(alpha(INK, 0.97 * name_a))
                .center()
                .banner(H * 0.40)
                .into(),
        );
        nodes.push(
            Type::new("an IDE that is itself a vieww app")
                .mono()
                .size(17.0)
                .track(4.0)
                .color(alpha(CYAN, 0.9 * name_a))
                .center()
                .banner(H * 0.40 + 86.0)
                .into(),
        );
        // The scrim that makes the name readable is drawn over the window,
        // not over the frame — the studio dims, the room does not.
        nodes.push(
            Stack::new()
                .push(
                    Positioned::fill().child(
                        crate::kit::painter(move |book: &mut Sketchbook, size: Size| {
                            let _ = size;
                            book.rrect(l.window, 16.0, alpha(Color::rgb(6, 6, 10), 0.72 * name_a));
                        }),
                    ),
                )
                .into(),
        );
        // Re-issue the name above its own scrim.
        nodes.push(
            Type::new("viewwstudio")
                .size(64.0)
                .medium()
                .track(1.5)
                .color(alpha(INK, 0.97 * name_a))
                .center()
                .banner(H * 0.40)
                .into(),
        );
        nodes.push(
            Type::new("an IDE that is itself a vieww app")
                .mono()
                .size(17.0)
                .track(4.0)
                .color(alpha(CYAN, 0.9 * name_a))
                .center()
                .banner(H * 0.40 + 86.0)
                .into(),
        );
    }

    nodes.push(act_card("ACT II", "THE STUDIO", "承", smoothstep(seg(sec, 0.6, 1.6)) * (1.0 - smoothstep(seg(sec, 6.0, 7.2)))));
    nodes.push(caption(
        "one session starts here · it does not restart again",
        smoothstep(seg(sec, 12.0, 13.0)) * (1.0 - smoothstep(seg(t, 0.94, 1.0))),
    ));
    let _ = (mix, clamp01, INK_SOFT);
    compose(bg, nodes)
}
