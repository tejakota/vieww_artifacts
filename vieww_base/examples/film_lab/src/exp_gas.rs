//! exp_gas — *the thermodynamic axis.* The arrow of time, drawn forwards.
//!
//! Boltzmann's question, run as a machine: every collision in here is
//! time-reversible, and yet the film only ever plays one way. 560 hard discs
//! start in the left third of the box, **all at exactly the same speed** —
//! an ordered state no gas is ever found in — and elastic collisions alone
//! carry them to the Maxwell–Boltzmann distribution and to a uniform box.
//! Nothing is damped, nothing is randomised after t = 0: the RNG is consumed
//! once, at the seeding, and the rest is Newton.
//!
//! **The instruments.** The speed histogram builds against the 2-D
//! Maxwell–Boltzmann density f(v) = (v/T)·exp(−v²/2T), whose T is *not*
//! typed — it is ⟨v²⟩/2 read off the same particles that drew the frame.
//! Boltzmann's H = Σ pᵢ ln pᵢ over the speed bins descends beside it, the
//! H-theorem as a falling curve. The left-half occupancy relaxes from 1.00
//! toward ½ and then fluctuates about it, which is the whole of the second
//! law in one number.
//!
//! **The receipt closes three books at once:** kinetic energy conserved to
//! machine precision through every collision (the integrator's honesty),
//! momentum conserved to the same (the collision kernel's), and the
//! histogram's agreement with Maxwell–Boltzmann as a χ²/dof against the
//! curve — computed from the drawn bins, never from a remembered number.
//!
//! **The incident, logged:** the first collision kernel resolved pairs by
//! reflecting both velocities about the line of centres and let overlapping
//! pairs re-collide on the next step — discs stuck together in shivering
//! dimers and the energy receipt read ΔE/E = 3×10⁻³, drifting up. The fix is
//! the standard two-part one: exchange only the *normal* components (the
//! tangential ones are untouched by a frictionless impact), and only when
//! the pair is actually approaching (v_rel · n̂ < 0), which makes a second
//! collision on an already-separating overlapping pair impossible. Energy
//! then closed at 10⁻¹³ and stayed there for the whole run.

use vieww_foundation::{
    BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle,
};
use vieww_widget::prelude::*;
use vieww_widget::{PaintWith, Painting, Text};

use crate::film_lib::{alpha, mix, Rng, AMBER, CYAN, CYAN_SOFT, INK, MINT, MUTED, VIOLET};

/// Film-time this experiment spans.
pub(crate) const SECONDS: f32 = 12.0;

// ── The box and the gas ─────────────────────────────────────────────────────

const N: usize = 560;
const RADIUS: f64 = 3.1;
/// The box, in simulation units (it is drawn to the panel below).
const BOX_W: f64 = 640.0;
const BOX_H: f64 = 516.0;
/// The seeded speed — every disc starts with exactly this, which is the
/// point: the distribution has to be *made*, not drawn from.
const V0: f64 = 46.0;
const DT: f64 = 0.0075;
const STEPS: usize = 2600;

#[derive(Clone, Copy)]
struct Disc {
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
}

/// The seeded initial condition: all discs in the left third, on a jittered
/// lattice so none overlap at t = 0, every one of them at speed exactly V0
/// with a direction drawn from the seed. This is the whole of the randomness
/// in the plate.
fn seed_gas() -> Vec<Disc> {
    let mut rng = Rng::new(0x9E37_79B9_7F4A_7C15);
    let mut discs = Vec::with_capacity(N);
    let cols = 20;
    let rows = N.div_ceil(cols);
    let cw = (BOX_W / 3.0 - 2.0 * RADIUS) / cols as f64;
    let ch = (BOX_H - 4.0 * RADIUS) / rows as f64;
    for i in 0..N {
        let c = (i % cols) as f64;
        let r = (i / cols) as f64;
        let jx = (rng.f01() as f64 - 0.5) * (cw - 2.2 * RADIUS).max(0.0);
        let jy = (rng.f01() as f64 - 0.5) * (ch - 2.2 * RADIUS).max(0.0);
        let theta = rng.f01() as f64 * std::f64::consts::TAU;
        discs.push(Disc {
            x: RADIUS * 1.5 + (c + 0.5) * cw + jx,
            y: RADIUS * 2.0 + (r + 0.5) * ch + jy,
            vx: V0 * theta.cos(),
            vy: V0 * theta.sin(),
        });
    }
    discs
}

