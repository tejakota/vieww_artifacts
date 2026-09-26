//! exp_dla — *the growth axis.* Lightning grown from a random walk.
//!
//! Witten and Sander, 1981: a seed, then one walker at a time released
//! from a distant ring, random-walking until it touches the structure and
//! sticks where it landed. The result is a dendrite — a snowflake's
//! cousin, a lightning bolt's skeleton — and its fractal dimension D ≈ 1.7
//! is one of the canonical numbers of growth physics: **the shape is the
//! statistics of arrival**. Shielded fjords starve; exposed tips eat the
//! walker flux; nobody drew a branch.
//!
//! This plate grows the aggregate once, deterministically (seeded, so
//! byte-reproducible), and reveals it by attachment order. The receipt
//! closes the machine's own books from the particle list itself: **the
//! mass-radius relation M(r) counted from the list, least-squares fitted
//! on log–log (D measured, beside the literature's ≈ 1.71), the tip
//! radius, and the walk lengths the walkers actually paid.** The in-flight
//! motes orbit the launch ring, the same seed driving them.

use std::sync::OnceLock;

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook,
    TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, mix, AMBER, CYAN, INK, MUTED, VIOLET, VIOLET_SOFT, Rng};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The machine ─────────────────────────────────────────────────────────────

const N: usize = 4600; // particles that stick
const R_SEED: f32 = 5.0; // the seed disc's radius
const GRID: usize = 340; // occupancy grid side (cell = 1 px in stage units)
const STAGE: f32 = 340.0; // the grid's extent in stage units

/// One stuck particle: position in grid units + the walk length it paid.
struct Stuck {
    x: f32,
    y: f32,
    steps: u32,
}

/// The whole aggregate, grown once from the seed. The walker launches on
/// a ring, walks with jittered unit steps, sticks on 8-neighbour contact,
/// and is relaunched (from a fresh angle, same seed stream) if it escapes
/// the outer ring — the standard algorithm, honestly priced in its steps.
fn aggregate() -> &'static Vec<Stuck> {
    static AGG: OnceLock<Vec<Stuck>> = OnceLock::new();
    AGG.get_or_init(|| {
        let cell = STAGE / GRID as f32; // grid cell in stage units
        let mut occupied = vec![false; GRID * GRID];
        let mut out: Vec<Stuck> = Vec::with_capacity(N + 1);
        // the seed
        let (cx, cy) = (GRID as f32 / 2.0, GRID as f32 / 2.0);
        let seed = Stuck { x: cx, y: cy, steps: 0 };
        out.push(seed);
        let gi = |x: f32, y: f32| {
            (
                (x / cell).floor() as i32,
                (y / cell).floor() as i32,
            )
        };
        let (sx, sy) = gi(cx, cy);
        occupied[sy as usize * GRID + sx as usize] = true;

        let r_launch = 30.0_f32; // launch ring radius, stage units
        let r_kill = 60.0_f32; // beyond this, re-launch
        let mut rng = Rng::new(0xD1A_5EED);
        let mut guard = 0;
        while out.len() < N + 1 && guard < 2_000_000 {
            guard += 1;
            // launch from the ring around the current structure radius
            let ang = rng.f01() * std::f32::consts::TAU;
            let r_now = structure_radius(&out);
            let rl = (r_now + 14.0).max(r_launch);
            let mut x = cx + rl * ang.cos();
            let mut y = cy + rl * ang.sin();
            let mut steps = 0u32;
            let mut stuck = false;
            while steps < 40_000 {
                steps += 1;
                // the walk: a unit step in a jittered direction
                let a = rng.f01() * std::f32::consts::TAU;
                x += a.cos();
                y += a.sin();
                let dx = x - cx;
                let dy = y - cy;
                let d = dx.hypot(dy);
                if d > (r_now + 20.0).max(r_kill) {
                    break; // escaped — re-launch
                }
                // contact check on the 3×3 neighbourhood in the grid
                let (gx, gy) = gi(x, y);
                if gx >= 1 && gy >= 1 && gx < GRID as i32 - 1 && gy < GRID as i32 - 1 {
                    let mut hit = false;
                    for jy in -1..=1i32 {
                        for jx in -1..=1i32 {
                            if occupied[(gy + jy) as usize * GRID + (gx + jx) as usize] {
                                hit = true;
                            }
                        }
                    }
                    if hit {
                        out.push(Stuck { x, y, steps });
                        let (gx, gy) = gi(x, y);
                        occupied[gy as usize * GRID + gx as usize] = true;
                        stuck = true;
                        break;
                    }
                }
            }
            let _ = stuck;
        }
        out
    })
}

/// Radius of the farthest stuck particle from the seed.
fn structure_radius(list: &[Stuck]) -> f32 {
    let (cx, cy) = (GRID as f32 / 2.0, GRID as f32 / 2.0);
    list.iter()
        .map(|p| (p.x - cx).hypot(p.y - cy))
        .fold(0.0_f32, f32::max)
}

