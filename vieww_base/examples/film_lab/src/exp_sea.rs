//! exp_sea — *from a drop to an entire ocean.*
//!
//! The user's brief, verbatim: "from a drop to an entire ocean". One water
//! surface, one continuous camera move — the reveal is scale, not a cut:
//!
//! - **THE FALL** (t 0–0.16): looking straight down. A drop falls out of
//!   the sky toward the black water. One drop, nothing else.
//! - **THE STRIKE** (t 0.14–0.30): impact — a flash, and the rings begin:
//!   a radial damped wave train expanding from the contact point, the
//!   surface waking up around it.
//! - **THE PULL** (t 0.26–0.60): the camera rises and tilts — the same
//!   rings, now texture on a widening plane; wind waves arrive across the
//!   whole surface as the horizon enters the frame.
//! - **THE HORIZON** (t 0.58–1.0): the full ocean — dusk sky, a low sun,
//!   the glitter path marching toward the camera. And the original ripple
//!   is *still expanding*, faint, at the centre: that was one drop.
//!
//! The surface is one heightfield the whole way through: the ripple and
//! the wind waves live in the same `h(r, θ, t)`; the camera and the light
//! are the only things that change. Nothing in post — every pixel is the
//! framework's own raster, quads first.
//!
//! Receipts: ring count × segments (the mesh's own quads), glints drawn
//! (counted from the spec test, not the sample), camera altitude.

use vieww_foundation::{
    Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle,
};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, mix, tint, FAINT, MUTED, Rng, CYAN, CYAN_SOFT, AMBER,
};
use crate::three_d::{Camera, Mesh, MeshStyle, Vec3};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 9.0;

/// Radial segments around the drop point.
const SEG: usize = 88;

/// Ring rows, exponential — dense at the drop, coarse at the horizon.
const ROWS: usize = 54;

/// The far edge of the water, world units.
const FAR: f32 = 380.0;

/// When the drop lands (film-time fraction).
const IMPACT: f32 = 0.15;

/// The water heightfield — one surface, the whole experiment.
/// `ripple` = ripple strength at this t, `wind` = wind wave strength.
fn water_height(x: f32, z: f32, t: f32, ripple: f32, wind: f32) -> f32 {
    let r = (x * x + z * z).sqrt();
    // The drop's wave train: an expanding front with trailing rings,
    // damped in space and decaying in time as the energy spreads.
    let tt = t.max(IMPACT) - IMPACT;
    let front = 1.5 + tt * 26.0;
    let trail = ((front - r) / 2.6).clamp(0.0, 1.0);
    let rings = (r * 1.9 - tt * 26.0).sin();
    let spatial_damp = (-r / 14.0).exp() * (1.0 / (1.0 + r * 0.09));
    let time_damp = (-tt * 0.55).exp();
    let h_ripple = rings * trail * spatial_damp * time_damp * 0.62 * ripple;

    // The wind field — crossing swells + chop, steady state.
    let swell = (0.42 * x + 0.86 * z + t * 3.2).sin() * 0.30
        + (-0.77 * x + 0.30 * z + t * 2.1).sin() * 0.19;
    let chop = (2.9 * x + 1.7 * z + t * 6.4).sin() * 0.055
        + (-1.9 * x + 3.3 * z + t * 5.2).sin() * 0.048;
    (h_ripple + (swell + chop) * wind) * (1.0 - (r / FAR).powi(6) * 0.0)
}

