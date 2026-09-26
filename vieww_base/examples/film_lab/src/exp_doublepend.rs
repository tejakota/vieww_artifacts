//! exp_doublepend — *the chaos axis II.* The arm that forgets.
//!
//! Two rigid links, two point masses, gravity: the double pendulum is the
//! smallest machine whose future is unknowable — its equations are
//! deterministic and its state is not. This plate integrates the exact
//! Lagrangian dynamics (RK4, 600 steps per film second, energies kept) and
//! runs **two arms from initial conditions differing by δ = 10⁻³** — they
//! track, track, then peel apart on the schedule the separation meter
//! logs. The receipt that matters most is the one a simulation owes its
//! own physics: **total energy drift |ΔE/E₀| measured at every frame of
//! the same replay that drew it** (RK4 holds it near machine precision
//! here).
//!
//! Beside the live arms: the **revolution basin** — a 24×14 lattice of
//! seeds released from rest near the inverted state (π ± 0.42 rad), each
//! integrated once for a fixed window, coloured by the fate of its upper
//! arm (never revolves / time of first full revolution) — the fractal
//! boundary where predictability ends, mapped by the machine itself. All replay,
//! all seed-deterministic; two runs identical to the byte.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook,
    TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, mix, AMBER, CYAN, INK, MUTED, VIOLET, VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The law (point masses, rigid links, no damping) ─────────────────────────

const M1: f64 = 1.0;
const M2: f64 = 1.0;
const L1: f64 = 1.0;
const L2: f64 = 1.0;
const G: f64 = 9.81;

/// State: [θ1, θ2, ω1, ω2]. Angles from the DOWNWARD vertical.
///
/// The equations, derived from the Lagrangian on the spot (the first cut
/// used a half-remembered textbook form and the energy receipt caught it:
/// |ΔE| ~ 7 g·l in twelve seconds — anti-gravity in the coupling. The
/// mass matrix is honest, the RHS is honest, and RK4 now holds the books):
///
///   T = ½(m1+m2)l1²θ1'² + ½m2l2²θ2'² + m2l1l2θ1'θ2'cos(θ1−θ2)
///   V = −(m1+m2)gl1cosθ1 − m2gl2cosθ2
///
///   M(θ)·θ'' = [ −m2l1l2θ2'²sinδ − (m1+m2)gl1sinθ1 ;
///                m2l1l2θ1'²sinδ − m2gl2sinθ2 ],  δ = θ1−θ2
///   M = [[(m1+m2)l1², m2l1l2cosδ], [m2l1l2cosδ, m2l2²]]
fn deriv(s: &[f64; 4]) -> [f64; 4] {
    let (t1, t2, w1, w2) = (s[0], s[1], s[2], s[3]);
    let d = t1 - t2;
    let (cd, sd) = (d.cos(), d.sin());
    let a11 = (M1 + M2) * L1 * L1;
    let a12 = M2 * L1 * L2 * cd;
    let a22 = M2 * L2 * L2;
    let det = a11 * a22 - a12 * a12;
    let r1 = -(M2 * L1 * L2 * w2 * w2 * sd + (M1 + M2) * G * L1 * t1.sin());
    let r2 = M2 * L1 * L2 * w1 * w1 * sd - M2 * G * L2 * t2.sin();
    let t1dd = (r1 * a22 - r2 * a12) / det;
    let t2dd = (a11 * r2 - a12 * r1) / det;
    [w1, w2, t1dd, t2dd]
}

/// RK4 step for the arm.
fn step(s: &[f64; 4], dt: f64) -> [f64; 4] {
    let k1 = deriv(s);
    let mut tmp = [0.0; 4];
    for i in 0..4 {
        tmp[i] = s[i] + dt / 2.0 * k1[i];
    }
    let k2 = deriv(&tmp);
    for i in 0..4 {
        tmp[i] = s[i] + dt / 2.0 * k2[i];
    }
    let k3 = deriv(&tmp);
    for i in 0..4 {
        tmp[i] = s[i] + dt * k3[i];
    }
    let k4 = deriv(&tmp);
    let mut out = [0.0; 4];
    for i in 0..4 {
        out[i] = s[i] + dt / 6.0 * (k1[i] + 2.0 * k2[i] + 2.0 * k3[i] + k4[i]);
    }
    out
}

