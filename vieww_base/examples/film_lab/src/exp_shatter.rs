//! exp_shatter — *X-08: the card breaks, and every shard carries a piece of
//! what it was.* The U-11 + U-14 plate.
//!
//! A frosted card holding the wordmark shatters at the strike: **64 convex
//! shards**, each one a **window onto the original artwork** — the
//! transform-outside / clip-inside pattern the upgrade file recorded because
//! nothing in the tree demonstrates it:
//!
//! ```text
//! book.transformed(flight, |g| {            // outside: the shard's flight
//!     g.layer(alpha, 0.0, Some(shard), |h| { // inside: the window
//!         source(h, size);                  // the artwork, again
//!     });
//! });
//! ```
//!
//! The clip lives in the group's pre-transform space, so the transform
//! carries the window *and* what it shows — a shard is the artwork, flying.
//! No redraw could show a different piece; the geometry guarantees it.
//!
//! The shards travelling fastest — the ones the upgrade file called "the one
//! thing that says render rather than photograph" — render through
//! [`Filtered::with_blur_angle`](vieww_widget::Filtered), the **U-14
//! widget-side door**: a directional Gaussian along the velocity vector,
//! edge-clamped (U-15) so it does not darken at its own buffer bounds. The
//! slow shards stay crisp; the receipt prints both counts, the mean speed,
//! and the sigma actually used — measured, not typed.
//!
//! The shards are real Voronoi cells — Sutherland–Hodgman half-plane clips
//! of the card rect (the U-13 note's "no boolean ops" workaround, ~25 lines),
//! computed once, deterministically, at first frame.

use std::sync::OnceLock;

use vieww_foundation::{
    Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle, Transform,
};
use vieww_widget::prelude::*;
use vieww_widget::{Filtered, Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, ease_out_cubic, mix, spring_out, tint, BG_DEEP, CANVAS, FAINT, INK, MUTED, Rng,
    VIOLET, VIOLET_DEEP, VIOLET_SOFT,
};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 10.0;

// ── The card ────────────────────────────────────────────────────────────────

/// The card, in card-local coordinates — the artwork every shard windows.
const CARD_X: f32 = 340.0;
const CARD_Y: f32 = 208.0;
const CARD_W: f32 = 600.0;
const CARD_H: f32 = 330.0;
const CARD_R: f32 = 18.0;

/// The strike, in film-t.
const STRIKE: f32 = 0.22;

/// Shard flight span, in film-t.
const FLIGHT_SPAN: f32 = 0.50;

// ── The shard field — built once, deterministic ─────────────────────────────

/// One shard: its clip polygon (card-local), centroid, and flight constants.
struct Shard {
    /// The convex window, in card-local coordinates.
    cell: Path,
    /// Centroid of the cell, card-local.
    center: Offset,
    /// Launch delay (0..1 of the flight span) — shards near the crack go first.
    delay: f32,
    /// Launch velocity, px per flight-span.
    vel: (f32, f32),
    /// Spin, radians over the flight.
    spin: f32,
    /// How fast this shard is, percentile 0..1 — the fastest get U-14 blur.
    speed_rank: f32,
}

struct Field {
    shards: Vec<Shard>,
    /// The crack origin, card-local.
    crack: Offset,
}

