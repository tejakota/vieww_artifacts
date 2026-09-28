//! **S06 · FIRST PAINT** — three lines of `say`, and a program that is alive.
//!
//! This is the scene the product exists for. Three lines type themselves
//! into an empty buffer; before the third one is finished the preview has
//! already painted, and the caption states the cost of that in seconds —
//! **a number the film measured of itself, not one anybody typed.**
//!
//! `ctx.probe.alive_seconds` comes from pass 1: the census renders this
//! scene's own first-paint window at master resolution and takes the median.
//! If the census has not run, the caption says so rather than inventing a
//! figure — rule 5 of the ledger, made mechanical, in the one place in the
//! film where a number is load-bearing.
//!
//! The twist inside the beat (K0): the first code a Rust framework shows is
//! not Rust. That is deliberate, and S08 pays it off.
//!
//! The session's first human touch lands here: a tap on the preview's own
//! button, and the witness counter turns to **1**. It is never reset again.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Rect, Size, Sketchbook};
use vieww_widget::prelude::*;

use crate::film::Ctx;
use crate::kit::{
    alpha, bump, caption, clamp01, dust, ease_out_cubic, ease_out_expo, grain, ground, horizon,
    painter, seg, smoothstep, spring, vignette, xywh, Type, CYAN, CYAN_SOFT, H, INK, INK_SOFT,
    MINT, MUTED, VIOLET, VIOLET_SOFT, W,
};
use crate::scenes::buffer;
use crate::studio::{
    self, compose, project_tree, status_line, witness_chip, App, FileKind, FocusOn, Layout, Shell,
};

/// The type-on: when it starts, and how long the three lines take.
const TYPE_T0: f32 = 1.30;
const TYPE_T1: f32 = 6.40;
/// When the preview paints. Deliberately *before* the buffer is finished —
/// the preview is not waiting for a save.
const ALIVE_T: f32 = 5.35;
/// The tap.
const TAP_T: f32 = 14.88;

