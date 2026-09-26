//! exp_sandpile — *the criticality axis II.* The machine that tunes itself.
//!
//! Bak, Tang and Wiesenfeld, 1987: drop grains one at a time on a lattice,
//! topple any cell that reaches four, let the grains that reach the edge
//! leave — and the pile organises itself to the angle where avalanches of
//! every size occur, with no parameter tuned by anyone. **Self-organised
//! criticality: the fingerprint is a power law** — avalanche sizes s with
//! frequency ∝ s^(−τ). This plate is that machine, exactly: a 160×90
//! lattice, 5,000 grains, every drop replayed from the seed every frame
//! (the pile at t is a pure function of the drop count), the currently
//! toppling cells flashed in white, and the log-log histogram of the whole
//! run's avalanches building in the corner.
//!
//! The receipt closes the machine's own books: **the exponent τ fitted by
//! least squares on the binned avalanche histogram the replay itself
//! produced** (2D BTW theory: τ ≈ 1.0 in the infinite-lattice limit — this
//! finite, open-boundary machine prints its own number beside it), the
//! largest avalanche, the mean, and the grains the edges took. No number
//! typed by a human.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, mix, AMBER, CYAN, INK, MUTED, VIOLET, Rng};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The machine ─────────────────────────────────────────────────────────────

/// The lattice.
const GX: usize = 96;
const GY: usize = 54;

/// Total grains dropped over the film (the machine's whole history) —
/// 12,000 on a 96×54 lattice: past the point where the pile spans the
/// system and the edges start taking grains (the steady state; at 5,000
/// the edges still held everything and the machine was still building).
const GRAINS: usize = 12000;

/// Drop target: centre with deterministic jitter.
fn drop_xy(i: usize) -> (usize, usize) {
    let mut rng = Rng::new(0x5A1D_0000 + i as u64);
    let jx = rng.sym() * 2.2;
    let jy = rng.sym() * 2.2;
    (
        ((GX as f32 / 2.0 + jx) as usize).clamp(1, GX - 2),
        ((GY as f32 / 2.0 + jy) as usize).clamp(1, GY - 2),
    )
}

/// One replay of the whole machine up to `grains` drops. Returns the pile
/// (heights), the cells toppled by the LAST drop (the live avalanche), and
/// the number of grains that left the edges during the replay.
fn replay(grains: usize) -> (Vec<u8>, Vec<usize>, usize) {
    let mut h = vec![0u8; GX * GY];
    let mut last_cells = Vec::new();
    let mut lost = 0usize;
    for g in 0..grains.min(GRAINS) {
        let (x, y) = drop_xy(g);
        h[y * GX + x] += 1;
        // BTW toppling: any cell ≥ 4 gives one grain to each neighbour.
        // Cells on the open boundary hand grains to the void — the
        // dissipation that lets the pile find its own critical slope.
        let mut cells = Vec::new();
        let mut stack = vec![(x, y)];
        while let Some((cx, cy)) = stack.pop() {
            let idx = cy * GX + cx;
            if h[idx] < 4 {
                continue;
            }
            h[idx] -= 4;
            cells.push(idx);
            // four neighbours; edge cells lose grains to the void
            if cx + 1 < GX {
                h[cy * GX + cx + 1] += 1;
                if h[cy * GX + cx + 1] >= 4 {
                    stack.push((cx + 1, cy));
                }
            } else {
                lost += 1;
            }
            if cx > 0 {
                h[cy * GX + cx - 1] += 1;
                if h[cy * GX + cx - 1] >= 4 {
                    stack.push((cx - 1, cy));
                }
            } else {
                lost += 1;
            }
            if cy + 1 < GY {
                h[(cy + 1) * GX + cx] += 1;
                if h[(cy + 1) * GX + cx] >= 4 {
                    stack.push((cx, cy + 1));
                }
            } else {
                lost += 1;
            }
            if cy > 0 {
                h[(cy - 1) * GX + cx] += 1;
                if h[(cy - 1) * GX + cx] >= 4 {
                    stack.push((cx, cy - 1));
                }
            } else {
                lost += 1;
            }
        }
        last_cells = cells;
    }
    (h, last_cells, lost)
}

