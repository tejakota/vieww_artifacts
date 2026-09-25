//! exp_beams — *X-05: light as volume.* The U-01 + U-06 plate.
//!
//! Eleven volumetric shafts through **one** Plus-blended, blurred group —
//! the economic form of U-01: before `Sketchbook::blended_layer`, additive
//! light had to be lifted out of the painter and rebuilt as N widget groups
//! wrapped in `Opacity::new(1.0).blend(mode)`; now one call in the place
//! that draws. And one group, not eleven — the U-06 lesson: a blurred layer
//! is priced per layer, and eleven of them is the version you write first
//! because you are thinking about one shaft at a time. The receipt prints
//! the group count this frame actually used.
//!
//! Six thousand dust motes in a second Plus group, unblurred — each mote's
//! brightness read from its own position inside the beam cones: the field
//! is the light, and the light is the receipt.
//!
//! The beams sway, breathe, and land in pools on a reflective floor. The
//! source is a breathing core with a bloom. No effect in post; every pixel
//! of glow is the rasterizer compositing `Plus` groups — `replay` carrying
//! the mode `Canvas::push_layer` always accepted (U-01's one field).

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook,
    TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, mix, tint, BG_DEEP, CANVAS, FAINT, INK, MUTED, Rng, VIOLET, VIOLET_SOFT,
};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 10.0;

/// The shaft count — the receipt's first line.
const SHAFTS: usize = 11;

/// The dust mote count.
const MOTES: usize = 6000;

/// The source pivot, canvas space.
const PIVOT: Offset = Offset::new(470.0, 96.0);

/// The floor line.
const FLOOR: f32 = 648.0;

// ── A shaft's geometry ──────────────────────────────────────────────────────

/// One shaft: its fan angle at the pivot and the widths of its footprint.
#[derive(Clone)]
struct Shaft {
    /// Angle from straight-down, radians — the fan.
    angle: f32,
    /// Footprint half-width where the beam meets the floor.
    spread: f32,
    /// Base brightness multiplier — no two shafts identical.
    gain: f32,
    /// Sway phase — each shaft breathes on its own clock.
    phase: f32,
}

fn shafts() -> Vec<Shaft> {
    let mut rng = Rng::new(0x615);
    (0..SHAFTS)
        .map(|i| {
            // A fan from -38° to +34° — wider right, where the floor pools.
            let frac = i as f32 / (SHAFTS - 1) as f32;
            Shaft {
                angle: (-0.66 + 1.05 * frac) + rng.sym() * 0.035,
                spread: 26.0 + rng.f01() * 34.0 + 30.0 * (1.0 - (frac - 0.5).abs() * 2.0),
                gain: 0.55 + rng.f01() * 0.5,
                phase: rng.f01() * std::f32::consts::TAU,
            }
        })
        .collect()
}

/// Is a dust mote inside any beam cone — and how deep? Returns brightness
/// 0..1. The motes are the light meter: this is the field, evaluated.
fn beam_light(x: f32, y: f32, sway: f32, fan: &[Shaft]) -> f32 {
    // Vector from pivot to mote.
    let dx = x - PIVOT.dx;
    let dy = (y - PIVOT.dy).max(1.0);
    let m_angle = (dx / dy).atan();
    let m_dist = (dx * dx + dy * dy).sqrt();
    let mut best = 0.0f32;
    for s in fan {
        let a = s.angle + sway * 0.10;
        let d = (m_angle - a).abs();
        if d < 0.05 {
            // Inside the cone: brightness falls with angular distance from
            // the axis and with distance down the beam.
            let core = 1.0 - d / 0.05;
            let falloff = (1.0 - m_dist / 820.0).clamp(0.0, 1.0);
            best = best.max(core * core * falloff * s.gain);
        }
    }
    best
}

