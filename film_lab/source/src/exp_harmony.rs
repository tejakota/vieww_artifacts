//! exp_harmony — *the phase axis.* Thirty-two clocks, one wave.
//!
//! The pendulum wave: 32 pendulums whose cycle counts form an integer
//! ladder — 32, 33, … 63 oscillations over the same span — so they start in
//! phase, drift apart into serpentine braids, helices, rotating shells and
//! counter-rotating vortices, and then **rephase exactly**, because every
//! pendulum has completed an integer number of swings. Every angle is the
//! closed form `θᵢ(t) = θ₀·cos(2π·cᵢ·t)` — there is no integration to drift
//! — and the receipt prints the rephase residual at t = 1 (the largest angle
//! from zero, measured from the same array that drew the last frame) as the
//! axis's whole point: **coherence is exact when the kinetics are closed-form.**
//!
//! The lengths obey L ∝ T² (the pendulum law, visible as the fan of the
//! strings); the balls ride a hue ladder; two ghost sets ride the same law at
//! t−Δ and t−2Δ through one Plus-blended group — evaluated, not remembered.
//! Below the bar, the live phase strip: one dot per pendulum at its own
//! displacement, the wave's portrait drawn as it happens.

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, mix, scaled, tint, FAINT, INK, MUTED, VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

/// The pendulum count.
const N: usize = 32;

/// Cycle-count ladder: pendulum i completes BASE + i cycles.
const BASE: f32 = 32.0;

/// The launch amplitude, radians.
const THETA0: f32 = 0.52;

/// The bar the pendulums hang from.
const BAR_Y: f32 = 88.0;

/// Lengths: the slowest pendulum (i = 0) is this long.
const L_MAX: f32 = 500.0;

/// Pendulum i's closed-form angle at film-time t.
#[must_use]
fn theta(i: usize, t: f32) -> f32 {
    let cycles = BASE + i as f32;
    THETA0 * (std::f32::consts::TAU * cycles * t).cos()
}

/// Pendulum i's drawn string length (L ∝ T², scaled to the canvas).
#[must_use]
fn length(i: usize) -> f32 {
    let cycles = BASE + i as f32;
    L_MAX / (cycles / BASE).powi(2)
}

/// The pivot x of pendulum i.
#[must_use]
fn pivot_x(i: usize) -> f32 {
    190.0 + (i as f32 / (N - 1) as f32) * 900.0
}

/// The bob centre of pendulum i at film-time t.
#[must_use]
fn bob(i: usize, t: f32) -> Offset {
    let th = theta(i, t);
    let l = length(i);
    Offset::new(
        pivot_x(i) + l * th.sin(),
        BAR_Y + l * th.cos(),
    )
}

/// A violet→cyan→mint hue ladder for the balls.
#[must_use]
fn ball_color(i: usize) -> Color {
    let f = i as f32 / (N - 1) as f32;
    let hue = 0.72 - 0.42 * f; // 259° → 108°
    let v = 0.82 + 0.16 * (f * 6.2832).sin();
    hsv(hue, 0.55, v)
}

