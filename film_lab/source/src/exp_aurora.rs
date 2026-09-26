//! exp_aurora — *light as material.* The background craft experiment.
//!
//! Two never-before-used primitives carry this experiment:
//!
//! - **`Gradient::sweep` / `conic`** — the ramp that turns. A lighthouse cone
//!   rotates over the void (a sweep gradient whose angles animate), the
//!   first conic gradient in the film's vocabulary.
//! - **`Blend` + `BlendMode::Screen`** — compositing as lighting. Three
//!   aurora ribbons, each its own painting, nested through three Screen
//!   blends so the lights *add* — over the void, over the stars, and over
//!   each other — the additive truth of light, in the framework's own
//!   blend pipeline.
//!
//! Then, **E-21: crafted degradation** — the wait's look, built rather than
//! suffered. The whole aurora world passes through a `FilterChain` ramp —
//! saturation bleeding out, brightness falling — while dust motes drift in
//! and per-frame grain arrives (seeded by the frame index derived from `t`:
//! deterministic, but *alive*). And then the release: the cut into S02 is
//! silence *and* clarity — saturation snaps back on a spring.
//!
//! A shooting star closes the experiment: one spark, one tail, gone.

use vieww_effects::{Blend, BlendMode, Filter, FilterChain};
use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::Painting;
use vieww_widget::PaintWith;

use crate::film_lib::{
    alpha, clamp01, mix, smoothstep, spring_out, BG_DEEP, CANVAS, CANVAS_H, CANVAS_W, MAGENTA,
    MUTED, Rng, VIOLET, VIOLET_DEEP, VIOLET_SOFT, CYAN, CYAN_SOFT,
};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 10.0;

// ── The degradation state machine: E-21's ramp and release ─────────────────

/// How degraded the world is at `t` — 0 pristine, 1 the wait.
fn degrade(t: f32) -> f32 {
    let ramp = smoothstep(clamp01((t - 0.52) / 0.20));
    // The release: the cut into S02 — clarity returns fast, sprung.
    let release = spring_out(clamp01((t - 0.88) / 0.12), 14.0, 0.7);
    ramp * (1.0 - release)
}

// ── The pieces, each its own painting ───────────────────────────────────────

/// The void and its stars — the world the light lands on.
fn void_paint(t: f32) -> WidgetNode {
    Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(10, 9, 16)),
                    (0.5, BG_DEEP),
                    (1.0, Color::rgb(13, 11, 21)),
                ]),
            );
            // The void's own breath: a faint violet nebula low-center, so the
            // darkness has texture and the lights land on *something*.
            book.circle(
                Offset::new(w * 0.5, h * 0.72),
                w * 0.42,
                Gradient::radial_fill().with_dither().with_stops(&[
                    (0.0, alpha(VIOLET, 0.05)),
                    (1.0, alpha(VIOLET, 0.0)),
                ]),
            );
            let mut rng = Rng::new(0xA10A);
            for _ in 0..110 {
                let x = rng.f01() * w;
                let y = rng.f01() * h;
                let r = 0.4 + rng.f01() * 1.0;
                let tw = 0.5 + 0.5 * (t * 3.0 + rng.f01() * 11.0).sin();
                book.circle(Offset::new(x, y), r, alpha(Color::WHITE, 0.05 + 0.10 * tw));
            }
        }),
    )
    .into()
}

/// The lighthouse cone — a rotating sweep gradient, the conic's first role.
fn cone_paint(t: f32) -> WidgetNode {
    let fade = 1.0 - clamp01((t - 0.34) / 0.22);
    Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            if fade <= 0.01 {
                return;
            }
            let center = Offset::new(CANVAS_W * 0.70, CANVAS_H * 0.42);
            let theta = t * std::f32::consts::TAU * 0.55;
            // The cone: a full circle filled with a FULL-TURN sweep whose
            // stops keep both edges transparent — the bright band rides
            // ahead of the beam with no pad artifacts at the seam.
            book.circle(
                center,
                640.0,
                Gradient::sweep(
                    Offset::new(0.5, 0.5),
                    theta - std::f32::consts::TAU,
                    theta,
                )
                .with_stops(&[
                    (0.0, alpha(VIOLET, 0.0)),
                    (0.70, alpha(VIOLET, 0.0)),
                    (0.84, alpha(mix(VIOLET, CYAN_SOFT, 0.3), 0.13 * fade)),
                    (0.93, alpha(CYAN_SOFT, 0.20 * fade)),
                    (0.97, alpha(VIOLET, 0.0)),
                    (1.0, alpha(VIOLET, 0.0)),
                ]),
            );
            // The pivot: a small bright point + its breathing ring.
            book.circle(center, 3.0, alpha(Color::WHITE, 0.9 * fade));
            book.ring(
                center,
                26.0 + 10.0 * (t * 2.0).sin(),
                1.2,
                alpha(VIOLET_SOFT, 0.30 * fade),
            );
            let _ = size;
        }),
    )
    .into()
}

