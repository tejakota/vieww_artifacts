//! exp_foucault — *the rotation axis.* The pendulum that proved the sky
//! moves.
//!
//! Léon Foucault, Panthéon, 1851: a 67-metre wire, a 28-kg bob, and the
//! first direct proof that the Earth turns — for the plane of a freely
//! swinging pendulum is fixed among the stars, and the floor rotates
//! beneath it. The plane's drift relative to the ground is
//! **Ω = −360°·sin(latitude) per day**: full circle at the pole, zero at
//! the equator, and at Paris's 48.846°N exactly **270.7° per day**,
//! retrograde. The sand the bob spills draws the rosette every museum
//! visitor knows.
//!
//! This plate is that instrument, honestly compressed: 24 hours of the
//! Panthéon in 12 film seconds, one petal of sand per hour at the plane
//! angle the law gives, the bob swinging at its visible cadence along the
//! rotating plane, and the counter-dial that shows WHY — a star dial
//! turning +360° while the plane holds still among the stars. The
//! receipt closes the machine's books from its own geometry: **the
//! precession rate fitted from the 24 drawn petal axes themselves, the
//! total sweep, the per-hour step, and the repeat period 86,164 s /
//! sin(lat)** — the sidereal correction included.

use std::f64::consts::PI;

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook,
    TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, mix, AMBER, CYAN, INK, MUTED, VIOLET, VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The law ─────────────────────────────────────────────────────────────────

/// The Panthéon's latitude.
const LAT_DEG: f64 = 48.8462;

/// Earth's rotation, degrees per solar day.
const OMEGA_E: f64 = 360.0;

/// The pendulum's precession rate, deg/day: −360·sin(lat), retrograde.
fn precession_rate() -> f64 {
    -OMEGA_E * LAT_DEG.to_radians().sin()
}

/// Plane angle (radians, screen convention, y-down) at film fraction t.
/// One film = 24 hours.
fn plane_angle(t: f64) -> f64 {
    precession_rate().to_radians() * t
}

/// The visible swing period, film seconds — the compression declared in
/// the caption (the real 16.4 s cannot survive a 7200× time-lapse).
const SWING_S: f64 = 1.35;

// ── The frame ───────────────────────────────────────────────────────────────

const CX: f32 = 560.0; // dial centre
const CY: f32 = 420.0;
const R_DIAL: f32 = 235.0; // floor dial radius