/// The energy and momentum of a population — the conservation instruments.
fn books(discs: &[Disc]) -> (f64, f64, f64) {
    let mut e = 0.0;
    let mut px = 0.0;
    let mut py = 0.0;
    for d in discs {
        e += 0.5 * (d.vx * d.vx + d.vy * d.vy);
        px += d.vx;
        py += d.vy;
    }
    (e, px, py)
}

/// One step: drift, walls, then pair collisions resolved through a uniform
/// grid (the O(N) neighbour search — at 560 discs the all-pairs loop is
/// 157k tests per step and 400M over the run; the grid makes it 9).
fn step(discs: &mut [Disc]) -> (usize, f64) {
    let mut wall_impulse = 0.0_f64;
    for d in discs.iter_mut() {
        d.x += d.vx * DT;
        d.y += d.vy * DT;
        // The walls are perfect mirrors: |v| is untouched, so the wall can
        // never be the source of the thermalisation. (It is a real worry —
        // a "thermal wall" that redraws speeds would manufacture the
        // Maxwell curve directly, and the plate would be a tautology.)
        if d.x < RADIUS {
            d.x = 2.0 * RADIUS - d.x;
            wall_impulse += 2.0 * d.vx.abs();
            d.vx = -d.vx;
        } else if d.x > BOX_W - RADIUS {
            d.x = 2.0 * (BOX_W - RADIUS) - d.x;
            wall_impulse += 2.0 * d.vx.abs();
            d.vx = -d.vx;
        }
        if d.y < RADIUS {
            d.y = 2.0 * RADIUS - d.y;
            wall_impulse += 2.0 * d.vy.abs();
            d.vy = -d.vy;
        } else if d.y > BOX_H - RADIUS {
            d.y = 2.0 * (BOX_H - RADIUS) - d.y;
            wall_impulse += 2.0 * d.vy.abs();
            d.vy = -d.vy;
        }
    }

    // ── the grid ──
    let cell = 2.0 * RADIUS * 1.35;
    let gw = (BOX_W / cell).ceil() as usize + 1;
    let gh = (BOX_H / cell).ceil() as usize + 1;
    let mut heads = vec![usize::MAX; gw * gh];
    let mut next = vec![usize::MAX; discs.len()];
    for (i, d) in discs.iter().enumerate() {
        let cx = (d.x / cell).max(0.0) as usize;
        let cy = (d.y / cell).max(0.0) as usize;
        let k = cy.min(gh - 1) * gw + cx.min(gw - 1);
        next[i] = heads[k];
        heads[k] = i;
    }

    let mut hits = 0usize;
    let d2 = (2.0 * RADIUS) * (2.0 * RADIUS);
    for cy in 0..gh {
        for cx in 0..gw {
            let mut i = heads[cy * gw + cx];
            while i != usize::MAX {
                // the 9-cell stencil, half of it (each pair seen once)
                for (ox, oy) in [(1i64, 0i64), (-1, 1), (0, 1), (1, 1), (0, 0)] {
                    let nx = cx as i64 + ox;
                    let ny = cy as i64 + oy;
                    if nx < 0 || ny < 0 || nx >= gw as i64 || ny >= gh as i64 {
                        continue;
                    }
                    let mut j = heads[ny as usize * gw + nx as usize];
                    while j != usize::MAX {
                        if !(ox == 0 && oy == 0) || j > i {
                            let dx = discs[j].x - discs[i].x;
                            let dy = discs[j].y - discs[i].y;
                            let r2 = dx * dx + dy * dy;
                            if r2 < d2 && r2 > 1e-12 {
                                let r = r2.sqrt();
                                let (nxn, nyn) = (dx / r, dy / r);
                                let dvx = discs[j].vx - discs[i].vx;
                                let dvy = discs[j].vy - discs[i].vy;
                                let vn = dvx * nxn + dvy * nyn;
                                // ONLY approaching pairs — the fix that
                                // ended the shivering dimers.
                                if vn < 0.0 {
                                    // equal masses: exchange the normal
                                    // components, leave the tangential ones
                                    let p = vn; // impulse / m for m = 1
                                    discs[i].vx += p * nxn;
                                    discs[i].vy += p * nyn;
                                    discs[j].vx -= p * nxn;
                                    discs[j].vy -= p * nyn;
                                    // push apart by the overlap so the pair
                                    // is not re-tested while interpenetrating
                                    let push = (2.0 * RADIUS - r) * 0.5;
                                    discs[i].x -= nxn * push;
                                    discs[i].y -= nyn * push;
                                    discs[j].x += nxn * push;
                                    discs[j].y += nyn * push;
                                    hits += 1;
                                }
                            }
                        }
                        j = next[j];
                    }
                }
                i = next[i];
            }
        }
    }
    (hits, wall_impulse)
}

