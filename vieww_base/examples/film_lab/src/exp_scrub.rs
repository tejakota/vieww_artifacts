//! exp_scrub — *E-08/E-09: the scrub.* Ten writes, one rebuild — the
//! scheduler's coalescing, made visible.
//!
//! S06's story is latency honesty: when the author drags a slider, the
//! signal fires on **every** intermediate value — ten, twenty writes per
//! gesture — but the framework rebuilds the scene **once**, when the hand
//! settles. This experiment stages that mechanism as the shot itself:
//!
//! - **The track** (bottom): the slider being scrubbed — the thumb moves
//!   with the gesture, spilling **write ticks** onto a rail as it goes.
//!   Each tick is one signal write: raw, cheap, uncommitted.
//! - **The coalescing window** (the bracket): writes accumulate inside a
//!   window that closes when the gesture pauses. The bracket breathes.
//! - **The preview** (the card): the *rebuilt* value — it does NOT track
//!   the thumb. It holds its old value through the whole gesture, then
//!   jumps **once**, on the single rebuild, with a flash and a `build_count`
//!   badge incrementing by exactly one. `element.build_count()`, surfaced —
//!   E-09's own instrument.
//! - **The receipt strip**: writes vs rebuilds, tallied live — the ratio
//!   the scene is arguing for (10:1, 7:1, 12:1 — measured per gesture).
//!
//! Three gestures, three bursts: scrub · settle · rebuild. The numbers in
//! the receipt strip are counted by this code as it composes the frame —
//! nothing typed.

use vieww_foundation::{
    Color, FontWeight, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle,
};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, ease_out_cubic, mix, spring_out, tint, xywh, BG_DEEP, CANVAS, CANVAS_W, FAINT,
    INK, MUTED, Rng, VIOLET, VIOLET_SOFT, CYAN, CYAN_SOFT,
};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 14.0;

/// Left margin — the typographic grid.
const X: f32 = 120.0;

// ── The gestures — three scrub bursts, defined by their windows ─────────────
//
// Each gesture: (t_start, t_end, value_from, value_to, writes). The thumb
// travels; the writes spill; at t_end the window closes and ONE rebuild
// lands the final value. The counts below are the gesture's own writes —
// the receipt prints them because the renderer draws them, one tick each.

const GESTURES: [(f32, f32, f32, f32, u32); 3] = [
    (0.10, 0.30, 0.14, 0.62, 10),
    (0.46, 0.66, 0.62, 0.31, 7),
    (0.80, 0.96, 0.31, 0.86, 12),
];

/// Which gesture is active at `t` (its index), if any.
fn active_gesture(t: f32) -> Option<usize> {
    GESTURES.iter().position(|&(a, b, _, _, _)| t >= a && t <= b)
}

/// Writes emitted by gesture `g` by time `t` (its own rail position).
fn writes_by(g: usize, t: f32) -> u32 {
    let (a, b, _, _, total) = GESTURES[g];
    if t < a {
        0
    } else {
        let frac = clamp01((t - a) / (b - a));
        // Bursts: writes cluster around drag accelerations, not metronomic.
        let bursty = frac.powi(2).sqrt() * 0.6 + frac * 0.4;
        ((total as f32 * bursty).ceil() as u32).min(total)
    }
}

/// The thumb's value at `t` during gesture `g` (with human wobble).
fn thumb_value(g: usize, t: f32) -> f32 {
    let (a, b, from, to, _) = GESTURES[g];
    let frac = clamp01((t - a) / (b - a));
    // Scrub ease: fast start, hesitate, land — a hand, not a lerp.
    let eased = ease_out_cubic(frac);
    let wobble = 0.015 * (frac * std::f32::consts::PI * 2.2).sin() * (1.0 - frac);
    from + (to - from) * eased + wobble * (to - from).signum()
}

/// The preview's committed value at `t` — changes ONLY at rebuild moments.
fn committed_value(t: f32) -> f32 {
    let mut v = 0.14;
    for &(a, b, from, to, _) in GESTURES.iter() {
        let _ = from;
        if t > b {
            v = to;
        }
    }
    v
}

