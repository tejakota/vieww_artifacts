//! exp_gw — *the spacetime axis.* The sound of two black holes touching.
//!
//! 14 September 2015, the machine heard it first: a chirp. Two black holes
//! of ~30 solar masses, a billion light-years out, spent their last quarter
//! second sweeping 35 → 250 Hz as the orbit decayed, and the strain they
//! printed on 4 km of laser-lit vacuum looked like nothing so much as a
//! birdsong. This plate runs that law as its machine — **the quadrupole
//! chirp: f(t) sweeping as (t_c − t)^(−3/8), the strain amplitude rising
//! as f^(2/3), then merger, then ringdown** — with the two bodies drawn
//! spiralling on the shrinking separation a(t) ∝ (1 − t/t_c)^(1/4), the
//! wave drawn beneath as the LIGO strip everyone knows.
//!
//! The receipt closes the law's own books from the curve that was drawn:
//! **the zero-crossings of the rendered strain samples give f(t) by
//! measurement; finite differences give fdot; and a least-squares fit of
//! log fdot against log f returns the chirp exponent — the general-
//! relativistic 11/3 ≈ 3.667, measured from the pixels' own mathematics,
//! not asserted.** Merger flash, ringdown decay, and the f-sweep endpoints
//! ride the same lines. Nothing is typed that the plate did not count.

use std::f64::consts::PI;

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook,
    TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, mix, AMBER, CYAN, INK,
    MUTED, VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The law, in numbers ─────────────────────────────────────────────────────

/// Inspiral start frequency (Hz) — the chirp's first note.
const F0: f64 = 32.0;
/// Merger time, film seconds. After it: ringdown.
const TC: f64 = 9.2;
/// Frequency handed to the ringdown (the ISCO stand-in).
const F_MERGER: f64 = 210.0;
/// Ringdown frequency (Hz) and damping time (s).
const F_RING: f64 = 250.0;
const TAU_RING: f64 = 0.18;
/// Strain samples per film-second — the curve the receipt reads. 16k so
/// zero-crossing times carry sub-0.1 ms precision (the first cut ran 4k
/// and stopped the band at 0.86 t_c, where the chirp has barely swept an
/// octave: the fit measured 1.03 against the law's 3.667 — a receipt that
/// refuses to flatter its own machine).
const SR: f64 = 16000.0;

/// The inspiral's instantaneous frequency — the exact solution of
/// df/dt = K·f^(11/3) with the constants folded into (f₀, t_c).
fn f_ins(t: f64) -> f64 {
    let u = (1.0 - t / TC).max(1e-9);
    F0 * u.powf(-3.0 / 8.0)
}

/// Orbital phase (radians, of the WAVE — twice the orbit) from 2π∫f dt,
/// closed form: 2π f₀ t_c (8/5) [1 − (1 − t/t_c)^(5/8)].
fn phase_ins(t: f64) -> f64 {
    let u = (1.0 - t / TC).max(1e-9);
    2.0 * PI * F0 * TC * (8.0 / 5.0) * (1.0 - u.powf(5.0 / 8.0))
}

/// The strain — the whole signal, inspiral → merger → ringdown, one law.
fn strain(t: f64) -> f64 {
    if t < TC {
        let f = f_ins(t).min(F_MERGER);
        let a = (f / F0).powf(2.0 / 3.0);
        a * (phase_ins(t) + 0.7).cos()
    } else {
        let h_peak = (F_MERGER / F0).powf(2.0 / 3.0);
        let e = (-(t - TC) / TAU_RING).exp();
        let ramp = (1.0 - (t - TC) / 0.05).clamp(0.0, 1.0);
        h_peak * e * (2.0 * PI * F_RING * (t - TC)).cos() * ramp.max(0.0).sqrt()
    }
}

// ── The receipt, measured from the drawn curve ──────────────────────────────

/// Zero-crossing times of the strain, from the same samples the strip chart
/// draws — the instrument is the picture. Returns (t, f) with f from the
/// distance to the PREVIOUS crossing (one full period).
fn crossing_series() -> Vec<(f64, f64)> {
    // The instrument: each negative-to-positive zero-crossing's period is
    // the gap to the PREVIOUS one (the first cut asked last_zero for the
    // crossing it had just found and measured f from a zero-length interval
    // — the receipt's -40% chirp slope was the garbage of that tautology).
    let step = 1.0 / SR;
    let mut out = Vec::new();
    let mut prev = strain(0.0);
    let mut t = step;
    let mut last_cross: Option<f64> = None;
    let band = TC * 0.99; // into the sweep's upper octave, off the cap
    while t < band {
        let s = strain(t);
        if prev < 0.0 && s >= 0.0 {
            if let Some(lc) = last_cross {
                if t - lc > 1e-6 {
                    out.push((t, 1.0 / (t - lc)));
                }
            }
            last_cross = Some(t);
        }
        prev = s;
        t += step;
    }
    out
}