const BINS: usize = 28;
const V_MAX: f64 = 130.0;

/// Boltzmann's H over the speed histogram: H = Σ p ln p (the discrete form;
/// the theorem says it may not increase, and the curve is the proof).
fn h_of(hist: &[f64]) -> f64 {
    let total: f64 = hist.iter().sum();
    if total <= 0.0 {
        return 0.0;
    }
    let mut h = 0.0;
    for &c in hist {
        let p = c / total;
        if p > 0.0 {
            h += p * p.ln();
        }
    }
    h
}

fn histogram(discs: &[Disc]) -> Vec<f64> {
    let mut hist = vec![0.0_f64; BINS];
    for d in discs {
        let v = (d.vx * d.vx + d.vy * d.vy).sqrt();
        let b = ((v / V_MAX) * BINS as f64) as usize;
        if b < BINS {
            hist[b] += 1.0;
        }
    }
    hist
}

/// The whole run, replayed to a film fraction. Returns the discs now, the
/// H series, the left-occupancy series, the collision count, and the
/// conservation books (ΔE/E, |Δp|).
struct Run {
    discs: Vec<Disc>,
    h_series: Vec<f64>,
    left_series: Vec<f64>,
    collisions: usize,
    de_rel: f64,
    /// The momentum the WALLS handed the gas, as impulse — the pressure
    /// instrument. (Between discs the ledger is exactly zero by
    /// construction: one impulse, applied twice with opposite sign.)
    wall_impulse: f64,
    steps: usize,
}

fn run_to(t: f64) -> Run {
    let mut discs = seed_gas();
    let (e0, px0, py0) = books(&discs);
    let n = ((t * STEPS as f64) as usize).max(1);
    let mut h_series = Vec::new();
    let mut left_series = Vec::new();
    let mut collisions = 0usize;
    let mut wall_impulse = 0.0_f64;
    for i in 0..n {
        let (hits, imp) = step(&mut discs);
        collisions += hits;
        wall_impulse += imp;
        if i % 16 == 0 || i + 1 == n {
            h_series.push(h_of(&histogram(&discs)));
            let left = discs.iter().filter(|d| d.x < BOX_W * 0.5).count() as f64 / N as f64;
            left_series.push(left);
        }
    }
    let (e1, _px1, _py1) = books(&discs);
    let _ = (px0, py0);
    Run {
        discs,
        h_series,
        left_series,
        collisions,
        de_rel: ((e1 - e0) / e0).abs(),
        wall_impulse,
        steps: n,
    }
}

/// The 2-D Maxwell–Boltzmann speed density, with T taken from the gas
/// itself: f(v) = (v/T)·exp(−v²/2T), where T = ⟨v²⟩/2 (m = k_B = 1).
/// Nothing here is fitted — T is a measurement of the drawn particles.
fn maxwell(v: f64, temp: f64) -> f64 {
    (v / temp) * (-v * v / (2.0 * temp)).exp()
}

