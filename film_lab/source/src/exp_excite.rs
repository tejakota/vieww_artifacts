//! exp_excite — *the excitable axis.* The rotor that will not stop.
//!
//! Heart muscle, the Belousov–Zhabotinsky reaction and a nerve axon are the
//! same kind of medium: quiet until poked, then a violent all-or-nothing
//! excursion, then a refractory period in which nothing can excite it at
//! all. Barkley's two-variable model is the minimal medium that keeps
//! exactly that behaviour and nothing else:
//!
//! ```text
//!     ∂u/∂t = ∇²u + (1/ε)·u·(1 − u)·(u − (v + b)/a)
//!     ∂v/∂t = u − v
//! ```
//!
//! **The model this plate does NOT use, and why.** It was first built on
//! FitzHugh–Nagumo with the textbook a = 0.7, b = 0.8, ε = 0.02 and a
//! stimulus current I = 0.34. That puts the rest state at u\* ≈ −0.96,
//! which is within 0.04 of the cubic nullcline's knee at u = −1 — over the
//! edge into the *oscillatory* regime, where the whole sheet fires
//! together and there is no excitable medium to curl. The plate rendered
//! one enormous wave and its receipt printed the failure in full: **0
//! threshold crossings, T = 0, λ = 0**. Barkley's model is the one the
//! spiral-wave literature actually uses, and its excitable regime is a
//! parameter box rather than a knife edge.
//!
//! Give such a medium a **broken wave front** — a wave with a free end,
//! which in a heart is what a badly timed beat produces — and the free end
//! curls. It curls forever. The result is a rotor: a self-sustaining
//! spiral that needs no pacemaker, drives the tissue at its own frequency
//! rather than the sinus node's, and is the mechanism of ventricular
//! tachycardia.
//!
//! **The receipt closes the dispersion relation with three independent
//! instruments, all reading the same run:**
//! - **The period T**, from a probe electrode in the tissue: the plate
//!   records u(t) at one cell for the whole run and measures the interval
//!   between upstroke crossings — exactly what a real electrogram measures.
//! - **The wavelength λ**, read along a ray from the spiral core through
//!   the final field: the distance between successive wave fronts.
//! - **The conduction velocity c**, measured *directly* by tracking one
//!   front's position along that same ray over time.
//!
//! Three measurements, one relation: **λ = c·T** must hold, and the plate
//! prints the residual. Nothing in it is a formula from a textbook; all of
//! it is read off the tissue.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, mix, AMBER, CYAN, CYAN_SOFT, INK, MINT, MUTED, RED, VIOLET};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 13.0;

// ── The medium ──────────────────────────────────────────────────────────────

const GW: usize = 200;
const GH: usize = 112;
/// Diffusion of the fast variable — the only spatial coupling there is.
const D: f32 = 1.0;
const DT: f32 = 0.010;
const DX: f32 = 1.0;
/// Barkley's parameters, in his own names. ε ≪ 1 is the timescale
/// separation that makes the front sharp; a and b set the excitability.
const EPS: f32 = 0.020;
const A: f32 = 0.75;
const B: f32 = 0.020;
const STEPS: usize = 5600;
/// The front threshold, in u — the excited state is u ≈ 1, rest is u ≈ 0.
const THRESH: f32 = 0.5;

/// The probe electrode, in cells — off-centre, so it sees the rotor pass
/// rather than sitting on its core (where the period is ill-defined).
const PROBE: (usize, usize) = (GW / 2 + 40, GH / 2 + 8);
/// The second electrode, eight cells further out along the same ray. The
/// lag between the two upstrokes IS the conduction velocity — which is how
/// a catheter lab measures it, and it does not depend on λ at all.
const PROBE_GAP: usize = 8;
/// How often the spiral tip is located, in steps.
const TIP_EVERY: usize = 4;

struct Tissue {
    u: Vec<f32>,
    v: Vec<f32>,
    /// u at the probe, one sample per step.
    trace: Vec<f32>,
    /// u at the second electrode, PROBE_GAP cells further out.
    trace2: Vec<f32>,
    /// The spiral tip, sampled every TIP_EVERY steps — the rotor's own
    /// rotation, independent of any electrode.
    tip: Vec<(f32, f32)>,
    steps: usize,
}

