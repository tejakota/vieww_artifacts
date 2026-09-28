//! **S01 · THE WAIT** — 起, the cold open.
//!
//! The world the film argues against, shown rather than described: a build
//! that will not finish. A terminal on a dark desk, a dependency graph
//! compiling crate by crate, a progress bar that *regresses* when the
//! estimate is revised, and a clock counting real minutes off a real
//! developer's afternoon.
//!
//! Everything about this scene is deliberately poor, and every part of the
//! poverty is a mechanism the film will later turn off:
//! - **24-in-60.** A 24 Hz clock sampled inside the 60 Hz render, so the
//!   motion holds in a true 2-3-2-3 pattern. The judder is real; nothing is
//!   dropped. When Act I ends, the film cuts to smooth 60 and the difference
//!   is the argument.
//! - **Drained colour.** A measured Rec. 709 desaturation, not a mood.
//! - **Scanlines and grain**, drawn as geometry by the same rasterizer that
//!   will later draw the studio — so even the ugliness is a capability.
//!
//! No product is named here. No logo. The wait belongs to everyone.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Rect, Size, Sketchbook};
use vieww_widget::prelude::*;

use crate::film::Ctx;
use crate::kit::{
    alpha, caption, clamp01, drained, dust, ease_out_cubic, fbm1, grain, held_24, meter, mix,
    painter, panel, rule_h, scanlines, seg, smoothstep, thousands, vignette, xywh, Type, AMBER,
    FAINT, H, INK, INK_SOFT, MUTED, RED, VOID, W,
};
use crate::studio::compose;

/// The crates the build is chewing through. A generic graph — the names are
/// the vocabulary of any large Rust build, and none of them is vieww's.
const GRAPH: &[(&str, u32)] = &[
    ("proc-macro2", 41), ("quote", 12), ("syn", 289), ("serde_derive", 176),
    ("serde", 118), ("libc", 402), ("hashbrown", 96), ("indexmap", 44),
    ("regex-syntax", 331), ("regex-automata", 508), ("regex", 212),
    ("tokio-macros", 18), ("tokio", 674), ("tracing-core", 88),
    ("futures-util", 396), ("bytes", 71), ("http", 133), ("hyper", 418),
    ("rustls", 552), ("ring", 981), ("app-core", 1204), ("app", 1471),
];

/// Where the drained world sits on the desaturation axis.
const DRAIN: f32 = 0.82;

fn dimmed(c: Color) -> Color {
    drained(c, DRAIN)
}

