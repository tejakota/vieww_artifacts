//! exp_mesh — the escalation: *a single line becomes a world.*
//!
//! The user's brief, verbatim: "from a single line to a large 3D avatar".
//! This experiment walks the first half of that road, in four beats:
//!
//! - **A · the line** (t 0–0.22): one 3D polyline draws itself across the
//!   void — dash-phase animation, the E-17 session-line technique in embryo.
//! - **B · the grid** (t 0.18–0.42): the line multiplies into a wireframe
//!   floor, rows arriving staggered.
//! - **C · the skin** (t 0.38–0.62): quads fill in a diagonal sweep — first
//!   flat, then the per-face gradient ramps arrive.
//! - **D · the object** (t 0.58–1.0): a torus rises above the shaded floor
//!   and rotates; the camera orbits on a **real `SpringAnimation`** —
//!   retargeted *mid-flight* at t ≈ 0.78 to prove interruptibility in the
//!   film's own deterministic replay.
//!
//! Every 3D pixel is the framework's own raster: vertices are projected here,
//! faces are ordinary `Path` fills, shading is a two-stop gradient per face.

use std::time::Duration;

use vieww_animation::{SpringAnimation, SpringPreset, Ticker};
use vieww_foundation::{Color, Dash, Gradient, Offset, Path, Rect, Size, Sketchbook, StrokeStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, ease_out_expo, mix, CANVAS, CYAN, CYAN_SOFT, VIOLET,
    VIOLET_DEEP, VIOLET_SOFT,
};
use crate::three_d::{draw_mesh, grid_mesh, torus_mesh, Camera, MeshStyle, Vec3};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 6.0;

// ── The deterministic spring replay ─────────────────────────────────────────
//
// The film is a pure function of film-time, so the spring's state at time `t`
// is *reconstructed* by replaying it at 60 Hz from frame zero. Deterministic,
// interruptible, and honest — the spring does the motion, not a formula.

fn camera_angle_at(t: f32) -> f32 {
    let seconds = t * SECONDS;
    let mut spring = SpringAnimation::new(0.30, SpringPreset::Expressive);
    spring.retarget(5.10);
    let mut now = Duration::ZERO;
    let step = Duration::from_secs_f32(1.0 / 60.0);
    // Retarget mid-flight at 4.2s — the interruptibility beat.
    let retarget_at = Duration::from_secs_f32(4.2);
    while now < Duration::from_secs_f64(seconds as f64) {
        if now >= retarget_at && now - step < retarget_at {
            spring.retarget(4.30);
        }
        spring.tick(now);
        now += step;
    }
    spring.value()
}

// ── The scene painter ───────────────────────────────────────────────────────

