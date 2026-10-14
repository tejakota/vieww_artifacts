//! **S16 · ONE CLICK** — the studio ships the thing it previewed.
//!
//! The last product beat, and the one that closes the loop Act I opened. The
//! film began with a build that would not finish; it ends with the studio's
//! Export view, three targets, and a staged build that completes on screen.
//!
//! Three progress rings fill at once — desktop, web, android — each with its
//! own artifact named underneath, each finishing into a green check. The
//! timer beside them runs, and it is the same kind of timer S01 spent fifteen
//! seconds on. The difference between the two numbers is the film's argument
//! stated as arithmetic.
//!
//! iOS is in the list and it is greyed, with one word beside it: *disputed*.
//! The film refuses to claim it even in the scene where claiming it would be
//! free.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook};
use vieww_widget::prelude::*;

use crate::film::Ctx;
use crate::kit::{
    alpha, bump, caption, clamp01, dust, ease_out_cubic, grain, ground, horizon, mix, painter, seg,
    smoothstep, spring, vignette, xywh, Type, AMBER, CYAN, CYAN_SOFT, FAINT, H, INK, INK_SOFT,
    MINT, MUTED, VIOLET, VIOLET_SOFT, W,
};
use crate::studio::compose;

/// The targets, with the artifact each produces and the beat it finishes on.
const TARGETS: &[(&str, &str, f32, bool)] = &[
    ("desktop", "counter.app · 8.4 MB", 7.10, true),
    ("web", "counter.wasm + index.html", 8.60, true),
    ("android", "counter.apk · signed", 10.90, true),
    ("ios", "disputed — excluded from this build", 0.0, false),
];

const START_T: f32 = 1.40;
const DONE_T: f32 = 11.40;

