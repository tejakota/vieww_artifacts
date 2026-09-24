//! exp_globe — **the reveal spine, E-20 F1–F4.** The last 20 seconds' core.
//!
//! Four frames of the storyboard, played as one continuous motion:
//!
//! - **F1 · the return** (t 0–0.17): the counter, full-frame, reading 7 —
//!   huge, still, certain. The session line arrives *node by node* along the
//!   floor of the frame, each touch blooming as it lands.
//! - **F2 · the turn** (t 0.18–0.32): ghosts of the counter part by pixels —
//!   two blurred copies (a `Text` through `Filtered::blur`, the rack-focus
//!   grammar at reveal scale) sliding apart as the completed line extends
//!   into **the axis**: one bright line leaving the composition. Caption,
//!   F2's own words: *what you watched, from outside.*
//! - **F3 · depth opens** (t 0.32–0.55): the root pull-back, spring-eased —
//!   camera rises and retreats, the floor plane arrives in perspective, fog
//!   at the horizon; the 7 recedes to the axis's head.
//! - **F4 · the unfold** (t 0.55–1.0): three planes fan out — description ·
//!   identity · geometry — each a translucent quad in the framework's own
//!   2D-projected 3D, carrying its tree glyph blooming level by level
//!   (annotations bloom in sequence, never simultaneously).
//!
//! Layer colors are the storyboard's own: description `#58a6ff`, identity
//! `#3fb950`, geometry `#ffa657`. Every 3D pixel is the framework's raster:
//! vertices projected here, quads and lines are ordinary paths.

use vieww_foundation::{
    Color, FontWeight, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle, TextAlign,
};
use vieww_widget::prelude::*;
use vieww_widget::{Filtered, Opacity, Painting, PaintWith, Transformed};

use crate::film_lib::{
    alpha, clamp01, ease_out_expo, mix, spring_out, BG_DEEP, CANVAS, CANVAS_W, INK, MUTED, Rng,
    VIOLET, VIOLET_SOFT,
};
use crate::three_d::{Camera, Vec3};

/// Film-time this experiment spans — the reveal's first four frames.
pub const SECONDS: f32 = 20.0;

// The storyboard's layer colors, verbatim from e20_storyboard_v1.svg.
const C_DESCRIPTION: Color = Color::rgb(88, 166, 255);
const C_IDENTITY: Color = Color::rgb(63, 185, 80);
const C_GEOMETRY: Color = Color::rgb(255, 166, 87);

/// The session line's nodes for F1 — the ladder's seven touches, node by node.
const LINE_NODES: usize = 7;

/// The big counter's home (F1): left-center mass.
const BIG_X: f32 = 176.0;
const BIG_Y: f32 = 214.0;
const BIG_SIZE: f32 = 216.0;

/// The receded counter's home (F3+): the axis head, top-center.
const SMALL_X: f32 = 612.0;
const SMALL_Y: f32 = 58.0;
const SMALL_SIZE: f32 = 60.0;

// ── Phase gates ─────────────────────────────────────────────────────────────

fn f1_t(t: f32) -> f32 {
    clamp01(t / 0.17)
}
fn f2_t(t: f32) -> f32 {
    clamp01((t - 0.18) / 0.14)
}
fn f3_t(t: f32) -> f32 {
    clamp01((t - 0.32) / 0.23)
}
fn f4_t(t: f32) -> f32 {
    clamp01((t - 0.55) / 0.45)
}

// ── Geometry: the axis, the floor, the planes ───────────────────────────────

/// The session line (F1/F2) runs along the bottom; its end node feeds the axis.
const LINE_Y: f32 = 636.0;
const LINE_X0: f32 = 96.0;
const LINE_X1: f32 = 1152.0;

/// The axis: from the line's end to the head. Rises in F2, straightens in F3.
fn axis_geometry(t: f32) -> (Offset, Offset) {
    let f2 = f2_t(t);
    let f3 = f3_t(t);
    let rise = spring_out(f2, 7.0, 0.75);
    let straight = ease_out_expo(f3);
    // The base: starts at the line's end, settles to bottom-center.
    let base = Offset::new(
        LINE_X1 + (CANVAS_W * 0.5 - LINE_X1) * straight,
        LINE_Y + (664.0 - LINE_Y) * straight,
    );
    // The tip: rises to top-center as F2 plays.
    let tip = Offset::new(
        LINE_X1 + (SMALL_X + 26.0 - LINE_X1) * rise,
        LINE_Y + (SMALL_Y + 30.0 - LINE_Y) * rise,
    );
    (base, tip)
}

