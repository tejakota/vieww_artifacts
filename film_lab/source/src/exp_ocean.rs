//! exp_ocean — *from a drop to an entire ocean.*
//!
//! The user's brief, verbatim. Four beats:
//!
//! - **A · the fall** (t 0–0.10): one luminous dot falls from above.
//! - **B · the strike** (t 0.10–0.28): the dot lands — squashes — and ripple
//!   rings expand outward, fading as they grow. Second and third rings
//!   follow, staggered.
//! - **C · the field** (t 0.28–0.50): a field of ripples at deterministic
//!   positions — the surface has become alive.
//! - **D · the ocean** (t 0.48–1.0): the ripples resolve into a true 3D
//!   heightfield — sum of three travelling sine waves, per-face gradient
//!   shading, depth fog, and glints at the crests. The ocean *grows out of
//!   the landing point*: quads appear inside an expanding radius, so the
//!   last ripple is the ocean's first wave. The camera drifts forward.
//!
//! All of it — ripples, waves, glints, fog — is vieww's own rasterizer
//! drawing circles, paths and gradients. No post. No images. No shader.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, ease_out_expo, mix, CANVAS, CYAN, CYAN_SOFT, Rng, VIOLET,
    VIOLET_DEEP, VIOLET_SOFT,
};
use crate::three_d::{displace_y, draw_mesh, grid_mesh, Camera, MeshStyle, Vec3};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 7.0;

/// The landing point in world space — the ocean's origin.
const ORIGIN: Vec3 = Vec3::new(0.0, 0.0, 0.0);