/// One aurora ribbon — a wide gradient stroke on a waving cubic, softened.
fn ribbon_paint(t: f32, idx: usize, color: Color, base_y: f32, phase: f32, width: f32) -> WidgetNode {
    let arrive = clamp01(t / 0.30);
    Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            if arrive <= 0.01 {
                return;
            }
            // The wave: control points breathing at the film's tempo.
            let wave = |s: f32| (t * 0.9 + phase + s * 2.2).sin() * (46.0 + 16.0 * idx as f32);
            let y_at = |s: f32| base_y + wave(s);
            let mut p = Path::new();
            p.move_to(Offset::new(-90.0, y_at(0.0)));
            for i in 1..=40 {
                let s = i as f32 / 40.0;
                let x = -90.0 + (CANVAS_W + 180.0) * s;
                // A smooth curve through the wave samples.
                p.line_to(Offset::new(x, y_at(s)));
            }
            // Wide gradient stroke — the ribbon's body — then a blur layer
            // for the softness light actually has.
            book.layer(1.0, 26.0, None, |inner| {
                inner.stroke(
                    p.clone(),
                    Gradient::vertical().with_stops(&[
                        (0.0, alpha(color, 0.0)),
                        (0.45, alpha(color, 0.34 * arrive)),
                        (1.0, alpha(color, 0.0)),
                    ]),
                    width,
                );
                // The ribbon's bright core — a thin stroke inside the body.
                inner.stroke(
                    p.clone(),
                    Gradient::vertical().with_stops(&[
                        (0.0, alpha(color, 0.0)),
                        (0.5, alpha(mix(color, Color::WHITE, 0.45), 0.28 * arrive)),
                        (1.0, alpha(color, 0.0)),
                    ]),
                    width * 0.3,
                );
            });
            // The curtain: vertical rays rising off the ribbon — the
            // structure an aurora actually has. Each ray is a short vertical
            // gradient stroke, its height and brightness modulated by a
            // deterministic slow wave along the ribbon's length.
            book.layer(1.0, 7.0, None, |inner| {
                let mut rng = Rng::new(0xC0A7 ^ (idx as u64 + 1));
                let n_rays = 26;
                for i in 0..n_rays {
                    let s = i as f32 / n_rays as f32;
                    let x = -60.0 + (CANVAS_W + 120.0) * s;
                    let wob = (t * 1.1 + phase + s * 5.3).sin();
                    let ray_h = (46.0 + 74.0 * (0.5 + 0.5 * wob)) * arrive;
                    if ray_h < 6.0 {
                        continue;
                    }
                    let y_base = y_at(s.clamp(0.0, 1.0)) + rng.sym() * 8.0;
                    let flicker = 0.5 + 0.5 * (t * 3.2 + s * 9.7).sin();
                    let a = 0.10 + 0.13 * (0.5 + 0.5 * wob) * flicker;
                    let mut ray = Path::new();
                    ray.move_to(Offset::new(x, y_base));
                    ray.line_to(Offset::new(x + rng.sym() * 6.0, y_base - ray_h));
                    inner.stroke(
                        ray,
                        Gradient::vertical().with_stops(&[
                            (0.0, alpha(color, 0.0)),
                            (1.0, alpha(mix(color, Color::WHITE, 0.25), a * arrive)),
                        ]),
                        2.0 + 2.6 * (0.5 + 0.5 * wob),
                    );
                }
            });
            let _ = size;
        }),
    )
    .into()
}

/// The shooting star — one spark, one tail, in the cyan ribbon's layer.
fn star_paint(t: f32) -> WidgetNode {
    let t0 = 0.72;
    let dur = 0.14;
    let s = clamp01((t - t0) / dur);
    Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _size: Size| {
            if s <= 0.0 || s >= 1.0 {
                return;
            }
            let x = 180.0 + 920.0 * s;
            let y = 70.0 + 170.0 * s;
            let head = Offset::new(x, y);
            let tail_len = 150.0 * (1.0 - s * 0.4);
            let tail = Offset::new(x - tail_len, y - tail_len * 0.28);
            // The tail: a gradient stroke, bright at the head.
            let mut path = Path::new();
            path.move_to(tail);
            path.line_to(head);
            book.layer(1.0, 6.0, None, |inner| {
                inner.stroke(
                    path,
                    Gradient::horizontal().with_stops(&[
                        (0.0, alpha(CYAN_SOFT, 0.0)),
                        (1.0, alpha(Color::WHITE, 0.65 * (1.0 - s))),
                    ]),
                    2.2,
                );
            });
            // The spark.
            book.layer(1.0, 7.0, None, |inner| {
                inner.circle(head, 12.0, Gradient::radial_fill().with_stops(&[
                    (0.0, alpha(Color::WHITE, 0.8 * (1.0 - s))),
                    (1.0, alpha(CYAN, 0.0)),
                ]));
            });
        }),
    )
    .into()
}

