//! exp_settle — *the text-kinetics family, II.* The settle plate.
//!
//! A sentence arrives out of noise: each glyph churns through a scramble
//! pool on a 14 Hz clock (deterministically — the pool index is a pure
//! function of glyph and cycle, so the churn re-renders to the byte), then
//! settles at its own appointed time, left to right with per-glyph jitter.
//! The settle is a spring pop — font-size modulation through
//! [`spring_out`], the analytic underdamped closed form — and the wave's
//! position is *printed*, as the leftmost unsettled glyph.
//!
//! Once the line is quiet, the underline strokes itself in by dash phase —
//! the E-17 technique, the film's own session-line grammar, on a plain
//! segment where the perimeter is honest geometry.
//!
//! The second line — the lab's motto — types on with a caret, glyph by
//! glyph, each rising 4px as it lands. Every number in the receipt is
//! computed by the same functions that drew the frame: the churn census, the
//! settle times, the wave position, the underline's drawn fraction.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, StrokeStyle, Dash,
    TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{alpha, clamp01, mix, spring_out, BG_DEEP, CANVAS, FAINT, INK, MUTED,
    VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 10.0;

/// The sentence the noise resolves into.
const LINE1: &str = "every frame is a receipt";

/// The motto, typed on once the line settles.
const LINE2: &str = "printed, never guessed";

/// The scramble pool — symbols that read as "signal not yet a letter".
const POOL: &[u8] = b"#%&*+=<>/{}[]0123456789";

/// Glyph churn rate, cycles per second.
const CHURN_HZ: f32 = 14.0;

/// The settle wave: first settle time, and the span it sweeps in t.
const SETTLE_T0: f32 = 0.08;
const SETTLE_SPAN: f32 = 0.42;

/// The spring each glyph lands on.
const SPRING_OMEGA: f32 = 16.0;
const SPRING_ZETA: f32 = 0.60;

// ── Layout ──────────────────────────────────────────────────────────────────

/// The big line's em size.
const EM1: f32 = 54.0;

/// Monospace advance at `EM1` — layout geometry, not a measurement claim.
const ADV1: f32 = 33.0;

const X0: f32 = 96.0;
const BASE1_Y: f32 = 296.0;
const LINE2_Y: f32 = 392.0;
const UNDERLINE_Y: f32 = 330.0;

/// The motto's em size and advance.
const EM2: f32 = 26.0;
const ADV2: f32 = 16.0;
const LINE2_T0: f32 = 0.60;
const LINE2_SPAN: f32 = 0.30;

// ── The settle field — pure functions of (glyph, t) ─────────────────────────

/// Glyph `i`'s appointed settle time, with deterministic jitter.
#[must_use]
fn settle_at(i: usize) -> f32 {
    let frac = i as f32 / LINE1.chars().count() as f32;
    let mut rng = crate::film_lib::Rng::new(0x51E7E + i as u64);
    let jitter = (rng.f01() - 0.5) * 0.045;
    SETTLE_T0 + frac * SETTLE_SPAN + jitter
}

/// The scramble character glyph `i` shows at churn cycle `c`.
#[must_use]
fn churn_char(i: usize, c: u64) -> char {
    let mut rng = crate::film_lib::Rng::new(0xBEEF + i as u64 * 131 + c);
    POOL[(rng.f01() * POOL.len() as f32) as usize % POOL.len()] as char
}

/// How many churn cycles glyph `i` consumed before its settle — the census.
#[must_use]
fn cycles_burned(i: usize) -> u64 {
    ((settle_at(i) * SECONDS) * CHURN_HZ).floor().max(0.0) as u64
}

/// The settle wave's position: the leftmost glyph still churning.
#[must_use]
fn wave_front(t: f32) -> usize {
    let mut front = LINE1.chars().count();
    for (i, ch) in LINE1.chars().enumerate() {
        if ch != ' ' && t < settle_at(i) {
            front = front.min(i);
        }
    }
    front
}

// ── The board ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let n1 = LINE1.chars().count();
    let all_settled = (0..n1).all(|i| t >= settle_at(i));
    let line2_t = clamp01((t - LINE2_T0) / LINE2_SPAN);
    let typed = ((line2_t * LINE2.chars().count() as f32).floor() as usize).min(LINE2.chars().count());

    let board = Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — quiet; the type is the plate.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(8, 8, 11)),
                    (1.0, BG_DEEP),
                ]),
            );
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.34, 0.44), 0.75).with_dither().with_stops(&[
                    (0.0, alpha(VIOLET_SOFT, 0.07)),
                    (1.0, alpha(VIOLET_SOFT, 0.0)),
                ]),
            );

            // The ghost target — the sentence, always faintly present: the
            // field the noise is resolving toward. (A Text widget below, at
            // a whisper.)

            // The underline — dash-phase stroke-on once the line is quiet
            // (E-17: the pattern is [drawn, longer-than-path], phase 0).
            if all_settled {
                let u = clamp01((t - (settle_at(n1 - 1) + 0.10)) / 0.18);
                if u > 0.0 {
                    let total = n1 as f32 * ADV1;
                    let style =
                        StrokeStyle::default().dash(Dash::new(vec![total * u, total + 1.0]));
                    book.stroke_styled(
                        {
                            let mut p = Path::new();
                            p.move_to(Offset::new(X0, UNDERLINE_Y));
                            p.line_to(Offset::new(X0 + total, UNDERLINE_Y));
                            p
                        },
                        alpha(VIOLET_SOFT, 0.95),
                        2.2,
                        style,
                    );
                }
            }
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));

    // The ghost target line — the sentence at a whisper, always.
    stack = stack.push(
        Positioned::new()
            .left(X0)
            .top(BASE1_Y - 60.0)
            .width(1200.0)
            .height(70.0)
            .child(
                Text::new(LINE1.to_string()).style(
                    TextStyle::new(EM1)
                        .monospace()
                        .letter_spacing(ADV1 - EM1 * 0.60)
                        .color(alpha(FAINT, 0.17)),
                ),
            ),
    );

    // ── The big line: one Text per glyph, scramble → settle → spring ─────
    for (i, ch) in LINE1.chars().enumerate() {
        let settled = t >= settle_at(i);
        let (shown, color, dy, em) = if settled {
            let u = clamp01((t - settle_at(i)) / 0.24);
            let pop = spring_out(u, SPRING_OMEGA, SPRING_ZETA);
            // Font-size modulation as the pop: the glyph lands at 1.0 with
            // the spring's overshoot riding the size, anchored so the
            // overshoot reads as growth from the baseline.
            let scale = 0.82 + 0.18 * pop;
            let c = mix(MUTED, INK, clamp01(u * 1.6));
            (ch.to_string(), alpha(c, 0.75 + 0.25 * clamp01(u * 2.0)), 0.0, EM1 * scale)
        } else if ch == ' ' {
            (String::new(), MUTED, 0.0, EM1)
        } else {
            // The churn — 14 Hz, deterministic, with the restless wave.
            let cycle = ((t * SECONDS) * CHURN_HZ).floor().max(0.0) as u64;
            let shown = churn_char(i, cycle).to_string();
            let wave = (t * SECONDS * 9.0 + i as f32 * 0.55).sin() * 4.5;
            (shown, alpha(MUTED, 0.80), wave, EM1)
        };

        if shown.is_empty() {
            continue;
        }
        // Baseline-anchored: the pop's size growth lifts the glyph's top,
        // and the y offset compensates so the baseline stays put.
        let top = BASE1_Y - em + dy;
        stack = stack.push(
            Positioned::new()
                .left(X0 + i as f32 * ADV1)
                .top(top)
                .width(ADV1 + 8.0)
                .height(em + 12.0)
                .child(
                    Text::new(shown).style(
                        TextStyle::new(em)
                            .monospace()
                            .letter_spacing(0.0)
                            .color(color),
                    ),
                ),
        );
    }

    // ── The motto: typed on, rising, with a caret ─────────────────────────
    let n2 = LINE2.chars().count();
    for (i, ch) in LINE2.chars().enumerate() {
        let on = i < typed;
        let c = if on {
            let u = clamp01((line2_t * n2 as f32 - i as f32).max(0.0) / 0.8);
            let rise = (1.0 - u) * 4.0;
            (ch.to_string(), alpha(mix(MUTED, INK, u), 0.45 + 0.55 * u), rise)
        } else {
            (String::new(), MUTED, 4.0)
        };
        if c.0.is_empty() {
            continue;
        }
        stack = stack.push(
            Positioned::new()
                .left(X0 + i as f32 * ADV2)
                .top(LINE2_Y + c.2)
                .width(ADV2 + 4.0)
                .height(EM2 + 6.0)
                .child(
                    Text::new(c.0).style(
                        TextStyle::new(EM2).monospace().color(c.1),
                    ),
                ),
        );
    }
    // The caret — blinking at the typing frontier.
    if line2_t > 0.0 && typed < n2 + 1 {
        let blink = ((t * SECONDS * 2.4).sin() + 1.0) * 0.5;
        let caret = Painting::sized(
            Size::new(10.0, EM2 + 4.0),
            PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                book.rect(
                    Rect::new(0.0, 0.0, 2.2, EM2 + 2.0),
                    alpha(VIOLET_SOFT, 0.45 + 0.55 * blink),
                );
            }),
        );
        stack = stack.push(
            Positioned::new()
                .left(X0 + typed as f32 * ADV2 + 2.0)
                .top(LINE2_Y + 2.0)
                .width(10.0)
                .height(EM2 + 4.0)
                .child(caret),
        );
    }

    stack.push(receipt_panel(t)).into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(t: f32) -> WidgetNode {
    let n1 = LINE1.chars().count();
    let settled = (0..n1).filter(|&i| t >= settle_at(i)).count();
    let front = wave_front(t);
    let burned: u64 = (0..n1).map(cycles_burned).sum();
    let line2_t = clamp01((t - LINE2_T0) / LINE2_SPAN);
    let typed = ((line2_t * LINE2.chars().count() as f32).floor() as usize)
        .min(LINE2.chars().count());
    let underline = if (0..n1).all(|i| t >= settle_at(i)) {
        clamp01((t - (settle_at(n1 - 1) + 0.10)) / 0.18)
    } else {
        0.0
    };

    const P_X: f32 = 42.0;
    const P_Y: f32 = 560.0;
    const P_W: f32 = 380.0;

    let lines = [
        "SETTLE · TEXT KINETICS II · SCRAMBLE → SETTLE".to_string(),
        format!("line {} glyphs · settled {} · wave front {}", n1, settled, front),
        format!("churn {:.0} Hz · cycles burned {}", CHURN_HZ, burned),
        format!("underline {:.0}% · motto typed {}/{}", underline * 100.0, typed,
            LINE2.chars().count()),
        format!("spring ω {:.0} ζ {:.2} (analytic)", SPRING_OMEGA, SPRING_ZETA),
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

    // The instrument: the settle timeline — a dot per glyph at its settle
    // time, the settled ones lit, a playhead at t.
    let timeline = Painting::sized(
        Size::new(P_W, 72.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            book.rrect(
                Rect::new(0.0, 0.0, P_W, 72.0),
                10.0,
                alpha(Color::rgb(16, 16, 21), 0.88),
            );
            book.stroke_rrect(
                Rect::new(0.0, 0.0, P_W, 72.0),
                10.0,
                alpha(Color::WHITE, 0.08),
                1.0,
            );
            let x_of = |tt: f32| -> f32 { 14.0 + tt.clamp(0.0, 1.0) * (P_W - 28.0) };
            let y = 30.0;

            // The wave's own track.
            book.line(
                Offset::new(x_of(0.0), y),
                Offset::new(x_of(1.0), y),
                alpha(FAINT, 0.25),
                1.0,
            );
            for i in 0..n1 {
                let st = settle_at(i);
                let lit = t >= st;
                book.circle(
                    Offset::new(x_of(st), y),
                    if lit { 3.4 } else { 2.2 },
                    if lit { INK } else { alpha(FAINT, 0.5) },
                );
            }
            // The playhead.
            book.line(
                Offset::new(x_of(t), 16.0),
                Offset::new(x_of(t), 44.0),
                alpha(VIOLET_SOFT, 0.9),
                1.4,
            );
            book.rect(
                Rect::new(14.0, 54.0, 14.0 + (P_W - 28.0) * t, 58.0),
                alpha(VIOLET_SOFT, 0.35),
            );
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 100.0)
            .width(P_W)
            .height(72.0)
            .child(timeline),
    );

    stack.into()
}
