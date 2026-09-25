//! exp_dolly — *the vertigo.* The U-04 projection spine, load-bearing.
//!
//! A dolly zoom, the one camera move that cannot be faked in 2D: the camera
//! retreats while the field of view narrows in exact compensation, and the
//! subject — a floating torus with a rotating monolith at its heart — holds
//! a *constant projected size* while the world behind it stretches to
//! infinity. The receipt measures the subject's on-screen height every
//! frame through the same `Camera::project` that drew it: the number that
//! must not move, printed next to the one that must (the floor-edge stretch
//! factor, measured the same way).
//!
//! The camera's eye orbits slowly at its current distance — a heading that
//! stays a heading (U-07's warning, honored: the target is fixed, the eye
//! moves, and the two are never conflated). Fog ranges ride the camera
//! distance so the retreat reads as depth rather than as fade-out.
//!
//! Three planes of receipt: the vertigo math itself, the painter's census
//! (quads in the scene), and the dust — 260 world-space motes whose
//! parallax is the retreat, drawn through the same projection.

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{alpha, clamp01, ease_in_out, mix, tint, BG_DEEP, CANVAS, FAINT, INK, MUTED,
    Rng, VIOLET, VIOLET_DEEP, VIOLET_SOFT};
use crate::three_d::{box_mesh, displace_y, draw_dot3, draw_mesh, grid_mesh, horizon, torus_mesh,
    Camera, Mesh, MeshStyle, Vec3};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 10.0;

/// The subject: a torus at this height, this size.
const SUBJECT: Vec3 = Vec3::new(0.0, 3.4, 0.0);
const TORUS_BIG: f32 = 3.3;
const TORUS_SMALL: f32 = 1.1;

/// The retreat: from this distance to this distance.
const D0: f32 = 11.0;
const D1: f32 = 36.0;

/// The compensation constant: `k = D0 · tan(fov₀/2)`, so `fov(t)` keeps the
/// subject's angular size pinned while `dist(t)` grows. (A fn, not a const:
/// `tan` is not a const fn.)
fn k_comp() -> f32 {
    const FOV0: f32 = 60.0_f32.to_radians();
    D0 * (FOV0 * 0.5).tan()
}

// ── The world — built once, deterministic ───────────────────────────────────

/// The dune floor: a displaced grid.
fn floor() -> Mesh {
    let mut m = grid_mesh(36, 36, 1.15, Color::rgb(24, 22, 34));
    displace_y(&mut m, |p| {
        0.65 * (p.x * 0.33).sin() * (p.z * 0.29).cos() + 0.35 * (p.z * 0.11).sin()
    });
    m
}

/// The pylons: a ring of tall boxes at the perimeter, no two alike.
fn pylons() -> Mesh {
    let mut rng = Rng::new(0xD011);
    let mut m = Mesh::new();
    for i in 0..9 {
        let a = i as f32 / 9.0 * std::f32::consts::TAU + rng.sym() * 0.12;
        let r = 15.0 + rng.f01() * 6.0;
        let h = 2.5 + rng.f01() * 6.5;
        let center = Vec3::new(a.cos() * r, h * 0.5, a.sin() * r);
        let half = Vec3::new(0.45 + rng.f01() * 0.5, h, 0.45 + rng.f01() * 0.5);
        let shade = 0.7 + rng.f01() * 0.4;
        m = m_join(m, box_mesh(
            center,
            half,
            mix(VIOLET_DEEP, Color::rgb(38, 33, 56), shade),
        ));
    }
    m
}

/// The torus, floating at the subject point.
fn torus() -> Mesh {
    torus_mesh(TORUS_BIG, TORUS_SMALL, 26, 13, Color::rgb(150, 120, 214))
}

/// The rotating monolith at the heart.
fn monolith(t: f32) -> Mesh {
    let mut m = box_mesh(
        SUBJECT,
        Vec3::new(0.55, 2.3, 0.55),
        Color::rgb(236, 232, 248),
    );
    let a = t * SECONDS * 0.55;
    for v in &mut m.verts {
        let rel = v.sub(SUBJECT).rot_y(a);
        *v = SUBJECT.add(rel);
    }
    m
}

/// `Mesh` has no `extend` — quads and verts concatenated here, indices offset.
fn m_join(mut a: Mesh, b: Mesh) -> Mesh {
    let off = a.verts.len();
    a.verts.extend(b.verts);
    for q in b.quads {
        a.quads.push([q[0] + off, q[1] + off, q[2] + off, q[3] + off]);
    }
    a.colors.extend(b.colors);
    a
}

/// World-space dust: a deterministically scattered shell, drawn per frame.
fn dust() -> Vec<Vec3> {
    let mut rng = Rng::new(0xD057);
    (0..260)
        .map(|_| {
            let a = rng.f01() * std::f32::consts::TAU;
            let r = 6.0 + rng.f01() * 64.0;
            let y = rng.f01() * 22.0;
            Vec3::new(a.cos() * r, y, a.sin() * r)
        })
        .collect()
}

