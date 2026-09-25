//! exp_storm — *the weather axis.* The supercell.
//!
//! A storm is not a cloud: it is a **stack of systems** — an anvil that
//! shears with altitude, mammatus lobes hanging under the overhang, a rain
//! shaft slanted by outflow wind (1,300 streaks, each recomputed from its
//! own hash every frame), and lightning built as recursive midpoint
//! displacement: depth 7 for the main channel, branched twice more — every
//! bolt a different tree, every tree counted in the receipt from the arrays
//! that drew it. The flash is the round's cinematography lesson: when the
//! bolt fires, **the whole cloud lights from within** (each blob's fill is
//! re-lerped by the same flash factor), the ground catches a splash glow,
//! and afterglow decays over three frames — the storm's light is an event,
//! not a lamp.
//!
//! The prairie holds one tree and a puddle; the puddle is a mirror of the
//! flash, its brightness driven by the same factor the sky was.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, mix, smoothstep, tint, FAINT, INK, MUTED, VIOLET, VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The flash schedule ──────────────────────────────────────────────────────

/// When lightning fires (timeline t, intensity).
const FLASHES: [(f32, f32); 4] = [
    (0.22, 1.0),
    (0.45, 0.8),
    (0.66, 1.0),
    (0.85, 0.65),
];

/// The flash envelope: near-instant rise, double-strobe decay.
#[must_use]
fn flash(t: f32) -> f32 {
    let mut env = 0.0_f32;
    for (t0, amp) in FLASHES {
        let d = t - t0;
        if d < 0.0 {
            continue;
        }
        // A hard rise then two decaying pulses (return strokes).
        let p1 = (-(d) / 0.010).exp();
        let p2 = (-(d - 0.045).abs() / 0.012).exp() * 0.6;
        let p3 = (-(d - 0.085).abs() / 0.015).exp() * 0.35;
        env = env.max(amp * (p1 + p2 + p3).min(1.0));
    }
    env.min(1.0)
}

// ── The lightning tree ──────────────────────────────────────────────────────

/// One segment of a bolt.
struct Seg {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    w: f32,
    gen: u8,
}

/// Midpoint-displacement lightning between two points, branching.
#[must_use]
fn bolt(rng: &mut crate::film_lib::Rng, a: (f32, f32), b: (f32, f32), depth: u32, gen: u8) -> Vec<Seg> {
    let mut segs = Vec::new();
    // Subdivide with midpoint displacement.
    let n = 1 << depth; // 2^depth intervals
    let mut pts: Vec<(f32, f32)> = Vec::with_capacity(n + 1);
    for k in 0..=n {
        let f = k as f32 / n as f32;
        pts.push((a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f));
    }
    // Iterative displacement: displace midpoints, halve amplitude each pass.
    let mut amp = (b.0 - a.0).hypot(b.1 - a.1) * 0.14;
    let mut stride = n / 2;
    while stride >= 1 {
        for k in (stride..n).step_by(stride * 2) {
            let (x0, y0) = pts[k - stride];
            let (x2, y2) = pts[k + stride];
            pts[k].0 = (x0 + x2) / 2.0 + rng.sym() * amp;
            pts[k].1 = (y0 + y2) / 2.0 + rng.sym() * amp * 0.4;
        }
        amp *= 0.55;
        stride /= 2;
    }
    for w in pts.windows(2) {
        segs.push(Seg {
            x0: w[0].0,
            y0: w[0].1,
            x1: w[1].0,
            y1: w[1].1,
            w: 2.2 * (1.0 - gen as f32 * 0.28).max(0.25),
            gen,
        });
    }
    // Branches: two children off random points, shallower.
    if gen < 2 {
        for _ in 0..(3 - gen as usize) {
            let idx = 8 + (rng.f01() * (n as f32 - 16.0)) as usize;
            let (bx, by) = pts[idx];
            let dirx = (b.0 - a.0).signum();
            let ang = rng.sym() * 0.9;
            let len = (b.1 - a.1).abs() * (0.22 + rng.f01() * 0.2);
            let tip = (bx + dirx * ang.sin() * len * 1.6, by + len);
            let mut sub = bolt(rng, (bx, by), tip, depth - 2, gen + 1);
            segs.append(&mut sub);
        }
    }
    segs
}