pub fn frame(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let frame_i = ctx.frame;

    let press = (0.70..1.40).contains(&sec).then(|| seg(sec, 0.70, 1.40));
    let elapsed = (sec - START_T).max(0.0).min(DONE_T - START_T);
    let all_done = sec >= DONE_T;
    let done_pop = spring(seg(sec, DONE_T, DONE_T + 1.2), 12.0, 0.4).clamp(0.0, 1.0);
    let out = smoothstep(seg(ctx.t, 0.94, 1.0));

    let cx = W * 0.5;
    let base_y = H * 0.46;

    let bg = painter(move |book: &mut Sketchbook, size: Size| {
        ground(book, size, sec, 1.0);
        horizon(book, size, VIOLET, 0.6);
        dust(book, size, sec, 34, VIOLET_SOFT, 0.5);

        // The button that started it — the studio's Export action, pressed
        // once, on camera.
        let btn = xywh(cx - 150.0, H * 0.175, 300.0, 56.0);
        let pressed = press.is_some_and(|p| p < 0.4);
        let b = Rect::new(btn.left, btn.top + if pressed { 2.0 } else { 0.0 }, btn.right, btn.bottom + if pressed { 2.0 } else { 0.0 });
        book.shadow(b, 28.0, crate::studio::elevation(2));
        book.rrect(
            b,
            b.height() * 0.5,
            Gradient::vertical().with_dither().with_stops(&[
                (0.0, alpha(VIOLET_SOFT, 1.0)),
                (1.0, alpha(crate::studio::ACCENT, 1.0)),
            ]),
        );
        if let Some(p) = press {
            let rr = b.height() * 0.5 + b.width() * 0.7 * ease_out_cubic(p);
            book.layer(1.0 - p, 0.0, Some(Path::rounded_rect(b, b.height() * 0.5)), |g| {
                g.circle(Offset::new(b.left + b.width() * 0.5, b.top + b.height() * 0.5), rr, alpha(Color::WHITE, 0.25));
            });
        }

        // The three rings.
        for (i, (_name, _art, at, real)) in TARGETS.iter().enumerate() {
            if !real {
                continue;
            }
            let x = cx - 420.0 + i as f32 * 420.0;
            let r = 86.0;
            let p = clamp01((sec - START_T) / (at - START_T).max(0.1));
            book.ring(Offset::new(x, base_y), r, 9.0, alpha(Color::rgb(0x26, 0x24, 0x22), 1.0));
            if p > 0.001 {
                crate::kit::arc_sweep(
                    book,
                    Offset::new(x, base_y),
                    r,
                    9.0,
                    -std::f32::consts::FRAC_PI_2,
                    p * std::f32::consts::TAU,
                    Gradient::sweep(Offset::new(0.5, 0.5), 0.0, std::f32::consts::TAU)
                        .with_dither()
                        .with_stops(&[
                            (0.0, alpha(VIOLET, 1.0)),
                            (0.7, alpha(CYAN, 1.0)),
                            (1.0, alpha(MINT, 1.0)),
                        ]),
                );
            }
            // The finish: one bloom, one check.
            if sec >= *at {
                let fin = bump(seg(sec, *at, at + 1.0), 0.0, 1.0);
                book.blended_layer(1.0, 34.0, BlendMode::Plus, None, |g| {
                    g.circle(Offset::new(x, base_y), r * 1.3, alpha(MINT, 0.20 + 0.25 * fin));
                });
                let k = spring(seg(sec, *at, at + 0.7), 14.0, 0.42).clamp(0.0, 1.0);
                let mut check = Path::new();
                check.move_to(Offset::new(x - 22.0 * k, base_y + 2.0));
                check.line_to(Offset::new(x - 6.0 * k, base_y + 18.0 * k));
                check.line_to(Offset::new(x + 24.0 * k, base_y - 18.0 * k));
                book.stroke(check, alpha(MINT, 0.95), 4.5);
            }
        }

        // The excluded target: a ring that is not drawn, only outlined, with
        // a diagonal through it. The film's refusal, in one shape.
        let x = cx + 420.0 + 420.0 * 0.0;
        let _ = x;
        let ex = cx + 630.0;
        book.ring(Offset::new(ex, base_y), 48.0, 2.0, alpha(FAINT, 0.55));
        book.line(
            Offset::new(ex - 34.0, base_y + 34.0),
            Offset::new(ex + 34.0, base_y - 34.0),
            alpha(FAINT, 0.55),
            2.0,
        );

        grain(book, size, frame_i, 0.012, 300);
        vignette(book, size, 0.95);
        if out > 0.004 {
            book.rect(Rect::new(0.0, 0.0, size.width, size.height), alpha(Color::BLACK, out));
        }
    });

    let mut nodes: Vec<WidgetNode> = vec![
        Type::new("one click")
            .size(46.0)
            .light()
            .track(0.6)
            .color(alpha(INK, 0.97 * (1.0 - out)))
            .center()
            .banner(H * 0.075)
            .into(),
        Type::new("export")
            .size(18.0)
            .medium()
            .color(alpha(Color::WHITE, 0.97 * (1.0 - out)))
            .center()
            .banner(H * 0.175 + 16.0)
            .into(),
    ];

    for (i, (name, art, at, real)) in TARGETS.iter().enumerate() {
        let (x, r) = if *real {
            (cx - 420.0 + i as f32 * 420.0, 86.0)
        } else {
            (cx + 630.0, 48.0)
        };
        let a = if *real { 1.0 } else { 0.55 } * (1.0 - out);
        nodes.push(
            Type::new(*name)
                .size(if *real { 24.0 } else { 18.0 })
                .medium()
                .color(alpha(if *real { INK } else { FAINT }, 0.96 * a))
                .center()
                .at(x - 200.0, base_y + r + 26.0)
                .width(400.0)
                .into(),
        );
        nodes.push(
            Type::new(*art)
                .mono()
                .size(12.0)
                .track(1.2)
                .color(alpha(if *real { MUTED } else { FAINT }, 0.9 * a))
                .center()
                .at(x - 230.0, base_y + r + 58.0)
                .width(460.0)
                .into(),
        );
        if *real {
            let done = sec >= *at;
            nodes.push(
                Type::new(if done { "done".to_string() } else { format!("{:.1}s", (sec - START_T).max(0.0)) })
                    .mono()
                    .size(14.0)
                    .track(1.6)
                    .color(alpha(if done { MINT } else { CYAN_SOFT }, 0.95 * a))
                    .center()
                    .at(x - 120.0, base_y - 8.0)
                    .width(240.0)
                    .into(),
            );
        }
    }

    // The timer, and the comparison the whole film has been building to.
    let timer_a = smoothstep(seg(sec, START_T, START_T + 0.5)) * (1.0 - out);
    nodes.push(
        Type::new(format!("{elapsed:.1} s"))
            .size(64.0)
            .medium()
            .color(alpha(if all_done { MINT } else { INK }, 0.97 * timer_a))
            .center()
            .banner(H * 0.745)
            .into(),
    );
    nodes.push(
        Type::new("three targets, from the buffer the session was editing")
            .mono()
            .size(14.0)
            .track(2.2)
            .color(alpha(MUTED, 0.9 * timer_a))
            .center()
            .banner(H * 0.745 + 78.0)
            .into(),
    );

    if done_pop > 0.02 {
        nodes.push(
            Type::new("the build finished while you were watching it")
                .size(30.0)
                .light()
                .color(alpha(INK, 0.96 * done_pop * (1.0 - out)))
                .center()
                .banner(H * 0.862)
                .into(),
        );
    }

    nodes.push(caption(
        "ios stays out · the film does not claim what the docs dispute",
        smoothstep(seg(sec, 3.4, 4.4)) * (1.0 - smoothstep(seg(ctx.t, 0.90, 0.98))),
    ));
    let _ = (AMBER, INK_SOFT, mix, clamp01, Size::new(0.0, 0.0));
    compose(bg, nodes)
}
