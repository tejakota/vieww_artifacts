//! exp_typo — *the sentence becomes weather.*
//!
//! The film's S02 question — "how long should it take to see what you
//! built?" — breaks apart into its letters, and the letters become
//! weather: each glyph a positioned, rotated, resized `Text` widget riding
//! a deterministic flow field (integrated once, RK2, the currents' table
//! discipline — the frame at any `t` is a lookup, never a memory), speed
//! made visible through **four direction-bucketed `Filtered::with_blur_angle`
//! groups** (the ghosts' economy: 4 filtered layers for 40 letters), and
//! then the whole storm **condenses** — every letter spirals into one
//! point that becomes a single falling drop. The drop is the sea's.
//!
//! - **SETTLED** (t 0–0.12): the sentence, quiet.
//! - **LOOSENING** (t 0.12–0.30): the line breaks; letters gain weight
//!   and phase; spaces stop mattering.
//! - **THE STORM** (t 0.28–0.70): full flow field, blur by speed, colour
//!   drifting violet ↔ cyan with depth.
//! - **THE CONDENSATION** (t 0.68–1.0): the spiral closes; one drop
//!   remains, and every letter is accounted for.
//!
//! Tolerance axis: dozens of live positioned text nodes with per-node
//! transforms — the `glyph_runs` receipt line is the point.

use std::sync::OnceLock;

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextStyle, Transform};
use vieww_widget::prelude::*;
use vieww_widget::{Filtered, Opacity, Painting, PaintWith, Transformed};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, ease_out_cubic, mix, tint, FAINT, MUTED, VIOLET, VIOLET_SOFT,
    CYAN, CYAN_SOFT, BG_DEEP,
};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 8.0;

/// The sentence — the film's own question.
const LINE: &str = "how long should it take to see what you built?";

/// Storm table resolution — steps of integration, per letter.
const STEPS: usize = 130;

/// Blur buckets (the ghosts' count).
const BUCKETS: usize = 4;

// ── The storm tables — integrated once, evaluated forever ──────────────────

struct Storm {
    /// Letter start positions (line space, origin at line centre).
    starts: Vec<Offset>,
    /// Non-space letter indices (for rendering).
    chars: Vec<(char, usize)>,
    /// Per-letter trajectory tables.
    tables: Vec<Vec<Offset>>,
    /// Per-letter rate multiplier.
    rates: Vec<f32>,
    /// The letter's storm phase (for colour + rotation).
    phases: Vec<f32>,
}

fn hash01(i: usize) -> f32 {
    let h = (i as u64).wrapping_mul(0x9E3779B97F4A7C15).rotate_left(17);
    (h >> 40) as f32 / ((1u64 << 24) as f32)
}

/// The flow field: two crossing sines plus a swirl about the canvas centre.
fn flow(p: Offset) -> Offset {
    let cx = 640.0;
    let cy = 330.0;
    let dx = p.dx - cx;
    let dy = p.dy - cy;
    let r = (dx * dx + dy * dy).sqrt().max(1.0);
    let swirl = 0.55;
    let s1 = (p.dy * 0.013 + 0.4).sin() + (p.dx * 0.011).sin() * 0.55;
    let s2 = -(p.dx * 0.010 + 0.9).sin() + (p.dy * 0.012).cos() * 0.55;
    Offset::new(
        s1 * 4.6 + -dy / r * swirl * 3.2 + 1.1,
        s2 * 4.6 + dx / r * swirl * 3.2 - 0.55,
    )
}