impl Field {
    fn build() -> Self {
        let rect = (CARD_W, CARD_H);
        let mut rng = Rng::new(0x5111);

        // The crack origin — near center, slightly low-left, jittered.
        let crack = Offset::new(CARD_W * (0.42 + rng.sym() * 0.04), CARD_H * (0.55 + rng.sym() * 0.04));

        // Seeds: 64, jittered grid so cells vary in size but stay coherent.
        let n = 64;
        let mut seeds = Vec::with_capacity(n);
        for i in 0..n {
            let gx = (i % 8) as f32 + 0.5 + rng.sym() * 0.34;
            let gy = (i / 8) as f32 + 0.5 + rng.sym() * 0.34;
            seeds.push(Offset::new(gx / 8.0 * CARD_W, gy / 8.0 * CARD_H));
        }

        // Voronoi cells by half-plane clipping (Sutherland–Hodgman on a
        // rectangle — each result stays convex, U-13's workaround).
        let mut shards = Vec::with_capacity(n);
        for (i, &s) in seeds.iter().enumerate() {
            let mut poly: Vec<(f32, f32)> = vec![
                (0.0, 0.0),
                (rect.0, 0.0),
                (rect.0, rect.1),
                (0.0, rect.1),
            ];
            // Clip against a subsample of the other seeds: full 63-way is
            // O(n²) fine here, but the nearest 24 dominate a cell's shape.
            let mut others: Vec<&Offset> = seeds
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, p)| p)
                .collect();
            others.sort_by(|a, b| {
                let da = (a.dx - s.dx).powi(2) + (a.dy - s.dy).powi(2);
                let db = (b.dx - s.dx).powi(2) + (b.dy - s.dy).powi(2);
                da.partial_cmp(&db).unwrap()
            });
            for &o in others.iter().take(24) {
                // Bisector of s and o: keep the s side.
                // n·(p - m) >= 0 where m = midpoint, n = s - o.
                let nx = s.dx - o.dx;
                let ny = s.dy - o.dy;
                let mx = (s.dx + o.dx) * 0.5;
                let my = (s.dy + o.dy) * 0.5;
                let mut out = Vec::with_capacity(poly.len());
                let len = poly.len();
                for k in 0..len {
                    let p = poly[k];
                    let q = poly[(k + 1) % len];
                    let dp = nx * (p.0 - mx) + ny * (p.1 - my);
                    let dq = nx * (q.0 - mx) + ny * (q.1 - my);
                    match (dp >= 0.0, dq >= 0.0) {
                        (true, true) => out.push(q),
                        (true, false) => {
                            let t = dp / (dp - dq);
                            out.push((p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t));
                        }
                        (false, true) => {
                            let t = dp / (dp - dq);
                            out.push((p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t));
                            out.push(q);
                        }
                        (false, false) => {}
                    }
                }
                poly = out;
                if poly.len() < 3 {
                    break;
                }
            }
            if poly.len() < 3 {
                continue;
            }

            // The cell as a Path, and its centroid.
            let mut cell = Path::new();
            cell.move_to(Offset::new(poly[0].0, poly[0].1));
            for p in &poly[1..] {
                cell.line_to(Offset::new(p.0, p.1));
            }
            cell.close();
            let (mut cx, mut cy) = (0.0f32, 0.0f32);
            for p in &poly {
                cx += p.0;
                cy += p.1;
            }
            let center = Offset::new(cx / poly.len() as f32, cy / poly.len() as f32);

            // Flight: outward from the crack, distance-weighted delay, spin.
            let mut dx = center.dx - crack.dx;
            let mut dy = center.dy - crack.dy - 14.0;
            let d = (dx * dx + dy * dy).sqrt().max(1.0);
            dx /= d;
            dy /= d;
            let speed = (140.0 + rng.f01() * 260.0) * (0.65 + 0.6 * (d / 300.0).min(1.0));
            let vel = (dx * speed + rng.sym() * 26.0, dy * speed + rng.sym() * 26.0);
            let delay = ((d / 340.0).min(1.0)) * 0.42 + rng.f01() * 0.08;
            let spin = rng.sym() * (0.9 + 1.6 * rng.f01());

            shards.push(Shard {
                cell,
                center,
                delay,
                vel,
                spin,
                speed_rank: 0.0,
            });
        }

        // Rank by launch speed — the top decile carries the motion blur.
        let mut speeds: Vec<(usize, f32)> = shards
            .iter()
            .enumerate()
            .map(|(i, s)| (i, (s.vel.0 * s.vel.0 + s.vel.1 * s.vel.1).sqrt()))
            .collect();
        speeds.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        let total = speeds.len().max(1);
        for (rank, &(i, _)) in speeds.iter().enumerate() {
            shards[i].speed_rank = rank as f32 / total as f32;
        }

        Field { shards, crack }
    }

    fn get() -> &'static Field {
        static FIELD: OnceLock<Field> = OnceLock::new();
        FIELD.get_or_init(Field::build)
    }
}

// ── The source artwork — what every shard is a window onto ──────────────────

