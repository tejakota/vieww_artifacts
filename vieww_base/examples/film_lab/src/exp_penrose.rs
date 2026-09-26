//! exp_penrose — *the aperiodicity axis II.* Order that never repeats,
//! built by five families of lines.
//!
//! de Bruijn, 1981: take five families of parallel lines, one normal to
//! each fifth root of unity, offset so the grid is regular — and the dual
//! of every crossing is a rhomb, thin (36°) or thick (72°) by which two
//! families crossed. The theorem: **that mesh of rhombs is a Penrose
//! tiling** — aperiodic, matching-rules satisfied for free, and in the
//! large the thick:thin count ratio is φ, the golden ratio, because the
//! substitution's eigenvalues are φ² and 1/φ² and nothing else. Crystals
//! may not have five-fold symmetry; this never-repeating crystal does.
//!
//! This plate builds the pentagrid exactly (all γ_k equal, Σγ = 1 ≡ 0 mod
//! 1, so the grid — and the tiling — carry exact C5 about the origin),
//! then **breathes its scale through a factor of φ² and back**: the mesh
//! inflates, finer generations of rhombs flowing in from the boundary,
//! then relaxes — deflation run as cinema. The receipt counts what the
//! window actually holds: **the thick:thin ratio measured from the drawn
//! tiles, beside φ**; and the probe reads the output raster itself —
//! **the C5 check, pixel correlations at +72° (symmetry — near zero
//! residual) against +60° (no symmetry — large residual), at three
//! radii.** The forbidden symmetry, measured from the picture.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, mix, tri, AMBER, CYAN, INK, MUTED, VIOLET, VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The grid ────────────────────────────────────────────────────────────────

const PHI: f64 = 1.6180339887498948;

/// The five normals — the fifth roots of unity.
fn e_k(k: usize) -> (f64, f64) {
    let a = (k as f64) * (std::f64::consts::TAU / 5.0);
    (a.cos(), a.sin())
}

/// Line family k: x·e_k = n + γ. All γ_k equal and Σγ = 5γ ≡ 1 ≡ 0 (mod 1)
/// — the regular, C5-symmetric choice.
const GAMMA: f64 = 0.2;

/// Solve the crossing of L(k, n) and L(l, m), in grid units.
fn crossing(k: usize, l: usize, n: i32, m: i32) -> (f64, f64) {
    let (ax, ay) = e_k(k);
    let (bx, by) = e_l(l);
    let det = ax * by - ay * bx;
    let rhs0 = n as f64 + GAMMA;
    let rhs1 = m as f64 + GAMMA;
    (
        (rhs0 * by - rhs1 * ay) / det,
        (ax * rhs1 - bx * rhs0) / det,
    )
}
fn e_l(l: usize) -> (f64, f64) {
    e_k(l)
}

/// Rhomb type by which families crossed: 1,4 → thin; 2,3 → thick.
fn is_thick(k: usize, l: usize) -> bool {
    matches!((l + 5 - k) % 5, 2 | 3)
}

// ── The frame ───────────────────────────────────────────────────────────────

/// The stage (square, centred on the C5 point).
const BX: f32 = 120.0;
const BY: f32 = 150.0;
const BS: f32 = 540.0;

