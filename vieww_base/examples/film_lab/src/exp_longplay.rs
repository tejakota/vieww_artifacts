//! exp_longplay — *the endurance axis.* 256 frames, one scene, no reset.
//!
//! The longest render before this was 32 frames. `hero4k`'s OOM at 4
//! frames × 4K said memory accumulates somewhere across frames; this plate
//! is the other half of that measurement — **8× the longest run, at lab
//! resolution, with the kernel's own number sampled every frame**: VmRSS
//! read from `/proc/self/status` and the build/raster split, written to
//! metrics.txt as a `series=` line every 16 frames. If RSS is flat, the
//! 1080p-pipeline accumulator hero4k tripped over is a 4K-scale fact; if
//! it climbs, the master plan needs a chunked-pass discipline at every
//! resolution. Either way, the next memory argument starts from a curve,
//! not a hunch.
//!
//! The content must earn 256 frames without repeating itself: **the night
//! watch** — a lighthouse on a headland, its beam sweeping a slow arc,
//! tide creeping up the shore line, stars drifting with the sky's own
//! rotation, a moon that sets over the plate's whole duration, one ship
//! crossing the horizon with a cabin light. Everything is closed-form —
//! no state, no accumulation — so the ONLY thing that can drift across
//! 256 frames is the renderer's own memory, which is the point.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, ease_in_out, mix, smoothstep, Rng, Receipt, BG, BG_DEEP,
    FAINT, INK, MUTED, VIOLET_SOFT, CYAN_SOFT, AMBER};

/// Film-time this experiment spans — the endurance axis: 256 frames of it.
pub const SECONDS: f32 = 256.0 / 12.0;

// ── The scene's fixed geography ─────────────────────────────────────────────

/// The lighthouse, on the right headland.
const LH: (f32, f32) = (1006.0, 402.0);

/// The horizon's y.
const HORIZON: f32 = 430.0;

/// A deterministic star.
struct Star {
    x: f32,
    y: f32,
    r: f32,
    tw: f32,
    phase: f32,
}

