//! exp_morph — *E-06: say → rust morph, state held.* The studio's
//! language switch — the author's words becoming the machine's code
//! without the session ever blinking.
//!
//! S04's beat: the author writes in **say** (natural language — "show
//! the counter in violet") and the studio compiles it to **rust**
//! (`Text::new(counter).style(violet)`). The morph is not a cut: a **scan
//! line** sweeps down the panel; above it, the say-line dissolves upward;
//! below it, the rust-line rises in. Line by line, the language switches.
//!
//! And through the whole switch — **state held**: the live element under
//! the code (the counter chip itself, ticking on the 24-in-60 cadence,
//! its violet marker lit) **never changes, never flickers, never
//! rebuilds**. The session's continuity is drawn as a continuous thing:
//! a horizontal **state line** runs from the first frame to the last,
//! unbroken, with the counter riding it. The graph's own words: *the
//! studio's language switch · state held.*
//!
//! The receipt, bottom-right: the say-line and rust-line **token counts**
//! as they hand over (words → glyphs), printed live — the translation's
//! arithmetic, not a claim about it.

use vieww_foundation::{Color, FontWeight, Gradient, Offset, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, mix, smoothstep, spring_out, tint, held_24_in_60, xywh, BG_DEEP,
    CANVAS, CANVAS_W, FAINT, INK, MUTED, Rng, VIOLET, VIOLET_SOFT, AMBER,
};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

/// Left margin.
const X: f32 = 120.0;

// ── The language pair — one instruction, two notations ──────────────────────

/// The say-line: natural language, sentence rhythm.
const SAY_LINES: [&str; 3] = [
    "show the counter",
    "in violet",
    "and keep it live",
];

/// The rust-line: the same instruction, in the machine's words.
const RUST_LINES: [&str; 3] = [
    "Text::new(counter)",
    ".style(violet)",
    ".live(true)",
];

/// How many code lines (both notations have the same count).
const N_LINES: usize = 3;

/// When the scan sweeps: starts, per-line stagger, completes.
const MORPH_T0: f32 = 0.22;
const MORPH_SPAN: f32 = 0.42;

// ── The morph state ─────────────────────────────────────────────────────────

/// Scan position at `t`: the line index (fractional) the sweep is at.
fn scan_pos(t: f32) -> f32 {
    let u = clamp01((t - MORPH_T0) / MORPH_SPAN);
    u * (N_LINES as f32 + 0.4)
}

/// Per-line morph progress: 0 = say, 1 = rust.
fn line_morph(i: usize, t: f32) -> f32 {
    let s = scan_pos(t);
    let pos = i as f32 + 0.5;
    // Each line morphs as the scan crosses IT, over ~0.8 line-heights.
    clamp01((s - pos + 0.4) / 0.8)
}

/// The scan line's own alpha — visible while sweeping, gone at rest.
fn scan_alpha(t: f32) -> f32 {
    let u = clamp01((t - MORPH_T0) / MORPH_SPAN);
    if u <= 0.0 || u >= 1.0 {
        0.0
    } else {
        (u * 8.0).sin().abs().min(1.0) * 0.6 + 0.3
    }
}

// ── The code panel — both notations, the sweep between ──────────────────────

/// Panel geometry.
const P_X: f32 = 140.0;
const P_Y: f32 = 128.0;
const P_W: f32 = 640.0;
const P_H: f32 = 280.0;
const LINE_H: f32 = 56.0;

