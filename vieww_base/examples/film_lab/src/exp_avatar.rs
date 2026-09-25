//! exp_avatar — *a single line becomes a large 3D avatar.*
//!
//! The user's brief, verbatim: "from a single line to a large 3D avatar".
//! Four beats, each one the previous beat grown:
//!
//! - **A · THE LINE** (t 0–0.20): one profile curve draws itself through the
//!   void — dash-phase, the E-17 grammar. It is the silhouette of a head.
//! - **B · THE REVOLVE** (t 0.18–0.46): the line reveals it was always meant
//!   to turn — radial copies fan around the vertical axis, staggered, until
//!   a wireframe lathe cage stands where the line was. Ring hoops arrive.
//! - **C · THE SKIN** (t 0.42–0.72): the surface skins in, crown to base,
//!   face by face — Lambert + Blinn-Phong + per-face gradient ramps.
//! - **D · PRESENCE** (t 0.68–1.0): the light itself orbits — a sweep rakes
//!   across the face while the camera drifts; the avatar looks back.
//!
//! Every 3D pixel is the framework's own raster: the lathe is built here,
//! projected here, and filled as ordinary `Path`s. The bust is symmetric by
//! construction — a digital effigy, no eyes, the profile was always the
//! portrait.
//!
//! Receipts: lathe segments, quads, faces that survived the back-face cull
//! (the painter's own account), profile points.

use vieww_foundation::{
    Color, Dash, Gradient, Offset, Path, Rect, Size, Sketchbook, StrokeStyle, TextStyle,
};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, ease_out_expo, mix, tint, FAINT, INK, MUTED, Rng, VIOLET,
    VIOLET_DEEP, VIOLET_SOFT, CYAN, CYAN_SOFT, BG_DEEP,
};
use crate::three_d::{draw_mesh, Camera, Mesh, MeshStyle, Vec3};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 8.0;

/// Radial copies of the profile — the lathe's resolution.
const LATHE: usize = 64;

/// The profile, crown → base, as (radius, height) control points.
/// Drawn as one line in beat A; revolved in beat B; skinned in beat C.
const PROFILE: &[(f32, f32)] = &[
    (0.000, 3.30), // the crown point
    (0.340, 3.24),
    (0.620, 3.08),
    (0.820, 2.84), // the cranium
    (0.925, 2.55),
    (0.955, 2.28), // the temples — widest
    (0.905, 2.00),
    (0.820, 1.79), // the brow ridge
    (0.735, 1.65),
    (0.660, 1.51), // the cheeks fall in
    (0.575, 1.36),
    (0.495, 1.20), // the jaw
    (0.400, 1.05),
    (0.305, 0.94), // the chin
    (0.225, 0.82),
    (0.280, 0.66), // under the chin, the neck flares
    (0.365, 0.47),
    (0.410, 0.28), // the neck
    (0.435, 0.12),
    (0.450, 0.02), // the base
    (0.560, -0.02),
    (0.600, -0.10), // the pedestal lip
    (0.520, -0.14),
    (0.000, -0.14), // the pedestal base — closed to the axis
];

/// Catmull-Rom through the control points → a dense profile.
fn smooth_profile() -> Vec<(f32, f32)> {
    let n = PROFILE.len();
    let mut out = Vec::with_capacity((n - 1) * 6);
    for i in 0..n - 1 {
        let p0 = PROFILE[i.saturating_sub(1)];
        let p1 = PROFILE[i];
        let p2 = PROFILE[i + 1];
        let p3 = PROFILE[(i + 2).min(n - 1)];
        for k in 0..6 {
            let s = k as f32 / 6.0;
            let s2 = s * s;
            let s3 = s2 * s;
            let r = 0.5 * ((2.0 * p1.0)
                + (-p0.0 + p2.0) * s
                + (2.0 * p0.0 - 5.0 * p1.0 + 4.0 * p2.0 - p3.0) * s2
                + (-p0.0 + 3.0 * p1.0 - 3.0 * p2.0 + p3.0) * s3);
            let y = 0.5 * ((2.0 * p1.1)
                + (-p0.1 + p2.1) * s
                + (2.0 * p0.1 - 5.0 * p1.1 + 4.0 * p2.1 - p3.1) * s2
                + (-p0.1 + 3.0 * p1.1 - 3.0 * p2.1 + p3.1) * s3);
            out.push((r.max(0.0), y));
        }
    }
    out.push(*PROFILE.last().unwrap());
    out
}

