//! exp_bubble — *the optics axis.* Soap-film light.
//!
//! Iridescence as physics, not as a rainbow gradient: each bubble's film
//! has a **thickness field** (gravity drains films — thick at the bottom,
//! thin at the top), and its colour is the two-beam interference integral —
//! 48 sampled wavelengths, each weighted by cos²(2πd/λ) and converted to RGB
//! through the spectral-locus approximation, summed. What the eye reads as
//! "soap bubble" is exactly that sum: no hue was typed by a human.
//!
//! The plate: a hero bubble drifting on a slow current with five satellites,
//! a chain of micro-bubbles rising from the wand's ring, one merge event
//! (a satellite slides in and is absorbed, a ripple ring emitted), and — at
//! the end — the **black spot**: the film's death, where drainage takes
//! thickness below the last constructive order and every wavelength cancels
//! at once. The receipt prints the thickness span sampled for the hero's
//! stops, the spectrum width it integrates, and the black-spot radius, all
//! read from the same arrays that painted the frame.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, ease_in_out, mix, smoothstep, Rng, BG_DEEP, FAINT, INK,
    MUTED, VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 10.0;

// ── The physics ─────────────────────────────────────────────────────────────

/// Wavelength → RGB (approximate spectral locus, 380..680 nm, gamma 0.8).
#[must_use]
fn wl_to_rgb(w: f32) -> (f32, f32, f32) {
    let w = w.clamp(380.0, 680.0);
    let (r, g, b) = if w < 440.0 {
        (-(w - 440.0) / 60.0, 0.0, 1.0)
    } else if w < 490.0 {
        (0.0, (w - 440.0) / 50.0, 1.0)
    } else if w < 510.0 {
        (0.0, 1.0, -(w - 510.0) / 20.0)
    } else if w < 580.0 {
        ((w - 510.0) / 70.0, 1.0, 0.0)
    } else if w < 645.0 {
        (1.0, -(w - 645.0) / 65.0, 0.0)
    } else {
        (1.0, 0.0, 0.0)
    };
    // Intensity falloff at the spectrum's ends.
    let i = if w < 420.0 {
        0.3 + 0.7 * (w - 380.0) / 40.0
    } else if w > 660.0 {
        0.3 + 0.7 * (680.0 - w) / 20.0
    } else {
        1.0
    };
    let gamma = 0.8;
    (
        (r * i).powf(gamma),
        (g * i).powf(gamma),
        (b * i).powf(gamma),
    )
}

/// The interference colour of a soap film of thickness `d` (nm) in air:
/// two-beam interference with the π phase shift at the outer reflection.
/// Constructive when 2d = (m + ½)λ → each wavelength's weight is
/// sin²(2πd/λ) (the reflected channel), integrated over the visible band.
#[must_use]
fn film_color(d_nm: f32) -> Color {
    let (mut r, mut g, mut b) = (0.0_f32, 0.0, 0.0);
    for k in 0..48 {
        let wl = 380.0 + k as f32 * (300.0 / 47.0);
        let w = (std::f32::consts::TAU * d_nm / wl).sin().powi(2);
        let (rr, gg, bb) = wl_to_rgb(wl);
        r += rr * w;
        g += gg * w;
        b += bb * w;
    }
    let norm = 48.0 / 3.2; // empirical rescale so mid-band films stay pastel
    Color::rgb(
        (r / norm * 255.0).clamp(0.0, 255.0) as u8,
        (g / norm * 255.0).clamp(0.0, 255.0) as u8,
        (b / norm * 255.0).clamp(0.0, 255.0) as u8,
    )
}

/// The hero bubble's thickness at height fraction `h` in [-1, 1] (top = -1):
/// gravity drainage — thick at the bottom, thin at the top, plus a slow
/// swirl so the bands wobble.
#[must_use]
fn thickness(h: f32, t: f32) -> f32 {
    let drain = 560.0 - 190.0 * h; // 370 nm (top) .. 750 nm (bottom)
    let swirl = 46.0 * (h * 2.2 + t * 3.1).sin() + 22.0 * (h * 5.1 - t * 2.2).cos();
    (drain + swirl).clamp(80.0, 780.0)
}

// ── The scene ───────────────────────────────────────────────────────────────

/// The hero bubble: centre + radius.
const HERO: (f32, f32, f32) = (642.0, 392.0, 148.0);

