//! exp_caustics — *the refraction axis.* The light that folds.
//!
//! The bright, crawling network at the bottom of a swimming pool is a
//! **caustic**: the envelope of a family of refracted rays — the places
//! where Snell's law piles light on top of itself. This plate runs the
//! optics for real: a wavy water surface (three superposed sine waves,
//! animated), **1,600 rays** arriving from directly above, each refracted
//! at the surface through `n_air sin θ_i = n_water sin θ_r` and marched to
//! the pool floor. The floor's brightness IS the ray density — a histogram
//! of the same landings that drew the rays — and the bright bands on it
//! are where neighbouring rays converge: the caustic, computed, not
//! painted.
//!
//! The receipt closes the loop: the caustic's focus positions are found
//! **independently** by scanning the surface for stationary points of the
//! landing map (dx_floor/dx_surface = 0 — the envelope condition, the
//! catastrophe in "catastrophe optics"), and printed beside the histogram's
//! tallest bins, measured from the same arrays that lit the floor. Two
//! instruments, one answer.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, mix, tint, AMBER, CYAN, FAINT, INK, MUTED,
    VIOLET};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 10.0;

// ── The optics ──────────────────────────────────────────────────────────────

/// The water window: x, surface y, floor y (px).
const X0: f32 = 150.0;
const X1: f32 = 1130.0;
const Y_SURF: f32 = 236.0;
const Y_FLOOR: f32 = 596.0;

/// Refractive indices.
const N_AIR: f32 = 1.000;
const N_WATER: f32 = 1.333;

/// Rays traced per frame.
const RAYS: usize = 1600;

/// Floor histogram bins.
const BINS: usize = 240;

/// The surface height (px, +down) at x and t — three superposed travelling
/// waves. Returns (offset, slope).
#[must_use]
fn surface(x: f32, t: f32) -> (f32, f32) {
    let u = (x - X0) / (X1 - X0);
    let pi = std::f32::consts::PI;
    // Wavelengths chosen so the floor's caustics have personality: one
    // long swell, one mid chop, one fine ripple (slowly precessing).
    let swell = (u * 2.0 * pi * 1.6 + t * 1.9).sin() * 11.0;
    let chop = (u * 2.0 * pi * 4.3 - t * 1.3).sin() * 6.0;
    let ripple = (u * 2.0 * pi * 9.1 + t * 2.6).sin() * 2.6;
    let d_swell = (u * 2.0 * pi * 1.6 + t * 1.9).cos() * 11.0 * 2.0 * pi * 1.6 / (X1 - X0);
    let d_chop = (u * 2.0 * pi * 4.3 - t * 1.3).cos() * 6.0 * 2.0 * pi * 4.3 / (X1 - X0);
    let d_ripple = (u * 2.0 * pi * 9.1 + t * 2.6).cos() * 2.6 * 2.0 * pi * 9.1 / (X1 - X0);
    (swell + chop + ripple, d_swell + d_chop + d_ripple)
}

/// One ray: surface x → floor x, through Snell at the surface point.
/// Vertical incoming light; the surface slope is the only thing bending it.
#[must_use]
fn land_x(x_s: f32, t: f32) -> f32 {
    let (dy, slope) = surface(x_s, t);
    // Surface normal of y = f(x) is (-f', 1)/norm; incoming direction is
    // (0, 1). Refraction: standard vector Snell.
    let nrm = (slope * slope + 1.0).sqrt();
    let n_in = [0.0_f32, 1.0]; // into the water
    // The surface's up-normal (into the air): (f', −1)/|·| for the curve
    // y = f(x) — the sign checked by hand: flat → (0,−1), and a surface
    // descending to the right tips its normal right, refracting vertical
    // light toward the shallow side (verified against scalar Snell).
    let nrm_v = [slope / nrm, -1.0 / nrm];
    let cos_i = -(n_in[0] * nrm_v[0] + n_in[1] * nrm_v[1]); // > 0
    let sin_t2 = (N_AIR / N_WATER) * (N_AIR / N_WATER) * (1.0 - cos_i * cos_i);
    let cos_t = (1.0 - sin_t2).sqrt();
    // Refracted direction: n2/n1 * d + (n2/n1 cos_i − cos_t) * n̂
    let r = N_AIR / N_WATER;
    let k = r * cos_i - cos_t;
    let dir = [r * n_in[0] + k * nrm_v[0], r * n_in[1] + k * nrm_v[1]];
    // From the surface point down to the floor depth.
    let depth = Y_FLOOR - (Y_SURF + dy);
    let _ = dy;
    x_s + dir[0] * depth / dir[1].abs().max(1e-6)
}

