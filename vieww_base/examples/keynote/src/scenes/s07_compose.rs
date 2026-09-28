//! **S07 · LIVE COMPOSE** — per keystroke, including the broken ones.
//!
//! The buffer grows into the whole component while the preview keeps up with
//! it letter by letter. Then the film does the thing product films never do:
//! it makes a mistake on purpose, on camera, and leaves it on screen.
//!
//! The mistake is a name that is not in scope — `tap` where `taps` was
//! meant. Three things happen, and each is the product's own behaviour:
//! 1. The editor squiggles the line and prints the compiler's sentence.
//! 2. The **preview does not go blank and does not crash.** The failing
//!    subtree is replaced by an error boundary, drawn by the framework; the
//!    rest of the tree keeps painting, and the spring keeps running.
//! 3. The fix is one character, and the preview recovers without a restart —
//!    the counter still holds its value, which is the whole argument.
//!
//! Two touches land here: **2** when the component is composed, **3** after
//! the repair. The second one is the important one: the session survived an
//! error without restarting, and the counter is the proof.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Rect, Size, Sketchbook};
use vieww_widget::prelude::*;

use crate::film::Ctx;
use crate::kit::{
    alpha, bump, caption, clamp01, dust, ease_out_cubic, grain, ground, horizon, mix, painter, seg,
    smoothstep, spring, vignette, xywh, Type, CYAN, H, INK, INK_SOFT, MINT, MUTED, RED, VIOLET,
    VIOLET_SOFT, W,
};
use crate::scenes::buffer;
use crate::studio::{
    self, compose, project_tree, status_line, witness_chip, App, Diag, FileKind, FocusOn, Layout,
    Sev, Shell,
};

/// The beats, in scene-seconds.
const GROW_T0: f32 = 0.55;
const GROW_T1: f32 = 8.20;
/// The error is typed, noticed, and left standing.
const BREAK_T: f32 = 9.40;
/// The repair.
const FIX_T: f32 = 15.60;
/// The two touches.
const TAP2_T: f32 = 8.16;
const TAP3_T: f32 = 20.64;

