//! exp_prism — *the gradient axis.* The spectrum furnace.
//!
//! One white beam enters a prism; a fan of forty-eight gradient-filled
//! rays leaves it and lands on a screen as **a 256-stop spectrum whose
//! stop offsets advance every frame and re-sort every frame** — U-08's
//! discipline (an animated phase must re-sort; reversed stops are a
//! debug panic, not a flat frame) run at 128× the stop count any plate
//! has ever carried.
//!
//! **The finding the axis was built to catch, caught on its first day:**
//! `Gradient::with_stops` carries `MAX_GRADIENT_STOPS = 8` and silently
//! `.take(8)`s anything longer — the plate's first screen was a flat
//! orange panel (the sorted first eight stops are all red-orange), no
//! assert, no warning. The stop field therefore rides as geometry:
//! **256 flat rects, one per stop, at the stop's own offset** — the
//! re-sort animation becomes the rects breathing — with the 8-stop
//! gradient the framework *can* carry drawn beneath. The ceiling is
//! the receipt; the workaround is the art. (todo-upgrades U-23.)
//!
//! Around the prism, the other doors of the gradient family at once:
//! three **sweep** rings built `with_stops_mirrored` (U-10 — a full-turn
//! sweep with a non-palindrome ramp seams at the wrap; the mirror is the
//! construction that cannot), a `radial_fill` bloom at the beam's entry,
//! and forty-eight per-ray two-stop linear gradients — **gradient-per-shape
//! at volume**, the axis `galaxy` never measured (its stars were flat).
//!
//! The receipt prints the stop census and the gradient census, computed by
//! the same arrays that drew the frame.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, mix, BG_DEEP, FAINT, INK, MUTED, VIOLET_SOFT, CYAN_SOFT,
    AMBER};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 10.0;

/// The stop census — the axis.
const STOPS: usize = 256;

/// The fan's ray count.
const RAYS: usize = 48;

// ── Geometry ─────────────────────────────────────────────────────────────────

/// The prism's vertices.
const TRI: [(f32, f32); 3] = [(438.0, 392.0), (576.0, 232.0), (576.0, 512.0)];

/// The beam's entry point (on the left face) and its exit (on the right).
const ENTRY: (f32, f32) = (486.0, 328.0);
const EXIT: (f32, f32) = (572.0, 316.0);

/// The screen — where the spectrum lands.
const SCREEN: (f32, f32, f32, f32) = (1042.0, 128.0, 1206.0, 592.0); // x0, y0, x1, y1

// ── Colour helpers ──────────────────────────────────────────────────────────

/// HSV → RGB (h in turns, s and v in [0, 1]).
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
    Color::rgb(
        (r * 255.0) as u8,
        (g * 255.0) as u8,
        (b * 255.0) as u8,
    )
}

// ── The animated spectrum — the axis itself ────────────────────────────────