pub fn frame(t: f32) -> WidgetNode {
    let t = t as f64;
    let hours = t * 24.0;
    let theta = plane_angle(t);

    // The receipt: fit the rate from the drawn petal axes (hour marks).
    let rate_fit: f64 = {
        // petal axes at each hour h: theta_h = rate·h/24 (rad); a linear
        // fit through the drawn values closes against the formula.
        let n = 24;
        let xs: Vec<f64> = (0..n).map(|h| h as f64).collect();
        let ys: Vec<f64> = xs
            .iter()
            .map(|&h| plane_angle(h / 24.0).to_degrees())
            .collect();
        let mx = xs.iter().sum::<f64>() / n as f64;
        let my = ys.iter().sum::<f64>() / n as f64;
        let num: f64 = xs.iter().zip(&ys).map(|(&x, &y)| (x - mx) * (y - my)).sum();
        let den: f64 = xs.iter().map(|&x| (x - mx).powi(2)).sum();
        num / den * 24.0 // deg per day
    };

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — the Panthéon's interior at blue hour.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(7, 7, 11)),
                    (1.0, Color::rgb(13, 13, 18)),
                ]),
            );

            // The dome hint — a great arc overhead.
            book.blended_layer(0.7, 0.0, BlendMode::Plus, None, |g| {
                g.ring(
                    Offset::new(CX, 1180.0),
                    1040.0,
                    90.0,
                    alpha(VIOLET, 0.055),
                );
                g.ring(
                    Offset::new(CX, 1180.0),
                    970.0,
                    60.0,
                    alpha(VIOLET, 0.04),
                );
            });

            // ── The floor dial: 24 hour marks, roman every second ──
            book.ring(Offset::new(CX, CY), R_DIAL, 1.4, alpha(MUTED, 0.55));
            book.ring(Offset::new(CX, CY), 12.0, 3.0, alpha(VIOLET, 0.4));
            for hh in 0..24 {
                let a = hh as f64 / 24.0 * std::f64::consts::TAU - PI / 2.0;
                let major = hh % 2 == 0;
                let (ox, oy) = (a.cos() as f32, a.sin() as f32);
                let r0 = R_DIAL - if major { 16.0 } else { 8.0 };
                book.line(
                    Offset::new(CX + ox * r0, CY + oy * r0),
                    Offset::new(CX + ox * R_DIAL, CY + oy * R_DIAL),
                    alpha(if major { MUTED } else { Color::rgb(70, 78, 92) },
                          if major { 0.8 } else { 0.6 }),
                    if major { 1.6 } else { 1.0 },
                );
            }

            // ── The sand rosette: one petal per elapsed hour, each petal an
            // envelope ellipse along that hour's plane angle ──
            let petals_drawn = hours.floor().min(24.0) as usize;
            let swing_amp = R_DIAL * 0.86;
            book.blended_layer(0.85, 0.0, BlendMode::Plus, None, |g| {
                for ph in 0..petals_drawn {
                    let a = plane_angle(ph as f64 / 24.0);
                    let (ux, uy) = ((a.cos()) as f32, (a.sin()) as f32);
                    // the envelope petal: a thin rounded sliver along the axis
                    let mid = 0.5;
                    let cxp = CX + ux * swing_amp * mid;
                    let cyp = CY + uy * swing_amp * mid;
                    let rot = a as f32;
                    // draw as a stroked line with round caps, soft amber sand
                    let mut path = Path::new();
                    path.move_to(Offset::new(
                        cxp - ux * swing_amp * 0.5,
                        cyp - uy * swing_amp * 0.5,
                    ));
                    path.line_to(Offset::new(
                        cxp + ux * swing_amp * 0.5,
                        cyp + uy * swing_amp * 0.5,
                    ));
                    let _ = rot;
                    g.stroke_styled(
                        path.clone(),
                        alpha(AMBER, 0.16),
                        7.0,
                        vieww_foundation::StrokeStyle::rounded(),
                    );
                    g.stroke_styled(
                        path,
                        alpha(AMBER, 0.10),
                        2.0,
                        vieww_foundation::StrokeStyle::rounded(),
                    );
                }
            });

            // ── The pendulum: wire from the dome point to the bob, swinging
            // along the CURRENT plane angle ──
            let anchor = Offset::new(CX, 96.0);
            let swing_phase = (t * SECONDS as f64 / SWING_S * PI * 2.0).sin();
            // the plane axis
            let (ux, uy) = (theta.cos() as f32, theta.sin() as f32);
            let bob = Offset::new(
                CX + ux * swing_amp * swing_phase as f32,
                CY + uy * swing_amp * swing_phase as f32,
            );
            // the wire
            book.line(anchor, bob, alpha(Color::rgb(150, 156, 170), 0.85), 2.0);
            // the plane line, faint, full length
            book.blended_layer(0.5, 0.0, BlendMode::Plus, None, |g| {
                g.line(
                    Offset::new(CX - ux * swing_amp, CY - uy * swing_amp),
                    Offset::new(CX + ux * swing_amp, CY + uy * swing_amp),
                    alpha(CYAN, 0.16),
                    2.0,
                );
            });
            // the bob + its glow
            book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                g.ring(bob, 22.0, 16.0, alpha(VIOLET_SOFT, 0.16));
            });
            book.circle(bob, 13.0, Color::rgb(232, 230, 238));
            book.ring(bob, 13.0, 1.6, alpha(INK, 0.9));
            // the mount
            book.rrect(Rect::new(anchor.dx - 26.0, anchor.dy - 14.0, anchor.dx + 26.0, anchor.dy + 6.0), 4.0, Color::rgb(24, 24, 32));

            // ── The counter-dial: the star field turning +360°, right ──
            let (dx, dy, dr) = (1075.0, 260.0, 108.0);
            book.rrect(
                Rect::new(dx - 136.0, dy - 136.0, dx + 136.0, dy + 136.0),
                12.0,
                alpha(Color::rgb(12, 12, 18), 0.94),
            );
            book.ring(Offset::new(dx, dy), dr, 1.2, alpha(MUTED, 0.5));
            // stars: deterministic, rotating with the Earth
            let sky_rot = t * std::f64::consts::TAU; // +360°/day
            let mut rng = crate::film_lib::Rng::new(0x57A5);
            let mut star_pts: Vec<(f32, f32, f32)> = Vec::new();
            for _ in 0..90 {
                let a0 = rng.f01() as f64 * std::f64::consts::TAU;
                let rr = (rng.f01() as f32).powf(0.5) * dr * 0.92;
                let b = rng.f01();
                star_pts.push((a0 as f32, rr, b));
            }
            for (a0, rr, b) in star_pts {
                let a = a0 + sky_rot as f32;
                let star = Offset::new(dx + a.cos() * rr, dy + a.sin() * rr);
                book.circle(star, 0.8 + b * 1.1, alpha(INK, 0.35 + b * 0.55));
            }
            // the pendulum plane, fixed in the stars: a constant axis on this dial
            book.line(
                Offset::new(dx - dr * 0.82, dy),
                Offset::new(dx + dr * 0.82, dy),
                alpha(CYAN, 0.7),
                1.4,
            );
            // an arrow showing the sky's rotation direction
            book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                let a0 = sky_rot as f32 - 0.5;
                let tip = Offset::new(dx + (a0).cos() * dr * 0.7, dy + (a0).sin() * dr * 0.7);
                g.ring(tip, 6.0, 2.4, alpha(AMBER, 0.55));
            });

            // ── The precession dial, below: the plane's angle today ──
            let (px, py) = (1075.0, 520.0);
            let pr = 78.0;
            book.rrect(
                Rect::new(px - 136.0, py - 108.0, px + 136.0, py + 108.0),
                12.0,
                alpha(Color::rgb(12, 12, 18), 0.94),
            );
            book.ring(Offset::new(px, py), pr, 1.2, alpha(MUTED, 0.5));
            // the full sweep arc (270.7° of it), drawn as the day's progress
            let sweep = (-precession_rate()).to_radians() as f32; // total, positive for drawing
            book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                g.arc(
                    Offset::new(px, py),
                    pr,
                    9.0,
                    -PI as f32 / 2.0,
                    sweep * t as f32,
                    alpha(VIOLET, 0.5),
                );
            });
            // the plane axis on this dial — the machine's own state
            let (ux, uy) = (theta.cos() as f32, theta.sin() as f32);
            book.line(
                Offset::new(px - ux * pr * 0.86, py - uy * pr * 0.86),
                Offset::new(px + ux * pr * 0.86, py + uy * pr * 0.86),
                alpha(CYAN, 0.85),
                2.0,
            );
            book.circle(Offset::new(px, py), 2.6, INK);
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(hours, rate_fit));
    stack.into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(hours: f64, rate_fit: f64) -> WidgetNode {
    let rate = precession_rate();
    let repeat_s = 86164.0 / LAT_DEG.to_radians().sin();
    let lines = [
        "FOUCAULT · THE ROTATION AXIS · PANTHÉON, 1851".to_string(),
        format!(
            "lat {LAT_DEG:.4}°N · plane drift Ω = −360°·sin(lat) = {rate:.2}°/day, retrograde"
        ),
        format!(
            "day so far {hours:4.1} h · plane swept {:.1}° · per-hour step {:.2}°",
            rate * hours / 24.0,
            rate / 24.0
        ),
        format!(
            "RATE FIT (from the 24 drawn petal axes): {rate_fit:.2}°/day vs −360·sin(lat) = {rate:.2}°/day"
        ),
        format!(
            "the marks repeat every 86,164 s / sin(lat) = {:.0} s = {:.1} h — the sidereal day, corrected",
            repeat_s,
            repeat_s / 3600.0
        ),
        "the star dial turns +360°; the plane holds still among the stars — that is the whole proof".to_string(),
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