fn scene(book: &mut Sketchbook, canvas: Size, t: f32) {
    let w = canvas.width;
    let h = canvas.height;

    // The deep: a vertical ramp, colder than exp_mesh — water, not void.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::vertical()
            .with_dither()
            .with_stops(&[
                (0.0, Color::rgb(10, 14, 28)),
                (0.45, Color::rgb(6, 8, 18)),
                (1.0, Color::rgb(3, 4, 9)),
            ]),
    );

    // Phase gates.
    let fall_t = clamp01(t / 0.16);
    let strike_t = clamp01((t - 0.10) / 0.18);
    let field_t = clamp01((t - 0.28) / 0.22);
    let ocean_t = clamp01((t - 0.48) / 0.52);

    // The camera: high and looking down at first, lowering toward the
    // horizon as the ocean grows.
    let lower = ease_in_out(ocean_t);
    let cam = Camera {
        eye: Vec3::new(0.0, 9.0 - 5.5 * lower, 13.0 - 4.0 * lower),
        target: Vec3::new(0.0, 0.4 - 0.2 * lower, -2.0 - 6.0 * lower),
        fov: 0.95,
    };

    // ── A · the fall: a bright dot, accelerating, with a fading trail ─────
    if fall_t < 1.0 {
        let y_world = 8.0 - 7.9 * (fall_t * fall_t);
        let drop = Vec3::new(0.0, y_world, 0.0);
        // The trail: five ghosts above the dot, stretching as it speeds.
        let stretch = 1.0 + fall_t * 1.6;
        for k in 1..=5 {
            let gy = y_world + k as f32 * 0.22 * stretch;
            if let Some((pt, _, scale)) = cam.project(Vec3::new(0.0, gy, 0.0), canvas) {
                let r = (0.15 * scale * 900.0).max(0.5);
                book.circle(pt, r, alpha(VIOLET_SOFT, 0.38 - k as f32 * 0.065));
            }
        }
        // The dot itself, glowing.
        if let Some((pt, _, scale)) = cam.project(drop, canvas) {
            let r = (0.3 * scale * 900.0).max(1.4);
            book.layer(1.0, 10.0, None, |inner| {
                inner.circle(
                    pt,
                    r * 3.4,
                    Gradient::radial_fill().with_stops(&[
                        (0.0, alpha(VIOLET_SOFT, 0.60)),
                        (1.0, alpha(VIOLET, 0.0)),
                    ]),
                );
            });
            book.circle(pt, r, Color::WHITE);
        }
    }

    // ── B · the strike: squash + expanding rings, on the floor plane ──────
    if strike_t > 0.0 {
        // Impact flash.
        if strike_t < 0.3 {
            let flash = 1.0 - strike_t / 0.3;
            if let Some((pt, _, scale)) = cam.project(ORIGIN, canvas) {
                let r = 1.4 * scale * 900.0 * (0.4 + 0.6 * (1.0 - flash));
                book.circle(
                    pt,
                    r,
                    Gradient::radial_fill().with_stops(&[
                        (0.0, alpha(Color::WHITE, 0.5 * flash)),
                        (0.5, alpha(CYAN_SOFT, 0.3 * flash)),
                        (1.0, alpha(CYAN, 0.0)),
                    ]),
                );
            }
        }

        // The rings: staggered, expanding, fading.
        let ring_specs = [(0.0, 1.0), (0.18, 0.8), (0.38, 0.62)];
        for (delay, strength) in ring_specs {
            let rt = clamp01((strike_t - delay) / (1.0 - delay));
            if rt <= 0.0 {
                continue;
            }
            let radius = 0.3 + ease_out_expo(rt) * 5.2;
            let a = (1.0 - rt) * 0.55 * strength;
            if a <= 0.01 {
                continue;
            }
            draw_floor_ring(book, &cam, canvas, radius, a);
        }

        // The squash dot stays a moment, then sinks.
        if strike_t < 0.55 {
            let fade = 1.0 - strike_t / 0.55;
            if let Some((pt, _, scale)) = cam.project(ORIGIN, canvas) {
                let rx = (0.34 * scale * 900.0).max(1.5);
                let ry = (0.13 * scale * 900.0).max(0.8);
                let mut p = Path::new();
                // An ellipse by cubic pairs (Path has no direct ellipse).
                ellipse_path(&mut p, pt, rx, ry);
                book.fill(p, alpha(Color::WHITE, 0.85 * fade));
            }
        }
    }

    // ── C · the field: ripples everywhere, deterministic ──────────────────
    if field_t > 0.0 {
        let fade_out = 1.0 - ocean_t; // the field yields to the ocean
        if fade_out > 0.02 {
            let mut rng = Rng::new(0x0CEA);
            for _ in 0..14 {
                let x = rng.sym() * 7.5;
                let z = rng.sym() * 7.5;
                let phase = rng.f01();
                let center = Vec3::new(x, 0.0, z);
                let rt = clamp01((field_t - phase * 0.5) / 0.5);
                if rt <= 0.0 {
                    continue;
                }
                let radius = 0.2 + ease_out_expo(rt) * (1.6 + rng.f01() * 1.4);
                let a = (1.0 - rt) * 0.30 * fade_out;
                if a <= 0.01 {
                    continue;
                }
                draw_floor_ring_at(book, &cam, canvas, center, radius, a);
            }
        }
    }

    // ── D · the ocean: the heightfield, growing from the landing point ────
    if ocean_t > 0.0 {
        // Sky glow low on the horizon — dawn behind the water.
        book.layer(1.0, 26.0, None, |inner| {
            inner.circle(
                Offset::new(w * 0.5, h * 0.58),
                w * 0.5,
                Gradient::radial_fill().with_stops(&[
                    (0.0, alpha(mix(VIOLET, CYAN, 0.45), 0.20)),
                    (0.6, alpha(VIOLET_DEEP, 0.10)),
                    (1.0, alpha(VIOLET, 0.0)),
                ]),
            );
        });

        let phase = t * SECONDS;
        // 44x44 cells over the same span — the smoothness the audit asked for.
        let mut ocean = grid_mesh(44, 44, 0.425, mix(VIOLET_DEEP, Color::rgb(12, 18, 44), 0.45));
        for v in &mut ocean.verts {
            v.x -= 9.35;
            v.z -= 9.35;
        }
        displace_y(&mut ocean, |p| wave_height(p, phase));
        // Color by height: crest lighter, trough deeper.
        for (qi, _q) in ocean.quads.iter().enumerate() {
            let c = ocean.quad_center(&ocean.quads[qi]);
            let hh = wave_height(c, phase);
            let k = clamp01((hh + 0.24) / 0.48);
            ocean.colors[qi] = mix(
                mix(VIOLET_DEEP, Color::rgb(10, 14, 40), 0.5),
                mix(CYAN, VIOLET, 0.35),
                k * 0.7,
            );
        }

        // The growth: quads appear inside an expanding radius from ORIGIN.
        let grow_r = ease_out_expo(ocean_t) * 15.0;
        for (qi, q) in ocean.quads.iter().enumerate() {
            let c = ocean.quad_center(q);
            let d = ((c.x - ORIGIN.x).powi(2) + (c.z - ORIGIN.z).powi(2)).sqrt();
            let edge = clamp01((grow_r - d) / 1.2);
            ocean.colors[qi] = alpha(ocean.colors[qi], edge);
        }

        let style = MeshStyle {
            flat: false,
            edge_alpha: 0.0,
            fog_color: Color::rgb(5, 7, 16),
            fog_start: 10.0,
            fog_end: 26.0,
            specular: 0.75,
            shininess: 56.0,
            ramp_light: 0.07,
            ramp_dark: 0.20,
            ..MeshStyle::default()
        };
        draw_mesh(book, &ocean, &cam, canvas, &style);

        // Glints: crest highlights with a shared bloom pass — one blurred
        // layer carries all the halos, the crisp cores draw on top.
        let mut rng = Rng::new(0x6117);
        let mut cores: Vec<(Offset, f32, f32)> = Vec::new();
        for i in 0..120 {
            let x = rng.sym() * 8.5;
            let z = rng.sym() * 8.5;
            let d = (x.powi(2) + z.powi(2)).sqrt();
            if d > grow_r {
                continue;
            }
            let hh = wave_height(Vec3::new(x, 0.0, z), phase);
            let glint = ((hh - 0.18) * 9.0).clamp(0.0, 1.0);
            let twinkle = 0.5 + 0.5 * (phase * 5.0 + i as f32 * 1.3).sin();
            let a = glint * twinkle * 0.8;
            if a <= 0.03 {
                continue;
            }
            let p = Vec3::new(x, hh, z);
            if let Some((pt, _, scale)) = cam.project(p, canvas) {
                let r = (0.05 + 0.05 * glint) * scale * 900.0;
                cores.push((pt, r.max(0.5), a));
            }
        }
        book.layer(1.0, 7.0, None, |inner| {
            for (pt, r, a) in &cores {
                inner.circle(
                    *pt,
                    r * 4.0,
                    Gradient::radial_fill().with_stops(&[
                        (0.0, alpha(mix(Color::WHITE, CYAN_SOFT, 0.2), a * 0.5)),
                        (1.0, alpha(CYAN, 0.0)),
                    ]),
                );
            }
        });
        for (pt, r, a) in cores {
            book.circle(pt, r, alpha(mix(Color::WHITE, CYAN_SOFT, 0.3), a));
        }
    }

    // The vignette.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::radial(Offset::new(0.5, 0.5), 0.78)
            .with_dither()
            .with_stops(&[
                (0.62, alpha(Color::BLACK, 0.0)),
                (1.0, alpha(Color::BLACK, 0.52)),
            ]),
    );
}

