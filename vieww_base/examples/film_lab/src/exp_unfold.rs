//! exp_unfold — *P-01: perspective is a shape, not a widget property.*
//! The U-04 plate.
//!
//! Three frosted planes fan out from a shared spine and unfold from
//! edge-on to their laid-back perspective angles — the E-20 F4 "three
//! planes, tree glyphs" beat, at lab scale. Every quad is
//! [`Transform3::project_rect`]: a rotation composed in front of a
//! [`perspective`](vieww_foundation::Transform3::perspective), producing an
//! ordinary [`Path`] that fills, strokes, clips and blurs like any other.
//! Before U-04 this was forty lines of hand-rolled projection in an
//! example; now the example is the forty lines of *content*.
//!
//! The tree glyphs ride the same transform — each glyph is a small shape in
//! the plane's own 2D space, projected pointwise through the plane's
//! `Transform3`, so foreshortening is automatic and honest. The floor grid
//! is 3D points projected two at a time (a straight 3D line projects to a
//! straight 2D line — the pinhole guarantees it).
//!
//! The receipt counts what the transform actually did: planes projected,
//! glyph quads, grid lines, and **behind-camera drops** — `project` returns
//! `None` at the camera plane, and the drops are counted, not swallowed
//! (U-16's discipline at the boundary).

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle,
    Transform3};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, mix, spring_out, tint, BG_DEEP, CANVAS, FAINT, INK, MUTED, VIOLET,
    VIOLET_DEEP, VIOLET_SOFT, MINT,
};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 10.0;

/// The lens — 960 px focal, the gentle end of the photographic range.
const FOCAL: f32 = 960.0;

/// The spine — the fold line the planes hinge on.
const SPINE_Y: f32 = 316.0;

/// The three planes, at rest (fully unfolded): center, tilt, fan offset.
#[derive(Clone)]
struct Plane {
    /// Rest rect in plane-local space (the plane's own 2D coordinates, laid
    /// flat before the fold).
    rest: Rect,
    /// Rest tilt from flat, radians — how far back the plane leans.
    tilt_rest: f32,
    /// Unfold stagger, fraction of the span.
    stagger: f32,
}

fn planes() -> Vec<Plane> {
    vec![
        Plane {
            rest: Rect::new(-250.0, -210.0, 250.0, 10.0),
            tilt_rest: -0.42,
            stagger: 0.0,
        },
        Plane {
            rest: Rect::new(-210.0, -180.0, 210.0, 10.0),
            tilt_rest: -0.58,
            stagger: 0.10,
        },
        Plane {
            rest: Rect::new(-170.0, -150.0, 170.0, 10.0),
            tilt_rest: -0.74,
            stagger: 0.20,
        },
    ]
}

/// The plane's transform at film-t: edge-on at t0, resting tilt at t1,
/// on a spring. `None` parts of the fold are the plane's own business.
fn plane_transform(t: f32, p: &Plane) -> Transform3 {
    // Unfold progress on a soft spring — no fighting the settle.
    let tau = clamp01((t - 0.10 - p.stagger * 0.30) / 0.45);
    let s = spring_out(tau, 5.6, 0.78);

    // From edge-on (nearly -π/2: the plane stands up out of the frame) to
    // the rest tilt. Slight per-plane sway after settling.
    let sway = (t * 3.0).sin() * 0.012;
    let tilt = p.tilt_rest + (1.0 - s) * (-std::f32::consts::FRAC_PI_2 + 0.05 - p.tilt_rest) + sway;

    // Hinge on the spine: fold about the X axis at SPINE_Y, then perspective.
    Transform3::translation(0.0, -SPINE_Y, 0.0)
        .then(Transform3::rotation_x(tilt))
        .then(Transform3::translation(0.0, SPINE_Y, 0.0))
        .then(Transform3::perspective(FOCAL))
}