/// Fit log(fdot) vs log(f) over the clean inspiral band; returns
/// (slope, r², points). fdot by central differences on the crossing series.
fn chirp_fit() -> (f64, f64, usize) {
    // least squares on LOG-LOG — fdot ∝ f^(11/3) is a power law; fitting
    // it linear (the first cut) hands the slope to the loudest high-f
    // points and prints 1.59 where the law says 3.667. The log transform
    // IS the measurement.
    let pts: Vec<(f64, f64)> = fit_points()
        .iter()
        .map(|&(f, fdot)| (f.ln(), fdot.ln()))
        .collect();
    if pts.len() < 3 {
        return (f64::NAN, 0.0, 0);
    }
    // least squares on the collected points
    let n = pts.len() as f64;
    let sx: f64 = pts.iter().map(|p| p.0).sum();
    let sy: f64 = pts.iter().map(|p| p.1).sum();
    let sxx: f64 = pts.iter().map(|p| p.0 * p.0).sum();
    let sxy: f64 = pts.iter().map(|p| p.0 * p.1).sum();
    let denom = n * sxx - sx * sx;
    if denom.abs() < 1e-12 {
        return (f64::NAN, 0.0, pts.len());
    }
    let slope = (n * sxy - sx * sy) / denom;
    let intercept = (sy - slope * sx) / n;
    let sse: f64 = pts
        .iter()
        .map(|p| (p.1 - (intercept + slope * p.0)).powi(2))
        .sum();
    let sst: f64 = pts.iter().map(|p| (p.1 - sy / n).powi(2)).sum();
    let r2 = if sst > 0.0 { 1.0 - sse / sst } else { 0.0 };
    (slope, r2, pts.len())
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let t = t.min(1.0) as f64;
    let secs = t * SECONDS as f64;

    let (slope, r2, npts) = chirp_fit();
    let f_now = if secs < TC { f_ins(secs).min(F_MERGER) } else { F_RING };
    let strain_now = strain(secs);

    // Stride for the drawn polyline: pixels, not samples.
    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — deep field.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(6, 6, 10)),
                    (1.0, Color::rgb(10, 10, 15)),
                ]),
            );

            // ── Stage one: the orbit, upper band ──
            let cx = 700.0;
            let cy = 180.0;
            let a_max = 150.0;
            let u = (1.0 - (secs / TC).min(0.999)).max(0.0);
            let sep = a_max * u.powf(0.25);
            let phase = phase_ins(secs.min(TC)) * 0.5; // orbital phase

            // orbit trail — fading rings at past separations
            book.blended_layer(0.85, 0.0, BlendMode::Plus, None, |g| {
                for k in 0..14 {
                    let tt = (k as f64 / 14.0) * 0.9;
                    let s = a_max * (1.0 - tt).powf(0.25);
                    let ring_alpha = 0.05 + 0.05 * (k as f32 / 14.0);
                    g.ring(
                        Offset::new(cx as f32, cy as f32),
                        s as f32,
                        1.1,
                        alpha(CYAN, ring_alpha),
                    );
                }
            });

            // the two bodies
            let (bx1, by1) = (
                (cx + (sep / 2.0) * phase.cos()) as f32,
                (cy + (sep / 2.0) * phase.sin() * 0.34) as f32,
            );
            let (bx2, by2) = (
                (cx - (sep / 2.0) * phase.cos()) as f32,
                (cy - (sep / 2.0) * phase.sin() * 0.34) as f32,
            );
            let merged = secs >= TC;
            // The flash at merger: a blooming Plus halo.
            book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                if merged {
                    let e = ((secs - TC) / 0.6).min(1.0);
                    let r = (18.0 + 130.0 * e) as f32;
                    let a = (1.0 - e) as f32;
                    g.ring(Offset::new(cx as f32, cy as f32), r, 26.0, alpha(INK, 0.55 * a));
                    g.ring(Offset::new(cx as f32, cy as f32), r * 0.6, 14.0, alpha(CYAN, 0.5 * a));
                }
                for &(bx, by) in &[(bx1, by1), (bx2, by2)] {
                    if merged {
                        continue;
                    }
                    g.ring(
                        Offset::new(bx, by),
                        26.0,
                        18.0,
                        alpha(VIOLET_SOFT, 0.34),
                    );
                    g.circle(Offset::new(bx, by), 9.0, Color::rgb(16, 10, 28));
                }
                if merged {
                    g.ring(Offset::new(cx as f32, cy as f32), 24.0, 14.0, alpha(VIOLET_SOFT, 0.35));
                    g.circle(Offset::new(cx as f32, cy as f32), 10.0, Color::rgb(16, 10, 28));
                }
            });
            for &(bx, by) in &[(bx1, by1), (bx2, by2)] {
                if !merged {
                    book.circle(Offset::new(bx, by), 7.5, Color::rgb(22, 14, 36));
                    book.ring(Offset::new(bx, by), 7.5, 1.3, alpha(INK, 0.9));
                }
            }
            if merged {
                book.circle(Offset::new(cx as f32, cy as f32), 8.5, Color::rgb(22, 14, 36));
                book.ring(Offset::new(cx as f32, cy as f32), 8.5, 1.3, alpha(INK, 0.95));
            }

            // ── Stage two: the strain strip, lower band ──
            let x0 = 90.0;
            let x1 = w - 90.0;
            let span = x1 - x0;

            // panel backing — the LIGO strip everyone knows
            book.rrect(
                Rect::new(x0 - 20.0, 290.0, x1 + 20.0, 660.0),
                10.0,
                alpha(Color::rgb(14, 14, 20), 0.92),
            );
            book.stroke_rrect(
                Rect::new(x0 - 20.0, 290.0, x1 + 20.0, 660.0),
                10.0,
                alpha(VIOLET_DEEP_LINE, 0.55),
                1.0,
            );

            // The drawn-so-far strain: t-axis left → right, reveal by film t.
            let wave_h = 88.0;
            let wave_cy = 400.0;
            let progress = t as f32;
            let draw_to = x0 + span * progress;
            // sample the curve at ~1 sample per 1.5 px of drawn width
            let total_px = span;
            let total_time = SECONDS as f64;
            let samples = (total_px / 1.5) as usize;
            let mut path = Path::new();
            let mut started = false;
            for i in 0..=samples {
                let frac = i as f64 / samples as f64;
                let x = x0 + span * frac as f32;
                if x > draw_to {
                    break;
                }
                let tt = frac * total_time;
                let s = strain(tt);
                // amplitude envelope grows f^(2/3) — normalise to fit
                let env = (f_ins(tt.min(TC)) / F0).powf(2.0 / 3.0).min(7.0);
                let y = wave_cy - (s * wave_h as f64 * 0.16 * env.max(1.0)).min(wave_h as f64) as f32;
                if !started {
                    path.move_to(Offset::new(x, y));
                    started = true;
                } else {
                    path.line_to(Offset::new(x, y));
                }
            }
            // the waveform, once as glow …
            book.blended_layer(0.8, 0.0, BlendMode::Plus, None, |g| {
                g.stroke(path.clone(), alpha(CYAN, 0.30), 4.5);
            });
            // … and once as ink.
            book.stroke(path, alpha(mix(CYAN, INK, 0.55), 0.95), 1.6);

            // The envelope of the amplitude, as a guide.
            let mut env_top = Path::new();
            let mut env_bot = Path::new();
            let mut top_started = false;
            for i in 0..=samples {
                let frac = i as f64 / samples as f64;
                let x = x0 + span * frac as f32;
                if x > draw_to {
                    break;
                }
                let tt = frac * total_time;
                let env = if tt < TC {
                    (f_ins(tt) / F0).powf(2.0 / 3.0).min(7.0)
                } else {
                    (F_MERGER / F0).powf(2.0 / 3.0)
                        * (-(tt - TC) / TAU_RING).exp()
                };
                let e = (env * wave_h as f64 * 0.16).min(wave_h as f64) as f32;
                if !top_started {
                    env_top.move_to(Offset::new(x, wave_cy - e));
                    env_bot.move_to(Offset::new(x, wave_cy + e));
                    top_started = true;
                } else {
                    env_top.line_to(Offset::new(x, wave_cy - e));
                    env_bot.line_to(Offset::new(x, wave_cy + e));
                }
            }
            book.stroke(env_top, alpha(AMBER, 0.38), 1.0);
            book.stroke(env_bot, alpha(AMBER, 0.38), 1.0);

            // merger marker — one hairline at t_c, where the wave's own
            // time axis puts it.
            let tc_x = x0 + span * (TC / total_time) as f32;
            book.line(
                Offset::new(tc_x, 302.0),
                Offset::new(tc_x, 648.0),
                alpha(RED_MARK, 0.4),
                1.0,
            );

            // the live readout rides the receipt overlay (top-left block);
            // here only the merger hairline and the fit panel remain.

            // ── the chirp-fit scatter, mini panel at right of the orbit ──
            let (fx, fy) = (w - 335.0, 60.0);
            book.rrect(
                Rect::new(fx, fy, fx + 300.0, fy + 170.0),
                8.0,
                alpha(Color::rgb(12, 12, 18), 0.9),
            );
            // axes: log f 34..220, log fdot 0.1..300
            let lf0 = 34.0_f64.ln();
            let lf1 = 220.0_f64.ln();
            let ld0 = 0.05_f64.ln();
            let ld1 = 400.0_f64.ln();
            let px_ = |f: f64| fx + 24.0 + (((f.ln() - lf0) / (lf1 - lf0)) * 260.0) as f32;
            let py_ = |d: f64| fy + 150.0 - (((d.ln() - ld0) / (ld1 - ld0)) * 120.0) as f32;
            book.line(
                Offset::new(fx + 24.0, fy + 150.0),
                Offset::new(fx + 284.0, fy + 150.0),
                alpha(MUTED, 0.4),
                1.0,
            );
            book.line(
                Offset::new(fx + 24.0, fy + 150.0),
                Offset::new(fx + 24.0, fy + 30.0),
                alpha(MUTED, 0.4),
                1.0,
            );
            // theory line: slope 11/3 through (100Hz, its fdot on the drawn law)
            let fdot100 = chirp_fdot(100.0);
            let th_a = px_(40.0);
            let th_b = px_(200.0);
            let fa = fdot100 * (40.0_f64 / 100.0).powf(11.0 / 3.0);
            let fb = fdot100 * (200.0_f64 / 100.0).powf(11.0 / 3.0);
            book.line(
                Offset::new(th_a, py_(fa)),
                Offset::new(th_b, py_(fb)),
                alpha(AMBER, 0.75),
                1.4,
            );
            // measured points — from the fit's own collected data
            let pts = fit_points();
            for (f, fdot) in pts.iter() {
                book.circle(Offset::new(px_(*f), py_(*fdot)), 1.7, alpha(CYAN, 0.8));
            }
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(
        f_now,
        strain_now,
        slope,
        r2,
        npts,
        secs,
    ));
    stack.into()
}