// ── The board ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let fan = shafts();

    let board = Painting::sized(
        CANVAS,
        PaintWith::new({
            let fan = fan.clone();
            move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — a stage, darker than the shafts will make it.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(7, 7, 10)),
                    (0.5, BG_DEEP),
                    (1.0, Color::rgb(9, 9, 13)),
                ]),
            );

            // Stars, sparse — the light will own this frame.
            let mut rng = Rng::new(0x616);
            for _ in 0..50 {
                let x = rng.f01() * w;
                let y = rng.f01() * h * 0.7;
                book.circle(Offset::new(x, y), 0.4 + rng.f01() * 0.7, alpha(Color::WHITE, 0.04));
            }

            // The beam sway and breath, this frame.
            let sway = (t * 0.9).sin();
            let breath = 0.82 + 0.18 * (t * 1.7).sin();
            let fade_in = clamp01(t / 0.12);

            // ── THE GROUP — one Plus-blended, blurred layer for all eleven
            //    shafts and their floor pools (U-01 economy, U-06 pricing).
            book.blended_layer(fade_in, 10.0, BlendMode::Plus, None, |g| {
                for s in &fan {
                    let a = s.angle + sway * 0.10;
                    let (sin, cos) = a.sin_cos();
                    // The shaft: pivot apex to floor footprint.
                    let len = (FLOOR - PIVOT.dy) / cos.max(0.2);
                    let foot_x = PIVOT.dx + sin * len;
                    // Apex width: 2-3 px — a shaft is a cone from a point.
                    let apex = 2.0;
                    let mut quad = Path::new();
                    quad.move_to(Offset::new(PIVOT.dx - apex, PIVOT.dy));
                    quad.line_to(Offset::new(PIVOT.dx + apex, PIVOT.dy));
                    quad.line_to(Offset::new(foot_x + s.spread, FLOOR));
                    quad.line_to(Offset::new(foot_x - s.spread, FLOOR));
                    quad.close();

                    let bright = 0.10 * s.gain * breath;
                    g.fill(
                        quad,
                        Gradient::vertical().with_dither().with_stops(&[
                            (0.0, alpha(mix(Color::WHITE, VIOLET_SOFT, 0.25), bright * 1.5)),
                            (0.35, alpha(VIOLET_SOFT, bright)),
                            (1.0, alpha(VIOLET, bright * 0.12)),
                        ]),
                    );

                    // The pool where the beam lands — a wide soft ellipse.
                    g.circle(
                        Offset::new(foot_x, FLOOR + 6.0),
                        s.spread * 1.8,
                        Gradient::radial_fill().with_dither().with_stops(&[
                            (0.0, alpha(mix(VIOLET_SOFT, Color::WHITE, 0.2), bright * 1.1)),
                            (1.0, alpha(VIOLET, 0.0)),
                        ]),
                    );
                }
            });

            // ── The dust — a second Plus group, crisp: the field made
            //    visible. Each mote reads its own cone depth.
            book.blended_layer(fade_in, 0.0, BlendMode::Plus, None, |g| {
                let mut rng = Rng::new(0x617);
                for _ in 0..MOTES {
                    let x0 = rng.f01() * w;
                    let depth = 0.25 + rng.f01() * 0.75;
                    let y0 = rng.f01() * h;
                    // Parallax drift: slow, down-right, depth-scaled.
                    let x = x0 + t * 34.0 * depth;
                    let y = y0 + t * 12.0 * depth;
                    // Wrap.
                    let x = x % w;
                    let lit = beam_light(x, y, sway, &fan);
                    if lit <= 0.015 {
                        continue;
                    }
                    let r = 0.5 + depth * 0.9;
                    let a = (lit * 0.85).min(1.0) * breath;
                    g.circle(
                        Offset::new(x, y),
                        r,
                        alpha(tint(VIOLET_SOFT, lit * 0.6), a),
                    );
                }
            });

            // ── The source — a bright core, its bloom, a breathing ring.
            book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                g.circle(PIVOT, 5.0 * breath, alpha(Color::WHITE, 0.95 * fade_in));
                g.circle(PIVOT, 12.0, alpha(tint(VIOLET_SOFT, 0.5), 0.6 * fade_in));
            });
            book.layer(1.0, 14.0, None, |g| {
                g.circle(
                    PIVOT,
                    46.0 + 8.0 * (t * 1.7).sin(),
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(tint(Color::WHITE, 0.2), 0.30 * fade_in * breath)),
                        (0.45, alpha(VIOLET_SOFT, 0.12 * fade_in)),
                        (1.0, alpha(VIOLET, 0.0)),
                    ]),
                );
            });
            book.ring(
                PIVOT,
                30.0 + 6.0 * (t * 1.1).sin(),
                1.0,
                alpha(VIOLET_SOFT, 0.25 * fade_in),
            );

            // The floor line — where the pools land.
            book.line(
                Offset::new(0.0, FLOOR),
                Offset::new(w, FLOOR),
                alpha(FAINT, 0.16 * fade_in),
                1.0,
            );

            // The vignette.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.42), 0.9).with_dither().with_stops(&[
                    (0.5, alpha(Color::BLACK, 0.0)),
                    (1.0, alpha(Color::BLACK, 0.40)),
                ]),
            );
        }}),
    );

    Stack::new()
        .push(Positioned::fill().child(board))
        .push(receipt_panel(t, &fan))
        .into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(t: f32, fan: &[Shaft]) -> WidgetNode {
    let sway = (t * 0.9).sin();
    let breath = 0.82 + 0.18 * (t * 1.7).sin();
    let fade_in = clamp01(t / 0.12);

    // Measured: how many motes are lit this frame — the field's census.
    let mut rng = Rng::new(0x617);
    let mut lit_count = 0usize;
    let mut total_light = 0.0f32;
    for _ in 0..MOTES {
        let x0 = rng.f01() * CANVAS.width;
        let depth = 0.25 + rng.f01() * 0.75;
        let y0 = rng.f01() * CANVAS.height;
        let x = (x0 + t * 34.0 * depth) % CANVAS.width;
        let y = y0 + t * 12.0 * depth;
        let lit = beam_light(x, y, sway, fan);
        if lit > 0.015 {
            lit_count += 1;
            total_light += lit;
        }
    }

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;
    const P_W: f32 = 340.0;

    let lines = [
        "X-05 · BEAMS · ONE GROUP, ONE PRICE (U-01/U-06)".to_string(),
        format!("shafts {} in ONE Plus-blended blurred group", SHAFTS),
        format!("dust {} · lit {} · mean light {:.2}", MOTES, lit_count,
            if lit_count > 0 { total_light / lit_count as f32 } else { 0.0 }),
        format!("breath {:.2} · sway {:+.2} rad", breath, sway * 0.10),
        format!("fade-in {:.0}%", fade_in * 100.0),
    ];

    let mut stack = Stack::new().push(
        Positioned::new()
            .left(P_X)
            .top(P_Y)
            .width(P_W)
            .height(18.0)
            .child(
                Text::new(lines[0].clone()).style(
                    TextStyle::new(12.0)
                        .monospace()
                        .letter_spacing(1.8)
                        .color(alpha(FAINT, 0.95)),
                ),
            ),
    );
    for (i, line) in lines.iter().enumerate().skip(1) {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + 18.0 + i as f32 * 16.0)
                .width(P_W)
                .height(15.0)
                .child(
                    Text::new(line.clone()).style(
                        TextStyle::new(11.0)
                            .monospace()
                            .color(alpha(MUTED, 0.9)),
                    ),
                ),
        );
    }

    // The instrument: the fan, drawn as the angular coverage map — every
    // shaft's angle as a ray from a mini pivot, brightness = gain.
    let fan_map = Painting::sized(
        Size::new(P_W, 88.0),
        PaintWith::new({
            let fan: Vec<Shaft> = fan.to_vec();
            move |book: &mut Sketchbook, _sz: Size| {
            book.rrect(
                Rect::new(0.0, 0.0, P_W, 88.0),
                10.0,
                alpha(Color::rgb(16, 16, 21), 0.88),
            );
            book.stroke_rrect(
                Rect::new(0.0, 0.0, P_W, 88.0),
                10.0,
                alpha(Color::WHITE, 0.08),
                1.0,
            );
            // Mini pivot, upper center; each shaft a ray to the panel floor.
            let pv = Offset::new(P_W * 0.5, 16.0);
            let fl = 62.0f32;
            for s in fan.iter() {
                let a = s.angle + sway * 0.10;
                let (sin, cos) = a.sin_cos();
                let tip = Offset::new(pv.dx + sin * fl, pv.dy + cos * fl);
                book.stroke(
                    {
                        let mut p = Path::new();
                        p.move_to(pv);
                        p.line_to(tip);
                        p
                    },
                    alpha(tint(VIOLET_SOFT, s.gain * 0.5), 0.35 + 0.55 * s.gain * breath),
                    1.0 + 1.6 * s.gain,
                );
            }
            // The current sway indicator — where the fan points now.
            book.circle(pv, 3.0, alpha(Color::WHITE, 0.9));
        }}),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 116.0)
            .width(P_W)
            .height(88.0)
            .child(fan_map),
    );

    stack.into()
}