/// The card's content, in card-local coordinates. Fifteen shapes; each shard
/// redraws this clipped to itself — the honest price of "no backdrop in a
/// painter" (U-11), paid in shapes and printed in the receipt.
fn source(book: &mut Sketchbook) {
    // Frosted body.
    book.rrect(
        Rect::new(0.0, 0.0, CARD_W, CARD_H),
        CARD_R,
        Gradient::vertical().with_dither().with_stops(&[
            (0.0, alpha(VIOLET_DEEP, 0.55)),
            (0.55, alpha(mix(VIOLET_DEEP, BG_DEEP, 0.6), 0.38)),
            (1.0, alpha(BG_DEEP, 0.5)),
        ]),
    );
    book.rrect(
        Rect::new(0.0, 0.0, CARD_W, 72.0),
        CARD_R,
        alpha(VIOLET_DEEP, 0.30),
    );
    book.stroke_rrect(
        Rect::new(0.0, 0.0, CARD_W, CARD_H),
        CARD_R,
        alpha(Color::WHITE, 0.16),
        1.4,
    );

    // The wordmark — the thing that breaks.
    book.rect(
        Rect::new(40.0, 118.0, 230.0, 128.0),
        Gradient::horizontal().with_dither().with_stops(&[
            (0.0, alpha(Color::WHITE, 0.92)),
            (0.5, alpha(tint(VIOLET_SOFT, 0.4), 0.9)),
            (1.0, alpha(Color::WHITE, 0.85)),
        ]),
    );

    // A stat row.
    book.circle(Offset::new(52.0, 52.0), 12.0, alpha(VIOLET_SOFT, 0.8));
    book.rrect(Rect::new(76.0, 44.0, 226.0, 60.0), 8.0, alpha(Color::WHITE, 0.10));
    book.rrect(Rect::new(300.0, 44.0, 470.0, 60.0), 8.0, alpha(Color::WHITE, 0.06));

    // A sparkline.
    let mut spark = Path::new();
    let pts: [(f32, f32); 11] = [
        (40.0, 292.0), (88.0, 268.0), (136.0, 286.0), (184.0, 232.0), (232.0, 250.0),
        (280.0, 208.0), (328.0, 226.0), (376.0, 186.0), (424.0, 204.0), (472.0, 164.0),
        (520.0, 178.0),
    ];
    spark.move_to(Offset::new(pts[0].0, pts[0].1));
    for p in &pts[1..] {
        spark.line_to(Offset::new(p.0, p.1));
    }
    book.stroke(spark, alpha(VIOLET_SOFT, 0.85), 3.0);

    // A progress bar.
    book.rrect(Rect::new(40.0, 214.0, 320.0, 222.0), 4.0, alpha(Color::WHITE, 0.10));
    book.rrect(Rect::new(40.0, 214.0, 232.0, 222.0), 4.0, alpha(VIOLET, 0.9));

    // Rising blocks — the chart.
    for (i, h) in [34.0, 58.0, 44.0, 78.0, 66.0, 96.0].iter().enumerate() {
        let x = 360.0 + i as f32 * 34.0;
        book.rrect(
            Rect::new(x, 300.0 - h, x + 24.0, 300.0),
            4.0,
            alpha(mix(VIOLET, VIOLET_SOFT, i as f32 / 6.0), 0.55),
        );
    }
}

// ── The strike and the flight ───────────────────────────────────────────────

/// The shard's flight at film-t `t`: (translate dx, dy, rotation, alive frac).
fn flight(s: &Shard, t: f32) -> (f32, f32, f32, f32) {
    let tau = clamp01((t - STRIKE - s.delay * 0.18) / FLIGHT_SPAN);
    if tau <= 0.0 {
        return (0.0, 0.0, 0.0, 0.0);
    }
    // Ease out hard: explosive at the strike, drifting by the end.
    let e = ease_out_cubic(tau);
    let dx = s.vel.0 * e;
    let dy = s.vel.1 * e + 150.0 * tau * tau; // gravity
    let rot = s.spin * spring_out(tau, 3.4, 0.9);
    let fade = 1.0 - clamp01((tau - 0.72) / 0.28) * 0.55;
    (dx, dy, rot, fade)
}

