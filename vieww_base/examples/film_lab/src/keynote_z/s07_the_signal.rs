//! S07 · THE SIGNAL — the law made visible: state lives outside the tree.
//! 0:55–1:04.
//!
//! A luminous line crosses the frame — the **session signal**, the
//! spine. Above and below it, widget trees mount, animate, and unmount
//! in sequence (the film's own scene changes, abstracted): boxes and
//! edges blooming in staggered springs, holding, dissolving. The line
//! does not move. A pulse of light travels it end to end, and every
//! node it passes under lights — *reading the signal is the
//! subscription.*
//!
//! This is the law the whole film runs on: the witness counter and the
//! session clock live in signals, constructed once, outside every
//! scene tree — which is why the counter survives every cut.

use vieww_foundation::{Color, Offset, Path, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{ease_out_cubic};
use super::{CANVAS, CYAN, CYAN_SOFT, Ctx, FAINT, H, INK, MINT, MUTED, VIOLET, VIOLET_SOFT, W, alpha, caption, clamp01, glow, grain, ground, mono, spring_out, stars, tint, vignette, xywh};


/// The signal line's y.
const SY: f32 = 540.0;
/// The line's x-extent.
const X0: f32 = 300.0;
const X1: f32 = 1620.0;

/// One tree's node: x offset from the tree's center, distance from the
/// signal line (positive = away, in the tree's direction), and which
/// generation it is (0 root, 1 mid, 2 leaf).
struct Node {
    x: f32,
    d: f32,
    gen: u8,
}

/// One tree — a small widget tree: a root, two mids, three leaves. The
/// root is farthest from the line; the leaves reach for it.
fn tree_nodes() -> Vec<Node> {
    vec![
        Node { x: 0.0, d: 152.0, gen: 0 },
        Node { x: -116.0, d: 64.0, gen: 1 },
        Node { x: 116.0, d: 64.0, gen: 1 },
        Node { x: -176.0, d: 6.0, gen: 2 },
        Node { x: -64.0, d: 6.0, gen: 2 },
        Node { x: 76.0, d: 6.0, gen: 2 },
    ]
}

/// The trees' mount windows, as (center_x, above_below, t0, t1) — the
/// scene fraction each tree lives for. The third tree straddles the
/// outro on purpose: the cut can interrupt a tree, never the line.
const TREES: [(f32, bool, f32, f32); 3] = [
    (620.0, true, 0.04, 0.44),
    (960.0, false, 0.34, 0.74),
    (1300.0, true, 0.60, 0.97),
];

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let frame_i = (ctx.abs * 60.0) as u64;

    // The pulse — a light packet traveling the line, left to right,
    // every 2.8 s; its position drives which nodes are lit.
    let pulse_cycle = 2.8;
    let pulse_x = X0 + (X1 - X0) * ((sec / pulse_cycle).fract());

    let mut stack = Stack::new();

    // The ground — the calmest register in the film: this is a law,
    // stated plainly.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            super::ground(book, w, h);
            super::stars(book, w, h, 0x5107, 70, t, 0.08);
            super::vignette(book, w, h, 0.52);
            super::grain(book, w, h, frame_i, 0.35);
        }),
    )));

    // THE LINE — the session signal: a base hairline, the signal's own
    // luminous body (a gradient that follows the pulse), and the pulse
    // itself, a bright packet with a trailing glow.
    let line = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            // The base — a hairline the whole width.
            book.line(Offset::new(X0, SY), Offset::new(X1, SY), alpha(Color::WHITE, 0.09), 1.0);
            // The signal body — brighter near the pulse, breathing.
            let near = |x: f32| 1.0 - ((x - pulse_x).abs() / 400.0).min(1.0);
            for (x, step) in [(X0, 24.0), (X0 + 264.0, 264.0), (X1 - 264.0, 264.0)] {
                let _ = step;
                let _ = x;
            }
            // Draw the body as a segmented stroke, alpha following the
            // pulse's proximity — the light is carried, not painted.
            let seg = 60.0;
            let mut x = X0;
            while x < X1 {
                let a = 0.10 + 0.65 * near(x + seg * 0.5);
                book.line(
                    Offset::new(x, SY),
                    Offset::new((x + seg).min(X1), SY),
                    alpha(VIOLET_SOFT, a),
                    2.2,
                );
                x += seg;
            }
            // The pulse — a hot packet with a comet tail.
            super::glow(book, pulse_x, SY, 130.0, VIOLET, 0.30);
            book.line(
                Offset::new((pulse_x - 90.0).max(X0), SY),
                Offset::new(pulse_x, SY),
                alpha(tint(VIOLET_SOFT, 0.5), 0.85),
                3.4,
            );
            book.circle(Offset::new(pulse_x, SY), 4.0, alpha(Color::WHITE, 0.95));
            // The endpoints — the signal's anchors.
            for ex in [X0, X1] {
                book.circle(Offset::new(ex, SY), 6.0, alpha(VIOLET_SOFT, 0.8));
                book.ring(Offset::new(ex, SY), 11.0, 1.2, alpha(VIOLET_SOFT, 0.5));
            }
        }),
    );
    stack = stack.push(Positioned::fill().child(line));

    // The trees — mounting, holding, unmounting around the line. Each
    // node blooms on a staggered spring; the tree dissolves upward when
    // its window closes. Nodes under the pulse light up cyan.
    for (cx, above, t0, t1) in TREES {
        let in_p = clamp01((t - t0) / 0.18);
        let out_p = clamp01((t - t1) / 0.14);
        if in_p <= 0.01 {
            continue;
        }
        let dir: f32 = if above { -1.0 } else { 1.0 };
        let rise = (1.0 - ease_out_cubic(in_p)) * 30.0 - out_p * 34.0;
        let nodes = tree_nodes();
        let tree_a = ease_out_cubic(in_p) * (1.0 - ease_out_cubic(out_p));

        // The edges — beneath the nodes, in world coordinates.
        let edges = Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                if tree_a <= 0.02 {
                    return;
                }
                let ns = tree_nodes();
                let world = |n: &Node| Offset::new(cx + n.x, SY + dir * n.d + rise);
                let root = world(&ns[0]);
                for m in [1usize, 2usize] {
                    let mid = world(&ns[m]);
                    book.line(root, mid, alpha(MUTED, 0.5 * tree_a), 1.2);
                    let (leaf_start, leaf_n) = if m == 1 { (3usize, 2usize) } else { (5usize, 1usize) };
                    for l in leaf_start..leaf_start + leaf_n {
                        let leaf = world(&ns[l]);
                        book.line(mid, leaf, alpha(MUTED, 0.38 * tree_a), 1.0);
                    }
                }
            }),
        );
        stack = stack.push(Positioned::fill().child(edges));

        // The nodes — each its own widget, blooming on its own spring.
        for (ni, node) in nodes.iter().enumerate() {
            let node_in = clamp01((in_p - ni as f32 * 0.05) / 0.5);
            if node_in <= 0.01 {
                continue;
            }
            let node_out = clamp01((out_p - ni as f32 * 0.03) / 0.4);
            let a = ease_out_cubic(node_in) * (1.0 - ease_out_cubic(node_out));
            let sp = spring_out(node_in, 10.0, 0.6);
            let sc = 0.6 + 0.4 * sp;
            let nx = cx + node.x;
            let ny = SY + dir * node.d + rise;
            // Lit when the pulse passes beneath.
            let lit = 1.0 - ((nx - pulse_x).abs() / 220.0).min(1.0);
            let node_col = super::mix(VIOLET_SOFT, CYAN_SOFT, lit);
            let w = 52.0 * sc;
            let h = 30.0 * sc;
            let nw = w.max(6.0);
            let nh = h.max(4.0);
            let gen = node.gen;
            let block = Painting::sized(
                Size::new(nw, nh),
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    // The box — a widget in the tree.
                    book.rrect(
                        xywh(0.0, 0.0, nw, nh),
                        6.0,
                        alpha(super::mix(Color::rgb(18, 18, 24), node_col, 0.12 + lit * 0.20), a * 0.95),
                    );
                    book.stroke_rrect(xywh(0.0, 0.0, nw, nh), 6.0, alpha(node_col, a * (0.35 + lit * 0.55)), 1.2);
                    // The gen ticks — root/mid/leaf markers.
                    for g in 0..=gen {
                        book.rrect(xywh(6.0 + g as f32 * 6.0, nh - 9.0, 3.0, 4.0), 1.0, alpha(node_col, a * 0.6));
                    }
                    if lit > 0.25 {
                        super::glow(book, nw * 0.5, nh * 0.5, 60.0, CYAN, lit * 0.35 * a);
                    }
                }),
            );
            stack = stack.push(
                Positioned::new()
                    .left(nx - nw * 0.5)
                    .top(ny - nh * 0.5)
                    .width(nw)
                    .height(nh)
                    .child(Opacity::new(a.max(0.0)).child(block)),
            );
        }
    }

    // The signal's label — left margin, the law's name.
    let label_a = clamp01((t - 0.10) / 0.12);
    stack = stack.push(
        Positioned::new()
            .left(300.0)
            .top(196.0)
            .width(800.0)
            .height(40.0)
            .child(Opacity::new(label_a).child(
                Text::new("the session — one signal, outside every tree")
                    .style(TextStyle::new(28.0).weight(vieww_foundation::FontWeight::Medium).letter_spacing(1.6).color(alpha(INK, 0.96)))
                    .align(TextAlign::Left),
            )),
    );

    // The endpoint annotations — mono, the signal's type signature.
    stack = stack.push(
        Positioned::new()
            .left(300.0)
            .top(SY + 16.0)
            .width(360.0)
            .height(26.0)
            .child(Opacity::new(label_a).child(
                Text::new("Signal<u32> · the witness")
                    .style(TextStyle::new(15.0).monospace().letter_spacing(1.6).color(alpha(MUTED, 0.9)))
                    .align(TextAlign::Left),
            )),
    );
    stack = stack.push(
        Positioned::new()
            .left(X1 - 360.0)
            .top(SY - 40.0)
            .width(360.0)
            .height(26.0)
            .child(Opacity::new(label_a).child(
                Text::new("Signal<Seconds> · the clock")
                    .style(TextStyle::new(15.0).monospace().letter_spacing(1.6).color(alpha(MUTED, 0.9)))
                    .align(TextAlign::Right),
            )),
    );

    // The captions — the law's statement, in two beats.
    stack = stack.push(caption(
        "trees mount. trees unmount. the line persists.",
        1002.0,
        clamp01((t - 0.16) / 0.12),
    ));
    stack = stack.push(caption(
        "state lives outside the tree — the counter survives every cut",
        966.0,
        clamp01((t - 0.52) / 0.12),
    ));

    let _ = (FAINT, MINT, Path::new(), super::H, sec);

    stack.into()
}
