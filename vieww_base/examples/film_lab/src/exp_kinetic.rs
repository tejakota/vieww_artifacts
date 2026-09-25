//! exp_kinetic — *the text-kinetics family.* Type, tick, roll, count.
//!
//! The film is narrated by numbers and questions; this experiment builds the
//! whole kinetic-typography vocabulary in one board, four bands playing
//! simultaneously (the lab audits families, the film stages them full-screen):
//!
//! - **E-02 · type-on** (band 1): the S02 question typed character by
//!   character, mono face, a spring caret that overshoots each landing, then
//!   the settle — letter-spacing collapses from loose to tight and the weight
//!   crossfades Regular → Medium, with an overshooting underline (E-04's
//!   technique at headline scale).
//! - **E-01 · the hour-counter** (band 2): the wait's own counter, ticking
//!   minutes on a **24-in-60 held cadence** — the tick lands only on held
//!   frames; the judder is the mechanism, not a rate change. Muted, degraded,
//!   dust motes drifting: the wait's aesthetic, crafted.
//! - **The ladder** (band 3): the witness counter 0 → 7 as an odometer — each
//!   human touch rolls one digit on its own spring; the step dots below are
//!   the ladder itself (say · rust · composed · carried · damage · desktop ·
//!   phone), one number per touch, never reset.
//! - **S11 · the receipts** (band 4): 59.3 fps · 4.09 ms · 10,800 frames
//!   counting up from zero on staggered springs. 59.3/4.09 arrive from CI
//!   artifacts at master render (the lab pins their targets); 10,800 is
//!   *derived*: 180 s × 60 fps. No number typed by a human.
//!
//! Every glyph is the framework's own text stack — `Text`, `TextStyle`
//! weights, letter-spacing, monospace — rendered through the native
//! rasterizer. The typographic grid is left-aligned at x = 120, matching the
//! sheets' stat typography.

use vieww_foundation::{Color, FontWeight, Gradient, Offset, Rect, Size, Sketchbook, TextStyle, TextAlign};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, mix, spring_out, tint, xywh, BG_DEEP, CANVAS, FAINT, INK, MUTED, Rng, VIOLET,
    VIOLET_SOFT,
};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 14.0;

// The film's own derived quantities — the receipt culture, in constants.
/// 59.3 fps median, measured by ci/mobile/device-suite.sh on the Redmi.
const FPS_MEDIAN: f32 = 59.3;
/// 4.09 ms median frame time, same suite, same device.
const MS_MEDIAN: f32 = 4.09;
/// The master's frame count, derived: 3:00 at 60 fps. Never typed.
const MASTER_FRAMES: f32 = 180.0 * 60.0;

/// The S02 question, verbatim from the graph.
const QUESTION: &str = "how long should it take to see what you built?";

/// Left margin — the typographic grid.
const X: f32 = 120.0;

/// DejaVu Sans Mono's advance: 0.60205 em. Layout arithmetic, not a caption.
fn mono_advance(size: f32) -> f32 {
    size * 0.60205
}

/// Numeric lerp — the scalar shadow of the palette's `mix`.
fn lerp_f(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * clamp01(t)
}