pub fn frame(ctx: &Ctx) -> WidgetNode {
    // The judder: every animated quantity in this scene reads *held* time.
    let held = held_24(ctx.sec);
    let t = ctx.t;
    let abs = ctx.abs;
    let frame_i = ctx.frame;

    // The build's own arithmetic. It advances, then the estimate is revised
    // and the bar loses ground — which is the true shape of the thing and
    // the reason nobody trusts the number.
    let raw = clamp01(held / 13.5);
    let revise1 = smoothstep(seg(held, 6.2, 6.5));
    let revise2 = smoothstep(seg(held, 10.4, 10.7));
    let progress = (raw * 0.92 - revise1 * 0.14 - revise2 * 0.09).clamp(0.0, 0.86);

    // How many crates are done — one per graph step, from the same clock.
    let done = ((GRAPH.len() as f32) * clamp01(held / 12.0)).floor() as usize;
    let mb = 4.1 + 37.6 * clamp01(held / 12.6);
    let elapsed = 132.0 + held * 14.0;

    // The remaining estimate, which is the joke: it goes *up*.
    let eta = 48.0 + 96.0 * revise1 + 74.0 * revise2 + (1.0 - raw) * 40.0;

    let fade_in = smoothstep(seg(t, 0.0, 0.10));
    let fade_out = 1.0 - smoothstep(seg(t, 0.93, 1.0));
    let vis = fade_in * fade_out;

    let bg = painter(move |book: &mut Sketchbook, size: Size| {
        let (w, h) = (size.width, size.height);
        // The room: a near-black desk with one cold overhead source.
        book.rect(
            Rect::new(0.0, 0.0, w, h),
            Gradient::vertical().with_dither().with_stops(&[
                (0.0, Color::rgb(11, 12, 14)),
                (0.34, Color::rgb(7, 8, 10)),
                (1.0, VOID),
            ]),
        );
        // The monitor's own spill on the desk — the only warmth, and it is
        // not warm.
        book.blended_layer(1.0, 80.0, BlendMode::Plus, None, |g| {
            g.circle(
                Offset::new(w * 0.5, h * 0.46),
                w * 0.34,
                Gradient::radial_fill().with_dither().with_stops(&[
                    (0.0, alpha(Color::rgb(90, 104, 120), 0.10 * vis)),
                    (1.0, alpha(Color::rgb(60, 70, 90), 0.0)),
                ]),
            );
        });

        // The terminal: a plain rectangle, no rounding, no shadow. This is
        // not a designed surface; it is a hole in the dark.
        let term = xywh(w * 0.13, h * 0.16, w * 0.74, h * 0.60);
        book.rect(
            term,
            Gradient::vertical().with_dither().with_stops(&[
                (0.0, alpha(Color::rgb(13, 14, 17), 0.97 * vis)),
                (1.0, alpha(Color::rgb(9, 10, 12), 0.97 * vis)),
            ]),
        );
        book.stroke_rrect(term, 0.0, alpha(dimmed(FAINT), 0.30 * vis), 1.0);
        rule_h(book, term.left, term.top + 30.0, term.width(), alpha(dimmed(FAINT), 0.22 * vis));

        // The dependency graph, drawn as it is consumed: a column of nodes
        // with edges, each one going from pending grey to done grey. Nothing
        // is ever *bright* in this scene.
        let gx = term.left + 34.0;
        let gy = term.top + 60.0;
        let cols = 4usize;
        let cell_w = (term.width() - 380.0) / cols as f32;
        for (i, (_name, cost)) in GRAPH.iter().enumerate() {
            let col = i % cols;
            let row = i / cols;
            let x = gx + col as f32 * cell_w;
            let y = gy + row as f32 * 42.0;
            let is_done = i < done;
            let is_current = i == done;
            let bar_w = (cell_w - 46.0) * (0.34 + (*cost as f32 / 1500.0) * 0.66);
            let c = if is_done {
                alpha(dimmed(MUTED), 0.55 * vis)
            } else if is_current {
                alpha(dimmed(AMBER), (0.45 + 0.35 * (abs * 6.0).sin().abs()) * vis)
            } else {
                alpha(dimmed(FAINT), 0.20 * vis)
            };
            book.rect(xywh(x, y, bar_w, 4.0), c);
            book.circle(Offset::new(x - 12.0, y + 2.0), 3.0, c);
            // The edge back to the previous node — the graph is a graph.
            if i > 0 && col > 0 {
                book.line(
                    Offset::new(x - 12.0, y + 2.0),
                    Offset::new(x - cell_w + 30.0, y + 2.0),
                    alpha(dimmed(FAINT), 0.12 * vis),
                    1.0,
                );
            }
        }

        // The progress bar. Its ground is wide, its fill is grey, and it
        // goes backwards twice.
        let bar = xywh(term.left + 34.0, term.bottom - 92.0, term.width() - 68.0, 10.0);
        meter(
            book,
            bar,
            progress,
            alpha(dimmed(Color::rgb(30, 32, 38)), 0.9 * vis),
            Gradient::horizontal().with_dither().with_stops(&[
                (0.0, alpha(dimmed(MUTED), 0.75 * vis)),
                (1.0, alpha(dimmed(INK_SOFT), 0.55 * vis)),
            ]),
        );
        // The revision's own tell: a ghost of where the bar *was*.
        if revise1 > 0.02 || revise2 > 0.02 {
            let ghost = (raw * 0.92).min(0.86);
            book.rect(
                xywh(bar.left + bar.width() * progress, bar.top - 3.0, bar.width() * (ghost - progress).max(0.0), 16.0),
                alpha(dimmed(RED), 0.10 * vis * (revise1 + revise2).min(1.0)),
            );
        }

        // The clock: an analogue face, because a number does not feel like
        // time passing. Its second hand steps in the same 24 Hz hold.
        let cx = term.right - 110.0;
        let cy = term.top + 130.0;
        book.ring(Offset::new(cx, cy), 46.0, 1.5, alpha(dimmed(FAINT), 0.35 * vis));
        for i in 0..12 {
            let a = i as f32 / 12.0 * std::f32::consts::TAU;
            let (s, c) = a.sin_cos();
            book.line(
                Offset::new(cx + s * 38.0, cy - c * 38.0),
                Offset::new(cx + s * 43.0, cy - c * 43.0),
                alpha(dimmed(FAINT), 0.30 * vis),
                1.2,
            );
        }
        let secs = elapsed % 60.0;
        let mins = elapsed / 60.0;
        let sa = secs / 60.0 * std::f32::consts::TAU;
        let ma = (mins % 60.0) / 60.0 * std::f32::consts::TAU;
        book.line(
            Offset::new(cx, cy),
            Offset::new(cx + ma.sin() * 26.0, cy - ma.cos() * 26.0),
            alpha(dimmed(INK_SOFT), 0.55 * vis),
            2.2,
        );
        book.line(
            Offset::new(cx, cy),
            Offset::new(cx + sa.sin() * 38.0, cy - sa.cos() * 38.0),
            alpha(dimmed(AMBER), 0.55 * vis),
            1.2,
        );
        book.circle(Offset::new(cx, cy), 2.5, alpha(dimmed(INK_SOFT), 0.6 * vis));

        // The chair, the desk edge, a mug: a silhouette, so the frame has a
        // room in it. Three shapes, no more.
        let desk = h * 0.80;
        book.rect(
            xywh(0.0, desk, w, h - desk),
            Gradient::vertical().with_dither().with_stops(&[
                (0.0, alpha(Color::rgb(16, 17, 20), 0.95)),
                (1.0, alpha(VOID, 1.0)),
            ]),
        );
        rule_h(book, 0.0, desk, w, alpha(dimmed(FAINT), 0.18));
        let mug_x = w * 0.19;
        book.rrect(xywh(mug_x, desk - 44.0, 46.0, 44.0), 4.0, alpha(Color::rgb(22, 23, 27), 0.95));
        book.ring(Offset::new(mug_x + 56.0, desk - 26.0), 12.0, 3.0, alpha(Color::rgb(22, 23, 27), 0.95));
        // Steam that gave up: a thin drifting thread, fading.
        let steam = 1.0 - clamp01(held / 8.0);
        if steam > 0.02 {
            for i in 0..3 {
                let ph = i as f32 * 2.1;
                let mut pts = Vec::new();
                for k in 0..8 {
                    let u = k as f32 / 7.0;
                    pts.push(Offset::new(
                        mug_x + 23.0 + fbm1(u * 3.0 + ph + held * 0.6, 0x57EA, 2) * 14.0,
                        desk - 48.0 - u * 60.0,
                    ));
                }
                crate::kit::polyline(book, &pts, alpha(Color::rgb(120, 126, 136), 0.05 * steam * vis), 2.0);
            }
        }

        // The degradation grammar, last: the world's own texture.
        dust(book, size, held * 0.4, 40, Color::rgb(150, 160, 175), 0.5 * vis);
        scanlines(book, size, held, 0.9 * vis);
        grain(book, size, frame_i, 0.055 * vis, 900);
        vignette(book, size, 1.15);
        // The opening fade is the scene's own, drawn — not a transition
        // applied over it.
        if vis < 0.999 {
            book.rect(Rect::new(0.0, 0.0, w, h), alpha(Color::BLACK, 1.0 - vis));
        }
    });

    // The terminal's text. The log is the scene's dialogue.
    let term_l = W * 0.13;
    let term_t = H * 0.16;
    let term_r = W * 0.87;
    let term_b = H * 0.16 + H * 0.60;

    let mut nodes: Vec<WidgetNode> = vec![
        Type::new("~/work/app — cargo build")
            .mono()
            .size(14.0)
            .track(1.2)
            .color(alpha(dimmed(MUTED), 0.8 * vis))
            .at(term_l + 20.0, term_t + 8.0)
            .width(600.0)
            .into(),
        Type::new(format!("{}/{} crates", done.min(GRAPH.len()), GRAPH.len()))
            .mono()
            .size(14.0)
            .color(alpha(dimmed(MUTED), 0.7 * vis))
            .right()
            .at(term_r - 320.0, term_t + 8.0)
            .width(300.0)
            .into(),
    ];

    // The crate currently being compiled, named, in the one place the eye
    // goes. This is the whole content of the wait.
    if done < GRAPH.len() {
        let (name, cost) = GRAPH[done];
        nodes.push(
            Type::new(format!("   Compiling {name}"))
                .mono()
                .size(21.0)
                .color(alpha(dimmed(INK), 0.88 * vis))
                .at(term_l + 34.0, term_b - 152.0)
                .width(700.0)
                .into(),
        );
        nodes.push(
            Type::new(format!("{} units", thousands(u64::from(cost))))
                .mono()
                .size(13.0)
                .color(alpha(dimmed(FAINT), 0.8 * vis))
                .at(term_l + 40.0, term_b - 124.0)
                .width(300.0)
                .into(),
        );
    }

    // The three numbers that make the scene: elapsed, estimate, and the
    // download. The estimate is larger than it was thirty frames ago.
    nodes.push(
        Type::new(format!("{:02}:{:02} elapsed", (elapsed / 60.0) as u32, (elapsed % 60.0) as u32))
            .mono()
            .size(15.0)
            .track(1.4)
            .color(alpha(dimmed(INK_SOFT), 0.8 * vis))
            .at(term_l + 34.0, term_b - 62.0)
            .width(300.0)
            .into(),
    );
    nodes.push(
        Type::new(format!("{:.1} MB fetched", mb))
            .mono()
            .size(15.0)
            .track(1.4)
            .color(alpha(dimmed(MUTED), 0.75 * vis))
            .center()
            .at(term_l, term_b - 62.0)
            .width(term_r - term_l)
            .into(),
    );
    let eta_flash = (revise1 * (1.0 - smoothstep(seg(held, 6.5, 7.8)))
        + revise2 * (1.0 - smoothstep(seg(held, 10.7, 12.0))))
    .min(1.0);
    nodes.push(
        Type::new(format!("~{:02}:{:02} remaining", (eta / 60.0) as u32, (eta % 60.0) as u32))
            .mono()
            .size(15.0)
            .track(1.4)
            .color(alpha(mix(dimmed(MUTED), dimmed(RED), eta_flash), (0.75 + 0.25 * eta_flash) * vis))
            .right()
            .at(term_r - 334.0, term_b - 62.0)
            .width(300.0)
            .into(),
    );

    // The title, one line, no product: the film's thesis stated as a fact
    // about the audience's own week.
    let title_a = smoothstep(seg(t, 0.40, 0.56)) * (1.0 - smoothstep(seg(t, 0.86, 0.96))) * vis;
    nodes.push(
        Type::new("every change costs this")
            .size(46.0)
            .light()
            .track(1.0)
            .color(alpha(dimmed(INK), 0.92 * title_a))
            .center()
            .banner(H * 0.862)
            .into(),
    );

    nodes.push(caption("act i · the wait · 24 frames sampled in 60", ease_out_cubic(seg(t, 0.12, 0.26)) * vis * 0.9));
    let _ = (panel, ctx.spine.witness);
    compose(bg, nodes)
}