/// A tree glyph: a small stacked-triangle tree, in plane-local coords.
/// Returns its triangles as point triples.
fn tree(x: f32, y: f32, scale: f32, seed: f32) -> Vec<[(f32, f32); 3]> {
    let sway = (seed * 6.0).sin() * 2.0 * scale;
    let mut tris = Vec::with_capacity(4);
    // Three canopy tiers + a trunk quad as two triangles.
    let tiers = [
        (0.0, -34.0, 13.0),
        (0.0, -24.0, 16.0),
        (0.0, -13.0, 19.0),
    ];
    for (k, (dx, dy, half)) in tiers.iter().enumerate() {
        let lift = k as f32 * 1.2 * scale;
        tris.push([
            (x + dx * scale + sway * (1.0 - k as f32 * 0.3), y + dy * scale - lift),
            (x + (dx - half) * scale, y + (dy + 16.0) * scale - lift),
            (x + (dx + half) * scale, y + (dy + 16.0) * scale - lift),
        ]);
    }
    // Trunk.
    tris.push([
        (x - 2.5 * scale, y + 2.0),
        (x + 2.5 * scale, y + 2.0),
        (x + 2.5 * scale, y - 12.0 * scale),
    ]);
    tris.push([
        (x - 2.5 * scale, y + 2.0),
        (x + 2.5 * scale, y - 12.0 * scale),
        (x - 2.5 * scale, y - 12.0 * scale),
    ]);
    tris
}

