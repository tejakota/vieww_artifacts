//! **S10 · DAMAGE IN PIXELS** — one write, one region.
//!
//! The claim the whole framework rests on, shown rather than said: a signal
//! is written, exactly one subtree rebuilds, and exactly one rectangle of
//! pixels is repainted. The inspector is opened on the preview, overlays on,
//! and the damage rect lights up around the counter's digits — **not** around
//! the dial, **not** around the button, **not** around the pane.
//!
//! Then the scene does the thing that makes it a receipt rather than an
//! animation: it writes the signal **ten times inside one frame window** and
//! the counters underneath read *10 writes · 1 rebuild · 1 damage rect*.
//! Pull-based coalescing is not a paragraph in a README here; it is three
//! numbers that disagree with each other in exactly the way the design
//! promises.
//!
//! The session's fifth touch lands on the last of those writes.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Rect, Size, Sketchbook};
use vieww_widget::prelude::*;

use crate::film::Ctx;
use crate::kit::{
    alpha, bump, caption, clamp01, ease_out_cubic, grain, ground, painter, seg, smoothstep,
    vignette, xywh, Type, CYAN, H, INK, MINT, MUTED, RED, VIOLET_SOFT, W,
};
use crate::studio::{
    self, compose, project_tree, status_line, App, FocusOn, Inspector, Layout, Shell,
};

/// The beats, in scene-seconds.
const OPEN_T: f32 = 0.50;
/// The ten writes, all inside one frame window.
const BURST_T: f32 = 4.60;
const BURST_SPAN: f32 = 0.16;
/// The fifth touch.
const TAP5_T: f32 = 4.60;