/// 10800 → "10,800". Comma-grouping for the receipts row.
fn group_commas(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Band labels: the lab's bookkeeping voice — mono, tracked wide, faint.
fn band_label(text: &str, y: f32) -> WidgetNode {
    Positioned::new()
        .left(X)
        .top(y)
        .width(900.0)
        .height(18.0)
        .child(
            Text::new(text)
                .style(
                    TextStyle::new(12.0)
                        .monospace()
                        .letter_spacing(3.0)
                        .color(alpha(FAINT, 0.85)),
                )
                .align(TextAlign::Left),
        )
        .into()
}

// ── Band 1 · E-02 type-on + the settle ──────────────────────────────────────

fn band_typeon(t: f32) -> WidgetNode {
    let size = 28.0;
    let adv = mono_advance(size);
    let total_chars = QUESTION.chars().count() as f32;

    // The typing window, then the settle window.
    let type_t = clamp01(t / 0.26);
    let settle_t = clamp01((t - 0.28) / 0.22);

    // Typed count, eased so the rhythm feels human-sprung, not metronomic.
    let typed = (QUESTION.chars().count() as f32 * ease_out_type(type_t)).round() as usize;
    let typed = typed.min(QUESTION.chars().count());
    let visible: String = QUESTION.chars().take(typed).collect();

    // The settle: letter-spacing collapses loose → tight, weight crossfades.
    let spacing = lerp_f(6.0, 0.5, settle_t);
    let width_now = total_chars * (adv + spacing) - spacing;

    // The caret: rides the typing head, overshoots on each landing.
    let caret_x = X + visible.chars().count() as f32 * (adv + spacing);
    let land_blink = if t < 0.28 { blink(t * SECONDS) } else { 1.0 };
    let caret_alpha = if t < 0.30 { 0.9 * land_blink } else { 0.0 };

    let mut band = Stack::new();

    // The question — Regular crossfading to Medium through the settle.
    band = band
        .push(
            Positioned::new()
                .left(X - 1.0)
                .top(110.0)
                .width(1100.0)
                .height(44.0)
                .child(Opacity::new(1.0 - settle_t).child(
                    Text::new(visible.clone()).style(
                        TextStyle::new(size)
                            .monospace()
                            .letter_spacing(spacing)
                            .weight(FontWeight::Regular)
                            .color(alpha(INK, 0.92)),
                    ),
                )),
        )
        .push(
            Positioned::new()
                .left(X)
                .top(110.0)
                .width(1100.0)
                .height(44.0)
                .child(Opacity::new(settle_t).child(
                    Text::new(visible.clone()).style(
                        TextStyle::new(size)
                            .monospace()
                            .letter_spacing(spacing)
                            .weight(FontWeight::Medium)
                            .color(INK),
                    ),
                )),
        );

    // The caret — a plain rect, blinking on the 24-in-60 cadence while typing.
    if caret_alpha > 0.01 {
        band = band.push(
            Positioned::new()
                .left(caret_x)
                .top(114.0)
                .width(3.0)
                .height(32.0)
                .child(
                    Container::new()
                        .color(alpha(VIOLET_SOFT, caret_alpha))
                        .radius(1.5),
                ),
        );
    }

    // The underline — overshoot spring (E-04's grammar, headline scale),
    // a painted gradient bar (Container takes a flat color; the gradient
    // is worth a painting).
    if settle_t > 0.0 {
        let uw = (width_now + 8.0) * spring_out(settle_t, 9.0, 0.62);
        band = band.push(
            Positioned::new()
                .left(X)
                .top(158.0)
                .width(uw.max(0.0) + 2.0)
                .height(6.0)
                .child(Painting::sized(
                    Size::new(uw.max(0.0) + 2.0, 6.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        book.rrect(
                            Rect::new(0.0, 0.0, uw.max(0.0), 3.0),
                            1.5,
                            Gradient::horizontal().with_stops(&[
                                (0.0, alpha(VIOLET, 0.95)),
                                (1.0, alpha(VIOLET_SOFT, 0.1)),
                            ]),
                        );
                    }),
                )),
        );
    }

    Stack::new()
        .push(band_label("E-02 · TYPE-ON", 82.0))
        .push(band)
        .into()
}

/// Typing ease: bursts and pauses — a keystroke cadence, not a slider.
fn ease_out_type(t: f32) -> f32 {
    // Piecewise-fast: 3 bursts of speed separated by human pauses.
    let t = clamp01(t);
    let burst = |u: f32| 1.0 - (1.0 - u).powi(2);
    if t < 0.35 {
        burst(t / 0.35) * 0.34
    } else if t < 0.5 {
        0.34
    } else if t < 0.78 {
        0.34 + burst((t - 0.5) / 0.28) * 0.4
    } else if t < 0.86 {
        0.74
    } else {
        0.74 + burst((t - 0.86) / 0.14) * 0.26
    }
}

