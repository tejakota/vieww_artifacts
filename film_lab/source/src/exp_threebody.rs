//! exp_threebody — *the chaos axis.* The dance and the earthquake.
//!
//! The Chenciner–Montgomery **figure-eight choreography**: three equal
//! masses chasing each other around a single closed curve, discovered by
//! simulation in 1993 (Moore) and proved to exist in 2000 — the three-body
//! problem's most famous island of order in a sea of chaos. This plate runs
//! it twice, in lockstep, from initial conditions that differ by
//! **δ = 10⁻⁵** on one coordinate. The machine integrates both systems
//! with RK4 (2,400 substeps per period, re-integrated from the initial
//! conditions every frame — no state survives between frames), and draws
//! the nominal choreography as solid light, the perturbed twin as hollow
//! rings.
//!
//! First they are indistinguishable — the same eight, twice. Then the rings
//! peel off the line. The separation meter (a log-scale strip at the
//! bottom, plotted from the same integrations that drew the frame) climbs
//! a straight line: exponential divergence, the Lyapunov fingerprint. The
//! receipt measures it: the growth exponent fitted from the separation
//! series between two marks, and the separation itself at t, printed in
//! the same units as the δ that started it.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle,
    StrokeStyle, Dash};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, mix, smoothstep, tint, CYAN, FAINT, INK, MUTED,
    MAGENTA, VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 14.0;

// ── The choreography ────────────────────────────────────────────────────────

/// The figure-eight initial conditions (equal masses, G = 1) — Simó's values.
const X0: [[f32; 2]; 3] = [
    [0.97000436, -0.24308753],
    [-0.97000436, 0.24308753],
    [0.0, 0.0],
];
const V0: [[f32; 2]; 3] = [
    [0.46620368, 0.43236573],
    [0.46620368, 0.43236573],
    [-0.93240737, -0.86473146],
];

/// The orbit's period (in the same units).
const PERIOD: f32 = 6.32591398;

/// How many periods the plate spans.
const N_PERIODS: f32 = 2.0;

/// The perturbation on body 0's x — the earthquake's seed.
const DELTA: f32 = 1.0e-5;

/// RK4 substeps per unit time.
const SUBSTEPS_PER_T: usize = 1200;

/// The acceleration on body i, given all positions (softened slightly to
/// keep the integrator honest through the eights' close passages).
#[must_use]
fn accel(pos: &[[f32; 2]; 3], i: usize) -> [f32; 2] {
    let mut a = [0.0_f32; 2];
    for j in 0..3 {
        if i == j {
            continue;
        }
        let dx = pos[j][0] - pos[i][0];
        let dy = pos[j][1] - pos[i][1];
        let r2 = dx * dx + dy * dy + 1.0e-6;
        let inv = 1.0 / (r2 * r2.sqrt());
        a[0] += dx * inv;
        a[1] += dy * inv;
    }
    a
}

/// One RK4 step of the whole system.
fn rk4(pos: &mut [[f32; 2]; 3], vel: &mut [[f32; 2]; 3], h: f32) {
    let n = 3;
    let mut k1p = [[0.0; 2]; 3];
    let mut k1v = [[0.0; 2]; 3];
    let mut k2p = [[0.0; 2]; 3];
    let mut k2v = [[0.0; 2]; 3];
    let mut k3p = [[0.0; 2]; 3];
    let mut k3v = [[0.0; 2]; 3];
    let mut k4p = [[0.0; 2]; 3];
    let mut k4v = [[0.0; 2]; 3];
    let mut mid = [[0.0_f32; 2]; 3];

    for i in 0..n {
        k1v[i] = accel(pos, i);
        k1p[i] = vel[i];
    }
    for i in 0..n {
        mid[i] = [pos[i][0] + k1p[i][0] * h * 0.5, pos[i][1] + k1p[i][1] * h * 0.5];
    }
    for i in 0..n {
        k2v[i] = accel(&mid, i);
        k2p[i] = [vel[i][0] + k1v[i][0] * h * 0.5, vel[i][1] + k1v[i][1] * h * 0.5];
    }
    for i in 0..n {
        mid[i] = [pos[i][0] + k2p[i][0] * h * 0.5, pos[i][1] + k2p[i][1] * h * 0.5];
    }
    for i in 0..n {
        k3v[i] = accel(&mid, i);
        k3p[i] = [vel[i][0] + k2v[i][0] * h * 0.5, vel[i][1] + k2v[i][1] * h * 0.5];
    }
    for i in 0..n {
        mid[i] = [pos[i][0] + k3p[i][0] * h, pos[i][1] + k3p[i][1] * h];
    }
    for i in 0..n {
        k4v[i] = accel(&mid, i);
        k4p[i] = [vel[i][0] + k3v[i][0] * h, vel[i][1] + k3v[i][1] * h];
    }
    for i in 0..n {
        for c in 0..2 {
            pos[i][c] += h * (k1p[i][c] + 2.0 * k2p[i][c] + 2.0 * k3p[i][c] + k4p[i][c]) / 6.0;
            vel[i][c] += h * (k1v[i][c] + 2.0 * k2v[i][c] + 2.0 * k3v[i][c] + k4v[i][c]) / 6.0;
        }
    }
}

