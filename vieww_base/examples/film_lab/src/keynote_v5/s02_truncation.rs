//! S02 · SILENT TRUNCATION — the tree you wrote is not the tree you get.
//! 0:10–0:20.
//!
//! A rendering tree as a living organism: a root, a trunk, branches, leaves
//! — every node placed deterministically, glowing faintly, swaying. Then
//! the platform's depth limit arrives as a vertical clip plane sweeping in
//! from the right, and everything past depth three desaturates, loosens,
//! and comes apart into dust (the typo plate's grammar — text becomes
//! weather; here a tree becomes weather). No error is thrown. Nothing
//! tells you. The word **silently** is the scene's only raised voice.

use vieww_foundation::{Color, Gradient, Offset, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, mix, tint, xywh, Rng, CYAN_SOFT, MUTED, RED, VIOLET, VIOLET_SOFT,
};

use super::Ctx;

/// One tree node: position, depth, parent index, sway phase.
struct Node {
    x: f32,
    y: f32,
    depth: u32,
    parent: usize,
    phase: f32,
    leaf: bool,
}

/// The tree — deterministic, hand-shaped by rules, not typed points.
fn tree() -> Vec<Node> {
    let mut rng = Rng::new(0x7302);
    let mut nodes = vec![Node {
        x: 210.0,
        y: 560.0,
        depth: 0,
        parent: 0,
        phase: 0.0,
        leaf: false,
    }];
    // A loose binary spread, four and five levels deep — deep enough that
    // the truncation at depth 3 visibly amputates most of it.
    let mut i = 0;
    while i < nodes.len() {
        let (x, y, d) = (nodes[i].x, nodes[i].y, nodes[i].depth);
        if d >= 5 {
            nodes[i].leaf = true;
            i += 1;
            continue;
        }
        let spread = 150.0 * (1.0 - d as f32 * 0.16);
        let n_kids = if d == 0 { 3 } else { 2 };
        for k in 0..n_kids {
            let jx = (k as f32 - (n_kids - 1) as f32 * 0.5) * spread + rng.sym() * 34.0;
            let jy = -68.0 - rng.f01() * 44.0;
            nodes.push(Node {
                x: x + jx,
                y: y + jy,
                depth: d + 1,
                parent: i,
                phase: rng.f01() * std::f32::consts::TAU,
                leaf: d + 1 == 5,
            });
        }
        i += 1;
    }
    nodes
}