// ── The camera — the vertigo itself ─────────────────────────────────────────

/// The retreat distance at `t`: a hold, the push, a far hold with drift.
#[must_use]
fn dist_at(t: f32) -> f32 {
    let push = ease_in_out(clamp01((t - 0.12) / 0.68));
    D0 + (D1 - D0) * push
}

/// The FOV that pins the subject: `2·atan(k/dist)`.
#[must_use]
pub fn fov_at(t: f32) -> f32 {
    2.0 * (k_comp() / dist_at(t)).atan()
}

/// The eye: fixed elevation over the target, slow orbit, current distance.
#[must_use]
fn eye_at(t: f32) -> Vec3 {
    let d = dist_at(t);
    let yaw = 0.18 + t * 0.50;
    Vec3::new(
        SUBJECT.x + d * yaw.cos(),
        SUBJECT.y + d * 0.30,
        SUBJECT.z + d * yaw.sin(),
    )
}

/// The subject's projected height on the canvas — the vertigo's invariant,
/// measured through the camera that will draw it.
#[must_use]
fn subject_px(cam: &Camera) -> f32 {
    let top = cam.project(SUBJECT.add(Vec3::new(0.0, TORUS_SMALL, 0.0)), CANVAS);
    let bottom = cam.project(SUBJECT.add(Vec3::new(0.0, -TORUS_SMALL, 0.0)), CANVAS);
    match (top, bottom) {
        (Some((a, _, _)), Some((b, _, _))) => (a.dy - b.dy).abs(),
        _ => 0.0,
    }
}

/// The floor edge's projected span — the vertigo's variable, same measure.
#[must_use]
fn floor_span_px(cam: &Camera) -> f32 {
    let e = 20.7; // the grid's half-extent
    let l = cam.project(Vec3::new(-e, 0.0, 0.0), CANVAS);
    let r = cam.project(Vec3::new(e, 0.0, 0.0), CANVAS);
    match (l, r) {
        (Some((a, _, _)), Some((b, _, _))) => (a.dx - b.dx).abs(),
        _ => 0.0,
    }
}