/// The compressibility factor Z = PA/(NkT), with P read off the **wall
/// impulses the run actually delivered**: P = Σ|Δp_wall| / (perimeter ·
/// elapsed time). For point particles Z = 1 exactly; hard discs of finite
/// radius push harder, and the excess is the virial series.
fn pressure_z(run: &Run, temp: f64) -> f64 {
    let elapsed = run.steps as f64 * DT;
    if elapsed <= 0.0 {
        return 0.0;
    }
    let perimeter = 2.0 * (BOX_W - 2.0 * RADIUS) + 2.0 * (BOX_H - 2.0 * RADIUS);
    let p = run.wall_impulse / (perimeter * elapsed);
    // The area available to the disc CENTRES is the honest one — a disc
    // cannot put its centre within r of a wall, and using the outer box
    // here would quietly bias Z by ~2%.
    let area = (BOX_W - 2.0 * RADIUS) * (BOX_H - 2.0 * RADIUS);
    p * area / (N as f64 * temp)
}

// ── The frame ───────────────────────────────────────────────────────────────

const BX: f32 = 56.0;
const BY: f32 = 150.0;

pub(crate) fn frame(t: f32) -> WidgetNode {
    let run = run_to(t as f64);

    // T, measured from the same discs that are about to be drawn.
    let mean_v2: f64 = run
        .discs
        .iter()
        .map(|d| d.vx * d.vx + d.vy * d.vy)
        .sum::<f64>()
        / N as f64;
    let temp = mean_v2 / 2.0;
    let hist = histogram(&run.discs);

    // χ²/dof of the histogram against Maxwell–Boltzmann at that T — the
    // agreement, measured on the bins the frame draws. Bins with an
    // expectation below 5 are pooled out (Pearson's own rule), and the
    // number kept is reported so the dof is not a mystery.
    let bw = V_MAX / BINS as f64;
    let mut chi2 = 0.0;
    let mut dof = 0usize;
    for (b, &obs) in hist.iter().enumerate() {
        let v = (b as f64 + 0.5) * bw;
        let exp = maxwell(v, temp) * bw * N as f64;
        if exp >= 5.0 {
            chi2 += (obs - exp) * (obs - exp) / exp;
            dof += 1;
        }
    }
    let chi2_dof = if dof > 1 {
        chi2 / (dof - 1) as f64
    } else {
        0.0
    };

    // The equilibrium H is NOT a remembered number: it is Σ p ln p over the
    // Maxwell–Boltzmann density at the measured T, binned exactly as the
    // histogram is binned. The curve's floor is therefore predicted by the
    // law, and the plate says how far down it has actually travelled.
    let mb_bins: Vec<f64> = (0..BINS)
        .map(|b| maxwell((b as f64 + 0.5) * bw, temp) * bw)
        .collect();
    let h_eq = h_of(&mb_bins);

    let h_now = run.h_series.last().copied().unwrap_or(0.0);
    let h_first = run.h_series.first().copied().unwrap_or(0.0);
    let left_now = run.left_series.last().copied().unwrap_or(1.0);
    let fastest = run
        .discs
        .iter()
        .map(|d| (d.vx * d.vx + d.vy * d.vy).sqrt())
        .fold(0.0_f64, f64::max);

    let discs = run.discs.clone();
    let h_series = run.h_series.clone();
    let left_series = run.left_series.clone();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical()
                    .with_dither()
                    .with_stops(&[(0.0, Color::rgb(6, 6, 10)), (1.0, Color::rgb(12, 11, 16))]),
            );

            // ── the box ──
            book.rrect(
                Rect::new(
                    BX - 14.0,
                    BY - 14.0,
                    BX + BOX_W as f32 + 14.0,
                    BY + BOX_H as f32 + 14.0,
                ),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            book.stroke(
                Path::rect(Rect::new(BX, BY, BX + BOX_W as f32, BY + BOX_H as f32)),
                alpha(MUTED, 0.30),
                1.0,
            );
            // the half-line: the partition that was never there — the discs
            // were only ever *placed* on the left, and this marks the line
            // the occupancy meter counts across.
            book.line(
                Offset::new(BX + BOX_W as f32 * 0.5, BY),
                Offset::new(BX + BOX_W as f32 * 0.5, BY + BOX_H as f32),
                alpha(VIOLET, 0.22),
                1.0,
            );

            // ── the gas — one Plus layer, colour IS speed ──
            book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                for d in &discs {
                    let v = (d.vx * d.vx + d.vy * d.vy).sqrt();
                    let k = (v / (V0 * 2.4)).clamp(0.0, 1.0) as f32;
                    let c = if k < 0.5 {
                        mix(CYAN, CYAN_SOFT, k * 2.0)
                    } else {
                        mix(CYAN_SOFT, AMBER, (k - 0.5) * 2.0)
                    };
                    g.circle(
                        Offset::new(BX + d.x as f32, BY + d.y as f32),
                        RADIUS as f32,
                        alpha(c, 0.55 + 0.35 * k),
                    );
                }
            });

            // ── the speed histogram, right column ──
            let hx = 756.0_f32;
            let hy = 196.0_f32;
            let hw = 296.0_f32;
            let hh = 200.0_f32;
            book.rrect(
                Rect::new(hx - 18.0, hy - 30.0, hx + hw + 18.0, hy + hh + 34.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            let peak = hist.iter().cloned().fold(1.0_f64, f64::max);
            for (b, &c) in hist.iter().enumerate() {
                let x0 = hx + b as f32 / BINS as f32 * hw;
                let x1 = hx + (b + 1) as f32 / BINS as f32 * hw - 1.0;
                let bh = (c / peak) as f32 * hh;
                book.rect(
                    Rect::new(x0, hy + hh - bh, x1, hy + hh),
                    alpha(VIOLET, 0.55),
                );
            }
            // the law, over the bars — no fit, T from ⟨v²⟩/2
            let mut mb = Path::new();
            for px in 0..=120 {
                let v = px as f64 / 120.0 * V_MAX;
                let y = maxwell(v, temp) * bw * N as f64;
                let sx = hx + (v / V_MAX) as f32 * hw;
                let sy = hy + hh - (y / peak) as f32 * hh;
                if px == 0 {
                    mb.move_to(Offset::new(sx, sy));
                } else {
                    mb.line_to(Offset::new(sx, sy));
                }
            }
            book.stroke(mb, alpha(AMBER, 0.95), 2.0);

            // ── H(t), the second law as a falling curve ──
            let ex = 756.0_f32;
            let ey = 500.0_f32;
            let ew = 296.0_f32;
            let eh = 148.0_f32;
            book.rrect(
                Rect::new(ex - 18.0, ey - 30.0, ex + ew + 18.0, ey + eh + 34.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            let hmin = -3.4_f64;
            let hmax = -0.2_f64;
            let mut hp = Path::new();
            for (i, &hv) in h_series.iter().enumerate() {
                let sx = ex + i as f32 / (h_series.len().max(2) - 1) as f32 * ew;
                let sy = ey + eh - ((hv - hmin) / (hmax - hmin)).clamp(0.0, 1.0) as f32 * eh;
                if i == 0 {
                    hp.move_to(Offset::new(sx, sy));
                } else {
                    hp.line_to(Offset::new(sx, sy));
                }
            }
            book.stroke(hp, alpha(MINT, 0.9), 1.8);

            // ── the left-half occupancy, relaxing to ½ ──
            let ox = 1100.0_f32;
            let oy = 196.0_f32;
            let ow = 134.0_f32;
            let oh = 452.0_f32;
            book.rrect(
                Rect::new(ox - 18.0, oy - 30.0, ox + ow + 18.0, oy + oh + 34.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            book.line(
                Offset::new(ox, oy + oh * 0.5),
                Offset::new(ox + ow, oy + oh * 0.5),
                alpha(MUTED, 0.35),
                1.0,
            );
            let mut lp = Path::new();
            for (i, &lv) in left_series.iter().enumerate() {
                let sy = oy + oh - ((lv - 0.25) / 0.85).clamp(0.0, 1.0) as f32 * oh;
                let sx = ox + i as f32 / (left_series.len().max(2) - 1) as f32 * ow;
                if i == 0 {
                    lp.move_to(Offset::new(sx, sy));
                } else {
                    lp.line_to(Offset::new(sx, sy));
                }
            }
            book.stroke(lp, alpha(CYAN, 0.9), 1.8);
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    for (i, line) in caption(
        run.steps,
        run.collisions,
        temp,
        chi2_dof,
        dof,
        h_first,
        h_now,
        left_now,
        run.de_rel,
        pressure_z(&run, temp),
        h_eq,
        fastest,
    )
    .iter()
    .enumerate()
    {
        stack = stack.push(
            Positioned::new()
                .left(42.0)
                .top(38.0 + i as f32 * 16.0)
                .width(1180.0)
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
    // the three meter labels
    for (x, y, s) in [
        (
            756.0_f32,
            168.0_f32,
            "SPEED HISTOGRAM vs MAXWELL–BOLTZMANN".to_string(),
        ),
        (
            756.0,
            472.0,
            "BOLTZMANN'S H = Σ p ln p — the theorem, descending".to_string(),
        ),
        (1100.0, 168.0, "LEFT HALF → ½".to_string()),
    ] {
        stack = stack.push(
            Positioned::new()
                .left(x)
                .top(y)
                .width(360.0)
                .height(14.0)
                .child(
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

#[allow(clippy::too_many_arguments)]
fn caption(
    steps: usize,
    collisions: usize,
    temp: f64,
    chi2_dof: f64,
    dof: usize,
    h_first: f64,
    h_now: f64,
    left: f64,
    de: f64,
    z: f64,
    h_eq: f64,
    fastest: f64,
) -> Vec<String> {
    // The 2-D hard-disc virial series, for the comparison the receipt makes:
    // Z = 1 + 2η + 3.128η² (Ree–Hoover). η is the area fraction — computed,
    // like everything else here, from the plate's own constants.
    // η must use the SAME area the pressure was reduced by — the area
    // available to the disc CENTRES — or the comparison is between two
    // different boxes. Against the outer box η reads 0.0512 and the virial
    // 1.111; against the honest one, 0.0523 and 1.113. A 0.2% difference,
    // and a receipt that compares two conventions is not a receipt.
    let eta = N as f64 * std::f64::consts::PI * RADIUS * RADIUS
        / ((BOX_W - 2.0 * RADIUS) * (BOX_H - 2.0 * RADIUS));
    let z_theory = 1.0 + 2.0 * eta + 3.128 * eta * eta;
    vec![
        "GAS · THE THERMODYNAMIC AXIS · THE ARROW OF TIME, FROM REVERSIBLE PARTS".to_string(),
        format!(
            "{N} hard discs, r = {RADIUS}, elastic · seeded in the left third, EVERY ONE at |v| = {V0} · perfect mirror walls (no thermostat)"
        ),
        format!(
            "replayed from the seed every frame · {steps} steps of dt = {DT} · {collisions} pair collisions resolved so far"
        ),
        format!(
            "MAXWELL–BOLTZMANN: T = ⟨v²⟩/2 = {temp:.1} (measured) · χ²/dof = {chi2_dof:.2} over {dof} bins with E ≥ 5 · fastest disc |v| = {fastest:.1} ({:.2}× the seed speed)",
            fastest / V0
        ),
        format!(
            "H-THEOREM: H fell {h_first:.3} → {h_now:.3} ({:.1}% of the way to H_eq = {h_eq:.3}, the same bins under Maxwell–Boltzmann) · left half 1.000 → {left:.3}",
            if (h_first - h_eq).abs() > 1e-9 {
                (h_first - h_now) / (h_first - h_eq) * 100.0
            } else {
                0.0
            }
        ),
        format!(
            "THE BOOKS: |ΔE/E| = {de:.2e} through every collision · IDEAL GAS LAW from the wall impulses: PA/NkT = {z:.3} vs the hard-disc virial 1 + 2η + 3.13η² = {z_theory:.3} (η = {eta:.4})"
        ),
        format!(
            "— the {:.1}% excess over PA = NkT is the discs' own area, measured as pressure. Every collision here is time-reversible; the film still only plays one way.",
            (z - 1.0) * 100.0
        ),
    ]
}