/// The screen's 256 stops this frame: offsets phase-advance, jitter, and
/// **re-sort** (U-08 — the animated ramp is rebuilt non-decreasing every
/// frame; the sort IS the discipline, run at 256 stops).
#[must_use]
fn spectrum_stops(t: f32) -> Vec<(f32, Color)> {
    let mut stops: Vec<(f32, Color)> = Vec::with_capacity(STOPS);
    for i in 0..STOPS {
        // The base ramp: hue across the screen.
        let base = i as f32 / (STOPS - 1) as f32;
        // The phase advance — different harmonics per stop, so the ramp
        // flows rather than slides.
        let w1 = (t * 6.2832 * 2.0 + base * 6.2832 * 3.0).sin() * 0.012;
        let w2 = (t * 6.2832 * 5.0 + base * 6.2832 * 7.0).sin() * 0.005;
        let off = (base + w1 + w2).clamp(0.0, 1.0);
        // Saturation breathes along the band.
        let s = 0.72 + 0.24 * (t * 6.2832 + base * 9.0).sin();
        let v = 0.86 + 0.13 * (t * 4.0 + base * 13.0).sin();
        let hue = base + 0.04 * (t + base * 2.0).sin();
        stops.push((off, hsv(hue, s.clamp(0.0, 1.0), v.clamp(0.0, 1.0))));
    }
    stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    // De-duplicate exact-equal offsets the harmonics can produce (a stop
    // pair at the same offset is legal; keeping both is noise).
    stops.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-6);
    stops
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let stops = spectrum_stops(t);
    let stop_count = stops.len();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — a dark gallery.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(6, 6, 10)),
                    (0.7, BG_DEEP),
                    (1.0, Color::rgb(4, 4, 7)),
                ]),
            );

            // The incident beam — white, from the left, breathing.
            let breath = 0.75 + 0.25 * (t * 6.2832 * 1.1).sin();
            let beam_h = 7.0 * breath;
            book.fill(
                {
                    let mut p = Path::new();
                    p.move_to(Offset::new(0.0, 356.0 - beam_h * 0.5));
                    p.line_to(Offset::new(ENTRY.0, ENTRY.1 - beam_h * 0.9));
                    p.line_to(Offset::new(ENTRY.0, ENTRY.1 + beam_h * 0.9));
                    p.line_to(Offset::new(0.0, 356.0 + beam_h * 0.5));
                    p.close();
                    p
                },
                Gradient::horizontal().with_dither().with_stops(&[
                    (0.0, alpha(Color::rgb(220, 225, 240), 0.0)),
                    (0.55, alpha(Color::rgb(228, 232, 244), 0.20 * breath)),
                    (1.0, alpha(Color::rgb(255, 255, 255), 0.46 * breath)),
                ]),
            );
            // The entry bloom.
            book.circle(
                Offset::new(ENTRY.0, ENTRY.1),
                20.0 * breath,
                Gradient::radial_fill().with_dither().with_stops(&[
                    (0.0, alpha(Color::rgb(255, 255, 250), 0.4 * breath)),
                    (1.0, alpha(Color::WHITE, 0.0)),
                ]),
            );

            // The sweep rings — U-10's mirrored palindrome, three of them,
            // rotating their phases at different rates.
            for k in 0..3 {
                let r = 96.0 + k as f32 * 34.0;
                let phase = t * (1.1 + k as f32 * 0.6);
                let a0 = phase * std::f32::consts::TAU;
                book.ring(
                    Offset::new(524.0, 372.0),
                    r,
                    7.0 - k as f32 * 1.6,
                    Gradient::sweep(Offset::new(0.5, 0.5), a0, a0 + std::f32::consts::TAU)
                        .with_dither()
                        .with_stops_mirrored(&[
                            (0.0, alpha(Color::WHITE, 0.0)),
                            (0.5, alpha(hsv(0.58 + k as f32 * 0.09, 0.7, 0.9), 0.20)),
                            (1.0, alpha(hsv(0.66 + k as f32 * 0.09, 0.8, 0.9), 0.0)),
                        ]),
                );
            }

            // The prism — glass triangle, cool and translucent.
            book.fill(
                {
                    let mut p = Path::new();
                    p.move_to(Offset::new(TRI[0].0, TRI[0].1));
                    p.line_to(Offset::new(TRI[1].0, TRI[1].1));
                    p.line_to(Offset::new(TRI[2].0, TRI[2].1));
                    p.close();
                    p
                },
                Gradient::linear(Offset::new(0.2, 0.0), Offset::new(0.8, 1.0))
                    .with_dither()
                    .with_stops(&[
                        (0.0, alpha(CYAN_SOFT, 0.10)),
                        (0.55, alpha(Color::rgb(180, 200, 230), 0.07)),
                        (1.0, alpha(VIOLET_SOFT, 0.12)),
                    ]),
            );
            // The prism's edges — bright hairlines.
            let mut edge = Path::new();
            edge.move_to(Offset::new(TRI[0].0, TRI[0].1));
            edge.line_to(Offset::new(TRI[1].0, TRI[1].1));
            edge.line_to(Offset::new(TRI[2].0, TRI[2].1));
            edge.close();
            book.stroke(edge, alpha(mix(INK, CYAN_SOFT, 0.4), 0.8), 1.4);

            // ── THE FAN — forty-eight rays, each its own gradient. ──
            // The sweep of the fan breathes with the incident angle.
            let sweep = (0.42 + 0.10 * (t * 6.2832 * 0.8).sin()) * std::f32::consts::PI;
            for k in 0..RAYS {
                let f = k as f32 / (RAYS - 1) as f32;
                let a = -sweep * 0.5 + f * sweep;
                let hue = f;
                // Ray length: the screen is far; the fan converges on it.
                let len = 620.0 + (f - 0.5).abs() * -180.0;
                let ex = EXIT.0 + a.cos() * len;
                let ey = EXIT.1 + a.sin() * len;
                let spread = 2.2 + f * 1.6;
                let mut ray = Path::new();
                ray.move_to(Offset::new(EXIT.0 - 4.0, EXIT.1 - 2.0));
                ray.line_to(Offset::new(EXIT.0 - 4.0, EXIT.1 + 2.0));
                ray.line_to(Offset::new(ex + spread, ey + spread * 0.4));
                ray.line_to(Offset::new(ex - spread, ey - spread * 0.4));
                ray.close();
                let dim = 0.5 + 0.5 * (t * 3.1 + f * 6.0).sin();
                book.fill(
                    ray,
                    Gradient::linear(Offset::new(0.0, 0.5), Offset::new(1.0, 0.5))
                        .with_dither()
                        .with_stops(&[
                            (0.0, alpha(hsv(hue, 0.85, 1.0), 0.5)),
                            (0.7, alpha(hsv(hue + 0.03, 0.9, 0.9), 0.16 + 0.10 * dim)),
                            (1.0, alpha(hsv(hue + 0.06, 0.9, 0.8), 0.0)),
                        ]),
                );
            }

            // ── THE SCREEN — the stop field, drawn as geometry. ──
            // THE FINDING OF THE PLATE: `Gradient::with_stops` carries
            // MAX_GRADIENT_STOPS = 8 and silently `.take(8)`s the rest —
            // the first render of this screen was a flat orange panel
            // (the sorted first eight stops are all red-orange) with no
            // assert, no warning. So the 256-stop spectrum rides as 256
            // flat rects, one per stop, at the stop's own offset — and
            // the re-sort animation becomes the rects breathing. The
            // gradient the framework CAN carry (8 stops) rides beneath.
            let (sx0, sy0, sx1, sy1) = SCREEN;
            book.rect(
                Rect::new(sx0, sy0, sx1, sy1),
                Gradient::horizontal().with_dither().with_stops(&[
                    (0.0, hsv(0.0, 0.8, 0.9)),
                    (0.17, hsv(0.14, 0.85, 0.92)),
                    (0.33, hsv(0.33, 0.8, 0.95)),
                    (0.5, hsv(0.5, 0.75, 0.95)),
                    (0.67, hsv(0.67, 0.8, 0.92)),
                    (0.83, hsv(0.83, 0.8, 0.9)),
                    (1.0, hsv(0.95, 0.8, 0.9)),
                ]),
            );
            for w in stops.windows(2) {
                let (o0, c0) = w[0];
                let (o1, _) = w[1];
                let x0 = sx0 + o0 * (sx1 - sx0);
                let x1 = sx0 + o1 * (sx1 - sx0);
                if x1 - x0 > 0.05 {
                    book.rect(Rect::new(x0, sy0, x1, sy1), c0);
                }
            }
            // A hairline frame + tick marks every 32 stops.
            book.stroke_rrect(
                Rect::new(sx0 - 3.0, sy0 - 3.0, sx1 + 3.0, sy1 + 3.0),
                4.0,
                alpha(FAINT, 0.4),
                1.0,
            );
            for k in 1..8 {
                let yy = sy0 + (sy1 - sy0) * k as f32 / 8.0;
                book.line(
                    Offset::new(sx1 + 3.0, yy),
                    Offset::new(sx1 + 9.0, yy),
                    alpha(FAINT, 0.6),
                    1.0,
                );
            }

            // The exit bloom at the prism's far face.
            book.circle(
                Offset::new(EXIT.0, EXIT.1),
                14.0,
                Gradient::radial_fill().with_dither().with_stops(&[
                    (0.0, alpha(Color::rgb(255, 255, 255), 0.5)),
                    (1.0, alpha(Color::WHITE, 0.0)),
                ]),
            );
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack.push(receipt_panel(t, stop_count)).into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(t: f32, live_stops: usize) -> WidgetNode {
    let lines = [
        "PRISM · THE GRADIENT AXIS · THE SPECTRUM FURNACE".to_string(),
        format!("stop field {live_stops}/256 · drawn as {live_stops} rects"),
        "MAX_GRADIENT_STOPS=8 · the framework's ceiling, measured".to_string(),
        format!("rays {RAYS} · gradient-per-ray (3 stops each) · re-sort U-08"),
        "3 sweep rings · with_stops_mirrored (U-10 seam-free)".to_string(),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 566.0;

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

    // The instrument: the stop field itself — 64 sampled stops drawn as a
    // comb, advancing phase visible as the comb's breathing.
    let sample: Vec<(f32, Color)> = spectrum_stops(t)
        .into_iter()
        .step_by(4)
        .collect();
    let comb = Painting::sized(
        Size::new(400.0, 40.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            book.rrect(
                Rect::new(0.0, 0.0, 400.0, 40.0),
                8.0,
                alpha(Color::rgb(16, 16, 21), 0.88),
            );
            book.stroke_rrect(
                Rect::new(0.0, 0.0, 400.0, 40.0),
                8.0,
                alpha(Color::WHITE, 0.08),
                1.0,
            );
            for (off, c) in &sample {
                let x = 12.0 + off * 376.0;
                book.rect(Rect::new(x - 1.4, 24.0, x + 1.4, 33.0), alpha(*c, 0.9));
            }
            // The phase playhead — the ramp's flow, marked.
            let px = 12.0 + (t * 6.2832 * 2.0).sin() * 0.5 + 0.5;
            book.rect(Rect::new(12.0 + px * 376.0 - 0.8, 12.0, 12.0 + px * 376.0 + 0.8, 20.0),
                alpha(AMBER, 0.7));
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 84.0)
            .width(400.0)
            .height(40.0)
            .child(comb),
    );

    stack.into()
}