// ── The board ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let cam = Camera {
        eye: eye_at(t),
        target: SUBJECT,
        fov: fov_at(t),
    };

    let floor = floor();
    let pylons = pylons();
    let torus = torus();
    let mono = monolith(t);
    let dust = dust();
    let quads = floor.quads.len() + pylons.quads.len() + torus.quads.len() + mono.quads.len();

    let board = Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;
            let d = dist_at(t);

            // The sky.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(6, 6, 10)),
                    (0.5, BG_DEEP),
                    (1.0, Color::rgb(10, 9, 14)),
                ]),
            );
            // The horizon breathes behind everything.
            horizon(book, CANVAS, VIOLET_DEEP);

            // Fog rides the camera distance — the retreat is depth, not fade.
            let mut style = MeshStyle {
                specular: 0.45,
                shininess: 30.0,
                fog_start: d * 0.55,
                fog_end: d * 3.1,
                ..MeshStyle::default()
            };

            // The floor and the pylons.
            draw_mesh(book, &floor, &cam, CANVAS, &style);
            style.edge_alpha = 0.10;
            draw_mesh(book, &pylons, &cam, CANVAS, &style);

            // The dust — parallax is the retreat.
            for p in &dust {
                let a = 0.16 + 0.28 * (1.0 - clamp01((p.y - 4.0) / 18.0));
                draw_dot3(book, *p, 0.05, &cam, CANVAS, alpha(tint(VIOLET_SOFT, 0.45), a));
            }

            // The Plus glow pinned behind the subject — constant, because
            // the subject is: the vertigo's tell.
            if let Some((c, _, _)) = cam.project(SUBJECT, CANVAS) {
                book.blended_layer(1.0, 14.0, vieww_foundation::BlendMode::Plus, None, |g| {
                    g.circle(
                        c,
                        128.0,
                        Gradient::radial_fill().with_dither().with_stops(&[
                            (0.0, alpha(tint(Color::WHITE, 0.2), 0.46)),
                            (0.5, alpha(VIOLET_SOFT, 0.22)),
                            (1.0, alpha(VIOLET, 0.0)),
                        ]),
                    );
                });
            }

            // The subject: monolith inside the torus, glinting.
            style.edge_alpha = 0.18;
            style.specular = 0.55;
            draw_mesh(book, &mono, &cam, CANVAS, &style);
            style.specular = 0.40;
            draw_mesh(book, &torus, &cam, CANVAS, &style);

            // The vignette.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.5), 0.92).with_dither().with_stops(&[
                    (0.6, alpha(Color::BLACK, 0.0)),
                    (1.0, alpha(Color::BLACK, 0.45)),
                ]),
            );
        }),
    );

    Stack::new()
        .push(Positioned::fill().child(board))
        .push(receipt_panel(t, quads))
        .into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(t: f32, quads: usize) -> WidgetNode {
    // Measured through the same camera that draws the frame.
    let cam = Camera {
        eye: eye_at(t),
        target: SUBJECT,
        fov: fov_at(t),
    };
    let base = Camera {
        eye: eye_at(0.0),
        target: SUBJECT,
        fov: fov_at(0.0),
    };
    let px_now = subject_px(&cam);
    let px0 = subject_px(&base);
    let stretch = floor_span_px(&cam) / floor_span_px(&base).max(1.0);

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;
    const P_W: f32 = 356.0;

    let lines = [
        "DOLLY · THE VERTIGO · SUBJECT PINNED (U-04)".to_string(),
        format!("dist {:.1} → {:.1} · fov {:.1}° → {:.1}°", dist_at(0.0), dist_at(t),
            fov_at(0.0) * 360.0 / std::f32::consts::TAU, fov_at(t) * 360.0 / std::f32::consts::TAU),
        format!("subject {:.1} px (t₀ {:.1}) · drift {:+.1} px", px_now, px0, px_now - px0),
        format!("floor stretch ×{:.2} — measured, same projection", stretch),
        format!("quads {} · dust 260 · push {:.0}%", quads,
            clamp01((t - 0.12) / 0.68) * 100.0),
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
                        .letter_spacing(1.8)
                        .color(alpha(MUTED, 1.0)),
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
                        TextStyle::new(11.0).monospace().color(alpha(mix(MUTED, INK, 0.4), 0.95)),
                    ),
                ),
        );
    }

    // The instrument: top-down plan — the eye retreating, the FOV wedge
    // narrowing, the subject's angular size visibly constant.
    let plan = Painting::sized(
        Size::new(P_W, 100.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            book.rrect(
                Rect::new(0.0, 0.0, P_W, 100.0),
                10.0,
                alpha(Color::rgb(16, 16, 21), 0.88),
            );
            book.stroke_rrect(
                Rect::new(0.0, 0.0, P_W, 100.0),
                10.0,
                alpha(Color::WHITE, 0.08),
                1.0,
            );

            // Subject at center; eye on the left, retreating left.
            let subj = Offset::new(P_W - 74.0, 50.0);
            let to_plan = |v: Vec3| -> Offset {
                // plan x: eye-line, 0 at subject, negative toward the eye.
                let eye = eye_at(t);
                let fwd = eye.sub(SUBJECT);
                let rel = v.sub(SUBJECT);
                // project onto (fwd, right) basis, scaled
                let f = fwd.norm();
                let right = Vec3::new(-f.z, 0.0, f.x).norm();
                let df = rel.dot(f);
                let dr = rel.dot(right);
                Offset::new(
                    subj.dx + df / 46.0,
                    (subj.dy - dr / 46.0).clamp(12.0, 88.0),
                )
            };

            // The floor grid, in plan.
            let mut grid = vieww_foundation::Path::new();
            for i in 0..=8 {
                let a = -20.7 + i as f32 * 41.4 / 8.0;
                let mut p1 = vieww_foundation::Path::new();
                p1.move_to(to_plan(Vec3::new(a, 0.0, -20.7)));
                p1.line_to(to_plan(Vec3::new(a, 0.0, 20.7)));
                grid.extend(&p1);
                let mut p2 = vieww_foundation::Path::new();
                p2.move_to(to_plan(Vec3::new(-20.7, 0.0, a)));
                p2.line_to(to_plan(Vec3::new(20.7, 0.0, a)));
                grid.extend(&p2);
            }
            book.stroke(grid, alpha(FAINT, 0.16), 1.0);

            // The eye and its FOV wedge.
            let eye = eye_at(t);
            let plan_eye = to_plan(eye);
            let half = fov_at(t) * 0.5;
            let fwd = SUBJECT.sub(eye).norm();
            for s in [-1.0, 1.0] {
                let dir = fwd.rot_y(s * half);
                let far = to_plan(eye.add(dir.scale(46.0 * 3.0)));
                book.stroke(
                    {
                        let mut p = vieww_foundation::Path::new();
                        p.move_to(plan_eye);
                        p.line_to(far);
                        p
                    },
                    alpha(VIOLET_SOFT, 0.55),
                    1.0,
                );
            }
            book.circle(plan_eye, 3.4, Color::WHITE);
            book.circle(subj, 4.6, alpha(VIOLET_SOFT, 0.9));
            book.ring(subj, 7.5, 1.0, alpha(VIOLET_SOFT, 0.4));
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 116.0)
            .width(P_W)
            .height(100.0)
            .child(plan),
    );

    stack.into()
}