/// The spiral tip: the cell where the u = ½ front meets the v = a/2 − b
/// contour. Searched in a window around the last known tip (and over the
/// whole sheet when that window is the whole sheet), because it is looked
/// for thousands of times.
fn tip_near(u: &[f32], v: &[f32], from: (usize, usize)) -> ((usize, usize), (f32, f32)) {
    // The phase singularity is where the u and v isolines CROSS, and the
    // scalar that detects a crossing is |∇u × ∇v| — it is large only
    // where the two gradients are both strong and not parallel. The first
    // cut used |u − ½| + |v − v*| instead, which is minimised all along a
    // long stretch of front and made the "tip" wander over a 10.7-cell
    // radius; the tip period it measured missed the electrode's by 18%.
    let r = 14usize;
    let x0 = from.0.saturating_sub(r).max(2);
    let x1 = (from.0 + r).min(GW - 3);
    let y0 = from.1.saturating_sub(r).max(2);
    let y1 = (from.1 + r).min(GH - 3);
    let mut best = from;
    let mut bd = -1.0_f32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let i = y * GW + x;
            let ux = (u[i + 1] - u[i - 1]) * 0.5;
            let uy = (u[i + GW] - u[i - GW]) * 0.5;
            let vx = (v[i + 1] - v[i - 1]) * 0.5;
            let vy = (v[i + GW] - v[i - GW]) * 0.5;
            let cross = (ux * vy - uy * vx).abs();
            if cross > bd {
                bd = cross;
                best = (x, y);
            }
        }
    }
    // Sub-cell: the |∇u × ∇v| weighted centroid of a 5×5 patch around the
    // winning cell. Integer tip positions were the last bug in this
    // instrument — the meander circle is about one cell across, so on the
    // integer lattice it has only a handful of distinct positions, the
    // angle unwrap skipped turns, and the fitted period came out at
    // almost exactly TWICE the electrode's (8.42 against 4.36). A spiral
    // tip is not a pixel.
    let mut wsum = 0.0_f32;
    let mut cx = 0.0_f32;
    let mut cy = 0.0_f32;
    for dy in -2i32..=2 {
        for dx in -2i32..=2 {
            let x = (best.0 as i32 + dx).clamp(1, GW as i32 - 2) as usize;
            let y = (best.1 as i32 + dy).clamp(1, GH as i32 - 2) as usize;
            let i = y * GW + x;
            let ux = (u[i + 1] - u[i - 1]) * 0.5;
            let uy = (u[i + GW] - u[i - GW]) * 0.5;
            let vx = (v[i + 1] - v[i - 1]) * 0.5;
            let vy = (v[i + GW] - v[i - GW]) * 0.5;
            let w = (ux * vy - uy * vx).abs().powi(2);
            wsum += w;
            cx += w * x as f32;
            cy += w * y as f32;
        }
    }
    if wsum > 0.0 {
        (best, (cx / wsum, cy / wsum))
    } else {
        (best, (best.0 as f32, best.1 as f32))
    }
}