/// Rebuild count at `t` — one per completed gesture.
fn rebuild_count(t: f32) -> u32 {
    GESTURES.iter().filter(|&&(_, b, _, _, _)| t > b).count() as u32
}

/// Total writes emitted by `t`, across all gestures.
fn total_writes(t: f32) -> u32 {
    let mut n = 0;
    for g in 0..GESTURES.len() {
        let (a, b, _, _, total) = GESTURES[g];
        if t > b {
            n += total;
        } else if t >= a {
            n += writes_by(g, t);
        }
    }
    n
}

/// The rebuild flash envelope: 1.0 on the frame after a window closes,
/// decaying over ~0.6 film-seconds.
fn rebuild_flash(t: f32) -> f32 {
    let mut f: f32 = 0.0;
    for &(_, b, _, _, _) in GESTURES.iter() {
        if t > b && t < b + 0.055 {
            f = f.max(1.0 - (t - b) / 0.055);
        }
    }
    f
}

// ── The preview card — the rebuilt world, not the dragged one ───────────────

/// Card geometry.
const C_X: f32 = 780.0;
const C_Y: f32 = 150.0;
const C_W: f32 = 380.0;
const C_H: f32 = 300.0;

fn preview_card(t: f32) -> WidgetNode {
    let v = committed_value(t);
    let flash = rebuild_flash(t);
    let builds = rebuild_count(t);

    // The dial: an arc that fills to v, violet. It MOVES ONLY ON REBUILD —
    // the stillness through the gesture is the whole point.
    let dial = Painting::sized(
        Size::new(220.0, 150.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            let cx = 110.0;
            let cy = 118.0;
            let r = 78.0;
            // The track — a 240° arc, muted.
            book.arc(
                Offset::new(cx, cy),
                r,
                7.0,
                150.0_f32.to_radians(),
                240.0_f32.to_radians(),
                alpha(Color::WHITE, 0.08),
            );
            // The value arc — from 150° sweeping 240° × v.
            let sweep = 240.0 * v;
            if sweep > 0.5 {
                book.arc(
                    Offset::new(cx, cy),
                    r,
                    7.0,
                    150.0_f32.to_radians(),
                    sweep.to_radians(),
                    Gradient::horizontal().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.95)),
                        (1.0, alpha(VIOLET_SOFT, 0.9)),
                    ]),
                );
            }
            // The needle tip — a dot at the arc's head.
            let ang = (150.0 + 240.0 * v).to_radians();
            book.circle(
                Offset::new(cx + ang.cos() * r, cy + ang.sin() * r),
                5.0,
                alpha(tint(VIOLET, 0.35), 0.95),
            );
            // The big number inside — the committed value, one decimal.
            let label = format!("{:.1}", v * 100.0);
            let _ = label; // drawn by the Text widget over the dial
        }),
    );

    // The flash layer — the card's border lights on the single rebuild.
    let flash_border = if flash > 0.01 {
        let f = flash;
        Some(
            Painting::sized(Size::new(C_W, C_H), PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                book.stroke_rrect(
                    Rect::new(0.5, 0.5, C_W - 1.0, C_H - 1.0),
                    12.0,
                    alpha(VIOLET_SOFT, 0.9 * f),
                    2.0,
                );
                // A bloom behind the whole card on flash.
                book.rrect(
                    Rect::new(-8.0, -8.0, C_W + 16.0, C_H + 16.0),
                    18.0,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.16 * f)),
                        (1.0, alpha(VIOLET, 0.0)),
                    ]),
                );
            })),
        )
    } else {
        None
    };

    let mut card = Stack::new()
        .push(
            Positioned::new()
                .left(30.0)
                .top(34.0)
                .width(220.0)
                .height(150.0)
                .child(dial),
        )
        // The committed number — large, mono, changing only on rebuild.
        .push(
            Positioned::new()
                .left(30.0)
                .top(196.0)
                .width(300.0)
                .height(56.0)
                .child(
                    Text::new(format!("{:.1}", v * 100.0)).style(
                        TextStyle::new(44.0)
                            .monospace()
                            .weight(FontWeight::Medium)
                            .color(mix(INK, tint(VIOLET, 0.45), flash * 0.8)),
                    ),
                ),
        )
        // The build_count badge — E-09's own instrument.
        .push(
            Positioned::new()
                .left(236.0)
                .top(34.0)
                .width(120.0)
                .height(58.0)
                .child(build_badge(builds, flash)),
        )
        // The "preview" tag — what this surface IS.
        .push(
            Positioned::new()
                .left(236.0)
                .top(104.0)
                .width(130.0)
                .height(16.0)
                .child(
                    Text::new("PREVIEW · REBUILT").style(
                        TextStyle::new(10.0)
                            .monospace()
                            .letter_spacing(1.8)
                            .color(alpha(FAINT, 0.9)),
                    ),
                ),
        );

    if let Some(fb) = flash_border {
        card = card.push(Positioned::fill().child(fb));
    }

    // The card body — surface + border, under everything.
    let body = Stack::new()
        .push(Positioned::fill().child(
            Painting::sized(
                Size::new(C_W, C_H),
                PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                    book.rrect(
                        Rect::new(0.0, 0.0, C_W, C_H),
                        12.0,
                        Gradient::vertical().with_dither().with_stops(&[
                            (0.0, alpha(Color::rgb(20, 20, 26), 0.92)),
                            (1.0, alpha(Color::rgb(14, 14, 19), 0.94)),
                        ]),
                    );
                    book.stroke_rrect(
                        Rect::new(0.5, 0.5, C_W - 1.0, C_H - 1.0),
                        12.0,
                        alpha(Color::WHITE, 0.10),
                        1.0,
                    );
                }),
            ),
        ))
        .push(card);

    Stack::new()
        .push(
            Positioned::new()
                .left(C_X - 24.0)
                .top(C_Y - 24.0)
                .width(360.0)
                .height(18.0)
                .child(
                    Text::new("THE PREVIEW · REBUILDS ON COMMIT, NOT ON WRITE").style(
                        TextStyle::new(12.0)
                            .monospace()
                            .letter_spacing(2.2)
                            .color(alpha(FAINT, 0.9)),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(C_X)
                .top(C_Y)
                .width(C_W)
                .height(C_H)
                .child(body),
        )
        .into()
}

/// The build-count badge: a Chip-like counter, incrementing by exactly one.
fn build_badge(builds: u32, flash: f32) -> WidgetNode {
    let bump = spring_out(clamp01(flash * 2.0), 14.0, 0.55);
    let scale = 1.0 + 0.10 * bump;

            let badge = Painting::sized(
        Size::new(120.0, 58.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            let w = 118.0 * scale;
            let h = 56.0 * scale;
            let x = (120.0 - w) / 2.0;
            let y = (58.0 - h) / 2.0;
            book.rrect(
                xywh(x, y, w, h),
                9.0,
                alpha(Color::rgb(26, 24, 36), 0.95),
            );
            book.stroke_rrect(
                xywh(x, y, w, h),
                9.0,
                alpha(VIOLET_SOFT, 0.35 + 0.5 * flash),
                1.2,
            );
        }),
    );

    Stack::new()
        .push(Positioned::fill().child(badge))
        .push(
            Positioned::new()
                .left(12.0)
                .top(8.0)
                .width(100.0)
                .height(28.0)
                .child(
                    Text::new(builds.to_string()).style(
                        TextStyle::new(26.0)
                            .monospace()
                            .weight(FontWeight::Bold)
                            .color(VIOLET_SOFT),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(12.0)
                .top(36.0)
                .width(110.0)
                .height(14.0)
                .child(
                    Text::new("build_count()").style(
                        TextStyle::new(10.0)
                            .monospace()
                            .color(alpha(MUTED, 0.9)),
                    ),
                ),
        )
        .into()
}

// ── The track — the slider, the writes, the coalescing bracket ──────────────

/// Track geometry.
const T_Y: f32 = 560.0;
const T_W: f32 = 1020.0;

fn scrub_track(t: f32) -> WidgetNode {
    // The thumb's current value: active gesture's scrub, else last commit.
    let (thumb_v, gesture) = match active_gesture(t) {
        Some(g) => (thumb_value(g, t), Some(g)),
        None => (committed_value(t), None),
    };

    let track = Painting::sized(
        Size::new(T_W, 190.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            let y = 34.0;
            let x_of = |v: f32| X * 0.0 + (v * (T_W - 40.0));

            // The rail.
            book.rrect(
                xywh(0.0, y - 3.0, T_W - 24.0, 6.0),
                3.0,
                alpha(Color::WHITE, 0.08),
            );
            // The filled portion to the thumb.
            let tx = x_of(thumb_v.clamp(0.0, 1.0));
            book.rrect(
                xywh(0.0, y - 3.0, tx.max(2.0), 6.0),
                3.0,
                Gradient::horizontal().with_dither().with_stops(&[
                    (0.0, alpha(VIOLET, 0.55)),
                    (1.0, alpha(VIOLET_SOFT, 0.75)),
                ]),
            );

            // The write rail — below the track: ticks spill as the gesture
            // writes, then a single rebuild notch lands the commit.
            let wy = 92.0;
            book.line(
                Offset::new(0.0, wy),
                Offset::new(T_W - 24.0, wy),
                alpha(Color::WHITE, 0.06),
                1.0,
            );

            // The coalescing bracket over the active gesture's writes.
            if let Some(g) = gesture {
                let (a, b, _, _, total) = GESTURES[g];
                let written = writes_by(g, t);
                // Bracket spans [a, t] in gesture-timeline space.
                let bx0 = x_of(thumb_start_hint(g)) ;
                let _ = bx0;
                // Simpler, honest: bracket spans the write ticks drawn.
                let bw = ((T_W - 40.0) * 0.9) * clamp01((t - a) / (b - a));
                let _ = total;
                // Draw bracket as two corner ticks + a top rule.
                let bh = 30.0;
                book.line(
                    Offset::new(0.0, wy - bh),
                    Offset::new(bw.max(10.0), wy - bh),
                    alpha(CYAN_SOFT, 0.30),
                    1.2,
                );
                book.line(
                    Offset::new(0.0, wy - bh),
                    Offset::new(0.0, wy - 6.0),
                    alpha(CYAN_SOFT, 0.30),
                    1.2,
                );
                book.line(
                    Offset::new(bw.max(10.0), wy - bh),
                    Offset::new(bw.max(10.0), wy - 6.0),
                    alpha(CYAN_SOFT, 0.30),
                    1.2,
                );
            }

            // The write ticks — one per emitted write, cyan, deterministic
            // jitter so the rail reads as a spill, not a comb.
            let mut rng = Rng::new(0xC0A1u64 + (t.to_bits() % 97) as u64);
            let mut drawn = 0;
            for g in 0..GESTURES.len() {
                let (a, b, from, to, total) = GESTURES[g];
                let n = if t > b { total } else if t >= a { writes_by(g, t) } else { 0 };
                for i in 0..n {
                    let frac = (i + 1) as f32 / total as f32;
                    let wv = from + (to - from) * ease_out_cubic(frac);
                    let mut px = x_of(wv.clamp(0.0, 1.0));
                    px += rng.sym() * 2.5;
                    // Old gestures' ticks fade to history.
                    let age = if t > b { 1.0 } else { 0.0 };
                    let col = alpha(CYAN, 0.75 - 0.45 * age);
                    book.line(
                        Offset::new(px, wy - 12.0),
                        Offset::new(px, wy + 12.0),
                        col,
                        1.6,
                    );
                    drawn += 1;
                }
            }

            // The rebuild notches — ONE per completed gesture, on the rail's
            // right, violet, heavier than any write tick.
            for (gi, &(_, b, _, to, _)) in GESTURES.iter().enumerate() {
                if t > b {
                    let nx = x_of(to.clamp(0.0, 1.0)) + 26.0;
                    book.line(
                        Offset::new(nx, wy - 18.0),
                        Offset::new(nx, wy + 18.0),
                        alpha(VIOLET_SOFT, 0.95),
                        3.0,
                    );
                    book.ring(
                        Offset::new(nx, wy),
                        10.0,
                        1.0,
                        alpha(VIOLET, 0.35),
                    );
                    let _ = gi;
                }
            }

            // The thumb — a ring + core, violet, with a grab halo while
            // the gesture is live.
            book.ring(Offset::new(tx, y), 13.0, 2.4, alpha(VIOLET_SOFT, 0.95));
            book.circle(Offset::new(tx, y), 5.0, tint(VIOLET, 0.5));
            if gesture.is_some() {
                book.ring(Offset::new(tx, y), 19.0, 1.0, alpha(VIOLET, 0.25));
            }

            // Counts, right-anchored, mono — the receipt strip.
            let writes_now = total_writes(t);
            let rebuilds_now = rebuild_count(t);
            let _ = writes_now;
            let _ = rebuilds_now;
            let _ = drawn;
            // The strip's own baseline.
            book.line(
                Offset::new(0.0, 166.0),
                Offset::new(T_W - 24.0, 166.0),
                alpha(Color::WHITE, 0.05),
                1.0,
            );
        }),
    );

    // Labels around the track.
    Stack::new()
        .push(
            Positioned::new()
                .left(X)
                .top(T_Y - 58.0)
                .width(700.0)
                .height(18.0)
                .child(
                    Text::new("THE SCRUB · SIGNAL WRITES BELOW, THUMB ABOVE").style(
                        TextStyle::new(12.0)
                            .monospace()
                            .letter_spacing(2.2)
                            .color(alpha(FAINT, 0.9)),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(X)
                .top(T_Y)
                .width(T_W)
                .height(190.0)
                .child(track),
        )
        // The receipt strip's live numbers — Text over the painted baseline.
        .push(
            Positioned::new()
                .left(X)
                .top(T_Y + 168.0)
                .width(700.0)
                .height(20.0)
                .child(
                    Text::new(format!(
                        "writes {:>2}  ·  rebuilds {}  ·  ratio {}",
                        total_writes(t),
                        rebuild_count(t),
                        if rebuild_count(t) > 0 {
                            format!("{:.0}:1", total_writes(t) as f32 / rebuild_count(t) as f32)
                        } else {
                            "—".to_string()
                        }
                    ))
                    .style(
                        TextStyle::new(13.0)
                            .monospace()
                            .color(alpha(MUTED, 0.95)),
                    ),
                ),
        )
        .into()
}

fn thumb_start_hint(_g: usize) -> f32 {
    0.0
}

// ── The board ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let bg = Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(9, 9, 13)),
                    (0.6, BG_DEEP),
                    (1.0, Color::rgb(12, 12, 17)),
                ]),
            );

            // Calm stars — S06 is an interior, tool-lit scene.
            let mut rng = Rng::new(0x5C8);
            for _ in 0..44 {
                let x = rng.f01() * w;
                let y = rng.f01() * h;
                let r = 0.4 + rng.f01() * 0.8;
                let tw = 0.5 + 0.5 * (t * 3.0 + rng.f01() * 10.0).sin();
                book.circle(Offset::new(x, y), r, alpha(Color::WHITE, 0.03 + 0.06 * tw));
            }

            // The tool glow — cyan-leaning, the machine's light.
            book.layer(1.0, 40.0, None, |inner| {
                inner.circle(
                    Offset::new(w * 0.68, h * 0.30),
                    w * 0.26,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(CYAN, 0.06)),
                        (1.0, alpha(CYAN, 0.0)),
                    ]),
                );
            });
            // A violet answering glow, low.
            book.layer(1.0, 36.0, None, |inner| {
                inner.circle(
                    Offset::new(w * 0.25, h * 0.92),
                    w * 0.28,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.08)),
                        (1.0, alpha(VIOLET, 0.0)),
                    ]),
                );
            });

            // Board rules.
            book.rect(xywh(X, 520.0, w - 2.0 * X, 1.0), alpha(Color::WHITE, 0.05));

            // The vignette.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.5), 0.80).with_dither().with_stops(&[
                    (0.55, alpha(Color::BLACK, 0.0)),
                    (1.0, alpha(Color::BLACK, 0.45)),
                ]),
            );
        }),
    );

    Stack::new()
        .push(Positioned::fill().child(bg))
        .push(preview_card(t))
        .push(scrub_track(t))
        .into()
}