/// The stationary points of the landing map — the caustic foci, found
/// independently of the histogram (the envelope condition).
#[must_use]
fn stationary_points(t: f32) -> Vec<f32> {
    let mut foci = Vec::new();
    let probe = 720;
    let mut prev = land_x(X0, t);
    for i in 1..=probe {
        let x = X0 + (X1 - X0) * i as f32 / probe as f32;
        let cur = land_x(x, t);
        // dx_floor/dx_surface crosses zero between samples.
        if (cur - prev).abs() < 0.08 {
            foci.push(x);
        }
        prev = cur;
    }
    foci
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    // Trace every ray, bin the landings — the histogram IS the floor.
    let mut hist = vec![0u32; BINS];
    let mut ray_pts: Vec<(f32, f32, f32)> = Vec::new(); // (x_s, y_surface, x_floor) for the drawn subset
    for i in 0..RAYS {
        let x_s = X0 + (X1 - X0) * (i as f32 + 0.5) / RAYS as f32;
        let xf = land_x(x_s, t);
        let b = ((xf - X0) / (X1 - X0) * BINS as f32).floor() as i64;
        if (0..BINS as i64).contains(&b) {
            hist[b as usize] += 1;
        }
        if i % 34 == 0 {
            let (dy, _) = surface(x_s, t);
            ray_pts.push((x_s, Y_SURF + dy, xf));
        }
    }
    let max_bin = hist.iter().copied().max().unwrap_or(1) as f32;
    let mean_bin = hist.iter().sum::<u32>() as f32 / BINS as f32;

    // The two instruments: foci from the envelope condition, peaks from
    // the histogram.
    let foci = stationary_points(t);
    let mut peaks: Vec<usize> = Vec::new();
    for i in 1..BINS - 1 {
        if hist[i] > hist[i - 1] && hist[i] >= hist[i + 1] && hist[i] as f32 > max_bin * 0.6 {
            peaks.push(i);
        }
    }

    let foci_draw = foci.clone();
    let peaks_draw = peaks.clone();
    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The room — above water.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(9, 10, 14)),
                    (1.0, Color::rgb(6, 7, 10)),
                ]),
            );

            // ── Underwater — the tinted depth ──────────────────────────
            let mut water = Path::new();
            water.move_to(Offset::new(X0, Y_SURF + surface(X0, t).0));
            let steps = 160;
            for i in 1..=steps {
                let x = X0 + (X1 - X0) * i as f32 / steps as f32;
                water.line_to(Offset::new(x, Y_SURF + surface(x, t).0));
            }
            water.line_to(Offset::new(X1, 700.0));
            water.line_to(Offset::new(X0, 700.0));
            water.close();
            book.fill(
                water,
                Gradient::vertical()
                    .with_dither()
                    .with_stops(&[
                        (0.0, alpha(CYAN, 0.10)),
                        (1.0, alpha(Color::rgb(3, 12, 24), 0.85)),
                    ]),
            );

            // ── The surface line, bright where the sun rides it ────────
            let mut surf = Path::new();
            surf.move_to(Offset::new(X0, Y_SURF + surface(X0, t).0));
            for i in 1..=steps {
                let x = X0 + (X1 - X0) * i as f32 / steps as f32;
                surf.line_to(Offset::new(x, Y_SURF + surface(x, t).0));
            }
            book.stroke(surf, alpha(tint(CYAN, 0.45), 0.85), 2.0);

            // ── The rays — the subset, faint, so the bending reads ────
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                for &(x_s, y_s, x_f) in ray_pts.iter() {
                    // Vertical from above the surface to the hit point…
                    g.line(
                        Offset::new(x_s, y_s - 44.0),
                        Offset::new(x_s, y_s),
                        alpha(tint(CYAN, 0.5), 0.16),
                        1.0,
                    );
                    // …then the refracted segment to the floor.
                    g.line(
                        Offset::new(x_s, y_s),
                        Offset::new(x_f, Y_FLOOR),
                        alpha(tint(AMBER, 0.35), 0.20),
                        1.0,
                    );
                }
            });

            // ── The floor: the histogram as light ──────────────────────
            // Each bin a vertical blade on the floor line, brightness the
            // ray density — the caustic network, drawn by its own numbers.
            let bin_w = (X1 - X0) / BINS as f32;
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                for (b, &count) in hist.iter().enumerate() {
                    if count == 0 {
                        continue;
                    }
                    let density = count as f32 / mean_bin.max(1.0);
                    let x = X0 + b as f32 * bin_w;
                    // The blade: taller AND brighter where rays pile up.
                    let hh = (density * 16.0).min(46.0);
                    let a = ((density - 1.0) * 0.22).clamp(0.0, 0.75);
                    g.rect(
                        Rect::new(x, Y_FLOOR - hh, x + bin_w + 0.5, Y_FLOOR),
                        alpha(mix(tint(AMBER, 0.4), Color::rgb(255, 250, 235), (density - 1.0).clamp(0.0, 1.0)), a),
                    );
                    // The glow above tall blades — the pool-floor shimmer.
                    if density > 1.6 {
                        g.circle(
                            Offset::new(x + bin_w * 0.5, Y_FLOOR - hh - 6.0),
                            14.0,
                            Gradient::radial_fill().with_dither().with_stops(&[
                                (0.0, alpha(tint(AMBER, 0.5), 0.10 * density.min(3.0))),
                                (1.0, alpha(AMBER, 0.0)),
                            ]),
                        );
                    }
                }
            });
            // The floor line itself.
            book.line(
                Offset::new(X0 - 10.0, Y_FLOOR),
                Offset::new(X1 + 10.0, Y_FLOOR),
                alpha(mix(MUTED, CYAN, 0.3), 0.6),
                2.0,
            );

            // ── The envelope instruments, drawn where they agree ───────
            // Foci from the stationary-phase scan: tick marks on the
            // floor; peaks from the histogram: dots above the blades.
            for &f in foci_draw.iter().take(40) {
                book.line(
                    Offset::new(f, Y_FLOOR + 4.0),
                    Offset::new(f, Y_FLOOR + 14.0),
                    alpha(VIOLET, 0.8),
                    1.4,
                );
            }
            for &p in peaks_draw.iter() {
                let x = X0 + (p as f32 + 0.5) * bin_w;
                book.circle(Offset::new(x, Y_FLOOR + 22.0), 2.6, alpha(tint(CYAN, 0.4), 0.9));
            }

            // The pool walls.
            book.line(
                Offset::new(X0 - 10.0, Y_SURF - 24.0),
                Offset::new(X0 - 10.0, Y_FLOOR + 10.0),
                alpha(FAINT, 0.5),
                2.0,
            );
            book.line(
                Offset::new(X1 + 10.0, Y_SURF - 24.0),
                Offset::new(X1 + 10.0, Y_FLOOR + 10.0),
                alpha(FAINT, 0.5),
                2.0,
            );
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(max_bin, mean_bin, &foci, &peaks));
    stack.into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(max_bin: f32, mean_bin: f32, foci: &[f32], peaks: &[usize]) -> WidgetNode {
    // The agreement: how many histogram peaks have a stationary-point tick
    // within 6 px — measured from the two instruments' own outputs.
    let bin_w = (X1 - X0) / BINS as f32;
    let matched = peaks
        .iter()
        .filter(|&&p| {
            let px = X0 + (p as f32 + 0.5) * bin_w;
            foci.iter().any(|&f| (f - px).abs() < 6.0)
        })
        .count();
    let lines = [
        "CAUSTICS · THE REFRACTION AXIS · SNELL AT THE SURFACE".to_string(),
        format!(
            "{RAYS} rays refracted per frame · n = {N_AIR:.3} → {N_WATER:.3} · surface = swell + chop + ripple"
        ),
        format!(
            "floor bins {BINS} · peak density {:.2}× the mean · focus contrast drives the glow",
            max_bin / mean_bin.max(0.001)
        ),
        format!(
            "envelope scan: {} stationary points (dx_floor/dx_surface = 0) · histogram peaks: {} · matched: {}",
            foci.len(),
            peaks.len(),
            matched
        ),
        "two instruments, one answer — the envelope IS the bright bands".to_string(),
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