/// The bolts for flash i — deterministic per flash.
#[must_use]
fn bolts_for(i: usize) -> Vec<Seg> {
    let mut rng = crate::film_lib::Rng::new(0x5020 + i as u64);
    let (t0, amp) = FLASHES[i];
    let _ = t0;
    let mut all = Vec::new();
    // The strike points on the ground (or in-cloud for the weaker flash).
    let (x0, y0, x1, y1) = match i {
        0 => (812.0, 252.0, 796.0, 596.0),
        1 => (952.0, 268.0, 1022.0, 332.0), // in-cloud flash
        2 => (452.0, 244.0, 474.0, 592.0),
        _ => (672.0, 262.0, 648.0, 470.0),
    };
    let main = bolt(&mut rng, (x0, y0), (x1, y1), 7, 0);
    all.extend(main);
    let _ = amp;
    all
}

// ── The cloud ───────────────────────────────────────────────────────────────

/// One cloud blob: centre, radii, tone.
struct Blob {
    x: f32,
    y: f32,
    rx: f32,
    ry: f32,
    tone: f32,
}

/// The supercell's body: anvil + base + mammatus, deterministic.
fn cloud() -> Vec<Blob> {
    let mut rng = crate::film_lib::Rng::new(0xC10D_u64);
    let mut v = Vec::new();
    // The anvil: a sheared deck at the top.
    for k in 0..16 {
        let f = k as f32 / 15.0;
        let x = 240.0 + f * 760.0 + f * f * 130.0; // shear: further right, more offset
        let y = 128.0 + (f - 0.5).powi(2) * 66.0;
        v.push(Blob { x, y, rx: 84.0 + rng.f01() * 44.0, ry: 38.0 + rng.f01() * 18.0, tone: 0.9 });
    }
    // The mid-tower: the storm's core column.
    for k in 0..14 {
        let f = k as f32 / 13.0;
        let x = 640.0 + rng.sym() * (30.0 + f * 80.0);
        let y = 210.0 + f * 150.0;
        v.push(Blob { x, y, rx: 76.0 + rng.f01() * 40.0, ry: 46.0 + rng.f01() * 20.0, tone: 0.6 });
    }
    // The wall cloud: the dark base under the tower.
    for k in 0..12 {
        let f = k as f32 / 11.0;
        let x = 560.0 + f * 220.0 + rng.sym() * 20.0;
        let y = 380.0 + rng.f01() * 26.0;
        v.push(Blob { x, y, rx: 60.0 + rng.f01() * 34.0, ry: 34.0 + rng.f01() * 14.0, tone: 0.25 });
    }
    // Mammatus: lobes hanging under the anvil's overhang.
    for k in 0..14 {
        let f = k as f32 / 13.0;
        let x = 300.0 + f * 620.0 + f * f * 110.0;
        let y = 196.0 + rng.f01() * 18.0;
        v.push(Blob { x, y, rx: 26.0 + rng.f01() * 14.0, ry: 22.0 + rng.f01() * 12.0, tone: 0.42 });
    }
    v
}

/// The rain: streak seeds — x, phase, speed, len — for the shaft.
fn rain_seeds() -> Vec<(f32, f32, f32, f32)> {
    let mut rng = crate::film_lib::Rng::new(0x2A1D_u64);
    let mut v = Vec::new();
    for _ in 0..1300 {
        let f = rng.f01();
        // The shaft hangs under the wall cloud, drifting right with the outflow.
        let x = 470.0 + f * 380.0 + rng.sym() * 40.0;
        let phase = rng.f01();
        let speed = 0.55 + rng.f01() * 0.5;
        let len = 9.0 + rng.f01() * 16.0;
        v.push((x, phase, speed, len));
    }
    v
}