/// Dust and grain — the wait's air, arriving with the degradation.
fn dust_paint(t: f32) -> WidgetNode {
    let degrade_t = degrade(t);
    // The frame index, derived from t — grain that varies per frame but
    // re-renders to the byte.
    let frame_i = (t * SECONDS * 60.0).round() as u64;
    Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;
            if degrade_t <= 0.01 {
                return;
            }
            // Dust motes: slow drifters, seeded once — the wait's air, now
            // thick enough to see.
            let mut rng = Rng::new(0xD057);
            for _ in 0..60 {
                let bx = rng.f01() * w;
                let by = rng.f01() * h;
                let r = 0.8 + rng.f01() * 1.7;
                let drift = (t * 1.4 + rng.f01() * 7.0).sin() * 10.0;
                let fall = t * 12.0 * rng.f01();
                book.circle(
                    Offset::new(bx + drift, (by + fall) % h),
                    r,
                    alpha(MUTED, 0.22 * degrade_t),
                );
            }
            // Grain: per-frame soft specks — round, small, alpha-varied; the
            // squares were a bug class of their own (macroblocking at sheet
            // scale). Round reads as film, square reads as corruption.
            let mut rng = Rng::new(0x6A1D ^ frame_i.wrapping_mul(0x9E37));
            let n = (260.0 * degrade_t) as usize;
            for _ in 0..n {
                let x = rng.f01() * w;
                let y = rng.f01() * h;
                let bright = rng.f01() > 0.5;
                let a = 0.015 + rng.f01() * 0.035;
                book.circle(
                    Offset::new(x, y),
                    0.5 + rng.f01() * 0.7,
                    if bright { alpha(Color::WHITE, a) } else { alpha(Color::BLACK, a * 1.6) },
                );
            }
        }),
    )
    .into()
}

/// The vignette — always last, never blended.
fn vignette_paint() -> WidgetNode {
    Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.5), 0.82).with_dither().with_stops(&[
                    (0.62, alpha(Color::BLACK, 0.0)),
                    (1.0, alpha(Color::BLACK, 0.5)),
                ]),
            );
        }),
    )
    .into()
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    // The lights: cone + three ribbons + the star, Screen-nested so light
    // adds — over the void, over the stars, over each other.
    let lights = Blend::new(BlendMode::Screen)
        .foreground(star_paint(t))
        .background(
            Blend::new(BlendMode::Screen)
                .foreground(ribbon_paint(t, 2, CYAN, 470.0, 2.1, 120.0))
                .background(
                    Blend::new(BlendMode::Screen)
                        .foreground(ribbon_paint(t, 1, mix(MAGENTA, VIOLET, 0.4), 310.0, 1.3, 150.0))
                        .background(
                            Blend::new(BlendMode::Screen)
                                .foreground(ribbon_paint(t, 0, VIOLET_DEEP, 190.0, 0.6, 170.0))
                                .background(cone_paint(t)),
                        ),
                ),
        );

    // The world: lights over the void, Screen.
    let world = Blend::new(BlendMode::Screen)
        .foreground(lights)
        .background(void_paint(t));

    // E-21: the degradation ramp — saturation bleeds, brightness falls.
    let d = degrade(t);
    let filtered = FilterChain::new()
        .filter(Filter::saturation(1.0 - 0.66 * d))
        .filter(Filter::brightness(1.0 - 0.22 * d))
        .child(world);

    // The technique label — the lab's bookkeeping voice.
    let label = Positioned::new()
        .left(34.0)
        .top(28.0)
        .width(560.0)
        .height(16.0)
        .child(
            Text::new("LIGHT AS MATERIAL · SWEEP CONE · SCREEN ×4 · E-21 RAMP").style(
                TextStyle::new(10.5)
                    .monospace()
                    .letter_spacing(2.2)
                    .color(alpha(MUTED, 0.4)),
            ),
        );

    Stack::new()
        .push(Positioned::fill().child(filtered))
        .push(Positioned::fill().child(dust_paint(t)))
        .push(Positioned::fill().child(vignette_paint()))
        .push(label)
        .into()
}