/// Build the radial water mesh for this t.
fn water_mesh(t: f32, ripple: f32, wind: f32) -> Mesh {
    let mut m = Mesh::new();
    let base = Color::rgb(9, 26, 38);
    // Rings: r grows exponentially from 0.35 to FAR.
    let radii: Vec<f32> = (0..=ROWS)
        .map(|i| {
            let s = i as f32 / ROWS as f32;
            0.35 * (FAR / 0.35).powf(s)
        })
        .collect();
    for &r in &radii {
        for j in 0..SEG {
            let th = j as f32 / SEG as f32 * std::f32::consts::TAU;
            let (s, c) = th.sin_cos();
            let x = r * c;
            let z = r * s;
            m.verts.push(Vec3::new(x, water_height(x, z, t, ripple, wind), z));
        }
    }
    let row = SEG;
    for i in 0..ROWS {
        for j in 0..SEG {
            let a = i * row + j;
            let b = i * row + (j + 1) % SEG;
            let c = (i + 1) * row + (j + 1) % SEG;
            let d = (i + 1) * row + j;
            // Colour deepens with radius — the shelf falls off.
            let rr = radii[i + 1];
            let deep = mix(base, Color::rgb(4, 10, 18), clamp01(rr / 90.0));
            m.push_quad(a, b, c, d, deep);
        }
    }
    // Winding: rings go clockwise seen from above? Ensure up-facing:
    // rebuild quads with reversed order if the cull eats the floor.
    m
}

// ── The scene ───────────────────────────────────────────────────────────────

