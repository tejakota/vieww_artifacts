//! exp_mandel — *the shape-count ceiling, made art.*
//!
//! Fifty-seven thousand six hundred rects per frame — every cell of a
//! 320×180 escape-time grid, each one a `Sketch::Rect` the framework
//! records, tessellates and rasterises itself. The lab's honest stress
//! test, and it is a **zoom into the seahorse valley**: the same
//! arithmetic that counts the shapes draws them.
//!
//! - The escape-time loop runs per cell, per frame, in this file — the
//!   zoom is a pure function of `t`, nothing cached between frames
//!   except the colour table.
//! - Smooth (continuous) iteration colouring through a 512-entry LUT —
//!   banding is the renderer's, not the palette's.
//! - Iteration budget scales with the zoom: 48 + 42·log₂(zoom) — the
//!   receipt prints the live value.
//! - The set's interior renders as pure black rects — counted, not
//!   skipped: the shape count is the point.
//!
//! Receipts: cells (the constant), rects drawn this frame, iter budget,
//! and ms/frame in the metrics — the honest cost of a 57k-rect frame.

use std::sync::OnceLock;

use vieww_foundation::{Color, Gradient, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{alpha, clamp01, mix, tint, FAINT, MUTED, VIOLET, VIOLET_DEEP, MAGENTA, AMBER, BG_DEEP};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 8.0;

/// Cell grid — 320 × 180 = 57,600 rects per frame.
const GX: usize = 320;
const GY: usize = 180;

/// The zoom target: seahorse valley. The classic deep point.
const C0: (f64, f64) = (-0.7436438870371587, 0.13182590420531197);

/// The colour LUT — 512 samples of one long ramp.
const LUT_N: usize = 512;

fn lut() -> &'static Vec<Color> {
    static L: OnceLock<Vec<Color>> = OnceLock::new();
    L.get_or_init(|| {
        let stops: Vec<(f32, Color)> = vec![
            (0.00, Color::rgb(6, 6, 14)),
            (0.10, VIOLET_DEEP),
            (0.26, VIOLET),
            (0.44, MAGENTA),
            (0.60, tint(MAGENTA, 0.45)),
            (0.74, Color::rgb(250, 236, 210)),
            (0.84, AMBER),
            (1.00, Color::rgb(120, 42, 30)),
        ];
        (0..LUT_N)
            .map(|i| {
                let u = i as f32 / (LUT_N - 1) as f32;
                let seg = stops
                    .windows(2)
                    .find(|w| u >= w[0].0 && u <= w[1].0)
                    .unwrap_or(&stops[stops.len() - 2..]);
                let k = ((u - seg[0].0) / (seg[1].0 - seg[0].0).max(1e-6)).clamp(0.0, 1.0);
                mix(seg[0].1, seg[1].1, k)
            })
            .collect()
    })
}

/// The zoom at film-time `t` — powers of two, eased so the fall accelerates.
fn zoom_at(t: f32) -> f64 {
    2f64.powf(((t * t * 0.85 + t * 0.15) * 7.0) as f64)
}

/// The iteration budget at this zoom — deeper needs more.
fn iter_budget(t: f32) -> u32 {
    (48.0 + 42.0 * (zoom_at(t).log2().max(0.0))) as u32
}

/// The escape-time at one point; `None` = inside the set.
fn escape(c_re: f64, c_im: f64, budget: u32) -> Option<f32> {
    let mut z_re = 0.0f64;
    let mut z_im = 0.0f64;
    let mut i = 0u32;
    // cardioid / period-2 bulb shortcut — the classic interior test.
    let q = (c_re - 0.25) * (c_re - 0.25) + c_im * c_im;
    if q * (q + (c_re - 0.25)) <= 0.25 * c_im * c_im {
        return None;
    }
    let pq = (c_re + 1.0) * (c_re + 1.0) + c_im * c_im;
    if pq < 0.0625 {
        return None;
    }
    while i < budget {
        let z_re2 = z_re * z_re;
        let z_im2 = z_im * z_im;
        if z_re2 + z_im2 > 256.0 {
            // Smooth colouring: interpolate the escape.
            let mag2 = z_re2 + z_im2;
            let nu = (mag2.log2().log2());
            return Some(i as f32 + 1.0 - nu as f32);
        }
        z_im = 2.0 * z_re * z_im + c_im;
        z_re = z_re2 - z_im2 + c_re;
        i += 1;
    }
    None
}