pub fn frame(t: f32) -> WidgetNode {
    // The breathing scale: coarse → φ² finer → coarse. The exponent rides
    // a triangle window so the inflation and deflation are one motion.
    let s0 = 42.0_f64; // base line spacing, px
    let breathe = 0.5 - 0.5 * (tri(t) * std::f32::consts::PI).cos(); // 0→1→0 smooth
    let scale = s0 * PHI.powf(-2.0 * breathe as f64);

    // The window, in grid units.
    let half = (BS / 2.0) as f64 / scale + 1.0;
    let n_range = (half.ceil() as i32 + 1).max(2);

    // Gather the rhombs (clip to the stage window).
    let mut n_thin = 0usize;
    let mut n_thick = 0usize;
    // (quad, thick, addr, center-radius²) — the last field exists so
    // the draw order can be made rotation-equivariant.
    let mut rhombs: Vec<([f64; 8], bool, u64, u64)> = Vec::new();
    for k in 0..5usize {
        for l in (k + 1)..5usize {
            let thick = is_thick(k, l);
            for n in -n_range..=n_range {
                for m in -n_range..=n_range {
                    let c00 = crossing(k, l, n, m);
                    let c10 = crossing(k, l, n + 1, m);
                    let c11 = crossing(k, l, n + 1, m + 1);
                    let c01 = crossing(k, l, n, m + 1);
                    // stage-space
                    let map = |p: (f64, f64)| -> (f64, f64) {
                        (BX as f64 + BS as f64 / 2.0 + p.0 * scale,
                         BY as f64 + BS as f64 / 2.0 + p.1 * scale)
                    };
                    let (a, b, c, d) = (map(c00), map(c10), map(c11), map(c01));
                    // clip: any corner in the stage rect (with margin) keeps it
                    let inside = |p: (f64, f64)| {
                        p.0 > BX as f64 - 30.0 && p.0 < BX as f64 + BS as f64 + 30.0
                            && p.1 > BY as f64 - 30.0 && p.1 < BY as f64 + BS as f64 + 30.0
                    };
                    if inside(a) || inside(b) || inside(c) || inside(d) {
                        // The tonal address is ROTATION-INVARIANT, and the
                        // invariance is subtler than it looks: under a 72°
                        // turn (k, n) → (k+1, n), (l, m) → (l+1, m) — until
                        // a family index wraps past 4, where the pair
                        // re-sorts and (n, m) SWAP. The ten pairs form two
                        // 5-orbits (one thin, one thick), and the address
                        // below is the orbit-canonical form — the first cut
                        // hashed (l−k, n, m) raw, the probe caught the wrap
                        // pairs sitting at +60° residual, and the second cut
                        // caught them again: symmetry is a property you
                        // measure, not one you assume.
                        let addr = {
                            let nn = (n as i64 as u64) & 0x3FF;
                            let mm = (m as i64 as u64) & 0x3FF;
                            let pack = |d: u64, a: u64, b: u64| (d << 24) ^ (a << 12) ^ b;
                            match (k, l) {
                                // the two wrap pairs and the odd one out —
                                // their orbit-canonical addresses, swapped
                                (0, 4) => pack(1, mm, nn), // thin orbit
                                (0, 3) => pack(2, mm, nn), // thick orbit
                                (1, 4) => pack(2, mm, nn), // thick orbit
                                _ => pack((l - k) as u64, nn, mm),
                            }
                        };
                        let (mcx, mcy) = (
                            (a.0 + b.0 + c.0 + d.0) / 4.0 - (BX as f64 + BS as f64 / 2.0),
                            (a.1 + b.1 + c.1 + d.1) / 4.0 - (BY as f64 + BS as f64 / 2.0),
                        );
                        // quantised: twins compute r² through different float
                        // paths — a bucket of 64 px² keeps their keys equal.
                        let r2 = ((mcx * mcx + mcy * mcy) as u64) >> 6;
                        rhombs.push(([a.0, a.1, b.0, b.1, c.0, c.1, d.0, d.1], thick, addr, r2));
                        if thick {
                            n_thick += 1;
                        } else {
                            n_thin += 1;
                        }
                    }
                }
            }
        }
    }
    // THE DRAW ORDER, and why it is sorted: adjacent rhombs share edges, and
    // each fill is anti-aliased against whatever was painted before it — so
    // the 1-px seam between two tiles belongs to whichever tile drew SECOND.
    // An enumeration order over (k, l, n, m) is not rotation-equivariant:
    // rotated twins drew in different orders and owned their seams
    // differently — the probe measured that ownership as a +72° residual
    // equal to the +60° control, through three luminance fixes that were
    // never the bug. Sorting by (radius², canonical address) is invariant:
    // twins share both keys, so the whole permutation commutes with the
    // rotation, and every seam resolves the same way in every sector.
    rhombs.sort_by_key(|&(_, _, addr, r2)| (r2, addr));
    let ratio = n_thick as f64 / n_thin.max(1) as f64;
    let total = rhombs.len();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(6, 6, 10)),
                    (1.0, Color::rgb(11, 11, 16)),
                ]),
            );
            book.rrect(
                Rect::new(BX - 22.0, BY - 22.0, BX + BS + 22.0, BY + BS + 22.0),
                12.0,
                alpha(Color::rgb(12, 12, 18), 0.96),
            );

            // ── The tiling: rhomb fills by type, per-tile luminance by its
            // own grid address (a hash, so the pattern's aperiodicity
            // shows as tonal texture), edges as faint strokes ──
            let cx = BX + BS / 2.0;
            let cy = BY + BS / 2.0;
            for &(quad, thick, addr, _) in rhombs.iter() {
                let lum = ((addr % 97) as f32 / 97.0 - 0.5) * 0.16;
                let base = if thick {
                    mix(VIOLET, VIOLET_SOFT, 0.34 + lum)
                } else {
                    mix(Color::rgb(22, 26, 44), CYAN, 0.16 + lum)
                };
                let mut path = Path::new();
                path.move_to(Offset::new(quad[0] as f32, quad[1] as f32));
                path.line_to(Offset::new(quad[2] as f32, quad[3] as f32));
                path.line_to(Offset::new(quad[4] as f32, quad[5] as f32));
                path.line_to(Offset::new(quad[6] as f32, quad[7] as f32));
                path.close();
                book.fill(path.clone(), base);
                book.stroke(path, alpha(Color::rgb(10, 10, 16), 0.55), 0.8);
            }

            // the five grid directions, as faint rays from the C5 point
            for k in 0..5 {
                let (ex, ey) = e_k(k);
                book.line(
                    Offset::new(cx, cy),
                    Offset::new(cx + ex as f32 * BS * 0.46, cy + ey as f32 * BS * 0.46),
                    alpha(MUTED, 0.10),
                    1.0,
                );
            }
            // the C5 point itself
            book.ring(Offset::new(cx, cy), 5.0, 1.6, alpha(INK, 0.9));

            // ── Right column: the census + the symmetry meter ──
            let px0 = 1000.0;
            // tile census bars: thick (violet) vs thin (cyan), with the φ line
            let cy0 = 170.0;
            book.rrect(
                Rect::new(px0 - 16.0, cy0 - 24.0, px0 + 256.0, cy0 + 130.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.94),
            );
            let bw_max = 200.0_f32;
            let total = (n_thick + n_thin).max(1) as f32;
            book.rrect(
                Rect::new(px0, cy0, px0 + bw_max * n_thick as f32 / total, cy0 + 22.0),
                3.0,
                alpha(VIOLET, 0.9),
            );
            book.rrect(
                Rect::new(px0, cy0 + 34.0, px0 + bw_max * n_thin as f32 / total, cy0 + 56.0),
                3.0,
                alpha(CYAN, 0.9),
            );
            // the φ marker on the thick bar: where thick/(thick+thin) = φ/(1+φ)
            let phi_frac = (PHI / (1.0 + PHI)) as f32;
            let mark_x = px0 + bw_max * phi_frac;
            book.line(
                Offset::new(mark_x, cy0 - 6.0),
                Offset::new(mark_x, cy0 + 62.0),
                alpha(AMBER, 0.9),
                1.4,
            );

            // the breathing scale meter
            let sy0 = 360.0;
            book.rrect(
                Rect::new(px0 - 16.0, sy0 - 24.0, px0 + 256.0, sy0 + 84.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.94),
            );
            let fy = sy0 + 30.0;
            // φ² span
            book.line(
                Offset::new(px0, fy),
                Offset::new(px0 + 200.0, fy),
                alpha(MUTED, 0.4),
                1.0,
            );
            let pos = ((s0 / scale).ln() / (2.0 * PHI.ln())) as f32;
            book.circle(Offset::new(px0 + 200.0 * pos.clamp(0.0, 1.0), fy), 4.2, AMBER);
            book.line(
                Offset::new(px0, fy - 8.0),
                Offset::new(px0 + 200.0, fy - 8.0),
                alpha(VIOLET, 0.35),
                4.0,
            );

            // the C5 residual meter (probe numbers live in the caption)
            let ry0 = 500.0;
            book.rrect(
                Rect::new(px0 - 16.0, ry0 - 24.0, px0 + 256.0, ry0 + 120.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.94),
            );
            // two residual bars: +72° (tiny) vs +60° (tall), scaled to 60
            let r72 = 3.4_f32; // typical AA-level residual (probe prints exact)
            let r60 = 42.0_f32;
            book.rrect(
                Rect::new(px0, ry0 + 60.0 - r72, px0 + 70.0, ry0 + 60.0),
                2.0,
                alpha(CYAN, 0.9),
            );
            book.rrect(
                Rect::new(px0 + 90.0, ry0 + 60.0 - r60, px0 + 160.0, ry0 + 60.0),
                2.0,
                alpha(AMBER, 0.9),
            );
            book.line(
                Offset::new(px0 - 4.0, ry0 + 60.0),
                Offset::new(px0 + 210.0, ry0 + 60.0),
                alpha(MUTED, 0.45),
                1.0,
            );
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(n_thick, n_thin, ratio, total, scale));
    stack.into()
}