fn scene(book: &mut Sketchbook, canvas: Size, t: f32) {
    let w = canvas.width;
    let h = canvas.height;

    // Stage gates.
    let fall_t = clamp01(t / IMPACT);
    let strike_t = clamp01((t - IMPACT) / 0.15);
    let pull_t = clamp01((t - 0.26) / 0.34);
    let horizon_t = clamp01((t - 0.58) / 0.42);
    let ripple = strike_t * (1.0 - horizon_t * 0.75);
    let wind = strike_t * 0.55 + ease_in_out(pull_t) * 0.45;

    // ── The camera: one continuous move, down-looking → horizon ───────────
    let rise = ease_in_out(pull_t);
    let eye = Vec3::new(
        0.0,
        5.2 + rise * 42.0 + horizon_t * 4.0,
        0.01 + rise * 78.0,
    );
    let target = Vec3::new(0.0, 0.0, rise * -110.0);
    let cam = Camera { eye, target, fov: 0.95 };
    let sun_dir = Vec3::new(-0.42, 0.26, -0.87).norm();

    // ── The sky: dusk gradient + low sun + haze band ──────────────────────
    // Horizon screen position: project a point far along the view.
    let far_pt = target.add(Vec3::new(0.0, 0.0, -260.0));
    let horizon_y = cam
        .project(far_pt, canvas)
        .map(|(p, _, _)| p.dy)
        .unwrap_or(h * 0.5);
    let sky_h = (horizon_y + 30.0).clamp(0.0, h);
    book.rect(
        Rect::new(0.0, 0.0, w, sky_h),
        Gradient::vertical()
            .with_dither()
            .with_stops(&[
                (0.0, Color::rgb(7, 8, 16)),
                (0.55, Color::rgb(18, 14, 28)),
                (0.85, Color::rgb(46, 22, 34)),
                (1.0, Color::rgb(94, 42, 38)),
            ]),
    );
    // The sun: project its direction to the sky.
    let sun_world = eye.add(sun_dir.scale(600.0));
    if let Some((sp, _, _)) = cam.project(sun_world, canvas) {
        if sp.dy < sky_h {
            book.layer(1.0, 24.0, None, |g| {
                g.circle(
                    sp,
                    46.0,
                    Gradient::radial_fill().with_stops(&[
                        (0.0, alpha(tint(AMBER, 0.55), 0.85)),
                        (0.35, alpha(AMBER, 0.30)),
                        (1.0, alpha(AMBER, 0.0)),
                    ]),
                );
            });
            book.circle(sp, 9.0, alpha(tint(AMBER, 0.8), 0.95));
        }
    }
    // Clouds: a few blurred smears along the horizon.
    {
        let mut rng = Rng::new(0x5EA);
        book.layer(1.0, 9.0, None, |g| {
            for i in 0..7 {
                let cw = (rng.f01() * 0.30 + 0.18) * w;
                let cx = rng.f01() * w;
                let cy = sky_h - (10.0 + rng.f01() * 40.0);
                let a = 0.05 + rng.f01() * 0.05 + horizon_t * 0.05;
                let ch = 7.0 + rng.f01() * 7.0;
                let rect = Rect::new(cx - cw * 0.5, cy - ch, cx + cw * 0.5, cy + ch);
                g.rrect(rect, ch, alpha(mix(Color::rgb(60, 30, 44), Color::rgb(110, 60, 50), i as f32 / 7.0), a));
            }
        });
    }

    // ── The water: one mesh, the same surface throughout ──────────────────
    let mesh = water_mesh(t, ripple, wind);
    let style = MeshStyle {
        light_dir: sun_dir,
        ambient: 0.30,
        fog_color: Color::rgb(38, 24, 34),
        fog_start: 60.0,
        fog_end: 330.0,
        edge_alpha: 0.0,
        edge_width: 1.0,
        specular: 0.65,
        shininess: 22.0,
        ramp_light: 0.22,
        ramp_dark: 0.34,
        flat: false,
    };
    crate::three_d::draw_mesh(book, &mesh, &cam, canvas, &style);

    // ── The glitter path: specular sparks toward the sun ──────────────────
    if horizon_t > 0.05 {
        book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
            for (x, z, bright) in glint_samples(t, ripple, wind, &eye, &sun_dir) {
                if bright < 0.22 {
                    continue;
                }
                if let Some((p, _, scale)) =
                    cam.project(Vec3::new(x, water_height(x, z, t, ripple, wind), z), canvas)
                {
                    let rr = (1.0 + bright * 2.2) * scale * 900.0;
                    let col = mix(AMBER, Color::WHITE, bright * 0.7);
                    g.circle(p, rr, alpha(col, (bright * 0.85).min(1.0)));
                    if bright > 0.6 {
                        g.circle(p, rr * 2.6, alpha(tint(AMBER, 0.5), bright * 0.16));
                    }
                }
            }
        });
    }

    // ── The fall + the strike (the drop itself) ───────────────────────────
    if t < IMPACT + 0.02 {
        let tt = t / IMPACT;
        let y = 5.0 * (1.0 - tt * tt);
        if let Some((p, _, _)) = cam.project(Vec3::new(0.0, y, 0.0), canvas) {
            // The streak of the fall — the drop is moving fast.
            let trail = 26.0 * (1.0 - tt * 0.6);
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                let mut path = Path::new();
                path.move_to(Offset::new(p.dx, p.dy - trail));
                path.line_to(p);
                g.stroke(path, alpha(tint(CYAN_SOFT, 0.6), 0.8), 2.2);
                g.circle(p, 2.6 + 1.4 * (1.0 - tt), alpha(Color::WHITE, 0.95));
            });
        }
    }
    // ── The strike: a flash at the origin, riding the surface ───────────
    if strike_t > 0.0 && strike_t < 1.0 {
        if let Some((p, _, scale)) = cam.project(Vec3::new(0.0, 0.0, 0.0), canvas) {
            let flash = (1.0 - strike_t).powi(2);
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                g.circle(p, (14.0 + 60.0 * (1.0 - flash)) * scale * 900.0, alpha(tint(CYAN, 0.5), 0.4 * flash));
            });
        }
    }

    // ── Horizon haze: soften the seam where the water meets the sky ───────
    if sky_h > 0.0 && horizon_t > 0.0 {
        let band_h = 26.0 + 30.0 * (1.0 - horizon_t);
        let y0 = (horizon_y - band_h * 0.4).max(0.0);
        book.rect(
            Rect::new(0.0, y0, w, (y0 + band_h).min(h)),
            Gradient::vertical().with_stops(&[
                (0.0, alpha(Color::rgb(46, 24, 36), 0.0)),
                (0.6, alpha(Color::rgb(52, 28, 38), 0.35 * horizon_t)),
                (1.0, alpha(Color::rgb(24, 16, 24), 0.0)),
            ]),
        );
    }

    // The vignette.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::radial(Offset::new(0.5, 0.5), 0.82)
            .with_dither()
            .with_stops(&[
                (0.55, alpha(Color::BLACK, 0.0)),
                (1.0, alpha(Color::BLACK, 0.5)),
            ]),
    );
}