fn scene(book: &mut Sketchbook, canvas: Size, t: f32) {
    let w = canvas.width;
    let h = canvas.height;

    // The void: a deep vertical ramp, plus a horizon glow that arrives with
    // the object.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::vertical()
            .with_dither()
            .with_stops(&[
                (0.0, Color::rgb(12, 10, 20)),
                (0.5, Color::rgb(7, 7, 12)),
                (1.0, Color::rgb(4, 4, 8)),
            ]),
    );

    // Sparse stars — the void is not empty, it is dark.
    {
        use crate::film_lib::Rng;
        let mut rng = Rng::new(0x3E5);
        for i in 0..70 {
            let x = rng.f01() * w;
            let y = rng.f01() * h * 0.7;
            let r = 0.5 + rng.f01() * 0.9;
            let tw = 0.5 + 0.5 * (t * 5.0 + i as f32 * 1.7).sin();
            book.circle(x_offset(x, y), r, alpha(Color::WHITE, 0.08 + 0.22 * tw));
        }
    }

    // Phase gates (gated with overlap so the cuts feel like growth).
    let line_t = clamp01(t / 0.22);
    let grid_t = clamp01((t - 0.18) / 0.24);
    let skin_t = clamp01((t - 0.38) / 0.24);
    let object_t = clamp01((t - 0.58) / 0.42);

    // The camera: orbit angle from the real spring, height + radius breathing.
    let angle = if t > 0.36 {
        camera_angle_at((t - 0.36) / 0.64)
    } else {
        0.30
    };
    let orbit_r = 13.0 + 4.0 * ease_out_expo(object_t);
    let eye_y = 4.2 + 2.4 * ease_in_out(object_t);
    let cam = Camera {
        eye: Vec3::new(angle.sin() * orbit_r, eye_y, angle.cos() * orbit_r),
        target: Vec3::new(0.0, 1.2, 0.0),
        fov: 0.9,
    };

    // ── A · the line: one diagonal in world space, drawn by dash phase ────
    let line_draw = ease_out_expo(line_t);
    if line_draw > 0.0 {
        let pts: Vec<Vec3> = (0..=40)
            .map(|i| {
                let s = i as f32 / 40.0;
                Vec3::new(-6.0 + 12.0 * s, 0.02, -6.0 + 6.0 * s)
            })
            .collect();
        // Dash-phase draw: only the first `frac` of the polyline's arc is on.
        stroke_dashed_polyline(book, &pts, &cam, canvas, line_draw, VIOLET_SOFT, 2.2);
    }

    // ── B · the grid: wireframe floor, rows staggered in ──────────────────
    if grid_t > 0.0 {
        let rows = 14;
        let cols = 14;
        let step = 1.0;
        let mut row_paths: Vec<Vec<Vec3>> = Vec::new();
        for r in 0..=rows {
            let x = -7.0 + r as f32 * step;
            let row: Vec<Vec3> = (0..=cols * 4)
                .map(|k| Vec3::new(x, 0.0, -7.0 + k as f32 * step * 0.25))
                .collect();
            row_paths.push(row);
        }
        for (r, row) in row_paths.iter().enumerate() {
            let stagger = clamp01((grid_t * rows as f32 * 1.35) - r as f32 * 0.9);
            if stagger <= 0.0 {
                continue;
            }
            stroke_dashed_polyline(
                book,
                row,
                &cam,
                canvas,
                ease_out_expo(stagger),
                alpha(VIOLET, 0.55),
                1.1,
            );
        }
        // Cross lines (the other axis), arriving behind the rows.
        for c in 0..=cols {
            let z = -7.0 + c as f32 * step;
            let cross: Vec<Vec3> = (0..=rows * 4)
                .map(|k| Vec3::new(-7.0 + k as f32 * step * 0.25, 0.0, z))
                .collect();
            let stagger = clamp01((grid_t * rows as f32 * 1.35) - (rows - c) as f32 * 0.9);
            if stagger <= 0.0 {
                continue;
            }
            stroke_dashed_polyline(
                book,
                &cross,
                &cam,
                canvas,
                ease_out_expo(stagger),
                alpha(VIOLET, 0.30),
                1.0,
            );
        }
    }

    // ── C · the skin: quads fill in a diagonal sweep ───────────────────────
    if skin_t > 0.0 {
        let mut floor = grid_mesh(14, 14, 1.0, mix(VIOLET_DEEP, Color::BLACK, 0.25));
        // Rebase the grid so it centers on the origin.
        for v in &mut floor.verts {
            v.x -= 7.0;
            v.z -= 7.0;
        }
        // The sweep: faces appear by (x + z), each fading in.
        let style = MeshStyle {
            flat: t < 0.50,
            edge_alpha: 0.04,
            specular: 0.25,
            ..MeshStyle::default()
        };
        draw_mesh_sweep(book, &floor, &cam, canvas, &style, skin_t);
    }

    // ── D · the object: the torus rises, rotates; horizon glow arrives ────
    if object_t > 0.0 {
        // Horizon glow behind everything.
        let pulse = 0.5 + 0.5 * (t * std::f32::consts::TAU * 0.8).sin();
        book.layer(1.0, 30.0, None, |inner| {
            inner.circle(
                Offset::new(w * 0.5, h * 0.55),
                w * 0.34,
                Gradient::radial_fill().with_stops(&[
                    (0.0, alpha(mix(VIOLET, CYAN, 0.3), 0.22 + 0.06 * pulse)),
                    (1.0, alpha(VIOLET, 0.0)),
                ]),
            );
        });

        // The grounding shadow: a dark ellipse under the object, scaled by
        // its height — contact, not float.
        let rise = ease_out_expo(object_t);
        if let Some((pt, _, scale)) = cam.project(Vec3::new(0.0, 0.0, 0.0), canvas) {
            let rx = (2.4 + rise * 0.9) * scale * 900.0;
            let ry = rx * 0.32;
            let mut p = Path::new();
            p.move_to(Offset::new(pt.dx - rx, pt.dy));
            p.cubic_to(
                Offset::new(pt.dx - rx, pt.dy - ry * 0.55),
                Offset::new(pt.dx - rx * 0.55, pt.dy - ry),
                Offset::new(pt.dx, pt.dy - ry),
            );
            p.cubic_to(
                Offset::new(pt.dx + rx * 0.55, pt.dy - ry),
                Offset::new(pt.dx + rx, pt.dy - ry * 0.55),
                Offset::new(pt.dx + rx, pt.dy),
            );
            p.cubic_to(
                Offset::new(pt.dx + rx, pt.dy + ry * 0.55),
                Offset::new(pt.dx + rx * 0.55, pt.dy + ry),
                Offset::new(pt.dx, pt.dy + ry),
            );
            p.cubic_to(
                Offset::new(pt.dx - rx * 0.55, pt.dy + ry),
                Offset::new(pt.dx - rx, pt.dy + ry * 0.55),
                Offset::new(pt.dx - rx, pt.dy),
            );
            p.close();
            book.layer(1.0, 10.0, None, |inner| {
                inner.fill(p, Gradient::radial_fill().with_stops(&[
                    (0.0, alpha(Color::BLACK, 0.55)),
                    (1.0, alpha(Color::BLACK, 0.0)),
                ]));
            });
        }
        let spin = t * 2.2;
        let mut torus = torus_mesh(2.6, 0.85, 36, 18, mix(VIOLET, Color::rgb(30, 20, 60), 0.35));
        let lift = 0.9 + rise * 2.2;
        for v in &mut torus.verts {
            let rotated = v.rot_y(spin);
            *v = rotated.add(Vec3::new(0.0, lift, 0.0));
        }
        let style = MeshStyle {
            edge_alpha: 0.05,
            edge_width: 1.0,
            specular: 0.85,
            shininess: 30.0,
            ..MeshStyle::default()
        };
        draw_mesh(book, &torus, &cam, canvas, &style);

        // A halo under the object: its light on the floor.
        book.layer(1.0, 16.0, None, |inner| {
            let floor_pt = Vec3::new(0.0, 0.02, 0.0);
            if let Some((pt, _, scale)) = cam.project(floor_pt, canvas) {
                let r = 2.6 * scale * 900.0;
                inner.circle(
                    pt,
                    r,
                    Gradient::radial_fill().with_stops(&[
                        (0.0, alpha(CYAN_SOFT, 0.22)),
                        (1.0, alpha(CYAN, 0.0)),
                    ]),
                );
            }
        });
    }

    // The vignette.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::radial(Offset::new(0.5, 0.5), 0.75)
            .with_dither()
            .with_stops(&[
                (0.60, alpha(Color::BLACK, 0.0)),
                (1.0, alpha(Color::BLACK, 0.5)),
            ]),
    );
}