/// The wait's blink cadence — 24 Hz sampled inside 60.
fn blink(t: f32) -> f32 {
    let held = crate::film_lib::held_24_in_60(t);
    if (held * 2.4).fract() < 0.55 {
        1.0
    } else {
        0.0
    }
}

// ── Band 2 · E-01 the hour-counter ──────────────────────────────────────────

fn band_counter(t: f32) -> WidgetNode {
    // The counter's minutes advance only on held 24 Hz steps — E-01's tick.
    // The tick keeps going: the wait is long, and the lab sheet must show it.
    let held = crate::film_lib::held_24_in_60(t * SECONDS);
    let minute = 14 + ((held / 0.75) as i32).min(8);
    let text = format!("1852 h {:02} m", minute);

    // A soft flash exactly when the minute digit changes.
    let flash = if t < 0.02 { 0.0 } else { minute_flash(t) };

    let counter = Stack::new()
        .push(
            Positioned::new()
                .left(X)
                .top(258.0)
                .width(700.0)
                .height(56.0)
                .child(
                    Text::new(text).style(
                        TextStyle::new(40.0)
                            .monospace()
                            .weight(FontWeight::Regular)
                            .letter_spacing(1.0)
                            .color(mix(MUTED, tint(MUTED, 0.4), flash)),
                    ),
                ),
        )
        // The tick rail: 24 marks, the cadence the counter lives on.
        .push(
            Positioned::new()
                .left(X)
                .top(318.0)
                .width(980.0)
                .height(10.0)
                .child(Painting::sized(
                    Size::new(980.0, 10.0),
                    PaintWith::new(move |book: &mut Sketchbook, _size: Size| {
                        for i in 0..24 {
                            let x = i as f32 / 23.0 * 964.0;
                            let tall = i % 6 == 0;
                            book.rect(
                                xywh(x, if tall { 0.0 } else { 2.5 }, 2.0, if tall { 10.0 } else { 5.0 }),
                                alpha(FAINT, 0.4),
                            );
                        }
                    }),
                )),
        );

    // Dust motes — the wait's air, drifting slow (deterministic).
    let motes = Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let mut rng = Rng::new(0xE01);
            for _ in 0..9 {
                let bx = X + rng.f01() * 900.0;
                let by = 252.0 + rng.f01() * 74.0;
                let drift = (t * 9.0 + rng.f01() * 6.0).sin() * 7.0;
                let r = 0.7 + rng.f01() * 1.1;
                book.circle(Offset::new(bx + drift, by), r, alpha(MUTED, 0.16));
            }
            let _ = size;
        }),
    );

    Stack::new()
        .push(band_label("E-01 · THE HOUR-COUNTER · TICK HELD AT 24-IN-60", 228.0))
        .push(Positioned::fill().child(motes))
        .push(counter)
        .into()
}

/// Flash envelope: 1.0 on the frame the minute changes, decaying.
fn minute_flash(t: f32) -> f32 {
    let held = crate::film_lib::held_24_in_60(t * SECONDS);
    let m = (held / 0.75) as i32;
    let prev = crate::film_lib::held_24_in_60((t * SECONDS) - 0.2);
    let m_prev = (prev / 0.75) as i32;
    if m > m_prev {
        0.85
    } else {
        0.0
    }
}

// ── Band 3 · the ladder, an odometer ────────────────────────────────────────

/// The ladder's seven touches, each a (label, t_touch) pair — the graph's
/// §4.2 sequence, compressed into the experiment's window.
const LADDER: [(&str, f32); 7] = [
    ("say", 0.34),
    ("rust", 0.42),
    ("composed", 0.50),
    ("carried", 0.58),
    ("damage", 0.65),
    ("desktop", 0.72),
    ("phone", 0.80),
];

/// Ladder roll position at `t` — Σ per-step springs. Scrub-safe shadow of the
/// real SpringAnimation (interruptibility is proven in exp_mesh).
fn ladder_p(t: f32) -> f32 {
    let mut p = 0.0;
    for (_, t0) in LADDER {
        p += spring_out(clamp01((t - t0) / 0.34), 13.0, 0.58);
    }
    p
}

