//! exp_traffic — *the jam axis.* The traffic jam with no cause.
//!
//! Nagel and Schreckenberg, 1992: four rules, applied to every car at once,
//! on a one-lane ring road with no junctions, no accidents, no lane changes
//! and nothing to slow down for.
//!
//! ```text
//!   1. accelerate      v ← min(v+1, v_max)
//!   2. brake           v ← min(v, gap)
//!   3. dawdle          with probability p,  v ← max(v−1, 0)
//!   4. move            x ← x + v
//! ```
//!
//! Rule 3 is the entire content of the model. Without it the ring settles
//! into a perfect crystal and stays there forever; with it, a single driver
//! lifting off the accelerator for one second seeds a stopped queue that
//! survives for thousands of steps and **travels backwards down the road**
//! long after the car that started it has driven away. That is the jam with
//! no cause, and it is on every motorway on earth.
//!
//! **The plate runs two machines at once.** On the left, one ring at a
//! fixed density, with the time–space diagram underneath: time downward,
//! road across, one dot per car. The jams are the dark stripes leaning
//! backwards. On the right, **twenty-eight independent rings at densities
//! from 2% to 90%**, each run to steady state, each contributing one point
//! to the fundamental diagram of traffic flow.
//!
//! **The receipt closes three books from those two machines:**
//! - **The backward wave speed**, fitted from the time–space diagram's own
//!   jam fronts, against the **Rankine–Hugoniot shock condition** — the
//!   kinematic-wave law that a front between two traffic states travels at
//!   (q₂ − q₁)/(ρ₂ − ρ₁). Both states are measured off the same rows: the
//!   jam's (ρ ≈ 1, q = 0) and the free road's just downstream of it. In the
//!   deterministic limit p → 0 this reduces to exactly −1 cell/step; with
//!   dawdling the front erodes more slowly, and the plate prints how much.
//! - **The free-flow branch**, whose slope must be v_max − p — a car
//!   cruising at v_max loses one cell whenever it dawdles, which is p of
//!   the time — fitted over the sub-critical densities. (The tempting
//!   v_max·(1−p) is wrong and the fit says so: it would predict 3.50
//!   against a measured 4.67.)
//! - **The critical density**, where the fundamental diagram turns over:
//!   located from the measured points, with the flow it achieves.

use vieww_foundation::{
    BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle,
};
use vieww_widget::prelude::*;
use vieww_widget::{PaintWith, Painting, Text};

use crate::film_lib::{alpha, mix, Rng, AMBER, CYAN, CYAN_SOFT, INK, MINT, MUTED, RED, VIOLET};

/// Film-time this experiment spans.
pub(crate) const SECONDS: f32 = 12.0;

// ── The road ────────────────────────────────────────────────────────────────

const L: usize = 240;
const V_MAX: i32 = 5;
const P_DAWDLE: f32 = 0.30;
/// The showcase ring's density — just above critical, where the jams live.
const SHOW_DENSITY: f32 = 0.16;
const HISTORY: usize = 168;

/// One Nagel–Schreckenberg update of a whole ring. `cells[i]` is the speed
/// of the car at cell i, or −1 for empty road.
fn ns_step(cells: &mut Vec<i32>, rng: &mut Rng) -> usize {
    let n = cells.len();
    let mut next = vec![-1i32; n];
    let mut moved = 0usize;
    for i in 0..n {
        let v = cells[i];
        if v < 0 {
            continue;
        }
        // 1. accelerate
        let mut v = (v + 1).min(V_MAX);
        // 2. brake to the gap
        let mut gap = 0;
        while gap < V_MAX + 1 && cells[(i + 1 + gap as usize) % n] < 0 {
            gap += 1;
        }
        v = v.min(gap);
        // 3. dawdle
        if rng.f01() < P_DAWDLE {
            v = (v - 1).max(0);
        }
        // 4. move
        let j = (i + v as usize) % n;
        next[j] = v;
        moved += v as usize;
    }
    *cells = next;
    moved
}

/// A ring at a given density, run for `steps`, returning the mean flow over
/// the last half (the steady state — the first half is the transient, and
/// including it is the classic way to get a fundamental diagram that sags).
fn ring_flow(density: f32, steps: usize, seed: u64) -> (f32, f32) {
    let mut rng = Rng::new(seed);
    let cars = ((L as f32 * density).round() as usize).max(1).min(L);
    let mut cells = vec![-1i32; L];
    // deterministic even placement, jittered from the seed
    let mut placed = 0usize;
    let mut i = 0usize;
    while placed < cars {
        let slot = (i * L / cars + (rng.f01() * 2.0) as usize) % L;
        if cells[slot] < 0 {
            cells[slot] = 0;
            placed += 1;
        }
        i += 1;
        if i > L * 8 {
            for c in cells.iter_mut() {
                if placed >= cars {
                    break;
                }
                if *c < 0 {
                    *c = 0;
                    placed += 1;
                }
            }
            break;
        }
    }
    let mut sum = 0.0;
    let mut count = 0usize;
    for s in 0..steps {
        let moved = ns_step(&mut cells, &mut rng);
        if s >= steps / 2 {
            // flow = (Σv)/L cars per cell per step
            sum += moved as f32 / L as f32;
            count += 1;
        }
    }
    let actual_density = cars as f32 / L as f32;
    (actual_density, sum / count.max(1) as f32)
}

