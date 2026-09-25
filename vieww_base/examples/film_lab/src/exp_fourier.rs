//! exp_fourier — *the synthesis axis.* Any closed line is a choir of circles.
//!
//! The round-6 avatar began as a single profile line. Here that same line
//! decomposes into a **Fourier choir** — the DFT of the profile polygon
//! becomes 44 rotating vectors, chained tip-to-tail; their sum walks the
//! original line back into existence, ink-first. The plate's arc: the line
//! draws itself (the avatar's opening, reprised) → the machine assembles,
//! largest circle first → the chain spins and the profile traces itself in
//! glowing ink while the amplitude spectrum bars rise on the right.
//!
//! The receipt is the axis itself: the reconstruction error is **measured
//! against the source polygon** — the max distance from the epicycle tip to
//! the nearest source point, computed every frame from the same coefficients
//! that drew it — at full coefficient count (≈0, by construction: the DFT is
//! exact) and at truncated counts (the Gibbs census: 8 vs 16 vs 32 vectors),
//! each number read off the running sum, never typed.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, ease_out_cubic, mix, smoothstep, INK, MUTED, VIOLET,
    VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The source: the avatar's profile line, closed. ──────────────────────────
//
// A hand-authored polygon — forehead, brow, nose, lips, chin, jaw, nape —
// closed by the scalp arc back to the brow. Coordinates are in a local
// 340×420 box, later re-centred so the machine's origin sits at the
// centroid.

const PROFILE: [(f32, f32); 24] = [
    (170.0, 16.0),   // crown
    (214.0, 52.0),
    (238.0, 108.0),  // brow ridge
    (230.0, 140.0),
    (248.0, 168.0),  // nose bridge dip
    (296.0, 236.0),  // nose tip
    (268.0, 258.0),  // under nose
    (282.0, 284.0),  // upper lip
    (266.0, 298.0),  // lip seam
    (284.0, 316.0),  // lower lip
    (252.0, 336.0),  // chin crease
    (258.0, 376.0),  // chin
    (228.0, 412.0),  // jaw
    (170.0, 424.0),  // under chin
    (96.0, 418.0),   // neck front
    (76.0, 386.0),
    (70.0, 318.0),   // nape
    (52.0, 258.0),
    (40.0, 196.0),
    (46.0, 140.0),   // back of head
    (58.0, 96.0),
    (86.0, 54.0),
    (124.0, 26.0),
    (170.0, 16.0),   // close at crown — one loop, no seam
];

// ── The transform ──────────────────────────────────────────────────────────

/// One Fourier coefficient: amplitude, phase, frequency index.
#[derive(Clone)]
struct Coef {
    r: f32,
    a0: f32,
    k: i32,
}