#[must_use]
fn stars() -> Vec<Star> {
    let mut rng = Rng::new(0x4C4F);
    (0..170)
        .map(|_| {
            let x = rng.f01() * 1280.0;
            let y = rng.f01() * rng.f01() * 340.0;
            Star {
                x,
                y,
                r: 0.7 + rng.f01() * 1.5,
                tw: 0.5 + rng.f01() * 1.7,
                phase: rng.f01(),
            }
        })
        .collect()
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let film = t * SECONDS;
    let st = stars();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The sky — deepening as the moon sets.
            let moon_k = 1.0 - clamp01(t / 0.92); // the moon sinks over the plate
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(5, 5, 9)),
                    (0.55, Color::rgb(8, 9, 14)),
                    (1.0, BG_DEEP),
                ]),
            );

            // The stars — drifting right with the sky's rotation, twinkling.
            let drift = t * 18.0;
            for s in &st {
                let x = ((s.x + drift) % 1280.0 + 1280.0) % 1280.0;
                let tw = (film * s.tw + s.phase * 6.283).sin() * 0.5 + 0.5;
                book.circle(
                    Offset::new(x, s.y),
                    s.r,
                    alpha(INK, 0.25 + 0.55 * tw * (0.4 + 0.6 * moon_k + 0.4)),
                );
            }

            // The moon — setting, left to right, over the whole plate.
            let mx = 180.0 + t * 980.0;
            let my = 96.0 + smoothstep(clamp01(t / 0.9)) * 300.0;
            let moon_a = 1.0 - clamp01((t - 0.86) / 0.12);
            if moon_a > 0.0 {
                book.circle(Offset::new(mx, my), 30.0, alpha(Color::rgb(233, 230, 222), 0.16 * moon_a));
                book.circle(Offset::new(mx, my), 19.0, alpha(Color::rgb(238, 235, 226), 0.92 * moon_a));
                book.circle(Offset::new(mx - 6.0, my - 4.0), 4.5, alpha(Color::rgb(214, 210, 200), 0.5 * moon_a));
                book.circle(Offset::new(mx + 5.0, my + 6.0), 3.0, alpha(Color::rgb(214, 210, 200), 0.4 * moon_a));
            }

            // The sea — a gradient with the moon's lane, darkening as it sets.
            book.rect(
                Rect::new(0.0, HORIZON, w, h - HORIZON),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(13, 17, 27)),
                    (0.45, Color::rgb(9, 12, 20)),
                    (1.0, Color::rgb(5, 6, 10)),
                ]),
            );
            // The moon's glitter lane on the water.
            if moon_a > 0.0 {
                for row in 0..26 {
                    let yy = HORIZON + 6.0 + row as f32 * 8.2;
                    let spread = 8.0 + row as f32 * 3.4;
                    let mut rng = Rng::new(0xA00 + row as u64);
                    for _ in 0..(2 + row / 3) {
                        let gx = mx + rng.sym() * spread;
                        let glint = rng.f01();
                        book.rect(
                            Rect::new(gx, yy, gx + 2.0 + glint * 5.0, yy + 1.4),
                            alpha(Color::rgb(232, 228, 216),
                                (0.35 - row as f32 * 0.011).max(0.05) * moon_a),
                        );
                    }
                }
            }

            // The ship — one crossing, slow, a cabin light.
            let ship_x = -60.0 + t * 1420.0;
            let ship_y = HORIZON + 14.0;
            let bob = (film * 1.9).sin() * 1.6;
            book.rect(
                Rect::new(ship_x, ship_y + bob, ship_x + 30.0, ship_y + 6.0 + bob),
                alpha(Color::rgb(16, 18, 24), 1.0),
            );
            book.rect(
                Rect::new(ship_x + 9.0, ship_y - 9.0 + bob, ship_x + 20.0, ship_y + bob),
                alpha(Color::rgb(14, 16, 22), 1.0),
            );
            book.circle(
                Offset::new(ship_x + 14.5, ship_y - 3.0 + bob),
                1.8,
                alpha(AMBER, 0.9),
            );

            // The far headland — right side.
            let mut land = Path::new();
            land.move_to(Offset::new(830.0, h));
            land.line_to(Offset::new(860.0, HORIZON + 8.0));
            land.line_to(Offset::new(1280.0, HORIZON + 2.0));
            land.line_to(Offset::new(1280.0, h));
            book.fill(land, alpha(Color::rgb(10, 11, 16), 1.0));

            // The near shore — left side, the tide creeping UP it.
            let tide = 46.0 + 26.0 * (t * 6.2832).sin() * 0.5 + 26.0 * smoothstep(t);
            let mut shore = Path::new();
            shore.move_to(Offset::new(0.0, h));
            shore.line_to(Offset::new(0.0, 512.0));
            shore.cubic_to(
                Offset::new(120.0, 540.0 - tide * 0.4),
                Offset::new(280.0, 560.0 - tide * 0.8),
                Offset::new(470.0, 574.0 - tide),
            );
            shore.line_to(Offset::new(470.0, h));
            book.fill(shore, alpha(Color::rgb(13, 14, 19), 1.0));
            // The wet edge of the sand.
            book.stroke(
                {
                    let mut p = Path::new();
                    p.move_to(Offset::new(0.0, 512.0));
                    p.cubic_to(
                        Offset::new(120.0, 540.0 - tide * 0.4),
                        Offset::new(280.0, 560.0 - tide * 0.8),
                        Offset::new(470.0, 574.0 - tide),
                    );
                    p
                },
                alpha(CYAN_SOFT, 0.14),
                1.2,
            );

            // ── The lighthouse: the tower, the lamp, the beam ──
            let (lx, ly) = LH;
            // Tower.
            book.rrect(Rect::new(lx - 11.0, ly - 54.0, lx + 11.0, HORIZON + 8.0), 3.0,
                alpha(Color::rgb(22, 24, 32), 1.0));
            for band in 0..3 {
                book.rect(
                    Rect::new(lx - 11.0, ly - 44.0 + band as f32 * 16.0, lx + 11.0,
                        ly - 38.0 + band as f32 * 16.0),
                    alpha(Color::rgb(180, 86, 82), 0.85),
                );
            }
            // The lamp room.
            book.rrect(Rect::new(lx - 8.0, ly - 66.0, lx + 8.0, ly - 52.0), 2.0,
                alpha(Color::rgb(28, 30, 40), 1.0));

            // The beam — a sweep, cone, once per ~4.3 s. Two blades so the
            // sweep reads from both sides.
            let sweep = (film * 1.46).fract() * 6.2832;
            for blade in [0.0, std::f32::consts::PI] {
                let a = sweep + blade;
                // Only draw while the beam points out to sea / across sky.
                let dirx = a.cos();
                let diry = a.sin() * 0.34;
                let len = 560.0;
                let tipx = lx + dirx * len;
                let tipy = (ly - 58.0) + diry * len;
                let mut beam = Path::new();
                beam.move_to(Offset::new(lx - 4.0, ly - 58.0));
                beam.line_to(Offset::new(tipx - diry * 34.0, tipy + 0.0));
                beam.line_to(Offset::new(tipx + diry * 34.0, tipy + 0.0));
                beam.line_to(Offset::new(lx + 4.0, ly - 58.0));
                let fade = (a.cos() * -1.0).max(0.0).min(1.0);
                book.fill(beam, alpha(Color::rgb(250, 246, 230), 0.05 + 0.05 * fade));
            }
            // The lamp itself — flares as the beam sweeps past the camera.
            let lamp_flare = ((sweep - std::f32::consts::PI).cos()).max(0.0).powi(6);
            book.circle(Offset::new(lx, ly - 58.0), 5.0, alpha(Color::rgb(255, 252, 240), 0.95));
            book.circle(Offset::new(lx, ly - 58.0), 12.0 + 26.0 * lamp_flare,
                alpha(Color::rgb(255, 248, 224), 0.10 + 0.22 * lamp_flare));

            // Foreground vignette.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.5), 0.8).with_dither().with_stops(&[
                    (0.0, alpha(Color::BLACK, 0.0)),
                    (1.0, alpha(Color::BLACK, 0.38)),
                ]),
            );
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(t));
    stack.into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(t: f32) -> WidgetNode {
    let film = t * SECONDS;
    let frame_i = (t * 255.0).round();
    let lines = [
        "LONGPLAY · THE ENDURANCE AXIS · 256 FRAMES".to_string(),
        format!("frame {frame_i}/255 · film {film:.0} s · no reset, no state"),
        format!("closed-form scene: the ONLY drift possible is the renderer"),
        format!("rss + build/raster sampled every 16 frames → metrics"),
        format!("beam period 4.3 s · tide 1 cycle · moon 1 set · 1 crossing"),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 566.0;
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

    // The instrument: the run's own progress, as a strip of 32 ticks (one
    // per 8 frames) — the endurance plate keeps its own clock visible.
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
            for k in 0..32 {
                let x = 12.0 + k as f32 * ((P_W - 24.0) / 31.0);
                let lit = (k as f32 / 31.0) <= t;
                book.rect(
                    Rect::new(x, 14.0, x + 5.0, 26.0),
                    alpha(if lit { VIOLET_SOFT } else { FAINT }, if lit { 0.5 } else { 0.22 }),
                );
            }
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

// ── The endurance instruments ───────────────────────────────────────────────

/// The per-frame series hook: RSS from the kernel, plus the measured
/// build/raster split, recorded every 16 frames (16 lines, one per
/// 16-frame block — a curve, not a log).
pub fn frame_hook(receipt: &mut Receipt, i: usize, build_ms: f64, render_ms: f64) {
    if i % 16 == 0 {
        let rss = crate::film_lib::rss_kib();
        receipt.series.push(format!(
            "f{i:03} rss={rss}KiB build={build_ms:.1}ms raster={render_ms:.1}ms"
        ));
    }
}

/// The probe — the last frame's facts, read out of the buffer: the lighthouse
/// lamp is lit, the sea is not flat black, the vignette holds.
pub fn probe(img: &image::RgbaImage) -> Vec<String> {
    let mut out = Vec::new();
    // The lamp.
    let (lx, ly) = LH;
    let mut lamp_peak = 0u8;
    for dy in -3..=3 {
        for dx in -3..=3 {
            let p = img.get_pixel((lx as i32 + dx) as u32, (ly as i32 - 58 + dy) as u32);
            lamp_peak = lamp_peak.max(p[0]);
        }
    }
    out.push(format!("lamp peak channel {lamp_peak}/255 (the light never sleeps)"));
    // The sea's mean — not flat black.
    let mut sum = 0u64;
    let mut n = 0u64;
    for y in (440..560).step_by(4) {
        for x in (600..900).step_by(8) {
            let p = img.get_pixel(x, y);
            sum += p[1] as u64;
            n += 1;
        }
    }
    out.push(format!("sea mean G {}/255", sum / n.max(1)));
    // The vignette: corner vs centre.
    let c = img.get_pixel(640, 340);
    let k = img.get_pixel(8, 8);
    out.push(format!(
        "vignette centre G {} vs corner G {} (Δ{})",
        c[1], k[1], c[1].saturating_sub(k[1])
    ));
    out
}