/// The showcase ring's history: the last HISTORY rows of the time–space
/// diagram, oldest first.
fn show_history(t: f64) -> (Vec<Vec<i32>>, usize) {
    let mut rng = Rng::new(0x7AFF_1C00_0000_0011);
    let cars = (L as f32 * SHOW_DENSITY).round() as usize;
    let mut cells = vec![-1i32; L];
    for k in 0..cars {
        cells[k * L / cars] = 0;
    }
    // Three windows' worth: the last HISTORY rows are then a settled shock
    // rather than one still forming, which is what the Rankine–Hugoniot
    // comparison requires. (At 2 windows the edge fit and the shock
    // condition disagreed by 22%; the jam was still growing.)
    let total = HISTORY * 3;
    let n = ((t * total as f64) as usize).max(1);
    let mut rows: Vec<Vec<i32>> = Vec::new();
    for _ in 0..n {
        ns_step(&mut cells, &mut rng);
        rows.push(cells.clone());
        if rows.len() > HISTORY {
            rows.remove(0);
        }
    }
    (rows, n)
}

/// The two traffic states, measured from the same rows the wave speed is
/// fitted on: everything inside a stopped run of three or more cars is
/// "jam", everything else is "free road". Returns (ρ_jam, q_jam, ρ_free,
/// q_free) — the four numbers the Rankine–Hugoniot condition needs.
fn two_states(rows: &[Vec<i32>]) -> (f32, f32, f32, f32) {
    let (mut rj, mut qj, mut nj) = (0.0_f32, 0.0_f32, 0.0_f32);
    let (mut rf, mut qf, mut nf) = (0.0_f32, 0.0_f32, 0.0_f32);
    for row in rows {
        let mut mask = vec![false; L];
        for i in 0..L {
            if row[i] == 0 && row[(i + L - 1) % L] != 0 {
                let mut n = 0usize;
                while row[(i + n) % L] == 0 && n < L {
                    n += 1;
                }
                if n >= 3 {
                    for k in 0..n {
                        mask[(i + k) % L] = true;
                    }
                }
            }
        }
        for i in 0..L {
            if mask[i] {
                nj += 1.0;
                if row[i] >= 0 {
                    rj += 1.0;
                    qj += row[i] as f32;
                }
            } else {
                nf += 1.0;
                if row[i] >= 0 {
                    rf += 1.0;
                    qf += row[i] as f32;
                }
            }
        }
    }
    (
        rj / nj.max(1.0),
        qj / nj.max(1.0),
        rf / nf.max(1.0),
        qf / nf.max(1.0),
    )
}

/// The backward wave speed, fitted from the time–space rows themselves.
///
/// A jam is a maximal run of stopped cars. Its **upstream edge** — the
/// cell of the last car in the run, reading against the direction of
/// travel — is what moves backwards. The fit tracks, row to row, the
/// displacement of the nearest jam edge and averages it. (Tracking the
/// jam's *centre* instead gives a number that wanders with the jam's
/// length; the edge is the thing the theory speaks about.)
fn wave_speed(rows: &[Vec<i32>]) -> (f32, usize) {
    let mut edges_per_row: Vec<Vec<i32>> = Vec::new();
    for row in rows {
        let mut edges = Vec::new();
        for i in 0..L {
            let here = row[i] == 0;
            let behind = row[(i + L - 1) % L] == 0;
            // the upstream end of a stopped run: stopped here, moving behind
            if here && !behind {
                edges.push(i as i32);
            }
        }
        edges_per_row.push(edges);
    }
    let mut deltas: Vec<f32> = Vec::new();
    for w in edges_per_row.windows(2) {
        for &e in &w[0] {
            // the nearest edge in the next row, on the ring
            let mut best = i32::MAX;
            for &f in &w[1] {
                let mut d = f - e;
                if d > L as i32 / 2 {
                    d -= L as i32;
                }
                if d < -(L as i32) / 2 {
                    d += L as i32;
                }
                if d.abs() < best.abs() {
                    best = d;
                }
            }
            if best != i32::MAX && best.abs() <= 3 {
                deltas.push(best as f32);
            }
        }
    }
    let n = deltas.len();
    (
        if n > 0 {
            deltas.iter().sum::<f32>() / n as f32
        } else {
            0.0
        },
        n,
    )
}