fn storm() -> &'static Storm {
    static S: OnceLock<Storm> = OnceLock::new();
    S.get_or_init(|| {
        // Lay the line out: proportional advance per char.
        let em = 30.0;
        let adv: Vec<f32> = LINE
            .chars()
            .map(|c| if c == ' ' { em * 0.36 } else { em * 0.52 })
            .collect();
        let total: f32 = adv.iter().sum::<f32>() - em * 0.16;
        let mut starts = Vec::with_capacity(LINE.len());
        let mut x = -total * 0.5;
        for (i, a) in adv.iter().enumerate() {
            starts.push(Offset::new(x + if i == 0 { 0.0 } else { 0.0 }, 0.0));
            x += a;
        }

        let chars: Vec<(char, usize)> = LINE
            .chars()
            .enumerate()
            .filter(|(_, c)| *c != ' ')
            .map(|(i, c)| (c, i))
            .collect();

        let n = LINE.len();
        let rates = (0..n).map(|i| 0.75 + hash01(i * 7 + 1) * 0.6).collect();
        let phases = (0..n).map(|i| hash01(i * 13 + 5) * 6.28).collect();

        // Integrate each character (spaces too — they keep the rhythm).
        let base = Offset::new(640.0, 318.0);
        let mut tables = Vec::with_capacity(n);
        for (i, s0) in starts.iter().enumerate() {
            let p0 = Offset::new(base.dx + s0.dx, base.dy + s0.dy);
            let mut path = Vec::with_capacity(STEPS + 1);
            let mut p = p0;
            path.push(p);
            for _ in 0..STEPS {
                // RK2: half-step sample of the field.
                let v = flow(p);
                let mid = Offset::new(p.dx + v.dx * 0.5, p.dy + v.dy * 0.5);
                let v2 = flow(mid);
                p = Offset::new(p.dx + v2.dx, p.dy + v2.dy);
                path.push(p);
            }
            tables.push(path);
        }

        Storm {
            starts,
            chars,
            tables,
            rates,
            phases,
        }
    })
}

// ── Positions, as pure functions of t ─────────────────────────────────────

/// The storm's progress along its tables at `t` (0 = not started).
fn storm_s(t: f32) -> f32 {
    ease_in_out(clamp01((t - 0.12) / 0.85))
}

/// The condensation blend (0 = storm, 1 = fully condensed).
fn condense_s(t: f32) -> f32 {
    ease_in_out(clamp01((t - 0.68) / 0.30))
}

/// A letter's centre at film-time `t` — trajectory lookup + condensation
/// blend, in canvas space.
fn letter_pos(idx: usize, t: f32) -> Offset {
    let s = storm();
    let table = &s.tables[idx];
    let k = (storm_s(t) * s.rates[idx] * STEPS as f32).round() as usize;
    let k = k.min(STEPS);
    let p = table[k];

    // Condensation: the golden spiral into one point.
    let c = condense_s(t);
    if c <= 0.0 {
        return p;
    }
    let centre = Offset::new(640.0, 300.0);
    let ang = idx as f32 * 2.39996 + t * 0.8;
    let r = (18.0 + (idx % 7) as f32 * 5.0) * (1.0 - c);
    let target = Offset::new(
        centre.dx + r * ang.cos(),
        centre.dy + r * ang.sin() * 0.72,
    );
    Offset::new(
        p.dx + (target.dx - p.dx) * c,
        p.dy + (target.dy - p.dy) * c,
    )
}

/// A letter's velocity (px per 0.02 t), from the same pure position.
fn letter_vel(idx: usize, t: f32) -> Offset {
    let a = letter_pos(idx, t);
    let b = letter_pos(idx, (t - 0.02).max(0.0));
    Offset::new(a.dx - b.dx, a.dy - b.dy)
}

// ── The scene ───────────────────────────────────────────────────────────────