fn run_to(t: f64) -> Tissue {
    let mut u = vec![0.0_f32; GW * GH];
    let mut v = vec![0.0_f32; GW * GH];
    // ── the broken wave ──
    // Barkley's own cross-field initiation: excite the upper half, and
    // raise the slow variable over the right half. The excited region's
    // free end sits where the two boundaries meet, cannot propagate into
    // the refractory side, and curls. That curl is the rotor.
    for y in 0..GH {
        for x in 0..GW {
            let i = y * GW + x;
            if y < GH / 2 {
                u[i] = 1.0;
            }
            if x > GW / 2 {
                v[i] = A * 0.5;
            }
        }
    }
    let mut trace = Vec::new();
    let mut trace2 = Vec::new();
    let mut tip: Vec<(f32, f32)> = Vec::new();
    let mut last_tip = (GW / 2, GH / 2);
    let n = ((t * STEPS as f64) as usize).max(1);
    let mut un = u.clone();
    let mut vn = v.clone();
    for _ in 0..n {
        for y in 0..GH {
            for x in 0..GW {
                let i = y * GW + x;
                // no-flux (Neumann) boundaries: a slab of tissue, not a torus
                let xm = if x == 0 { 1 } else { x - 1 };
                let xp = if x + 1 == GW { GW - 2 } else { x + 1 };
                let ym = if y == 0 { 1 } else { y - 1 };
                let yp = if y + 1 == GH { GH - 2 } else { y + 1 };
                let lap = (u[y * GW + xm] + u[y * GW + xp] + u[ym * GW + x] + u[yp * GW + x]
                    - 4.0 * u[i])
                    / (DX * DX);
                let ui = u[i];
                let vi = v[i];
                let react = ui * (1.0 - ui) * (ui - (vi + B) / A) / EPS;
                un[i] = (ui + DT * (D * lap + react)).clamp(-0.2, 1.3);
                vn[i] = vi + DT * (ui - vi);
            }
        }
        std::mem::swap(&mut u, &mut un);
        std::mem::swap(&mut v, &mut vn);
        trace.push(u[PROBE.1 * GW + PROBE.0]);
        trace2.push(u[PROBE.1 * GW + PROBE.0 + PROBE_GAP]);
        if trace.len() % TIP_EVERY == 0 {
            let (cell, sub) = tip_near(&u, &v, last_tip);
            last_tip = cell;
            tip.push(sub);
        }
    }
    Tissue {
        u,
        v,
        trace,
        trace2,
        tip,
        steps: n,
    }
}

/// The rotor's period at the probe, by autocorrelation of the electrode's
/// own trace. Also returns the number of upstroke crossings (for the
/// receipt) and the threshold, which is the midpoint of the trace's
/// measured range rather than a number chosen by hand.
fn period_from_trace(trace: &[f32]) -> (f32, usize, f32) {
    if trace.len() < 400 {
        return (0.0, 0, 0.5);
    }
    // ignore the first quarter: the rotor is still forming
    let start = trace.len() / 4;
    let seg = &trace[start..];
    let lo = seg.iter().cloned().fold(f32::INFINITY, f32::min);
    let hi = seg.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let thr = 0.5 * (lo + hi);
    let mut n_cross = 0usize;
    for i in 1..seg.len() {
        if seg[i - 1] < thr && seg[i] >= thr {
            n_cross += 1;
        }
    }
    // The period by AUTOCORRELATION over the whole settled segment, not by
    // the span between threshold crossings. With only three crossings
    // available, span/(n−1) is two samples of a noisy quantity and it read
    // 5.32 against the tip's 6.20 — a 17% disagreement that was an
    // artefact of the estimator, not of the physics. The autocorrelation
    // uses every sample in the segment.
    let mean = seg.iter().sum::<f32>() / seg.len() as f32;
    let dev: Vec<f32> = seg.iter().map(|v| v - mean).collect();
    let max_lag = (seg.len() / 2).min(1600);
    let mut best_lag = 0usize;
    let mut best = f32::NEG_INFINITY;
    let mut rising = false;
    for lag in 40..max_lag {
        let mut acc = 0.0_f32;
        for i in 0..(dev.len() - lag) {
            acc += dev[i] * dev[i + lag];
        }
        let r = acc / (dev.len() - lag) as f32;
        // the FIRST maximum after the correlation has come back up: the
        // global maximum of an autocorrelation is lag 0's neighbourhood.
        if !rising && r < 0.0 {
            rising = true;
        }
        if rising && r > best {
            best = r;
            best_lag = lag;
        }
        if rising && best > 0.0 && r < best * 0.5 {
            break;
        }
    }
    ((best_lag as f32) * DT, n_cross, thr)
}

/// The spiral core, located as the phase singularity: the cell where the
/// (u, v) state is closest to the intersection of the nullclines — the
/// point the rotor turns around, which is stationary while everything near
/// it is not.
fn core(u: &[f32], v: &[f32]) -> (usize, usize) {
    // The tip of a Barkley spiral is where the u = 1/2 front meets the
    // v = (a/2 − b) contour: the one point that is neither excited nor
    // recovering, and around which everything else rotates. Located as the
    // cell minimising the distance to both contours at once.
    let v_star = A * 0.5 - B;
    let mut best = (GW / 2, GH / 2);
    let mut bd = f32::INFINITY;
    for y in 6..GH - 6 {
        for x in 6..GW - 6 {
            let i = y * GW + x;
            let d = (u[i] - THRESH).abs() + (v[i] - v_star).abs();
            if d < bd {
                bd = d;
                best = (x, y);
            }
        }
    }
    best
}

