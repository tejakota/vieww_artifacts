//! exp_ghosts — *E-24: motion persistence.* The U-14 + U-06 discipline plate.
//!
//! A courier flies a Lissajous figure — closed form, so every ghost's past
//! position is *evaluated*, not remembered: ghost `k` stands at `P(s - k·dt)`,
//! which makes the trail deterministic by construction and byte-identical
//! across runs without a single frame of carried state.
//!
//! The trail is drawn as **four direction buckets**, not fifty-six ghosts:
//! each bucket is one [`Filtered::with_blur_angle`] widget blurring along the
//! bucket's motion axis (U-14's directional blur riding the velocity vector),
//! with the ghosts inside a Plus-blended group (U-01). Four filtered layers
//! for the whole trail — plus the courier's bloom, five — is the U-06 lesson
//! in its economic form: the receipt prints the count next to the ghost
//! census, and the harness's `guard_filtered_layers_below` is the same number
//! asserting itself.
//!
//! The blur is edge-clamped (U-15), so a trail that runs near the frame edge
//! stays lit to its own rim instead of darkening — visible where the
//! lissajous swings wide.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Rect, Size, Sketchbook, TextStyle,
    Transform};
use vieww_widget::prelude::*;
use vieww_widget::{Filtered, Painting, PaintWith};