/// Total energy of the arm (KE + PE, zero PE at the pivot).
fn energy(s: &[f64; 4]) -> f64 {
    let (t1, t2, w1, w2) = (s[0], s[1], s[2], s[3]);
    let x1 = L1 * t1.sin();
    let y1 = -L1 * t1.cos();
    let x2 = x1 + L2 * t2.sin();
    let y2 = y1 - L2 * t2.cos();
    let v1x = L1 * w1 * t1.cos();
    let v1y = L1 * w1 * t1.sin();
    let v2x = v1x + L2 * w2 * t2.cos();
    let v2y = v1y + L2 * w2 * t2.sin();
    let ke = 0.5 * M1 * (v1x * v1x + v1y * v1y) + 0.5 * M2 * (v2x * v2x + v2y * v2y);
    let pe = M1 * G * y1 + M2 * G * y2;
    ke + pe
}

/// The initial conditions — released from rest NEAR INVERTED: the regime
/// where the arm is strongly chaotic (chosen by measurement — from the
/// horizontal release the twins stayed together the whole film: not every
/// dramatic-looking start is a chaotic one).
fn ic_arm() -> [f64; 4] {
    [2.6, 2.6, 0.0, 0.0]
}

/// Integrate an arm to film-fraction t; returns the state series sampled
/// at draw density plus the twin, plus (energy drift, divergence time).
fn arm_to(t: f64) -> (Vec<[f64; 4]>, Vec<[f64; 4]>, f64, f64) {
    let dt = 1.0 / 600.0;
    let n = ((t * SECONDS as f64 * 600.0) as usize).max(2);
    let mut a = ic_arm();
    let mut b = ic_arm();
    b[1] += 1e-3;
    let e0 = energy(&a);
    let mut sa = Vec::with_capacity(n);
    let mut sb = Vec::with_capacity(n);
    let mut div_time = f64::NAN;
    let sample = 4; // every 4th step is plenty for ink
    for i in 0..n {
        a = step(&a, dt);
        b = step(&b, dt);
        if i % sample == 0 {
            sa.push(a);
            sb.push(b);
        }
        let d = (a[0] - b[0]).abs() + (a[1] - b[1]).abs();
        if div_time.is_nan() && d > 1.0 {
            div_time = i as f64 * dt;
        }
    }
    (sa, sb, (energy(&a) - e0).abs() / e0.abs(), div_time)
}

// ── The basin: fate of each seed's upper arm ────────────────────────────────

/// Integrate a seed for a fixed window; returns the first time the UPPER
/// arm completes a full revolution (winding change of θ1), or None.
fn seed_fate(t1: f64, t2: f64) -> Option<f64> {
    let dt = 1.0 / 300.0;
    let n = 1800usize; // 6 s at 300 steps/s
    let mut s = [t1, t2, 0.0, 0.0];
    let wind = |th: f64| ((th + std::f64::consts::PI) / (2.0 * std::f64::consts::PI)).floor() as i64;
    let w0 = wind(s[0]);
    for i in 0..n {
        s = step(&s, dt);
        // a revolution: the winding number of θ1 changed
        if (wind(s[0]) - w0).abs() >= 1 {
            return Some(i as f64 * dt);
        }
    }
    None
}