fn band_ladder(t: f32) -> WidgetNode {
    let p = ladder_p(t);
    let digit_size = 42.0;
    let digit_h = 50.0;

    // The odometer column: digits near the roll position, sliding under a clip.
    let base = p.floor() as i32;
    let mut column = Stack::new();
    for d in (base - 1)..=(base + 2) {
        if (0..=9).contains(&d) {
            let settle = d == 7 && p > 6.97;
            column = column.push(
                Positioned::new()
                    .left(0.0)
                    .top(d as f32 * digit_h)
                    .width(64.0)
                    .height(digit_h)
                    .child(
                        Text::new(d.to_string()).style(
                            TextStyle::new(digit_size)
                                .monospace()
                                .weight(if settle { FontWeight::Bold } else { FontWeight::Medium })
                                .color(if settle { VIOLET_SOFT } else { INK }),
                        ),
                    ),
            );
        }
    }

    let odometer = Clip::rect().child(
        Container::new()
            .size(64.0, digit_h)
            .child(Transformed::translate(Offset::new(0.0, -p * digit_h)).child(column)),
    );

    // The step rail: seven dots, filling left to right as the touches land.
    let rail = Painting::sized(
        Size::new(760.0, 60.0),
        PaintWith::new(move |book: &mut Sketchbook, _size: Size| {
            for (i, (label, _t0)) in LADDER.iter().enumerate() {
                let cx = 120.0 + 84.0 * i as f32;
                let cy = 30.0;
                let done = p >= i as f32 + 0.98;
                let arriving = clamp01((p - i as f32) / 0.98);
                // The node.
                book.circle(
                    Offset::new(cx, cy),
                    if done { 5.0 } else { 3.0 + arriving * 2.0 },
                    if done { alpha(VIOLET_SOFT, 0.95) } else { alpha(MUTED, 0.25 + arriving * 0.4) },
                );
                // The pulse ring on the currently-arriving touch.
                if !done && arriving > 0.0 {
                    let ring_r = 6.0 + 8.0 * (1.0 - arriving);
                    book.ring(
                        Offset::new(cx, cy),
                        ring_r,
                        1.4,
                        alpha(VIOLET, (1.0 - arriving) * 0.6),
                    );
                }
                // The tether to the next node.
                if i + 1 < LADDER.len() {
                    let seg = clamp01(p - i as f32);
                    if seg > 0.0 {
                        book.line(
                            Offset::new(cx + 6.0, cy),
                            Offset::new(cx + 84.0 - 6.0, cy),
                            alpha(VIOLET, 0.14),
                            1.2,
                        );
                        // The progress stroke over the tether.
                        book.line(
                            Offset::new(cx + 6.0, cy),
                            Offset::new(cx + 6.0 + 72.0 * seg, cy),
                            alpha(VIOLET_SOFT, 0.8),
                            1.8,
                        );
                    }
                }
                let _ = label;
            }
        }),
    );
    // Tiny ladder labels under the dots — the graph's own words.
    let labels = (0..7)
        .fold(Stack::new(), |acc, i| {
            let (label, _t0) = LADDER[i];
            let lit = p >= i as f32 + 0.98;
            acc.push(
                Positioned::new()
                    .left(120.0 + 84.0 * i as f32 - 34.0)
                    .top(458.0)
                    .width(100.0)
                    .height(17.0)
                    .child(
                        Text::new(label).style(
                            TextStyle::new(12.5)
                                .monospace()
                                .letter_spacing(0.5)
                                .color(if lit { alpha(INK, 0.8) } else { alpha(MUTED, 0.55) }),
                        ),
                    ),
            )
        });

    Stack::new()
        .push(band_label("THE WITNESS · ONE NUMBER PER TOUCH · NEVER RESET", 366.0))
        .push(
            Positioned::new()
                .left(X)
                .top(396.0)
                .width(64.0)
                .height(digit_h)
                .child(odometer),
        )
        .push(
            Positioned::new()
                .left(120.0 + 84.0 * 7.0)
                .top(396.0)
                .width(12.0)
                .height(digit_h)
                .child(
                    Text::new("→").style(TextStyle::new(digit_size).color(alpha(FAINT, 0.7))),
                ),
        )
        .push(Positioned::new().left(X - 44.0).top(404.0).width(760.0).height(60.0).child(rail))
        .push(labels)
        .into()
}