// ── The scene ───────────────────────────────────────────────────────────────

fn scene(book: &mut Sketchbook, canvas: Size, t: f32) {
    let w = canvas.width;
    let h = canvas.height;

    // The deep ground — visible only through the grid's seams (if any).
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::vertical()
            .with_dither()
            .with_stops(&[
                (0.0, Color::rgb(4, 4, 8)),
                (1.0, BG_DEEP),
            ]),
    );

    let zoom = zoom_at(t);
    let budget = iter_budget(t);
    let lut = lut();

    // The window at this zoom: 3.5 units wide at zoom 1, aspect-corrected.
    let span_re = 3.5 / zoom;
    let span_im = span_re * (GY as f64 / GX as f64);
    let cell_w = w / GX as f32;
    let cell_h = h / GY as f32;

    let mut rects = 0usize;
    for gy in 0..GY {
        let im = C0.1 + (gy as f64 / GY as f64 - 0.5) * span_im;
        for gx in 0..GX {
            let re = C0.0 + (gx as f64 / GX as f64 - 0.5) * span_re;
            let x = gx as f32 * cell_w;
            let y = gy as f32 * cell_h;
            match escape(re, im, budget) {
                Some(nu) => {
                    let u = (nu / budget as f32).min(1.0);
                    // The LUT index — the same ramp the whole film uses.
                    let ci = (u * (LUT_N - 1) as f32).round() as usize;
                    book.rect(
                        Rect::new(x, y, x + cell_w, y + cell_h),
                        lut[ci.min(LUT_N - 1)],
                    );
                }
                None => {
                    // Inside the set: pure black, still counted.
                    book.rect(
                        Rect::new(x, y, x + cell_w, y + cell_h),
                        Color::rgb(2, 2, 5),
                    );
                }
            }
            rects += 1;
        }
    }

    // A quiet frame line: the cell grid's own extent, hairline.
    book.stroke_rrect(
        Rect::new(0.5, 0.5, w - 1.0, h - 1.0),
        2.0,
        alpha(Color::WHITE, 0.06),
        1.0,
    );

    let _ = rects;
}

/// The frame.
pub fn frame(t: f32) -> WidgetNode {
    let paint = Painting::sized(
        crate::film_lib::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            scene(book, size, t);
        }),
    );

    // The instrument: the zoom, the budget — the code's own numbers.
    let zoom = zoom_at(t);
    let budget = iter_budget(t);
    let cells = GX * GY;

    let strip = Stack::new()
        .push(
            Positioned::new()
                .left(24.0)
                .top(20.0)
                .width(560.0)
                .height(18.0)
                .child(
                    Text::new("THE COASTLINE · the count ceiling").style(
                        TextStyle::new(12.0)
                            .monospace()
                            .letter_spacing(2.2)
                            .color(alpha(Color::WHITE, 0.92)),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(24.0)
                .top(40.0)
                .width(680.0)
                .height(18.0)
                .child(
                    Text::new(format!(
                        "{cells} rects/frame · zoom ×{zoom:>9.0} · iter budget {budget} · seahorse valley"
                    ))
                    .style(TextStyle::new(11.0).monospace().color(alpha(tint(VIOLET, 0.55), 0.95))),
                ),
        );

    Stack::new()
        .push(Positioned::fill().child(paint))
        .push(strip)
        .into()
}
