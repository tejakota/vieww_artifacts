//! B01 · THREE TREES — the engine's answer, part one. 1:10–1:24.
//!
//! The tower does not get taller; it gets *replaced*. The seven toll
//! slabs shatter and dissolve, and out of their dust three clean shapes
//! assemble — three trees, one job each: **Widget** (what you mean),
//! **Element** (what persists across rebuilds), **RenderObject** (what
//! draws). Commands flow down them as pulses of light, and the gap
//! line's sag lifts as the middleman's weight leaves it.
//!
//! The engine's register arrives with them: cyan — the machine act's
//! colour, the film's own palette for everything vieww *is*.

use vieww_foundation::{Color, Offset, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;

use super::{
    ACCENT, Ctx, ENGINE, MUTED, W, alpha, caption, clamp01, distance_chip, gap_line, glow, grain,
    ground, pole_caret, pole_screen, progress_rail, spring_out, tint, vignette, xywh,
};
use crate::film_lib::{Rng, ease_out_cubic};

/// The three trees — name, sub, colour, x position.
const TREES: [(&str, &str, f32); 3] = [
    ("WIDGET", "what you mean", 700.0),
    ("ELEMENT", "what persists", 960.0),
    ("RENDER", "what draws", 1220.0),

];
/// The tree glyph's home y.
const TREE_Y: f32 = 470.0;
const TREE_R: f32 = 74.0;

/// One tree glyph: a node at the root, two children, four leaves — a
/// binary tree drawn as strokes, breathing on its own harmonic.
fn tree_glyph(book: &mut Sketchbook, cx: f32, cy: f32, r: f32, t: f32, a: f32, color: Color) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.01 {
        return;
    }
    let sway = (t * 0.8 + cx * 0.01).sin() * 0.02;
    let depth = 3;
    // Recursive binary fan: each level half the span, tilted by sway.
    fn fan(
        book: &mut Sketchbook,
        x0: f32,
        y0: f32,
        span: f32,
        level: usize,
        depth: usize,
        sway: f32,
        t: f32,
        a: f32,
        color: Color,
    ) {
        if level >= depth {
            // A leaf — a small node at the branch's end.
            book.circle(Offset::new(x0, y0), 3.2, alpha(color, a));
            return;
        }
        let grow = 1.0 - level as f32 * 0.22;
        let dy = span * 0.9;
        for (dir, k) in [(-1.0f32, 0.0f32), (1.0, 0.5)] {
            let dx = span * 0.5 * dir * (1.0 + sway * 10.0);
            let x1 = x0 + dx;
            let y1 = y0 + dy * grow;
            book.line(Offset::new(x0, y0), Offset::new(x1, y1), alpha(color, a * 0.8), 2.2);
            fan(book, x1, y1, span * 0.5, level + 1, depth, sway + k * 0.1, t, a, color);
        }
        if level == 0 {
            book.circle(Offset::new(x0, y0), 5.5, alpha(tint(color, 0.3), a));
        }
    }
    fan(book, cx, cy - r * 0.5, r * 1.2, 0, depth, sway, t, a, color);
    // The halo — the tree's identity as one organism.
    glow(book, cx, cy, r * 1.7, color, a * 0.10);
}

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let frame_i = (ctx.abs * 60.0) as u64;

    let room = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            ground(book, w, h);
            vignette(book, w, h, 0.58);
            grain(book, w, h, frame_i, 0.4);
            // The toll tower's dust — dissolving upward, gone by mid-scene.
            let dissolve = clamp01((t - 0.02) / 0.3);
            if dissolve < 1.0 {
                let mut rng = Rng::new(0x7011);
                for _ in 0..90 {
                    let bx = 800.0 + rng.f01() * 320.0;
                    let by = 560.0 - rng.f01() * 380.0 * dissolve - dissolve * 160.0;
                    let r = 0.8 + rng.f01() * 2.2;
                    book.circle(
                        Offset::new(bx, by),
                        r,
                        alpha(MUTED, 0.5 * (1.0 - dissolve)),
                    );
                }
            }
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(room));

    // The gap line — the sag lifts as the tower dissolves.
    let sag = 42.0 * (1.0 - ease_out_cubic(clamp01((t - 0.05) / 0.35)));
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            gap_line(book, 430.0, 560.0, 1360.0, 0.0, sec, sag, 0.0, MUTED, 0.9);
            pole_caret(book, 560.0, 430.0, sec, 1.0);
            pole_screen(book, 1360.0, 430.0, 0.15, 0.9, MUTED);
        }),
    )));

    // The three trees — assembling in order, root first.
    for (i, (name, sub, x)) in TREES.iter().enumerate() {
        let grow = spring_out(clamp01((t - 0.14 - i as f32 * 0.13) / 0.6), 7.0, 0.66);
        if grow <= 0.02 {
            continue;
        }
        let tree_a = ease_out_cubic(clamp01((t - 0.14 - i as f32 * 0.13) / 0.3));
        let x = *x;
        let name = *name;
        let sub = *sub;
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                tree_glyph(book, x, TREE_Y, TREE_R * grow.max(0.08), sec, tree_a, ENGINE);
            }),
        )));
        // The name + role, under each tree.
        if tree_a > 0.3 {
            let na = (tree_a - 0.3) / 0.7;
            stack = stack.push(
                Positioned::new()
                    .left(x - 160.0)
                    .top(TREE_Y + 110.0)
                    .width(320.0)
                    .height(28.0)
                    .child(
                        super::Opacity::new(na).child(
                            Text::new(name)
                                .style(
                                    TextStyle::new(20.0)
                                        .monospace()
                                        .letter_spacing(4.0)
                                        .color(alpha(ENGINE, 0.95)),
                                )
                                .align(TextAlign::Center),
                        ),
                    ),
            );
            stack = stack.push(
                Positioned::new()
                    .left(x - 160.0)
                    .top(TREE_Y + 142.0)
                    .width(320.0)
                    .height(24.0)
                    .child(
                        super::Opacity::new(na * 0.9).child(
                            Text::new(sub)
                                .style(
                                    TextStyle::new(14.0)
                                        .monospace()
                                        .letter_spacing(1.8)
                                        .color(alpha(MUTED, 0.85)),
                                )
                                .align(TextAlign::Center),
                        ),
                    ),
            );
        }
    }

    // The command flow — pulses travelling tree→tree→screen, the pipeline
    // alive. Three pulses, spaced, riding the line's height.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            let flow_a = clamp01((t - 0.5) / 0.3);
            if flow_a > 0.02 {
                for k in 0..3 {
                    let phase = (sec * 0.55 + k as f32 / 3.0).fract();
                    let x = 700.0 + (1220.0 - 700.0) * phase;
                    let y = 430.0 + 12.0 * (phase * std::f32::consts::PI).sin();
                    let trail = 0.4 + 0.6 * (phase * std::f32::consts::PI).sin();
                    glow(book, x, y, 26.0, ENGINE, flow_a * 0.5 * trail);
                    book.circle(Offset::new(x, y), 3.4, alpha(tint(ENGINE, 0.3), flow_a * trail));
                }
            }
        }),
    )));

    // The caption + the act chip — movement II opens.
    stack = stack.push(super::act_chip("MOVEMENT II", "THE ENGINE", clamp01((t - 0.04) / 0.10)));
    stack = stack.push(caption(
        "vieww: three trees, one job each — and nothing between them and the pixels",
        1002.0,
        clamp01((t - 0.06) / 0.12),
    ));
    stack = stack.push(caption(
        "widgets describe intent · elements keep identity · render objects draw",
        966.0,
        clamp01((t - 0.52) / 0.12),
    ));

    stack = stack.push(distance_chip(ctx.abs, clamp01(t / 0.1)));
    stack = stack.push(progress_rail(ctx.abs));

    let _ = W;
    let _ = ACCENT;
    stack.into()
}
