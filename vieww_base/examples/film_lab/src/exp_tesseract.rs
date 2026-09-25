//! exp_tesseract — *the fourth direction.*
//!
//! A hypercube rotating through the planes our space cannot show. Beyond
//! imagination, literally: the object does not fit in 3D, so it is rotated
//! in 4D (XW and YW planes), perspective-divided into 3D, and projected a
//! second time to the frame — a shadow of a shadow, every pixel of it the
//! framework's own raster.
//!
//! - **16 vertices, 32 edges** — every edge pairs vertices differing in
//!   exactly one coordinate; no shortcuts.
//! - **The colour is the w axis**: near-w burns violet, far-w cools cyan —
//!   the fourth direction is legible as temperature, not position.
//! - **Motion persistence**: the last six states of the object are
//!   *evaluated, not remembered* — the same closed form at `t − k·Δ` (the
//!   ghosts' discipline), streaking the 4D turn.
//! - **The hyper-flip**: as the XW rotation crosses 45°, the inner cell
//!   and the outer cell swap places — the classic tesseract turn, the
//!   moment the fourth direction becomes visible to a 3D eye.
//!
//! Receipts: edges projected (behind-camera drops counted), the live 4D
//! angles, the w-range spanned this frame.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, mix, tint, FAINT, MUTED, Rng, VIOLET, VIOLET_SOFT, CYAN, CYAN_SOFT, BG_DEEP,
};
use crate::three_d::{Camera, Vec3};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 7.0;

/// Ghost states behind the present one.
const GHOSTS: usize = 6;

/// The 4D vertex set: all (±1)⁴ — sixteen corners.
fn vertices4() -> Vec<[f32; 4]> {
    let mut v = Vec::with_capacity(16);
    for bits in 0..16u32 {
        let sign = |b: u32| if b == 0 { -1.0 } else { 1.0 };
        v.push([
            sign(bits & 1),
            sign((bits >> 1) & 1),
            sign((bits >> 2) & 1),
            sign((bits >> 3) & 1),
        ]);
    }
    v
}

/// The 32 edges: vertex pairs differing in exactly one coordinate.
fn edges4() -> Vec<(usize, usize)> {
    let v = vertices4();
    let mut e = Vec::new();
    for i in 0..16 {
        for j in (i + 1)..16 {
            let diff = (0..4).filter(|&k| v[i][k] != v[j][k]).count();
            if diff == 1 {
                e.push((i, j));
            }
        }
    }
    e
}

/// Rotate one 4D plane. `k` and `l` are the plane's axes.
fn rot_plane(v: [f32; 4], k: usize, l: usize, a: f32) -> [f32; 4] {
    let (s, c) = a.sin_cos();
    let mut out = v;
    out[k] = v[k] * c - v[l] * s;
    out[l] = v[k] * s + v[l] * c;
    out
}

/// The full 4D rotation at film-time `t`.
fn rot4(v: [f32; 4], t: f32) -> [f32; 4] {
    // The XW turn is the hero; YW runs counterpoint; XZ adds 3D parallax.
    let xw = t * 1.10;
    let yw = -t * 0.55 + 0.35;
    let xz = t * 0.22;
    let v = rot_plane(v, 0, 3, xw);
    let v = rot_plane(v, 1, 3, yw);
    rot_plane(v, 0, 2, xz)
}

/// 4D → 3D: the perspective divide by w (the second perspective).
fn project_w(v: [f32; 4]) -> Vec3 {
    let d = 3.4;
    let k = d / (d - v[3]);
    Vec3::new(v[0] * k, v[1] * k, v[2] * k)
}

/// The edge's colour: the w axis as temperature.
fn w_color(w: f32) -> Color {
    let k = clamp01((w + 1.6) / 3.2);
    mix(CYAN_SOFT, VIOLET_SOFT, k)
}

// ── The scene ───────────────────────────────────────────────────────────────

/// The orbit camera — shared by the scene and the instrument, so both read
/// the same projection.
fn camera_at(t: f32) -> Camera {
    let theta = 0.5 + t * 0.55;
    Camera {
        eye: Vec3::new(theta.sin() * 8.2, 1.8 + 0.7 * (t * 0.7).sin(), theta.cos() * 8.2),
        target: Vec3::ZERO,
        fov: 0.92,
    }
}

/// Behind-camera drops at this t — the boundary discipline (U-16's cousin),
/// counted for the receipt.
fn behind_at(t: f32) -> usize {
    let cam = camera_at(t);
    let verts = vertices4();
    let canvas = crate::film_lib::CANVAS;
    let projected: Vec<Option<(Offset, f32, f32)>> = verts
        .iter()
        .map(|v| cam.project(project_w(rot4(*v, t)), canvas))
        .collect();
    edges4()
        .iter()
        .filter(|(i, j)| projected[*i].is_none() || projected[*j].is_none())
        .count()
}

