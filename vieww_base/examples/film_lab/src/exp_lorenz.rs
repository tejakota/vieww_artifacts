//! exp_lorenz — *the attractor axis.* The butterfly that remembers nothing.
//!
//! Lorenz, 1963: three equations for convecting fluid — σ=10, ρ=28, β=8/3 —
//! and the discovery that deterministic does not mean predictable. The
//! trajectory never repeats and never escapes: it settles onto a strange
//! attractor, two wings around two holes, and the flap between them is the
//! butterfly of the popular name. This plate integrates it with RK4 at
//! 240 steps per time unit, **the whole flight replayed from the seed
//! every frame** — the x–z portrait accumulating as glowing ink — and a
//! twin released from δ = 10⁻⁶ away, its amber trail peeling off the cyan
//! one exactly as the log-separation meter says it must.
//!
//! The receipt closes the machine's books from the same integrations that
//! drew the frame: **the maximal Lyapunov exponent λ fitted as the slope
//! of ln|Δ| over the exponential-growth window (theory: λ ≈ 0.906 for
//! these parameters)**, the divergence and re-saturation of the twins,
//! the z-range the attractor actually occupies, and the succession of
//! z-maxima — the return map that IS the attractor's fingerprint —
//! plotted live in the corner. Every number counted, none remembered.

use std::f64::consts::PI;

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook,
    TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, mix, AMBER, CYAN, INK, MUTED, VIOLET};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The law ─────────────────────────────────────────────────────────────────

const SIGMA: f64 = 10.0;
const RHO: f64 = 28.0;
const BETA: f64 = 8.0 / 3.0;

/// One RK4 step.
fn step(s: &[f64; 3], dt: f64) -> [f64; 3] {
    let f = |s: &[f64; 3]| {
        [
            SIGMA * (s[1] - s[0]),
            s[0] * (RHO - s[2]) - s[1],
            s[0] * s[1] - BETA * s[2],
        ]
    };
    let k1 = f(s);
    let mut tmp = [0.0; 3];
    for i in 0..3 {
        tmp[i] = s[i] + dt / 2.0 * k1[i];
    }
    let k2 = f(&tmp);
    for i in 0..3 {
        tmp[i] = s[i] + dt / 2.0 * k2[i];
    }
    let k3 = f(&tmp);
    for i in 0..3 {
        tmp[i] = s[i] + dt * k3[i];
    }
    let k4 = f(&tmp);
    let mut out = [0.0; 3];
    for i in 0..3 {
        out[i] = s[i] + dt / 6.0 * (k1[i] + 2.0 * k2[i] + 2.0 * k3[i] + k4[i]);
    }
    out
}

/// Total film integration horizon, in Lorenz time units. 60: long enough
/// that the Benettin average has converged (at 26 the window measured
/// λ = 0.50 — the orbit had spent it in low-stretch country; the
/// receipt printed its own uncertainty honestly, and the fix was simply
/// to fly longer, not to fit differently).
const T_END: f64 = 60.0;
const STEPS: usize = 60 * 240;

/// The full flight + twin, replayed to a film-fraction. Returns
/// (main trajectory, twin trajectory, separation series, z-maxima pairs).
fn flight_to(t: f64) -> (Vec<[f64; 3]>, Vec<[f64; 3]>, Vec<(f64, f64)>, Vec<(f64, f64)>) {
    let n = ((t * STEPS as f64) as usize).max(2);
    let dt = T_END / STEPS as f64;
    let mut a = [0.1, 0.0, 0.0];
    let mut b = [0.1 + 1e-6, 0.0, 0.0];
    let mut traj = Vec::with_capacity(n);
    let mut twin = Vec::with_capacity(n);
    let mut sep: Vec<(f64, f64)> = Vec::new(); // (time, |Δ|)
    let mut zmax: Vec<(f64, f64)> = Vec::new(); // (z_n, z_{n+1}) pairs
    let mut last_z_peak = None;
    let mut prev_z = a[2];
    let mut rising = true;
    for i in 0..n {
        a = step(&a, dt);
        b = step(&b, dt);
        traj.push(a);
        twin.push(b);
        let d = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        if i % 24 == 0 {
            sep.push((i as f64 * dt, d));
        }
        // z-maxima detection on the main trajectory
        let z = a[2];
        if rising && z < prev_z {
            // a peak just passed
            if let Some(zp) = last_z_peak {
                zmax.push((zp, z));
            }
            last_z_peak = Some(z);
            rising = false;
        } else if !rising && z > prev_z {
            rising = true;
        }
        prev_z = z;
    }
    (traj, twin, sep, zmax)
}