/// The camera: the root pull-back, spring-eased (F3).
fn camera_at(t: f32) -> Camera {
    let f3 = f3_t(t);
    let pull = spring_out(f3, 6.0, 0.8);
    Camera {
        eye: Vec3::new(0.0, 5.4 + 3.8 * pull, 15.5 + 7.5 * pull),
        target: Vec3::new(0.0, 2.0 + 0.3 * pull, 0.0),
        fov: 0.9,
    }
}

/// One fanned plane: label, color, fan-end center, rotY, tilt, stagger.
struct PlaneSpec {
    label: &'static str,
    color: Color,
    end_center: Vec3,
    rot_y: f32,
    tilt: f32,
    stagger: f32,
}

const PLANE_W: f32 = 7.4;
const PLANE_H: f32 = 4.6;

const PLANES: [PlaneSpec; 3] = [
    PlaneSpec {
        label: "description",
        color: C_DESCRIPTION,
        end_center: Vec3::new(-4.9, 3.1, 1.0),
        rot_y: 0.62,
        tilt: 0.10,
        stagger: 0.0,
    },
    PlaneSpec {
        label: "identity",
        color: C_IDENTITY,
        end_center: Vec3::new(0.0, 3.3, 0.2),
        rot_y: 0.0,
        tilt: 0.0,
        stagger: 0.10,
    },
    PlaneSpec {
        label: "geometry",
        color: C_GEOMETRY,
        end_center: Vec3::new(4.9, 3.1, 1.0),
        rot_y: -0.62,
        tilt: 0.10,
        stagger: 0.20,
    },
];

/// A plane's corner in world space at fan progress `p` (0 = stacked deck,
/// 1 = fanned). `center`, `rot_y`, `tilt` interpolate from the deck pose.
fn plane_pose(spec: &PlaneSpec, p: f32) -> (Vec3, f32, f32) {
    let deck_center = Vec3::new(0.0, 3.1, 2.8);
    let center = deck_center.lerp(spec.end_center, p);
    (center, spec.rot_y * p, spec.tilt * p)
}

/// Plane-local (u, v) in [-0.5, 0.5]² → world position.
fn plane_world(center: Vec3, rot_y: f32, tilt: f32, u: f32, v: f32) -> Vec3 {
    // Local axes: right (rotY), up (rotY then tilt back).
    let right = Vec3::new(rot_y.cos(), 0.0, -rot_y.sin());
    let up_raw = Vec3::new(0.0, tilt.cos(), tilt.sin());
    let up = Vec3::new(
        up_raw.x * rot_y.cos() - up_raw.z * rot_y.sin(),
        up_raw.y,
        up_raw.x * rot_y.sin() + up_raw.z * rot_y.cos(),
    );
    center.add(right.scale(u * PLANE_W)).add(up.scale(v * PLANE_H))
}

/// A tree glyph: nodes in plane-local uv, edges as (parent, child) indices.
struct TreeGlyph {
    nodes: Vec<(f32, f32)>,
    edges: Vec<(usize, usize)>,
}

/// Description's tree — the widget tree, wide and shallow.
fn tree_description() -> TreeGlyph {
    TreeGlyph {
        nodes: vec![
            (0.0, -0.34),
            (-0.24, -0.06),
            (0.24, -0.06),
            (-0.36, 0.24),
            (-0.13, 0.24),
            (0.13, 0.24),
            (0.36, 0.24),
        ],
        edges: vec![(0, 1), (0, 2), (1, 3), (1, 4), (2, 5), (2, 6)],
    }
}

/// Identity's tree — the element tree, balanced.
fn tree_identity() -> TreeGlyph {
    TreeGlyph {
        nodes: vec![
            (0.0, -0.36),
            (-0.18, 0.0),
            (0.18, 0.0),
            (-0.28, 0.30),
            (-0.02, 0.30),
            (0.28, 0.30),
        ],
        edges: vec![(0, 1), (0, 2), (1, 3), (1, 4), (2, 5)],
    }
}

