//! exp_blackhole — *the heavy light.*
//!
//! Beyond imagination, drawn with arithmetic: an accretion disk of 2,400
//! orbiting particles, Doppler-boosted on the approaching side, its far
//! half bent over the top of the shadow the way a real hole bends it; a
//! photon ring; a star field displaced by the lens — every star's position
//! recomputed from the same deflection formula, the ones that fell inside
//! the ring swallowed and counted.
//!
//! The construction is a 2D fake of a relativistic picture, and says so:
//! the bending is one formula, `deflect(r) = k·(R/r)²`, applied to stars
//! and disk alike. But nothing here is a texture — every particle is a
//! `Path` the framework rasterised, through **one Plus-blended group**
//! (U-01's economy, U-06's discipline), and the receipt counts what the
//! lens ate.
//!
//! - **the disk** — Keplerian orbits (ω ∝ a⁻³ᐟ²), temperature by radius
//!   (white-hot inner rim → ember outer edge), beamed by orbital phase;
//! - **the arc** — the far side's light, bent over the shadow's top edge,
//!   compressed onto the halo ring;
//! - **the lens** — 2,400 background stars displaced outward near the
//!   shadow, tangentially smeared where the deflection is strongest;
//! - **the infall** — a spiral of matter raining in, trailing.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, mix, tint, FAINT, MUTED, Rng, VIOLET, VIOLET_DEEP, CYAN, CYAN_SOFT, AMBER,
    RED, BG_DEEP,
};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 7.0;

/// Disk particle count.
const DISK: usize = 2400;

/// Background star count.
const STARS: usize = 2400;

/// The disk's inner / outer radius, px.
const A_MIN: f32 = 62.0;
const A_MAX: f32 = 210.0;

/// The shadow's radius, px (the event horizon's silhouette).
const R_SHADOW: f32 = 46.0;

/// The photon ring radius, px.
const R_PHOTON: f32 = 53.0;

/// The disk's inclination squash.
const INCL: f32 = 0.30;

/// Orbital rate at the inner edge, turns per experiment.
const OMEGA_IN: f32 = 3.4;

/// Temperature colour by normalised radius (0 = inner rim).
fn temp_color(u: f32) -> Color {
    let u = clamp01(u);
    if u < 0.22 {
        mix(Color::WHITE, CYAN_SOFT, u / 0.22)
    } else if u < 0.58 {
        mix(CYAN_SOFT, AMBER, (u - 0.22) / 0.36)
    } else {
        mix(AMBER, RED, (u - 0.58) / 0.42)
    }
}

/// The lens: radial displacement factor for a point at distance `r` from
/// the centre. Far away ≈ 1 (undeviated); near the shadow it grows.
fn deflect(r: f32) -> f32 {
    1.0 + 1.35 * (R_PHOTON / r.max(1.0)).powi(2)
}

// ── The scene ───────────────────────────────────────────────────────────────

