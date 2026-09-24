//! exp_currents — *a static field, made visible by its motion.* The flow plate.
//!
//! A curl-noise velocity field — the perpendicular gradient of a scalar
//! value-noise, divergence-free by construction, so the streamlines that
//! advect through it never terminate inside the domain. The field is
//! integrated **once**, deterministically, at first frame: 168 streamlines,
//! RK2, 110 steps each — then every frame is pure phase.
//!
//! What moves is the *pattern, not the geometry*: each streamline is one
//! stroked path whose [`Dash`] marches downstream at its own speed — E-17's
//! dash-phase grammar, but continuous, a river read as kinetic. On top ride
//! 36 tracers, bright dots whose positions are looked up in each streamline's
//! arc-length table at the same speed the dashes march — the tracers *are*
//! the dashes, promoted to light.
//!
//! One Plus-blended group carries all 168 lines (U-01's economy); the
//! receipt prints the census a single layer actually transported.

use std::sync::OnceLock;

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook,
    StrokeStyle, TextStyle, Dash};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{alpha, clamp01, mix, BG_DEEP, CANVAS, CYAN, CYAN_SOFT, FAINT, INK, MUTED,
    VIOLET_SOFT, Rng};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 10.0;

/// Streamlines integrated through the field.
const STREAMS: usize = 168;

/// Integration steps per streamline.
const STEPS: usize = 110;

/// Step length, px.
const STEP: f32 = 5.5;

/// Dash pattern: on, off — the marching river.
const DASH_ON: f32 = 34.0;
const DASH_OFF: f32 = 26.0;

/// Tracers riding the streamlines.
const TRACERS: usize = 36;

// ── The field — curl of scalar value noise ──────────────────────────────────

/// The scalar noise: bilinear over an 11×7 Rng lattice, wrapped.
fn noise_at(lattice: &[[f32; 11]; 7], x: f32, y: f32) -> f32 {
    let gx = x / 176.0;
    let gy = y / 145.0;
    let xi = gx.floor() as i32;
    let yi = gy.floor() as i32;
    let fx = gx - xi as f32;
    let fy = gy - yi as f32;
    let smx = fx * fx * (3.0 - 2.0 * fx);
    let smy = fy * fy * (3.0 - 2.0 * fy);
    let at = |ix: i32, iy: i32| -> f32 {
        let ix = ix.rem_euclid(11) as usize;
        let iy = iy.rem_euclid(7) as usize;
        lattice[iy][ix]
    };
    let a = at(xi, yi);
    let b = at(xi + 1, yi);
    let c = at(xi, yi + 1);
    let d = at(xi + 1, yi + 1);
    a + (b - a) * smx + (c - a) * smy + (a - b - c + d) * smx * smy
}

/// The velocity at a point: the curl of the noise — divergence-free.
fn velocity(lattice: &[[f32; 11]; 7], x: f32, y: f32) -> (f32, f32) {
    const EPS: f32 = 6.0;
    let dndx = (noise_at(lattice, x + EPS, y) - noise_at(lattice, x - EPS, y)) / (2.0 * EPS);
    let dndy = (noise_at(lattice, x, y + EPS) - noise_at(lattice, x, y - EPS)) / (2.0 * EPS);
    // 12 px of reach per noise unit — the swirl's scale.
    (dndy * 12.0, -dndx * 12.0)
}

// ── The streamlines — integrated once, tabled by arc length ─────────────────

/// One streamline: its polyline, cumulative arc lengths, mean speed, tint.
struct Stream {
    pts: Vec<Offset>,
    /// `cum[i]` = arc length from the start to `pts[i]`.
    cum: Vec<f32>,
    speed: f32,
    /// The dash march rate, px/s — each line's own current.
    march: f32,
    hue: f32,
}