use crate::film_lib::{alpha, clamp01, mix, tint, BG_DEEP, CANVAS, FAINT, INK, MUTED, VIOLET,
    VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 10.0;

/// Ghosts in the trail.
const GHOSTS: usize = 56;

/// Ghost spacing, in film seconds of path-history.
const DT: f32 = 0.035;

/// Direction buckets — the filtered-layer count (U-06's number, kept honest).
const BUCKETS: usize = 4;

/// The blur sigma each bucket runs.
const SIGMA: f32 = 3.2;

/// The lissajous center and reach.
const CENTER: Offset = Offset::new(640.0, 350.0);

// ── The path — closed form, so the past is a function call ──────────────────

/// The courier's position at film-time `s` seconds. A 1:2 lissajous with a
/// phase skew — a figure that crosses itself twice per loop, so the trail
/// regularly meets its own future and past.
#[must_use]
pub fn path_at(s: f32) -> Offset {
    let a = s * 0.72;
    let x = CENTER.dx + 480.0 * a.sin();
    let y = CENTER.dy + 210.0 * (2.0 * a + std::f32::consts::FRAC_PI_3).sin();
    Offset::new(x, y)
}

/// The motion direction at `s`, radians — the blur axis for a ghost there.
#[must_use]
fn heading_at(s: f32) -> f32 {
    let eps = 0.012;
    let a = path_at(s - eps);
    let b = path_at(s + eps);
    (b.dy - a.dy).atan2(b.dx - a.dx)
}

/// Which bucket a heading falls in — BUCKETS wedges of the full turn.
#[must_use]
fn bucket_of(angle: f32) -> usize {
    let a = (angle.rem_euclid(std::f32::consts::TAU)) / std::f32::consts::TAU;
    ((a * BUCKETS as f32).floor() as usize).min(BUCKETS - 1)
}

/// The bucket's blur axis — its wedge center, in radians.
#[must_use]
fn bucket_angle(bucket: usize) -> f32 {
    (bucket as f32 + 0.5) / BUCKETS as f32 * std::f32::consts::TAU
}

// ── The board ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let fade_in = clamp01(t / 0.06);
    let s = t * SECONDS;

    // ── Layer 1: the ground, the stars, the path's own ghost chart ─────
    let ground = Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(7, 7, 10)),
                    (0.55, BG_DEEP),
                    (1.0, Color::rgb(9, 9, 13)),
                ]),
            );

            let mut rng = crate::film_lib::Rng::new(0x9C1);
            for _ in 0..70 {
                let x = rng.f01() * w;
                let y = rng.f01() * h;
                book.circle(Offset::new(x, y), 0.4 + rng.f01() * 0.7, alpha(Color::WHITE, 0.05));
            }

            // The full figure, faint — where the next ten seconds go.
            let mut chart = vieww_foundation::Path::new();
            let mut started = false;
            let mut ss = 0.0f32;
            while ss <= SECONDS {
                let p = path_at(ss);
                if started {
                    chart.line_to(p);
                } else {
                    chart.move_to(p);
                    started = true;
                }
                ss += 0.02;
            }
            book.stroke(chart, alpha(FAINT, 0.16 * fade_in), 1.0);
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(ground));

    // ── Layers 2..BUCKETS+1: the trail, one Filtered widget per bucket ──
    for bucket in 0..BUCKETS {
        let ghosts = Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                book.blended_layer(fade_in, 0.0, BlendMode::Plus, None, |g| {
                    for k in 1..=GHOSTS {
                        let sk = s - k as f32 * DT;
                        if sk < 0.0 {
                            continue;
                        }
                        if bucket_of(heading_at(sk)) != bucket {
                            continue;
                        }
                        let p = path_at(sk);
                        let age = k as f32 / GHOSTS as f32;
                        // The trail falls off as it ages — a power, not a
                        // line, so the head reads hot.
                        let a = (1.0 - age).powi(2) * 0.52;
                        let r = 7.5 * (1.0 - 0.55 * age);
                        let col = mix(VIOLET_SOFT, Color::WHITE, (1.0 - age) * 0.5);
                        g.circle(p, r, alpha(col, a));
                        g.circle(p, r * 2.4, alpha(VIOLET, a * 0.30));
                    }
                });
            }),
        );
        let blurred = Filtered::new()
            .with_blur(SIGMA)
            .with_blur_angle(bucket_angle(bucket))
            .child(ghosts);
        stack = stack.push(Positioned::fill().child(blurred));
    }

    // ── Layer BUCKETS+2: the courier itself — crisp on top of its past ──
    let here = path_at(s);
    let head = heading_at(s);
    let courier = Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            let body = Transform::rotate_around(here, head + std::f32::consts::FRAC_PI_2);

            // The freshest streak — unblurred, the read that blur *begins*.
            book.blended_layer(fade_in, 0.0, BlendMode::Plus, None, |g| {
                let mut tail = vieww_foundation::Path::new();
                let mut started = false;
                let mut ss = (s - 0.30).max(0.0);
                while ss <= s {
                    let p = path_at(ss);
                    if started {
                        tail.line_to(p);
                    } else {
                        tail.move_to(p);
                        started = true;
                    }
                    ss += 0.02;
                }
                if started {
                    g.stroke(tail, alpha(tint(VIOLET_SOFT, 0.5), 0.75 * fade_in), 3.2);
                }
            });

            // The pill: white core, violet rim, nose light.
            book.transformed(body, |g| {
                let core = Rect::new(here.dx - 16.0, here.dy - 7.0, here.dx + 16.0, here.dy + 7.0);
                g.rrect(core, 6.5, alpha(Color::WHITE, 0.96 * fade_in));
                g.stroke_rrect(core, 6.5, alpha(VIOLET_SOFT, 0.8 * fade_in), 1.4);
                g.circle(Offset::new(here.dx + 14.0, here.dy), 3.0, alpha(Color::WHITE, fade_in));
            });

            // The bloom riding the courier — the trail's one extra filtered
            // layer, counted in the receipt's claim.
            book.layer(1.0, 14.0, None, |g| {
                g.circle(
                    here,
                    34.0,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(tint(Color::WHITE, 0.1), 0.55 * fade_in)),
                        (0.5, alpha(VIOLET_SOFT, 0.30 * fade_in)),
                        (1.0, alpha(VIOLET, 0.0)),
                    ]),
                );
            });

            // The vignette.
            book.rect(
                Rect::new(0.0, 0.0, CANVAS.width, CANVAS.height),
                Gradient::radial(Offset::new(0.5, 0.5), 0.95).with_dither().with_stops(&[
                    (0.55, alpha(Color::BLACK, 0.0)),
                    (1.0, alpha(Color::BLACK, 0.42)),
                ]),
            );
        }),
    );
    stack = stack.push(Positioned::fill().child(courier));

    stack.push(receipt_panel(t)).into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(t: f32) -> WidgetNode {
    let fade_in = clamp01(t / 0.06);
    let s = t * SECONDS;

    // The census — the same arithmetic the buckets ran, printed.
    let mut pops = [0usize; BUCKETS];
    let mut live = 0usize;
    for k in 1..=GHOSTS {
        let sk = s - k as f32 * DT;
        if sk < 0.0 {
            continue;
        }
        live += 1;
        pops[bucket_of(heading_at(sk))] += 1;
    }
    let filtered = BUCKETS + 1; // buckets + the courier's bloom — the receipt's claim

    const P_X: f32 = 42.0;
    const P_Y: f32 = 560.0;
    const P_W: f32 = 360.0;

    let lines = [
        "E-24 · GHOSTS · MOTION PERSISTENCE (U-14/U-06)".to_string(),
        format!("ghosts {} live {} · buckets {}", GHOSTS, live, BUCKETS),
        format!(
            "bucket census [{} {} {} {}] · σ {:.1}",
            pops[0], pops[1], pops[2], pops[3], SIGMA
        ),
        format!(
            "filtered {} — one per ghost would be {}",
            filtered,
            GHOSTS + 1
        ),
        format!("heading {:+.2} rad · fade-in {:.0}%", heading_at(s), fade_in * 100.0),
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
                        .color(alpha(MUTED, 1.0)),
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
                        TextStyle::new(11.0).monospace().color(alpha(mix(MUTED, INK, 0.4), 0.95)),
                    ),
                ),
        );
    }

    // The instrument: the figure in miniature, the courier's position, and
    // the four bucket wedges sized by their census — the direction field.
    let chart = Painting::sized(
        Size::new(P_W, 88.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
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

            let to_panel = |p: Offset| -> Offset {
                Offset::new(
                    (p.dx - CENTER.dx) / 480.0 * (P_W * 0.5 - 50.0) + P_W * 0.5 - 12.0,
                    (p.dy - CENTER.dy) / 210.0 * 28.0 + 44.0,
                )
            };
            let mut figure = vieww_foundation::Path::new();
            let mut started = false;
            let mut ss = 0.0f32;
            while ss <= SECONDS {
                let p = to_panel(path_at(ss));
                if started {
                    figure.line_to(p);
                } else {
                    figure.move_to(p);
                    started = true;
                }
                ss += 0.03;
            }
            book.stroke(figure, alpha(FAINT, 0.30), 1.0);

            // The four bucket wedges, filled by their census share.
            for (b, pop) in pops.iter().enumerate() {
                let a0 =
                    bucket_angle(b) - std::f32::consts::TAU / BUCKETS as f32 / 2.0;
                let a1 = a0 + std::f32::consts::TAU / BUCKETS as f32;
                let c = Offset::new(P_W - 40.0, 44.0);
                let r = 10.0 + 12.0 * *pop as f32 / GHOSTS as f32;
                let mut wedge = vieww_foundation::Path::new();
                wedge.move_to(c);
                let steps = 10;
                for i in 0..=steps {
                    let a = a0 + (a1 - a0) * i as f32 / steps as f32;
                    wedge.line_to(Offset::new(c.dx + r * a.cos(), c.dy + r * a.sin()));
                }
                wedge.close();
                book.fill(wedge.clone(), alpha(VIOLET_SOFT, 0.10 + 0.30 * *pop as f32 / 22.0));
                book.stroke(wedge, alpha(VIOLET_SOFT, 0.35), 1.0);
            }

            // Where the courier is now.
            let here = to_panel(path_at(s));
            book.circle(here, 3.2, Color::WHITE);
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 116.0)
            .width(P_W)
            .height(88.0)
            .child(chart),
    );

    stack.into()
}
