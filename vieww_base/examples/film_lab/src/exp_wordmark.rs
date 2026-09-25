//! exp_wordmark — *X-03: the mark as real geometry.* The U-03 plate.
//!
//! The wordmark **vieww** drawn as **glyph outlines handed over by the
//! framework itself** — `vieww_paint::native::glyph::outline_glyph`, the
//! public door the bench asked for — not as a Text widget, and not by
//! re-parsing the face with `ttf-parser` one layer above the rasterizer
//! (the workaround the upgrade file was written to retire).
//!
//! What the outline buys, on screen:
//! - **E-17 stroke-on reveal** — the skeleton of each letter strokes itself
//!   in through a scan window (a clip layer whose cut advances left→right;
//!   letter stagger falls out of the x order for free).
//! - **Chrome fill** — a horizontal silver ramp that *phase-advances* every
//!   frame. Its stops wrap through 1.0 and are **re-sorted each frame** —
//!   the U-08 discipline, done where everyone can see it, with the count of
//!   wrapped stops printed in the receipt.
//! - **A specular band** that crosses the letters once through a
//!   `Plus`-blended group — `Sketchbook::blended_layer`, the U-01 seam.
//! - **A reflection** — the same outlines, mirrored, in a blurred layer,
//!   faded out by a ground gradient painted over it. Every pixel vieww.
//!
//! The receipt is the shaping itself: glyph count, unique outlines, the
//! face's units-per-em, and the measured advance — no number typed by a
//! human. Beside it, the **U-10 dial**: a sweep gradient whose ramp is a
//! palindrome (`with_stops_mirrored`) turning with no seam at twelve
//! o'clock, with a needle at the phase the chrome is actually at.
//!
//! Grammar notes, from the graph:
//! - X-03 "wordmark" — `outline_glyph` (U-03) + chrome sweep (U-08/U-10)
//! - E-17 self-draw — the clip-window reveal variant of the dash-phase
//!   technique (a closed outline has no cheap perimeter; the window is the
//!   honest spelling of "reveal" for letterforms).

use std::sync::OnceLock;

use vieww_foundation::{
    BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle, Transform,
};
use vieww_paint::native::{outline_glyph, units_per_em};
use vieww_text::{FontStore, Paragraph, TextSpan};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{alpha, clamp01, mix, BG_DEEP, CANVAS, FAINT, INK, MUTED, Rng, VIOLET,
    VIOLET_SOFT, VIOLET_DEEP};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 10.0;

/// The wordmark, verbatim — lowercase, the mark's own voice.
const MARK: &str = "vieww";

/// Em size the mark is shaped at.
const EM: f32 = 118.0;

/// Letter tracking, in px — the wordmark's own spacing, added on top of the
/// shaped advances (which the receipt reports separately).
const TRACKING: f32 = 14.0;

/// Stroke width of the skeleton reveal.
const SKELETON_W: f32 = 2.0;

// ── Timeline ────────────────────────────────────────────────────────────────

const REVEAL_T0: f32 = 0.08;
const REVEAL_SPAN: f32 = 0.42;

const FILL_T0: f32 = 0.42;
const FILL_SPAN: f32 = 0.20;

const REFLECT_T0: f32 = 0.50;
const REFLECT_SPAN: f32 = 0.20;

const GLINT_T0: f32 = 0.72;
const GLINT_SPAN: f32 = 0.20;

/// Chrome phase period, in film seconds.
const CHROME_PERIOD: f32 = 6.0;

// ── The shaped mark — built once, measured, never guessed ───────────────────

/// One placed letter: its outline in canvas coordinates (origin at the
/// mark's left edge, baseline at y = 0), and its measured width.
pub(crate) struct Letter {
    pub(crate) outline: Path,
    pub(crate) width: f32,
}

/// Everything the receipt reports about the face and the shaping.
pub(crate) struct Mark {
    pub(crate) letters: Vec<Letter>,
    /// The union outline — every letter concatenated, one fill target.
    /// Letters are normalized: x in [0, advance], baseline at BASELINE.
    pub(crate) all: Path,
    /// Total advance in px at `EM`, tracking included.
    pub(crate) advance: f32,
    pub(crate) upem: f32,
    pub(crate) glyph_count: usize,
    /// Ascent of the shaped run above the baseline, px.
    pub(crate) ascent: f32,
    /// Descent below the baseline, px (positive number).
    pub(crate) descent: f32,
}