// ── Band 4 · S11 the receipts row ───────────────────────────────────────────

fn band_stats(t: f32) -> WidgetNode {
    let cells: [(f32, &str, u8); 3] = [
        (FPS_MEDIAN, "FPS MEDIAN · REDMI NOTE 7 PRO · CI", 1),
        (MS_MEDIAN, "MS MEDIAN · FRAME · CI", 2),
        (MASTER_FRAMES, "FRAMES · 3:00 AT 60 · DERIVED", 0),
    ];
    let starts = [0.58, 0.66, 0.74];

    let mut band = Stack::new();
    for (i, (value, label, decimals)) in cells.iter().enumerate() {
        let label = *label;
        let value = *value;
        let decimals = *decimals;
        let x = X + i as f32 * 420.0;
        let v = value * spring_out(clamp01((t - starts[i]) / 0.34), 12.0, 0.7);
        let text = if decimals == 0 {
            group_commas(v.round().max(0.0) as u32)
        } else {
            format!("{:.*}", decimals as usize, v)
        };
        let settled = v >= value * 0.999;
        band = band
            .push(
                Positioned::new()
                    .left(x)
                    .top(546.0)
                    .width(400.0)
                    .height(52.0)
                    .child(
                        Text::new(text).style(
                            TextStyle::new(38.0)
                                .monospace()
                                .weight(if settled { FontWeight::Bold } else { FontWeight::Medium })
                                .color(if settled { INK } else { alpha(MUTED, 0.9) }),
                        ),
                    ),
            )
            .push(
                Positioned::new()
                    .left(x)
                    .top(596.0)
                    .width(400.0)
                    .height(16.0)
                    .child(
                        Text::new(label).style(
                            TextStyle::new(11.0)
                                .monospace()
                                .letter_spacing(2.0)
                                .color(alpha(FAINT, 0.9)),
                        ),
                    ),
            );
    }

    Stack::new()
        .push(band_label("S11 · THE RECEIPTS · EVERY NUMBER MEASURED", 516.0))
        .push(band)
        .into()
}

// ── The board ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let bg = Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground: deep, faintly violet toward the floor.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(9, 9, 13)),
                    (0.65, BG_DEEP),
                    (1.0, Color::rgb(13, 11, 20)),
                ]),
            );

            // A starfield's worth of calm — sparse, low.
            let mut rng = Rng::new(0xBEEF);
            for _ in 0..36 {
                let x = rng.f01() * w;
                let y = rng.f01() * h;
                let r = 0.4 + rng.f01() * 0.8;
                let tw = 0.5 + 0.5 * (t * 4.0 + rng.f01() * 8.0).sin();
                book.circle(Offset::new(x, y), r, alpha(Color::WHITE, 0.03 + 0.08 * tw));
            }

            // The horizon glow under the receipts.
            book.layer(1.0, 42.0, None, |inner| {
                inner.circle(
                    Offset::new(w * 0.28, h * 0.94),
                    w * 0.30,
                    Gradient::radial_fill().with_stops(&[
                        (0.0, alpha(VIOLET, 0.10)),
                        (1.0, alpha(VIOLET, 0.0)),
                    ]),
                );
            });

            // Band rules — the board's grid, barely there.
            for y in [204.0, 342.0, 486.0] {
                book.rect(xywh(X, y, w - 2.0 * X, 1.0), alpha(Color::WHITE, 0.05));
            }

            // The vignette.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.5), 0.78).with_dither().with_stops(&[
                    (0.55, alpha(Color::BLACK, 0.0)),
                    (1.0, alpha(Color::BLACK, 0.45)),
                ]),
            );
        }),
    );

    Stack::new()
        .push(Positioned::fill().child(bg))
        .push(band_typeon(t))
        .push(band_counter(t))
        .push(band_ladder(t))
        .push(band_stats(t))
        .into()
}