/// HSV → RGB (h in turns).
#[must_use]
fn hsv(h: f32, s: f32, v: f32) -> Color {
    let h = (h.fract() + 1.0).fract();
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match i as u32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Color::rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

pub fn frame(t: f32) -> WidgetNode {
    // The census, measured from the same array that draws the frame:
    // the rephase residual — the largest |θ| across all pendulums.
    let rephase = (0..N).map(|i| theta(i, 1.0).abs()).fold(0.0_f32, f32::max);
    // The wave's visible lobes: sign changes of the bob x-offsets.
    let mut lobes = 0usize;
    let mut prev = 0.0_f32;
    for i in 0..N {
        let d = bob(i, t).dx - pivot_x(i);
        if prev != 0.0 && (d > 0.0) != (prev > 0.0) {
            lobes += 1;
        }
        if d.abs() > 1e-6 {
            prev = d;
        }
    }

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — a concert-hall dark.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(8, 9, 13)),
                    (1.0, Color::rgb(13, 14, 19)),
                ]),
            );

            // The rig: end posts and the bar.
            book.rect(Rect::new(150.0, BAR_Y - 6.0, 166.0, BAR_Y + 6.0), mix(MUTED, Color::BLACK, 0.5));
            book.rect(Rect::new(1114.0, BAR_Y - 6.0, 1130.0, BAR_Y + 6.0), mix(MUTED, Color::BLACK, 0.5));
            book.rect(Rect::new(150.0, BAR_Y - 2.5, 1130.0, BAR_Y + 2.5), mix(MUTED, Color::BLACK, 0.35));
            book.stroke_rrect(
                Rect::new(150.0, BAR_Y - 2.5, 1130.0, BAR_Y + 2.5),
                2.0,
                alpha(tint(MUTED, 0.2), 0.5),
                1.0,
            );

            // The ghosts — the same law at t−Δ, through one Plus group:
            // evaluated, never remembered.
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                for (lag, dim) in [(0.022_f32, 0.22), (0.045, 0.10)] {
                    let tg = (t - lag).max(0.0);
                    for i in 0..N {
                        let b = bob(i, tg);
                        g.circle(
                            b,
                            4.4,
                            alpha(scaled(ball_color(i), 0.8), dim * (1.0 - lag * 8.0).max(0.2)),
                        );
                    }
                }
            });

            // The pendulums — strings then bobs, back to front.
            for i in (0..N).rev() {
                let pivot = Offset::new(pivot_x(i), BAR_Y);
                let b = bob(i, t);
                book.line(pivot, b, alpha(mix(FAINT, INK, 0.25), 0.5), 1.0);
            }
            for i in 0..N {
                let pivot = Offset::new(pivot_x(i), BAR_Y);
                let b = bob(i, t);
                // The pivot bead.
                book.circle(pivot, 2.4, alpha(INK, 0.55));
                // The bob: a flat sphere + an offset specular (gradient
                // geometry is local-box space, so the highlight is its own
                // small circle — the honest spelling).
                book.circle(b, 7.0, ball_color(i));
                book.circle(
                    Offset::new(b.dx - 2.2, b.dy - 2.4),
                    3.4,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(Color::WHITE, 0.85)),
                        (1.0, alpha(Color::WHITE, 0.0)),
                    ]),
                );
            }

            // ── The phase strip — the wave's live portrait. ──
            let strip_y = 668.0;
            book.rect(
                Rect::new(150.0, strip_y - 26.0, 1130.0, strip_y + 26.0),
                alpha(Color::rgb(15, 16, 22), 0.85),
            );
            book.stroke_rrect(
                Rect::new(150.0, strip_y - 26.0, 1130.0, strip_y + 26.0),
                8.0,
                alpha(Color::WHITE, 0.07),
                1.0,
            );
            book.line(
                Offset::new(150.0, strip_y),
                Offset::new(1130.0, strip_y),
                alpha(FAINT, 0.35),
                1.0,
            );
            for i in 0..N {
                let d = bob(i, t).dx - pivot_x(i);
                book.line(
                    Offset::new(pivot_x(i), strip_y),
                    Offset::new(pivot_x(i) + d, strip_y),
                    alpha(scaled(ball_color(i), 0.55), 0.6),
                    1.0,
                );
                book.circle(
                    Offset::new(pivot_x(i) + d, strip_y),
                    2.6,
                    alpha(ball_color(i), 0.95),
                );
            }
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack.push(receipt_panel(t, rephase, lobes)).into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(t: f32, rephase: f32, lobes: usize) -> WidgetNode {
    let lines = [
        "HARMONY · THE PHASE AXIS · 32 CLOCKS, ONE WAVE".to_string(),
        format!(
            "cycle ladder {}..{} (integer — rephases at t=1 by construction)",
            BASE as u32,
            BASE as u32 + N as u32 - 1
        ),
        format!("rephase residual at t=1: {rephase:.6} rad (closed form)"),
        format!(
            "lengths {:.0}→{:.0} px · L ∝ T² · θ₀ {THETA0:.2} rad",
            length(0),
            length(N - 1)
        ),
        format!("live: lobes in the braid {lobes} · ghosts 2×32, Plus group"),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(640.0)
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

    // The instrument: the phase spiral — every pendulum's (cos, sin) phase
    // plotted as one dot on a circle; in phase = all dots at one point; the
    // braid = the dots distributed around the spiral of cycle counts.
    let phase = Painting::sized(
        Size::new(120.0, 120.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            let c = Offset::new(60.0, 60.0);
            book.rrect(
                Rect::new(0.0, 0.0, 120.0, 120.0),
                10.0,
                alpha(Color::rgb(14, 14, 20), 0.9),
            );
            book.stroke_rrect(
                Rect::new(0.0, 0.0, 120.0, 120.0),
                10.0,
                alpha(Color::WHITE, 0.08),
                1.0,
            );
            book.ring(c, 44.0, 1.0, alpha(FAINT, 0.4));
            book.ring(c, 26.0, 1.0, alpha(FAINT, 0.25));
            book.line(Offset::new(16.0, 60.0), Offset::new(104.0, 60.0), alpha(FAINT, 0.3), 1.0);
            book.line(Offset::new(60.0, 16.0), Offset::new(60.0, 104.0), alpha(FAINT, 0.3), 1.0);
            for i in 0..N {
                let ph = std::f32::consts::TAU * (BASE + i as f32) * t;
                let r = 14.0 + 30.0 * (i as f32 / (N - 1) as f32);
                book.circle(
                    Offset::new(c.dx + ph.cos() * r, c.dy + ph.sin() * r),
                    2.0,
                    alpha(ball_color(i), 0.95),
                );
            }
            // The coherence pulse: how bunched the phases are, shown as a
            // centre glow (bright at t≈0 and t≈1, dim mid-plate).
            let bunch = (0..N)
                .map(|i| {
                    let ph = std::f32::consts::TAU * (BASE + i as f32) * t;
                    (ph.cos(), ph.sin())
                })
                .fold((0.0_f32, 0.0_f32), |acc, p| (acc.0 + p.0, acc.1 + p.1));
            let coh = ((bunch.0 / N as f32).powi(2) + (bunch.1 / N as f32).powi(2)).sqrt();
            book.circle(
                c,
                20.0,
                Gradient::radial_fill().with_dither().with_stops(&[
                    (0.0, alpha(VIOLET_SOFT, 0.55 * coh)),
                    (1.0, alpha(VIOLET_SOFT, 0.0)),
                ]),
            );
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 88.0)
            .width(120.0)
            .height(120.0)
            .child(phase),
    );

    stack.into()
}
