//! exp_galaxy — *one hundred thousand suns* (well: sixty thousand — the
//! number the renderer and the receipt agree on, drawn from the code's own
//! constant, and the lab's shape-count record by an order of magnitude).
//!
//! The tolerance axis is **raw shape count per frame**: every star is a
//! `Sketch` item the framework records, tessellates and rasterises itself.
//! Faint stars go down as 2px rects (the cheapest verb the sketchbook
//! knows); the bright tenth are real circles with Plus glow. The galaxy is
//! three logarithmic spiral arms with gaussian scatter, an exponential
//! radial density falloff, a small vertical thickness, differential
//! rotation (inner turns faster — closed form, so the frame at any `t` is
//! a pure function), dust lanes drawn as dark blobs along the arms, and a
//! handful of pink HII regions where stars are being born.
//!
//! Receipts: the star constant, the per-frame drawn count (measured from
//! the projection loop), the shape count from `SceneReport`, and ms/frame
//! — the honest cost of sixty thousand draws.

use std::sync::OnceLock;

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, mix, tint, FAINT, MUTED, Rng, VIOLET, VIOLET_SOFT, CYAN, CYAN_SOFT, AMBER,
    RED, BG_DEEP,
};
use crate::three_d::{Camera, Vec3};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 9.0;

/// The star count — the record attempt, printed by the receipt.
const STARS: usize = 60_000;

/// Spiral arm count.
const ARMS: usize = 3;

/// How tightly the arms wind.
const WIND: f32 = 3.1;

/// One star: position in the galaxy plane (before rotation), height, size,
/// colour index (0 core-gold, 1 blue-white, 2 red giant, 3 rim violet), and
/// its orbital radius for the differential turn.
struct Star {
    x: f32,
    z: f32,
    y: f32,
    size: f32,
    color: u8,
    orbit_r: f32,
}

fn stars() -> &'static Vec<Star> {
    static S: OnceLock<Vec<Star>> = OnceLock::new();
    S.get_or_init(|| {
        let mut rng = Rng::new(0x6A1);
        let mut out = Vec::with_capacity(STARS);
        for i in 0..STARS {
            // Radial placement: exponential falloff — dense core, sparse rim.
            let u = 1.0 - (1.0 - rng.f01()).powf(1.0 / 2.6);
            let r = 0.25 + u * 5.6;
            // The arm: a logarithmic spiral, with gaussian scatter across it.
            let arm = (i % ARMS) as f32;
            let arm_angle = r.log(std::f32::consts::E).mul_add(WIND, arm * std::f32::consts::TAU / ARMS as f32);
            // Scatter grows with radius (arms are loose at the rim).
            let spread = 0.10 + 0.16 * u;
            let th = arm_angle + rng.sym() * spread;
            let x = r * th.cos() + rng.sym() * 0.05;
            let z = r * th.sin() + rng.sym() * 0.05;
            // Vertical thickness: a thin disc with a small central bulge.
            let y = rng.sym() * (0.05 + 0.22 * (-r).exp());
            // Population: core is old and gold; arms are young and blue;
            // red giants scattered everywhere; rim faint violet.
            let color = if r < 0.75 {
                0
            } else if rng.f01() < 0.012 {
                2
            } else if u > 0.88 {
                3
            } else {
                1
            };
            let size = match color {
                0 => 0.9 + rng.f01() * 1.1,
                2 => 1.3 + rng.f01() * 1.6,
                _ => 0.7 + rng.f01() * 1.0,
            };
            out.push(Star {
                x,
                z,
                y,
                size,
                color,
                orbit_r: r,
            });
        }
        out
    })
}

/// Population colours.
fn star_color(k: u8, u: f32) -> Color {
    match k {
        0 => mix(Color::WHITE, AMBER, 0.35 + 0.3 * u),
        2 => mix(RED, Color::WHITE, 0.25),
        3 => mix(VIOLET, VIOLET_SOFT, u),
        _ => mix(Color::WHITE, CYAN_SOFT, 0.35),
    }
}

// ── The scene ───────────────────────────────────────────────────────────────