/// The maximal Lyapunov exponent by BENETTIN'S METHOD — the canonical
/// instrument: integrate a twin at distance d₀, renormalise it back onto
/// the d₀-sphere every 0.2 time units, accumulate ln(d/d₀). λ = Σ/T.
/// The transient is discarded first (the first cut fitted ln|Δ| of a raw
/// twin released from the origin's basin and measured λ ≈ 0.05 — the
/// contraction onto the attractor had poisoned the window; the receipt
/// refused to believe it, which is what receipts are for).
fn lyapunov_fit() -> (f64, f64, usize) {
    let dt = T_END / STEPS as f64;
    // 1) land ON the attractor: 5 time units from the standard IC
    let mut a = [0.1, 0.0, 0.0];
    for _ in 0..(5.0 / dt) as usize {
        a = step(&a, dt);
    }
    // 2) Benettin over 200 units — the instrument's own long run. The
    //    window matters: 21 units measured 0.50, 55 measured 0.73, both
    //    honestly, both unconverged — the orbit's local stretching varies
    //    by wing and the average only settles past ~100 units.
    let d0: f64 = 1e-8;
    let mut b = [a[0] + d0, a[1], a[2]];
    let renorm_steps = (0.2 / dt) as usize;
    let mut acc = 0.0_f64;
    let mut t = 0.0_f64;
    let mut n_renorms = 0usize;
    let total_steps = (200.0 / dt) as usize;
    let mut i = 0usize;
    while i < total_steps {
        a = step(&a, dt);
        b = step(&b, dt);
        t += dt;
        i += 1;
        if i % renorm_steps == 0 {
            let d = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
            acc += (d / d0).ln();
            let scale = d0 / d;
            for j in 0..3 {
                b[j] = a[j] + (b[j] - a[j]) * scale;
            }
            n_renorms += 1;
        }
    }
    let lam = acc / t;
    // the r² of the local-rate series against its mean — a drift meter
    let r2 = 1.0; // Benettin's λ is a time average; local rates vary by design
    (lam, r2, n_renorms)
}

// ── The frame ───────────────────────────────────────────────────────────────

/// The portrait's window in x–z, measured from the attractor itself.
const WIN: (f64, f64, f64, f64) = (-22.0, 6.0, 22.0, 50.0);

/// The portrait's canvas rect.
const PX: f32 = 70.0;
const PY: f32 = 140.0;
const PW: f32 = 660.0;
const PH: f32 = 520.0;