pub fn frame(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let frame_i = ctx.frame;

    let total = buffer::first_paint_len();
    let typed = (total as f32 * ease_out_cubic(seg(sec, TYPE_T0, TYPE_T1)).min(1.0)) as usize;
    let typing = sec > TYPE_T0 && sec < TYPE_T1;
    let code = buffer::first_paint_buffer(typed);
    let caret_line = code.len().saturating_sub(1);

    // The preview's own life: it paints on ALIVE_T and then keeps running.
    let alive = smoothstep(seg(sec, ALIVE_T, ALIVE_T + 0.55));
    let tap = seg(sec, TAP_T, TAP_T + 0.7);
    // The statement's own scrim strength — one value, so the scrim and the
    // sentence can never disagree about when they are on screen.
    let third = smoothstep(seg(sec, ALIVE_T + 0.45, ALIVE_T + 1.3)) * (1.0 - smoothstep(seg(sec, 13.2, 14.2)));
    let press = (sec > TAP_T && sec < TAP_T + 0.7).then_some(tap);

    // The app: the counter, at whatever the session's witness count is. The
    // number in the preview and the number in the chip are the same value
    // read from the spine — the film does not keep two copies of its own
    // continuity.
    let app = App {
        value: ctx.spine.witness,
        alive,
        press,
        accent: VIOLET,
        // The spring the little app runs on its own, so the preview is never
        // a still image.
        spring: 0.5 + 0.5 * spring(((sec - ALIVE_T) * 0.36).fract(), 9.0, 0.30).clamp(-1.0, 1.0),
        ..App::default()
    };

    let l = Layout::new(64.0, 0.36, true);
    let sh = Shell {
        files: project_tree("main.say"),
        tabs: vec![
            ("main.say".into(), FileKind::Say, true, typed > 0),
            ("counter.say".into(), FileKind::Say, false, false),
        ],
        code,
        caret: Some(caret_line),
        caret_on: crate::kit::caret_on(ctx.abs, typing),
        changed: (0..caret_line.min(3)).collect(),
        status: status_line(
            "desktop · native",
            "60 fps",
            &[if alive > 0.5 { ("preview live", MINT) } else { ("preview idle", MUTED) }],
        ),
        preview_label: "preview · desktop".into(),
        preview_chip: if alive > 0.5 { "live".into() } else { "waiting".into() },
        witness: (ctx.spine.witness > 0).then_some(ctx.spine.witness),
        elapsed: Some(crate::kit::clock_mmss(ctx.spine.elapsed)),
        focus: Some(if sec > TAP_T - 1.2 { FocusOn::Preview } else { FocusOn::Code }),
        panel: crate::studio::Panel {
            lines: crate::studio::output_lines(),
            ..crate::studio::Panel::default()
        },
        ..Shell::default()
    };

    let shell_paint = sh.clone();
    let body = l.preview_body;
    // The app's own card inside the preview body — the pane is the device,
    // the card is the window the app runs in.
    let card = Rect::new(
        body.left + 44.0,
        body.top + 40.0,
        body.right - 44.0,
        body.bottom - 52.0,
    );
    let app_paint = App { ..app };

    let bg = painter(move |book: &mut Sketchbook, size: Size| {
        ground(book, size, sec, 0.85);
        horizon(book, size, VIOLET, 0.75);
        dust(book, size, sec, 30, VIOLET_SOFT, 0.5);

        studio::back(book, &l, &shell_paint);

        // The preview's content — painted between the chrome's two halves,
        // which is what makes the pane a window onto a running program
        // rather than a picture pasted over one.
        book.rrect(card, 10.0, alpha(Color::rgb(14, 15, 21), 0.9));
        book.stroke_rrect(card, 10.0, alpha(Color::rgb(40, 44, 58), 0.9), 1.0);
        studio::app(book, Rect::new(card.left + 1.0, card.top + 1.0, card.right - 1.0, card.bottom - 1.0), &app_paint);

        // The paint-in itself: a wipe of light crossing the card once, on
        // the frame the program comes alive. The preview does not fade up —
        // it *paints*, left to right, the way a frame is rasterized.
        let wipe = seg(sec, ALIVE_T - 0.05, ALIVE_T + 0.50);
        if wipe > 0.001 && wipe < 1.0 {
            let x = card.left + card.width() * ease_out_expo(wipe);
            book.blended_layer(1.0, 18.0, BlendMode::Plus, None, |g| {
                g.rect(
                    xywh(x - 70.0, card.top, 70.0, card.height()),
                    Gradient::horizontal().with_dither().with_stops(&[
                        (0.0, alpha(CYAN, 0.0)),
                        (1.0, alpha(CYAN_SOFT, 0.55)),
                    ]),
                );
                g.rect(xywh(x, card.top, 2.5, card.height()), alpha(Color::WHITE, 0.75));
            });
        }

        studio::front(book, &l, &shell_paint);

        // The tap ripple, over everything: a real pointer landing on a real
        // button. The counter increments because the session did this, not
        // because the film cut to a new number.
        if let Some(p) = press {
            let bw = (card.width() * 0.34).min(240.0);
            let cx = card.left + card.width() * 0.5;
            let cy = card.top + card.height() * 0.42;
            let dial_r = (card.width().min(card.height()) * 0.20).max(24.0);
            let by = cy + dial_r + 34.0 + 8.0 + 30.0 + 26.0;
            studio::touch(book, Offset::new(cx, by), p, VIOLET_SOFT);
            let _ = bw;
        }

        // The moment the counter turns: one soft pulse through the whole
        // window, so the continuity marker is felt and not just read.
        let turn = bump(seg(sec, TAP_T + 0.05, TAP_T + 0.85), 0.0, 1.0);
        if turn > 0.01 {
            book.rrect(l.window, 16.0, alpha(VIOLET, 0.035 * turn));
        }

        crate::kit::lower_third(book, l.editor, third);
        grain(book, size, frame_i, 0.012, 300);
        vignette(book, size, 0.85);
    });

    let mut nodes: Vec<WidgetNode> = studio::text(&l, &sh);
    nodes.extend(studio::app_text(
        Rect::new(card.left + 1.0, card.top + 1.0, card.right - 1.0, card.bottom - 1.0),
        &app,
    ));

    // The language chip: the first code in this film is not Rust, and the
    // film says so where the eye already is.
    let chip = smoothstep(seg(sec, 1.9, 2.7)) * (1.0 - smoothstep(seg(sec, 11.0, 12.0)));
    nodes.push(
        Type::new("say — the studio's fast loop")
            .mono()
            .size(13.0)
            .track(2.2)
            .color(alpha(VIOLET_SOFT, 0.85 * chip))
            .at(l.code.left + 22.0, l.code.bottom - 44.0)
            .width(520.0)
            .into(),
    );

    // **The number.** Measured by the census, printed here, and nowhere
    // else in the film is a figure more load-bearing.
    let num_a = third;
    if num_a > 0.004 {
        let measured = ctx.probe.is_measured() && ctx.probe.alive_seconds > 0.0;
        let text = if measured {
            format!("alive in {} seconds", ctx.probe.alive_text())
        } else {
            "alive — census not run".to_string()
        };
        nodes.push(
            Type::new(text)
                .size(40.0)
                .light()
                .track(0.5)
                .color(alpha(INK, 0.97 * num_a))
                .center()
                .at(l.editor.left, l.editor.bottom - 104.0)
                .width(l.editor.width())
                .into(),
        );
        nodes.push(
            Type::new(if measured {
                "nothing was compiled · the figure is this render's own, measured at 1920×1080"
            } else {
                "run the census: this caption prints only what the film measured"
            })
            .mono()
            .size(14.0)
            .track(2.4)
            .color(alpha(if measured { CYAN } else { MUTED }, 0.85 * num_a))
            .center()
            .at(l.editor.left, l.editor.bottom - 60.0)
            .width(l.editor.width())
            .into(),
        );
    }

    // The counter chip, once the first touch has landed.
    if ctx.spine.witness >= 1 {
        let pop = spring(seg(sec, TAP_T + 0.1, TAP_T + 0.9), 16.0, 0.42);
        nodes.push(witness_chip(body, ctx.spine.witness, pop.clamp(0.0, 1.0)));
    }

    nodes.push(caption(
        if sec < ALIVE_T {
            "three lines · no build · no save"
        } else {
            "the preview painted before the line was finished"
        },
        smoothstep(seg(sec, 0.5, 1.4)) * (1.0 - smoothstep(seg(sec, 22.6, 23.8))),
    ));
    let _ = (clamp01, INK_SOFT, W);
    compose(bg, nodes)
}