/// Geometry's tree — the render tree, deep and narrow.
fn tree_geometry() -> TreeGlyph {
    TreeGlyph {
        nodes: vec![
            (0.0, -0.38),
            (0.0, -0.10),
            (-0.16, 0.20),
            (0.16, 0.20),
            (-0.16, 0.42),
            (0.16, 0.42),
        ],
        edges: vec![(0, 1), (1, 2), (1, 3), (2, 4), (3, 5)],
    }
}

// ── The scene painter ───────────────────────────────────────────────────────

fn scene(book: &mut Sketchbook, t: f32) {
    let w = CANVAS_W;
    let h = CANVAS.height;

    // The void — deepest of the film.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::vertical().with_dither().with_stops(&[
            (0.0, Color::rgb(7, 7, 11)),
            (0.55, BG_DEEP),
            (1.0, Color::rgb(5, 5, 9)),
        ]),
    );

    // Stars — calm, sparse, twinkling slow.
    let mut rng = Rng::new(0x90D);
    for _ in 0..54 {
        let x = rng.f01() * w;
        let y = rng.f01() * h;
        let r = 0.4 + rng.f01() * 0.9;
        let tw = 0.5 + 0.5 * (t * 2.6 + rng.f01() * 10.0).sin();
        book.circle(Offset::new(x, y), r, alpha(Color::WHITE, 0.03 + 0.07 * tw));
    }

    // ── F3+ · the floor plane, in perspective ───────────────────────────────
    let f3 = f3_t(t);
    let f4 = f4_t(t);
    if f3 > 0.0 {
        let cam = camera_at(t);
        let floor_alpha = 0.5 * f3;
        // Horizon glow first — the fog the floor emerges from.
        book.layer(1.0, 36.0, None, |inner| {
            inner.circle(
                Offset::new(w * 0.5, h * 0.62),
                w * 0.36,
                Gradient::radial_fill().with_stops(&[
                    (0.0, alpha(VIOLET, 0.10 * f3)),
                    (1.0, alpha(VIOLET, 0.0)),
                ]),
            );
        });
        // The grid: rows and cols of projected 3D lines, fog-faded by depth.
        let extent = 13.0;
        let step = 1.3;
        let n = (extent * 2.0 / step) as usize;
        for i in 0..=n {
            let x = -extent + i as f32 * step;
            // Row along z.
            let a = cam.project(Vec3::new(x, 0.0, -extent), CANVAS);
            let b = cam.project(Vec3::new(x, 0.0, extent * 0.55), CANVAS);
            if let (Some(a), Some(b)) = (a, b) {
                book.line(a.0, b.0, alpha(mix(VIOLET, Color::BLACK, 0.45), floor_alpha * 0.4), 1.0);
            }
            // Column along x.
            let z = -extent + i as f32 * step;
            let a = cam.project(Vec3::new(-extent, 0.0, z), CANVAS);
            let b = cam.project(Vec3::new(extent, 0.0, z), CANVAS);
            if let (Some(a), Some(b)) = (a, b) {
                book.line(a.0, b.0, alpha(mix(VIOLET, Color::BLACK, 0.45), floor_alpha * 0.25), 1.0);
            }
        }
    }

    // ── F1/F2 · the session line arrives node by node ──────────────────────
    let f1 = f1_t(t);
    let line_fade = 1.0 - clamp01((f3 - 0.3) / 0.5); // fades as depth takes over
    if f1 > 0.0 && line_fade > 0.0 {
        let draw = ease_out_expo(f1);
        let x_span = LINE_X1 - LINE_X0;
        // The line.
        let mut p = Path::new();
        p.move_to(Offset::new(LINE_X0, LINE_Y));
        // Gentle life: a slight undulation, the session's own shape.
        for i in 0..=60 {
            let s = i as f32 / 60.0;
            let x = LINE_X0 + x_span * s;
            let y = LINE_Y + (s * std::f32::consts::TAU * 1.5).sin() * 3.0;
            p.line_to(Offset::new(x, y));
        }
        // Measure for the dash.
        let total = x_span * 1.05;
        let style = vieww_foundation::StrokeStyle::default()
            .dash(vieww_foundation::Dash::new(vec![total * draw, total + 1.0]));
        book.stroke_styled(p, alpha(VIOLET_SOFT, 0.55 * line_fade), 1.6, style);

        // The nodes, blooming as the front passes.
        for i in 0..LINE_NODES {
            let nf = i as f32 / (LINE_NODES - 1) as f32;
            let node_t = clamp01((draw - nf) / 0.02);
            if node_t <= 0.0 {
                continue;
            }
            let spring = spring_out(node_t, 12.0, 0.55);
            let x = LINE_X0 + x_span * nf;
            let y = LINE_Y + (nf * std::f32::consts::TAU * 1.5).sin() * 3.0;
            let node = Offset::new(x, y);
            book.ring(node, 5.0 + 13.0 * (1.0 - node_t), 1.4, alpha(VIOLET, (1.0 - node_t) * 0.5 * line_fade));
            book.circle(node, 3.6 + 0.8 * (spring - 1.0), alpha(VIOLET_SOFT, 0.95 * line_fade));
        }
    }

    // ── F2+ · the axis ──────────────────────────────────────────────────────
    let f2 = f2_t(t);
    if f2 > 0.0 {
        let (base, tip) = axis_geometry(t);
        let mut axis = Path::new();
        axis.move_to(base);
        axis.line_to(tip);
        let len = (tip.dx - base.dx).hypot(tip.dy - base.dy);
        let style = vieww_foundation::StrokeStyle::default()
            .dash(vieww_foundation::Dash::new(vec![len, len + 1.0]));
        book.stroke_styled(axis.clone(), alpha(INK, 0.9), 2.4, style);
        // The analytic glow along it.
        let outline = axis.stroke_outline(14.0);
        book.fill(outline, Gradient::vertical().with_stops(&[
            (0.0, alpha(VIOLET_SOFT, 0.0)),
            (0.5, alpha(VIOLET_SOFT, 0.14)),
            (1.0, alpha(VIOLET_SOFT, 0.0)),
        ]));
        // The head: bright, deliberate.
        book.layer(1.0, 9.0, None, |inner| {
            inner.circle(tip, 15.0, Gradient::radial_fill().with_stops(&[
                (0.0, alpha(Color::WHITE, 0.5)),
                (1.0, alpha(VIOLET_SOFT, 0.0)),
            ]));
        });
        book.circle(tip, 2.8, Color::WHITE);
    }

    // ── F4 · the unfold: three planes fan, glyphs bloom in sequence ────────
    if f4 > 0.0 {
        let cam = camera_at(t);
        for (pi, spec) in PLANES.iter().enumerate() {
            let fan = spring_out(clamp01((f4 - 0.10 - spec.stagger) / 0.45), 7.5, 0.72);
            if fan <= 0.0 {
                continue;
            }
            let (center, rot_y, tilt) = plane_pose(spec, fan);
            // The quad.
            let corners = [
                plane_world(center, rot_y, tilt, -0.5, -0.5),
                plane_world(center, rot_y, tilt, 0.5, -0.5),
                plane_world(center, rot_y, tilt, 0.5, 0.5),
                plane_world(center, rot_y, tilt, -0.5, 0.5),
            ];
            let proj: Vec<Option<Offset>> = corners.iter().map(|c| cam.project(*c, CANVAS).map(|x| x.0)).collect();
            if proj.iter().any(|p| p.is_none()) {
                continue;
            }
            let pts: Vec<Offset> = proj.into_iter().map(|p| p.unwrap()).collect();
            let mut quad = Path::new();
            quad.move_to(pts[0]);
            quad.line_to(pts[1]);
            quad.line_to(pts[2]);
            quad.line_to(pts[3]);
            quad.close();
            // Glass fill — the layer color, barely there, brighter at the
            // floor of the plane.
            book.fill(
                quad.clone(),
                Gradient::vertical().with_stops(&[
                    (0.0, alpha(spec.color, 0.02 * fan)),
                    (1.0, alpha(spec.color, 0.10 * fan)),
                ]),
            );
            // The border.
            book.stroke(quad, alpha(spec.color, 0.5 * fan), 1.1);
            // The ground shadow: the plane's light on the floor.
            if let Some((base_pt, _, base_scale)) =
                cam.project(Vec3::new(center.x, 0.0, center.z + 0.8), CANVAS)
            {
                book.layer(1.0, 14.0, None, |inner| {
                    inner.circle(
                        base_pt,
                        2.6 * base_scale * 900.0,
                        Gradient::radial_fill().with_stops(&[
                            (0.0, alpha(spec.color, 0.07 * fan)),
                            (1.0, alpha(spec.color, 0.0)),
                        ]),
                    );
                });
            }

            // The tree glyph, blooming level by level — in sequence.
            let tree = match pi {
                0 => tree_description(),
                1 => tree_identity(),
                _ => tree_geometry(),
            };
            let bloom = clamp01((f4 - 0.22 - spec.stagger) / 0.5);
            let world_nodes: Vec<Vec3> = tree
                .nodes
                .iter()
                .map(|&(u, v)| plane_world(center, rot_y, tilt, u, v))
                .collect();
            let proj_nodes: Vec<Option<Offset>> =
                world_nodes.iter().map(|n| cam.project(*n, CANVAS).map(|x| x.0)).collect();
            // Edges first — the structure arrives before the nodes.
            for &(a, b) in &tree.edges {
                if let (Some(pa), Some(pb)) = (proj_nodes[a], proj_nodes[b]) {
                    let edge_t = clamp01(bloom * 2.2 - a as f32 * 0.2);
                    if edge_t <= 0.0 {
                        continue;
                    }
                    book.line(pa, pb, alpha(spec.color, 0.58 * edge_t * fan), 1.4);
                }
            }
            // Nodes, level by level.
            for (ni, pn) in proj_nodes.iter().enumerate() {
                let level = tree.nodes[ni].1; // v is the level proxy
                let level_t = clamp01((bloom - (0.5 - level) * 0.55) / 0.22);
                if level_t <= 0.0 {
                    continue;
                }
                if let Some(p) = pn {
                    let spring = spring_out(level_t, 11.0, 0.6);
                    let r = 5.4 * (0.7 + 0.3 * spring);
                    if ni == 0 {
                        // Roots are brighter — the root of each tree.
                        book.circle(*p, r + 1.2, alpha(mix(spec.color, Color::WHITE, 0.35), 0.95 * fan));
                    } else {
                        book.circle(*p, r, alpha(spec.color, 0.85 * fan));
                    }
                }
            }
        }
    }

    // The vignette — tighter than usual: the reveal is an interior.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::radial(Offset::new(0.5, 0.48), 0.72).with_dither().with_stops(&[
            (0.55, alpha(Color::BLACK, 0.0)),
            (1.0, alpha(Color::BLACK, 0.5)),
        ]),
    );
}