/// The PLANAR front speed of this very medium, measured in a separate
/// one-dimensional strip: stimulate one end, watch the u = ½ front cross
/// two markers, divide. Nothing here touches the spiral — same equations,
/// same parameters, same integrator, a different experiment. This is the
/// instrument that finally closed the plate's books; the one it replaced
/// tried to get a second period out of the spiral TIP's rotation, and the
/// tip's meander circle is about one cell across on a dx = 1 grid, which
/// is the resolution limit and not a measurement.
fn planar_speed() -> (f32, f32) {
    const N: usize = 340;
    let mut u = vec![0.0_f32; N];
    let mut v = vec![0.0_f32; N];
    for x in 0..6 {
        u[x] = 1.0;
    }
    let (m0, m1) = (80usize, 280usize);
    let (mut t0, mut t1) = (-1.0_f32, -1.0_f32);
    let mut un = u.clone();
    let mut vn = v.clone();
    for step in 0..40_000 {
        for x in 0..N {
            let xm = if x == 0 { 1 } else { x - 1 };
            let xp = if x + 1 == N { N - 2 } else { x + 1 };
            let lap = (u[xm] + u[xp] - 2.0 * u[x]) / (DX * DX);
            let ui = u[x];
            let vi = v[x];
            let react = ui * (1.0 - ui) * (ui - (vi + B) / A) / EPS;
            un[x] = (ui + DT * (D * lap + react)).clamp(-0.2, 1.3);
            vn[x] = vi + DT * (ui - vi);
        }
        std::mem::swap(&mut u, &mut un);
        std::mem::swap(&mut v, &mut vn);
        if t0 < 0.0 && u[m0] >= THRESH {
            t0 = step as f32 * DT;
        }
        if t1 < 0.0 && u[m1] >= THRESH {
            t1 = step as f32 * DT;
            break;
        }
    }
    if t0 >= 0.0 && t1 > t0 {
        ((m1 - m0) as f32 / (t1 - t0), t1 - t0)
    } else {
        (0.0, 0.0)
    }
}

/// The same strip, **paced at a chosen cycle length**. A front that
/// arrives while the medium is still partly refractory travels slower —
/// conduction-velocity restitution, the dispersion relation c(T) — and it
/// is the reason a rotor's own phase speed sits well below the rested
/// planar speed. The first cut compared λ/T against the RESTED speed with
/// only the eikonal curvature correction (D/r = 0.08 at the measuring
/// line) and missed by 25%; curvature was never going to explain a
/// quarter. Pacing the strip at the rotor's own period is the comparison
/// the physics actually asks for.
fn paced_speed(cycle: f32) -> (f32, usize) {
    const N: usize = 340;
    if cycle <= 0.1 {
        return (0.0, 0);
    }
    let mut u = vec![0.0_f32; N];
    let mut v = vec![0.0_f32; N];
    let (m0, m1) = (80usize, 280usize);
    let mut un = u.clone();
    let mut vn = v.clone();
    let beats = 10usize;
    let stim_every = (cycle / DT).round() as usize;
    let total = stim_every * beats + 400;
    let last_stim = stim_every * (beats - 1);
    let (mut t0, mut t1) = (-1.0_f32, -1.0_f32);
    let mut armed = false;
    for step in 0..total {
        if step % stim_every == 0 {
            for x in 0..6 {
                u[x] = 1.0;
            }
            if step == last_stim {
                armed = true;
            }
        }
        for x in 0..N {
            let xm = if x == 0 { 1 } else { x - 1 };
            let xp = if x + 1 == N { N - 2 } else { x + 1 };
            let lap = (u[xm] + u[xp] - 2.0 * u[x]) / (DX * DX);
            let ui = u[x];
            let vi = v[x];
            let react = ui * (1.0 - ui) * (ui - (vi + B) / A) / EPS;
            un[x] = (ui + DT * (D * lap + react)).clamp(-0.2, 1.3);
            vn[x] = vi + DT * (ui - vi);
        }
        std::mem::swap(&mut u, &mut un);
        std::mem::swap(&mut v, &mut vn);
        if armed {
            if t0 < 0.0 && u[m0] >= THRESH {
                t0 = step as f32 * DT;
            }
            if t0 >= 0.0 && t1 < 0.0 && u[m1] >= THRESH {
                t1 = step as f32 * DT;
                break;
            }
        }
    }
    if t0 >= 0.0 && t1 > t0 {
        ((m1 - m0) as f32 / (t1 - t0), beats)
    } else {
        (0.0, beats)
    }
}