// ── The frame ───────────────────────────────────────────────────────────────

pub(crate) fn frame(t: f32) -> WidgetNode {
    let (rows, step_no) = show_history(t as f64);
    // The fit uses the second half of the window only: the first rows are
    // the crystal breaking up, and a transient is not a shock.
    let settled = &rows[rows.len() / 2..];
    let (wave, wave_n) = wave_speed(settled);
    let (rho_j, q_j, rho_f, q_f) = two_states(settled);
    let rh = if (rho_f - rho_j).abs() > 1e-6 {
        (q_f - q_j) / (rho_f - rho_j)
    } else {
        0.0
    };

    // ── the fundamental diagram: 28 independent rings ──
    let mut fd: Vec<(f32, f32)> = Vec::new();
    for k in 0..28 {
        let d = 0.02 + k as f32 / 27.0 * 0.88;
        fd.push(ring_flow(d, 420, 0x51D0_0000_0000_0001 + k as u64 * 7919));
    }
    // free-flow slope, fitted over the points below the peak
    let peak = fd
        .iter()
        .cloned()
        .fold((0.0_f32, 0.0_f32), |a, b| if b.1 > a.1 { b } else { a });
    let free: Vec<&(f32, f32)> = fd.iter().filter(|p| p.0 < peak.0 * 0.75).collect();
    let slope = if free.len() > 1 {
        let sxy: f32 = free.iter().map(|p| p.0 * p.1).sum();
        let sxx: f32 = free.iter().map(|p| p.0 * p.0).sum();
        sxy / sxx.max(1e-9)
    } else {
        0.0
    };
    let slope_theory = V_MAX as f32 - P_DAWDLE;

    let current = rows.last().cloned().unwrap_or_default();
    let stopped = current.iter().filter(|&&v| v == 0).count();
    let cars = current.iter().filter(|&&v| v >= 0).count();
    let mean_v = if cars > 0 {
        current
            .iter()
            .filter(|&&v| v >= 0)
            .map(|&v| v as f32)
            .sum::<f32>()
            / cars as f32
    } else {
        0.0
    };
    let rows_draw = rows.clone();
    let fd_draw = fd.clone();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            book.rect(
                Rect::new(0.0, 0.0, size.width, size.height),
                Gradient::vertical()
                    .with_dither()
                    .with_stops(&[(0.0, Color::rgb(6, 6, 10)), (1.0, Color::rgb(12, 11, 16))]),
            );

            // ── the ring road ──
            let rcx = 220.0_f32;
            let rcy = 330.0_f32;
            let rr = 130.0_f32;
            book.rrect(
                Rect::new(rcx - 190.0, rcy - 170.0, rcx + 190.0, rcy + 170.0),
                12.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            book.ring(
                Offset::new(rcx, rcy),
                rr,
                26.0,
                alpha(Color::rgb(22, 22, 30), 0.9),
            );
            book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                for (i, &v) in current.iter().enumerate() {
                    if v < 0 {
                        continue;
                    }
                    let a =
                        i as f32 / L as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                    let k = v as f32 / V_MAX as f32;
                    let c = if v == 0 {
                        RED
                    } else {
                        mix(AMBER, CYAN_SOFT, k)
                    };
                    g.circle(
                        Offset::new(rcx + a.cos() * rr, rcy + a.sin() * rr),
                        3.4,
                        alpha(c, 0.55 + 0.45 * k),
                    );
                }
            });

            // ── the time–space diagram ──
            let tx = 46.0_f32;
            let ty = 520.0_f32;
            let tw = 560.0_f32;
            let th = 176.0_f32;
            book.rrect(
                Rect::new(tx - 14.0, ty - 28.0, tx + tw + 14.0, ty + th + 14.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            let cw = tw / L as f32;
            let rh = th / HISTORY as f32;
            for (r, row) in rows_draw.iter().enumerate() {
                let y = ty + r as f32 * rh;
                for (i, &v) in row.iter().enumerate() {
                    if v < 0 {
                        continue;
                    }
                    let k = v as f32 / V_MAX as f32;
                    let c = if v == 0 { RED } else { mix(AMBER, CYAN, k) };
                    book.rect(
                        Rect::new(
                            tx + i as f32 * cw,
                            y,
                            tx + (i as f32 + 1.0) * cw,
                            y + rh + 0.4,
                        ),
                        alpha(c, 0.30 + 0.62 * (1.0 - k)),
                    );
                }
            }

            // ── the fundamental diagram ──
            let fx = 700.0_f32;
            let fy = 234.0_f32;
            let fw = 520.0_f32;
            let fh = 400.0_f32;
            book.rrect(
                Rect::new(fx - 20.0, fy - 30.0, fx + fw + 20.0, fy + fh + 40.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            let qmax = fd_draw.iter().map(|p| p.1).fold(1e-6_f32, f32::max) * 1.15;
            // the free-flow line, from the fitted slope
            book.line(
                Offset::new(fx, fy + fh),
                Offset::new(fx + (qmax / slope.max(1e-6)) * fw, fy),
                alpha(MINT, 0.45),
                1.2,
            );
            // the jam branch that the ring's own congested points trace
            let mut p = Path::new();
            for (i, &(d, q)) in fd_draw.iter().enumerate() {
                let o = Offset::new(fx + d * fw, fy + fh - (q / qmax) * fh);
                if i == 0 {
                    p.move_to(o);
                } else {
                    p.line_to(o);
                }
            }
            book.stroke(p, alpha(VIOLET, 0.55), 1.4);
            for &(d, q) in fd_draw.iter() {
                book.circle(
                    Offset::new(fx + d * fw, fy + fh - (q / qmax) * fh),
                    3.4,
                    alpha(if d < peak.0 { CYAN } else { AMBER }, 0.95),
                );
            }
            // the critical density, marked where the measurement puts it
            book.line(
                Offset::new(fx + peak.0 * fw, fy),
                Offset::new(fx + peak.0 * fw, fy + fh),
                alpha(AMBER, 0.35),
                1.0,
            );
            // the showcase ring's own operating point
            book.ring(
                Offset::new(
                    fx + SHOW_DENSITY * fw,
                    fy + fh
                        - (fd_draw
                            .iter()
                            .min_by(|a, b| {
                                (a.0 - SHOW_DENSITY)
                                    .abs()
                                    .partial_cmp(&(b.0 - SHOW_DENSITY).abs())
                                    .unwrap()
                            })
                            .map(|p| p.1)
                            .unwrap_or(0.0)
                            / qmax)
                            * fh,
                ),
                8.0,
                1.6,
                alpha(INK, 0.8),
            );
        }),
    );

    let lines = ["TRAFFIC · THE JAM AXIS · NAGEL–SCHRECKENBERG, AND THE JAM WITH NO CAUSE".to_string(),
        format!(
            "accelerate → brake to the gap → dawdle with p = {P_DAWDLE} → move · v_max = {V_MAX} cells/step · ring of {L} cells · replayed from the seed every frame · step {step_no}"
        ),
        format!(
            "THE SHOWCASE RING at ρ = {SHOW_DENSITY}: {cars} cars, {stopped} of them completely stopped right now ({:.1}%), mean speed {mean_v:.2} cells/step — and nothing is blocking the road",
            stopped as f32 / cars.max(1) as f32 * 100.0
        ),
        format!(
            "BACKWARD WAVE from {wave_n} jam-edge displacements: {wave:+.3} cells/step · RANKINE–HUGONIOT on the same rows — jam (ρ {rho_j:.2}, q {q_j:.2}), free (ρ {rho_f:.3}, q {q_f:.3}) → {rh:+.3} ({:+.1}%)",
            (wave - rh) / rh.abs().max(1e-6) * 100.0
        ),
        format!(
            "FUNDAMENTAL DIAGRAM, 28 independent rings each run 420 steps (flow averaged over the second half only): free-flow slope fitted {slope:.3} vs v_max − p = {slope_theory:.3} ({:+.1}%)",
            (slope - slope_theory) / slope_theory * 100.0
        ),
        format!(
            "CRITICAL DENSITY located from those points: ρ_c = {:.3} at peak flow q = {:.3} cars/cell/step — past it, adding cars REMOVES throughput, which is the whole of rush hour",
            peak.0, peak.1
        )];

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(42.0)
                .top(38.0 + i as f32 * 16.0)
                .width(1200.0)
                .height(15.0)
                .child(
                    Text::new(line.clone()).style(
                        TextStyle::new(if i == 0 { 12.0 } else { 11.0 })
                            .monospace()
                            .letter_spacing(if i == 0 { 1.8 } else { 0.0 })
                            .color(alpha(
                                if i == 0 { MUTED } else { mix(MUTED, INK, 0.45) },
                                0.95,
                            )),
                    ),
                ),
        );
    }
    for (x, y, s) in [
        (46.0_f32, 492.0_f32, "TIME–SPACE DIAGRAM — road across, time downward · red is stopped · the stripes lean backwards".to_string()),
        (700.0, 206.0, "FUNDAMENTAL DIAGRAM — flow vs density · green: the fitted free-flow line".to_string()),
        (46.0, 170.0, "THE RING — one lane, no junctions, nothing to slow down for".to_string()),
    ] {
        stack = stack.push(
            Positioned::new().left(x).top(y).width(600.0).height(14.0).child(
                Text::new(s).style(
                    TextStyle::new(9.5)
                        .monospace()
                        .letter_spacing(0.9)
                        .color(alpha(MUTED, 0.85)),
                ),
            ),
        );
    }
    stack.into()
}