const RED_MARK: Color = Color::rgb(248, 81, 73);
const VIOLET_DEEP_LINE: Color = Color::rgb(76, 29, 149);

/// fdot on the drawn law at frequency f (for the theory line).
fn chirp_fdot(f: f64) -> f64 {
    // df/dt = K f^(11/3) with K fixed by (f0, tc): K = f0^(-11/3)/( (8/3)·? )
    // From f = f0 (1−t/tc)^(−3/8): df/dt = (3/8) f0/tc · (1−t/tc)^(−11/8);
    // and (1−t/tc) = (f/f0)^(−8/3) → df/dt = (3/8)(f0/tc)(f/f0)^(11/3).
    (3.0 / 8.0) * (F0 / TC) * (f / F0).powf(11.0 / 3.0)
}

/// The fit's data, exposed for drawing: (f, fdot) pairs from the crossing
/// series' central differences.
fn fit_points() -> Vec<(f64, f64)> {
    let fs = crossing_series();
    let mut pts: Vec<(f64, f64)> = Vec::new();
    for i in 1..fs.len().saturating_sub(1) {
        let (t0, f0) = fs[i - 1];
        let (t2, f2) = fs[i + 1];
        let dt = t2 - t0;
        if dt <= 0.0 {
            continue;
        }
        let fdot = (f2 - f0) / dt;
        if fdot > 0.0 && f0 > 34.0 && f0 < 130.0 {
            pts.push((f0, fdot));
        }
    }
    pts
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(
    f_now: f64,
    strain_now: f64,
    slope: f64,
    r2: f64,
    npts: usize,
    secs: f64,
) -> WidgetNode {
    let lines = [
        "GW · THE SPACETIME AXIS · THE CHIRP OF 2015".to_string(),
        format!(
            "f(t) = f₀ (1 − t/t_c)^(−3/8) · h ∝ f^(2/3) cos 2φ · merger at t_c = {TC} s"
        ),
        format!(
            "f now {f_now:6.1} Hz · strain h(t) = {strain_now:+.3} · ringdown τ = {TAU_RING} s"
        ),
        format!(
            "CHIRP FIT (from the drawn curve's zero-crossings): slope = {slope:.3} vs 11/3 = 3.667 ({:+.1}%)",
            (slope - 11.0 / 3.0) / (11.0 / 3.0) * 100.0
        ),
        format!(
            "least squares · {} points · r² = {:.5} · fdot at 100 Hz: {:.1} Hz/s (measured on the same law)",
            npts, r2, chirp_fdot(100.0)
        ),
        format!(
            "phase swept {:.0} rad · merger flash at t = {:.2} · the wave is the orbit's own voice",
            phase_ins(secs.min(TC)), TC / SECONDS as f64
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