pub fn frame(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let frame_i = ctx.frame;

    // The buffer grows line by line; the last line types character by
    // character, so the growth reads as a hand and not as a reveal.
    let grow = ease_out_cubic(seg(sec, GROW_T0, GROW_T1));
    let lines_f = grow * buffer::SAY.len() as f32;
    let lines = (lines_f.ceil() as usize).clamp(1, buffer::SAY.len());
    let last_chars = {
        let frac = lines_f - lines_f.floor();
        let n = buffer::SAY[lines - 1].chars().count();
        if lines_f >= buffer::SAY.len() as f32 { n } else { (n as f32 * frac).round() as usize }
    };

    let broken = sec >= BREAK_T && sec < FIX_T;
    let code = buffer::say_buffer(lines, Some(last_chars), broken);

    // The caret sits on the line being written; during the break it is on
    // the offending line, because that is where the person is looking.
    let caret_line = if broken || (sec >= FIX_T && sec < FIX_T + 1.6) {
        buffer::BROKEN_LINE
    } else {
        lines - 1
    };
    let typing = sec < GROW_T1 || (sec >= BREAK_T - 0.4 && sec < BREAK_T + 0.3) || (sec >= FIX_T - 0.3 && sec < FIX_T + 0.4);

    let diag = broken.then(|| Diag {
        line: buffer::BROKEN_LINE,
        message: buffer::DIAGNOSTIC.to_string(),
        severity: Sev::Error,
        reveal: ease_out_cubic(seg(sec, BREAK_T + 0.15, BREAK_T + 0.9)),
    });

    // The app. Broken while the name is missing; the accent shifts as the
    // theme line is written, which is the "live" in live compose.
    let accent = mix(VIOLET, CYAN, 0.30 * smoothstep(seg(sec, 5.0, 7.2)));
    let press = if (TAP2_T..TAP2_T + 0.7).contains(&sec) {
        Some(seg(sec, TAP2_T, TAP2_T + 0.7))
    } else if (TAP3_T..TAP3_T + 0.7).contains(&sec) {
        Some(seg(sec, TAP3_T, TAP3_T + 0.7))
    } else {
        None
    };
    let app = App {
        value: ctx.spine.witness,
        alive: 1.0,
        press,
        broken,
        accent,
        spring: 0.5 + 0.5 * spring((sec * 0.34).fract(), 9.0, 0.30).clamp(-1.0, 1.0),
        ..App::default()
    };

    // The three statements share one scrim; `third` is the maximum of their
    // envelopes, so the lower third is up exactly while a sentence is.
    let third = [
        (2.2f32, 8.4f32),
        (BREAK_T + 0.7, FIX_T - 0.4),
        (FIX_T + 0.6, 22.6),
    ]
    .into_iter()
    .map(|(from, to)| smoothstep(seg(sec, from, from + 0.7)) * (1.0 - smoothstep(seg(sec, to - 0.6, to))))
    .fold(0.0f32, f32::max);

    let l = Layout::new(64.0, 0.36, true);
    let sh = Shell {
        files: project_tree("counter.say"),
        tabs: vec![
            ("main.say".into(), FileKind::Say, false, false),
            ("counter.say".into(), FileKind::Say, true, true),
        ],
        code,
        caret: Some(caret_line),
        caret_on: crate::kit::caret_on(ctx.abs, typing),
        changed: (0..lines).collect(),
        diag,
        status: status_line(
            "desktop · native",
            "60 fps",
            &[if broken {
                ("1 error · preview held", RED)
            } else {
                ("0 errors", MINT)
            }],
        ),
        preview_label: "preview · desktop".into(),
        preview_chip: if broken { "boundary".into() } else { "live".into() },
        witness: Some(ctx.spine.witness),
        elapsed: Some(crate::kit::clock_mmss(ctx.spine.elapsed)),
        focus: Some(if press.is_some() { FocusOn::Preview } else { FocusOn::Code }),
        panel: crate::studio::Panel {
            lines: crate::studio::output_lines(),
            ..crate::studio::Panel::default()
        },
        ..Shell::default()
    };

    let shell_paint = sh.clone();
    let body = l.preview_body;
    let card = Rect::new(body.left + 44.0, body.top + 40.0, body.right - 44.0, body.bottom - 52.0);
    let app_paint = App { ..app };
    let inner = Rect::new(card.left + 1.0, card.top + 1.0, card.right - 1.0, card.bottom - 1.0);

    let bg = painter(move |book: &mut Sketchbook, size: Size| {
        ground(book, size, sec, 0.85);
        horizon(book, size, if broken { RED } else { VIOLET }, 0.7);
        dust(book, size, sec, 30, VIOLET_SOFT, 0.5);

        studio::back(book, &l, &shell_paint);

        book.rrect(card, 10.0, alpha(Color::rgb(14, 15, 21), 0.9));
        book.stroke_rrect(
            card,
            10.0,
            alpha(if broken { RED } else { Color::rgb(40, 44, 58) }, if broken { 0.55 } else { 0.9 }),
            1.0,
        );
        studio::app(book, inner, &app_paint);

        // Every keystroke lands in the preview: a one-frame pulse on the
        // card's edge, timed off the *character count*, so the preview
        // flickers exactly as fast as the typing.
        if sec < GROW_T1 {
            let k = (lines_f * 24.0).fract();
            if k < 0.35 {
                book.rrect(card, 10.0, alpha(accent, 0.05 * (1.0 - k / 0.35)));
            }
        }

        // The break: the boundary arrives with one red wash that recedes
        // immediately. The film does not linger on the failure; it lingers
        // on the fact that everything else kept running.
        let brk = bump(seg(sec, BREAK_T, BREAK_T + 1.0), 0.0, 1.0);
        if brk > 0.01 {
            book.rrect(l.window, 16.0, alpha(RED, 0.05 * brk));
        }
        // The repair: green, once, and the boundary is gone.
        let fix = bump(seg(sec, FIX_T, FIX_T + 1.1), 0.0, 1.0);
        if fix > 0.01 {
            book.rrect(l.window, 16.0, alpha(MINT, 0.05 * fix));
            book.blended_layer(1.0, 22.0, BlendMode::Plus, None, |g| {
                g.rect(
                    xywh(card.left, card.top, card.width() * ease_out_cubic(seg(sec, FIX_T, FIX_T + 0.6)), card.height()),
                    Gradient::horizontal().with_dither().with_stops(&[
                        (0.0, alpha(MINT, 0.0)),
                        (1.0, alpha(MINT, 0.18 * fix)),
                    ]),
                );
            });
        }

        studio::front(book, &l, &shell_paint);

        if let Some(p) = press {
            let cx = inner.left + inner.width() * 0.5;
            let cy = inner.top + inner.height() * 0.42;
            let dial_r = (inner.width().min(inner.height()) * 0.20).max(24.0);
            studio::touch(book, Offset::new(cx, cy + dial_r + 98.0), p, VIOLET_SOFT);
        }

        crate::kit::lower_third(book, l.editor, third);
        grain(book, size, frame_i, 0.012, 300);
        vignette(book, size, 0.85);
    });

    let mut nodes: Vec<WidgetNode> = studio::text(&l, &sh);
    nodes.extend(studio::app_text(inner, &app));

    if ctx.spine.witness >= 1 {
        nodes.push(witness_chip(body, ctx.spine.witness, 1.0));
    }

    // The three sentences this scene is for, each on its own beat, each
    // stating a mechanism rather than a benefit.
    let beats: [(f32, f32, &str, Color); 3] = [
        (2.2, 8.4, "every keystroke lands — no save, no reload", INK),
        (BREAK_T + 0.7, FIX_T - 0.4, "the error renders inside the product · the rest of the tree kept painting", RED),
        (FIX_T + 0.6, 22.6, "one character · the state survived the failure", MINT),
    ];
    for (from, to, text, c) in beats {
        let a = smoothstep(seg(sec, from, from + 0.7)) * (1.0 - smoothstep(seg(sec, to - 0.6, to)));
        if a > 0.004 {
            nodes.push(
                Type::new(text)
                    .size(31.0)
                    .light()
                    .color(alpha(c, 0.96 * a))
                    .center()
                    .at(l.editor.left, l.editor.bottom - 104.0)
                    .width(l.editor.width())
                    .into(),
            );
        }
    }

    nodes.push(caption(
        if broken { "a boundary, not a crash" } else { "live compose · say" },
        smoothstep(seg(sec, 0.6, 1.4)) * (1.0 - smoothstep(seg(sec, 23.0, 24.0))),
    ));
    let _ = (clamp01, INK_SOFT, MUTED, W);
    compose(bg, nodes)
}