/// A full replay to film-fraction `t`: returns (nominal trail, twin trail,
/// nominal positions, twin positions, separation series [(t, d)]).
#[must_use]
fn replay(t: f32) -> (Vec<[f32; 2]>, Vec<[f32; 2]>, [[f32; 2]; 3], [[f32; 2]; 3], Vec<(f32, f32)>) {
    let mut pos = X0;
    let mut vel = V0;
    let mut twin_pos = X0;
    twin_pos[0][0] += DELTA;
    let mut twin_vel = V0;

    let total = N_PERIODS * PERIOD * t.max(0.0);
    let steps = (total * SUBSTEPS_PER_T as f32) as usize;
    let h = 1.0 / SUBSTEPS_PER_T as f32;

    // Trails: sample every ~1/240 of time — smooth but bounded.
    let sample_every = SUBSTEPS_PER_T / 6;
    let mut trail = Vec::new();
    let mut twin_trail = Vec::new();
    let mut sep_series = Vec::new();
    let sep_every = SUBSTEPS_PER_T / 2;

    for k in 0..steps {
        rk4(&mut pos, &mut vel, h);
        rk4(&mut twin_pos, &mut twin_vel, h);
        if k % sample_every == 0 {
            // Trail point = body 0's position (the choreography is one
            // curve; body 0 traces it, the others share it phase-shifted).
            trail.push(pos[0]);
            twin_trail.push(twin_pos[0]);
        }
        if k % sep_every == 0 {
            let d = (0..3)
                .map(|i| {
                    let dx = pos[i][0] - twin_pos[i][0];
                    let dy = pos[i][1] - twin_pos[i][1];
                    dx * dx + dy * dy
                })
                .sum::<f32>()
                .sqrt();
            sep_series.push((k as f32 * h, d));
        }
    }
    (trail, twin_trail, pos, twin_pos, sep_series)
}

/// Fit the local exponential growth rate between two times on the
/// separation series (a Lyapunov estimate, measured not asserted).
#[must_use]
fn growth_rate(series: &[(f32, f32)], t0: f32, t1: f32) -> Option<f32> {
    let at = |tm: f32| -> Option<f32> {
        let mut best = None;
        let mut best_dt = f32::INFINITY;
        for &(ts, d) in series {
            let dt = (ts - tm).abs();
            if dt < best_dt && d > 1e-12 {
                best_dt = dt;
                best = Some(d);
            }
        }
        best
    };
    let d0 = at(t0)?;
    let d1 = at(t1)?;
    if d1 <= d0 || d0 <= 0.0 {
        return None;
    }
    Some((d1 / d0).ln() / (t1 - t0))
}

// ── The frame ───────────────────────────────────────────────────────────────

/// The world→screen mapping: the eight is ~2.0 units wide.
const SCALE: f32 = 300.0;
const CENTRE: (f32, f32) = (640.0, 350.0);

fn project(p: [f32; 2]) -> Offset {
    Offset::new(CENTRE.0 + p[0] * SCALE, CENTRE.1 + p[1] * SCALE)
}