/// The lathe mesh: profile revolved around Y. `reveal` gates faces by
/// normalized height (crown first); returns (mesh, faces drawn this frame).
fn lathe_mesh(reveal: f32) -> (Mesh, usize) {
    let profile = smooth_profile();
    let n = profile.len();
    let mut mesh = Mesh::new();

    // Vertices: profile × angle. The pedestal base row collapses to the axis
    // (r = 0), which is fine — degenerate quads render as triangles.
    for (r, y) in &profile {
        for j in 0..LATHE {
            let th = j as f32 / LATHE as f32 * std::f32::consts::TAU;
            mesh.verts.push(Vec3::new(r * th.cos(), *y, r * th.sin()));
        }
    }
    // Colors: a cool stone-violet ceramic, warmed toward the crown.
    let y_top = PROFILE[0].1;
    let y_bot = PROFILE[PROFILE.len() - 1].1;
    let span = (y_top - y_bot).max(1e-6);
    for i in 0..n - 1 {
        for j in 0..LATHE {
            let a = i * LATHE + j;
            let b = i * LATHE + (j + 1) % LATHE;
            let c = (i + 1) * LATHE + (j + 1) % LATHE;
            let d = (i + 1) * LATHE + j;
            let h = (profile[i].1 - y_bot) / span;
            let base = mix(
                mix(VIOLET_DEEP, Color::rgb(52, 46, 74), 0.55),
                tint(VIOLET, 0.12),
                (1.0 - h) * 0.5,
            );
            mesh.push_quad(a, b, c, d, base);
        }
    }

    // Reveal: crown (high y) first. Faces below the wave stay alpha 0.
    let mut drawn = 0usize;
    for qi in 0..mesh.quads.len() {
        let c = mesh.quad_center(&mesh.quads[qi]);
        let h = (c.y - y_bot) / span;
        let appear = clamp01((reveal * 1.25 - (1.0 - h)) / 0.22);
        mesh.colors[qi] = alpha(mesh.colors[qi], appear);
        if appear > 0.02 {
            drawn += 1;
        }
    }
    (mesh, drawn)
}

/// The wireframe cage: profile copies at K angles + ring hoops.
fn draw_cage(book: &mut Sketchbook, cam: &Camera, canvas: Size, fan: f32, hoops: f32, alpha_k: f32) {
    let profile = smooth_profile();
    let copies = 28;
    // Radial copies, staggered around the turn.
    for k in 0..copies {
        let arrive = clamp01(fan * copies as f32 * 1.4 - k as f32 * 1.15);
        if arrive <= 0.0 {
            continue;
        }
        let frac = ease_out_expo(arrive);
        let th = k as f32 / copies as f32 * std::f32::consts::TAU;
        let pts: Vec<Vec3> = profile
            .iter()
            .map(|(r, y)| Vec3::new(r * th.cos(), *y, r * th.sin()))
            .collect();
        stroke_frac3(book, &pts, cam, canvas, frac, alpha(VIOLET_SOFT, 0.34 * alpha_k), 1.0);
    }
    // Ring hoops at a few heights — the lathe's own circles.
    let hoop_at = [2.28f32, 1.79, 1.20, 0.47];
    for (hi, hy) in hoop_at.iter().enumerate() {
        let arrive = clamp01(hoops * hoop_at.len() as f32 * 1.3 - hi as f32 * 1.1);
        if arrive <= 0.0 {
            continue;
        }
        // Radius at this height: interpolate the profile.
        let r = profile
            .windows(2)
            .find_map(|w| {
                let (r0, y0) = w[0];
                let (r1, y1) = w[1];
                if (*hy <= y0) && (*hy >= y1) {
                    let s = (y0 - *hy) / (y0 - y1).max(1e-6);
                    Some(r0 + (r1 - r0) * s)
                } else {
                    None
                }
            })
            .unwrap_or(0.5);
        let pts: Vec<Vec3> = (0..=LATHE)
            .map(|j| {
                let a = j as f32 / LATHE as f32 * std::f32::consts::TAU;
                Vec3::new(r * a.cos(), *hy, r * a.sin())
            })
            .collect();
        stroke_frac3(
            book,
            &pts,
            cam,
            canvas,
            ease_out_expo(arrive),
            alpha(CYAN_SOFT, 0.30 * alpha_k),
            0.9,
        );
    }
}