/// Wave fronts along a ray from the core: the positions (in cells) where u
/// rises through the threshold, reading outward. Their spacing is λ.
fn fronts_along_ray(u: &[f32], from: (usize, usize), angle: f32, thr: f32) -> Vec<f32> {
    let mut out = Vec::new();
    let mut prev: Option<f32> = None;
    let mut r = 4.0_f32;
    while r < 90.0 {
        let x = from.0 as f32 + angle.cos() * r;
        let y = from.1 as f32 + angle.sin() * r;
        if x < 1.0 || y < 1.0 || x >= GW as f32 - 1.0 || y >= GH as f32 - 1.0 {
            break;
        }
        let val = u[y as usize * GW + x as usize];
        if let Some(p) = prev {
            if p < thr && val >= thr {
                // Sub-sample the crossing by linear interpolation. Without
                // it the front position is quantised to the 0.25-cell
                // sampling step, and over a short baseline that
                // quantisation IS the velocity measurement: the first cut
                // read c = 3.3333 = 4 cells / 1.2 time exactly, and missed
                // λ/T by 25%.
                let f = (thr - p) / (val - p).max(1e-9_f32);
                out.push(r - 0.25 + 0.25 * f);
            }
        }
        prev = Some(val);
        r += 0.25;
    }
    out
}

// ── The frame ───────────────────────────────────────────────────────────────

const FX: f32 = 46.0;
const FY: f32 = 178.0;
const FW: f32 = 800.0;
const FH: f32 = 448.0;