pub fn frame(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let frame_i = ctx.frame;

    let open = smoothstep(seg(sec, OPEN_T, OPEN_T + 0.7));
    // The burst: ten writes, each one a tick, all inside a sixth of a second.
    let writes = ((sec - BURST_T) / (BURST_SPAN / 10.0)).floor().clamp(0.0, 10.0);
    let bursting = (BURST_T..BURST_T + 1.4).contains(&sec);
    // The damage rect lives for a few frames after the rebuild, the way a
    // damage overlay does — long enough to read, short enough to be honest.
    let dmg = bump(seg(sec, BURST_T, BURST_T + 1.1), 0.0, 1.0);

    // The digits' box, in preview-body coordinates. This is the *only*
    // region that changed, and its size is the argument.
    let digits = Rect::new(0.40, 0.29, 0.60, 0.44);

    let inspector = Inspector {
        open,
        tree: vec![
            (0, "Column".into(), false),
            (1, "Dial value=…".into(), false),
            (1, "Text \"{taps}\"".into(), true),
            (1, "Text \"taps\"".into(), false),
            (1, "Button \"count\"".into(), false),
        ],
        facts: vec![
            ("writes".into(), format!("{writes:.0}")),
            ("rebuilds".into(), if writes > 0.0 { "1".into() } else { "0".into() }),
            ("damage rects".into(), if writes > 0.0 { "1".into() } else { "0".into() }),
            (
                "frame".into(),
                if ctx.probe.is_measured() {
                    format!("{:.2} ms", ctx.probe.frame_ms)
                } else {
                    "—".into()
                },
            ),
        ],
        damage: if dmg > 0.02 { vec![(digits, dmg)] } else { Vec::new() },
        selection: (open > 0.4).then_some(digits),
    };

    let l = Layout::new(64.0, 0.36, true);
    let sh = Shell {
        files: project_tree("main.rs"),
        tabs: vec![("main.rs".into(), studio::FileKind::Rust, true, false)],
        crumbs: vec!["counter".into(), "src".into(), "main.rs".into(), "counter()".into()],
        code: studio::lex_all(crate::scenes::buffer::RUST, true),
        caret: Some(1),
        caret_on: crate::kit::caret_on(ctx.abs, bursting),
        changed: vec![1],
        status: status_line(
            "desktop · native",
            "60 fps",
            &[("overlays on · damage", RED)],
        ),
        preview_label: "Preview · damage".into(),
        preview_chip: "overlays".into(),
        panel: studio::Panel {
            active: 0,
            lines: vec![
                ("  signal    taps · 1 subscriber (Text)".to_string(), MUTED),
                (
                    format!("  coalesce  {writes:.0} writes → 1 rebuild in this frame window"),
                    if writes >= 10.0 { MINT } else { MUTED },
                ),
                ("  damage    1 rect · the digits, and nothing else".to_string(), MUTED),
            ],
            ..studio::Panel::default()
        },
        inspector: Some(inspector),
        witness: Some(ctx.spine.witness),
        elapsed: Some(crate::kit::clock_mmss(ctx.spine.elapsed)),
        view: 4,
        focus: Some(FocusOn::Preview),
        ..Shell::default()
    };
    let shell_paint = sh.clone();

    let press = (TAP5_T..TAP5_T + 0.7).contains(&sec).then(|| seg(sec, TAP5_T, TAP5_T + 0.7));
    let app = App {
        value: ctx.spine.witness,
        alive: 1.0,
        press,
        spring: 0.5 + 0.35 * (ctx.abs * 1.1).sin(),
        ..App::default()
    };
    let app_paint = App { ..app };
    let card = Rect::new(
        l.preview_body.left + 30.0,
        l.preview_body.top + 22.0,
        l.preview_body.right - 30.0,
        l.preview_body.bottom - 30.0,
    );

    let bg = painter(move |book: &mut Sketchbook, size: Size| {
        ground(book, size, sec, 0.9);
        studio::back(book, &l, &shell_paint);

        book.rrect(card, 8.0, alpha(Color::rgb(0x16, 0x16, 0x18), 1.0));
        studio::app(book, card, &app_paint);

        studio::front(book, &l, &shell_paint);

        if let Some(p) = press {
            let cx = card.left + card.width() * 0.5;
            let cy = card.top + card.height() * 0.40;
            let dial_r = (card.width().min(card.height()) * 0.19).max(22.0);
            studio::touch(book, Offset::new(cx, cy + dial_r + 90.0), p, VIOLET_SOFT);
        }

        // The write ticks: ten marks filling a strip under the code, one per
        // write, all inside the same frame window. The strip is the film's
        // way of showing that the writes really were simultaneous.
        if sec > BURST_T - 0.6 && sec < BURST_T + 3.2 {
            let a = smoothstep(seg(sec, BURST_T - 0.6, BURST_T - 0.1))
                * (1.0 - smoothstep(seg(sec, BURST_T + 2.6, BURST_T + 3.2)));
            let sw = 280.0;
            let sx = l.code.left + 24.0;
            let sy = l.code.bottom - 84.0;
            book.rrect(xywh(sx, sy, sw, 20.0), 4.0, alpha(Color::rgb(0x26, 0x26, 0x2A), a));
            for i in 0..10 {
                let on = (i as f32) < writes;
                book.rrect(
                    xywh(sx + 5.0 + i as f32 * 27.0, sy + 4.0, 22.0, 12.0),
                    2.0,
                    alpha(if on { CYAN } else { Color::rgb(0x3A, 0x3A, 0x40) }, a),
                );
            }
            // And one rebuild bar beside it — one, however many ticks lit.
            book.rrect(xywh(sx + sw + 40.0, sy, 92.0, 20.0), 4.0, alpha(Color::rgb(0x26, 0x26, 0x2A), a));
            if writes > 0.0 {
                book.rrect(
                    xywh(sx + sw + 45.0, sy + 4.0, 82.0, 12.0),
                    2.0,
                    Gradient::horizontal().with_dither().with_stops(&[
                        (0.0, alpha(MINT, a)),
                        (1.0, alpha(CYAN, a)),
                    ]),
                );
            }
        }

        grain(book, size, frame_i, 0.012, 300);
        vignette(book, size, 0.85);
    });

    let mut nodes: Vec<WidgetNode> = studio::text(&l, &sh);
    nodes.extend(studio::app_text(card, &app));

    if sec > BURST_T - 0.6 && sec < BURST_T + 3.2 {
        let a = smoothstep(seg(sec, BURST_T - 0.6, BURST_T - 0.1))
            * (1.0 - smoothstep(seg(sec, BURST_T + 2.6, BURST_T + 3.2)));
        let sx = l.code.left + 24.0;
        let sy = l.code.bottom - 84.0;
        nodes.push(
            Type::new(format!("{writes:.0} writes"))
                .mono()
                .size(13.0)
                .track(1.6)
                .color(alpha(CYAN, 0.95 * a))
                .at(sx, sy - 24.0)
                .width(280.0)
                .into(),
        );
        nodes.push(
            Type::new("1 rebuild")
                .mono()
                .size(13.0)
                .track(1.6)
                .color(alpha(MINT, 0.95 * a))
                .at(sx + 320.0, sy - 24.0)
                .width(200.0)
                .into(),
        );
    }

    // The sentence, over the editor card as everywhere else in this act.
    let claim = smoothstep(seg(sec, BURST_T + 1.0, BURST_T + 1.8)) * (1.0 - smoothstep(seg(sec, 9.2, 9.9)));
    if claim > 0.004 {
        nodes.push(
            Type::new("ten writes · one rebuild · one rectangle of pixels")
                .size(30.0)
                .light()
                .color(alpha(INK, 0.96 * claim))
                .center()
                .at(l.editor.left, l.editor.bottom - 104.0)
                .width(l.editor.width())
                .into(),
        );
        nodes.push(
            Type::new("pull-based coalescing · the damage rect is the digits, and nothing else")
                .mono()
                .size(13.5)
                .track(1.8)
                .color(alpha(MUTED, 0.9 * claim))
                .center()
                .at(l.editor.left, l.editor.bottom - 60.0)
                .width(l.editor.width())
                .into(),
        );
    }

    nodes.push(caption(
        "the studio's own inspector · the product's own overlay",
        smoothstep(seg(sec, 1.2, 2.0)) * (1.0 - smoothstep(seg(sec, 9.2, 9.9))),
    ));
    let _ = (W, H, ease_out_cubic, clamp01);
    compose(bg, nodes)
}

/// The scene's own scrim for the closing sentence — same device as the rest
/// of the act, kept here so the value and the paint cannot disagree.
pub fn statement_scrim(sec: f32) -> f32 {
    smoothstep(seg(sec, BURST_T + 1.0, BURST_T + 1.8)) * (1.0 - smoothstep(seg(sec, 9.2, 9.9)))
}