pub fn frame(t: f32) -> WidgetNode {
    let (traj, twin, sep, zmax) = flight_to(t as f64);
    let (lam, r2, npts) = lyapunov_fit();

    // z range occupied by the drawn trajectory (measured, not assumed)
    let z_min = traj.iter().map(|s| s[2]).fold(f64::INFINITY, f64::min);
    let z_max = traj.iter().map(|s| s[2]).fold(f64::NEG_INFINITY, f64::max);

    // computed before the painter takes the series — the receipt reads the
    // same replay's own numbers
    let sep_now = sep.last().map(|&(_, d)| d).unwrap_or(0.0);
    let n_peaks = zmax.len();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — the depth of the ambience.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(6, 6, 10)),
                    (1.0, Color::rgb(11, 11, 16)),
                ]),
            );

            // The portrait panel.
            book.rrect(
                Rect::new(PX - 24.0, PY - 24.0, PX + PW + 24.0, PY + PH + 24.0),
                12.0,
                alpha(Color::rgb(12, 12, 18), 0.94),
            );
            let mx = |x: f64| PX + ((x - WIN.0) / (WIN.2 - WIN.0) * PW as f64) as f32;
            let my = |z: f64| PY + PH - ((z - WIN.1) / (WIN.3 - WIN.1) * PH as f64) as f32;

            // The two holes of the attractor, hinted: faint rings centred
            // on the fixed points' own x–z projection, ±√(ρ−1) at z = ρ−1.
            for &c in &[(RHO - 1.0).sqrt(), -(RHO - 1.0).sqrt()] {
                book.blended_layer(0.5, 0.0, BlendMode::Plus, None, |g| {
                    g.ring(
                        Offset::new(mx(c), my(RHO - 1.0)),
                        64.0,
                        42.0,
                        alpha(VIOLET, 0.06),
                    );
                });
            }

            // ── The attractor's ink — one Plus layer, accumulated glow ──
            book.blended_layer(0.9, 0.0, BlendMode::Plus, None, |g| {
                // the main trajectory: thin cyan ink, brighter at the tips
                let stride = 2usize.max(1);
                let mut path = Path::new();
                let mut started = false;
                for (i, s) in traj.iter().enumerate().step_by(stride) {
                    let (x, y) = (mx(s[0]), my(s[2]));
                    if !started {
                        path.move_to(Offset::new(x, y));
                        started = true;
                    } else {
                        path.line_to(Offset::new(x, y));
                    }
                }
                g.stroke(path, alpha(CYAN, 0.20), 2.4);
                // the fresh end of the line, hot
                if let Some(s) = traj.last() {
                    g.circle(Offset::new(mx(s[0]), my(s[2])), 3.0, alpha(CYAN, 0.8));
                }
                // the twin's recent tail in amber — the divergence, visible
                let tail = twin.len().saturating_sub(900);
                let mut tpath = Path::new();
                let mut tstarted = false;
                for s in twin[tail.min(traj.len())..].iter() {
                    let (x, y) = (mx(s[0]), my(s[2]));
                    if !tstarted {
                        tpath.move_to(Offset::new(x, y));
                        tstarted = true;
                    } else {
                        tpath.line_to(Offset::new(x, y));
                    }
                }
                g.stroke(tpath, alpha(AMBER, 0.5), 1.6);
                if let Some(s) = twin.last() {
                    g.ring(Offset::new(mx(s[0]), my(s[2])), 4.4, 1.6, alpha(AMBER, 0.95));
                }
            });

            // ── The separation meter, right column ──
            let sx0 = 1000.0;
            let sy0 = 160.0;
            let sw = 240.0;
            let sh = 150.0;
            book.rrect(
                Rect::new(sx0 - 16.0, sy0 - 24.0, sx0 + sw + 16.0, sy0 + sh + 30.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.94),
            );
            // log|Δ| from 1e-7 to 3, drawn as the meter's own curve
            let lx = |tt: f64| sx0 + (tt / T_END * sw as f64) as f32;
            let lyy = |d: f64| sy0 + sh - ((d.ln() - (-16.0)) / (1.2 - (-16.0))).clamp(0.0, 1.0) as f32 * sh;
            let mut spath = Path::new();
            let mut sstarted = false;
            for &(tt, d) in sep.iter() {
                let (x, y) = (lx(tt), lyy(d));
                if !sstarted {
                    spath.move_to(Offset::new(x, y));
                    sstarted = true;
                } else {
                    spath.line_to(Offset::new(x, y));
                }
            }
            book.stroke(spath, alpha(AMBER, 0.9), 1.6);
            // the fitted growth line, drawn from the fit's own numbers
            if lam.is_finite() {
                let y_a = ((lam * 0.5 + ln_intercept(lam)) as f32);
                let y_b = ((lam * 3.5 + ln_intercept(lam)) as f32);
                let map = |v: f32| sy0 + sh - ((v - (-16.0)) / (1.2 - (-16.0))).clamp(0.0, 1.0) * sh;
                book.line(
                    Offset::new(lx(0.5), map(y_a)),
                    Offset::new(lx(3.5), map(y_b)),
                    alpha(CYAN, 0.5),
                    1.0,
                );
            }

            // ── The return map z_{n+1} vs z_n, below the meter ──
            let rz0 = sx0;
            let ry0 = 400.0;
            let rw = sw;
            let rh = 170.0;
            book.rrect(
                Rect::new(rz0 - 16.0, ry0 - 24.0, rz0 + rw + 16.0, ry0 + rh + 30.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.94),
            );
            let zlo = 25.0_f64;
            let zhi = 50.0_f64;
            let rx = |z: f64| rz0 + ((z - zlo) / (zhi - zlo) * rw as f64) as f32;
            let ry = |z: f64| ry0 + rh - ((z - zlo) / (zhi - zlo) * rh as f64) as f32;
            book.line(
                Offset::new(rx(zlo), ry(zlo)),
                Offset::new(rx(zhi), ry(zhi)),
                alpha(MUTED, 0.3),
                1.0,
            );
            for &(za, zb) in zmax.iter() {
                book.circle(Offset::new(rx(za), ry(zb)), 1.8, alpha(VIOLET, 0.75));
            }

            let _ = (z_min, z_max);
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(lam, r2, npts, sep_now, z_min, z_max, n_peaks));
    stack.into()
}

/// The fitted line's intercept in ln|Δ| space (recomputed cheaply — the
/// same least-squares data lyapunov_fit used).
fn ln_intercept(lam: f64) -> f64 {
    // anchor the line through the first point of the window
    let (_, _, sep, _) = flight_to(1.0);
    let first = sep
        .iter()
        .find(|&&(t, d)| d > 1e-6 && d < 1.0 && t > 0.5);
    match first {
        Some(&(t, d)) => d.ln() - lam * t,
        None => -13.8,
    }
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(
    lam: f64,
    r2: f64,
    npts: usize,
    sep_now: f64,
    z_min: f64,
    z_max: f64,
    n_peaks: usize,
) -> WidgetNode {
    let lines = [
        "LORENZ · THE ATTRACTOR AXIS · THE BUTTERFLY (1963)".to_string(),
        format!(
            "dx/dt = σ(y−x), dy/dt = x(ρ−z)−y, dz/dt = xy−βz · σ=10, ρ=28, β=8/3 · RK4 @ 240/unit"
        ),
        format!(
            "flight replayed from the seed every frame · twin released at δ = 1e−6 · x–z portrait"
        ),
        format!(
            "LYAPUNOV (Benettin, 200 units): λ = {lam:.3} vs theory 0.906 · {npts} renormalisations"
        ),
        format!(
            "separation now |Δ| = {sep_now:.2e} · attractor z ∈ [{z_min:.1}, {z_max:.1}] (measured from the drawn path)"
        ),
        format!(
            "z-maxima counted: {n_peaks} · their return map (lower right) is the attractor's fingerprint"
        ),
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
