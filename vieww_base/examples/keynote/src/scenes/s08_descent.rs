//! **S08 · THE DESCENT** — say is a doorway, not a wall.
//!
//! The objection a Rust audience has been holding since S06 gets answered
//! here: *what happens when the DSL runs out?* The answer is that it does
//! not run out — it opens. The command palette is invoked on camera, the
//! buffer transliterates line by line into the Rust it always was, the tab
//! becomes `main.rs`, and **the preview never restarts**: the counter holds
//! its value, the spring keeps its phase, and the session's elapsed chip
//! does not blink.
//!
//! That last part is the claim, and the witness counter is what makes it
//! checkable. If the program had restarted, the number would be zero.
//! Instead the fourth touch lands *during* the descent, on a program that is
//! now Rust, and the counter goes to **4** — continuous across the language
//! boundary.
//!
//! The walk is one cursor moving down the file, drawn from `buffer::RUST`
//! and `buffer::SAY` side by side: the same program at two altitudes, and
//! the transliteration is an index.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Rect, Size, Sketchbook};
use vieww_widget::prelude::*;

use crate::film::Ctx;
use crate::kit::{
    alpha, bump, caption, clamp01, dust, ease_out_cubic, grain, ground, horizon, mix, painter, seg,
    smoothstep, spring, vignette, xywh, Type, AMBER, CYAN, CYAN_SOFT, H, INK, INK_SOFT, MINT,
    MUTED, VIOLET, VIOLET_SOFT, W,
};
use crate::scenes::buffer;
use crate::studio::{
    self, compose, project_tree, status_line, witness_chip, App, FileKind, FocusOn, Layout, Palette,
    Shell,
};

/// The palette opens, the command is chosen, the walk runs.
const PAL_T0: f32 = 0.85;
const PAL_PICK: f32 = 2.55;
const PAL_T1: f32 = 3.15;
const WALK_T0: f32 = 3.35;
const WALK_T1: f32 = 12.40;
/// The fourth touch — on the Rust program, mid-descent.
const TAP4_T: f32 = 14.80;

/// The palette's rows. The first is the one that fires.
const ROWS: [(&str, &str); 4] = [
    ("say: transliterate buffer to Rust", "⏎"),
    ("say: show generated Rust beside", "⇧⏎"),
    ("preview: pin to device", "⌘P"),
    ("build: run all targets", "⌘B"),
];

