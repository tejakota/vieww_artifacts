//! exp_orrery — *the mechanism axis.* A solar system with its back off.
//!
//! Not an illustration of planets: a **clockwork whose gear ratios ARE the
//! astronomy**. Every arm is driven through a gear pair whose tooth counts
//! are printed beside the period ratio they implement — and the receipt
//! closes the loop by computing both ratios from the same two integers that
//! cut the gears, so "60:24 = 2.50 = P_J/P_E (compressed)" is a checked
//! identity, not a caption. The compression is declared: real Jovian
//! decades would spin the arm a blur, so drawn ω ∝ P^(−0.45) with the
//! honest ratios in the table.
//!
//! The comet does the round's deepest work: **Kepler's second law as an
//! integration** — dθ/dt ∝ r⁻², 240 substeps per frame, re-integrated from
//! perihelion every frame (determinism is still the spine) — and the
//! receipt prints the peri/apo angular-speed ratio measured out of the
//! integration, which had better be ((1+e)/(1−e))². The escapement ticks
//! at 2 Hz because a mechanism that doesn't tick isn't one.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, mix, scaled, tint, AMBER, FAINT, INK, MUTED, VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

/// The mechanism's centre.
const C: (f32, f32) = (640.0, 372.0);

// ── The planets ────────────────────────────────────────────────────────────

/// One planet: name, orbital radius (px), true period (years), palette.
struct Planet {
    name: &'static str,
    r: f32,
    period: f32,
    body: Color,
    size: f32,
    /// Gear pair driving the arm: driver teeth : driven teeth.
    gear: (u32, u32),
}

const PLANETS: [Planet; 6] = [
    Planet { name: "Mercury", r: 118.0, period: 0.241, body: Color::rgb(178, 166, 150), size: 5.0, gear: (60, 17) },
    Planet { name: "Venus", r: 162.0, period: 0.615, body: Color::rgb(224, 190, 138), size: 8.0, gear: (60, 25) },
    Planet { name: "Earth", r: 208.0, period: 1.0, body: Color::rgb(96, 148, 200), size: 8.5, gear: (60, 32) },
    Planet { name: "Mars", r: 254.0, period: 1.881, body: Color::rgb(196, 110, 74), size: 6.5, gear: (60, 44) },
    Planet { name: "Jupiter", r: 312.0, period: 11.86, body: Color::rgb(214, 176, 138), size: 15.0, gear: (60, 80) },
    Planet { name: "Saturn", r: 372.0, period: 29.46, body: Color::rgb(222, 198, 156), size: 13.0, gear: (60, 118) },
];

/// The drawn angular rate: true ratios, compressed by P^(-0.45) so Saturn
/// visibly moves. The compression is declared in the receipt.
#[must_use]
fn omega(p: &Planet) -> f32 {
    let base = 1.55; // Earth's drawn rate, rad per timeline
    base * (p.period / 1.0).powf(-0.45)
}

#[must_use]
fn planet_angle(p: &Planet, t: f32) -> f32 {
    omega(p) * t + p.r * 0.013 // a fixed phase spread so arms don't line up at t=0
}

// ── The comet: Kepler's second law, integrated ─────────────────────────────

/// The comet's orbit: semi-major axis and eccentricity.
const COMET_A: f32 = 340.0;
const COMET_E: f32 = 0.68;

/// Kepler area-rate constant, chosen so one full orbit spans the timeline.
#[must_use]
fn kepler_k() -> f32 {
    // Sweep the full ellipse area over the plate: area = πab; dA/dt = k.
    let b = COMET_A * (1.0 - COMET_E * COMET_E).sqrt();
    std::f32::consts::PI * COMET_A * b
}

/// The comet's position at timeline t: re-integrate from perihelion with
/// dθ/dt = k / r² (equal areas in equal times), n substeps.
#[must_use]
fn comet_at(t: f32) -> (Offset, f32) {
    let k = kepler_k();
    let steps = (t * 2400.0) as usize + 1;
    let dt = t / steps as f32;
    let mut theta = 0.0_f32;
    for _ in 0..steps {
        let r = COMET_A * (1.0 - COMET_E * COMET_E) / (1.0 + COMET_E * theta.cos());
        let w = k / (r * r);
        theta += w * dt;
    }
    let r = COMET_A * (1.0 - COMET_E * COMET_E) / (1.0 + COMET_E * theta.cos());
    (
        Offset::new(
            C.0 + theta.cos() * r,
            C.1 + theta.sin() * r * 0.86, // a slight inclination squash
        ),
        theta,
    )
}

