//! exp_shadowplay — *the blur-economy axis.* The lantern river, at the guard.
//!
//! `ghosts` ran four blurred groups and called it economy; this plate
//! spends on purpose: **thirty filtered layers in one frame** — 27 paper
//! lanterns each carrying its own glow group, two river-reflection groups
//! (near and far, smeared along the water), one hanging mist — against a
//! U-06 guard set at 32. Two to spare, on purpose: the guard's edge,
//! measured, not grazed by accident.
//!
//! The receipt the axis wants is the *price*: what do 30 small blurred
//! groups cost against 4 big ones? `ghosts`' 4+1 groups ran 97 ms with its
//! bloom; this plate's number prints beside them in metrics.txt, and the
//! gauge on the panel shows the count live against the red guard line —
//! the only plate in the library whose instrument is a fuel gauge.
//!
//! The scene: a night river, lanterns rising from the water, their light
//! doubled in the reflection, reeds on the near bank, a moon low. The
//! lanterns rise at their own speeds, the near ones big and slow, the far
//! ones small and quick.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Filtered, Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, mix, Rng, AMBER, BG_DEEP, FAINT, INK, MUTED, RED,
    VIOLET_SOFT, CYAN_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

/// The lantern count — 27 glows + 2 reflections + 1 mist = 30 of 32.
const LANTERNS: usize = 27;

/// The guard the harness sets — printed as the gauge's red line.
const GUARD: usize = 32;

/// The waterline.
const WATER: f32 = 520.0;

/// One lantern's whole life, a pure function of its index.
struct Lantern {
    x0: f32,
    rise: f32,    // screen heights per plate
    speed: f32,   // 0..1 of the plate spent rising
    sway: f32,
    sway_hz: f32,
    size: f32,
    depth: f32, // 0 far — 1 near
    hue: f32,   // 0 amber — 1 violet
    phase: f32,
}

#[must_use]
fn lanterns() -> Vec<Lantern> {
    let mut rng = Rng::new(0x1A47);
    let mut out = Vec::with_capacity(LANTERNS);
    for i in 0..LANTERNS {
        let depth = rng.f01();
        out.push(Lantern {
            x0: 70.0 + rng.f01() * 1140.0,
            rise: 430.0 + rng.f01() * 60.0,
            speed: 0.35 + rng.f01() * 0.6,
            sway: 12.0 + rng.f01() * 26.0,
            sway_hz: 0.4 + rng.f01() * 0.8,
            size: 14.0 + depth * 26.0,
            depth,
            hue: (i as f32 / LANTERNS as f32) * 0.7,
            phase: rng.f01(),
        });
    }
    out
}