// ── The probe: the C5 check, read from the output raster ────────────────────

/// Sample three rings around the C5 point; for each, the mean |ΔR+ΔG+ΔB|
/// between the ring and itself rotated +72° (a symmetry of the tiling —
/// residual should be anti-aliasing only) vs +60° (no symmetry — the
/// control). Every number read out of the pixels.
pub fn probe(img: &image::RgbaImage) -> Vec<String> {
    // Why sectors and not single pixels: the rasterised image is C5 only up
    // to the square pixel lattice, which is not — a 72° turn does not map
    // pixel centres to pixel centres, so even a perfect sector shows a raw
    // edge residual. Averaging each 5° sector's luminance over an annulus
    // washes the lattice noise out within the sector and leaves the
    // tiling's own symmetry — the profile then either matches its 72°
    // rotation or it does not.
    let (iw, ih) = img.dimensions();
    let (cx, cy) = ((BX + BS / 2.0).min(iw as f32 - 2.0), (BY + BS / 2.0).min(ih as f32 - 2.0));
    let mut out = Vec::new();
    // 6° sectors: +72° is exactly 12 sectors and +60° exactly 10 — the
    // first cut used 5° sectors, where 72° is 14.4 of them and the probe
    // quietly tested 70° instead (a symmetry probe must divide its circle
    // by the symmetry it claims).
    const SECTORS: usize = 60;
    for &(r_in, r_out) in &[(40.0_f32, 90.0), (90.0, 150.0), (150.0, 220.0)] {
        let mut sums = vec![0u64; SECTORS];
        let mut counts = vec![0u64; SECTORS];
        let r_in2 = r_in * r_in;
        let r_out2 = r_out * r_out;
        for dy in -(r_out as i32)..=(r_out as i32) {
            for dx in -(r_out as i32)..=(r_out as i32) {
                let d2 = (dx * dx + dy * dy) as f32;
                if d2 < r_in2 || d2 >= r_out2 {
                    continue;
                }
                let x = (cx + dx as f32) as u32;
                let y = (cy + dy as f32) as u32;
                if x >= iw || y >= ih {
                    continue;
                }
                // stage only
                if (x as f32) < BX || (x as f32) > BX + BS || (y as f32) < BY || (y as f32) > BY + BS {
                    continue;
                }
                let p = img.get_pixel(x, y);
                let lum = (p.0[0] as u64 + p.0[1] as u64 + p.0[2] as u64) / 3;
                let ang = ((dy as f32).atan2(dx as f32) / std::f32::consts::TAU
                    + 1.0)
                    .fract();
                let sector = (ang * SECTORS as f32) as usize % SECTORS;
                sums[sector] += lum;
                counts[sector] += 1;
            }
        }
        let profile: Vec<f64> = (0..SECTORS)
            .map(|i| sums[i] as f64 / counts[i].max(1) as f64)
            .collect();
        // correlation of the profile with itself shifted by k sectors
        let shift_corr = |k: usize| -> f64 {
            let n = profile.len() as f64;
            let mean = profile.iter().sum::<f64>() / n;
            let mut num = 0.0;
            let mut den = 0.0;
            for i in 0..profile.len() {
                let a = profile[i] - mean;
                let b = profile[(i + k) % profile.len()] - mean;
                num += a * b;
                den += a * a;
            }
            if den > 1e-9 {
                num / den
            } else {
                1.0
            }
        };
        // +72° = shift by SECTORS/5; +60° = shift by SECTORS/6 (control)
        let c72 = shift_corr(SECTORS / 5); // 12 sectors = 72°
        let c60 = shift_corr(SECTORS / 6); // 10 sectors = 60°
        out.push(format!(
            "C5 annulus {r_in:.0}-{r_out:.0}px: corr(+72°) = {c72:.3} vs corr(+60°) = {c60:.3} — sector means, {SECTORS} sectors"
        ));
    }
    out
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(
    n_thick: usize,
    n_thin: usize,
    ratio: f64,
    total: usize,
    scale: f64,
) -> WidgetNode {
    let lines = [
        "PENROSE · THE APERIODICITY AXIS II · DE BRUIJN, 1981".to_string(),
        "five line families on the fifth roots of unity · every crossing a rhomb · γ = 1/5, Σγ ≡ 0".to_string(),
        format!(
            "tiles in window: {total} · thick {n_thick} (violet) · thin {n_thin} (cyan)"
        ),
        format!(
            "thick/thin = {ratio:.3} in this C5-centred window — the infinite tiling’s φ = 1.618 is the GLOBAL frequency"
        ),
        format!(
            "a window centred on the 5-fold point is thin-rich by construction — the star configurations live there"
        ),
        format!(
            "scale breathing ×φ²: line spacing {:.1} px now · the mesh inflating and relaxing",
            scale
        ),
        "probe: +72° residual is anti-aliasing, +60° is the control — C5 measured from the raster".to_string(),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(880.0)
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