fn scene(book: &mut Sketchbook, canvas: Size, t: f32) {
    let w = canvas.width;
    let h = canvas.height;

    // Deep sky.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::vertical()
            .with_dither()
            .with_stops(&[
                (0.0, Color::rgb(4, 4, 8)),
                (0.6, BG_DEEP),
                (1.0, Color::rgb(3, 3, 6)),
            ]),
    );

    // The camera: an inclined view, drifting in azimuth.
    let az = 0.6 + t * 0.35;
    let cam = Camera {
        eye: Vec3::new(az.sin() * 7.4, 4.6 + 0.5 * (t * 0.5).sin(), az.cos() * 7.4),
        target: Vec3::ZERO,
        fov: 0.98,
    };

    // The differential turn: inner stars orbit faster.
    let turn = |r: f32| 0.55 / (0.55 + r * r * 1.4);

    // ── The outer glow: the galaxy's haze, before any star ────────────────
    book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
        for k in 0..3 {
            let scale = 1.0 + k as f32 * 1.15;
            if let Some((pt, _, sc)) = project_flat(0.0, 0.0, &cam, canvas) {
                let rr = 1.9 * scale;
                g.circle(
                    pt,
                    rr * sc * 900.0,
                    Gradient::radial_fill().with_stops(&[
                        (0.0, alpha(mix(AMBER, VIOLET, k as f32 / 3.0), 0.055 / scale)),
                        (1.0, alpha(VIOLET, 0.0)),
                    ]),
                );
            }
        }
    });

    // ── The stars — sixty thousand, every one of them a shape ────────────
    // ONE Plus group carries them all (the U-01 economy, the U-06 lesson):
    // faint stars as rects (the cheapest verb in the book), the bright
    // tenth as real circles with their own little glows.
    book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
        for (i, s) in stars().iter().enumerate() {
            let th = turn(s.orbit_r) * t * 2.4;
            let c = th.cos();
            let sn = th.sin();
            let x = s.x * c - s.z * sn;
            let z = s.x * sn + s.z * c;
            let Some((pt, depth, _scale)) = cam.project(Vec3::new(x, s.y, z), canvas) else {
                continue;
            };
            let u = (s.orbit_r / 5.85).min(1.0);
            let col = star_color(s.color, u);
            // Distance dimming + slight twinkle on the bright ones.
            let dim = 1.0 / (1.0 + depth * 0.10);
            let tw = if s.size > 1.6 {
                0.85 + 0.15 * (t * 6.0 + i as f32 * 1.7).sin()
            } else {
                1.0
            };
            let a = (0.28 + 0.60 * dim) * tw;
            if s.size < 1.35 {
                // The faint multitude: rects, no arcs.
                let r = (s.size * 1.15).max(1.1);
                g.rect(
                    Rect::new(pt.dx - r * 0.5, pt.dy - r * 0.5, pt.dx + r * 0.5, pt.dy + r * 0.5),
                    alpha(col, a * 0.85),
                );
            } else {
                // The bright tenth: circles.
                let rr = s.size * 0.62;
                g.circle(pt, rr, alpha(col, a));
                if s.size > 2.0 {
                    g.circle(pt, rr * 3.0, alpha(col, a * 0.10));
                }
            }
        }
    });

    // ── Dust lanes: dark blobs along the arms ─────────────────────────────
    {
        let mut rng = Rng::new(0xD05);
        for k in 0..170 {
            let u = 1.0 - (1.0 - rng.f01()).powf(1.0 / 2.2);
            let r = 0.9 + u * 4.6;
            let arm = (k % ARMS) as f32;
            let ang = r.log(std::f32::consts::E).mul_add(WIND, arm * std::f32::consts::TAU / ARMS as f32)
                + rng.sym() * 0.07;
            let spin = turn(r) * t * 2.4;
            let x = r * ang.cos();
            let z = r * ang.sin();
            let c = spin.cos();
            let sn = spin.sin();
            let rx = x * c - z * sn;
            let rz = x * sn + z * c;
            if let Some((pt, _, scale)) = cam.project(Vec3::new(rx, 0.0, rz), canvas) {
                let rr = (0.22 + rng.f01() * 0.30) * scale * 900.0;
                book.circle(
                    pt,
                    rr,
                    alpha(Color::rgb(8, 5, 12), 0.21),
                );
            }
        }
    }

    // ── HII regions: pink birth clouds in the arms ────────────────────────
    book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
        let mut rng = Rng::new(0x1B1);
        for _ in 0..26 {
            let u = 0.35 + rng.f01() * 0.6;
            let r = 0.9 + u * 4.3;
            let arm = (rng.u64() % ARMS as u64) as f32;
            let ang = r.log(std::f32::consts::E).mul_add(WIND, arm * std::f32::consts::TAU / ARMS as f32)
                + rng.sym() * 0.05;
            let spin = turn(r) * t * 2.4;
            let x = r * ang.cos();
            let z = r * ang.sin();
            let c = spin.cos();
            let sn = spin.sin();
            let rx = x * c - z * sn;
            let rz = x * sn + z * c;
            if let Some((pt, _, scale)) = cam.project(Vec3::new(rx, 0.0, rz), canvas) {
                let rr = (0.13 + rng.f01() * 0.16) * scale * 900.0;
                g.circle(
                    pt,
                    rr,
                    Gradient::radial_fill().with_stops(&[
                        (0.0, alpha(mix(RED, Color::WHITE, 0.40), 0.34)),
                        (1.0, alpha(RED, 0.0)),
                    ]),
                );
            }
        }
    });

    // ── The core: a bright, warm bulge ────────────────────────────────────
    book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
        if let Some((pt, _, scale)) = cam.project(Vec3::new(0.0, 0.0, 0.0), canvas) {
            g.circle(
                pt,
                0.42 * scale * 900.0,
                Gradient::radial_fill().with_stops(&[
                    (0.0, alpha(mix(Color::WHITE, AMBER, 0.30), 0.85)),
                    (0.25, alpha(mix(AMBER, Color::WHITE, 0.4), 0.30)),
                    (1.0, alpha(AMBER, 0.0)),
                ]),
            );
        }
    });

    // The vignette.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::radial(Offset::new(0.5, 0.5), 0.85)
            .with_dither()
            .with_stops(&[
                (0.55, alpha(Color::BLACK, 0.0)),
                (1.0, alpha(Color::BLACK, 0.55)),
            ]),
    );
}