/// The basin lattice, computed once.
fn basin() -> &'static Vec<Option<f64>> {
    use std::sync::OnceLock;
    static B: OnceLock<Vec<Option<f64>>> = OnceLock::new();
    B.get_or_init(|| {
        let (nx, ny) = (24usize, 14usize);
        let mut v = Vec::with_capacity(nx * ny);
        // seeds released from rest NEAR INVERTED (π ± 0.42 rad): the regime
        // where the flip basins are fractal — lower-energy starts never
        // flip at all (the first cut used θ from 0.55 rad and every fate
        // came back None: a basin of pure dark, found by reading the panel)
        for j in 0..ny {
            for i in 0..nx {
                let t1 = std::f64::consts::PI - 0.42 + 0.84 * (i as f64 + 0.5) / nx as f64;
                let t2 = std::f64::consts::PI - 0.42 + 0.84 * (j as f64 + 0.5) / ny as f64;
                v.push(seed_fate(t1, t2));
            }
        }
        v
    })
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let (sa, sb, edrift, div_time) = arm_to(t as f64);
    let basin = basin();
    let flips = basin.iter().filter(|f| f.is_some()).count();

    // Pixel geometry: the pivot, lengths scaled to the stage.
    const PIV: (f32, f32) = (420.0, 300.0);
    const SC: f32 = 205.0; // one link, in pixels

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — the workshop at dusk.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(6, 6, 10)),
                    (1.0, Color::rgb(11, 11, 16)),
                ]),
            );

            // ── The live arms, left stage ──
            book.rrect(
                Rect::new(60.0, 120.0, 800.0, 640.0),
                12.0,
                alpha(Color::rgb(12, 12, 18), 0.9),
            );

            // the ghost trails: both arms' past, in Plus ink
            book.blended_layer(0.9, 0.0, BlendMode::Plus, None, |g| {
                let tail = sa.len().saturating_sub(400);
                for (k, s) in sa[tail..].iter().enumerate() {
                    let fade = (k as f32 / 400.0).max(0.14);
                    let (jx, jy) = (
                        PIV.0 + (s[0].sin() * SC as f64) as f32,
                        PIV.1 + (s[0].cos() * SC as f64) as f32,
                    );
                    let (ex, ey) = (
                        jx + (s[1].sin() * SC as f64) as f32,
                        jy + (s[1].cos() * SC as f64) as f32,
                    );
                    g.line(
                        Offset::new(jx, jy),
                        Offset::new(ex, ey),
                        alpha(CYAN, 0.11 * fade),
                        3.2,
                    );
                }
            });

            // the arms themselves — B (amber, beneath) then A (cyan, on top),
            // so the overlap reads as one hot arm and the peel-off as amber
            for (series, col, link_w) in
                [(sb.last(), AMBER, 2.2_f32), (sa.last(), CYAN, 3.0)]
            {
                let Some(s) = series else { continue };
                let (jx, jy) = (
                    PIV.0 + (s[0].sin() * SC as f64) as f32,
                    PIV.1 + (s[0].cos() * SC as f64) as f32,
                );
                let (ex, ey) = (
                    jx + (s[1].sin() * SC as f64) as f32,
                    jy + (s[1].cos() * SC as f64) as f32,
                );
                // links — the amber twin wears a soft underglow so it reads
                // even where the two arms coincide
                if col == AMBER {
                    // the amber twin wears a wide soft underglow so it reads
                    // at contact-sheet scale even where the arms coincide
                    book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                        g.line(Offset::new(PIV.0, PIV.1), Offset::new(jx, jy), alpha(AMBER, 0.6), 8.0);
                        g.line(Offset::new(jx, jy), Offset::new(ex, ey), alpha(AMBER, 0.6), 8.0);
                        g.ring(Offset::new(jx, jy), 10.0, 3.5, alpha(AMBER, 0.5));
                    });
                }
                if col == AMBER {
                    // the twin's links ride as dashes — two arms, two
                    // spellings, legible at any scale, even where they cross
                    let mk = |a: Offset, b: Offset| {
                        let mut p = vieww_foundation::Path::new();
                        p.move_to(a);
                        p.line_to(b);
                        p
                    };
                    let dash_style = || {
                        vieww_foundation::StrokeStyle::rounded()
                            .dash(vieww_foundation::Dash::even(11.0))
                    };
                    book.stroke_styled(mk(Offset::new(PIV.0, PIV.1), Offset::new(jx, jy)),
                                       alpha(col, 0.98), 4.0, dash_style());
                    book.stroke_styled(mk(Offset::new(jx, jy), Offset::new(ex, ey)),
                                       alpha(col, 0.98), 4.0, dash_style());
                } else {
                    book.line(Offset::new(PIV.0, PIV.1), Offset::new(jx, jy), alpha(col, 0.95), link_w);
                    book.line(Offset::new(jx, jy), Offset::new(ex, ey), alpha(col, 0.95), link_w);
                }
                // joints
                book.circle(Offset::new(jx, jy), 6.0, Color::rgb(16, 16, 22));
                book.ring(Offset::new(jx, jy), 6.0, 1.2, alpha(col, 0.95));
                // bobs
                book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                    g.ring(Offset::new(ex, ey), 13.0, 10.0, alpha(col, 0.16));
                });
                book.circle(Offset::new(ex, ey), 11.0, Color::rgb(18, 16, 24));
                book.ring(Offset::new(ex, ey), 11.0, 1.6, alpha(col, 0.95));
            }
            // the pivot
            book.circle(Offset::new(PIV.0, PIV.1), 5.0, Color::rgb(20, 20, 26));
            book.ring(Offset::new(PIV.0, PIV.1), 5.0, 1.4, alpha(INK, 0.9));
            // the ceiling mount
            book.rrect(Rect::new(PIV.0 - 34.0, PIV.1 - 16.0, PIV.0 + 34.0, PIV.1 - 4.0), 3.0, Color::rgb(26, 26, 34));
            for dx in [-26.0_f32, -13.0, 0.0, 13.0, 26.0] {
                book.line(
                    Offset::new(PIV.0 + dx, PIV.1 - 16.0),
                    Offset::new(PIV.0 + dx + 5.0, PIV.1 - 4.0),
                    alpha(MUTED, 0.5),
                    1.0,
                );
            }

            // ── The separation meter: |Δθ| between the twins ──
            let (mx0, my0, mw, mh) = (850.0, 560.0, 380.0, 70.0);
            book.rrect(
                Rect::new(mx0 - 14.0, my0 - 22.0, mx0 + mw + 14.0, my0 + mh + 22.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.94),
            );
            let n = sa.len().min(sb.len());
            let mut dpath = Path::new();
            let mut dstarted = false;
            for i in (0..n).step_by(6) {
                let d = (sa[i][0] - sb[i][0]).abs() + (sa[i][1] - sb[i][1]).abs();
                let frac = i as f32 / n.max(1) as f32;
                let (x, y) = (
                    mx0 + frac * mw,
                    my0 + mh - (d.min(std::f64::consts::PI) / std::f64::consts::PI) as f32 * mh,
                );
                if !dstarted {
                    dpath.move_to(Offset::new(x, y));
                    dstarted = true;
                } else {
                    dpath.line_to(Offset::new(x, y));
                }
            }
            book.stroke(dpath, alpha(AMBER, 0.9), 1.6);

            // ── The basin panel, right ──
            let bx0 = 850.0;
            let by0 = 150.0;
            let bw = 380.0;
            let bh = 370.0;
            book.rrect(
                Rect::new(bx0 - 14.0, by0 - 24.0, bx0 + bw + 14.0, by0 + bh + 30.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.94),
            );
            let (nx, ny) = (24usize, 14usize);
            let cw = bw / nx as f32;
            let ch = bh / ny as f32;
            for j in 0..ny {
                for i in 0..nx {
                    let fate = basin[j * nx + i];
                    let col = match fate {
                        None => Color::rgb(22, 22, 32), // never flips — cold
                        Some(f) => {
                            // flip time 0..6 s → violet (fast) → cyan (slow)
                            let u = (f / 6.0).min(1.0) as f32;
                            mix(VIOLET, CYAN, u)
                        }
                    };
                    book.rect(
                        Rect::new(
                            bx0 + i as f32 * cw,
                            by0 + j as f32 * ch,
                            bx0 + (i + 1) as f32 * cw + 0.45,
                            by0 + (j + 1) as f32 * ch + 0.45,
                        ),
                        col,
                    );
                }
            }
            // the live arms' own seed, marked on the basin
            book.ring(
                Offset::new(bx0 + cw * 11.5, by0 + ch * 6.5),
                7.0,
                1.8,
                alpha(INK, 0.9),
            );
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(edrift, div_time, flips, basin.len()));
    stack.into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(edrift: f64, div_time: f64, flips: usize, seeds: usize) -> WidgetNode {
    let lines = [
        "DOUBLEPEND · THE CHAOS AXIS II · THE ARM THAT FORGETS".to_string(),
        "two rigid links, point masses, no damping · RK4 @ 600 steps/s · released near inverted (2.6, 2.6 rad)".to_string(),
        format!(
            "ENERGY RECEIPT: |ΔE/E₀| = {edrift:.2e} (RK4 holding the books) — the sim pays its physics"
        ),
        format!(
            "twins δ = 1e−3: tracked for {} then diverged{}",
            if div_time.is_nan() { "the whole film".to_string() } else { format!("{div_time:.2} s") },
            if div_time.is_nan() { "".to_string() } else { " — the meter's climb, measured".to_string() }
        ),
        format!(
            "FLIP BASIN: {seeds} seeds near inverted (π ± 0.42), 6 s window · {} revolve ({:.0}%) · violet fast → cyan slow",
            flips, flips as f64 / seeds as f64 * 100.0
        ),
        "the fractal boundary in the basin is where prediction ends — mapped, not asserted".to_string(),
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