impl Mark {
    fn build() -> Self {
        // The face: Carlito Regular — the closest system face to the film's
        // Inter/SF voice. Fall back to DejaVu if the file is absent; the
        // receipt prints the upem either way, so the substitution is visible.
        let bytes = std::fs::read("/usr/share/fonts/truetype/english/Carlito-Regular.ttf")
            .or_else(|_| std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"))
            .unwrap_or_default();

        // Shape the run the way the rasterizer does: FontStore → Paragraph →
        // positioned glyph runs. This is the U-03 recipe verbatim — the
        // shaping layer hands over ids and offsets, the door hands back
        // outlines, and the caller does the same scale-and-place arithmetic
        // `place_glyph` performs before flattening.
        let mut store = FontStore::with_application_faces(
            [bytes.clone()],
            Some("Carlito"),
            None,
        );
        let paragraph = Paragraph::layout(
            &mut store,
            &[TextSpan::new(MARK, TextStyle::new(EM))],
            4000.0,
        );

        let face_index = 0u32;
        let upem = units_per_em(&bytes, face_index).unwrap_or(2048.0);
        let scale = EM / upem;

        let mut letters = Vec::new();
        let mut all = Path::new();
        let mut glyph_count = 0usize;
        let mut top = f32::MAX;
        let mut bottom = f32::MIN;

        for run in paragraph.runs() {
            for (i, glyph) in run.glyphs.iter().enumerate() {
                glyph_count += 1;
                let Some(outline) =
                    outline_glyph(run.font.bytes(), face_index, glyph.id, run.font.variations())
                else {
                    continue;
                };
                // Place: font units → px, then to (shaped offset +
                // per-glyph tracking, baseline at BASELINE). Y is already
                // flipped to the screen convention by the door itself — the
                // same arithmetic `place_glyph` applies, with the board's
                // baseline as the run origin's y.
                let x = glyph.offset.dx + i as f32 * TRACKING;
                let place = Transform::scale(scale, scale)
                    .then(Transform::translate(Offset::new(x, BASELINE)));
                let placed = outline.transformed(place);
                let b = placed.bounds();
                if b.width() > 0.0 || b.height() > 0.0 {
                    top = top.min(b.top);
                    bottom = bottom.max(b.top + b.height());
                }
                all.extend(&placed);
                letters.push(Letter {
                    outline: placed,
                    width: b.width(),
                });
            }
        }

        // The advance, measured from the geometry, tracking included: the
        // right edge of the last placed letter minus the left edge of the
        // first. ("vieww" shapes as one left-to-right run, so letter
        // positions already carry the advances.)
        let right = letters
            .last()
            .map(|l| {
                let b = l.outline.bounds();
                b.left + b.width()
            })
            .unwrap_or(0.0);
        let left_edge = letters
            .first()
            .map(|l| l.outline.bounds().left)
            .unwrap_or(0.0);
        let advance = right - left_edge;

        // Normalize: shift every letter so the word starts at x = 0 — the
        // board centers it by the measured advance, no hand numbers.
        let shift = Transform::translate(Offset::new(-left_edge, 0.0));
        for letter in &mut letters {
            letter.outline = letter.outline.transformed(shift);
        }
        all = all.transformed(shift);

        // Ascent/descent from the actual outline extremes, relative to the
        // baseline (letters are placed AT the baseline).
        let ascent = if top.is_finite() { BASELINE - top } else { EM * 0.75 };
        let descent = if bottom.is_finite() { bottom - BASELINE } else { EM * 0.25 };

        Mark {
            letters,
            all,
            advance,
            upem,
            glyph_count,
            ascent,
            descent,
        }
    }

    pub(crate) fn get() -> &'static Mark {
        static MARK: OnceLock<Mark> = OnceLock::new();
        MARK.get_or_init(Mark::build)
    }
}

// ── The chrome ramp — U-08, the phase that wraps and re-sorts ───────────────