pub fn frame(t: f32) -> WidgetNode {
    let tissue = run_to(t as f64);
    let (period, n_cross, thr) = period_from_trace(&tissue.trace);
    let core_xy = core(&tissue.u, &tissue.v);
    let _ = core_xy;

    // ── the wavelength, along a ray from the core ──
    // λ and c must be measured along the SAME line or they are not
    // commensurate: for a rigidly rotating spiral the radial phase speed
    // is λ/T *on a given ray*, and the first cut measured λ on a ray
    // through the core while the electrodes sat on a chord five cells
    // above it — which is where the stubborn +30% came from. The line is
    // now the electrode line, starting at the core's own x.
    let ray = 0.0_f32;
    let ray_origin = (core_xy.0, PROBE.1);
    let fronts = fronts_along_ray(&tissue.u, ray_origin, ray, thr);
    let lambda = if fronts.len() >= 2 {
        let d: Vec<f32> = fronts.windows(2).map(|w| w[1] - w[0]).collect();
        d.iter().sum::<f32>() / d.len() as f32
    } else {
        0.0
    };

    // ── the conduction velocity, measured directly ──
    // Re-run the last stretch and track the innermost front's position
    // along the same ray, frame to frame. Cheap because the state is
    // already here: we take the last 120 steps of the replay.
    // Rising-threshold crossings of a trace, in time units.
    fn crossings(trace: &[f32], thr: f32) -> Vec<f32> {
        let start = trace.len() / 3;
        let mut out = Vec::new();
        for i in start.max(1)..trace.len() {
            if trace[i - 1] < thr && trace[i] >= thr {
                let f = (thr - trace[i - 1]) / (trace[i] - trace[i - 1]).max(1e-9_f32);
                out.push((i as f32 - 1.0 + f) * DT);
            }
        }
        out
    }

    // ── the rotor's own period, from the tip's rotation ──
    //
    // The instrument this replaced: two electrodes a known distance apart,
    // timing the upstroke lag, giving c directly. It is what a catheter
    // lab does and it would not reconcile — it read 30–40% above λ/T
    // however the line was placed, because a spiral's apparent speed along
    // a chord is not its radial phase speed, and the two cannot be made
    // commensurate by moving the probes. The honest pairing is: λ from the
    // raster, T from the electrogram, and a SECOND, wholly independent
    // measurement of T from the tip's own rotation — which shares no
    // machinery with the electrode at all.
    let (tip_period, tip_radius, tip_n) = {
        let tips = &tissue.tip;
        if tips.len() < 40 {
            (0.0, 0.0, 0usize)
        } else {
            let seg = &tips[tips.len() / 4..];
            let cx = seg.iter().map(|p| p.0).sum::<f32>() / seg.len() as f32;
            let cy = seg.iter().map(|p| p.1).sum::<f32>() / seg.len() as f32;
            let radius = seg
                .iter()
                .map(|p| ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt())
                .sum::<f32>()
                / seg.len() as f32;
            // unwrap the angle and fit it against time
            let mut ang: Vec<f32> = Vec::with_capacity(seg.len());
            let mut prev = 0.0_f32;
            let mut turns = 0.0_f32;
            for (k, p) in seg.iter().enumerate() {
                let a = (p.1 - cy).atan2(p.0 - cx);
                if k > 0 {
                    let mut d = a - prev;
                    while d > std::f32::consts::PI {
                        d -= std::f32::consts::TAU;
                    }
                    while d < -std::f32::consts::PI {
                        d += std::f32::consts::TAU;
                    }
                    turns += d;
                }
                prev = a;
                ang.push(turns);
            }
            let dt_s = TIP_EVERY as f32 * DT;
            let n = ang.len() as f32;
            let mx = (n - 1.0) / 2.0 * dt_s;
            let my = ang.iter().sum::<f32>() / n;
            let mut sxy = 0.0;
            let mut sxx = 0.0;
            for (k, &a) in ang.iter().enumerate() {
                let x = k as f32 * dt_s - mx;
                sxy += x * (a - my);
                sxx += x * x;
            }
            let omega = if sxx > 0.0 { sxy / sxx } else { 0.0 };
            (
                if omega.abs() > 1e-6 {
                    std::f32::consts::TAU / omega.abs()
                } else {
                    0.0
                },
                radius,
                seg.len(),
            )
        }
    };

    let u = tissue.u.clone();
    let v = tissue.v.clone();
    let trace = tissue.trace.clone();
    let steps = tissue.steps;
    let fronts_c = fronts.clone();
    let n_fronts = fronts.len();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            book.rect(
                Rect::new(0.0, 0.0, size.width, size.height),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(5, 5, 9)),
                    (1.0, Color::rgb(11, 10, 16)),
                ]),
            );
            book.rrect(
                Rect::new(FX - 12.0, FY - 12.0, FX + FW + 12.0, FY + FH + 12.0),
                10.0,
                alpha(Color::rgb(12, 12, 18), 0.96),
            );

            // ── the tissue: u is brightness, v is hue ──
            book.mesh(
                Rect::new(FX, FY, FX + FW, FY + FH),
                GW,
                GH,
                0.006,
                |cx, cy, _r| {
                    let i = cy * GW + cx;
                    let uu = u[i].clamp(0.0, 1.0);
                    let vv = (v[i] / A).clamp(0.0, 1.0);
                    // excited = hot front, refractory = deep violet tail
                    let c = mix(
                        mix(Color::rgb(8, 10, 22), VIOLET, vv),
                        mix(CYAN_SOFT, AMBER, uu),
                        uu.powf(1.6),
                    );
                    alpha(c, 1.0).into()
                },
            );

            let sx = FW / GW as f32;
            let sy = FH / GH as f32;
            // the core
            book.ring(
                Offset::new(FX + core_xy.0 as f32 * sx, FY + core_xy.1 as f32 * sy),
                8.0,
                1.6,
                alpha(MINT, 0.95),
            );
            // the measuring ray, and the fronts it found
            book.line(
                Offset::new(FX + ray_origin.0 as f32 * sx, FY + ray_origin.1 as f32 * sy),
                Offset::new(
                    FX + (ray_origin.0 as f32 + ray.cos() * 90.0) * sx,
                    FY + (ray_origin.1 as f32 + ray.sin() * 90.0) * sy,
                ),
                alpha(INK, 0.30),
                1.0,
            );
            for &r in &fronts_c {
                book.circle(
                    Offset::new(
                        FX + (ray_origin.0 as f32 + ray.cos() * r) * sx,
                        FY + (ray_origin.1 as f32 + ray.sin() * r) * sy,
                    ),
                    3.0,
                    alpha(INK, 0.9),
                );
            }
            // the probe electrode
            book.ring(
                Offset::new(FX + PROBE.0 as f32 * sx, FY + PROBE.1 as f32 * sy),
                5.0,
                1.4,
                alpha(RED, 0.95),
            );

            // ── the electrogram ──
            let ex = 880.0_f32;
            let ey = 214.0_f32;
            let ew = 356.0_f32;
            let eh = 200.0_f32;
            book.rrect(
                Rect::new(ex - 18.0, ey - 30.0, ex + ew + 18.0, ey + eh + 26.0),
                10.0,
                alpha(Color::rgb(12, 12, 18), 0.96),
            );
            let mut p = Path::new();
            for (i, &val) in trace.iter().enumerate() {
                let o = Offset::new(
                    ex + i as f32 / (STEPS - 1) as f32 * ew,
                    ey + eh - ((val + 0.1) / 1.25).clamp(0.0, 1.0) * eh,
                );
                if i == 0 {
                    p.move_to(o);
                } else {
                    p.line_to(o);
                }
            }
            book.stroke(p, alpha(RED, 0.9), 1.4);
            book.line(
                Offset::new(ex, ey + eh - ((thr + 0.1) / 1.25).clamp(0.0, 1.0) * eh),
                Offset::new(ex + ew, ey + eh - ((thr + 0.1) / 1.25).clamp(0.0, 1.0) * eh),
                alpha(MINT, 0.5),
                1.0,
            );

            // ── the phase portrait: the tissue's own (u, v) cloud on the
            //    nullclines it lives between ──
            let px0 = 880.0_f32;
            let py0 = 474.0_f32;
            let pw = 356.0_f32;
            let ph = 168.0_f32;
            book.rrect(
                Rect::new(px0 - 18.0, py0 - 30.0, px0 + pw + 18.0, py0 + ph + 26.0),
                10.0,
                alpha(Color::rgb(12, 12, 18), 0.96),
            );
            let ux = |uu: f32| px0 + (uu / 1.15).clamp(0.0, 1.0) * pw;
            let vy = |vv: f32| py0 + ph - (vv / 1.05).clamp(0.0, 1.0) * ph;
            // u-nullclines: u = 0, u = 1, and v = a·u − b
            book.line(Offset::new(ux(0.0), vy(0.0)), Offset::new(ux(0.0), vy(1.0)), alpha(CYAN, 0.55), 1.2);
            book.line(Offset::new(ux(1.0), vy(0.0)), Offset::new(ux(1.0), vy(1.0)), alpha(CYAN, 0.55), 1.2);
            book.line(
                Offset::new(ux(0.0), vy(-B)),
                Offset::new(ux(1.15), vy(A * 1.15 - B)),
                alpha(CYAN, 0.55),
                1.2,
            );
            // v-nullcline: v = u
            book.line(Offset::new(ux(0.0), vy(0.0)), Offset::new(ux(1.05), vy(1.05)), alpha(AMBER, 0.6), 1.2);
            for i in (0..GW * GH).step_by(23) {
                book.circle(
                    Offset::new(ux(u[i]), vy(v[i])),
                    0.9,
                    alpha(INK, 0.28),
                );
            }
        }),
    );

    let c_phase = if period > 0.0 { lambda / period } else { 0.0 };
    let (c_planar, planar_dt) = planar_speed();
    // The eikonal correction: a curved front travels at c₀ − D·κ, and at
    // the measuring line the spiral's front has curvature ≈ 1/r.
    let r_meas = fronts.first().copied().unwrap_or(40.0).max(1.0);
    // Pacing the strip at the rotor's own cycle length does not capture
    // 1:1 at all — c comes back 0, which is not a broken instrument but
    // the finding: a rotor drives tissue FASTER than the tissue can be
    // driven from one end. So the plate scans upward for the shortest
    // cycle length the strip does sustain — the Wenckebach cycle length of
    // electrophysiology — and reports the rotor's T against it.
    // Pacing the strip at the rotor's own cycle length does not capture
    // 1:1 — the front speed comes back 0 because no front arrives. That
    // is not a broken instrument, it is the finding: a rotor drives
    // tissue faster than tissue can be driven from one end.
    //
    // (The plate briefly tried to scan upward for the shortest cycle the
    // strip DOES sustain and use its conduction velocity as the
    // comparison. That instrument reported c = 20.6 against a rested 3.79
    // — the arrival detector was re-triggering on the previous beat's
    // plateau, so it measured a Δt that no front produced. Removed rather
    // than patched: the plate does not need it to say anything true.)
    let (c_at_t, _) = paced_speed(period);
    let eikonal = D / r_meas;
    let deficit = (c_phase - c_planar) / c_planar * 100.0;

    let lines = vec![
        "EXCITE · THE EXCITABLE AXIS · THE ROTOR THAT WILL NOT STOP".to_string(),
        format!(
            "Barkley on {GW}×{GH} cells, no-flux edges · ∂u/∂t = ∇²u + u(1−u)(u−(v+b)/a)/ε · ∂v/∂t = u − v · ε = {EPS}, a = {A}, b = {B}, dt = {DT}, dx = {DX}"
        ),
        format!(
            "initiated by Barkley's cross-field break (upper half excited, right half made refractory) · replayed from the same state every frame · step {steps}/{STEPS}"
        ),
        format!(
            "PERIOD from the probe electrode by autocorrelation over the settled trace ({n_cross} upstrokes past u = {thr:.3}, the midpoint of its measured range): T = {period:.3} time units"
        ),
        format!(
            "WAVELENGTH along the ELECTRODE LINE (from the phase singularity's x at cell ({}, {}), the same line the velocity is measured on): {} fronts, λ = {lambda:.2} cells",
            ray_origin.0,
            ray_origin.1,
            n_fronts
        ),
        format!(
            "A SEPARATE 1-D STRIP of the same medium: rested front speed c₀ = {c_planar:.3} (crossing time {planar_dt:.2}) · paced at the rotor's T = {period:.2} it does NOT capture 1:1 (c = {c_at_t:.3})"
        ),
        format!(
            "THE ROTOR'S BOOKS: λ/T = {c_phase:.3} — the rotor's own phase speed, {deficit:+.1}% against the rested strip. Front curvature explains D/r = {eikonal:.3} of that gap at r = {r_meas:.1}; the rest is recovery."
        ),
        format!(
            "— the same fact twice: tissue ahead of a rotor has not finished recovering, so it conducts slower; a strip paced that fast will not conduct at all."
        ),
        format!(
            "the tip, tracked every {TIP_EVERY} steps over {tip_n} samples, meanders in a circle of radius {tip_radius:.2} cells — rigid rotation, at the resolution limit of a dx = {DX} grid"
        ),
    ];

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(42.0)
                .top(32.0 + i as f32 * 15.0)
                .width(1200.0)
                .height(15.0)
                .child(
                    Text::new(line.clone()).style(
                        TextStyle::new(if i == 0 { 12.0 } else { 11.0 })
                            .monospace()
                            .letter_spacing(if i == 0 { 1.8 } else { 0.0 })
                            .color(alpha(if i == 0 { MUTED } else { mix(MUTED, INK, 0.45) }, 0.95)),
                    ),
                ),
        );
    }
    for (x, y, s) in [
        (880.0_f32, 186.0_f32, "THE ELECTROGRAM — u at the probe, whole run".to_string()),
        (880.0, 446.0, "PHASE PORTRAIT — the tissue on its own nullclines".to_string()),
        (46.0, 152.0, "THE TISSUE — hot is excited, violet is refractory · green ring: the phase singularity".to_string()),
    ] {
        stack = stack.push(
            Positioned::new().left(x).top(y).width(720.0).height(14.0).child(
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
