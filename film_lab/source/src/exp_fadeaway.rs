//! exp_fadeaway — *from text to a blurred fade-away.*
//!
//! The user's brief, verbatim: "from a text to a blurred fade away
//! transition". This is the transition-grammar prop: every way a sentence
//! can leave, choreographed as one continuous release, at **32-frame
//! sampling** (the sampled-attack lesson — a transition is exactly the
//! thing a 16-frame sheet lies about).
//!
//! Five stages, cross-faded:
//! - **HOLD** (t 0–0.12): the question, sharp. It breathes, barely.
//! - **SPLIT** (t 0.10–0.32): chromatic aberration grows — the line drawn
//!   three times (pure R / G / B) with radial offsets through **Plus**:
//!   white where aligned, fringes where not. The sharp ink rides beneath.
//! - **DEFOCUS** (t 0.28–0.55): the blur ramp — ONE blurred group (U-06
//!   discipline), σ 0→16, the line lifting as it softens, ink compensated
//!   (alpha raised as the blur eats it — the compensation printed live).
//! - **RELEASE** (t 0.50–0.86): per-letter dust — each glyph outline (U-03)
//!   breaks into a 3×4 grid of clipped windows (U-11 transform-outside /
//!   clip-inside), staggering left → right, drifting up and away.
//! - **GONE** (t 0.84–1.0): the baseline remains — one hairline, the memory
//!   of the line — and the count: every letter accounted for.
//!
//! Receipts: glyph count, shard count live, σ, chroma offset, letters
//! released.

use std::sync::OnceLock;

use vieww_foundation::{
    BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle, Transform,
};
use vieww_paint::native::{outline_glyph, units_per_em};
use vieww_text::{FontStore, Paragraph, TextSpan};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, ease_out_cubic, mix, spring_out, tint, FAINT, MUTED, VIOLET,
    VIOLET_SOFT, BG_DEEP,
};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 7.0;

/// The sentence — the film's own S02 question.
const LINE: &str = "how long should it take to see what you built?";

/// The em size for the question, px.
const EM: f32 = 44.0;

/// Per-letter shard grid.
const SHARD_COLS: usize = 3;
const SHARD_ROWS: usize = 4;

// ── Shaping: the sentence as real glyph outlines (the U-03 door) ────────────

pub struct Letter {
    /// Placed outline (baseline at 0, run origin at x=0).
    pub outline: Path,
    /// Bounding box in line space: (left, top, right, bottom).
    pub bounds: (f32, f32, f32, f32),
}

struct Shaped {
    letters: Vec<Letter>,
    /// Total advance, px.
    advance: f32,
    glyph_count: usize,
}

fn shape_line() -> Shaped {
    let bytes = std::fs::read("/usr/share/fonts/truetype/english/Carlito-Regular.ttf")
        .or_else(|_| std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"))
        .unwrap_or_default();

    let mut store = FontStore::with_application_faces([bytes.clone()], Some("Carlito"), None);
    let paragraph = Paragraph::layout(
        &mut store,
        &[TextSpan::new(LINE, TextStyle::new(EM))],
        2000.0,
    );

    let face_index = 0u32;
    let upem = units_per_em(&bytes, face_index).unwrap_or(2048.0);
    let scale = EM / upem;

    let mut letters = Vec::new();
    let mut glyph_count = 0usize;

    for run in paragraph.runs() {
        for glyph in run.glyphs.iter() {
            glyph_count += 1;
            let Some(outline) =
                outline_glyph(run.font.bytes(), face_index, glyph.id, run.font.variations())
            else {
                continue;
            };
            let place = Transform::scale(scale, scale)
                .then(Transform::translate(Offset::new(glyph.offset.dx, 0.0)));
            let placed = outline.transformed(place);
            let b = placed.bounds();
            letters.push(Letter {
                outline: placed,
                bounds: (b.left, b.top, b.left + b.width(), b.top + b.height()),
            });
        }
    }

    let left_edge = letters.first().map(|l| l.bounds.0).unwrap_or(0.0);
    let right = letters.last().map(|l| l.bounds.2).unwrap_or(0.0);
    let shift = Transform::translate(Offset::new(-left_edge, 0.0));
    for letter in &mut letters {
        letter.outline = letter.outline.transformed(shift);
        let (l, tp, r, bt) = letter.bounds;
        letter.bounds = (l - left_edge, tp, r - left_edge, bt);
    }

    Shaped {
        letters,
        advance: right - left_edge,
        glyph_count,
    }
}

fn shaped() -> &'static Shaped {
    static S: OnceLock<Shaped> = OnceLock::new();
    S.get_or_init(shape_line)
}