fn scene(book: &mut Sketchbook, canvas: Size, t: f32) {
    let w = canvas.width;
    let h = canvas.height;

    // The centre — a slow drift for parallax.
    let c = Offset::new(
        w * 0.5 + 9.0 * (t * 0.9).sin(),
        h * 0.52 + 5.0 * (t * 0.7).cos(),
    );

    // Deep space.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::vertical()
            .with_dither()
            .with_stops(&[
                (0.0, Color::rgb(5, 5, 9)),
                (0.55, BG_DEEP),
                (1.0, Color::rgb(4, 4, 7)),
            ]),
    );

    // Faint nebula washes — the sky is not empty, it is dark.
    book.layer(1.0, 40.0, None, |g| {
        let mut rng = Rng::new(0xB1D);
        for _ in 0..3 {
            let x = rng.f01() * w;
            let y = rng.f01() * h;
            let r = (0.16 + rng.f01() * 0.22) * w;
            let col = if rng.f01() < 0.5 { VIOLET_DEEP } else { VIOLET };
            g.circle(
                Offset::new(x, y),
                r,
                Gradient::radial_fill().with_stops(&[
                    (0.0, alpha(col, 0.05)),
                    (1.0, alpha(col, 0.0)),
                ]),
            );
        }
    });

    // ── The lensed star field ──────────────────────────────────────────────
    let mut swallowed = 0usize;
    {
        let mut rng = Rng::new(0x57A2);
        for i in 0..STARS {
            let x = rng.f01() * w;
            let y = rng.f01() * h;
            let r0 = (x - c.dx).hypot(y - c.dy);
            // Stars behind the shadow do not exist for us.
            if r0 < R_SHADOW * 1.1 {
                swallowed += 1;
                continue;
            }
            let d = deflect(r0);
            let sx = c.dx + (x - c.dx) * d;
            let sy = c.dy + (y - c.dy) * d;
            // Skip if lensed out of frame.
            if sx < 0.0 || sx > w || sy < 0.0 || sy > h {
                continue;
            }
            let tw = 0.5 + 0.5 * (t * 2.4 + i as f32 * 1.93).sin();
            let base_a = 0.10 + 0.42 * rng.f01();
            let size = 0.4 + rng.f01() * 1.2;
            // Tangential smear where the lens is strong.
            if r0 < R_PHOTON * 2.4 {
                let ang = (y - c.dy).atan2(x - c.dx);
                let arc_len = 0.10 + (R_PHOTON * 2.4 / r0 - 1.0).max(0.0) * 0.22;
                let path = Path::arc(Offset::new(sx, sy), r0 * d, ang - arc_len, arc_len * 2.0);
                book.stroke(path, alpha(Color::WHITE, base_a * 0.8 * (0.6 + 0.4 * tw)), 1.0);
            } else {
                book.circle(Offset::new(sx, sy), size, alpha(Color::WHITE, base_a * (0.6 + 0.4 * tw)));
            }
        }
    }

    // ── The disk behind (far side), then the shadow, then the rest ────────
    // One Plus group carries the disk's light; ordering inside is painter's.
    book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
        // Far half first (it sits behind the shadow).
        draw_disk_half(g, &c, t, false, 0.85);
        // The halo: the far side's light bent over the shadow's top.
        draw_disk_arc(g, &c, t, 0.95);
    });

    // The shadow: pure black, opaque — the one thing that emits nothing.
    book.circle(c, R_SHADOW, Color::BLACK);

    // The photon ring: the light that orbited. Slightly anisotropic —
    // brighter on the beamed side.
    book.blended_layer(1.0, 1.5, BlendMode::Plus, None, |g| {
        let ring_path = Path::arc_ring(c, R_PHOTON, 2.6, 0.0, std::f32::consts::TAU);
        g.fill(ring_path, alpha(tint(AMBER, 0.55), 0.55));
        let hot = Path::arc_ring(
            c,
            R_PHOTON,
            3.4,
            std::f32::consts::PI * 0.75,
            std::f32::consts::PI * 0.8,
        );
        g.fill(hot, alpha(Color::WHITE, 0.75));
    });

    // Near half of the disk — drawn over the shadow's lower edge, the way
    // matter in front of the hole passes in front.
    book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
        draw_disk_half(g, &c, t, true, 1.0);
    });

    // ── The infall: a spiral of matter raining in ────────────────────────
    book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
        let mut rng = Rng::new(0x1F4);
        for k in 0..36 {
            let birth = rng.f01();
            let u = ((t * 0.55 + birth) % 1.0 + k as f32 * 0.0277) % 1.0;
            // Spiral in: r from 3·R to the photon ring.
            let r = A_MAX * (1.0 - u) + R_PHOTON * (0.6 + u * 0.4);
            let th = u * 9.0 + k as f32 * 0.7;
            let px = c.dx + r * th.cos();
            let py = c.dy + r * th.sin() * (0.5 + 0.5 * (1.0 - u));
            let a = (1.0 - u) * 0.5;
            let col = mix(tint(AMBER, 0.4), Color::WHITE, (1.0 - u) * 0.4);
            g.circle(Offset::new(px, py), 1.2 + (1.0 - u) * 1.6, alpha(col, a));
            // Its little trail.
            let th2 = th - 0.22;
            let r2 = r * 1.045;
            g.line(
                Offset::new(px, py),
                Offset::new(c.dx + r2 * th2.cos(), c.dy + r2 * th2.sin() * (0.5 + 0.5 * (1.0 - u))),
                alpha(col, a * 0.4),
                1.0,
            );
        }
    });

    // The wash: the disk's inner glow bleeding around the shadow.
    book.layer(1.0, 26.0, None, |g| {
        g.circle(
            c,
            R_PHOTON * 3.4,
            Gradient::radial_fill().with_stops(&[
                (0.0, alpha(mix(AMBER, VIOLET, 0.35), 0.16)),
                (1.0, alpha(AMBER, 0.0)),
            ]),
        );
    });

    // The vignette.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::radial(Offset::new(0.5, 0.5), 0.85)
            .with_dither()
            .with_stops(&[
                (0.6, alpha(Color::BLACK, 0.0)),
                (1.0, alpha(Color::BLACK, 0.55)),
            ]),
    );
}