// ── The board ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let ps = planes();

    // Receipt counts, shared between painter and panel: (planes, glyphs,
    // grid lines, behind-camera drops).
    let counts = std::rc::Rc::new(std::cell::RefCell::new((0usize, 0usize, 0usize, 0usize)));

    let board = Painting::sized(
        CANVAS,
        PaintWith::new({
            let counts = counts.clone();
            let ps = ps.clone();
            move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(8, 8, 12)),
                    (0.55, BG_DEEP),
                    (1.0, Color::rgb(11, 11, 16)),
                ]),
            );

            // Stars.
            let mut rng = crate::film_lib::Rng::new(0x815);
            for _ in 0..60 {
                let x = rng.f01() * w;
                let y = rng.f01() * h;
                book.circle(Offset::new(x, y), 0.4 + rng.f01() * 0.7, alpha(Color::WHITE, 0.035));
            }

            // The fold glow at the spine — where the planes hinge.
            book.layer(1.0, 22.0, None, |g| {
                g.circle(
                    Offset::new(w * 0.5, SPINE_Y),
                    360.0,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.13)),
                        (1.0, alpha(VIOLET, 0.0)),
                    ]),
                );
            });

            // The spine line.
            book.line(
                Offset::new(90.0, SPINE_Y),
                Offset::new(w - 90.0, SPINE_Y),
                alpha(FAINT, 0.22),
                1.0,
            );

            // ── The floor grid: 3D lines projected two points at a time.
            // A floor point is (x, FLOOR_Y) at depth z; the horizon fold is
            // matrix composition — translate the horizon to 0, perspective,
            // translate back — so z=0 projects to the floor line and
            // z→∞ converges on the horizon.
            const FLOOR_Y: f32 = 660.0;
            const HORIZON: f32 = 430.0;
            let grid_t = Transform3::translation(0.0, -HORIZON, 0.0)
                .then(Transform3::perspective(FOCAL))
                .then(Transform3::translation(0.0, HORIZON, 0.0));
            let fade = clamp01(t / 0.14);
            let mut grid_lines = 0usize;
            let mut drops = 0usize;
            for zi in 0..9 {
                let z = 40.0 + zi as f32 * 90.0;
                for xi in -6..=6 {
                    let x = xi as f32 * 150.0;
                    let a = grid_t.project(Offset::new(x, FLOOR_Y), z);
                    let b = grid_t.project(Offset::new(x, FLOOR_Y), z + 90.0);
                    match (a, b) {
                        (Some(a), Some(b)) => {
                            let deep = 1.0 - z / 850.0;
                            book.line(a, b, alpha(VIOLET, 0.10 * fade * deep), 1.0);
                            grid_lines += 1;
                        }
                        _ => drops += 1,
                    }
                }
                // One horizontal per depth, for the weave.
                if let (Some(a), Some(b)) = (
                    grid_t.project(Offset::new(-900.0, FLOOR_Y), z),
                    grid_t.project(Offset::new(900.0, FLOOR_Y), z),
                ) {
                    let deep = 1.0 - z / 850.0;
                    book.line(a, b, alpha(FAINT, 0.07 * fade * deep), 1.0);
                    grid_lines += 1;
                } else {
                    drops += 1;
                }
            }

            // ── The planes: farthest first — fan order by tilt ──
            let mut planes_projected = 0usize;
            let mut glyph_quads = 0usize;
            for p in ps.iter().rev() {
                let xf = plane_transform(t, p);
                let quad = match xf.project_rect(p.rest.translate(Offset::new(w * 0.5, 0.0))) {
                    Some(q) => q,
                    None => {
                        drops += 1;
                        continue;
                    }
                };
                planes_projected += 1;

                // The frosted body — the projected quad, filled.
                let settle = clamp01((t - 0.10 - p.stagger * 0.30) / 0.45);
                book.layer(settle, 0.0, None, |g| {
                    g.fill(
                        quad.clone(),
                        Gradient::vertical().with_dither().with_stops(&[
                            (0.0, alpha(mix(VIOLET_DEEP, BG_DEEP, 0.25), 0.92)),
                            (0.6, alpha(mix(VIOLET_DEEP, BG_DEEP, 0.55), 0.78)),
                            (1.0, alpha(mix(VIOLET_DEEP, BG_DEEP, 0.35), 0.85)),
                        ]),
                    );
                });
                // The lit edge — the fold line catches light.
                book.stroke(quad.clone(), alpha(tint(VIOLET_SOFT, 0.45), 0.75 * settle), 1.6);

                // Tree glyphs, projected pointwise through the same xf.
                let mut rng = crate::film_lib::Rng::new(0x816 + p.rest.left as u64);
                for g_i in 0..7 {
                    let gx = p.rest.left + 30.0 + rng.f01() * (p.rest.width() - 60.0);
                    let gy = p.rest.top + 40.0 + rng.f01() * (p.rest.height() - 80.0);
                    let scale = 0.7 + rng.f01() * 0.7;
                    let seed = rng.f01() * 10.0;
                    for tri in tree(gx, gy, scale, seed) {
                        let a = xf.project(Offset::new(tri[0].0 + w * 0.5, tri[0].1), 0.0);
                        let b = xf.project(Offset::new(tri[1].0 + w * 0.5, tri[1].1), 0.0);
                        let c = xf.project(Offset::new(tri[2].0 + w * 0.5, tri[2].1), 0.0);
                        if let (Some(a), Some(b), Some(c)) = (a, b, c) {
                            let mut path = Path::new();
                            path.move_to(a).line_to(b).line_to(c).close();
                            let depth_hint = 0.35 + 0.4 * rng.f01();
                            book.fill(
                                path,
                                alpha(mix(MINT, VIOLET_SOFT, 0.45), depth_hint * 0.8 * settle),
                            );
                            glyph_quads += 1;
                        } else {
                            drops += 1;
                        }
                    }
                }
            }

            *counts.borrow_mut() = (planes_projected, glyph_quads, grid_lines, drops);

            // The vignette.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.45), 0.9).with_dither().with_stops(&[
                    (0.55, alpha(Color::BLACK, 0.0)),
                    (1.0, alpha(Color::BLACK, 0.42)),
                ]),
            );
        }}),
    );

    let (planes_n, glyphs_n, grid_n, drops_n) = *counts.borrow();

    Stack::new()
        .push(Positioned::fill().child(board))
        .push(receipt_panel(t, planes_n, glyphs_n, grid_n, drops_n))
        .into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(t: f32, planes_n: usize, glyphs_n: usize, grid_n: usize, drops_n: usize) -> WidgetNode {
    const P_X: f32 = 872.0;
    const P_Y: f32 = 44.0;
    const P_W: f32 = 348.0;

    let tilts: Vec<f32> = planes().iter().map(|p| p.tilt_rest).collect();

    let lines = [
        "P-01 · UNFOLD · TRANSFORM3 (U-04)".to_string(),
        format!("planes {}/3 projected · glyphs {} quads · grid {} lines", planes_n, glyphs_n, grid_n),
        format!("behind-camera drops {} — counted, not swallowed", drops_n),
        format!("focal {:.0}px · tilts {:.2}/{:.2}/{:.2} rad",
            FOCAL, tilts[0], tilts[1], tilts[2]),
        format!("unfold {:.0}%", clamp01((t - 0.10) / 0.45) * 100.0),
    ];

    let mut stack = Stack::new().push(
        Positioned::new()
            .left(P_X)
            .top(P_Y)
            .width(P_W)
            .height(18.0)
            .child(
                Text::new(lines[0].clone()).style(
                    TextStyle::new(12.0)
                        .monospace()
                        .letter_spacing(2.0)
                        .color(alpha(FAINT, 0.95)),
                ),
            ),
    );
    for (i, line) in lines.iter().enumerate().skip(1) {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + 18.0 + i as f32 * 16.0)
                .width(P_W)
                .height(15.0)
                .child(
                    Text::new(line.clone()).style(
                        TextStyle::new(11.0)
                            .monospace()
                            .color(alpha(MUTED, 0.9)),
                    ),
                ),
        );
    }

    // The instrument: a side elevation of the fold — each plane's tilt as a
    // ray from a shared spine dot, the spring progress as a filled wedge.
    let elev = Painting::sized(
        Size::new(P_W, 92.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            book.rrect(
                Rect::new(0.0, 0.0, P_W, 92.0),
                10.0,
                alpha(Color::rgb(16, 16, 21), 0.88),
            );
            book.stroke_rrect(
                Rect::new(0.0, 0.0, P_W, 92.0),
                10.0,
                alpha(Color::WHITE, 0.08),
                1.0,
            );
            // The spine point, left-center; each plane a ray at its live tilt.
            let spine = Offset::new(36.0, 46.0);
            let len = 250.0f32;
            for (i, p) in planes().iter().enumerate() {
                let tau = clamp01((t - 0.10 - p.stagger * 0.30) / 0.45);
                let s = spring_out(tau, 5.6, 0.78);
                let tilt = p.tilt_rest
                    + (1.0 - s) * (-std::f32::consts::FRAC_PI_2 + 0.05 - p.tilt_rest);
                // Screen-drawing angle: tilt from vertical-down.
                let draw = std::f32::consts::FRAC_PI_2 + tilt;
                let tip = Offset::new(
                    spine.dx + draw.cos() * len,
                    spine.dy + draw.sin() * len,
                );
                let mut ray = Path::new();
                ray.move_to(spine);
                ray.line_to(tip);
                book.stroke(
                    ray,
                    alpha([VIOLET_SOFT, VIOLET, tint(VIOLET, 0.4)][i], 0.5 + 0.4 * s),
                    2.0,
                );
                // The rest-tilt ghost — where this plane settles.
                let draw_rest = std::f32::consts::FRAC_PI_2 + p.tilt_rest;
                let rest_tip = Offset::new(
                    spine.dx + draw_rest.cos() * len * 0.9,
                    spine.dy + draw_rest.sin() * len * 0.9,
                );
                let mut ghost = Path::new();
                ghost.move_to(spine);
                ghost.line_to(rest_tip);
                book.stroke(ghost, alpha(FAINT, 0.22), 1.0);
            }
            // The spine.
            book.circle(spine, 4.0, alpha(Color::WHITE, 0.9));
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 112.0)
            .width(P_W)
            .height(92.0)
            .child(elev),
    );

    stack.into()
}