pub fn frame(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let frame_i = ctx.frame;

    // The walk: a cursor moving down the file, one line at a time, with the
    // line under it mid-rewrite.
    let walk = ease_out_cubic(seg(sec, WALK_T0, WALK_T1));
    let n = buffer::SAY.len().max(buffer::RUST.len());
    let pos = walk * n as f32;
    let converted = pos.floor() as usize;
    let mid = (converted < n && walk < 1.0).then(|| (converted, pos.fract()));
    let code = buffer::descent_buffer(converted, mid);
    let is_rust = walk >= 1.0;

    // The palette.
    let pal_open = smoothstep(seg(sec, PAL_T0, PAL_T0 + 0.35)) * (1.0 - smoothstep(seg(sec, PAL_T1, PAL_T1 + 0.3)));
    let palette = (pal_open > 0.01).then(|| Palette {
        query: format!(
            "> {}",
            crate::kit::typed("say: transliterate", seg(sec, PAL_T0 + 0.2, PAL_PICK - 0.25))
        ),
        rows: ROWS.iter().map(|(a, b)| ((*a).to_string(), (*b).to_string())).collect(),
        selected: 0,
        open: pal_open,
    });

    let press = (TAP4_T..TAP4_T + 0.7).contains(&sec).then(|| seg(sec, TAP4_T, TAP4_T + 0.7));
    let app = App {
        value: ctx.spine.witness,
        alive: 1.0,
        press,
        accent: mix(VIOLET, AMBER, 0.28 * smoothstep(seg(sec, WALK_T0, WALK_T1))),
        // The spring's phase is a function of *absolute* film-time, not of
        // the scene's — which is exactly why it does not restart when the
        // language does.
        spring: 0.5 + 0.5 * spring((ctx.abs * 0.34).fract(), 9.0, 0.30).clamp(-1.0, 1.0),
        ..App::default()
    };

    let claim_a = smoothstep(seg(sec, WALK_T1 + 0.4, WALK_T1 + 1.3)) * (1.0 - smoothstep(seg(sec, 18.4, 19.4)));
    let third = claim_a;

    let l = Layout::new(64.0, 0.36, true);
    let sh = Shell {
        files: project_tree(if is_rust { "main.rs" } else { "counter.say" }),
        tabs: vec![
            ("counter.say".into(), FileKind::Say, !is_rust, true),
            ("main.rs".into(), FileKind::Rust, is_rust, is_rust),
        ],
        code,
        caret: Some(converted.min(n.saturating_sub(1))),
        caret_on: crate::kit::caret_on(ctx.abs, sec > WALK_T0 && sec < WALK_T1),
        changed: (0..converted).collect(),
        status: status_line(
            "desktop · native",
            "60 fps",
            &[if is_rust { ("rust · state held", MINT) } else { ("transliterating", AMBER) }],
        ),
        preview_label: "preview · desktop".into(),
        preview_chip: "live".into(),
        palette,
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
    let inner = Rect::new(card.left + 1.0, card.top + 1.0, card.right - 1.0, card.bottom - 1.0);
    let app_paint = App { ..app };

    let bg = painter(move |book: &mut Sketchbook, size: Size| {
        ground(book, size, sec, 0.85);
        horizon(book, size, mix(VIOLET, AMBER, 0.3 * walk), 0.7);
        dust(book, size, sec, 30, VIOLET_SOFT, 0.5);

        studio::back(book, &l, &shell_paint);

        book.rrect(card, 10.0, alpha(Color::rgb(14, 15, 21), 0.9));
        book.stroke_rrect(card, 10.0, alpha(Color::rgb(40, 44, 58), 0.9), 1.0);
        studio::app(book, inner, &app_paint);

        studio::front(book, &l, &shell_paint);

        // The descent cursor: a bright rule travelling down the gutter, and
        // the line under it lit. The whole scene's motion is this one bar.
        if sec > WALK_T0 && walk < 1.0 {
            let y = l.line_y(converted) - 5.0;
            if y > l.code.top && y < l.code.bottom {
                book.rect(
                    xywh(l.gutter.left, y, l.gutter.width() + l.code.width(), studio::CODE_LEAD),
                    alpha(AMBER, 0.07),
                );
                book.blended_layer(1.0, 14.0, BlendMode::Plus, None, |g| {
                    g.rect(
                        xywh(l.gutter.left, y + studio::CODE_LEAD - 2.0, l.gutter.width() + l.code.width(), 2.0),
                        Gradient::horizontal().with_dither().with_stops(&[
                            (0.0, alpha(AMBER, 0.55)),
                            (0.7, alpha(CYAN_SOFT, 0.35)),
                            (1.0, alpha(CYAN, 0.0)),
                        ]),
                    );
                });
            }
        }

        // The completion: one quiet sweep, and the buffer is Rust.
        let done = bump(seg(sec, WALK_T1, WALK_T1 + 1.2), 0.0, 1.0);
        if done > 0.01 {
            book.rrect(l.window, 16.0, alpha(AMBER, 0.035 * done));
        }

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
    nodes.push(witness_chip(body, ctx.spine.witness, 1.0));

    // The claim, and the proof, side by side. The left line is the sentence;
    // the right is the instrument that makes it checkable.
    let claim = claim_a;
    if claim > 0.004 {
        nodes.push(
            Type::new("start in say · go as deep as you want")
                .size(34.0)
                .light()
                .color(alpha(INK, 0.96 * claim))
                .center()
                .at(l.editor.left, l.editor.bottom - 104.0)
                .width(l.editor.width())
                .into(),
        );
        nodes.push(
            Type::new("the buffer changed language and the program did not restart")
                .mono()
                .size(14.0)
                .track(2.4)
                .color(alpha(CYAN, 0.88 * claim))
                .center()
                .at(l.editor.left, l.editor.bottom - 58.0)
                .width(l.editor.width())
                .into(),
        );
    }

    // The palette's own sentence, while it is open: the command is named,
    // so the mechanism is never mysterious.
    let pal_a = smoothstep(seg(sec, PAL_T0 + 0.5, PAL_T0 + 1.1)) * (1.0 - smoothstep(seg(sec, PAL_T1, PAL_T1 + 0.3)));
    if pal_a > 0.004 {
        nodes.push(
            Type::new("the studio's own command palette · no hidden step")
                .mono()
                .size(13.5)
                .track(2.2)
                .color(alpha(MUTED, 0.9 * pal_a))
                .center()
                .banner(H * 0.40)
                .into(),
        );
    }

    nodes.push(caption(
        if is_rust { "same buffer · main.rs · state held" } else { "say → rust, line by line" },
        smoothstep(seg(sec, 0.4, 1.2)) * (1.0 - smoothstep(seg(sec, 19.0, 20.0))),
    ));
    let _ = (clamp01, INK_SOFT, W);
    compose(bg, nodes)
}
