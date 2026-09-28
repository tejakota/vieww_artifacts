//! **S17 · THE END CARD** — the film audits itself.
//!
//! The wordmark returns, settled, with the lineage line beneath it and the
//! install commands typed on in mono. Underneath, the thing that makes this
//! an end card rather than a title: **the film's own manifest**, drawn as
//! bars, read from `manifest.txt`.
//!
//! Those numbers are the census of *this render*: how many shapes the film
//! asked the rasterizer to draw, how many glyph runs it shaped, how many
//! layers it composited, how many of those were blurred. They are produced by
//! pass 1 and displayed by pass 2, which is the two-pass loop the harness
//! exists for — the endcard cannot know the film's totals until the film has
//! been walked, and the film is not finished until the endcard is drawn.
//!
//! If the census has not run, this card says so instead of printing a
//! number. That is rule 5 of the ledger arriving at the film's last frame
//! with its jacket still on.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Rect, Size, Sketchbook};
use vieww_widget::prelude::*;
use vieww_widget::{Filtered, Opacity};

use crate::film::Ctx;
use crate::kit::{
    alpha, bump, caption, clamp01, dust, ease_out_cubic, ease_out_expo, grain, ground, horizon, mix,
    painter, seg, smoothstep, spring, sting, thousands, tint, typed, vignette, xywh, Type, CYAN,
    CYAN_SOFT, FAINT, H, INK, INK_SOFT, MINT, MUTED, VIOLET, VIOLET_SOFT, W,
};
use crate::studio::compose;

const MARK: &str = "viewwstudio";
const UNDER: &str = "built on vieww";
/// The install lines, single-sourced from the CLI's own documented commands.
const INSTALL_SH: &str = "curl -fsSL https://vieww.dev | sh";
const INSTALL_PS: &str = "irm https://vieww.dev/install.ps1 | iex";
const REPO: &str = "github.com/tejakota/vieww_artifacts";

const MARK_Y: f32 = H * 0.135;
const MARK_SIZE: f32 = 96.0;

const TYPE_T: f32 = 2.20;
const BARS_T: f32 = 5.40;
const STING_T: f32 = 15.60;