pub fn frame(t: f32) -> WidgetNode {
    let (trail, twin_trail, pos, twin_pos, sep_series) = replay(clamp01(t));
    let sep_now = sep_series
        .last()
        .map(|&(_, d)| d)
        .unwrap_or(DELTA);
    let lambda = growth_rate(&sep_series, 3.0, 11.0);

    // The reveal: the twin rides hidden (rings exactly on the bodies) for
    // the first period, then becomes visible — the same machine, measured
    // twice. (It was always running.)
    let twin_visible = smoothstep((t - 0.45) / 0.12);

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — a void with a star of drift.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.48), 0.9).with_dither().with_stops(&[
                    (0.0, Color::rgb(10, 9, 14)),
                    (1.0, Color::rgb(5, 5, 8)),
                ]),
            );

            // ── The figure-eight, drawn by its own dancers ──────────────
            if trail.len() > 1 {
                // The full swept trail (the curve so far), soft.
                let mut p = Path::new();
                for (i, &q) in trail.iter().enumerate() {
                    let o = project(q);
                    if i == 0 {
                        p.move_to(o);
                    } else {
                        p.line_to(o);
                    }
                }
                book.stroke(p, alpha(VIOLET_SOFT, 0.20), 2.2);

                // The twin's trail, hollow-toned, drawn once it peels.
                if twin_trail.len() > 1 && twin_visible > 0.01 {
                    let mut p2 = Path::new();
                    for (i, &q) in twin_trail.iter().enumerate() {
                        let o = project(q);
                        if i == 0 {
                            p2.move_to(o);
                        } else {
                            p2.line_to(o);
                        }
                    }
                    book.stroke(
                        p2,
                        alpha(MAGENTA, 0.30 * twin_visible),
                        1.6,
                    );
                }
            }

            // ── The bodies ─────────────────────────────────────────────
            // The nominal three, glowing on the curve.
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                for (i, &q) in pos.iter().enumerate() {
                    let o = project(q);
                    g.circle(
                        o,
                        26.0,
                        Gradient::radial_fill().with_dither().with_stops(&[
                            (0.0, alpha(tint(CYAN, 0.35), 0.5)),
                            (1.0, alpha(CYAN, 0.0)),
                        ]),
                    );
                    g.circle(o, 6.5, alpha(INK, 0.96));
                    let _ = i;
                }
            });

            // The twin's three, as rings — over the bodies while locked,
            // then wherever the earthquake took them.
            if twin_visible > 0.01 {
                for &q in twin_pos.iter() {
                    let o = project(q);
                    book.ring(o, 11.0 + 3.0 * twin_visible, 1.8, alpha(MAGENTA, 0.9 * twin_visible));
                }
            } else {
                // Locked phase: a single ring ticks around the lead body
                // as the tell that the second machine is running.
                let pulse = 0.5 + 0.5 * (t * 6.2832 * 2.0).sin();
                let o = project(pos[0]);
                book.ring(o, 12.0, 1.2, alpha(MAGENTA, 0.35 + 0.3 * pulse));
            }

            // ── The separation meter — a log strip ─────────────────────
            // d from δ to 2.0, log scale, plotted from the same series.
            let m_x0 = 170.0;
            let m_x1 = 1110.0;
            let m_y = 626.0;
            book.rrect(
                Rect::new(m_x0 - 20.0, m_y - 34.0, m_x1 + 20.0, m_y + 30.0),
                10.0,
                alpha(Color::rgb(10, 10, 15), 0.9),
            );
            // The log-scale axis: ln(d/δ) from 0 to ln(2/δ).
            let d_lo = DELTA;
            let d_hi = 2.0_f32;
            let l_lo = d_lo.ln();
            let l_hi = d_hi.ln();
            let map = |d: f32, t_frac: f32| -> Offset {
                let l = d.clamp(d_lo, d_hi).ln();
                let x = m_x0 + t_frac * (m_x1 - m_x0);
                let y = m_y + 12.0 - (l - l_lo) / (l_hi - l_lo) * 38.0;
                Offset::new(x, y)
            };
            // Grid lines at decades: δ×10⁰, ×10¹, … ×10⁵.
            let mut decade = DELTA;
            while decade < d_hi {
                let y = map(decade, 0.0).dy;
                book.line(
                    Offset::new(m_x0, y),
                    Offset::new(m_x1, y),
                    alpha(FAINT, 0.35),
                    1.0,
                );
                decade *= 10.0;
            }
            // The curve.
            if sep_series.len() > 1 {
                let t_end = sep_series.last().map(|&(ts, _)| ts).unwrap_or(1.0).max(1e-6);
                let mut p = Path::new();
                for (i, &(ts, d)) in sep_series.iter().enumerate() {
                    let o = map(d, ts / t_end);
                    if i == 0 {
                        p.move_to(o);
                    } else {
                        p.line_to(o);
                    }
                }
                book.stroke(p, alpha(tint(MAGENTA, 0.15), 0.95), 1.8);
                // The straight-line fit, dashed — exponential divergence's
                // signature, drawn through the two fit marks.
                if let Some(lam) = lambda {
                    let t0f = 3.0 / t_end;
                    let t1f = 11.0 / t_end;
                    let d_at = |tm: f32| (DELTA.ln() + lam * tm).exp().min(d_hi).max(d_lo);
                    let a = map(d_at(3.0), t0f);
                    let b = map(d_at(11.0), t1f);
                    let mut fit = Path::new();
                    fit.move_to(a);
                    fit.line_to(b);
                    book.stroke_styled(
                        fit,
                        alpha(tint(CYAN, 0.3), 0.7),
                        1.2,
                        StrokeStyle::rounded().dash(Dash::even(6.0)),
                    );
                }
                // The rider: separation now.
                let o = map(sep_now, 1.0);
                book.circle(o, 3.2, alpha(MAGENTA, 0.95));
            }
            // The axis' start mark: δ itself.
            let o = map(DELTA, 0.0);
            book.circle(o, 2.4, alpha(CYAN, 0.9));
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(sep_now, lambda));
    stack.into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(sep: f32, lambda: Option<f32>) -> WidgetNode {
    let lines = [
        "THREEBODY · THE CHAOS AXIS · THE FIGURE-EIGHT, TWICE".to_string(),
        format!(
            "Chenciner–Montgomery choreography · T = {PERIOD:.5} · RK4 ×{} substeps/T, replayed per frame",
            SUBSTEPS_PER_T
        ),
        format!(
            "twin's initial offset δ = {DELTA:.0e} on one coordinate — integrated in lockstep"
        ),
        format!(
            "separation now: {sep:.4} ({:.0}× δ) · growth λ ≈ {} / period ×{:.2}",
            sep / DELTA,
            lambda.map(|l| format!("{l:.3}")).unwrap_or_else(|| "—".into()),
            lambda.map(|l| (l * PERIOD).exp()).unwrap_or(0.0)
        ),
        "the meter's straight line IS the butterfly effect, measured".to_string(),
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