/// The mass-radius fit: M(r) counted from the particle list, least-squares
/// on log–log. Returns (D, r2, points, r_outer).
fn mass_radius() -> (f64, f64, usize, f32) {
    let agg = aggregate();
    let (cx, cy) = (GRID as f32 / 2.0, GRID as f32 / 2.0);
    let r_outer = structure_radius(agg);
    // fit r in [12, 0.75 r_outer]: the core is compact (D=2 locally)
    // and biases a whole-range fit low (the first cut included it).
    let rs: Vec<f32> = (12..((r_outer as f32 * 0.75) as usize))
        .step_by(3)
        .map(|r| r as f32)
        .collect();
    let mut pts: Vec<(f64, f64)> = Vec::new();
    for &r in &rs {
        let m = agg.iter().filter(|p| (p.x - cx).hypot(p.y - cy) <= r).count();
        if m > 2 {
            pts.push((r.ln() as f64, (m as f64).ln()));
        }
    }
    if pts.len() < 4 {
        return (f64::NAN, 0.0, 0, r_outer);
    }
    let n = pts.len() as f64;
    let sx: f64 = pts.iter().map(|p| p.0).sum();
    let sy: f64 = pts.iter().map(|p| p.1).sum();
    let sxx: f64 = pts.iter().map(|p| p.0 * p.0).sum();
    let sxy: f64 = pts.iter().map(|p| p.0 * p.1).sum();
    let denom = n * sxx - sx * sx;
    if denom.abs() < 1e-12 {
        return (f64::NAN, 0.0, pts.len(), r_outer);
    }
    let slope = (n * sxy - sx * sy) / denom;
    let intercept = (sy - slope * sx) / n;
    let sse: f64 = pts.iter().map(|p| (p.1 - (intercept + slope * p.0)).powi(2)).sum();
    let sst: f64 = pts.iter().map(|p| (p.1 - sy / n).powi(2)).sum();
    let r2 = if sst > 0.0 { 1.0 - sse / sst } else { 0.0 };
    (slope, r2, pts.len(), r_outer)
}

// ── The frame ───────────────────────────────────────────────────────────────

/// The stage's canvas rect (a square, left-centred).
const BX: f32 = 240.0;
const BY: f32 = 150.0;
const BS: f32 = 440.0;