/// The base chrome ramp: dark edges, a bright band either side of the tip,
/// white at the tip. A silver ramp with a violet cast.
const CHROME_BASE: [(f32, Color); 7] = [
    (0.00, Color::rgb(38, 36, 52)),
    (0.18, Color::rgb(96, 88, 130)),
    (0.42, Color::rgb(178, 168, 220)),
    (0.50, Color::rgb(255, 255, 255)),
    (0.58, Color::rgb(178, 168, 220)),
    (0.82, Color::rgb(96, 88, 130)),
    (1.00, Color::rgb(38, 36, 52)),
];

/// The chrome stops at `phase` — offsets wrapped through 1.0, then
/// **re-sorted**, exactly as U-08 prescribes. Returns the stops and how many
/// wrapped (the receipt prints it; a wrapped stop is the one-frame flicker
/// the un-sorted version produces).
pub(crate) fn chrome_stops(phase: f32) -> (Vec<(f32, Color)>, usize) {
    let mut wrapped = 0usize;
    let mut stops: Vec<(f32, Color)> = CHROME_BASE
        .iter()
        .map(|&(o, c)| {
            let mut x = o + phase;
            while x >= 1.0 {
                x -= 1.0;
                wrapped += 1;
            }
            (x, c)
        })
        .collect();
    // THE re-sort — the line U-08 exists to make you write.
    stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    (stops, wrapped)
}

// ── The board geometry ──────────────────────────────────────────────────────

/// Where the mark's baseline sits.
pub(crate) const BASELINE: f32 = 340.0;

fn mark_left(mark: &Mark) -> f32 {
    (CANVAS.width - mark.advance) * 0.5
}

// ── The wordmark painter ────────────────────────────────────────────────────

fn wordmark(book: &mut Sketchbook, t: f32) {
    let mark = Mark::get();
    if mark.letters.is_empty() {
        return;
    }
    let left = mark_left(mark);
    let reveal = clamp01((t - REVEAL_T0) / REVEAL_SPAN);
    let fill_a = clamp01((t - FILL_T0) / FILL_SPAN);
    let phase = (t * SECONDS / CHROME_PERIOD).fract();

    // 1 · The skeleton — E-17 as a scan window: stroke the union outline,
    //    clipped to a rect whose right edge advances with the reveal. Left
    //    letters complete before right ones begin — stagger for free.
    let placed = mark
        .all
        .transformed(Transform::translate(Offset::new(left, 0.0)));
    if reveal > 0.0 {
        let cut = left + (mark.advance + 10.0) * reveal;
        let clip = Path::rect(Rect::new(left - 4.0, BASELINE - mark.ascent - 30.0, cut,
            BASELINE + mark.descent + 30.0));
        book.layer(1.0, 0.0, Some(clip), |g| {
            g.stroke(placed.clone(), alpha(mix(INK, VIOLET_SOFT, 0.25), 0.92), SKELETON_W);
        });
    }

    // 2 · The chrome fill — the union path filled once, so the ramp flows
    //    across the whole word, stops phase-advanced and re-sorted.
    if fill_a > 0.0 {
        let (stops, _wrapped) = chrome_stops(phase);
        let mut chrome = Gradient::horizontal().with_dither();
        chrome = chrome.with_stops(&stops);
        book.layer(fill_a, 0.0, None, |g| {
            g.fill(placed.clone(), chrome);
        });
    }

    // 3 · The specular band — one pass, Plus-blended (U-01): a narrow
    //    bright slab crossing the letters after the fill settles.
    let glint = clamp01((t - GLINT_T0) / GLINT_SPAN);
    if glint > 0.0 && glint < 1.0 {
        let x = left - 90.0 + (mark.advance + 180.0) * glint;
        let fade = (glint * (1.0 - glint) * 4.0).sqrt();
        book.blended_layer(1.0, 6.0, BlendMode::Plus, None, |g| {
            g.rect(
                Rect::new(x - 26.0, BASELINE - mark.ascent - 8.0, x + 26.0,
                    BASELINE + mark.descent + 8.0),
                Gradient::horizontal().with_stops(&[
                    (0.0, alpha(Color::WHITE, 0.0)),
                    (0.5, alpha(mix(Color::WHITE, VIOLET_SOFT, 0.3), 0.30 * fade)),
                    (1.0, alpha(Color::WHITE, 0.0)),
                ]),
            );
        });
    }

    // 4 · The reflection — the same outlines mirrored about the floor line,
    //    in a blurred group, faded out by the ground gradient painted over it.
    let reflect = clamp01((t - REFLECT_T0) / REFLECT_SPAN);
    if reflect > 0.0 {
        let mirror = Transform::scale(1.0, -1.0)
            .then(Transform::translate(Offset::new(left, 2.0 * (BASELINE + 9.0))));
        let flipped = mark.all.transformed(mirror);
        book.layer(reflect, 2.5, None, |g| {
            g.fill(flipped, alpha(mix(VIOLET_SOFT, Color::WHITE, 0.2), 0.14));
        });
    }

    // 5 · The floor — a hairline the mark sits on, and the grounding wash.
    let floor_a = clamp01(t / 0.10) * 0.5;
    book.line(
        Offset::new(left - 40.0, BASELINE + 6.0),
        Offset::new(left + mark.advance + 40.0, BASELINE + 6.0),
        alpha(FAINT, floor_a),
        1.0,
    );
    book.layer(1.0, 30.0, None, |g| {
        g.circle(
            Offset::new(left + mark.advance * 0.5, BASELINE + 10.0),
            mark.advance * 0.42,
            Gradient::radial_fill().with_dither().with_stops(&[
                (0.0, alpha(VIOLET, 0.10)),
                (1.0, alpha(VIOLET, 0.0)),
            ]),
        );
    });
}