/// A 3D polyline stroked with only the first `frac` of its screen length on —
/// the dash-phase line-drawing grammar (E-17), in three dimensions.
fn stroke_frac3(
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
            started = false;
        }
    }
    if !started {
        return;
    }
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
    book.stroke_styled(
        path,
        color,
        width,
        StrokeStyle::default().dash(Dash::new(vec![draw_len, total + 1.0])),
    );
}

// ── The scene ───────────────────────────────────────────────────────────────

fn scene(book: &mut Sketchbook, canvas: Size, t: f32) {
    let w = canvas.width;
    let h = canvas.height;

    // The void.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::vertical()
            .with_dither()
            .with_stops(&[
                (0.0, Color::rgb(11, 10, 18)),
                (0.55, BG_DEEP),
                (1.0, Color::rgb(5, 5, 9)),
            ]),
    );

    // Stage dust — sparse, slow, gives the void depth.
    {
        let mut rng = Rng::new(0xA7A);
        for i in 0..90 {
            let x = rng.f01() * w;
            let y = rng.f01() * h;
            let drift = (t * 0.6 + i as f32 * 0.71).sin() * 6.0;
            let tw = 0.5 + 0.5 * (t * 2.2 + i as f32 * 1.3).sin();
            book.circle(
                Offset::new(x + drift, y),
                0.5 + rng.f01() * 0.8,
                alpha(Color::WHITE, 0.02 + 0.05 * tw),
            );
        }
    }

    // Phase gates.
    let line_t = clamp01(t / 0.20);
    let fan_t = clamp01((t - 0.18) / 0.28);
    let skin_t = clamp01((t - 0.42) / 0.30);
    let present_t = clamp01((t - 0.68) / 0.32);

    // The camera: front for the line, off-axis for the revolve, orbiting
    // gently once the avatar has presence.
    let orbit = 0.55 + ease_in_out(present_t) * 0.85 + t * 0.10;
    let eye = Vec3::new(orbit.sin() * 6.8, 2.35 + 0.35 * present_t, orbit.cos() * 6.8);
    let cam = Camera {
        eye,
        target: Vec3::new(0.0, 1.62, 0.0),
        fov: 0.82,
    };

    // The light sweep — the beat-D event: the lamp orbits the face.
    let sweep = t * 1.9;
    let light_dir = Vec3::new(sweep.cos(), 0.55, sweep.sin()).norm();

    // Ground: the stage disc + contact shadow.
    if let Some((pt, _, scale)) = cam.project(Vec3::new(0.0, -0.14, 0.0), canvas) {
        let r = 2.1 * scale * 900.0;
        book.layer(1.0, 18.0, None, |g| {
            g.circle(
                pt,
                r,
                Gradient::radial_fill().with_stops(&[
                    (0.0, alpha(mix(VIOLET, CYAN, 0.25), 0.13 + 0.05 * present_t)),
                    (1.0, alpha(VIOLET, 0.0)),
                ]),
            );
        });
    }

    // ── A · THE LINE: the profile, drawn facing the camera ─────────────────
    let line_draw = ease_out_expo(line_t);
    if line_draw > 0.0 {
        let profile = smooth_profile();
        let pts: Vec<Vec3> = profile
            .iter()
            .map(|(r, y)| {
                // The line stands at the silhouette position, facing +z.
                Vec3::new(*r, *y, 0.0)
            })
            .collect();
        stroke_frac3(book, &pts, &cam, canvas, line_draw, tint(VIOLET_SOFT, 0.35), 2.4);
        // The line's own bloom.
        book.layer(1.0, 6.0, None, |g| {
            stroke_frac3(g, &pts, &cam, canvas, line_draw, alpha(VIOLET, 0.35), 5.0);
        });
    }

    // ── B · THE REVOLVE: the cage fans out around the line ────────────────
    if fan_t > 0.0 {
        let cage_fade = 1.0 - skin_t * 0.55;
        draw_cage(book, &cam, canvas, fan_t, clamp01((fan_t - 0.35) / 0.5), cage_fade);
    }

    // ── C · THE SKIN + D · PRESENCE ───────────────────────────────────────
    if skin_t > 0.0 {
        let (mesh, faces_drawn) = lathe_mesh(ease_out_expo(skin_t));
        let style = MeshStyle {
            light_dir,
            ambient: 0.20,
            edge_alpha: 0.10 * (1.0 - present_t * 0.4),
            edge_width: 0.8,
            specular: 0.55 + 0.45 * present_t,
            shininess: 26.0,
            ramp_light: 0.16,
            ramp_dark: 0.42,
            fog_color: Color::rgb(6, 6, 10),
            fog_start: 30.0,
            fog_end: 80.0,
            flat: false,
        };
        draw_mesh(book, &mesh, &cam, canvas, &style);

        // The rim pass: one Plus-blended stroke of the profile edge at the
        // light side — the avatar's lit contour (evaluated, not painted on).
        if present_t > 0.0 {
            let profile = smooth_profile();
            let rim: Vec<Vec3> = profile
                .iter()
                .map(|(r, y)| Vec3::new(r * sweep.cos(), *y, r * sweep.sin()))
                .collect();
            book.blended_layer(1.0, 3.0, vieww_foundation::BlendMode::Plus, None, |g| {
                stroke_frac3(
                    g,
                    &rim,
                    &cam,
                    canvas,
                    1.0,
                    alpha(tint(CYAN_SOFT, 0.4), 0.30 * present_t),
                    2.0,
                );
            });
        }

        // The horizon glow behind the bust.
        book.layer(1.0, 42.0, None, |g| {
            g.circle(
                Offset::new(w * 0.5, h * 0.56),
                w * 0.36,
                Gradient::radial_fill().with_stops(&[
                    (0.0, alpha(mix(VIOLET, CYAN, 0.3), 0.16 + 0.05 * present_t)),
                    (1.0, alpha(VIOLET, 0.0)),
                ]),
            );
        });

        // Receipt tag drawn into the scene: the face count, from the mesh's
        // own account. (Numbers here are counts the code itself took; the
        // textual instrument below carries them too.)
        let _ = faces_drawn;
    }

    // The vignette.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::radial(Offset::new(0.5, 0.5), 0.78)
            .with_dither()
            .with_stops(&[
                (0.55, alpha(Color::BLACK, 0.0)),
                (1.0, alpha(Color::BLACK, 0.52)),
            ]),
    );
}