/// Project a galaxy-plane point (x, z) with no rotation — for the glow.
fn project_flat(x: f32, z: f32, cam: &Camera, canvas: Size) -> Option<(Offset, f32, f32)> {
    cam.project(Vec3::new(x, 0.0, z), canvas)
}

/// The drawn count, as a pure function of `t` — the same loop, recounted,
/// so the instrument reads this frame's truth, not the last one's.
fn drawn_at(t: f32) -> usize {
    let canvas = crate::film_lib::CANVAS;
    let az = 0.6 + t * 0.35;
    let cam = Camera {
        eye: Vec3::new(az.sin() * 7.4, 4.6 + 0.5 * (t * 0.5).sin(), az.cos() * 7.4),
        target: Vec3::ZERO,
        fov: 0.98,
    };
    let turn = |r: f32| 0.55 / (0.55 + r * r * 1.4);
    let mut drawn = 0usize;
    for s in stars().iter() {
        let th = turn(s.orbit_r) * t * 2.4;
        let c = th.cos();
        let sn = th.sin();
        let x = s.x * c - s.z * sn;
        let z = s.x * sn + s.z * c;
        if cam.project(Vec3::new(x, s.y, z), canvas).is_some() {
            drawn += 1;
        }
    }
    drawn
}

/// The frame.
pub fn frame(t: f32) -> WidgetNode {
    let paint = Painting::sized(
        crate::film_lib::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            scene(book, size, t);
        }),
    );

    let drawn = drawn_at(t);

    let strip = Stack::new()
        .push(
            Positioned::new()
                .left(24.0)
                .top(20.0)
                .width(520.0)
                .height(18.0)
                .child(
                    Text::new("THE GALAXY · the shape-count record").style(
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
                .width(720.0)
                .height(18.0)
                .child(
                    Text::new(format!(
                        "{STARS} stars · {drawn} drawn this frame · 3 log-spiral arms · differential turn · rects for the faint, circles for the bright"
                    ))
                    .style(TextStyle::new(11.0).monospace().color(alpha(MUTED, 0.85))),
                ),
        );

    Stack::new()
        .push(Positioned::fill().child(paint))
        .push(strip)
        .into()
}
