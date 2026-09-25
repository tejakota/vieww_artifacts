//! exp_megapath — *the single-path axis.* One line, no others.
//!
//! The whole plate is ONE `Path` — a single continuous serpentine scan
//! whose local row pitch is modulated by a landscape brightness field, so the line crowds where the
//! image is dark (the mountains) and thins where it is bright (the sky, the
//! moon). ~37,000 `line_to` segments, stroked as **one verb** — the count
//! ceiling of a *single path*, the axis `galaxy` measured with many small
//! shapes and nobody had measured with one big one.
//!
//! The landscape: night sky, a moon whose rows flow around it like field
//! lines, two mountain ridges, water at the base. Nothing is filled — the
//! *local density of the one line* is the only shading the picture has. This is the author's genesis
//! line taken to its own extreme: not line → 3D this time, but line →
//! thirty-seven thousand segments, and still one line.
//!
//! The beats: the line draws itself from the outside in (dash phase, the
//! E-17 grammar on the biggest path it has ever ridden), then a bright
//! pulse laps the whole drawing — the same path stroked a second time, a
//! moving window of light. The receipt prints the segment census and the
//! arc length, measured off the same points that drew the frame.

use std::sync::OnceLock;

use vieww_foundation::{Color, Dash, Gradient, Offset, Path, Rect, Size, Sketchbook, StrokeStyle,
    TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{alpha, clamp01, ease_out_cubic, mix, Rng, BG_DEEP, FAINT, INK, MUTED,
    VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 11.0;

/// Pitch (gap between turns) at the darkest and brightest field values.
const PITCH_DENSE: f32 = 1.9;
const PITCH_SPARSE: f32 = 15.5;

/// Arc-length step per segment, in px.
const STEP: f32 = 3.0;

// ── The landscape brightness field — b(x, y) in [0, 1] ──────────────────────
//
// Bright = sparse line, dark = dense. Built once; the serpentine
// integrator samples it once per emitted point.

/// Deterministic 1-D value noise for the ridgelines.
#[must_use]
fn vnoise(seed: u64, x: f32) -> f32 {
    let i = x.floor();
    let f = x - i;
    let at = |k: f32| -> f32 {
        let mut rng = Rng::new(seed ^ (k as u64).wrapping_mul(0x9E3779B97F4A7C15));
        rng.f01()
    };
    let u = f * f * (3.0 - 2.0 * f);
    at(i) * (1.0 - u) + at(i + 1.0) * u
}

/// The moon: centre + radius.
const MOON: (f32, f32, f32) = (1018.0, 152.0, 74.0);

/// The horizon line.
const HORIZON: f32 = 0.66;

#[must_use]
fn brightness(x: f32, y: f32, w: f32, h: f32) -> f32 {
    let yn = y / h;
    // Sky: bright, brighter toward the top.
    let mut b = 0.92 - 0.35 * (yn / HORIZON).clamp(0.0, 1.0);

    // The moon: a sparse hole in the line, with a soft rim.
    let (mx, my, mr) = MOON;
    let d = ((x - mx).powi(2) + (y - my).powi(2)).sqrt();
    if d < mr {
        b = b.max(1.15);
    } else if d < mr * 2.1 {
        let k = ((d - mr) / (mr * 1.1)).clamp(0.0, 1.0);
        b = b.max(1.15 - 0.35 * k);
    }

    // Two ridges, back then front — darkness where the mountains are.
    let xr = x / w * 5.0;
    let back = HORIZON - 0.16 - vnoise(0x51D6E, xr) * 0.075;
    let front = HORIZON - 0.075 - vnoise(0xF0E7, xr * 1.5) * 0.06;
    if yn > back {
        b *= 0.24;
    }
    if yn > front {
        b *= 0.16;
    }
    // Ridge-line accent: just below each crest, one band of extra density
    // so the ridges read as lines, not just regions.
    let ridge_band = |crest: f32| {
        let d = (yn - (crest + 0.012)).abs();
        (d / 0.020).clamp(0.0, 1.0)
    };
    b *= 0.34 + 0.66 * ridge_band(back).min(ridge_band(front));

    // Water: bright again, with a moonlit glitter path under the moon.
    if yn > HORIZON {
        b = 0.80;
        let gx = ((x - MOON.0) / 90.0).abs();
        if gx < 1.0 {
            let ripple = ((yn - HORIZON) * 90.0).sin() * 0.5 + 0.5;
            b += (1.0 - gx) * 0.28 * ripple;
        }
        // A few horizontal wave strokes in the water.
        b -= vnoise(0xBEA4, x / w * 22.0 + yn * 6.0) * 0.10;
    }
    b.clamp(0.0, 1.2)
}

/// One point of the line.
struct LinePoint {
    x: f32,
    y: f32,
    /// Arc length at this point, for the dash phase.
    s: f32,
}

/// The line, integrated once — a SERPENTINE scan with per-point local
/// pitch. (The first cut was a spiral whose pitch sampled the field at
/// the walking point — but the spacing between a spiral's turns at any
/// angle is the *turn-averaged* pitch, so the picture washed out to a
/// near-uniform 9-15% ink field, measured. A serpentine's row spacing at
/// (x, y) is exactly `pitch(x, y)`: the engraving's locality, exact —
/// and the rows FLOW AROUND the moon like field lines, because the next
/// row's y is integrated pointwise along the current one.)
#[must_use]
fn build_line() -> Vec<LinePoint> {
    let w = 1280.0f32;
    let h = 720.0f32;
    let mut pts: Vec<LinePoint> = Vec::with_capacity(48_000);

    // The current row, sampled at every STEP of x — the next row is
    // integrated from it, so the rows bend around the field's bright
    // holes (the moon) instead of crossing them.
    let nx = (w / STEP).ceil() as usize;
    let mut row: Vec<f32> = vec![5.0; nx + 1];
    let mut s = 0.0f32;
    let mut going_right = true;

    while row.iter().any(|&y| y < h - 3.0) {
        let idx: Vec<usize> = if going_right {
            (0..=nx).collect()
        } else {
            (0..=nx).rev().collect()
        };
        // Walk this row, emitting points; integrate the next row's y.
        let mut next: Vec<f32> = row.clone();
        let mut prev_pt: Option<(f32, f32)> = None;
        for &i in &idx {
            let x = (i as f32 * STEP).min(w - 0.5);
            let y = row[i];
            if let Some((px, py)) = prev_pt {
                s += ((x - px).powi(2) + (y - py).powi(2)).sqrt().max(STEP * 0.5);
            }
            pts.push(LinePoint { x, y, s });
            prev_pt = Some((x, y));
            // The local pitch — where the NEXT row sits under this point.
            let b = brightness(x, y, w, h);
            let pitch = PITCH_DENSE + (PITCH_SPARSE - PITCH_DENSE) * b;
            next[i] = y + pitch;
        }
        // The rows' far edge joins them: the turn happens at the border.
        row = next;
        going_right = !going_right;
        // Safety: never spin on a fully-clamped row.
        if row.iter().all(|&y| y >= h) {
            break;
        }
    }
    pts
}

/// The line, built exactly once (no census inside — the OnceLock lesson).
fn the_line() -> &'static Vec<LinePoint> {
    static LINE: OnceLock<Vec<LinePoint>> = OnceLock::new();
    LINE.get_or_init(build_line)
}

/// The total arc length.
#[must_use]
fn total_len() -> f32 {
    the_line().last().map(|p| p.s).unwrap_or(0.0)
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let pts = the_line();
    let total = total_len();

    // The beats: draw-on for the first half, then pulses lap the line.
    let draw_u = clamp01(t / 0.52);
    let drawn = total * ease_out_cubic(draw_u);
    // Two pulses, opposite senses, after the drawing completes.
    let pulse_on = t > 0.46;
    let pulse_a = ((t - 0.46) * 1.35).fract() * total;
    let pulse_b = total - ((t - 0.40) * 0.9).fract() * total;

    // One path, built per frame from the cached points (the path object is
    // cheap; the POINTS are the asset, and they are built once).
    let mut path = Path::new();
    let mut first = true;
    for p in pts {
        if first {
            path.move_to(Offset::new(p.x, p.y));
            first = false;
        } else {
            path.line_to(Offset::new(p.x, p.y));
        }
    }

    let seg_count = pts.len().saturating_sub(1);

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — a night wash barely deeper than the line's sky.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(6, 6, 9)),
                    (0.62, BG_DEEP),
                    (1.0, Color::rgb(5, 5, 8)),
                ]),
            );

            // A faint violet stage under the future drawing, growing as it
            // draws — the plate warms with its own arrival.
            let warm = 0.25 + 0.75 * draw_u;
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.47), 0.75).with_dither().with_stops(&[
                    (0.0, alpha(VIOLET_SOFT, 0.055 * warm)),
                    (1.0, alpha(VIOLET_SOFT, 0.0)),
                ]),
            );

            // ── THE LINE. One stroke, dash-phase draw-on. ──
            let style = if draw_u >= 1.0 {
                StrokeStyle::default()
            } else {
                StrokeStyle::default().dash(Dash::new(vec![drawn, total + 2.0]))
            };
            book.stroke_styled(path.clone(), alpha(INK, 0.85), 1.05, style);

            // The pulse windows — the same path again, light riding the line.
            if pulse_on {
                let pulse = 190.0;
                book.stroke_styled(
                    path.clone(),
                    alpha(Color::rgb(255, 250, 240), 0.95),
                    2.6,
                    StrokeStyle::default().dash(
                        Dash::new(vec![pulse, total - pulse]).offset(total - pulse_a),
                    ),
                );
                book.stroke_styled(
                    path.clone(),
                    alpha(Color::rgb(196, 160, 255), 0.95),
                    2.8,
                    StrokeStyle::default().dash(
                        Dash::new(vec![pulse * 0.66, total - pulse * 0.66])
                            .offset(total - pulse_b),
                    ),
                );
            }

            // The moon's glow — the one non-line element, so the eye has a
            // bright anchor (the dark-plate lesson).
            let (mx, my, mr) = MOON;
            book.circle(Offset::new(mx, my), mr + 26.0, alpha(Color::rgb(60, 62, 78), 0.5));
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));

    stack = stack.push(receipt_panel(t, seg_count, total));

    stack.into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(t: f32, segs: usize, total: f32) -> WidgetNode {
    let draw_u = clamp01(t / 0.52);
    let drawn_px = total * ease_out_cubic(draw_u);
    let laps = if t > 0.46 { ((t - 0.46) * 1.35).floor() } else { 0.0 };
    let lines = [
        "MEGAPATH · THE SINGLE-PATH AXIS · ONE LINE".to_string(),
        format!("segments {segs} · arc {total:.0} px · pitch {:.1}-{:.1} px",
            PITCH_DENSE, PITCH_SPARSE),
        format!("drawn {drawn_px:.0} px ({:.0}%) · strokes this frame {}",
            draw_u * 100.0, if t > 0.46 { 3 } else { 1 }),
        format!("pulse laps {laps:.0} · one Path, one verb, no fills"),
        format!("field: sky+moon+2 ridges+water · b(x,y) → pitch"),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 578.0;
    const P_W: f32 = 400.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(P_W)
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

    // The instrument: the draw-on progress as a thin arc-density strip.
    let strip = Painting::sized(
        Size::new(P_W, 40.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            book.rrect(
                Rect::new(0.0, 0.0, P_W, 40.0),
                8.0,
                alpha(Color::rgb(16, 16, 21), 0.88),
            );
            book.stroke_rrect(
                Rect::new(0.0, 0.0, P_W, 40.0),
                8.0,
                alpha(Color::WHITE, 0.08),
                1.0,
            );
            // The drawn fraction of the arc, as a filled bar.
            book.rect(
                Rect::new(12.0, 15.0, 12.0 + (P_W - 24.0) * draw_u, 25.0),
                alpha(VIOLET_SOFT, 0.35),
            );
            // The pulse position, as a bright tick on the same bar.
            if t > 0.46 {
                let pulse_a = ((t - 0.46) * 1.35).fract();
                let px = 12.0 + (P_W - 24.0) * pulse_a;
                book.rect(Rect::new(px - 2.0, 12.0, px + 2.0, 28.0), alpha(INK, 0.9));
            }
            book.rect(Rect::new(12.0, 30.0, 12.0 + (P_W - 24.0) * t, 33.0), alpha(FAINT, 0.5));
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 84.0)
            .width(P_W)
            .height(40.0)
            .child(strip),
    );

    stack.into()
}