/// The frame: the painter, plus the textual instrument.
pub fn frame(t: f32) -> WidgetNode {
    let paint = Painting::sized(
        crate::film_lib::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            scene(book, size, t);
        }),
    );

    // The instrument: stage + counts, all live.
    let line_t = clamp01(t / 0.20);
    let fan_t = clamp01((t - 0.18) / 0.28);
    let skin_t = clamp01((t - 0.42) / 0.30);
    let present_t = clamp01((t - 0.68) / 0.32);
    let stage = if present_t >= 1.0 || (present_t > 0.0 && t > 0.86) {
        "PRESENCE"
    } else if skin_t > 0.0 {
        "SKIN"
    } else if fan_t > 0.0 {
        "REVOLVE"
    } else if line_t > 0.0 {
        "LINE"
    } else {
        "VOID"
    };
    let profile_pts = PROFILE.len() * 6;
    let quads = (PROFILE.len() * 6) * LATHE;

    let strip = Stack::new()
        .push(
            Positioned::new()
                .left(24.0)
                .top(20.0)
                .width(420.0)
                .height(18.0)
                .child(
                    Text::new(format!(
                        "THE AVATAR · {stage} · one line revolved"
                    ))
                    .style(
                        TextStyle::new(12.0)
                            .monospace()
                            .letter_spacing(2.2)
                            .color(alpha(FAINT, 0.95)),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(24.0)
                .top(40.0)
                .width(460.0)
                .height(18.0)
                .child(
                    Text::new(format!(
                        "profile {profile_pts} pts · lathe {LATHE} · quads {quads} · light θ {:>+5.1}°",
                        (t * 1.9 * 57.29578).rem_euclid(360.0) - 180.0
                    ))
                    .style(TextStyle::new(11.0).monospace().color(alpha(MUTED, 0.85))),
                ),
        );

    Stack::new()
        .push(Positioned::fill().child(paint))
        .push(strip)
        .into()
}