/// The ink color of the line — violet-tinted white.
fn ink(a: f32) -> Color {
    alpha(mix(tint(VIOLET_SOFT, 0.5), Color::WHITE, 0.55), a.clamp(0.0, 1.0))
}

// ── The scene ───────────────────────────────────────────────────────────────

fn scene(book: &mut Sketchbook, canvas: Size, t: f32) {
    let w = canvas.width;
    let h = canvas.height;
    let s = shaped();

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

    // Stage gates.
    let split_t = clamp01((t - 0.10) / 0.22);
    let defocus_t = clamp01((t - 0.28) / 0.27);
    let release_t = clamp01((t - 0.50) / 0.38);
    let gone_t = clamp01((t - 0.88) / 0.12);

    let cx = w * 0.5;
    let base_y = h * 0.46;
    let x0 = cx - s.advance * 0.5;

    // The two live instruments of the release.
    let chroma = ease_out_cubic(split_t) * 7.5;
    let sigma = ease_in_out(defocus_t) * 16.0;
    // Ink compensation: blur eats coverage at 8-bit depth — lift the alpha
    // as σ grows so the line reads as brightening, not muddying.
    let comp = 1.0 + defocus_t * 0.85;
    let lift = ease_in_out(defocus_t) * 26.0;

    // ── The SPLIT pass: three pure-channel copies through Plus ────────────
    if split_t > 0.0 && split_t < 1.0 && gone_t < 1.0 {
        let a_split = 0.8 * split_t * (1.0 - gone_t);
        let board = Transform::translate(Offset::new(x0, base_y - lift * 0.3));
        book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
            g.transformed(board, |g2| {
                for letter in &s.letters {
                    let (l, _, _, _) = letter.bounds;
                    if letter.bounds.2 - l < 0.5 {
                        continue;
                    }
                    // Radial direction from line centre, per letter.
                    let dir_x = (l + (letter.bounds.2 - l) * 0.5 - s.advance * 0.5)
                        / (s.advance * 0.5).max(1.0);
                    let k = ease_out_cubic(split_t) * (0.35 + 0.65 * dir_x.abs());
                    let rgb = [
                        (Color::rgb(255, 46, 96), -chroma * k),
                        (Color::rgb(64, 255, 128), 0.0),
                        (Color::rgb(96, 138, 255), chroma * k),
                    ];
                    for (col, dx) in rgb {
                        let tr = Transform::translate(Offset::new(dx, 0.0));
                        g2.transformed(tr, |h2| {
                            h2.fill(letter.outline.clone(), alpha(col, a_split));
                        });
                    }
                }
            });
        });
    }

    // ── The main line: sharp ink that defocuses ────────────────────────────
    let ink_left = (1.0 - clamp01(release_t * 1.4)) * (1.0 - gone_t);
    if ink_left > 0.0 {
        let board = Transform::translate(Offset::new(x0, base_y - lift));
        let a = comp * ink_left;
        if sigma > 0.05 {
            // ONE blurred group — the U-06 economy, stated in the receipt.
            book.layer(1.0, sigma, None, |g| {
                g.transformed(board, |g2| {
                    for letter in &s.letters {
                        if letter.bounds.2 - letter.bounds.0 < 0.5 {
                            continue;
                        }
                        g2.fill(letter.outline.clone(), ink(a));
                    }
                });
            });
        } else {
            book.transformed(board, |g2| {
                for letter in &s.letters {
                    if letter.bounds.2 - letter.bounds.0 < 0.5 {
                        continue;
                    }
                    g2.fill(letter.outline.clone(), ink(a));
                }
            });
        }
    }

    // ── The RELEASE: per-letter shard dust, staggered left → right ────────
    if release_t > 0.0 && gone_t < 1.0 {
        for (li, letter) in s.letters.iter().enumerate() {
            let (l, tp, r, bt) = letter.bounds;
            if r - l < 0.5 {
                continue; // a space — nothing to release
            }
            // The release wave: left letter first.
            let phase = clamp01(release_t * 2.2 - (l / s.advance.max(1.0)) * 1.25);
            if phase <= 0.0 {
                continue;
            }
            let rise = ease_out_cubic(phase);
            let pop = spring_out(phase, 7.0, 0.62);

            // The board at release time: the letter sits where the defocus
            // left it.
            let board = Transform::translate(Offset::new(x0, base_y - lift));

            // The letter's own body fades as its shards take over.
            let body_left = 1.0 - clamp01(phase * 2.0);
            if body_left > 0.0 {
                let a_body = comp * body_left * (1.0 - gone_t);
                book.layer(1.0, sigma * body_left, None, |g| {
                    g.transformed(board, |g2| {
                        g2.fill(letter.outline.clone(), ink(a_body));
                    });
                });
            }

            // The shards: a 3×4 grid of windows onto the letter — the U-11
            // transform-outside / clip-inside pattern, verbatim from the
            // upgrade file: the flight is outside, the clip is inside, and
            // the window still looks onto the original artwork.
            let (cw, ch) = ((r - l) / SHARD_COLS as f32, (bt - tp) / SHARD_ROWS as f32);
            for sy in 0..SHARD_ROWS {
                for sx in 0..SHARD_COLS {
                    let seed = (li * 41 + sy * 7 + sx * 3) as u64;
                    let fx = ((seed as f32 * 0.971).fract() - 0.5) * 2.0;
                    let fy = ((seed as f32 * 1.237).fract() - 0.5) * 2.0;
                    let spin = ((seed as f32 * 0.773).fract() - 0.5) * 2.6;

                    let u = (sx as f32 + 0.5) / SHARD_COLS as f32;
                    let v = (sy as f32 + 0.5) / SHARD_ROWS as f32;
                    let dist = 26.0 + 150.0 * rise;
                    let dx = (u - 0.5) * dist * 1.7 + fx * 46.0 * rise;
                    let dy = -rise * (60.0 + v * 120.0) + fy * 26.0 * rise;
                    let rot = spin * rise;
                    let scale = (1.0 - 0.42 * rise) * (1.0 + 0.14 * pop);
                    let alpha_k = (1.0 - rise * 0.72) * (1.0 - gone_t);

                    // The shard window, in line space.
                    let cell_l = l + sx as f32 * cw;
                    let cell_t = tp + sy as f32 * ch;
                    let pad = 1.2;
                    let clip = Path::rounded_rect(
                        Rect::new(
                            cell_l + pad,
                            cell_t + pad,
                            cell_l + cw - pad,
                            cell_t + ch - pad,
                        ),
                        2.0,
                    );

                    // The flight: rotate + scale about the shard's own
                    // centre, then the board, then the flight offset.
                    let centre = Offset::new(cell_l + cw * 0.5, cell_t + ch * 0.5);
                    let about = Transform::rotate_around(centre, rot)
                        .then(
                            Transform::translate(centre)
                                .then(Transform::scale(scale, scale))
                                .then(Transform::translate(Offset::new(-centre.dx, -centre.dy))),
                        )
                        .then(Transform::translate(Offset::new(dx, dy)))
                        .then(board);

                    book.transformed(about, |g| {
                        g.layer(alpha_k, 0.0, Some(clip), |h| {
                            h.fill(letter.outline.clone(), ink(0.98));
                        });
                    });
                    // The shard's ember: a Plus companion mote riding the
                    // same flight — the dust the letter leaves behind.
                    book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                        let centre_l = l + cw * 0.5;
                        let centre_t = tp + ch * 0.5;
                        let fp = about.apply(Offset::new(centre_l, centre_t));
                        g.circle(fp, 1.6 + 1.2 * (1.0 - rise), alpha(tint(VIOLET, 0.45), alpha_k * 0.5));
                    });
                }
            }
        }
    }

    // ── Free dust: motes the letters leave behind (not letter-bound) ───────
    if release_t > 0.1 && gone_t < 1.0 {
        use crate::film_lib::Rng;
        let mut rng = Rng::new(0xFAD);
        let count = 90;
        book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
            for i in 0..count {
                let u = rng.f01();
                let v = rng.f01();
                let born = clamp01(release_t * 2.4 - u * 1.1);
                if born <= 0.0 {
                    continue;
                }
                let x = x0 + u * s.advance;
                let y = base_y + (v - 0.5) * EM;
                let rise = ease_out_cubic(born) * (60.0 + v * 140.0);
                let drift = ((i as f32 * 0.733).fract() - 0.5) * 90.0 * born;
                let r = 0.8 + (i as f32 * 0.379).fract() * 1.8;
                g.circle(
                    Offset::new(x + drift, y - rise),
                    r,
                    alpha(tint(VIOLET, 0.55), 0.5 * (1.0 - born) * (1.0 - gone_t)),
                );
            }
        });
    }

    // ── GONE: the baseline hairline remains ────────────────────────────────
    if t > 0.30 {
        let gone_gate = clamp01((t - 0.30) / 0.3);
        let a = 0.16 + 0.30 * gone_t;
        let y = base_y + EM * 0.28 - lift * (1.0 - release_t);
        book.line(
            Offset::new(x0 - 12.0, y),
            Offset::new(x0 + s.advance + 12.0, y),
            alpha(mix(FAINT, VIOLET_SOFT, gone_gate), a),
            1.0,
        );
        if gone_t > 0.3 {
            book.layer(1.0, 8.0 * gone_t, None, |g| {
                g.line(
                    Offset::new(x0 - 12.0, y),
                    Offset::new(x0 + s.advance + 12.0, y),
                    alpha(VIOLET, 0.30 * gone_t),
                    2.4,
                );
            });
        }
    }

    // The vignette.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::radial(Offset::new(0.5, 0.5), 0.8)
            .with_dither()
            .with_stops(&[
                (0.6, alpha(Color::BLACK, 0.0)),
                (1.0, alpha(Color::BLACK, 0.45)),
            ]),
    );
}