/// The avalanche sizes over the FULL history — the histogram's own data.
fn avalanche_sizes() -> Vec<usize> {
    let mut h = vec![0u8; GX * GY];
    let mut sizes = Vec::with_capacity(GRAINS);
    for g in 0..GRAINS {
        let (x, y) = drop_xy(g);
        h[y * GX + x] += 1;
        let mut size = 0usize;
        let mut stack = vec![(x, y)];
        while let Some((cx, cy)) = stack.pop() {
            let idx = cy * GX + cx;
            if h[idx] < 4 {
                continue;
            }
            h[idx] -= 4;
            size += 1;
            if cx + 1 < GX {
                h[cy * GX + cx + 1] += 1;
                if h[cy * GX + cx + 1] >= 4 {
                    stack.push((cx + 1, cy));
                }
            }
            if cx > 0 {
                h[cy * GX + cx - 1] += 1;
                if h[cy * GX + cx - 1] >= 4 {
                    stack.push((cx - 1, cy));
                }
            }
            if cy + 1 < GY {
                h[(cy + 1) * GX + cx] += 1;
                if h[(cy + 1) * GX + cx] >= 4 {
                    stack.push((cx, cy + 1));
                }
            }
            if cy > 0 {
                h[(cy - 1) * GX + cx] += 1;
                if h[(cy - 1) * GX + cx] >= 4 {
                    stack.push((cx, cy - 1));
                }
            }
        }
        sizes.push(size);
    }
    sizes
}

/// Bin the avalanche sizes by powers of 2^(1/2) and fit the exponent τ by
/// least squares on log–log. Returns (tau, r2, bins used, intercept) with
/// the fit carried in count-normalised space (n / peak), the same axes the
/// panel draws.
fn powerlaw_fit() -> (f64, f64, usize, f64) {
    let sizes = avalanche_sizes();
    // bins: s in [2^(k/2), 2^((k+1)/2))
    let mut bins: Vec<(f64, f64)> = vec![(0.0, 0.0); 26]; // (centre, count)
    for &s in &sizes {
        if s == 0 {
            continue;
        }
        let k = ((s as f64).log2() * 2.0).floor() as usize;
        if k < bins.len() {
            bins[k].1 += 1.0;
        }
    }
    for (k, b) in bins.iter_mut().enumerate() {
        b.0 = 2f64.powf(k as f64 / 2.0 + 0.25);
    }
    let peak = bins.iter().map(|b| b.1).fold(0.0_f64, f64::max).max(1.0);
    // DENSITY fit — and the normalisation is the whole story: the count
    // per half-octave bin must be divided by the bin width, which is
    // proportional to the bin's centre. The first cut fitted raw counts
    // and printed τ = 0.28 beside the textbook's 1.25, while the
    // histogram itself was carrying the law perfectly — flat counts
    // across four decades of log bins IS the 1/s density. A receipt must
    // normalise before it quotes an exponent.
    let pts: Vec<(f64, f64)> = bins
        .iter()
        .filter(|(_, c)| *c >= 8.0)
        .map(|&(c, n)| (c.ln(), (n / c).ln()))
        .filter(|&(_, ln)| ln.is_finite())
        .collect();
    let _ = peak;
    if pts.len() < 4 {
        return (f64::NAN, 0.0, 0, 0.0);
    }
    let n = pts.len() as f64;
    let sx: f64 = pts.iter().map(|p| p.0).sum();
    let sy: f64 = pts.iter().map(|p| p.1).sum();
    let sxx: f64 = pts.iter().map(|p| p.0 * p.0).sum();
    let sxy: f64 = pts.iter().map(|p| p.0 * p.1).sum();
    let denom = n * sxx - sx * sx;
    if denom.abs() < 1e-12 {
        return (f64::NAN, 0.0, pts.len(), 0.0);
    }
    let slope = (n * sxy - sx * sy) / denom;
    let intercept = (sy - slope * sx) / n;
    let sse: f64 = pts.iter().map(|p| (p.1 - (intercept + slope * p.0)).powi(2)).sum();
    let sst: f64 = pts.iter().map(|p| (p.1 - sy / n).powi(2)).sum();
    let r2 = if sst > 0.0 { 1.0 - sse / sst } else { 0.0 };
    // τ is the NEGATIVE of the log-log slope (N ∝ s^-tau)
    (-slope, r2, pts.len(), intercept)
}