fn scene(book: &mut Sketchbook, canvas: Size, t: f32) {
    let w = canvas.width;
    let h = canvas.height;

    // The void.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::vertical()
            .with_dither()
            .with_stops(&[
                (0.0, Color::rgb(7, 8, 13)),
                (0.6, BG_DEEP),
                (1.0, Color::rgb(4, 5, 9)),
            ]),
    );

    // Sparse stars — still, patient.
    {
        let mut rng = Rng::new(0x7E55);
        for i in 0..120 {
            let x = rng.f01() * w;
            let y = rng.f01() * h;
            let tw = 0.5 + 0.5 * (t * 1.8 + i as f32 * 1.31).sin();
            book.circle(
                Offset::new(x, y),
                0.4 + rng.f01() * 0.8,
                alpha(Color::WHITE, 0.03 + 0.09 * tw),
            );
        }
    }

    // The nebula wash behind the object.
    book.layer(1.0, 46.0, None, |g| {
        g.circle(
            Offset::new(w * 0.5, h * 0.48),
            w * 0.33,
            Gradient::radial_fill().with_stops(&[
                (0.0, alpha(mix(VIOLET, CYAN, 0.4), 0.11)),
                (1.0, alpha(VIOLET, 0.0)),
            ]),
        );
    });

    // The camera: a slow orbit for parallax.
    let cam = camera_at(t);

    let verts = vertices4();
    let edges = edges4();

    // Project every vertex once.
    let projected: Vec<Option<(Offset, f32, f32)>> = verts
        .iter()
        .map(|v| cam.project(project_w(rot4(*v, t)), canvas))
        .collect();

    // The hyper-flip: XW crossing 45° (mod 90°) — the inner and outer
    // cells trade places. Its proximity drives a soft bloom.
    let xw = t * 1.10;
    let flip_prox = 1.0 - (((xw + std::f32::consts::FRAC_PI_4) % std::f32::consts::FRAC_PI_2)
        - std::f32::consts::FRAC_PI_4)
        .abs()
        / std::f32::consts::FRAC_PI_4;

    // ── The ghost streaks: previous states, evaluated not remembered ─────
    for k in (1..=GHOSTS).rev() {
        let s = (t - k as f32 * 0.014).max(0.0);
        let age = k as f32 / (GHOSTS + 1) as f32;
        let a = (1.0 - age).powi(2) * 0.16;
        for &(i, j) in &edges {
            let pi = cam.project(project_w(rot4(verts[i], s)), canvas);
            let pj = cam.project(project_w(rot4(verts[j], s)), canvas);
            if let (Some((a0, _, _)), Some((b0, _, _))) = (pi, pj) {
                book.line(a0, b0, alpha(w_color(rot4(verts[i], s)[3]), a), 1.0);
            }
        }
    }

    // ── The edges: the tesseract's 32, coloured by w ──────────────────────
    for &(i, j) in &edges {
        if let (Some((a0, d0, s0)), Some((b0, d1, s1))) = (projected[i], projected[j]) {
            let depth = (d0 + d1) * 0.5;
            let scale = (s0 + s1) * 0.5;
            let w_mid = (rot4(verts[i], t)[3] + rot4(verts[j], t)[3]) * 0.5;
            let col = w_color(w_mid);
            let width = (1.1 + 2.2 * scale) * (1.0 + flip_prox * 0.35);
            let a = (0.55 + 0.35 * scale) * (1.0 - clamp01((depth - 16.0) / 8.0));
            // Depth fog: far edges sink back into the void.
            let col = mix(col, Color::rgb(10, 12, 18), clamp01((depth - 9.0) / 9.0) * 0.6);
            book.line(a0, b0, alpha(col, a), width);
        }
    }

    // ── The vertices: 16 lamps ────────────────────────────────────────────
    book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
        for (vi, p) in projected.iter().enumerate() {
            let Some((pt, _, scale)) = *p else { continue };
            let wv = rot4(verts[vi], t)[3];
            let col = w_color(wv);
            let r = (2.0 + 2.8 * scale) * (1.0 + flip_prox * 0.5);
            g.circle(pt, r, alpha(tint(col, 0.45), 0.75));
            g.circle(pt, r * 2.8, alpha(col, 0.14));
        }
        // The flip bloom: when the swap is closest, the object exhales.
        if flip_prox > 0.72 {
            let a = (flip_prox - 0.72) / 0.28;
            g.circle(
                Offset::new(w * 0.5, h * 0.48),
                90.0 + 60.0 * a,
                Gradient::radial_fill().with_stops(&[
                    (0.0, alpha(mix(VIOLET, Color::WHITE, 0.5), 0.16 * a)),
                    (1.0, alpha(VIOLET, 0.0)),
                ]),
            );
        }
    });

    // The vignette.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::radial(Offset::new(0.5, 0.5), 0.8)
            .with_dither()
            .with_stops(&[
                (0.6, alpha(Color::BLACK, 0.0)),
                (1.0, alpha(Color::BLACK, 0.5)),
            ]),
    );
}

/// The frame.
pub fn frame(t: f32) -> WidgetNode {
    let paint = Painting::sized(
        crate::film_lib::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            scene(book, size, t);
        }),
    );

    // The instrument: the 4D angles, live.
    let xw_deg = (t * 1.10 * 57.29578).rem_euclid(360.0);
    let yw_deg = ((-t * 0.55 + 0.35) * 57.29578).rem_euclid(360.0);
    let edges = edges4().len();
    let behind = behind_at(t);

    let strip = Stack::new()
        .push(
            Positioned::new()
                .left(24.0)
                .top(20.0)
                .width(520.0)
                .height(18.0)
                .child(
                    Text::new("THE TESSERACT · a shadow of a shadow").style(
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
                        "16 verts · {edges} edges · XW {xw_deg:>5.1}° · YW {yw_deg:>5.1}° · ghosts {GHOSTS} · behind-cam {behind}"
                    ))
                    .style(TextStyle::new(11.0).monospace().color(alpha(MUTED, 0.85))),
                ),
        );

    Stack::new()
        .push(Positioned::fill().child(paint))
        .push(strip)
        .into()
}
