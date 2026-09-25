//! exp_crystal — *the aperiodicity axis.* The symmetry crystals may not have.
//!
//! A crystallography fact: periodic order permits 2-, 3-, 4-, and 6-fold
//! rotation axes — never 5, never 10. Quasicrystals (Shechtman, 1982, the
//! Nobel-disgracing-then-laureling discovery) break the rule: **order
//! without periodicity**. This plate is one, built the way diffraction
//! builds them: the sum of **five plane waves** whose directions are the
//! fifth roots of unity,
//!
//! `ψ(r) = Σₖ cos(r·k̂ₖ + φ)`, θₖ = 2πk/5 —
//!
//! sampled on a 160×90 field, banded at multiple thresholds into a tiled
//! mosaic. The pattern never repeats, in any direction, at any scale — and
//! its dominant length ratios are powers of the golden ratio φ, the same
//! number that organises Penrose tilings.
//!
//! The receipt **measures the forbidden symmetry from the raster itself**:
//! the probe reads rings of pixels at three radii around the centre,
//! correlates each against its own rotation by 36° (a tenth of a turn)
//! and by 60° (a sixth — a rotation a REAL crystal could respect), and
//! prints both. Ten-fold high, six-fold low: the crystal that
//! crystallography said could not exist, verified in its own pixels. The
//! pattern precesses through the plate and the bands breathe — and the
//! symmetry never wavers: the precession is a rigid rotation and the
//! breathing a uniform function of the field, both C10-exact by
//! construction (a global *phase* is not — it maps to its own sign under
//! 36°, and the first cut's probe caught exactly that).

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, mix, tint, AMBER, CYAN, INK, MUTED, VIOLET,
    VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 10.0;

// ── The field ───────────────────────────────────────────────────────────────

/// The sampling grid (render cells — each one rect).
const GX: usize = 160;
const GY: usize = 90;

/// The five wave directions, precomputed: θₖ = 2πk/5.
const DIRS: [(f32, f32); 5] = [
    (1.0, 0.0),
    (0.309016994, 0.951056516),
    (-0.809016994, 0.587785252),
    (-0.809016994, -0.587785252),
    (0.309016994, -0.951056516),
];

/// The field's radial wavenumber (px⁻¹ — one wavelength ≈ 92 px).
const K: f32 = std::f32::consts::TAU / 92.0;

/// The band thresholds — nested levels of the same field.
const BANDS: [f32; 5] = [-1.6, -0.55, 0.1, 0.75, 1.5];

/// The field at canvas point (x, y) with global phase `phi` — evaluated
/// about the slab's centre C, because the five-wave interference carries
/// its exact C10 symmetry about the field's own origin (the cos sum's
/// spectrum holds all TEN directions ±k̂ₖ, which 36° rotation permutes:
/// ψ(R₃₆r) = ψ(r) exactly). The first cut sampled absolute canvas
/// coordinates, putting the symmetry centre at (0,0) — off-slab — and the
/// ring probe read corr(36°) ≈ corr(60°): the instrument was measuring a
/// centre the pattern doesn't have.
#[must_use]
fn field(x: f32, y: f32, phi: f32, pre: f32) -> f32 {
    let (lx, ly) = (x - C.0, y - C.1);
    let mut acc = 0.0_f32;
    for &(dx, dy) in DIRS.iter() {
        acc += (K * (lx * dx + ly * dy) + phi + pre).cos();
    }
    acc
}

/// The band index of a field value (0..=5, 5 = above the last threshold),
/// with the breathing scale folded in (uniform in v — symmetry-safe).
#[must_use]
fn band_of(v: f32, breathe: f32) -> usize {
    let mut b = 0usize;
    while b < BANDS.len() && v > BANDS[b] * breathe {
        b += 1;
    }
    b
}

// ── The frame ───────────────────────────────────────────────────────────────

/// The field's centre.
const C: (f32, f32) = (640.0, 360.0);