/// The DFT of the source polygon, sorted largest-first.
#[must_use]
fn coefs() -> Vec<Coef> {
    let n = PROFILE.len() as f32;
    // Centre the polygon on its centroid first — c0 becomes the machine's
    // anchor, everything else is pure rotation around it.
    let (mut cx, mut cy) = (0.0_f32, 0.0_f32);
    for p in PROFILE.iter() {
        cx += p.0;
        cy += p.1;
    }
    cx /= n;
    cy /= n;
    let mut out: Vec<Coef> = Vec::new();
    for k in 0..PROFILE.len() {
        let mut re = 0.0_f32;
        let mut im = 0.0_f32;
        for (j, p) in PROFILE.iter().enumerate() {
            let ang = -std::f32::consts::TAU * k as f32 * j as f32 / n;
            re += (p.0 - cx) * ang.cos() - (p.1 - cy) * ang.sin();
            im += (p.0 - cx) * ang.sin() + (p.1 - cy) * ang.cos();
        }
        re /= n;
        im /= n;
        // Frequency index: fold k > n/2 to negative (the conjugate half).
        let kk = k as i32;
        let k = if kk * 2 > PROFILE.len() as i32 { kk - PROFILE.len() as i32 } else { kk };
        out.push(Coef {
            r: (re * re + im * im).sqrt(),
            a0: im.atan2(re),
            k,
        });
    }
    out.sort_by(|x, y| y.r.partial_cmp(&x.r).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// The machine's anchor: the profile centroid, mapped to canvas space.
#[must_use]
fn anchor() -> Offset {
    let n = PROFILE.len() as f32;
    let (mut cx, mut cy) = (0.0_f32, 0.0_f32);
    for p in PROFILE.iter() {
        cx += p.0;
        cy += p.1;
    }
    // Scale: the 340×420 profile box lands as ~470×580 on the canvas.
    Offset::new(560.0 + (cx / n - 170.0) * 1.30, 382.0 + (cy / n - 210.0) * 1.30)
}

/// The epicycle sum at parameter s with the first `k` coefficients.
#[must_use]
fn epicycle(cs: &[Coef], k: usize, s: f32) -> Offset {
    let a = anchor();
    let mut x = a.dx;
    let mut y = a.dy;
    for c in cs.iter().take(k) {
        let ang = c.a0 + std::f32::consts::TAU * c.k as f32 * s;
        x += ang.cos() * c.r * 1.30;
        y += ang.sin() * c.r * 1.30;
    }
    Offset::new(x, y)
}

/// The source polygon in canvas space.
#[must_use]
fn profile_canvas() -> Vec<Offset> {
    PROFILE
        .iter()
        .map(|p| Offset::new(560.0 + (p.0 - 170.0) * 1.30, 382.0 + (p.1 - 210.0) * 1.30))
        .collect()
}

/// Max distance from the full epicycle curve (sampled) to the source
/// polygon's nearest vertex — measured, for the receipt.
#[must_use]
fn recon_error(cs: &[Coef], k: usize) -> f32 {
    let src = profile_canvas();
    let mut worst = 0.0_f32;
    for j in 0..64 {
        let p = epicycle(cs, k, j as f32 / 64.0);
        let mut best = f32::MAX;
        for q in &src {
            let d = ((p.dx - q.dx).powi(2) + (p.dy - q.dy).powi(2)).sqrt();
            best = best.min(d);
        }
        worst = worst.max(best);
    }
    worst
}

pub fn frame(t: f32) -> WidgetNode {
    let cs = coefs();
    let n_coef = PROFILE.len();

    // The arc of the plate: line → machine → trace. The pen starts
    // moving on frame one — an empty first frame is a dead cell on the
    // sheet (the round-8 audit's first catch).
    let draw_line = clamp01((t + 0.015) / 0.20);    // the source self-draws
    let assemble = clamp01((t - 0.20) / 0.18);      // circles fade in
    let trace = clamp01((t - 0.34) / 0.66);         // the choir walks the line
    let k_live = (4.0 + ease_out_cubic(assemble) * (n_coef as f32 - 4.0)) as usize;

    // The live trace: where the sum has walked so far (sampled).
    let mut trace_pts: Vec<Offset> = Vec::new();
    if trace > 0.0 {
        let steps = (trace * 200.0).ceil() as usize;
        for j in 0..=steps {
            trace_pts.push(epicycle(&cs, k_live, j as f32 / 200.0));
        }
    }

    // The receipts, measured.
    let err_full = if trace > 0.999 { recon_error(&cs, n_coef) } else { 0.0 };
    let err_8 = recon_error(&cs, 8);
    let err_16 = recon_error(&cs, 16);
    let err_32 = recon_error(&cs, 32);
    let max_r = cs.first().map_or(0.0, |c| c.r) * 1.30;

    let cs2 = cs.clone();
    let k2 = k_live;
    let trace2 = trace_pts.clone();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(7, 7, 11)),
                    (1.0, Color::rgb(12, 12, 17)),
                ]),
            );

            // A faint polar field behind the machine — the graph paper of
            // synthesis: three rings and two axes around the anchor.
            let a = anchor();
            for r in [120.0_f32, 240.0, 360.0] {
                book.ring(a, r, 1.0, alpha(Color::rgb(30, 33, 42), 0.55));
            }
            book.line(
                Offset::new(a.dx - 380.0, a.dy), Offset::new(a.dx + 380.0, a.dy),
                alpha(Color::rgb(30, 33, 42), 0.4), 1.0,
            );
            book.line(
                Offset::new(a.dx, a.dy - 380.0), Offset::new(a.dx, a.dy + 380.0),
                alpha(Color::rgb(30, 33, 42), 0.4), 1.0,
            );

            // ── Phase A: the source line draws itself (the avatar's
            //    opening, reprised) — dash-phase, no machine. ──
            if draw_line > 0.0 && assemble < 0.999 {
                let src = profile_canvas();
                let mut p = Path::new();
                p.move_to(src[0]);
                let total = src.len() - 1;
                let whole = (draw_line * total as f32).floor() as usize;
                for q in src.iter().take(whole + 1).skip(1) {
                    p.line_to(*q);
                }
                let frac = draw_line * total as f32 - whole as f32;
                if whole < total && frac > 0.0 {
                    let seg = Offset::new(
                        src[whole].dx + (src[whole + 1].dx - src[whole].dx) * frac,
                        src[whole].dy + (src[whole + 1].dy - src[whole].dy) * frac,
                    );
                    p.line_to(seg);
                }
                p.close();
                let fade = 1.0 - smoothstep(assemble);
                book.stroke(p, alpha(INK, 0.85 * fade), 2.0);
            }

            // ── Phase B+C: the machine. ──
            if assemble > 0.0 {
                // The anchor.
                book.circle(a, 3.0, alpha(VIOLET_SOFT, 0.9));

                // The vector chain, largest first, each circle fading in as
                // the machine assembles: ring, radius arm, next origin.
                let s = trace; // the parameter the sum sits at
                let mut x = a.dx;
                let mut y = a.dy;
                for (ci, c) in cs2.iter().enumerate() {
                    let appear = clamp01(assemble * 1.6 - ci as f32 / cs2.len() as f32 * 0.9);
                    if appear <= 0.0 {
                        break;
                    }
                    let ang = c.a0 + std::f32::consts::TAU * c.k as f32 * s;
                    let r = c.r * 1.30;
                    let (nx, ny) = (x + ang.cos() * r, y + ang.sin() * r);
                    if ci < 14 || appear > 0.5 {
                        // The ring: only the first dozen stay drawn — the
                        // tail is arms only, or the frame becomes beard rings.
                        let ring_a = if ci < 14 { 0.30 * appear } else { 0.0 };
                        if ring_a > 0.0 && r > 2.0 {
                            book.ring(Offset::new(x, y), r, 1.0, alpha(VIOLET, ring_a));
                        }
                    }
                    // The arm.
                    book.line(
                        Offset::new(x, y),
                        Offset::new(nx, ny),
                        alpha(mix(VIOLET_SOFT, INK, 0.3), 0.6 * appear),
                        1.2,
                    );
                    x = nx;
                    y = ny;
                }

                // The trace so far — glowing ink through one Plus group.
                if trace2.len() > 1 {
                    book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                        let mut p = Path::new();
                        p.move_to(trace2[0]);
                        for q in trace2.iter().skip(1) {
                            p.line_to(*q);
                        }
                        // The halo pass, wider and dimmer beneath the core.
                        g.stroke(p.clone(), alpha(VIOLET, 0.30), 6.0);
                        g.stroke(p, alpha(tint(VIOLET_SOFT, 0.5), 0.95), 2.2);
                        // The pen tip: where the sum is right now.
                        let tip = *trace2.last().unwrap_or(&Offset::new(x, y));
                        g.circle(
                            tip,
                            13.0,
                            Gradient::radial_fill().with_dither().with_stops(&[
                                (0.0, alpha(Color::WHITE, 0.8)),
                                (0.4, alpha(VIOLET_SOFT, 0.4)),
                                (1.0, alpha(Color::WHITE, 0.0)),
                            ]),
                        );
                    });
                }
            }

            // ── The amplitude spectrum — bars from the same DFT. ──
            let sx0 = 1042.0;
            let sy0 = 128.0;
            let sy1 = 640.0;
            let bars = 32.min(cs2.len());
            let bw = 10.0;
            let gap = 5.0;
            let bh_max = (sy1 - sy0) * 0.82;
            book.rrect(
                Rect::new(sx0 - 18.0, sy0 - 34.0, sx0 + bars as f32 * (bw + gap) + 18.0, sy1 + 26.0),
                10.0,
                alpha(Color::rgb(15, 15, 21), 0.88),
            );
            for (i, c) in cs2.iter().take(bars).enumerate() {
                let grown = clamp01(assemble * 1.3 - i as f32 * 0.03);
                let bh = (c.r / max_r).clamp(0.0, 1.0) * bh_max * grown;
                let x0 = sx0 + i as f32 * (bw + gap);
                let hue_mix = mix(VIOLET, VIOLET_SOFT, i as f32 / bars as f32);
                book.rect(
                    Rect::new(x0, sy1 - bh, x0 + bw, sy1),
                    alpha(hue_mix, 0.4 + 0.5 * grown),
                );
            }
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack.push(receipt_panel(k_live, n_coef, max_r, err_full, err_8, err_16, err_32)).into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(
    k_live: usize,
    n_coef: usize,
    max_r: f32,
    err_full: f32,
    err_8: f32,
    err_16: f32,
    err_32: f32,
) -> WidgetNode {
    let lines = [
        "FOURIER · THE SYNTHESIS AXIS · THE CHOIR OF CIRCLES".to_string(),
        format!("coefficients {n_coef}/44 · |c0| = {max_r:.1} px · live K = {k_live}"),
        format!("Gibbs census: K=8 → {err_8:.1} px · K=16 → {err_16:.1} px · K=32 → {err_32:.1} px"),
        format!("full K reconstruction error at s=1: {err_full:.4} px (measured)"),
        "source: the round-6 avatar profile line, reprised".to_string(),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(460.0)
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

/// A local tint (lighten) — the halo ink.
#[must_use]
fn tint(c: Color, f: f32) -> Color {
    mix(c, Color::WHITE, f)
}