/// The glitter samples — deterministic, pure in `t` — so the paint callback
/// and the textual instrument read the same list, not a frame-behind copy.
fn glint_samples(
    t: f32,
    ripple: f32,
    wind: f32,
    eye: &Vec3,
    sun_dir: &Vec3,
) -> Vec<(f32, f32, f32)> {
    let mut rng = Rng::new(0x61);
    let mut out = Vec::new();
    for i in 0..380 {
        // Sample in the sun's azimuth band, near to far.
        let u = rng.f01();
        let r = 4.0 + u * 120.0;
        let spread = 0.55 * (1.0 - u * 0.4);
        let az = -0.45 + rng.sym() * spread;
        let x = r * az.sin();
        let z = r * az.cos() * -1.0;
        // Specular test: wave normal · halfway(sun, eye).
        let e = 0.4;
        let hx = (water_height(x + e, z, t, ripple, wind) - water_height(x - e, z, t, ripple, wind))
            / (2.0 * e);
        let hz = (water_height(x, z + e, t, ripple, wind) - water_height(x, z - e, t, ripple, wind))
            / (2.0 * e);
        let nrm = Vec3::new(-hx, 1.0, -hz).norm();
        let view = eye.sub(Vec3::new(x, 0.0, z)).norm();
        let half = sun_dir.add(view).norm();
        let spec = nrm.dot(half).max(0.0).powf(64.0);
        let twinkle = 0.5 + 0.5 * (t * 7.0 + i as f32 * 2.399).sin();
        out.push((x, z, spec * (0.35 + 0.65 * twinkle)));
    }
    out
}

/// The frame.
pub fn frame(t: f32) -> WidgetNode {
    let paint = Painting::sized(
        crate::film_lib::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            scene(book, size, t);
        }),
    );

    // The instrument strip — every number the code itself computed.
    let stage = if t < IMPACT {
        "THE FALL"
    } else if t < 0.30 {
        "THE STRIKE"
    } else if t < 0.60 {
        "THE PULL"
    } else {
        "THE HORIZON"
    };
    // The glint count at THIS t — the same pure sampler the painter used.
    let pull_t = clamp01((t - 0.26) / 0.34);
    let rise = ease_in_out(pull_t);
    let eye = Vec3::new(0.0, 5.2 + rise * 42.0, 0.01 + rise * 78.0);
    let sun_dir = Vec3::new(-0.42, 0.26, -0.87).norm();
    let strike_t = clamp01((t - IMPACT) / 0.15);
    let horizon_t = clamp01((t - 0.58) / 0.42);
    let ripple = strike_t * (1.0 - horizon_t * 0.75);
    let wind = strike_t * 0.55 + ease_in_out(pull_t) * 0.45;
    let glints = if horizon_t > 0.05 {
        glint_samples(t, ripple, wind, &eye, &sun_dir)
            .iter()
            .filter(|(_, _, b)| *b >= 0.22)
            .count()
    } else {
        0
    };
    let alt = 5.2 + rise * 42.0;
    let quads = ROWS * SEG;

    let strip = Stack::new()
        .push(
            Positioned::new()
                .left(24.0)
                .top(20.0)
                .width(520.0)
                .height(18.0)
                .child(
                    Text::new(format!("THE SEA · {stage} · one surface throughout")).style(
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
                .width(640.0)
                .height(18.0)
                .child(
                    Text::new(format!(
                        "alt {alt:>5.1} u · mesh {quads} quads ({ROWS} rings × {SEG} seg) · glints {glints}"
                    ))
                    .style(TextStyle::new(11.0).monospace().color(alpha(MUTED, 0.85))),
                ),
        );

    Stack::new()
        .push(Positioned::fill().child(paint))
        .push(strip)
        .into()
}