/// The wave heightfield: three travelling sine waves crossing.
fn wave_height(p: Vec3, phase: f32) -> f32 {
    let a1 = 0.16 * (p.x * 0.9 + phase * 1.6).sin();
    let a2 = 0.11 * (p.z * 1.3 - phase * 2.1).sin();
    let a3 = 0.07 * ((p.x + p.z) * 0.7 + phase * 1.1).sin();
    a1 + a2 + a3
}

/// A ripple ring lying on the XZ plane at the origin.
fn draw_floor_ring(book: &mut Sketchbook, cam: &Camera, canvas: Size, radius: f32, a: f32) {
    draw_floor_ring_at(book, cam, canvas, ORIGIN, radius, a);
}

/// A ripple ring lying on the XZ plane at `center` — projected as a polyline
/// stroke, since perspective makes a projected circle an ellipse.
fn draw_floor_ring_at(
    book: &mut Sketchbook,
    cam: &Camera,
    canvas: Size,
    center: Vec3,
    radius: f32,
    a: f32,
) {
    let pts: Vec<Vec3> = (0..=48)
        .map(|i| {
            let ang = i as f32 / 48.0 * std::f32::consts::TAU;
            Vec3::new(
                center.x + ang.cos() * radius,
                0.015,
                center.z + ang.sin() * radius,
            )
        })
        .collect();
    let mut path = Path::new();
    let mut started = false;
    for p in &pts {
        if let Some((pt, _, _)) = cam.project(*p, canvas) {
            if started {
                path.line_to(pt);
            } else {
                path.move_to(pt);
                started = true;
            }
        }
    }
    if started {
        path.close();
        book.stroke(path, alpha(mix(CYAN_SOFT, Color::WHITE, 0.4), a), 2.0);
    }
}

/// An ellipse path via four cubic segments (kappa approximation).
fn ellipse_path(path: &mut Path, center: Offset, rx: f32, ry: f32) {
    const K: f32 = 0.552_284;
    let kx = rx * K;
    let ky = ry * K;
    let (cx, cy) = (center.dx, center.dy);
    path.move_to(Offset::new(cx + rx, cy));
    path.cubic_to(
        Offset::new(cx + rx, cy + ky),
        Offset::new(cx + kx, cy + ry),
        Offset::new(cx, cy + ry),
    );
    path.cubic_to(
        Offset::new(cx - kx, cy + ry),
        Offset::new(cx - rx, cy + ky),
        Offset::new(cx - rx, cy),
    );
    path.cubic_to(
        Offset::new(cx - rx, cy - ky),
        Offset::new(cx - kx, cy - ry),
        Offset::new(cx, cy - ry),
    );
    path.cubic_to(
        Offset::new(cx + kx, cy - ry),
        Offset::new(cx + rx, cy - ky),
        Offset::new(cx + rx, cy),
    );
    path.close();
}

/// The frame: all paint.
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