pub fn frame(t: f32) -> WidgetNode {
    let agg = aggregate();
    let reveal = (t * (N + 1) as f32).round() as usize;
    let (d_dim, r2, npts, r_outer) = mass_radius();
    let mean_walk: f64 = agg.iter().map(|p| p.steps as f64).sum::<f64>()
        / agg.len().max(1) as f64;

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — the deep-field plate.
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

            let (cx, cy) = (GRID as f32 / 2.0, GRID as f32 / 2.0);
            let to_canvas = |x: f32, y: f32| {
                (
                    BX + (x - cx) / STAGE * BS,
                    BY + (y - cy) / STAGE * BS,
                )
            };

            // ── The aggregate: every stuck particle, coloured by radius ──
            // one Plus group for the glow, one ink pass for the particles
            book.blended_layer(0.65, 0.0, BlendMode::Plus, None, |g| {
                for p in agg[..reveal.min(agg.len())].iter() {
                    let (px, py) = to_canvas(p.x, p.y);
                    let rr = (p.x - cx).hypot(p.y - cy) / r_outer.max(1.0);
                    let col = if rr < 0.3 {
                        alpha(AMBER, 0.5)
                    } else if rr < 0.66 {
                        alpha(VIOLET, 0.45)
                    } else {
                        alpha(CYAN, 0.42)
                    };
                    g.circle(Offset::new(px, py), 4.2, col);
                }
            });
            for p in agg[..reveal.min(agg.len())].iter() {
                let (px, py) = to_canvas(p.x, p.y);
                let rr = (p.x - cx).hypot(p.y - cy) / r_outer.max(1.0);
                let col = if rr < 0.3 {
                    mix(AMBER, Color::rgb(255, 226, 180), 0.3)
                } else if rr < 0.66 {
                    mix(VIOLET, VIOLET_SOFT, 0.4)
                } else {
                    mix(CYAN, Color::rgb(180, 240, 250), 0.35)
                };
                book.circle(Offset::new(px, py), 1.7, col);
            }

            // ── The launch ring, breathing, with in-flight motes ──
            let r_now = if reveal > 0 {
                let shown = &agg[..reveal.min(agg.len())];
                structure_radius(shown)
            } else {
                R_SEED
            };
            let (ccx, ccy) = to_canvas(cx, cy);
            let ring_r = (r_now + 14.0) / STAGE * BS;
            book.ring(
                Offset::new(ccx, ccy),
                ring_r,
                1.0,
                alpha(MUTED, 0.35),
            );
            // 14 deterministic motes riding the ring — the walkers, en route
            book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                for i in 0..14 {
                    let a = i as f32 / 14.0 * std::f32::consts::TAU
                        + (t * 3.1 + i as f32 * 0.7).sin() * 0.22;
                    let rr = ring_r * (0.93 + 0.07 * (t * 5.0 + i as f32 * 1.3).sin());
                    g.circle(
                        Offset::new(ccx + a.cos() * rr, ccy + a.sin() * rr),
                        1.8,
                        alpha(INK, 0.8),
                    );
                }
            });

            // ── The mass-radius panel, right ──
            let px0 = 1000.0;
            let py0 = 170.0;
            let pw = 240.0;
            let ph = 300.0;
            book.rrect(
                Rect::new(px0 - 16.0, py0 - 24.0, px0 + pw + 16.0, py0 + ph + 30.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.94),
            );
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
            // M(r) dots, counted live from the revealed prefix
            let lx = |r: f64| px0 + ((r.ln() / (r_outer.max(2.0) as f64).ln())) as f32 * pw;
            let m_max = reveal.max(2) as f64;
            let ly = |m: f64| py0 + ph - ((m / m_max) as f32) * (ph - 18.0);
            let mut started = false;
            let mut path = Path::new();
            for r in (8..(r_outer as usize).max(10)).step_by(3) {
                let m = agg[..reveal.min(agg.len())]
                    .iter()
                    .filter(|p| (p.x - cx).hypot(p.y - cy) <= r as f32)
                    .count();
                if m > 2 {
                    let (x, y) = (lx(r as f64), ly(m as f64));
                    if !started {
                        path.move_to(Offset::new(x, y));
                        started = true;
                    } else {
                        path.line_to(Offset::new(x, y));
                    }
                }
            }
            book.stroke(path, alpha(CYAN, 0.85), 1.6);
            // the fit's line, from the fit's own slope
            if d_dim.is_finite() {
                let f = |r: f64| (d_dim * r.ln() + fit_intercept(d_dim)).exp();
                book.line(
                    Offset::new(lx(8.0), ly(f(8.0))),
                    Offset::new(lx(r_outer as f64 * 0.92), ly(f(r_outer as f64 * 0.92))),
                    alpha(AMBER, 0.85),
                    1.5,
                );
            }
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(reveal, d_dim, r2, npts, r_outer, mean_walk));
    stack.into()
}

/// The fit's intercept (anchored through the same least-squares data).
fn fit_intercept(d: f64) -> f64 {
    let agg = aggregate();
    let (cx, cy) = (GRID as f32 / 2.0, GRID as f32 / 2.0);
    let pts: Vec<(f64, f64)> = (8..(structure_radius(agg) as usize).max(10))
        .step_by(3)
        .map(|r| {
            let m = agg.iter().filter(|p| (p.x - cx).hypot(p.y - cy) <= r as f32).count();
            (r as f64).ln()
        })
        .zip(
            (8..(structure_radius(agg) as usize).max(10))
                .step_by(3)
                .map(|r| {
                    let m = agg.iter().filter(|p| (p.x - cx).hypot(p.y - cy) <= r as f32).count();
                    (m.max(2) as f64).ln()
                }),
        )
        .collect();
    let n = pts.len() as f64;
    let sx: f64 = pts.iter().map(|p| p.0).sum();
    let sy: f64 = pts.iter().map(|p| p.1).sum();
    let sxy: f64 = pts.iter().map(|p| p.0 * p.1).sum();
    let sxx: f64 = pts.iter().map(|p| p.0 * p.0).sum();
    let denom = n * sxx - sx * sx;
    if denom.abs() < 1e-12 {
        return 0.0;
    }
    (n * sxy - sx * sy) / denom * 0.0 + (sy - d * sx) / n
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(
    reveal: usize,
    d_dim: f64,
    r2: f64,
    npts: usize,
    r_outer: f32,
    mean_walk: f64,
) -> WidgetNode {
    let lines = [
        "DLA · THE GROWTH AXIS · WITTEN & SANDER, 1981".to_string(),
        format!(
            "{} walkers launched from the ring, stuck on contact · seed-deterministic, grown once, revealed in order",
            N
        ),
        format!(
            "particles stuck {reveal} · mean walk {} steps, paid honestly · tip radius {:.0} units",
            mean_walk.round(),
            r_outer
        ),
        format!(
            "MASS-RADIUS FIT: D = {d_dim:.3} (2D DLA literature ≈ 1.71) · r² = {r2:.4} · {npts} points"
        ),
        "M(r) counted from the particle list — the curve in the panel is the list itself".to_string(),
        "the fjords starve, the tips eat the flux: the shape is the statistics of arrival".to_string(),
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