fn code_panel(t: f32) -> WidgetNode {
    let scan_y_rel = 44.0 + scan_pos(t) * LINE_H * 0.98;

    // Panel furniture + scan + line ghosts, painted.
    let panel = Painting::sized(
        Size::new(P_W, P_H),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            // The panel body.
            book.rrect(
                Rect::new(0.0, 0.0, P_W, P_H),
                12.0,
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, alpha(Color::rgb(19, 19, 25), 0.95)),
                    (1.0, alpha(Color::rgb(13, 13, 18), 0.96)),
                ]),
            );
            book.stroke_rrect(
                Rect::new(0.5, 0.5, P_W - 1.0, P_H - 1.0),
                12.0,
                alpha(Color::WHITE, 0.10),
                1.0,
            );

            // Mode tabs at the top — say (amber) and rust (violet). The
            // active tab brightens as the morph proceeds.
            let overall = clamp01((t - MORPH_T0) / MORPH_SPAN);
            book.rrect(
                xywh(16.0, 14.0, 64.0, 24.0),
                6.0,
                alpha(AMBER, 0.08 + 0.04 * (1.0 - overall)),
            );
            book.stroke_rrect(
                xywh(16.0, 14.0, 64.0, 24.0),
                6.0,
                alpha(AMBER, 0.25 + 0.45 * (1.0 - overall)),
                1.0,
            );
            book.rrect(
                xywh(88.0, 14.0, 64.0, 24.0),
                6.0,
                alpha(VIOLET, 0.08 + 0.04 * overall),
            );
            book.stroke_rrect(
                xywh(88.0, 14.0, 64.0, 24.0),
                6.0,
                alpha(VIOLET, 0.25 + 0.45 * overall),
                1.0,
            );

            // The gutter — line numbers, one per code line.
            for i in 0..N_LINES {
                let ly = 44.0 + i as f32 * LINE_H + LINE_H * 0.5;
                book.rect(
                    xywh(30.0, ly - 1.0, 22.0, 1.0),
                    alpha(Color::WHITE, 0.07),
                );
            }

            // The scan line — a bright horizontal rule with a bloom, riding
            // the sweep. Its trailing gradient above (already-morphed).
            let sa = scan_alpha(t);
            if sa > 0.01 {
                let sy = scan_y_rel;
                book.rect(
                    xywh(24.0, sy, P_W - 48.0, 2.0),
                    alpha(VIOLET_SOFT, 0.85 * sa),
                );
                book.rect(
                    xywh(24.0, sy - 3.0, P_W - 48.0, 8.0),
                    Gradient::vertical().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.0)),
                        (1.0, alpha(VIOLET, 0.10 * sa)),
                    ]),
                );
                // The scan head — a diamond at the left edge.
                book.circle(Offset::new(26.0, sy + 1.0), 4.0, alpha(tint(VIOLET, 0.5), 0.9 * sa));
            }
        }),
    );

    // The code lines — each a pair (say below-fading-up, rust
    // rising-in), crossfaded by the line's own morph progress.
    let mut lines = Stack::new();
    for i in 0..N_LINES {
        let m = line_morph(i, t);
        let ly = P_Y + 44.0 + i as f32 * LINE_H;
        // The say-line: warm muted, dissolving upward as m → 1.
        if m < 0.999 {
            let slide = -14.0 * m;
            lines = lines.push(
                Positioned::new()
                    .left(P_X + 66.0)
                    .top(ly + slide)
                    .width(540.0)
                    .height(40.0)
                    .child(
                        Opacity::new(1.0 - m).child(
                            Text::new(SAY_LINES[i]).style(
                                TextStyle::new(24.0)
                                    .weight(FontWeight::Regular)
                                    .color(alpha(AMBER, 0.75)),
                            ),
                        ),
                    ),
            );
        }
        // The rust-line: mono, syntax-toned, rising from below.
        if m > 0.001 {
            let slide = 14.0 * (1.0 - m);
            lines = lines.push(
                Positioned::new()
                    .left(P_X + 66.0)
                    .top(ly + slide)
                    .width(540.0)
                    .height(40.0)
                    .child(
                        Opacity::new(m).child(
                            Text::new(RUST_LINES[i]).style(
                                TextStyle::new(21.0)
                                    .monospace()
                                    .weight(FontWeight::Medium)
                                    .color(alpha(INK, 0.92)),
                            ),
                        ),
                    ),
            );
        }
    }

    // Tab labels — drawn crisp over the panel.
    let tabs = Stack::new()
        .push(
            Positioned::new()
                .left(P_X + 16.0)
                .top(P_Y + 17.0)
                .width(64.0)
                .height(18.0)
                .child(
                    Text::new("say").style(
                        TextStyle::new(12.0)
                            .monospace()
                            .color(alpha(tint(AMBER, 0.3), 0.55 + 0.4 * (1.0 - clamp01((t - MORPH_T0) / MORPH_SPAN)))),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(P_X + 88.0)
                .top(P_Y + 17.0)
                .width(64.0)
                .height(18.0)
                .child(
                    Text::new("rust").style(
                        TextStyle::new(12.0)
                            .monospace()
                            .color(alpha(VIOLET_SOFT, 0.55 + 0.4 * clamp01((t - MORPH_T0) / MORPH_SPAN))),
                    ),
                ),
        );

    Stack::new()
        .push(
            Positioned::new()
                .left(P_X)
                .top(P_Y - 26.0)
                .width(700.0)
                .height(18.0)
                .child(
                    Text::new("THE STUDIO · LANGUAGE SWITCH · ONE INSTRUCTION, TWO NOTATIONS").style(
                        TextStyle::new(12.0)
                            .monospace()
                            .letter_spacing(2.2)
                            .color(alpha(FAINT, 0.9)),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(P_X)
                .top(P_Y)
                .width(P_W)
                .height(P_H)
                .child(panel),
        )
        .push(lines)
        .push(tabs)
        .into()
}

// ── The state line — continuity made visible ────────────────────────────────

/// The live element: a counter chip riding an unbroken line across the
/// bottom third. It ticks on the held 24-in-60 cadence THE WHOLE TIME —
/// before, during, and after the morph. State held.
fn state_line(t: f32) -> WidgetNode {
    // The counter: minutes advance on the wait's cadence — never reset.
    let held = held_24_in_60(t * SECONDS);
    let minute = 17 + ((held / 0.9) as i32).min(6);
    let text = format!("1,852 h {:02} m", minute);

    let line = Painting::sized(
        Size::new(CANVAS_W, 130.0),
        PaintWith::new(move |book: &mut Sketchbook, sz: Size| {
            let w = sz.width;
            let y = 64.0;

            // The state line — unbroken, edge to edge. THE continuity.
            book.line(
                Offset::new(0.0, y),
                Offset::new(w, y),
                alpha(VIOLET, 0.28),
                1.4,
            );
            // A brighter core over the center span.
            book.line(
                Offset::new(X * 0.4, y),
                Offset::new(w - X * 0.4, y),
                alpha(VIOLET_SOFT, 0.35),
                1.4,
            );

            // The chip's anchor point — a lit node on the line.
            book.ring(Offset::new(w * 0.44, y), 9.0, 2.0, alpha(VIOLET_SOFT, 0.8));
            book.circle(Offset::new(w * 0.44, y), 3.5, tint(VIOLET, 0.5));

            // Pulse ticks along the line — the session's heartbeat, a tick
            // per held 24 Hz step. These NEVER stop for the morph.
            let beat = (t * SECONDS * 24.0) as i32;
            for b in beat.saturating_sub(30)..=beat {
                if b < 0 {
                    continue;
                }
                let bx = X * 0.4 + (b as f32 / (SECONDS * 24.0)) * (w - X * 0.8);
                book.rect(
                    xywh(bx, y - 3.0, 1.0, 6.0),
                    alpha(VIOLET, 0.10 + 0.14 * ((b == beat) as i32 as f32)),
                );
            }
        }),
    );

    // The chip itself — glass, violet marker, the live counter.
    let chip = Stack::new()
        .push(Positioned::fill().child(
            Painting::sized(
                Size::new(250.0, 60.0),
                PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                    book.rrect(
                        Rect::new(0.0, 0.0, 250.0, 60.0),
                        10.0,
                        Gradient::vertical().with_dither().with_stops(&[
                            (0.0, alpha(Color::rgb(26, 24, 38), 0.8)),
                            (1.0, alpha(Color::rgb(16, 15, 24), 0.85)),
                        ]),
                    );
                    book.stroke_rrect(
                        Rect::new(0.5, 0.5, 249.0, 59.0),
                        10.0,
                        alpha(VIOLET_SOFT, 0.30),
                        1.2,
                    );
                    // The marker dot — lit the entire experiment.
                    book.circle(Offset::new(26.0, 30.0), 5.0, VIOLET);
                    book.ring(Offset::new(26.0, 30.0), 9.0, 1.4, alpha(VIOLET, 0.4));
                }),
            ),
        ))
        .push(
            Positioned::new()
                .left(44.0)
                .top(10.0)
                .width(200.0)
                .height(26.0)
                .child(
                    Text::new(text).style(
                        TextStyle::new(20.0)
                            .monospace()
                            .weight(FontWeight::Medium)
                            .color(alpha(INK, 0.92)),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(44.0)
                .top(37.0)
                .width(200.0)
                .height(15.0)
                .child(
                    Text::new("SESSION CLOCK · NEVER RESET").style(
                        TextStyle::new(9.5)
                            .monospace()
                            .letter_spacing(1.6)
                            .color(alpha(MUTED, 0.8)),
                    ),
                ),
        );

    Stack::new()
        .push(
            Positioned::new()
                .left(0.0)
                .top(470.0)
                .width(CANVAS_W)
                .height(130.0)
                .child(line),
        )
        .push(
            Positioned::new()
                .left(CANVAS_W * 0.44 - 125.0)
                .top(502.0)
                .width(250.0)
                .height(60.0)
                .child(chip),
        )
        // The "state held" annotation — the graph's own words.
        .push(
            Positioned::new()
                .left(CANVAS_W * 0.44 + 150.0)
                .top(516.0)
                .width(300.0)
                .height(18.0)
                .child(
                    Text::new("state held — it never rebuilt").style(
                        TextStyle::new(13.0)
                            .monospace()
                            .color(alpha(VIOLET_SOFT, 0.75)),
                    ),
                ),
        )
        .into()
}

// ── The receipt — the translation's arithmetic ──────────────────────────────

fn receipt(t: f32) -> WidgetNode {
    let overall = clamp01((t - MORPH_T0) / MORPH_SPAN);
    let say_words: usize = SAY_LINES.join(" ").split_whitespace().count();
    let rust_glyphs: usize = RUST_LINES.concat().chars().filter(|c| !c.is_whitespace()).count();

    Stack::new()
        .push(
            Positioned::new()
                .left(836.0)
                .top(470.0)
                .width(340.0)
                .height(16.0)
                .child(
                    Text::new("THE HANDOVER · LIVE COUNT").style(
                        TextStyle::new(11.0)
                            .monospace()
                            .letter_spacing(2.0)
                            .color(alpha(FAINT, 0.9)),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(836.0)
                .top(490.0)
                .width(340.0)
                .height(20.0)
                .child(
                    Text::new(format!(
                        "{} words → {} glyphs",
                        say_words, rust_glyphs
                    ))
                    .style(
                        TextStyle::new(14.0)
                            .monospace()
                            .color(alpha(MUTED, 0.95)),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(836.0)
                .top(514.0)
                .width(340.0)
                .height(20.0)
                .child(
                    Text::new(format!(
                        "morph {:.0}%",
                        overall * 100.0
                    ))
                    .style(
                        TextStyle::new(14.0)
                            .monospace()
                            .color(alpha(VIOLET_SOFT, 0.8)),
                    ),
                ),
        )
        .into()
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

            // Stars.
            let mut rng = Rng::new(0x606);
            for _ in 0..46 {
                let x = rng.f01() * w;
                let y = rng.f01() * h;
                let r = 0.4 + rng.f01() * 0.8;
                let tw = 0.5 + 0.5 * (t * 3.0 + rng.f01() * 10.0).sin();
                book.circle(Offset::new(x, y), r, alpha(Color::WHITE, 0.03 + 0.06 * tw));
            }

            // Twin glows: amber low-left (the author's warmth), violet
            // high-right (the machine's).
            book.layer(1.0, 38.0, None, |inner| {
                inner.circle(
                    Offset::new(w * 0.22, h * 0.78),
                    w * 0.24,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(AMBER, 0.06)),
                        (1.0, alpha(AMBER, 0.0)),
                    ]),
                );
            });
            book.layer(1.0, 36.0, None, |inner| {
                inner.circle(
                    Offset::new(w * 0.78, h * 0.22),
                    w * 0.24,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.08)),
                        (1.0, alpha(VIOLET, 0.0)),
                    ]),
                );
            });

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
        .push(code_panel(t))
        .push(state_line(t))
        .push(receipt(t))
        .into()
}