// ── The receipt panel — the shaping, measured ───────────────────────────────

const P_X: f32 = 872.0;
const P_Y: f32 = 150.0;
const P_W: f32 = 340.0;
const P_H: f32 = 216.0;

fn receipt(book: &mut Sketchbook, t: f32) {
    let phase = (t * SECONDS / CHROME_PERIOD).fract();
    let (stops, wrapped) = chrome_stops(phase);

    // The card.
    book.rrect(
        Rect::new(0.0, 0.0, P_W, P_H),
        10.0,
        alpha(Color::rgb(16, 16, 21), 0.88),
    );
    book.stroke_rrect(
        Rect::new(0.0, 0.0, P_W, P_H),
        10.0,
        alpha(Color::WHITE, 0.08),
        1.0,
    );

    // The U-10 dial — a sweep ramp that is a palindrome, turning with no
    // seam at twelve o'clock, needle at the chrome's own phase.
    let dial_c = Offset::new(P_W - 44.0, 46.0);
    let dial_r = 24.0;
    let theta = phase * std::f32::consts::TAU;
    book.circle(
        dial_c,
        dial_r,
        Gradient::sweep(
            Offset::new(0.5, 0.5),
            theta - std::f32::consts::TAU,
            theta,
        )
        .with_stops_mirrored(&[
            (0.0, VIOLET_DEEP),
            (0.30, VIOLET),
            (0.5, Color::WHITE),
        ]),
    );
    book.ring(dial_c, dial_r, 1.0, alpha(Color::WHITE, 0.14));
    // The needle — a hairline at the sweep's end, through the phase.
    book.line(
        Offset::new(dial_c.dx, dial_c.dy),
        Offset::new(
            dial_c.dx + (theta - std::f32::consts::FRAC_PI_2).cos() * (dial_r + 5.0),
            dial_c.dy + (theta - std::f32::consts::FRAC_PI_2).sin() * (dial_r + 5.0),
        ),
        alpha(INK, 0.85),
        1.2,
    );
    let _ = wrapped;

    // The stop ramp, linear, as it is this frame — wrapped and sorted.
    let ramp_x = 16.0;
    let ramp_y = 92.0;
    let ramp_w = P_W - 32.0;
    let mut ramp = Gradient::horizontal();
    ramp = ramp.with_stops(&stops);
    book.rrect(
        Rect::new(ramp_x, ramp_y, ramp_x + ramp_w, ramp_y + 10.0),
        5.0,
        ramp,
    );
    // A tick under every stop — the geometry of the ramp, this frame.
    for &(o, _) in &stops {
        let x = ramp_x + ramp_w * o;
        book.line(
            Offset::new(x, ramp_y + 12.0),
            Offset::new(x, ramp_y + 18.0),
            alpha(MUTED, 0.7),
            1.0,
        );
    }

    // The glint indicator on the ramp — where the band is right now.
    let glint = clamp01((t - GLINT_T0) / GLINT_SPAN);
    if glint > 0.0 && glint < 1.0 {
        let x = ramp_x + ramp_w * glint;
        book.ring(
            Offset::new(x, ramp_y + 5.0),
            8.0,
            1.4,
            alpha(mix(Color::WHITE, VIOLET_SOFT, 0.3), 0.9),
        );
    }

    // The chrome phase marker riding the ramp.
    let px = ramp_x + ramp_w * phase;
    book.circle(Offset::new(px, ramp_y + 5.0), 3.4, Color::WHITE);
}