pub fn frame(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let frame_i = ctx.frame;

    let mark_a = ease_out_cubic(seg(sec, 0.25, 1.30));
    let under_a = ease_out_cubic(seg(sec, 1.10, 2.00));
    let rule = ease_out_expo(seg(sec, 0.95, 2.10));
    let sh_a = ease_out_cubic(seg(sec, TYPE_T, TYPE_T + 0.3));
    let ps_a = ease_out_cubic(seg(sec, TYPE_T + 1.5, TYPE_T + 1.8));
    let bars = smoothstep(seg(sec, BARS_T, BARS_T + 0.8));
    let st = sting(sec - STING_T, 1.6);

    let p = ctx.probe;
    let measured = p.is_measured();

    // The bars. Each is a real total from the census, scaled against the
    // largest so the shape of the film's cost is visible at a glance.
    let rows: [(&str, u64); 6] = [
        ("shapes", p.shapes),
        ("glyphs", p.glyphs),
        ("strokes", p.strokes),
        ("glyph runs", p.glyph_runs),
        ("layers", p.layers),
        ("blurred layers", p.filtered),
    ];
    let peak = rows.iter().map(|(_, v)| *v).max().unwrap_or(1).max(1);

    let bg = painter(move |book: &mut Sketchbook, size: Size| {
        ground(book, size, sec, 1.0);
        horizon(book, size, VIOLET, 0.85 + 0.3 * st);
        dust(book, size, sec, 40, VIOLET_SOFT, 0.55);

        // The lineage rule under the mark — the same gradient the film has
        // used since S03, arriving for the last time.
        if rule > 0.01 {
            let rw = 620.0 * rule;
            let rx = size.width * 0.5 - rw * 0.5;
            let ry = MARK_Y + MARK_SIZE * 1.16;
            book.rrect(
                xywh(rx, ry, rw, 3.0),
                1.5,
                Gradient::horizontal().with_dither().with_stops(&[
                    (0.0, alpha(VIOLET, 0.0)),
                    (0.28, alpha(VIOLET, 0.95)),
                    (0.72, alpha(mix(VIOLET, CYAN, 0.6), 0.9)),
                    (1.0, alpha(CYAN, 0.0)),
                ]),
            );
            if st > 0.01 {
                book.blended_layer(1.0, 24.0, BlendMode::Plus, None, |g| {
                    g.rrect(xywh(rx, ry - 3.0, rw, 9.0), 4.5, alpha(VIOLET_SOFT, 0.35 * st));
                });
            }
        }

        // The install lines' plates.
        for (i, a) in [sh_a, ps_a].into_iter().enumerate() {
            if a <= 0.02 {
                continue;
            }
            let w = 560.0;
            let x = size.width * 0.5 - w - 18.0 + i as f32 * (w + 36.0);
            let y = MARK_Y + MARK_SIZE * 1.58;
            let r = xywh(x, y, w, 56.0);
            book.rrect(r, 8.0, alpha(Color::rgb(0x18, 0x15, 0x13), 0.94 * a));
            book.stroke_rrect(r, 8.0, alpha(crate::studio::LINE, a), 1.0);
            book.rrect(xywh(r.left, r.top, 3.0, r.height()), 1.5, alpha(if i == 0 { VIOLET } else { CYAN }, 0.9 * a));
        }

        // The manifest's bars.
        if bars > 0.01 {
            let bx = size.width * 0.5 - 430.0;
            let bw = 860.0;
            for (i, (_label, v)) in rows.into_iter().enumerate() {
                let y = H * 0.545 + i as f32 * 52.0;
                let grow = ease_out_cubic(seg(sec, BARS_T + 0.25 + i as f32 * 0.14, BARS_T + 1.35 + i as f32 * 0.14));
                book.rrect(xywh(bx, y, bw, 12.0), 6.0, alpha(Color::rgb(0x22, 0x20, 0x1E), bars));
                if measured {
                    let frac = (v as f64 / peak as f64) as f32;
                    book.rrect(
                        xywh(bx, y, bw * frac * grow, 12.0),
                        6.0,
                        Gradient::horizontal().with_dither().with_stops(&[
                            (0.0, alpha(VIOLET, bars)),
                            (1.0, alpha(CYAN_SOFT, bars)),
                        ]),
                    );
                }
            }
        }

        // The sting: one lift of the whole frame, once, at the end.
        if st > 0.01 {
            book.rect(Rect::new(0.0, 0.0, size.width, size.height), alpha(tint(VIOLET, 0.5), 0.045 * st));
        }

        grain(book, size, frame_i, 0.012, 320);
        vignette(book, size, 1.0);
    });

    let mark_node = |a: f32, c: Color| -> WidgetNode {
        Type::new(MARK)
            .size(MARK_SIZE)
            .medium()
            .track(1.2)
            .leading(1.0)
            .color(alpha(c, a))
            .center()
            .banner(0.0)
            .width(W)
            .into()
    };

    let mut nodes: Vec<WidgetNode> = Vec::new();

    // The mark's bloom, then the mark.
    if mark_a > 0.02 {
        nodes.push(
            Stack::new()
                .push(
                    Positioned::new()
                        .left(0.0)
                        .top(MARK_Y)
                        .width(W)
                        .height(MARK_SIZE * 1.4)
                        .child(
                            Opacity::new(0.34 * mark_a + 0.25 * st)
                                .blend(BlendMode::Plus)
                                .child(Filtered::blur(14.0).child(mark_node(1.0, VIOLET_SOFT))),
                        ),
                )
                .into(),
        );
    }
    nodes.push(
        Stack::new()
            .push(
                Positioned::new()
                    .left(0.0)
                    .top(MARK_Y)
                    .width(W)
                    .height(MARK_SIZE * 1.4)
                    .child(mark_node(0.99 * mark_a, mix(INK, tint(VIOLET_SOFT, 0.3), 0.08 + 0.2 * st))),
            )
            .into(),
    );
    nodes.push(
        Type::new(UNDER)
            .mono()
            .size(19.0)
            .track(6.5)
            .color(alpha(CYAN, 0.92 * under_a))
            .center()
            .banner(MARK_Y + MARK_SIZE * 1.30)
            .into(),
    );

    // The install lines, typed on.
    for (i, (line, a, from)) in [
        (INSTALL_SH, sh_a, TYPE_T),
        (INSTALL_PS, ps_a, TYPE_T + 1.5),
    ]
    .into_iter()
    .enumerate()
    {
        if a <= 0.02 {
            continue;
        }
        let w = 560.0;
        let x = W * 0.5 - w - 18.0 + i as f32 * (w + 36.0);
        let y = MARK_Y + MARK_SIZE * 1.58;
        let prog = seg(sec, from + 0.15, from + 1.15);
        let shown = typed(line, prog);
        let caret = if prog < 1.0 && crate::kit::caret_on(ctx.abs, true) { "▌" } else { "" };
        nodes.push(
            Type::new(format!("{shown}{caret}"))
                .mono()
                .size(17.0)
                .track(0.4)
                .color(alpha(INK, 0.97 * a))
                .at(x + 22.0, y + 18.0)
                .width(w - 36.0)
                .into(),
        );
    }
    nodes.push(
        Type::new("macOS · Linux                                                   Windows")
            .mono()
            .size(11.0)
            .track(1.6)
            .color(alpha(FAINT, 0.85 * ps_a))
            .center()
            .banner(MARK_Y + MARK_SIZE * 1.58 + 66.0)
            .into(),
    );

    // The manifest's labels and figures.
    if bars > 0.01 {
        let bx = W * 0.5 - 430.0;
        nodes.push(
            Type::new(if measured {
                format!("this film, counted by the renderer that drew it · {} frames", thousands(p.frames))
            } else {
                "the census has not run — this card prints nothing it did not measure".to_string()
            })
            .mono()
            .size(13.0)
            .track(2.0)
            .color(alpha(if measured { MUTED } else { crate::kit::AMBER }, 0.9 * bars))
            .center()
            .banner(H * 0.492)
            .into(),
        );
        for (i, (label, v)) in rows.into_iter().enumerate() {
            let y = H * 0.545 + i as f32 * 52.0;
            nodes.push(
                Type::new(label)
                    .mono()
                    .size(13.0)
                    .track(1.6)
                    .color(alpha(MUTED, 0.9 * bars))
                    .at(bx, y - 24.0)
                    .width(320.0)
                    .into(),
            );
            nodes.push(
                Type::new(if measured { thousands(v) } else { "—".to_string() })
                    .mono()
                    .size(17.0)
                    .color(alpha(INK, 0.96 * bars))
                    .right()
                    .at(bx + 860.0 - 320.0, y - 26.0)
                    .width(320.0)
                    .into(),
            );
        }
    }

    // The repo, the bench, and the sting's own line.
    let tail = smoothstep(seg(sec, 12.4, 13.4));
    nodes.push(
        Type::new(REPO)
            .mono()
            .size(15.0)
            .track(1.8)
            .color(alpha(INK_SOFT, 0.9 * tail))
            .center()
            .banner(H * 0.885)
            .into(),
    );
    nodes.push(
        Type::new(if p.bench.is_empty() {
            "bench not recorded".to_string()
        } else {
            format!("{} · median frame {:.2} ms at 1920×1080", p.bench, p.frame_ms)
        })
        .mono()
        .size(11.5)
        .track(1.4)
        .color(alpha(FAINT, 0.85 * tail))
        .center()
        .banner(H * 0.885 + 30.0)
        .into(),
    );

    // **The sting.** The last claim, and the only one the film can make about
    // itself: every frame you have watched was drawn by the thing it is about.
    let sting_a = smoothstep(seg(sec, STING_T, STING_T + 0.7));
    if sting_a > 0.004 {
        nodes.push(
            Type::new("every frame of this film was rendered by vieww")
                .size(32.0)
                .light()
                .track(0.5)
                .color(alpha(mix(INK, VIOLET_SOFT, 0.2 * st), 0.97 * sting_a))
                .center()
                .banner(H * 0.44)
                .into(),
        );
    }

    nodes.push(caption(
        "no browser · no compositor · nothing added in post",
        smoothstep(seg(sec, 7.4, 8.4)) * (1.0 - smoothstep(seg(ctx.t, 0.93, 0.99))),
    ));
    let _ = (MINT, bump, spring, clamp01, Size::new(0.0, 0.0));
    compose(bg, nodes)
}