// ── The widget layer: the counter, its ghosts, the labels ──────────────────

/// Scale about a center: translate + scale composed.
fn scale_about(cx: f32, cy: f32, s: f32) -> vieww_foundation::Transform {
    vieww_foundation::Transform::translate(Offset::new(cx * (1.0 - s), cy * (1.0 - s)))
        .then(vieww_foundation::Transform::scale(s, s))
}

pub fn frame(t: f32) -> WidgetNode {
    let f1 = f1_t(t);
    let f2 = f2_t(t);
    let f3 = f3_t(t);
    let f4 = f4_t(t);

    let mut overlays = Stack::new();

    // The glow behind the counter — its presence, painted.
    let glow = Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let mass_t = clamp01(f1 / 0.08);
            let w = size.width;
            let h = size.height;
            if mass_t > 0.0 && f3 < 1.0 {
                book.layer(1.0, 40.0, None, |inner| {
                    inner.circle(
                        Offset::new(BIG_X + 150.0, BIG_Y + 150.0),
                        240.0 * (1.0 - f3 * 0.5),
                        Gradient::radial_fill().with_stops(&[
                            (0.0, alpha(VIOLET, 0.14 * mass_t)),
                            (1.0, alpha(VIOLET, 0.0)),
                        ]),
                    );
                });
            }
            let _ = (w, h);
        }),
    );
    overlays = overlays.push(Positioned::fill().child(glow));

    // ── The counter itself: big → receding → small at the axis head ────────
    if f3 < 1.0 {
        // The big counter: present from F1, receding through F3.
        let recede = f3;
        let s = 1.0 - 0.42 * recede;
        let big_alpha = 1.0 - clamp01((recede - 0.55) / 0.45);
        if big_alpha > 0.01 {
            overlays = overlays.push(
                Positioned::new()
                    .left(BIG_X)
                    .top(BIG_Y)
                    .width(360.0)
                    .height(300.0)
                    .child(
                        Opacity::new(big_alpha).child(
                            Transformed::new(scale_about(BIG_X + 150.0, BIG_Y + 150.0, s)).child(
                                Text::new("7").style(
                                    TextStyle::new(BIG_SIZE)
                                        .weight(FontWeight::Medium)
                                        .color(INK),
                                ),
                            ),
                        ),
                    ),
            );
        }
    }
    if f3 > 0.35 {
        // The receded counter at the axis head.
        let small_alpha = clamp01((f3 - 0.35) / 0.35);
        overlays = overlays.push(
            Positioned::new()
                .left(SMALL_X)
                .top(SMALL_Y)
                .width(120.0)
                .height(110.0)
                .child(
                    Opacity::new(small_alpha).child(
                        Text::new("7").style(
                            TextStyle::new(SMALL_SIZE).weight(FontWeight::Medium).color(INK),
                        ),
                    ),
                ),
        );
    }

    // ── F2 · the ghosts part by pixels ─────────────────────────────────────
    if f2 > 0.0 && f2 < 1.0 {
        let part = spring_out(f2, 5.0, 0.85);
        let blur = 1.0 + 11.0 * f2;
        let ghost_alpha = 0.38 * (1.0 - clamp01((f2 - 0.75) / 0.25));
        for dir in [-1.0, 1.0] {
            let dx = dir * 210.0 * part;
            overlays = overlays.push(
                Positioned::new()
                    .left(BIG_X + dx)
                    .top(BIG_Y + 10.0 * dir * part)
                    .width(360.0)
                    .height(300.0)
                    .child(
                        Opacity::new(ghost_alpha).child(
                            Filtered::blur(blur).child(
                                Text::new("7").style(
                                    TextStyle::new(BIG_SIZE)
                                        .weight(FontWeight::Regular)
                                        .color(MUTED),
                                ),
                            ),
                        ),
                    ),
            );
        }
    }

    // ── F2's caption ────────────────────────────────────────────────────────
    if f2 > 0.5 && f3 < 0.6 {
        let cap_alpha = clamp01((f2 - 0.5) / 0.3) * (1.0 - clamp01((f3 - 0.25) / 0.35));
        if cap_alpha > 0.01 {
            overlays = overlays.push(
                Positioned::new()
                    .left(CANVAS_W - 500.0)
                    .top(300.0)
                    .width(440.0)
                    .height(26.0)
                    .child(
                        Opacity::new(cap_alpha).child(
                            Text::new("what you watched, from outside.").style(
                                TextStyle::new(15.0).weight(FontWeight::Medium).color(INK),
                            ),
                        ),
                    ),
            );
        }
    }

    // ── F4 · the plane labels, blooming in sequence ─────────────────────────
    if f4 > 0.25 {
        let cam = camera_at(t);
        for spec in PLANES.iter() {
            let appear = clamp01((f4 - 0.35 - spec.stagger) / 0.3);
            if appear <= 0.0 {
                continue;
            }
            // Anchor: the plane's top edge center.
            let top_center = plane_world(spec.end_center, spec.rot_y, spec.tilt, 0.0, -0.5);
            if let Some((p, _, _)) = cam.project(top_center, CANVAS) {
                overlays = overlays.push(
                    Positioned::new()
                        .left(p.dx - 90.0)
                        .top(p.dy - 30.0)
                        .width(180.0)
                        .height(20.0)
                        .child(
                            Opacity::new(appear).child(
                                Text::new(spec.label).style(
                                    TextStyle::new(13.5)
                                        .monospace()
                                        .letter_spacing(2.5)
                                        .color(spec.color),
                                ),
                            ),
                        ),
                );
            }
        }

        // F4's caption — the three words, together, bottom-center.
        let cap4 = clamp01((f4 - 0.55) / 0.3);
        if cap4 > 0.01 {
            overlays = overlays.push(
                Positioned::new()
                    .left(CANVAS_W * 0.5 - 260.0)
                    .top(680.0)
                    .width(520.0)
                    .height(22.0)
                    .child(
                        Opacity::new(cap4).child(
                            Text::new("description · identity · geometry")
                                .style(
                                    TextStyle::new(13.0)
                                        .monospace()
                                        .letter_spacing(2.0)
                                        .color(alpha(MUTED, 0.9)),
                                )
                                .align(TextAlign::Center),
                        ),
                    ),
            );
        }
    }

    let paint = Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            scene(book, t);
            let _ = size;
        }),
    );

    Stack::new()
        .push(Positioned::fill().child(paint))
        .push(overlays)
        .into()
}