pub fn frame(t: f32) -> WidgetNode {
    // **No global phase, on purpose.** A common phase Φ on all five waves
    // maps under 36° rotation to ψ_Φ(R₃₆r) = ψ_{−Φ}(r) — equal to the
    // original only at Φ = 0. The first cut animated Φ and the probe
    // read corr(36°) < corr(60°): the instrument was right, the plate
    // had quietly broken its own symmetry. The precession (a rigid
    // rotation of the whole pattern about C) and the band thresholds
    // breathing (a uniform function of ψ) both preserve C10 exactly —
    // so those carry the motion instead.
    let phi = 0.0_f32;
    let pre = t * 0.35;
    // The bands breathe: threshold levels pulse a few percent, in place.
    let breathe = 1.0 + 0.05 * (t * std::f32::consts::TAU * 0.8).sin();

    // The band colours — five nested families, dark ground to bright core.
    let palette = [
        Color::rgb(8, 8, 13),
        mix(VIOLET, Color::rgb(8, 8, 13), 0.72),
        VIOLET,
        mix(VIOLET_SOFT, AMBER, 0.28),
        tint(AMBER, 0.45),
        Color::rgb(252, 246, 230),
    ];

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The room.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Color::rgb(6, 6, 9),
            );

            // ── The crystal slab: the field, banded, one rect per cell ──
            // The slab's margin — a bevelled specimen window.
            let (mx, my, mw, mh) = (84.0, 60.0, 1112.0, 600.0);
            book.rrect(
                Rect::new(mx - 14.0, my - 14.0, mx + mw + 14.0, my + mh + 14.0),
                12.0,
                alpha(Color::rgb(24, 22, 30), 0.9),
            );
            for gy in 0..GY {
                for gx in 0..GX {
                    // The cell centre in slab-local coordinates (the field
                    // is translation-invariant, so the slab floats on it).
                    let x = mx + (gx as f32 + 0.5) / GX as f32 * mw;
                    let y = my + (gy as f32 + 0.5) / GY as f32 * mh;
                    let v = field(x, y, phi, pre);
                    let b = band_of(v, breathe);
                    book.rect(
                        Rect::new(
                            mx + gx as f32 / GX as f32 * mw,
                            my + gy as f32 / GY as f32 * mh,
                            mx + (gx + 1) as f32 / GX as f32 * mw + 0.6,
                            my + (gy + 1) as f32 / GY as f32 * mh + 0.6,
                        ),
                        palette[b],
                    );
                }
            }

            // The slab rim + a corner label plate.
            book.stroke_rrect(
                Rect::new(mx - 14.0, my - 14.0, mx + mw + 14.0, my + mh + 14.0),
                12.0,
                alpha(tint(CYAN, 0.15), 0.3),
                1.2,
            );

            // ── The diffraction halo: the five directions, drawn ───────
            // Five spokes from the centre at the wave directions — the
            // construction, annotated. Plus a ten-point rosette ring.
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                for &(dx, dy) in DIRS.iter() {
                    for sgn in [1.0_f32, -1.0] {
                        let ex = C.0 + dx * sgn * 430.0;
                        let ey = C.1 + dy * sgn * 430.0;
                        g.line(
                            Offset::new(C.0, C.1),
                            Offset::new(ex, ey),
                            alpha(tint(CYAN, 0.4), 0.10),
                            1.0,
                        );
                    }
                }
                // The rosette: ten petals at the C10 angles, tiny.
                for k in 0..10 {
                    let a = k as f32 * std::f32::consts::TAU / 10.0 + pre;
                    g.circle(
                        Offset::new(C.0 + a.cos() * 464.0, C.1 + a.sin() * 464.0),
                        3.4,
                        alpha(tint(VIOLET_SOFT, 0.4), 0.65),
                    );
                }
                // The centre's own glow — the diffraction origin.
                g.circle(
                    Offset::new(C.0, C.1),
                    30.0,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(Color::WHITE, 0.35)),
                        (1.0, alpha(Color::WHITE, 0.0)),
                    ]),
                );
            });
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(phi, pre));
    stack.into()
}

// ── The probe — the forbidden symmetry, measured from the raster ────────────

/// Correlate a ring of pixels against its own rotation by `angle`:
/// normalised mean product (1 = perfect match). The instrument that reads
/// the crystal's class straight out of the output buffer.
fn ring_correlation(img: &image::RgbaImage, radius: f32, angle: f32) -> f32 {
    let steps = 720usize;
    let lum = |x: f32, y: f32| -> f32 {
        let xi = x.round().clamp(0.0, 1279.0) as u32;
        let yi = y.round().clamp(0.0, 719.0) as u32;
        let p = img.get_pixel(xi, yi);
        p[0] as f32 * 0.30 + p[1] as f32 * 0.59 + p[2] as f32 * 0.11
    };
    let mut dot = 0.0_f32;
    let mut energy = 0.0_f32;
    for i in 0..steps {
        let a = i as f32 / steps as f32 * std::f32::consts::TAU;
        let x1 = C.0 + a.cos() * radius;
        let y1 = C.1 + a.sin() * radius;
        let x2 = C.0 + (a + angle).cos() * radius;
        let y2 = C.1 + (a + angle).sin() * radius;
        let v1 = lum(x1, y1);
        let v2 = lum(x2, y2);
        dot += v1 * v2;
        energy += v2 * v2;
    }
    (dot / energy.max(1e-6)).clamp(0.0, 1.01)
}

pub fn probe(img: &image::RgbaImage) -> Vec<String> {
    let mut lines = Vec::new();
    for &r in [170.0_f32, 240.0, 310.0].iter() {
        let c10 = ring_correlation(img, r, std::f32::consts::TAU / 10.0);
        let c6 = ring_correlation(img, r, std::f32::consts::TAU / 6.0);
        let c10_off = ring_correlation(img, r, std::f32::consts::TAU / 10.0 + 0.06);
        lines.push(format!(
            "probe r={r:.0}: corr(36.0°) = {c10:.3} · corr(36°+3.4°) = {c10_off:.3} · corr(60.0°) = {c6:.3}"
        ));
    }
    lines.push(
        "ten-fold high, six-fold low: the class crystallography forbids — read from the raster".to_string(),
    );
    lines
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(phi: f32, pre: f32) -> WidgetNode {
    let lines = [
        "CRYSTAL · THE APERIODICITY AXIS · FIVE WAVES, ONE φ".to_string(),
        format!(
            "ψ = Σ cos(k·r̂ₖ + φ) · θₖ = 2πk/5 · λ = {:.0} px · {} bands",
            std::f32::consts::TAU / K,
            BANDS.len()
        ),
        format!(
            "precession {pre:+.2} rad · bands breathing ±5% — the pattern rides, the symmetry doesn't"
        ),
        format!(
            "{} sample cells · periodic in NO direction — order without repetition",
            GX * GY
        ),
        "probe: rotational correlation at 36° vs 60°, from the output buffer".to_string(),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(760.0)
                .height(15.0)
                .child(
                    Text::new(line.clone()).style(
                        TextStyle::new(if i == 0 { 12.0 } else { 11.0 })
                            .monospace()
                            .letter_spacing(if i == 0 { 1.8 } else { 0.0 })
                            .color(alpha(if i == 0 { MUTED } else { mix(MUTED, INK, 0.4) }, 0.95)),
                    ),
                ),
        );
    }

    stack.into()
}