/// A lantern's position at t (bottom of its rise at its speed's start).
#[must_use]
fn lan_pos(l: &Lantern, t: f32) -> (f32, f32) {
    let u = clamp01((t - (1.0 - l.speed)) / l.speed);
    let eased = u; // linear rise — the river carries them at one pace
    let film = t * SECONDS;
    let x = l.x0 + (film * l.sway_hz * std::f32::consts::TAU + l.phase * 9.0).sin() * l.sway;
    let y = WATER - 26.0 - eased * l.rise;
    (x, y)
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let lans = lanterns();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The night — river air, a moon low behind haze.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(5, 6, 11)),
                    (0.5, Color::rgb(9, 10, 18)),
                    (0.72, BG_DEEP),
                    (1.0, Color::rgb(4, 5, 8)),
                ]),
            );
            // Stars, few and faint.
            let mut rng = Rng::new(0x57A2);
            for _ in 0..46 {
                let sx = rng.f01() * w;
                let sy = rng.f01() * 300.0;
                let r = 0.6 + rng.f01() * 1.1;
                book.circle(Offset::new(sx, sy), r, alpha(INK, 0.14 + rng.f01() * 0.22));
            }
            // The moon.
            book.circle(Offset::new(214.0, 122.0), 46.0, alpha(Color::rgb(228, 226, 214), 0.14));
            book.circle(Offset::new(214.0, 122.0), 26.0, alpha(Color::rgb(233, 231, 220), 0.9));
            book.circle(Offset::new(206.0, 116.0), 5.0, alpha(Color::rgb(210, 206, 195), 0.5));
            book.circle(Offset::new(222.0, 130.0), 3.6, alpha(Color::rgb(210, 206, 195), 0.4));

            // The far bank — a low dark treeline.
            let mut bank = Path::new();
            bank.move_to(Offset::new(0.0, WATER - 44.0));
            let mut rng = Rng::new(0xBA7C);
            for k in 0..22 {
                let x = k as f32 * 60.0;
                let bumps = rng.f01() * 22.0;
                bank.cubic_to(
                    Offset::new(x + 14.0, WATER - 46.0 - bumps),
                    Offset::new(x + 40.0, WATER - 40.0 - bumps * 0.7),
                    Offset::new(x + 60.0, WATER - 42.0),
                );
            }
            bank.line_to(Offset::new(w, WATER));
            bank.line_to(Offset::new(0.0, WATER));
            book.fill(bank, alpha(Color::rgb(6, 7, 10), 1.0));

            // The river — dark, with the moon's lane.
            book.rect(
                Rect::new(0.0, WATER, w, h - WATER),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(10, 13, 20)),
                    (1.0, Color::rgb(4, 5, 9)),
                ]),
            );
            for row in 0..8 {
                let yy = WATER + 8.0 + row as f32 * 24.0;
                book.rect(
                    Rect::new(150.0 - row as f32 * 8.0, yy, 270.0 + row as f32 * 10.0, yy + 1.3),
                    alpha(Color::rgb(226, 224, 212), (0.16 - row as f32 * 0.016).max(0.03)),
                );
            }

            // The near reeds — the bank's silhouette, drawn once, crisp,
            // with pale seed heads so the bank reads against the dark water.
            let mut rng = Rng::new(0x4EED);
            for _ in 0..44 {
                let rx = rng.f01() * w;
                let rh = 30.0 + rng.f01() * 62.0;
                let lean = rng.sym() * 18.0;
                let mut reed = Path::new();
                reed.move_to(Offset::new(rx, h));
                let ctl = Offset::new(rx + lean * 0.4, h - rh * 0.6);
                reed.cubic_to(ctl, ctl, Offset::new(rx + lean, h - rh));
                book.stroke(reed, alpha(Color::rgb(7, 8, 11), 0.95), 1.7);
                book.circle(
                    Offset::new(rx + lean, h - rh),
                    1.8 + rng.f01() * 1.4,
                    alpha(Color::rgb(122, 108, 84), 0.5),
                );
            }
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));

    // ── The glow groups — one per lantern, the budget spent ──
    // Depth decides σ: near lanterns bloom wider.
    let mut glow_positions: Vec<(f32, f32, f32)> = Vec::with_capacity(LANTERNS);
    for l in &lans {
        let (x, y) = lan_pos(l, t);
        glow_positions.push((x, y, l.size));
        let sigma = 1.6 + l.depth * 3.4;
        let (lsize, lhue) = (l.size, l.hue);
        let glow = Painting::sized(
            Size::new(lsize * 6.0, lsize * 6.0),
            PaintWith::new(move |g: &mut Sketchbook, sz: Size| {
                let c = sz.width * 0.5;
                let warm = mix(AMBER, VIOLET_SOFT, lhue);
                g.circle(
                    Offset::new(c, c),
                    lsize * 1.7,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(mix(Color::WHITE, warm, 0.3), 0.55)),
                        (0.45, alpha(warm, 0.30)),
                        (1.0, alpha(warm, 0.0)),
                    ]),
                );
                g.circle(Offset::new(c, c), lsize * 0.5, alpha(mix(Color::WHITE, warm, 0.2), 0.5));
            }),
        );
        stack = stack.push(
            Positioned::new()
                .left(x - lsize * 3.0)
                .top(y - lsize * 3.0)
                .width(lsize * 6.0)
                .height(lsize * 6.0)
                .child(Filtered::new().with_blur(sigma).child(glow)),
        );
    }

    // ── The reflection groups — near and far, two filtered layers for 27
    //    smeared lights (the economy this plate refuses, applied sideways).
    for (group_near, pass) in [(true, 0usize), (false, 1)] {
        let mut inner = Stack::new();
        let mut any = false;
        for l in &lans {
            if (l.depth > 0.5) != group_near {
                continue;
            }
            let (x, y) = lan_pos(l, t);
            let ry = WATER + (WATER - y) * 0.42 + 6.0;
            if ry > 716.0 {
                continue;
            }
            any = true;
            let (lsize, lhue) = (l.size, l.hue);
            let refl = Painting::sized(
                Size::new(lsize * 4.0, 56.0),
                PaintWith::new(move |g: &mut Sketchbook, sz: Size| {
                    let c = sz.width * 0.5;
                    let warm = mix(AMBER, VIOLET_SOFT, lhue);
                    // A vertical smear: three stacked, fading discs.
                    for k in 0..3 {
                        g.circle(
                            Offset::new(c, 12.0 + k as f32 * 16.0),
                            lsize * (0.8 - k as f32 * 0.16),
                            Gradient::radial_fill().with_dither().with_stops(&[
                                (0.0, alpha(warm, 0.34 - k as f32 * 0.09)),
                                (1.0, alpha(warm, 0.0)),
                            ]),
                        );
                    }
                }),
            );
            inner = inner.push(
                Positioned::new()
                    .left(x - lsize * 2.0)
                    .top(ry)
                    .width(lsize * 4.0)
                    .height(56.0)
                    .child(refl),
            );
        }
        if any {
            let rsig = if group_near { 3.2 } else { 1.8 };
            stack = stack.push(
                Positioned::fill().child(
                    Filtered::new()
                        .with_blur(rsig)
                        .with_blur_angle(std::f32::consts::FRAC_PI_2) // smeared down the water
                        .child(inner),
                ),
            );
        }
        let _ = pass;
    }

    // ── The mist — one hanging veil over the water, the 30th group. ──
    let mist = Painting::sized(
        Size::new(1280.0, 90.0),
        PaintWith::new(move |g: &mut Sketchbook, sz: Size| {
            g.rect(
                Rect::new(0.0, 0.0, sz.width, sz.height),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, alpha(CYAN_SOFT, 0.0)),
                    (0.5, alpha(CYAN_SOFT, 0.10)),
                    (1.0, alpha(VIOLET_SOFT, 0.0)),
                ]),
            );
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(0.0)
            .top(WATER - 44.0)
            .width(1280.0)
            .height(90.0)
            .child(Filtered::new().with_blur(6.0).child(mist)),
    );

    // ── The bodies — every lantern's paper, crisp, in ONE painting. ──
    let bodies = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |g: &mut Sketchbook, _sz: Size| {
            for (x, y, size) in &glow_positions {
                let bw = size * 0.72;
                let bh = size * 1.05;
                let warm = mix(AMBER, VIOLET_SOFT, 0.25);
                // The paper shell — translucent, lit from within.
                g.rrect(
                    Rect::new(x - bw * 0.5, y - bh * 0.5, x + bw * 0.5, y + bh * 0.5),
                    bw * 0.30,
                    alpha(mix(Color::rgb(255, 214, 150), warm, 0.4), 0.55),
                );
                // Ribs.
                for k in -1..=1 {
                    let rx = x + k as f32 * bw * 0.28;
                    g.line(
                        Offset::new(rx, y - bh * 0.44),
                        Offset::new(rx, y + bh * 0.44),
                        alpha(Color::rgb(120, 84, 44), 0.4),
                        0.9,
                    );
                }
                // Caps.
                g.rrect(
                    Rect::new(x - bw * 0.22, y - bh * 0.62, x + bw * 0.22, y - bh * 0.44),
                    2.0,
                    alpha(Color::rgb(60, 46, 30), 0.9),
                );
                g.rrect(
                    Rect::new(x - bw * 0.18, y + bh * 0.44, x + bw * 0.18, y + bh * 0.58),
                    2.0,
                    alpha(Color::rgb(60, 46, 30), 0.9),
                );
            }
        }),
    );
    stack = stack.push(Positioned::fill().child(bodies));

    stack.push(receipt_panel(t)).into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(t: f32) -> WidgetNode {
    let risen = lanterns().iter().filter(|l| t > (1.0 - l.speed)).count();
    let lines = [
        "SHADOWPLAY · THE BLUR ECONOMY AT THE GUARD".to_string(),
        format!("filtered groups built 30 (27 glows + 2 refl + 1 mist)"),
        format!("guard {} · measured count + ms → metrics", GUARD),
        format!("lanterns risen {risen}/{LANTERNS} · σ 1.6-5.0 by depth"),
        "the only plate whose instrument is a fuel gauge".to_string(),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 566.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(430.0)
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

    // The gauge — 32 slots, 30 spent, the guard's line in red.
    let gauge = Painting::sized(
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
            for k in 0..GUARD {
                let x = 12.0 + k as f32 * ((400.0 - 24.0) / (GUARD - 1) as f32);
                let spent = k < 30;
                if k == GUARD - 1 {
                    // The guard line — the last slot, red.
                    book.rect(Rect::new(x - 2.0, 9.0, x + 2.0, 31.0), alpha(RED, 0.85));
                } else {
                    book.rect(
                        Rect::new(x - 1.5, 14.0, x + 1.5, 26.0),
                        alpha(if spent { AMBER } else { FAINT }, if spent { 0.72 } else { 0.20 }),
                    );
                }
            }
            // The live needle at 30.
            let nx = 12.0 + 29.0 * ((400.0 - 24.0) / (GUARD - 1) as f32);
            book.line(Offset::new(nx, 6.0), Offset::new(nx, 34.0), alpha(INK, 0.9), 1.4);
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 84.0)
            .width(400.0)
            .height(40.0)
            .child(gauge),
    );

    stack.into()
}