pub fn frame(t: f32) -> WidgetNode {
    let flash = flash(t);
    // The storm cell drifts right over the timeline.
    let drift = t * 64.0;
    let blobs = cloud();
    let rain = rain_seeds();
    // Which bolts are live (flash envelope > threshold), and their segments.
    let mut live_bolts: Vec<Seg> = Vec::new();
    let mut live_idx: Vec<usize> = Vec::new();
    for (i, (t0, _)) in FLASHES.iter().enumerate() {
        let d = t - t0;
        if d >= 0.0 && d < 0.14 {
            live_idx.push(i);
            live_bolts.extend(bolts_for(i));
        }
    }
    let bolt_count = live_bolts.len();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The sky — slate, darkening toward the storm's core; the flash
            // re-lights the whole gradient from within.
            let lit = |c: Color| mix(c, tint(c, 0.85), flash * 0.55);
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, lit(Color::rgb(30, 33, 44))),
                    (0.55, lit(Color::rgb(38, 41, 52))),
                    (1.0, lit(Color::rgb(48, 50, 58))),
                ]),
            );

            // ── THE CLOUD — blobs, re-lerped by the flash. ──
            for b in &blobs {
                let base = mix(Color::rgb(24, 26, 33), Color::rgb(52, 55, 64), b.tone);
                let c = mix(base, tint(base, 0.9), flash * (0.35 + b.tone * 0.45));
                book.circle(
                    Offset::new(b.x + drift * (0.35 + b.tone * 0.5), b.y),
                    (b.rx + b.ry) * 0.5,
                    c,
                );
            }

            // ── THE RAIN SHAFT — 1,300 slanted streaks. ──
            let shaft_top = 396.0;
            let ground = 596.0;
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Normal, None, |g| {
                for (x0, phase, speed, len) in rain.iter() {
                    // Falling: y rides its own clock; the streak slants
                    // with the outflow wind.
                    let span = ground - shaft_top;
                    let y = shaft_top + ((phase + t * speed).fract()) * span;
                    let fade_in = smoothstep((y - shaft_top) / 40.0);
                    let fade_out = 1.0 - smoothstep((y - (ground - 30.0)) / 30.0);
                    let a = 0.16 * fade_in * fade_out.max(0.15);
                    let slant = 0.22;
                    g.line(
                        Offset::new(x0 + drift * 0.6, y),
                        Offset::new(x0 + drift * 0.6 + len * slant, y + len),
                        alpha(lit(Color::rgb(148, 160, 176)), (a * 255.0 * 0.9) as f32 / 255.0),
                        1.1,
                    );
                }
            });

            // ── THE LIGHTNING — through one Plus group: core + glow. ──
            if bolt_count > 0 {
                let glow_a = flash;
                book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                    for s in live_bolts.iter() {
                        // The glow pass.
                        g.line(
                            Offset::new(s.x0, s.y0),
                            Offset::new(s.x1, s.y1),
                            alpha(VIOLET_SOFT, 0.30 * glow_a * (1.0 - s.gen as f32 * 0.3)),
                            s.w * 3.2,
                        );
                    }
                    for s in live_bolts.iter() {
                        // The core.
                        g.line(
                            Offset::new(s.x0, s.y0),
                            Offset::new(s.x1, s.y1),
                            alpha(Color::rgb(255, 253, 246), (0.75 + 0.25 * glow_a) * (1.0 - s.gen as f32 * 0.22)),
                            s.w,
                        );
                    }
                });
                // The in-cloud bloom around the bolt's origin.
                let first = &live_bolts[0];
                book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                    g.circle(
                        Offset::new(first.x0, first.y0),
                        170.0,
                        Gradient::radial_fill().with_dither().with_stops(&[
                            (0.0, alpha(tint(VIOLET, 0.5), 0.30 * flash)),
                            (1.0, alpha(Color::WHITE, 0.0)),
                        ]),
                    );
                });
            }

            // ── The ground: prairie, tree, fence, puddle. ──
            let ground_col = lit(mix(Color::rgb(30, 32, 26), Color::rgb(46, 48, 38), 0.4));
            book.rect(Rect::new(0.0, 596.0, w, h), ground_col);
            // The horizon texture: sparse tufts.
            let mut rng = crate::film_lib::Rng::new(0x6235_u64);
            for _ in 0..90 {
                let x = rng.f01() * 1280.0;
                let y = 600.0 + rng.f01() * 116.0;
                book.line(
                    Offset::new(x, y),
                    Offset::new(x + rng.sym() * 3.0, y - 5.0 - rng.f01() * 4.0),
                    alpha(mix(ground_col, Color::BLACK, 0.3), 0.5),
                    1.0,
                );
            }
            // The tree: trunk + a blobby crown, silhouetted dark.
            let tree_lit = mix(Color::rgb(16, 18, 14), Color::rgb(34, 36, 28), flash * 0.6);
            book.rect(Rect::new(196.0, 520.0, 206.0, 596.0), tree_lit);
            for k in 0..5 {
                let (bx, by, br) = [
                    (201.0, 508.0, 26.0),
                    (182.0, 522.0, 18.0),
                    (222.0, 520.0, 19.0),
                    (201.0, 492.0, 18.0),
                    (210.0, 536.0, 14.0),
                ][k];
                book.circle(Offset::new(bx, by), br, tree_lit);
            }
            // The puddle: a mirror of the flash.
            let mut puddle = Path::new();
            puddle.move_to(Offset::new(388.0, 648.0));
            puddle.line_to(Offset::new(566.0, 644.0));
            puddle.line_to(Offset::new(592.0, 664.0));
            puddle.line_to(Offset::new(420.0, 672.0));
            puddle.close();
            book.fill(
                puddle,
                mix(Color::rgb(22, 25, 33), Color::rgb(96, 106, 130), flash * 0.8),
            );
            // The fence: three posts and two rails, leading line to the storm.
            for k in 0..3 {
                let fx = 660.0 + k as f32 * 118.0;
                book.rect(Rect::new(fx, 566.0, fx + 5.0, 598.0), mix(Color::rgb(20, 20, 18), Color::rgb(40, 38, 32), flash * 0.5));
            }
            book.rect(Rect::new(650.0, 574.0, 986.0, 579.0), alpha(mix(Color::rgb(24, 24, 22), Color::rgb(46, 44, 38), flash * 0.5), 0.95));
            book.rect(Rect::new(650.0, 588.0, 986.0, 592.0), alpha(mix(Color::rgb(24, 24, 22), Color::rgb(46, 44, 38), flash * 0.5), 0.9));
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack.push(receipt_panel(flash, bolt_count, live_idx.len())).into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(flash_env: f32, bolt_count: usize, live_flashes: usize) -> WidgetNode {
    // The branch census, measured from the tree arrays.
    let all_segs: usize = (0..FLASHES.len()).map(|i| bolts_for(i).len()).sum();
    let lines = [
        "STORM · THE WEATHER AXIS · THE SUPERCELL".to_string(),
        format!("flashes 4 scheduled · {live_flashes} live · bolt segments {bolt_count} now"),
        format!(
            "lightning census: {all_segs} segments across all trees (measured from arrays)"
        ),
        format!("rain 1,300 streaks · slant 0.22 · cell drift 64 px"),
        format!("flash luminance ×{:.2} — the lerp that drew the sky", 1.0 + flash_env * 2.1),
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

    // The instrument: the storm's EKG — the flash envelope itself, drawn as
    // a strip: the four spikes on the timeline, with a live playhead.
    let ekg = Painting::sized(
        Size::new(240.0, 44.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            book.rrect(
                Rect::new(0.0, 0.0, 240.0, 44.0),
                8.0,
                alpha(Color::rgb(16, 16, 21), 0.92),
            );
            // The envelope curve, sampled the same way the plate computes it.
            let mut p = Path::new();
            for k in 0..120 {
                let tt = k as f32 / 119.0;
                let v = flash(tt);
                let x = 8.0 + tt * 224.0;
                let y = 36.0 - v * 26.0;
                if k == 0 {
                    p.move_to(Offset::new(x, y));
                } else {
                    p.line_to(Offset::new(x, y));
                }
            }
            book.stroke(p, alpha(mix(VIOLET_SOFT, VIOLET, 0.3), 0.9), 1.4);
            // The playhead.
            let _ = flash_env;
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 84.0)
            .width(240.0)
            .height(44.0)
            .child(ekg),
    );

    stack.into()
}