/// The panel's text labels — the receipt's voice, over the painter's card.
fn receipt_labels(t: f32) -> WidgetNode {
    let mark = Mark::get();
    let phase = (t * SECONDS / CHROME_PERIOD).fract();
    let (stops, wrapped) = chrome_stops(phase);

    let lines = [
        "X-03 · WORDMARK · OUTLINE DOOR (U-03)".to_string(),
        format!("glyphs {} · outlines {} · upem {:.0}", mark.glyph_count, mark.letters.len(), mark.upem),
        format!("advance {:.0}px @ {:.0}em · track {:.0}px", mark.advance, EM, TRACKING),
        format!("chrome stops {} · wrapped {} → re-sorted", stops.len(), wrapped),
        format!("reveal {:.0}% · fill {:.0}% · phase {:.2}",
            clamp01((t - REVEAL_T0) / REVEAL_SPAN) * 100.0,
            clamp01((t - FILL_T0) / FILL_SPAN) * 100.0,
            phase),
    ];

    let mut stack = Stack::new()
        .push(
            Positioned::new()
                .left(P_X + 16.0)
                .top(P_Y - 22.0)
                .width(P_W)
                .height(18.0)
                .child(
                    Text::new(lines[0].clone()).style(
                        TextStyle::new(12.0)
                            .monospace()
                            .letter_spacing(2.2)
                            .color(alpha(FAINT, 0.95)),
                    ),
                ),
        );
    for (i, line) in lines.iter().enumerate().skip(1) {
        stack = stack.push(
            Positioned::new()
                .left(P_X + 16.0)
                .top(P_Y + 122.0 + i as f32 * 17.0)
                .width(P_W - 12.0)
                .height(16.0)
                .child(
                    Text::new(line.clone()).style(
                        TextStyle::new(11.5)
                            .monospace()
                            .color(alpha(MUTED, 0.92)),
                    ),
                ),
        );
    }
    stack.into()
}

// ── The board ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let bg = Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground: deep, quiet — a type beat.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(8, 8, 11)),
                    (0.55, BG_DEEP),
                    (1.0, Color::rgb(13, 12, 19)),
                ]),
            );

            // Sparse stars — same seed family as the spring plate, calmer.
            let mut rng = Rng::new(0x503);
            for _ in 0..64 {
                let x = rng.f01() * w;
                let y = rng.f01() * h;
                let r = 0.4 + rng.f01() * 0.8;
                let tw = 0.5 + 0.5 * (t * 2.6 + rng.f01() * 9.0).sin();
                book.circle(Offset::new(x, y), r, alpha(Color::WHITE, 0.03 + 0.06 * tw));
            }

            // The wordmark and its receipt.
            wordmark(book, t);
            receipt(book, t);

            // The reflection fade — a ground gradient painted OVER the
            // mirrored fill, so the reflection sinks into the floor.
            let reflect = clamp01((t - REFLECT_T0) / REFLECT_SPAN);
            if reflect > 0.0 {
                book.rect(
                    Rect::new(0.0, BASELINE + 18.0, w, BASELINE + 150.0),
                    Gradient::vertical().with_stops(&[
                        (0.0, alpha(BG_DEEP, 0.0)),
                        (0.5, alpha(BG_DEEP, 0.7)),
                        (1.0, alpha(BG_DEEP, 1.0)),
                    ]),
                );
            }

            // The vignette.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.45), 0.85).with_dither().with_stops(&[
                    (0.55, alpha(Color::BLACK, 0.0)),
                    (1.0, alpha(Color::BLACK, 0.45)),
                ]),
            );
        }),
    );

    Stack::new()
        .push(Positioned::fill().child(bg))
        .push(receipt_labels(t))
        .into()
}