fn field() -> &'static (Vec<Stream>, [[f32; 11]; 7]) {
    static FIELD: OnceLock<(Vec<Stream>, [[f32; 11]; 7])> = OnceLock::new();
    FIELD.get_or_init(|| {
        let mut rng = Rng::new(0xC0A1);
        let mut lattice = [[0.0f32; 11]; 7];
        for row in &mut lattice {
            for v in row {
                *v = rng.f01();
            }
        }

        // Seeds on a jittered lattice — even coverage, no two lines twins.
        let mut streams = Vec::new();
        let cols = 14;
        let rows = 12;
        let mut seed_rng = Rng::new(0xC0A2);
        for r in 0..rows {
            for c in 0..cols {
                if streams.len() >= STREAMS {
                    break;
                }
                let x = (c as f32 + 0.5) / cols as f32 * CANVAS.width + seed_rng.sym() * 26.0;
                let y = (r as f32 + 0.5) / rows as f32 * CANVAS.height + seed_rng.sym() * 18.0;
                let mut pts = vec![Offset::new(x, y)];
                let mut cum = vec![0.0f32];
                let mut speed_acc = 0.0f32;
                for _ in 0..STEPS {
                    let (vx, vy) = velocity(&lattice, x, y);
                    let (x0, y0) = {
                        let last = pts.last().expect("streamline has a start");
                        (last.dx, last.dy)
                    };
                    // RK2: midpoint velocity decides the step.
                    let (mx, my) = velocity(
                        &lattice,
                        x0 + vx * STEP * 0.5,
                        y0 + vy * STEP * 0.5,
                    );
                    let len = (mx * mx + my * my).sqrt();
                    if len < 1e-4 {
                        break;
                    }
                    let nx = x0 + mx / len * STEP;
                    let ny = y0 + my / len * STEP;
                    if nx < -30.0 || nx > CANVAS.width + 30.0 || ny < -30.0 || ny > CANVAS.height + 30.0 {
                        break;
                    }
                    let next = Offset::new(nx, ny);
                    cum.push(*cum.last().unwrap() + STEP);
                    pts.push(next);
                    speed_acc += len;
                }
                if pts.len() < 12 {
                    continue;
                }
                let speed = speed_acc / pts.len() as f32;
                streams.push(Stream {
                    pts,
                    cum,
                    speed,
                    march: 22.0 + speed * 340.0,
                    hue: (x / CANVAS.width).clamp(0.0, 1.0),
                });
            }
        }
        (streams, lattice)
    })
}

/// A point at arc length `s` along a streamline — binary search in the table.
fn point_at(s: &Stream, dist: f32) -> Option<Offset> {
    let total = *s.cum.last()?;
    let d = dist.clamp(0.0, total);
    let i = s.cum.partition_point(|&c| c < d).max(1);
    let c0 = s.cum[i - 1];
    let c1 = s.cum[i];
    let f = if c1 > c0 { (d - c0) / (c1 - c0) } else { 0.0 };
    let a = s.pts[i - 1];
    let b = s.pts[i];
    Some(Offset::new(
        a.dx + (b.dx - a.dx) * f,
        a.dy + (b.dy - a.dy) * f,
    ))
}