fn scene(book: &mut Sketchbook, canvas: Size, t: f32) {
    let w = canvas.width;
    let h = canvas.height;
    let s = storm();

    // The ground.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::vertical()
            .with_dither()
            .with_stops(&[
                (0.0, Color::rgb(9, 9, 14)),
                (0.6, BG_DEEP),
                (1.0, Color::rgb(5, 5, 9)),
            ]),
    );

    // The wind, made faintly visible: streamlines of the same field the
    // letters ride (the currents' receipt, drawn as texture).
    {
        let mut rng = crate::film_lib::Rng::new(0x799);
        let gust = clamp01((t - 0.12) * 1.8);
        let mut path = vieww_foundation::Path::new();
        for i in 0..22 {
            let mut p = Offset::new(rng.f01() * w, rng.f01() * h);
            path.move_to(p);
            for _ in 0..26 {
                let v = flow(p);
                p = Offset::new(p.dx + v.dx * 1.6, p.dy + v.dy * 1.6);
                path.line_to(p);
            }
        }
        book.stroke(path, alpha(VIOLET, 0.045 * gust), 1.0);
    }

    // The condensation point's arrival — the drop forming.
    let c = condense_s(t);
    if c > 0.08 {
        let centre = Offset::new(640.0, 300.0);
        let glow = (c - 0.08) / 0.92;
        // A ring pulse when the spiral closes tight.
        let pulse = (glow * 3.2).sin().abs() * glow;
        book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
            g.circle(
                centre,
                10.0 + glow * 42.0,
                Gradient::radial_fill().with_stops(&[
                    (0.0, alpha(mix(CYAN_SOFT, Color::WHITE, 0.5), 0.45 * glow)),
                    (1.0, alpha(CYAN, 0.0)),
                ]),
            );
            if pulse > 0.05 {
                g.circle(
                    centre,
                    26.0 + glow * 90.0 * (1.0 - pulse * 0.3),
                    Gradient::radial_fill().with_stops(&[
                        (0.0, alpha(CYAN, 0.0)),
                        (0.7, alpha(CYAN_SOFT, 0.20 * pulse)),
                        (1.0, alpha(CYAN, 0.0)),
                    ]),
                );
            }
            g.circle(centre, 2.6 + glow * 4.2, alpha(Color::WHITE, 0.95 * glow));
        });
        // The last beat: the drop detaches and starts to fall — toward the
        // sea plate's world. One falling streak.
        if t > 0.93 {
            let fall = ease_out_cubic(clamp01((t - 0.93) / 0.07));
            let y = 300.0 + fall * fall * 120.0;
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                g.line(
                    Offset::new(640.0, y - 10.0),
                    Offset::new(640.0, y),
                    alpha(tint(CYAN_SOFT, 0.5), 0.6),
                    2.0,
                );
            });
        }
    }

    // The vignette.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::radial(Offset::new(0.5, 0.5), 0.82)
            .with_dither()
            .with_stops(&[
                (0.6, alpha(Color::BLACK, 0.0)),
                (1.0, alpha(Color::BLACK, 0.46)),
            ]),
    );
}

/// Which blur bucket a heading falls in — BUCKETS wedges of the full turn.
fn bucket_of(angle: f32) -> usize {
    let a = angle.rem_euclid(std::f32::consts::TAU);
    ((a / std::f32::consts::TAU * BUCKETS as f32).floor() as usize).min(BUCKETS - 1)
}

/// The bucket's blur axis — its wedge centre, radians.
fn bucket_angle(bucket: usize) -> f32 {
    (bucket as f32 + 0.5) / BUCKETS as f32 * std::f32::consts::TAU
}