/// When the clip plane (x position, film px) has passed a node.
pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let frame_i = (ctx.abs * 60.0) as u64;

    let nodes = tree();

    // The clip plane sweeps right→left? No — it arrives from the right and
    // sweeps left, like a scissor coming down the canvas: everything it
    // passes is "optimized away".
    let plane_x = 1780.0 - ease_in_out(clamp01((t - 0.30) / 0.34)) * 900.0;

    let mut stack = Stack::new();

    // The ground + a softer, sicker starfield.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            super::ground(book, w, h);
            super::stars(book, w, h, 0x2EED, 70, t, 0.06);
            // A cold horizon under the tree.
            book.layer(1.0, 40.0, None, |g| {
                g.circle(
                    Offset::new(w * 0.30, h * 0.86),
                    w * 0.26,
                    Gradient::radial_fill()
                        .with_dither()
                        .with_stops(&[(0.0, alpha(CYAN_SOFT, 0.05)), (1.0, alpha(CYAN_SOFT, 0.0))]),
                );
            });
            super::vignette(book, w, h, 0.5);
        }),
    )));

    // The tree itself — `nodes` moves into the paint closure (the tree is
    // owned; the closure is 'static by construction).
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            // Pass 1 — the branches (edges) with their dissolve state.
            for n in nodes.iter() {
                if n.depth == 0 {
                    continue;
                }
                let p = &nodes[n.parent];
                let sway = (t * 0.9 + n.phase).sin() * (2.0 + n.depth as f32 * 0.9);
                let cut = plane_x < n.x;
                let since = if cut {
                    ((1780.0 - plane_x) - (1780.0 - n.x)) / 265.0
                } else {
                    -1.0
                };
                let dissolve = clamp01(since);
                let a = (1.0 - dissolve) * if n.depth <= 3 { 0.85 } else { 0.7 };
                let col = if n.depth <= 3 {
                    alpha(VIOLET_SOFT, a * 0.55)
                } else {
                    alpha(mix(VIOLET, CYAN_SOFT, 0.4), a * 0.5)
                };
                let mut path = vieww_foundation::Path::new();
                let mid = Offset::new(
                    (p.x + n.x) / 2.0 + (n.y - p.y) * 0.12,
                    (p.y + n.y) / 2.0 + sway * 0.5,
                );
                path.move_to(Offset::new(p.x, p.y + sway * 0.4));
                path.cubic_to(
                    Offset::new(p.x, p.y - 30.0 + sway),
                    mid,
                    Offset::new(n.x + sway, n.y),
                );
                book.stroke(path, col, (3.4 - n.depth as f32 * 0.5).max(1.1));
            }
            // Pass 2 — the nodes: living ones glow, dying ones dust apart.
            for n in nodes.iter() {
                let sway = (t * 0.9 + n.phase).sin() * (2.0 + n.depth as f32 * 0.9);
                let cut = plane_x < n.x;
                let since = if cut {
                    ((1780.0 - plane_x) - (1780.0 - n.x)) / 265.0
                } else {
                    -1.0
                };
                let dissolve = clamp01(since);
                let x = n.x + sway;
                let y = n.y;
                if dissolve >= 1.0 {
                    continue;
                }
                if !cut || dissolve <= 0.0 {
                    // Alive — a node, leaves slightly brighter.
                    let r = if n.leaf {
                        3.4
                    } else {
                        5.6 - n.depth as f32 * 0.5
                    };
                    let c = if n.leaf {
                        alpha(tint(CYAN_SOFT, 0.2), 0.85)
                    } else {
                        alpha(VIOLET_SOFT, 0.9)
                    };
                    book.circle(Offset::new(x, y), r, c);
                    book.layer(1.0, 8.0, None, |g| {
                        g.circle(
                            Offset::new(x, y),
                            r * 3.4,
                            Gradient::radial_fill().with_dither().with_stops(&[
                                (0.0, alpha(VIOLET, 0.22)),
                                (1.0, alpha(VIOLET, 0.0)),
                            ]),
                        );
                    });
                } else {
                    // Dying — the glyph loosens into specks that fall.
                    let mut rng = Rng::new(0xD177 ^ (n.x as u64) << 8 ^ n.y as u64);
                    for _ in 0..10 {
                        let ang = rng.f01() * std::f32::consts::TAU;
                        let dist = rng.f01() * 16.0 * dissolve;
                        let fall = dissolve * dissolve * 120.0 * rng.f01();
                        let drift = (t * 2.0 + rng.f01() * 7.0).sin() * 8.0 * dissolve;
                        let a = (1.0 - dissolve) * (0.4 + rng.f01() * 0.5);
                        book.circle(
                            Offset::new(
                                x + ang.cos() * dist + drift,
                                y + ang.sin() * dist * 0.6 + fall,
                            ),
                            0.8 + rng.f01() * 1.6,
                            alpha(CYAN_SOFT, a),
                        );
                    }
                }
            }
        }),
    )));

    // The clip plane — a hairline with a diamond head, cold and certain.
    let plane_a = clamp01((t - 0.28) / 0.10);
    if plane_a > 0.0 {
        let px = plane_x;
        stack = stack.push(
            Positioned::fill().child(Opacity::new(plane_a).child(Painting::sized(
                super::CANVAS,
                PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                    let h = s.height;
                    book.layer(1.0, 16.0, None, |g| {
                        g.rect(xywh(px - 30.0, 0.0, 60.0, h), alpha(RED, 0.07));
                    });
                    book.rrect(
                        xywh(px - 1.0, 0.0, 2.0, h),
                        1.0,
                        alpha(tint(RED, 0.15), 0.9),
                    );
                    // The diamond head.
                    let mut d = vieww_foundation::Path::new();
                    d.move_to(Offset::new(px, 470.0));
                    d.line_to(Offset::new(px + 16.0, 540.0));
                    d.line_to(Offset::new(px, 610.0));
                    d.line_to(Offset::new(px - 16.0, 540.0));
                    d.close();
                    book.fill(d.clone(), alpha(tint(RED, 0.2), 0.95));
                    book.stroke(d, alpha(Color::WHITE, 0.5), 1.0);
                }),
            ))),
        );
    }

    // The depth labels — a quiet ruler at the tree's base.
    let ruler_a = clamp01((t - 0.10) / 0.3);
    if ruler_a > 0.0 {
        stack = stack.push(
            Positioned::new()
                .left(150.0)
                .top(620.0)
                .width(1500.0)
                .height(400.0)
                .child(Opacity::new(ruler_a).child(Painting::sized(
                    Size::new(1500.0, 400.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        for d in 0..6u32 {
                            let x = 60.0 + d as f32 * 210.0;
                            let gone = plane_x < x;
                            book.line(
                                Offset::new(x, 0.0),
                                Offset::new(x, -14.0),
                                alpha(if gone { RED } else { MUTED }, if gone { 0.8 } else { 0.4 }),
                                1.4,
                            );
                        }
                    }),
                ))),
        );
        for d in 0..6u32 {
            let gone = plane_x < 60.0 + 150.0 + d as f32 * 210.0;
            let c = if gone {
                alpha(tint(RED, 0.2), 0.95)
            } else {
                alpha(MUTED, 0.75)
            };
            stack = stack.push(
                Positioned::new()
                    .left(150.0 + 30.0 + d as f32 * 210.0)
                    .top(652.0)
                    .width(200.0)
                    .height(24.0)
                    .child(
                        Opacity::new(ruler_a).child(
                            Text::new(format!("depth {}", d)).style(
                                TextStyle::new(16.0)
                                    .monospace()
                                    .letter_spacing(1.5)
                                    .color(c),
                            ),
                        ),
                    ),
            );
        }
    }

    // The warning — S01's line, now the scene's voice.
    let warn_a = clamp01((t - 0.66) / 0.12);
    if warn_a > 0.0 {
        let pulse = 0.5 + 0.5 * (sec * 3.0).sin();
        stack =
            stack.push(
                Positioned::new()
                    .left(0.0)
                    .top(860.0)
                    .width(1920.0)
                    .height(50.0)
                    .child(
                        Opacity::new(warn_a).child(
                            Text::new("rendering tree truncated at depth 3 — silently")
                                .style(TextStyle::new(30.0).monospace().letter_spacing(3.0).color(
                                    mix(
                                        alpha(tint(RED, 0.2), 0.95),
                                        alpha(Color::WHITE, 0.9),
                                        pulse * 0.18,
                                    ),
                                ))
                                .align(TextAlign::Center),
                        ),
                    ),
            );
    }

    // Grain — less than S01: the world is waking.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            super::grain(book, s.width, s.height, frame_i, 0.8);
        }),
    )));

    stack = stack.push(super::caption(
        "your tree was amputated. nobody asked you.",
        1000.0,
        clamp01((t - 0.72) / 0.12),
    ));

    stack.into()
}