// ── The board ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let (streams, lattice) = field();
    let fade_in = clamp01(t / 0.08);

    let board = Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — deep water.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(6, 8, 12)),
                    (0.6, BG_DEEP),
                    (1.0, Color::rgb(7, 10, 14)),
                ]),
            );

            // The field's own heat — a coarse census of |v|, sampled where
            // the noise is evaluated. Faint: the river will carry it.
            for gy in 0..8 {
                for gx in 0..14 {
                    let x = (gx as f32 + 0.5) / 14.0 * w;
                    let y = (gy as f32 + 0.5) / 8.0 * h;
                    let (vx, vy) = velocity(lattice, x, y);
                    let m = (vx * vx + vy * vy).sqrt();
                    book.circle(
                        Offset::new(x, y),
                        3.0 + 26.0 * m,
                        Gradient::radial_fill().with_dither().with_stops(&[
                            (0.0, alpha(CYAN, 0.05 + 0.09 * m)),
                            (1.0, alpha(CYAN, 0.0)),
                        ]),
                    );
                }
            }

            // ── THE RIVER — one Plus group, every line marching ─────────
            book.blended_layer(fade_in, 0.0, BlendMode::Plus, None, |g| {
                for s in streams {
                    let mut p = Path::new();
                    p.move_to(s.pts[0]);
                    for pt in &s.pts[1..] {
                        p.line_to(*pt);
                    }
                    // The line's own current: the pattern marches downstream
                    // at `s.march` px/s. Color rides position; alpha rides
                    // speed — the slow water is quiet.
                    let col = mix(VIOLET_SOFT, CYAN_SOFT, s.hue);
                    let a = (0.17 + 0.34 * (s.speed * 4.0).clamp(0.0, 1.0)) * fade_in;
                    let dash = Dash {
                        pattern: vec![DASH_ON, DASH_OFF],
                        offset: -(t * SECONDS * s.march),
                    };
                    let style = StrokeStyle::default().dash(dash);
                    g.stroke_styled(p, alpha(col, a), 1.4, style);
                }
            });

            // ── THE TRACERS — the dashes promoted to light ─────────────
            book.blended_layer(fade_in, 0.0, BlendMode::Plus, None, |g| {
                let mut rng = Rng::new(0xC0A3);
                for i in 0..TRACERS {
                    let s = &streams[i % streams.len()];
                    // Each tracer starts where it starts, deterministically,
                    // and rides the same march the dashes do.
                    let start = rng.f01() * s.cum.last().copied().unwrap_or(0.0);
                    let phase = t * SECONDS * s.march;
                    let dist = (start + phase).rem_euclid(s.cum.last().copied().unwrap_or(1.0).max(1.0));
                    let Some(pt) = point_at(s, dist) else { continue };
                    let col = mix(Color::WHITE, CYAN_SOFT, 0.30);
                    g.circle(pt, 2.4, alpha(col, fade_in));
                    g.circle(pt, 7.0, alpha(CYAN_SOFT, 0.24 * fade_in));
                }
            });

            // The vignette.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.5), 0.9).with_dither().with_stops(&[
                    (0.55, alpha(Color::BLACK, 0.0)),
                    (1.0, alpha(Color::BLACK, 0.40)),
                ]),
            );
        }),
    );

    Stack::new()
        .push(Positioned::fill().child(board))
        .push(receipt_panel(t))
        .into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(t: f32) -> WidgetNode {
    let (streams, _) = field();
    let fade_in = clamp01(t / 0.08);
    let lens: Vec<f32> = streams.iter().map(|s| *s.cum.last().unwrap()).collect();
    let mean_len = lens.iter().sum::<f32>() / lens.len().max(1) as f32;
    let total_ink = lens.iter().sum::<f32>();
    let speeds: Vec<f32> = streams.iter().map(|s| s.speed).collect();
    let smax = speeds.iter().cloned().fold(0.0f32, f32::max);
    let march_max = streams.iter().map(|s| s.march).fold(0.0f32, f32::max);

    const P_X: f32 = 42.0;
    const P_Y: f32 = 560.0;
    const P_W: f32 = 368.0;

    let lines = [
        "CURRENTS · CURL NOISE · THE RIVER MARCHES (E-17)".to_string(),
        format!("streams {} × ≤{} steps · mean {:.0} px · ink {:.0} px",
            streams.len(), STEPS, mean_len, total_ink),
        format!("|v| max {:.3} · march max {:.0} px/s", smax, march_max),
        format!("dash [{:.0} {:.0}] · tracers {} · two Plus groups", DASH_ON, DASH_OFF,
            TRACERS),
        format!("fade-in {:.0}% · every number integrated, not typed", fade_in * 100.0),
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

    // The instrument: the field's direction map — a sparse quiver of the
    // actual velocity field, arrows sized by |v|, drawn from the same
    // lattice the river integrates.
    let quiver = Painting::sized(
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
            for gy in 0..4 {
                for gx in 0..10 {
                    // The panel's own mini-lattice sample of the field.
                    let x = 16.0 + gx as f32 * (P_W - 32.0) / 9.0;
                    let y = 14.0 + gy as f32 * 20.0;
                    let world_x = gx as f32 / 9.0 * CANVAS.width;
                    let world_y = gy as f32 / 3.0 * CANVAS.height;
                    let (vx, vy) = velocity(&field().1, world_x, world_y);
                    let m = (vx * vx + vy * vy).sqrt();
                    let len = 6.0 + 10.0 * (m * 4.0).clamp(0.0, 1.0);
                    let (dx, dy) = if m > 1e-4 { (vx / m, vy / m) } else { (1.0, 0.0) };
                    let tail = Offset::new(x - dx * 3.0, y - dy * 3.0);
                    let tip = Offset::new(x + dx * len, y + dy * len);
                    book.line(
                        tail,
                        tip,
                        alpha(mix(VIOLET_SOFT, CYAN_SOFT, world_x / CANVAS.width),
                            0.25 + 0.55 * (m * 4.0).clamp(0.0, 1.0)),
                        1.1,
                    );
                    // The arrowhead — two short barbs back from the tip.
                    book.line(
                        tip,
                        Offset::new(tip.dx - dx * 4.0 - dy * 3.0, tip.dy - dy * 4.0 + dx * 3.0),
                        alpha(FAINT, 0.5),
                        1.0,
                    );
                    book.line(
                        tip,
                        Offset::new(tip.dx - dx * 4.0 + dy * 3.0, tip.dy - dy * 4.0 - dx * 3.0),
                        alpha(FAINT, 0.5),
                        1.0,
                    );
                }
            }
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 116.0)
            .width(P_W)
            .height(88.0)
            .child(quiver),
    );

    let _ = t;
    stack.into()
}