/// The frame: the painting, then the letters — grouped by blur bucket.
pub fn frame(t: f32) -> WidgetNode {
    let s = storm();
    let loose = clamp01((t - 0.12) / 0.18);
    let storm_g = clamp01((t - 0.28) / 0.42);
    let c = condense_s(t);

    let paint = Painting::sized(
        crate::film_lib::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            scene(book, size, t);
        }),
    );

    // Build per-letter state: bucket, speed, alpha, size, rotation.
    struct LetterState {
        idx: usize,
        ch: char,
        pos: Offset,
        speed: f32,
        bucket: usize,
        rot: f32,
        size: f32,
        color: Color,
        alpha: f32,
    }
    let mut states: Vec<LetterState> = Vec::with_capacity(s.chars.len());
    let mut speeds = 0.0f32;
    for &(ch, idx) in &s.chars {
        let p = letter_pos(idx, t);
        let v = letter_vel(idx, t);
        let speed = (v.dx * v.dx + v.dy * v.dy).sqrt() * 30.0; // px per second-ish
        speeds += speed;
        let ang = v.dy.atan2(v.dx);
        let ph = s.phases[idx];
        let depth = 0.5 + 0.5 * (ph + t * 1.1).sin();
        let rot = (ph + t * (0.4 + storm_g * 1.4)) * 0.22 * storm_g;
        let size = 24.0 * (1.0 + 0.34 * depth * storm_g) * (1.0 - c * 0.55);
        let color = mix(VIOLET_SOFT, CYAN_SOFT, depth);
        let a = (1.0 - c * 0.88).max(0.0) * (0.45 + 0.55 * loose);
        states.push(LetterState {
            idx,
            ch,
            pos: p,
            speed,
            bucket: bucket_of(ang),
            rot,
            size,
            color,
            alpha: a,
        });
    }
    let mean_speed = if states.is_empty() { 0.0 } else { speeds / states.len() as f32 };
    let blur_k = (mean_speed / 260.0).clamp(0.0, 1.0) * storm_g * (1.0 - c);
    let sigma = 1.0 + blur_k * 3.6;

    // One letter as a widget subtree: positioned, rotated about its centre,
    // coloured by its depth.
    let letter_node = |st: &LetterState| -> WidgetNode {
        let b = st.size;
        let alpha_k = st.alpha;
        Stack::new()
            .push(
                Positioned::new()
                    .left(st.pos.dx - b * 0.5)
                    .top(st.pos.dy - b * 0.62)
                    .width(b)
                    .height(b * 1.24)
                    .child(
                        Transformed::new(Transform::rotate_around(
                            Offset::new(b * 0.5, b * 0.62),
                            st.rot,
                        ))
                        .child(
                            Opacity::new(alpha_k).child(
                                Text::new(st.ch.to_string()).style(
                                    TextStyle::new(st.size)
                                        .color(st.color)
                                        .weight(FontWeight::Medium),
                                ),
                            ),
                        ),
                    ),
            )
            .into()
    };

    let mut stack = Stack::new().push(Positioned::fill().child(paint));

    // The letters, bucketed: BUCKETS Filtered widgets (each one filtered
    // layer), plus one crisp group for the slow letters.
    for bucket in 0..BUCKETS {
        let in_bucket: Vec<&LetterState> = states.iter().filter(|st| st.bucket == bucket).collect();
        if in_bucket.is_empty() {
            continue;
        }
        let mut inner = Stack::new();
        for st in &in_bucket {
            inner = inner.push(letter_node(st));
        }
        let blurred = Filtered::new()
            .with_blur(sigma)
            .with_blur_angle(bucket_angle(bucket))
            .child(inner);
        stack = stack.push(Positioned::fill().child(blurred));
    }

    // The instrument strip.
    let letters = states.len();
    let strip = Stack::new()
        .push(
            Positioned::new()
                .left(24.0)
                .top(20.0)
                .width(560.0)
                .height(18.0)
                .child(
                    Text::new("THE TYPO-STORM · the sentence becomes weather").style(
                        TextStyle::new(12.0)
                            .monospace()
                            .letter_spacing(2.2)
                            .color(alpha(FAINT, 0.95)),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(24.0)
                .top(40.0)
                .width(720.0)
                .height(18.0)
                .child(
                    Text::new(format!(
                        "{letters} letters · {BUCKETS} blur buckets (U-14) · v̄ {mean_speed:>5.0} px/s · σ {sigma:>4.1} · condense {c:>4.0}%"
                    ))
                    .style(TextStyle::new(11.0).monospace().color(alpha(MUTED, 0.85))),
                ),
        );
    stack = stack.push(strip);

    stack.into()
}