/// Draw one half of the accretion disk. `near` selects the half in front of
/// the shadow (orbit angle where sin θ > 0) or behind.
fn draw_disk_half(g: &mut Sketchbook, c: &Offset, t: f32, near: bool, gain: f32) {
    let mut rng = Rng::new(0xD15C);
    let _ = &mut rng;
    for i in 0..DISK {
        // Deterministic orbit parameters, hashed from the index.
        let h1 = (i as u32).wrapping_mul(0x9E3779B1).rotate_left(13) as f32 / u32::MAX as f32;
        let h2 = (i as u32).wrapping_mul(0x85EBCA6B).rotate_left(7) as f32 / u32::MAX as f32;
        let h3 = (i as u32).wrapping_mul(0xC2B2AE35).rotate_left(17) as f32 / u32::MAX as f32;
        // Radius: denser at the inner rim (u^0.6 sampling).
        let u = (i as f32 / DISK as f32).powf(0.62);
        let a = A_MIN + (A_MAX - A_MIN) * u;
        // Keplerian rate: ω ∝ a^-3/2, normalised so the inner edge turns
        // OMEGA_IN times per experiment.
        let omega = OMEGA_IN * (A_MIN / a).powf(1.5);
        let th0 = h1 * std::f32::consts::TAU;
        let th = th0 + omega * t * std::f32::consts::TAU;
        let s = th.sin();
        if near != (s > 0.0) {
            continue;
        }
        let px = c.dx + a * th.cos();
        let py = c.dy + a * s * INCL;
        // Doppler beaming: the approaching side (moving toward the viewer)
        // is brighter. Orbital velocity direction is (-sin, cos·incl).
        let beam = 1.0 + 0.85 * (-th.sin());
        // Temperature by radius + twinkle.
        let tw = 0.75 + 0.25 * (t * 5.0 + h2 * 6.28).sin();
        let bright = (0.34 + 0.5 * (1.0 - u)) * beam * tw * gain;
        let col = temp_color(u);
        let r = 0.9 + h3 * 1.5 + (1.0 - u) * 0.8;
        g.circle(Offset::new(px, py), r, alpha(col, bright.min(1.0)));
        if bright > 0.75 {
            g.circle(Offset::new(px, py), r * 2.6, alpha(col, (bright - 0.75) * 0.5));
        }
    }
}

/// The far side's light, bent over the shadow's top — the halo arc.
fn draw_disk_arc(g: &mut Sketchbook, c: &Offset, t: f32, gain: f32) {
    for i in 0..DISK / 2 {
        let h1 = (i as u32).wrapping_mul(0x9E3779B1).rotate_left(13) as f32 / u32::MAX as f32;
        let h2 = (i as u32).wrapping_mul(0x85EBCA6B).rotate_left(7) as f32 / u32::MAX as f32;
        let u = (i as f32 / (DISK / 2) as f32).powf(0.62);
        let a = A_MIN + (A_MAX - A_MIN) * u;
        let omega = OMEGA_IN * (A_MIN / a).powf(1.5);
        let th = h1 * std::f32::consts::TAU + omega * t * std::f32::consts::TAU;
        let s = th.sin();
        if s >= 0.0 {
            continue; // only the far half bends over the top
        }
        // The bent image: hugging the shadow's top, compressed radially.
        let x = c.dx + a * th.cos();
        let y = c.dy + a * s * INCL;
        let dx = x - c.dx;
        let dy = y - c.dy;
        let ang = dy.atan2(dx);
        // Above the centre only (the top arc).
        if dy >= 0.0 {
            continue;
        }
        let rr = R_PHOTON * 1.12 + (a - A_MIN) * 0.22;
        let px = c.dx + rr * ang.cos();
        let py = c.dy + rr * ang.sin();
        let beam = 1.0 + 0.85 * (-th.sin());
        let tw = 0.75 + 0.25 * (t * 5.0 + h2 * 6.28).sin();
        let bright = (0.30 + 0.42 * (1.0 - u)) * beam * tw * gain;
        let col = temp_color(u);
        g.circle(Offset::new(px, py), 0.8 + (1.0 - u) * 1.4, alpha(col, bright.min(1.0)));
    }
}

/// The frame.
pub fn frame(t: f32) -> WidgetNode {
    let paint = Painting::sized(
        crate::film_lib::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            scene(book, size, t);
        }),
    );

    let strip = Stack::new()
        .push(
            Positioned::new()
                .left(24.0)
                .top(20.0)
                .width(520.0)
                .height(18.0)
                .child(
                    Text::new("THE BLACK HOLE · the heavy light").style(
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
                .width(680.0)
                .height(18.0)
                .child(
                    Text::new(format!(
                        "{DISK} orbitals · ω ∝ a^-3/2 · {STARS} stars lensed · far side bent over the top · 3 Plus groups"
                    ))
                    .style(TextStyle::new(11.0).monospace().color(alpha(MUTED, 0.85))),
                ),
        );

    Stack::new()
        .push(Positioned::fill().child(paint))
        .push(strip)
        .into()
}