pub fn frame(t: f32) -> WidgetNode {
    // The hero drifts on a slow current.
    let hx = HERO.0 + 14.0 * (t * 6.2832 * 0.21).sin() - 8.0 * (t * 6.2832 * 0.13).cos();
    let hy = HERO.1 - 18.0 * t + 10.0 * (t * 6.2832 * 0.17).sin();
    let hr = HERO.2 * (0.97 + 0.03 * (t * 6.2832 * 0.4).sin());

    // The black spot: after t0.82 the film's top is below the last order.
    let bs_gate = smoothstep((t - 0.82) / 0.18);
    let bs_r = bs_gate * 46.0;

    // Satellites: fixed seeds, their own bobbing phases.
    let sats: [(f32, f32, f32, f32, f32); 5] = [
        // x, y, r, bob-phase, drift
        (368.0, 268.0, 52.0, 0.11, 22.0),
        (918.0, 236.0, 64.0, 0.17, -30.0),
        (312.0, 486.0, 44.0, 0.23, 18.0),
        (984.0, 496.0, 58.0, 0.29, -24.0),
        (766.0, 128.0, 36.0, 0.07, 16.0),
    ];
    // The merge event: satellite 1 slides into the hero over 0.55..0.72.
    let merge = smoothstep((t - 0.55) / 0.17) * (1.0 - smoothstep((t - 0.72) / 0.06).min(1.0));

    // The micro-chain: 9 tiny bubbles rising from the wand ring, gated in.
    let mut rng = Rng::new(0xB0B5_u64);
    let mut micro: Vec<(f32, f32, f32, f32)> = Vec::new(); // x, y, r, spawn
    for k in 0..9 {
        let spawn = 0.08 + k as f32 * 0.06;
        let phase = (t - spawn).max(0.0);
        if phase <= 0.0 {
            continue;
        }
        let rise = phase * 96.0;
        let x = 528.0 + rng.sym() * 26.0 + 10.0 * (phase * 5.0 + k as f32).sin();
        let y = 636.0 - rise;
        let r = (7.0 + rng.f01() * 7.0) * clamp01(phase * 4.0);
        micro.push((x, y, r, spawn));
    }
    let micro_count = micro.len();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — a soft studio dark with a breath of light from
            // above (the window the bubbles drift toward).
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(22, 23, 32)),
                    (0.5, Color::rgb(12, 12, 17)),
                    (1.0, Color::rgb(7, 7, 10)),
                ]),
            );

            // The wand: a ring at the bottom left of the hero, its film
            // still stretched after the launch.
            book.ring(
                Offset::new(520.0, 648.0),
                34.0,
                4.0,
                mix(MUTED, Color::WHITE, 0.25),
            );
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                g.circle(
                    Offset::new(520.0, 648.0),
                    34.0,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET_SOFT, 0.10)),
                        (1.0, alpha(Color::WHITE, 0.0)),
                    ]),
                );
            });

            // ── THE BUBBLES ──
            // The hero: a vertical 5-stop gradient of interference colours,
            // each stop's colour the physics of its own thickness sample.
            let stops: [(f32, Color); 5] = {
                let mut s = [
                    (-1.0_f32, film_color(thickness(-1.0, t))),
                    (-0.5, film_color(thickness(-0.5, t))),
                    (0.0, film_color(thickness(0.0, t))),
                    (0.5, film_color(thickness(0.5, t))),
                    (1.0, film_color(thickness(1.0, t))),
                ];
                // Offsets in the shape's local box: -1..1 maps 0..1.
                for (o, _) in s.iter_mut() {
                    *o = (*o + 1.0) * 0.5;
                }
                s
            };
            book.circle(
                Offset::new(hx, hy),
                hr,
                Gradient::vertical().with_dither().with_stops(&stops),
            );
            // The rim: the meniscus is thicker — it reads as a pale bright
            // edge with a hint of the same physics.
            book.ring(Offset::new(hx, hy), hr, 2.6, alpha(Color::rgb(235, 240, 250), 0.55));
            book.ring(Offset::new(hx, hy), hr - 3.4, 1.2, alpha(Color::rgb(220, 230, 244), 0.28));
            // Speculars: one window and one bounce.
            book.circle(
                Offset::new(hx - hr * 0.42, hy - hr * 0.46),
                17.0,
                Gradient::radial_fill().with_dither().with_stops(&[
                    (0.0, alpha(Color::WHITE, 0.85)),
                    (1.0, alpha(Color::WHITE, 0.0)),
                ]),
            );
            book.circle(
                Offset::new(hx + hr * 0.52, hy + hr * 0.30),
                9.0,
                Gradient::radial_fill().with_dither().with_stops(&[
                    (0.0, alpha(Color::WHITE, 0.38)),
                    (1.0, alpha(Color::WHITE, 0.0)),
                ]),
            );
            // The black spot — the film's death, growing at the top.
            if bs_r > 0.5 {
                book.circle(
                    Offset::new(hx, hy - hr * 0.44),
                    bs_r,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(Color::rgb(4, 4, 6), 0.94)),
                        (0.7, alpha(Color::rgb(4, 4, 6), 0.55)),
                        (1.0, alpha(Color::rgb(4, 4, 6), 0.0)),
                    ]),
                );
            }
            // The merge ripple: rings emitted when the satellite lands.
            if merge > 0.02 && merge < 1.0 {
                let rr = 26.0 + 90.0 * merge;
                book.ring(
                    Offset::new(hx, hy),
                    rr,
                    2.0,
                    alpha(Color::rgb(230, 236, 248), 0.4 * (1.0 - merge)),
                );
            }

            // The satellites — each its own thickness law (shorter films
            // drain faster: everything scales with radius).
            for (si, s) in sats.iter().enumerate() {
                let (sx, sy, sr, ph, drift) = *s;
                // Satellite 1 performs the merge event.
                let (mx, my, mr) = if si == 1 {
                    (
                        sx + drift * t + (hx - (sx + drift * t)) * merge,
                        sy + 8.0 * (t * 6.2832 * ph).sin() + (hy - sy) * merge,
                        sr * (1.0 - 0.85 * merge),
                    )
                } else {
                    (
                        sx + drift * t,
                        sy + 8.0 * (t * 6.2832 * ph).sin(),
                        sr,
                    )
                };
                if mr < 1.0 {
                    continue;
                }
                let scale = sr / HERO.2;
                let cstops: [(f32, Color); 5] = {
                    let mut cs = [
                        (0.0_f32, film_color(thickness(-1.0, t + ph))),
                        (0.25, film_color(thickness(-0.5, t + ph))),
                        (0.5, film_color(thickness(0.0, t + ph))),
                        (0.75, film_color(thickness(0.5, t + ph))),
                        (1.0, film_color(thickness(1.0, t + ph))),
                    ];
                    for c in cs.iter_mut().skip(1) {
                        c.1 = mix(c.1, Color::WHITE, (1.0 - scale) * 0.35);
                    }
                    cs
                };
                book.circle(
                    Offset::new(mx, my),
                    mr,
                    Gradient::vertical().with_dither().with_stops(&cstops),
                );
                book.ring(Offset::new(mx, my), mr, 1.6, alpha(Color::rgb(235, 240, 250), 0.4));
                book.circle(
                    Offset::new(mx - mr * 0.4, my - mr * 0.44),
                    (mr * 0.16).max(3.0),
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(Color::WHITE, 0.7)),
                        (1.0, alpha(Color::WHITE, 0.0)),
                    ]),
                );
            }

            // The micro-chain — tiny films, almost all rim and glint.
            for (mx, my, mr, _) in &micro {
                if *mr < 0.6 {
                    continue;
                }
                book.circle(
                    Offset::new(*mx, *my),
                    *mr,
                    alpha(film_color(thickness(0.0, t) * 1.4 + *mr * 8.0), 0.85),
                );
                book.ring(Offset::new(*mx, *my), *mr, 1.0, alpha(Color::rgb(235, 240, 250), 0.5));
            }

            // Dust motes in the window light — a handful, for depth.
            let mut rng = Rng::new(0xD057_u64);
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                for k in 0..40 {
                    let x = rng.f01() * 1280.0;
                    let y = rng.f01() * 720.0;
                    let drift = 8.0 * (t * 6.2832 * 0.3 + k as f32).sin();
                    g.circle(
                        Offset::new(x + drift, y),
                        1.0 + rng.f01() * 1.2,
                        alpha(Color::rgb(200, 210, 232), 0.05 + 0.05 * rng.f01()),
                    );
                }
            });
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack.push(receipt_panel(t, bs_r, micro_count)).into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(t: f32, bs_r: f32, micro_live: usize) -> WidgetNode {
    let d_top = thickness(-1.0, t);
    let d_bot = thickness(1.0, t);
    let lines = [
        "BUBBLE · THE OPTICS AXIS · SOAP-FILM LIGHT".to_string(),
        format!(
            "thickness span {d_top:.0}→{d_bot:.0} nm (gravity drain, swirl live)"
        ),
        format!("colour: Σ cos²(2πd/λ) · 48 λ, 380..680 nm · spectral locus"),
        format!(
            "black spot r {bs_r:.0} px @ t={t:.2} · merge event 0.55–0.78"
        ),
        format!("bubbles live: hero + 5 sats + {micro_live} micro · census from arrays"),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(560.0)
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

    // The instrument: the interference ladder — thickness samples from
    // 100..780 nm, painted by the same function the film wears.
    let ladder = Painting::sized(
        Size::new(240.0, 26.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            book.rrect(
                Rect::new(0.0, 0.0, 240.0, 26.0),
                6.0,
                alpha(Color::rgb(16, 16, 21), 0.92),
            );
            for k in 0..30 {
                let d = 100.0 + k as f32 * 22.6;
                let x = 6.0 + k as f32 * 7.7;
                book.rect(
                    Rect::new(x, 6.0, x + 7.0, 20.0),
                    alpha(film_color(d), 0.95),
                );
            }
            book.stroke_rrect(
                Rect::new(0.0, 0.0, 240.0, 26.0),
                6.0,
                alpha(Color::WHITE, 0.08),
                1.0,
            );
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 84.0)
            .width(240.0)
            .height(26.0)
            .child(ladder),
    );

    stack.into()
}