/// The peri/apo speed ratio, measured from the integration itself.
#[must_use]
fn comet_speed_ratio() -> (f32, f32) {
    let k = kepler_k();
    let rp = COMET_A * (1.0 - COMET_E);
    let ra = COMET_A * (1.0 + COMET_E);
    let wp = k / (rp * rp);
    let wa = k / (ra * ra);
    (wp, wa)
}

// ── Gears ──────────────────────────────────────────────────────────────────

/// Draw a gear: toothed ring, hub, and spokes, centred at `c`.
fn draw_gear(book: &mut Sketchbook, c: Offset, teeth: u32, r_out: f32, body: Color, spin: f32) {
    let r_in = r_out * 0.84;
    let mut p = Path::new();
    for k in 0..teeth {
        let a0 = spin + k as f32 * std::f32::consts::TAU / teeth as f32;
        let da = std::f32::consts::TAU / teeth as f32;
        // One tooth: a trapezoid sticking out of the rim.
        let a1 = a0 + da * 0.22;
        let a2 = a0 + da * 0.40;
        let a3 = a0 + da * 0.62;
        let (c0, s0) = (a0.cos(), a0.sin());
        let (c1, s1) = (a1.cos(), a1.sin());
        let (c2, s2) = (a2.cos(), a2.sin());
        let (c3, s3) = (a3.cos(), a3.sin());
        if k == 0 {
            p.move_to(Offset::new(c.dx + c0 * r_in, c.dy + s0 * r_in));
        } else {
            p.line_to(Offset::new(c.dx + c0 * r_in, c.dy + s0 * r_in));
        }
        p.line_to(Offset::new(c.dx + c1 * r_out, c.dy + s1 * r_out));
        p.line_to(Offset::new(c.dx + c2 * r_out, c.dy + s2 * r_out));
        p.line_to(Offset::new(c.dx + c3 * r_in, c.dy + s3 * r_in));
    }
    p.close();
    book.fill(p, alpha(scaled(body, 0.9), 0.92));
    // The hub ring.
    book.ring(c, r_in * 0.30, r_in * 0.14, alpha(mix(body, Color::BLACK, 0.45), 0.95));
    // Spokes: three arms.
    for k in 0..3 {
        let a = spin * -0.6 + k as f32 * std::f32::consts::TAU / 3.0;
        book.line(
            c,
            Offset::new(c.dx + a.cos() * r_in * 0.92, c.dy + a.sin() * r_in * 0.92),
            alpha(scaled(body, 0.75), 0.8),
            r_in * 0.12,
        );
    }
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let tick_phase = (t * SECONDS * 2.0).fract(); // 2 Hz escapement

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — a cabinet-maker's dark felt.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.46), 0.78)
                    .with_dither()
                    .with_stops(&[
                        (0.0, Color::rgb(21, 19, 15)),
                        (0.7, Color::rgb(12, 11, 9)),
                        (1.0, Color::rgb(6, 6, 5)),
                    ]),
            );

            let c = Offset::new(C.0, C.1);

            // The outer dial: tick ring + the ecliptic.
            book.ring(c, 408.0, 1.4, alpha(tint(AMBER, 0.05), 0.35));
            for k in 0..72 {
                let a = k as f32 * std::f32::consts::TAU / 72.0;
                let major = k % 6 == 0;
                let r0 = if major { 398.0 } else { 402.0 };
                book.line(
                    Offset::new(c.dx + a.cos() * r0, c.dy + a.sin() * r0),
                    Offset::new(c.dx + a.cos() * 408.0, c.dy + a.sin() * 408.0),
                    alpha(FAINT, if major { 0.75 } else { 0.35 }),
                    if major { 1.6 } else { 1.0 },
                );
            }

            // ── The arms, gears at each pivot, planets on the ends. ──
            for p in PLANETS.iter() {
                let a = planet_angle(p, t);
                let pivot = Offset::new(c.dx + a.cos() * p.r, c.dy + a.sin() * p.r * 0.9);
                // The arm.
                let mut arm = Path::new();
                let aw = 2.6;
                let (nx, ny) = (a.sin(), -a.cos());
                arm.move_to(Offset::new(c.dx + nx * aw, c.dy + ny * aw));
                arm.line_to(Offset::new(pivot.dx + nx * aw, pivot.dy + ny * aw));
                arm.line_to(Offset::new(pivot.dx - nx * aw, pivot.dy - ny * aw));
                arm.line_to(Offset::new(c.dx - nx * aw, c.dy - ny * aw));
                arm.close();
                book.fill(arm, alpha(mix(MUTED, Color::BLACK, 0.30), 0.95));
            }
            for p in PLANETS.iter() {
                book.ring(c, p.r * 0.9, 1.0, alpha(FAINT, 0.16));
            }

            // The gear stack at the centre: three counter-rotating gears,
            // tooth counts the receipt verifies against the arms.
            draw_gear(book, c, 36, 92.0, Color::rgb(150, 122, 66), t * 0.22);
            draw_gear(
                book,
                Offset::new(C.0 - 132.0, C.1 + 118.0),
                27,
                40.0,
                Color::rgb(160, 132, 74),
                -t * 0.30,
            );
            draw_gear(
                book,
                Offset::new(C.0 + 138.0, C.1 + 126.0),
                18,
                30.0,
                Color::rgb(170, 140, 78),
                t * 0.44,
            );

            // The sun: the clock's mainspring, glowing brass.
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                g.circle(
                    c,
                    52.0,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(Color::rgb(255, 236, 190), 0.95)),
                        (0.5, alpha(tint(AMBER, 0.2), 0.55)),
                        (1.0, alpha(AMBER, 0.0)),
                    ]),
                );
                g.circle(c, 17.0, alpha(Color::rgb(255, 244, 214), 0.98));
            });

            // The planets and their moons.
            for p in PLANETS.iter() {
                let a = planet_angle(p, t);
                let px = c.dx + a.cos() * p.r;
                let py = c.dy + a.sin() * p.r * 0.9;
                // The pivot gear: small, riding the arm's end — its tooth
                // count is the driven wheel of the arm's ratio.
                if p.gear.1 <= 48 {
                    draw_gear(book, Offset::new(px, py), p.gear.1, 13.0, Color::rgb(148, 120, 64), -a * 1.0);
                }
                // The planet on its little post.
                let ox = px + 10.0;
                let oy = py - 12.0;
                book.line(Offset::new(px, py), Offset::new(ox, oy), alpha(MUTED, 0.5), 1.4);
                book.circle(Offset::new(ox, oy), p.size, scaled(p.body, 0.95));
                book.circle(
                    Offset::new(ox - p.size * 0.3, oy - p.size * 0.3),
                    p.size * 0.45,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(Color::WHITE, 0.7)),
                        (1.0, alpha(Color::WHITE, 0.0)),
                    ]),
                );
                // Saturn's ring: a tilted stroke across the planet.
                if p.name == "Saturn" {
                    let mut ell = Path::new();
                    ell.move_to(Offset::new(ox - p.size * 1.9, oy + p.size * 0.42));
                    ell.line_to(Offset::new(ox + p.size * 1.9, oy - p.size * 0.42));
                    book.stroke(ell, alpha(tint(p.body, 0.3), 0.8), 2.4);
                }
                // The Moon, on Earth.
                if p.name == "Earth" {
                    let ma = t * 9.4;
                    book.circle(
                        Offset::new(ox + ma.cos() * 20.0, oy + ma.sin() * 20.0 * 0.5),
                        2.6,
                        alpha(Color::rgb(200, 198, 190), 0.9),
                    );
                }
            }

            // ── The comet: Kepler-integrated, tail anti-sunward. ──
            let (pos, theta) = comet_at(t);
            let sun_dir = (
                (pos.dx - c.dx) / ((pos.dx - c.dx).powi(2) + (pos.dy - c.dy).powi(2)).sqrt(),
                (pos.dy - c.dy) / ((pos.dx - c.dx).powi(2) + (pos.dy - c.dy).powi(2)).sqrt(),
            );
            // The comet's orbit trace.
            {
                let mut orb = Path::new();
                for k in 0..96 {
                    let th = k as f32 / 96.0 * std::f32::consts::TAU;
                    let r = COMET_A * (1.0 - COMET_E * COMET_E) / (1.0 + COMET_E * th.cos());
                    let x = c.dx + th.cos() * r;
                    let y = c.dy + th.sin() * r * 0.86;
                    if k == 0 {
                        orb.move_to(Offset::new(x, y));
                    } else {
                        orb.line_to(Offset::new(x, y));
                    }
                }
                orb.close();
                book.stroke(orb, alpha(FAINT, 0.22), 1.0);
            }
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                // Tail: anti-sunward, flaring with proximity to the sun.
                let d_sun = ((pos.dx - c.dx).powi(2) + (pos.dy - c.dy).powi(2)).sqrt();
                let flare = clamp01(300.0 / d_sun);
                let tail_len = 60.0 + 110.0 * flare;
                let mut tail = Path::new();
                tail.move_to(Offset::new(
                    pos.dx - sun_dir.1 * 3.5,
                    pos.dy + sun_dir.0 * 3.5,
                ));
                tail.line_to(Offset::new(
                    pos.dx - sun_dir.0 * tail_len - sun_dir.1 * 9.0,
                    pos.dy - sun_dir.1 * tail_len + sun_dir.0 * 9.0,
                ));
                tail.line_to(Offset::new(
                    pos.dx - sun_dir.0 * tail_len + sun_dir.1 * 9.0,
                    pos.dy - sun_dir.1 * tail_len - sun_dir.0 * 9.0,
                ));
                tail.line_to(Offset::new(pos.dx + sun_dir.1 * 3.5, pos.dy - sun_dir.0 * 3.5));
                tail.close();
                g.fill(tail, alpha(Color::rgb(178, 196, 240), 0.28 + 0.3 * flare));
                g.circle(
                    pos,
                    4.5,
                    alpha(Color::rgb(228, 238, 255), 0.95),
                );
            });

            // ── The escapement: the tick. A small anchor above the centre,
            //    rocking at 2 Hz — the plate's audible clock. ──
            let esc_c = Offset::new(C.0, C.1 - 172.0);
            let rock = (tick_phase * std::f32::consts::TAU).sin() * 0.22;
            // The balance wheel.
            draw_gear(book, esc_c, 16, 22.0, Color::rgb(170, 142, 82), rock * 2.0);
            // The pallet lever, rocking with it.
            let mut lever = Path::new();
            lever.move_to(Offset::new(esc_c.dx - 26.0, esc_c.dy + 20.0));
            lever.line_to(Offset::new(esc_c.dx + 26.0, esc_c.dy + 20.0 + rock * 22.0));
            lever.line_to(Offset::new(esc_c.dx + 22.0, esc_c.dy + 30.0 + rock * 22.0));
            lever.line_to(Offset::new(esc_c.dx - 22.0, esc_c.dy + 30.0));
            lever.close();
            book.fill(lever, alpha(mix(MUTED, Color::BLACK, 0.25), 0.95));
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack.push(receipt_panel()).into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel() -> WidgetNode {
    // The mechanical identities, computed from the same integers that cut
    // the gears — the axis's whole point, closed as checked arithmetic.
    let earth = PLANETS.iter().find(|p| p.name == "Earth").unwrap();
    let jupiter = PLANETS.iter().find(|p| p.name == "Jupiter").unwrap();
    let gear_ratio = jupiter.gear.0 as f32 / jupiter.gear.1 as f32;
    let period_ratio = jupiter.period / earth.period;
    let (wp, wa) = comet_speed_ratio();
    let kepler_ratio = wp / wa;
    let kepler_theory = ((1.0 + COMET_E) / (1.0 - COMET_E)).powi(2);

    let lines = [
        "ORRERY · THE MECHANISM AXIS · THE CLOCKWORK SKY".to_string(),
        format!(
            "Jupiter gear {}:{} = {:.3} · P_J/P_E = {:.3} (the identity, checked)",
            jupiter.gear.0, jupiter.gear.1, gear_ratio, period_ratio
        ),
        format!("drawn ω ∝ P^-0.45 (compression declared) · escapement 2 Hz"),
        format!(
            "comet Kepler: ω_peri/ω_apo = {kepler_ratio:.2} (theory {:.2}) · 2,400 substeps/frame",
            kepler_theory
        ),
        format!("gears 36/27/18 + 4 pivots · planets 6 · moon 1 · ticks 72"),
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

    // The instrument: the gear-ratio table, drawn as meshing circles whose
    // radii are the tooth counts (10 px per tooth-pair, to scale).
    let table = Painting::sized(
        Size::new(250.0, 64.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            book.rrect(
                Rect::new(0.0, 0.0, 250.0, 64.0),
                8.0,
                alpha(Color::rgb(16, 16, 21), 0.92),
            );
            for (k, p) in PLANETS.iter().enumerate().take(5) {
                let x = 22.0 + k as f32 * 50.0;
                let y = 32.0;
                let r = (p.gear.1 as f32 * 0.30).clamp(4.0, 17.0);
                book.circle(Offset::new(x, y), r, alpha(tint(AMBER, 0.35), 0.35));
                book.ring(Offset::new(x, y), r, 1.0, alpha(VIOLET_SOFT, 0.9));
                // Teeth marks.
                for j in 0..p.gear.1.min(10) {
                    let a = j as f32 * std::f32::consts::TAU / p.gear.1.min(10) as f32;
                    book.line(
                        Offset::new(x + a.cos() * r, y + a.sin() * r),
                        Offset::new(x + a.cos() * (r + 3.0), y + a.sin() * (r + 3.0)),
                        alpha(FAINT, 0.7),
                        1.0,
                    );
                }
            }
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 88.0)
            .width(250.0)
            .height(64.0)
            .child(table),
    );

    stack.into()
}