/// Stroke a polyline, drawing only the first `frac` of its length —
/// dash-phase line-drawing, the session line's own technique: the pattern is
/// `[draw_len, off]` with `off` longer than the whole path, so the dash
/// travels along the stroke as `frac` grows.
fn x_offset(x: f32, y: f32) -> Offset {
    Offset::new(x, y)
}

fn stroke_dashed_polyline(
    book: &mut Sketchbook,
    pts: &[Vec3],
    cam: &Camera,
    canvas: Size,
    frac: f32,
    color: Color,
    width: f32,
) {
    if frac <= 0.0 || pts.len() < 2 {
        return;
    }
    // Project all points first, then measure cumulative length on screen.
    let projected: Vec<Option<Offset>> = pts
        .iter()
        .map(|p| cam.project(*p, canvas).map(|(pt, _, _)| pt))
        .collect();
    let mut path = Path::new();
    let mut started = false;
    for w in projected.windows(2) {
        if let (Some(a), Some(b)) = (w[0], w[1]) {
            if !started {
                path.move_to(a);
                started = true;
            }
            path.line_to(b);
        } else if started {
            // A break in projection: end this sub-path, start a new one later.
            started = false;
        }
    }
    if !started {
        return;
    }

    // Total length on screen, measured from the same projected points.
    let mut total = 0.0;
    for w in projected.windows(2) {
        if let (Some(a), Some(b)) = (w[0], w[1]) {
            total += (b.dx - a.dx).hypot(b.dy - a.dy);
        }
    }
    if total <= 1e-6 {
        return;
    }
    let draw_len = total * frac.clamp(0.0, 1.0);
    let dash = Dash::new(vec![draw_len, total + 1.0]);
    let style = StrokeStyle::default().dash(dash);
    book.stroke_styled(path, color, width, style);
}

/// A sweep-revealed mesh: each quad appears when the sweep wave crosses its
/// center, fading in over a short window. Same painter's sort and shading as
/// `draw_mesh`, with the per-face reveal factor.
fn draw_mesh_sweep(
    book: &mut Sketchbook,
    mesh: &crate::three_d::Mesh,
    cam: &Camera,
    canvas: Size,
    style: &MeshStyle,
    sweep_t: f32,
) {
    let mut mesh = mesh.clone();
    // The sweep coordinate: normalized (x + z) in world space.
    let min_c = mesh
        .quads
        .iter()
        .map(|q| {
            let c = mesh.quad_center(q);
            c.x + c.z
        })
        .fold(f32::INFINITY, f32::min);
    let max_c = mesh
        .quads
        .iter()
        .map(|q| {
            let c = mesh.quad_center(q);
            c.x + c.z
        })
        .fold(f32::NEG_INFINITY, f32::max);
    let span = (max_c - min_c).max(1e-6);

    for (qi, q) in mesh.quads.iter().enumerate() {
        let c = mesh.quad_center(q);
        let coord = (c.x + c.z - min_c) / span;
        let appear = clamp01((sweep_t * 1.3 - coord) / 0.18);
        if appear <= 0.0 {
            mesh.colors[qi] = alpha(mesh.colors[qi], 0.0);
        } else {
            mesh.colors[qi] = alpha(mesh.colors[qi], appear);
        }
    }
    draw_mesh(book, &mesh, cam, canvas, style);
}

/// The frame: just the painter — this experiment is all paint.
pub fn frame(t: f32) -> WidgetNode {
    Stack::new()
        .push(Positioned::fill().child(
            Painting::sized(
                CANVAS,
                PaintWith::new(move |book: &mut Sketchbook, size: Size| {
                    scene(book, size, t);
                }),
            ),
        ))
        .into()
}