/// The release state, as a pure function of `t` — the same arithmetic the
/// paint callback uses, computed once more so the textual instrument reads
/// the render's own numbers, not a frame-behind copy.
fn release_state(t: f32) -> (usize, usize) {
    let s = shaped();
    let release_t = clamp01((t - 0.50) / 0.38);
    let gone_t = clamp01((t - 0.88) / 0.12);
    if release_t <= 0.0 || gone_t >= 1.0 {
        return (0, 0);
    }
    let mut shards = 0usize;
    let mut released = 0usize;
    for letter in &s.letters {
        let (l, _, r, _) = letter.bounds;
        if r - l < 0.5 {
            continue;
        }
        let phase = clamp01(release_t * 2.2 - (l / s.advance.max(1.0)) * 1.25);
        if phase > 0.0 {
            released += 1;
            shards += SHARD_COLS * SHARD_ROWS;
        }
    }
    (shards, released)
}

/// The frame.
pub fn frame(t: f32) -> WidgetNode {
    let paint = Painting::sized(
        crate::film_lib::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            scene(book, size, t);
        }),
    );

    let s = shaped();
    let split_t = clamp01((t - 0.10) / 0.22);
    let defocus_t = clamp01((t - 0.28) / 0.27);
    let chroma = ease_out_cubic(split_t) * 7.5;
    let sigma = ease_in_out(defocus_t) * 16.0;
    let comp = 1.0 + defocus_t * 0.85;
    let ink_letters = s
        .letters
        .iter()
        .filter(|l| l.bounds.2 - l.bounds.0 >= 0.5)
        .count();
    let (shards, released) = release_state(t);

    let strip = Stack::new()
        .push(
            Positioned::new()
                .left(24.0)
                .top(20.0)
                .width(560.0)
                .height(18.0)
                .child(
                    Text::new("THE FADE-AWAY · transition grammar · 32-frame sample").style(
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
                .width(640.0)
                .height(18.0)
                .child(
                    Text::new(format!(
                        "σ {sigma:>4.1} px · chroma ±{chroma:>4.1} px · ink ×{comp:>4.2} · glyphs {}",
                        s.glyph_count,
                    ))
                    .style(TextStyle::new(11.0).monospace().color(alpha(MUTED, 0.85))),
                ),
        )
        .push(
            Positioned::new()
                .left(24.0)
                .top(58.0)
                .width(640.0)
                .height(18.0)
                .child(
                    Text::new(format!(
                        "letters released {released:>2}/{ink_letters:>2} · shards live {shards:>3} · one blurred group (U-06) · windows 3×4 (U-11)",
                    ))
                    .style(TextStyle::new(11.0).monospace().color(alpha(MUTED, 0.7))),
                ),
        );

    Stack::new()
        .push(Positioned::fill().child(paint))
        .push(strip)
        .into()
}