/// The speed at film-t, px per frame (16 frames → t step = 1/15) — what the
/// blur sigma is derived from, and what the receipt prints.
fn speed_now(s: &Shard, t: f32) -> f32 {
    let tau = clamp01((t - STRIKE - s.delay * 0.18) / FLIGHT_SPAN);
    if tau <= 0.0 || tau >= 1.0 {
        return 0.0;
    }
    // |d(pos)/d(t)| in px per unit t; /15 for px per rendered frame.
    let e0 = ease_out_cubic((tau - 0.02).max(0.0));
    let e1 = ease_out_cubic((tau + 0.02).min(1.0));
    let vx = (s.vel.0 * (e1 - e0)) / 0.04;
    let vy = (s.vel.1 * (e1 - e0) + 150.0 * ((tau + 0.02).powi(2) - (tau - 0.02).powi(2)) / 0.04) / 1.0;
    ((vx * vx + vy * vy).sqrt() / 15.0).min(60.0)
}

/// Blur sigma for a fast shard from its speed — sigma ≈ a sixth of the
/// per-frame travel, capped where the box passes get long.
fn sigma_for(speed: f32) -> f32 {
    (speed * 0.16).min(9.0)
}

// ── The board ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let struck = t >= STRIKE;

    // The background + the intact card + the slow shards: one painter.
    let board = Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(8, 8, 11)),
                    (0.6, BG_DEEP),
                    (1.0, Color::rgb(12, 11, 18)),
                ]),
            );

            // Quiet stars.
            let mut rng = Rng::new(0x5112);
            for _ in 0..70 {
                let x = rng.f01() * w;
                let y = rng.f01() * h;
                let r = 0.4 + rng.f01() * 0.8;
                book.circle(Offset::new(x, y), r, alpha(Color::WHITE, 0.03 + 0.05 * rng.f01()));
            }

            let field = Field::get();

            if !struck {
                // ── Intact: the card floats, breathes, its tension grows ──
                let bob = (t * 7.0).sin() * 5.0;
                let tension = clamp01((t - 0.14) / 0.08);
                let wobble = (t * 46.0).sin() * 0.012 * tension;
                let flight = Transform::translate(Offset::new(0.0, bob))
                    .then(Transform::rotate_around(
                        Offset::new(CARD_X + CARD_W * 0.5, CARD_Y + CARD_H * 0.5),
                        wobble,
                    ));

                // The glow behind the card.
                book.layer(1.0, 26.0, None, |g| {
                    g.circle(
                        Offset::new(CARD_X + CARD_W * 0.5, CARD_Y + CARD_H * 0.55),
                        CARD_W * 0.62,
                        Gradient::radial_fill().with_dither().with_stops(&[
                            (0.0, alpha(VIOLET, 0.16 + 0.10 * tension)),
                            (1.0, alpha(VIOLET, 0.0)),
                        ]),
                    );
                });

                book.transformed(flight, |g| {
                    g.layer(1.0, 0.0, None, |h| {
                        h.transformed(
                            Transform::translate(Offset::new(CARD_X, CARD_Y)),
                            |k| source(k),
                        );
                    });
                });

                // The crack, forming: a jagged line that flickers brighter.
                if tension > 0.0 {
                    let c = field.crack;
                    let mut rng = Rng::new(0x5113);
                    let mut p = Path::new();
                    let mut cy = 0.0f32;
                    let mut cx = c.dx;
                    p.move_to(Offset::new(CARD_X + cx, CARD_Y + cy));
                    for _ in 0..7 {
                        cx += rng.sym() * 20.0;
                        cy += CARD_H / 7.0;
                        p.line_to(Offset::new(CARD_X + cx, CARD_Y + cy));
                    }
                    let flick = 0.5 + 0.5 * (t * 60.0).sin();
                    book.stroke(
                        p,
                        alpha(Color::WHITE, 0.25 + 0.5 * tension * flick),
                        1.0 + 1.2 * tension,
                    );
                }
            } else {
                // ── Struck: every shard is a flying window (U-11) ──
                let since = (t - STRIKE) / 0.06;
                let flash = (1.0 - clamp01(since)).powi(2);

                // The strike flash — a Plus-blended bloom at the crack.
                if flash > 0.01 {
                    let c = field.crack;
                    book.blended_layer(1.0, 18.0, vieww_foundation::BlendMode::Plus, None, |g| {
                        g.circle(
                            Offset::new(CARD_X + c.dx, CARD_Y + c.dy),
                            60.0 + 300.0 * (1.0 - flash),
                            Gradient::radial_fill().with_dither().with_stops(&[
                                (0.0, alpha(Color::WHITE, 0.7 * flash)),
                                (0.4, alpha(VIOLET_SOFT, 0.4 * flash)),
                                (1.0, alpha(VIOLET, 0.0)),
                            ]),
                        );
                    });
                }

                // The slow shards (speed_rank below the blur line), crisp.
                for s in &field.shards {
                    if s.speed_rank >= 0.9 {
                        continue; // the fast ones render as widgets below
                    }
                    let (dx, dy, rot, fade) = flight(s, t);
                    if fade <= 0.0 {
                        continue;
                    }
                    let center = Offset::new(CARD_X + s.center.dx + dx, CARD_Y + s.center.dy + dy);
                    let flight_t = Transform::rotate_around(center, rot);
                    book.transformed(flight_t, |g| {
                        g.layer(fade, 0.0, Some(s.cell.clone()), |h| {
                            h.transformed(
                                Transform::translate(Offset::new(CARD_X, CARD_Y)),
                                |k| source(k),
                            );
                        });
                    });
                    // The shard's rim — the fracture surface catching light.
                    let rim = s.cell.transformed(
                        Transform::translate(Offset::new(CARD_X, CARD_Y)).then(flight_t),
                    );
                    book.stroke(rim, alpha(tint(VIOLET_SOFT, 0.5), 0.35 * fade), 0.8);
                }

                // Dust — the fracture's fines, Plus-blended, one blurred group.
                let dust_a = clamp01((t - STRIKE) / 0.10) * (1.0 - clamp01((t - 0.68) / 0.32));
                if dust_a > 0.01 {
                    book.blended_layer(1.0, 3.0, vieww_foundation::BlendMode::Plus, None, |g| {
                        let mut rng = Rng::new(0x5114);
                        let spread = ease_out_cubic(clamp01((t - STRIKE) / FLIGHT_SPAN));
                        for _ in 0..90 {
                            let a = rng.f01() * std::f32::consts::TAU;
                            let r = 30.0 + rng.f01() * 320.0 * spread;
                            let x = CARD_X + field.crack.dx + a.cos() * r;
                            let y = CARD_Y + field.crack.dy + a.sin() * r * 0.7 + 40.0 * spread;
                            let sz = 0.6 + rng.f01() * 1.4;
                            g.circle(
                                Offset::new(x, y),
                                sz,
                                alpha(tint(VIOLET_SOFT, 0.5), dust_a * (0.2 + 0.5 * rng.f01())),
                            );
                        }
                    });
                }
            }

            // The vignette.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.5), 0.85).with_dither().with_stops(&[
                    (0.55, alpha(Color::BLACK, 0.0)),
                    (1.0, alpha(Color::BLACK, 0.42)),
                ]),
            );
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));

    // ── The fast shards: the U-14 door — Filtered::with_blur_angle ──
    if struck {
        let field = Field::get();
        for s in &field.shards {
            if s.speed_rank < 0.9 {
                continue;
            }
            let (dx, dy, rot, fade) = flight(s, t);
            if fade <= 0.0 {
                continue;
            }
            let speed = speed_now(s, t);
            let sigma = sigma_for(speed);
            let angle = s.vel.1.atan2(s.vel.0);
            let center = Offset::new(CARD_X + s.center.dx + dx, CARD_Y + s.center.dy + dy);
            let flight_t = Transform::rotate_around(center, rot);
            let cell = s.cell.clone();

            let shard_widget = Painting::sized(
                CANVAS,
                PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                    // `Fn` (not `FnOnce`): the window is cloned per call.
                    book.transformed(flight_t, |g| {
                        g.layer(fade, 0.0, Some(cell.clone()), |h| {
                            h.transformed(
                                Transform::translate(Offset::new(CARD_X, CARD_Y)),
                                |k| source(k),
                            );
                        });
                    });
                }),
            );

            // The blur rides the velocity vector — the whole point (U-14).
            let blurred = Filtered::new()
                .with_blur(sigma)
                .with_blur_angle(angle)
                .child(shard_widget);

            stack = stack.push(Positioned::fill().child(blurred));
        }
    }

    // ── The receipt panel ──
    stack = stack.push(receipt_panel(t));
    stack.into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(t: f32) -> WidgetNode {
    let field = Field::get();
    let struck = t >= STRIKE;
    let fast = field
        .shards
        .iter()
        .filter(|s| s.speed_rank >= 0.9)
        .count();
    let mean_speed = if struck {
        let mut acc = 0.0;
        let mut n = 0;
        for s in &field.shards {
            let v = speed_now(s, t);
            if v > 0.0 {
                acc += v;
                n += 1;
            }
        }
        if n > 0 { acc / n as f32 } else { 0.0 }
    } else {
        0.0
    };
    // Sigma range across the blurred decile, this frame.
    let (sigma_min, sigma_max) = if struck {
        let sigmas: Vec<f32> = field
            .shards
            .iter()
            .filter(|s| s.speed_rank >= 0.9)
            .map(|s| sigma_for(speed_now(s, t)))
            .collect();
        (
            sigmas.iter().cloned().fold(f32::MAX, f32::min).max(0.1),
            sigmas.iter().cloned().fold(0.0, f32::max).max(0.1),
        )
    } else {
        (0.0, 0.0)
    };

    let P_X = 42.0;
    let P_Y = 42.0;
    let P_W = 330.0;

    let lines = [
        "X-08 · SHATTER · THE WINDOW PATTERN (U-11)".to_string(),
        format!("shards {} · cells ≥3-gon {} · crack ({:.0}, {:.0})",
            field.shards.len(),
            field.shards.len(),
            field.crack.dx, field.crack.dy),
        format!("fast {} through U-14 blur · mean {:.1} px/frame", fast, mean_speed),
        if struck {
            format!("sigma {:.1}..{:.1} along atan2(v) · edge-clamped (U-15)", sigma_min, sigma_max)
        } else {
            format!("intact · strike at t={:.2}", STRIKE)
        },
        "source redrawn per shard — the honest U-11 price".to_string(),
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
                        .letter_spacing(2.0)
                        .color(alpha(FAINT, 0.95)),
                ),
            ),
    );
    for (i, line) in lines.iter().enumerate().skip(1) {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + 20.0 + i as f32 * 16.0)
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

    // The receipt's instrument: a speed histogram of the shard field, live.
    let hist = Painting::sized(
        Size::new(P_W, 54.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            book.rrect(
                Rect::new(0.0, 0.0, P_W, 54.0),
                10.0,
                alpha(Color::rgb(16, 16, 21), 0.88),
            );
            book.stroke_rrect(
                Rect::new(0.0, 0.0, P_W, 54.0),
                10.0,
                alpha(Color::WHITE, 0.08),
                1.0,
            );
            // Bars: speed distribution, sorted ranks, the blurred decile violet.
            let n = field.shards.len().max(1);
            let bw = (P_W - 24.0) / n as f32;
            for (i, s) in field.shards.iter().enumerate() {
                let v = if struck { speed_now(s, t).min(60.0) } else { 0.0 };
                let bh = 2.0 + (v / 60.0) * 38.0;
                let color = if s.speed_rank >= 0.9 {
                    alpha(VIOLET_SOFT, 0.9)
                } else {
                    alpha(INK, 0.55)
                };
                book.rect(
                    Rect::new(12.0 + i as f32 * bw, 46.0 - bh, 12.0 + i as f32 * bw + bw.max(1.2), 46.0),
                    color,
                );
            }
            book.line(
                Offset::new(12.0, 46.0),
                Offset::new(P_W - 12.0, 46.0),
                alpha(Color::WHITE, 0.14),
                1.0,
            );
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 104.0)
            .width(P_W)
            .height(54.0)
            .child(hist),
    );

    stack.into()
}