// ── The frame ───────────────────────────────────────────────────────────────

/// The lattice stage, in canvas coordinates.
const X0: f32 = 60.0;
const Y0: f32 = 130.0;
const CW: f32 = 9.2;
const CH: f32 = 9.2;

pub fn frame(t: f32) -> WidgetNode {
    let drops = (t * GRAINS as f32).round() as usize;
    let (pile, live, lost) = replay(drops);

    let (tau, r2, nbins, fit_intercept) = powerlaw_fit();
    let sizes = avalanche_sizes();
    let max_av = sizes.iter().copied().max().unwrap_or(0);
    let mean_av: f64 = sizes.iter().sum::<usize>() as f64 / GRAINS as f64;

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — an instrument room at dusk.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(7, 7, 10)),
                    (1.0, Color::rgb(12, 12, 17)),
                ]),
            );

            // ── The pile, one rect per cell, colour = local slope ──
            let live_set: Vec<bool> = {
                let mut v = vec![false; GX * GY];
                for &c in &live {
                    if c < v.len() {
                        v[c] = true;
                    }
                }
                v
            };
            for y in 0..GY {
                for x in 0..GX {
                    let idx = y * GX + x;
                    let hv = pile[idx];
                    // the slope the eye reads: difference to the strongest
                    // neighbour (down-slope), i.e. the local relief
                    let nb = [
                        pile[idx.saturating_sub(1)],
                        pile[(idx + 1).min(GX * GY - 1)],
                        pile[idx.saturating_sub(GX)],
                        pile[(idx + GX).min(GX * GY - 1)],
                    ];
                    let relief = nb.iter().fold(0i32, |m, &n| m.max(hv as i32 - n as i32));
                    if !live_set[idx] && hv == 0 {
                        continue; // the empty board stays the ground's own dark
                    }
                    let col = if live_set[idx] {
                        Color::rgb(246, 246, 250) // the live avalanche, white-hot
                    } else if relief <= 0 {
                        mix(Color::rgb(22, 20, 34), VIOLET_DEEPISH, 0.5) // plateau
                    } else if relief == 1 {
                        mix(Color::rgb(28, 24, 44), VIOLET_DEEPISH, 0.65)
                    } else if relief == 2 {
                        mix(VIOLET_DEEPISH, VIOLET, 0.55)
                    } else {
                        mix(AMBER, Color::rgb(255, 224, 178), 0.35)
                    };
                    book.rect(
                        Rect::new(
                            X0 + x as f32 * CW,
                            Y0 + y as f32 * CH,
                            X0 + (x + 1) as f32 * CW + 0.45,
                            Y0 + (y + 1) as f32 * CH + 0.45,
                        ),
                        col,
                    );
                }
            }

            // The pile's rim light — a soft Plus halo over the cone.
            book.blended_layer(0.5, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                let cx = X0 + GX as f32 * CW * 0.5;
                let cy = Y0 + GY as f32 * CH * 0.5;
                g.ring(
                    Offset::new(cx, cy),
                    (GX as f32 * CW * 0.62).min(420.0),
                    34.0,
                    alpha(VIOLET, 0.10),
                );
            });

            // ── The log-log histogram panel, right ──
            let px0 = 960.0;
            let py0 = 150.0;
            let pw = 280.0;
            let ph = 340.0;
            book.rrect(
                Rect::new(px0 - 16.0, py0 - 26.0, px0 + pw + 16.0, py0 + ph + 44.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.94),
            );
            // axes
            book.line(
                Offset::new(px0, py0 + ph),
                Offset::new(px0 + pw, py0 + ph),
                alpha(MUTED, 0.45),
                1.0,
            );
            book.line(
                Offset::new(px0, py0 + ph),
                Offset::new(px0, py0),
                alpha(MUTED, 0.45),
                1.0,
            );
            // x: log s from 1 to 4096; y: log N (normalised) — histogram dots
            let lx = |s: f64| px0 + ((s.ln() / 8.0).clamp(0.0, 1.0) * pw as f64) as f32;
            let bins: Vec<(f64, f64)> = {
                let mut b = vec![0f64; 26];
                for &s in &sizes {
                    if s == 0 {
                        continue;
                    }
                    let k = ((s as f64).log2() * 2.0).floor() as usize;
                    if k < b.len() {
                        b[k] += 1.0;
                    }
                }
                (0..b.len())
                    .map(|k| (2f64.powf(k as f64 / 2.0 + 0.25), b[k]))
                    .collect()
            };
            let peak = bins.iter().map(|b| b.1).fold(0.0_f64, f64::max).max(1.0);
            let ly = |n: f64| (py0 + ph - ((n / peak).clamp(0.0, 1.0) as f32) * (ph - 30.0));
            // the same axes, for values already in normalised space (the fit)
            let lyv = |v: f64| (py0 + ph - (v.clamp(0.0, 1.0) as f32) * (ph - 30.0));
            for &(s, n) in bins.iter() {
                if n < 1.0 {
                    continue;
                }
                book.circle(
                    Offset::new(lx(s), ly(n)),
                    if n / peak > 0.5 { 3.0 } else { 2.2 },
                    alpha(CYAN, 0.85),
                );
            }
            // the fitted power law, drawn from the fit's own numbers
            if tau.is_finite() {
                let f = |s: f64| (fit_intercept - tau * s.ln()).exp();
                book.line(
                    Offset::new(lx(3.0), lyv(f(3.0))),
                    Offset::new(lx(1024.0), lyv(f(1024.0))),
                    alpha(AMBER, 0.85),
                    1.5,
                );
            }
            // tick labels ride the overlay panel below.

            // the drop counter, a thin progress spine under the lattice
            let spine_y = Y0 + GY as f32 * CH + 18.0;
            book.rrect(
                Rect::new(X0, spine_y, X0 + GX as f32 * CW, spine_y + 3.0),
                1.5,
                Color::rgb(24, 24, 32),
            );
            book.rrect(
                Rect::new(X0, spine_y, X0 + GX as f32 * CW * t, spine_y + 3.0),
                1.5,
                alpha(VIOLET, 0.9),
            );
            let _ = &sizes; // the histogram's own data, drawn above
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(drops, lost, tau, r2, nbins, max_av, mean_av));
    stack.into()
}

const VIOLET_DEEPISH: Color = Color::rgb(70, 46, 130);

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(
    drops: usize,
    lost: usize,
    tau: f64,
    r2: f64,
    nbins: usize,
    max_av: usize,
    mean_av: f64,
) -> WidgetNode {
    let lines = [
        "SANDPILE · THE CRITICALITY AXIS II · SELF-ORGANISED".to_string(),
        format!(
            "BTW 1987 · {GX}×{GY} open lattice · {GRAINS} grains · every drop replayed from the seed"
        ),
        format!(
            "grains dropped {drops} · left at the edges {lost} · pile held {} · live avalanche flashed white",
            drops.saturating_sub(lost)
        ),
        format!(
            "AVALANCHE POWER LAW: τ = {tau:.3} (2D BTW size exponent ≈ 1.25, Ktitarev et al.) · r² = {r2:.4} · {nbins} bins"
        ),
        format!(
            "mean avalanche {mean_av:.1} topples · largest {max_av} · sizes from the same replay that drew the frame"
        ),
        "no parameter was tuned — the pile found its own critical slope".to_string(),
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
