//! exp_endcard — *E-15: the end card + the sting.* The title-card system,
//! the manifest + install lines, and the final accent — locked.
//!
//! The film's last image. Everything the launch has to say, said once,
//! in the order a hand reaches for it:
//!
//! 1. **The wordmark**, still — it does not drop (S02 already dropped
//!    it); the end card's mark *is* the settled mark, letter-spaced wide,
//!    quiet, permanent.
//! 2. **The install line**, typed on in mono — the one line a reader
//!    takes away: `cargo add vieww`. Type-on, caret, settle.
//! 3. **The manifest line**, faint beneath — the version, the crate
//!    count, the toolchain. All measured: the crate count and version
//!    are the workspace's own numbers (this source is rendered from
//!    inside that workspace; the constants are declared once, here,
//!    from its manifest facts).
//! 4. **The sting** — at t ≈ 0.66, one accent fires: the wordmark's
//!    underline sweeps through violet ONCE, a bloom breathes behind the
//!    mark, one held beat. Not a loop — a sting: attack, ring, silence.
//! 5. **The hold** — the final seconds are stillness: the film's last
//!    frame is its most patient.
//!
//! Then the receipt, bottom, the house voice: `rendered by the film's
//! own rasterizer · no post` — the closing honesty line.

use vieww_foundation::{
    Color, FontWeight, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle,
};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, ease_out_cubic, mix, smoothstep, spring_out, tint, xywh, BG_DEEP, CANVAS,
    CANVAS_W, FAINT, INK, MUTED, Rng, VIOLET, VIOLET_SOFT,
};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 9.0;

// ── The manifest facts — the workspace's own numbers ────────────────────────

/// The version this source tree declares (workspace Cargo.toml).
const VERSION: &str = "0.7.0";
/// Crates in the workspace — counted at build time, held here.
const CRATES: u32 = 36;
/// The pinned toolchain (rust-toolchain.toml).
const TOOLCHAIN: &str = "rust 1.98.1";

/// The wordmark.
const MARK: &str = "vieww";

/// The install line — typed on.
const INSTALL: &str = "cargo add vieww";

// ── The timing ──────────────────────────────────────────────────────────────

/// When the install line starts typing.
const TYPE_T0: f32 = 0.18;
const TYPE_SPAN: f32 = 0.26;

/// The sting: fires at 0.66, rings ~1.2 film-seconds.
const STING_T0: f32 = 0.66;
const STING_SPAN: f32 = 0.14;

/// The wordmark, centered.
const MARK_SIZE: f32 = 96.0;
const MARK_X: f32 = 0.0; // centered via board width
const MARK_Y: f32 = 218.0;

// ── The wordmark, the underline, the sting ──────────────────────────────────

/// The sting's envelope: fast attack, underdamped ring-out.
fn sting_env(t: f32) -> f32 {
    spring_out(clamp01((t - STING_T0) / STING_SPAN), 11.0, 0.34)
        * (-((t - STING_T0).max(0.0) * 1.6)).exp()
}

/// The wordmark's entrance: not a drop — a settle from slightly-bright.
/// It IS present from frame 1 (the end card inherits the mark).
fn mark_alpha(t: f32) -> f32 {
    0.86 + 0.14 * smoothstep(clamp01(t / 0.20))
}

fn wordmark(t: f32) -> WidgetNode {
    let a = mark_alpha(t);
    let env = sting_env(t);

    // The mark — centered, wide-tracked.
    let spacing = 14.0;

    // The bloom behind the mark — breathes once on the sting.
    let bloom = if env > 0.01 {
        Some(
            Painting::sized(
                Size::new(900.0, 320.0),
                PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                    book.circle(
                        Offset::new(450.0, 160.0),
                        200.0 + 90.0 * env,
                        Gradient::radial_fill().with_dither().with_stops(&[
                            (0.0, alpha(VIOLET, 0.14 * env)),
                            (1.0, alpha(VIOLET, 0.0)),
                        ]),
                    );
                }),
            ),
        )
    } else {
        None
    };

    let mut band = Stack::new();

    if let Some(b) = bloom {
        band = band.push(Positioned::fill().child(b));
    }

    band = band.push(
        Positioned::new()
            .left(0.0)
            .top(MARK_Y)
            .width(CANVAS_W)
            .height(130.0)
            .child(
                Opacity::new(a).child(
                    Text::new(MARK).style(
                        TextStyle::new(MARK_SIZE)
                            .weight(FontWeight::Regular)
                            .letter_spacing(spacing)
                            .color(mix(alpha(INK, 1.0), tint(VIOLET_SOFT, 0.25), env * 0.6)),
                    )
                    .align(TextAlign::Center),
                ),
            ),
    );

    // The underline — sits quiet under the mark from early on, then the
    // STING sweeps a bright pulse through it, once.
    let rest_w = 540.0;
    let underline = Painting::sized(
        Size::new(rest_w + 4.0, 12.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            // The resting rule — barely there.
            book.rrect(
                Rect::new(0.0, 0.0, rest_w, 3.0),
                1.5,
                alpha(Color::WHITE, 0.10),
            );
            // The sting sweep — a bright bar racing across, once.
            if env > 0.01 {
                let sweep = clamp01((t - STING_T0) / (STING_SPAN * 1.6));
                let head = rest_w * ease_out_cubic(sweep);
                let tail = (head - 130.0 * (1.0 - sweep)).max(0.0);
                book.rrect(
                    xywh(tail, -1.0, (head - tail).max(2.0), 5.0),
                    2.5,
                    Gradient::horizontal().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.0)),
                        (0.5, alpha(VIOLET_SOFT, 0.95 * env + 0.1)),
                        (1.0, alpha(VIOLET, 0.0)),
                    ]),
                );
            }
        }),
    );
    band = band.push(
        Positioned::new()
            .left((CANVAS_W - rest_w) / 2.0)
            .top(MARK_Y + 118.0)
            .width(rest_w + 4.0)
            .height(12.0)
            .child(underline),
    );

    band.into()
}

// ── The install line — typed on with a caret, then settles ──────────────────

fn install_line(t: f32) -> WidgetNode {
    let typed = ((INSTALL.chars().count() as f32) * ease_out_cubic(clamp01((t - TYPE_T0) / TYPE_SPAN))).round() as usize;
    let typed = typed.min(INSTALL.chars().count());
    let visible: String = INSTALL.chars().take(typed).collect();
    let done = typed >= INSTALL.chars().count();

    let mono_adv = 15.5; // 26 px mono advance — layout arithmetic
    let caret_x = (CANVAS_W - INSTALL.chars().count() as f32 * mono_adv) / 2.0
        + typed as f32 * mono_adv;
    let caret_a = if !done {
        0.9 * (0.55 + 0.45 * (t * SECONDS * 2.0).sin())
    } else {
        // One last blink, then gone.
        if (t - TYPE_T0 - TYPE_SPAN) * SECONDS < 1.2 {
            0.6 * (0.5 + 0.5 * (t * SECONDS * 2.0).sin())
        } else {
            0.0
        }
    };

    let mut band = Stack::new();

    // A soft panel behind the install line — the "copy me" affordance.
    band = band.push(Positioned::fill().child(
        Painting::sized(
            Size::new(430.0, 56.0),
            PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                book.rrect(
                    Rect::new(0.0, 0.0, 430.0, 56.0),
                    9.0,
                    alpha(Color::rgb(20, 19, 28), 0.75),
                );
                book.stroke_rrect(
                    Rect::new(0.5, 0.5, 429.0, 55.0),
                    9.0,
                    alpha(Color::WHITE, 0.08),
                    1.0,
                );
            }),
        ),
    ));

    band = band.push(
        Positioned::new()
            .left(0.0)
            .top(14.0)
            .width(430.0)
            .height(30.0)
            .child(
                Text::new(visible).style(
                    TextStyle::new(24.0)
                        .monospace()
                        .weight(FontWeight::Medium)
                        .color(alpha(INK, 0.95)),
                )
                .align(TextAlign::Center),
            ),
    );

    if caret_a > 0.01 {
        band = band.push(
            Positioned::new()
                .left(caret_x - (CANVAS_W - 430.0) / 2.0)
                .top(16.0)
                .width(2.6)
                .height(26.0)
                .child(
                    Painting::sized(
                        Size::new(2.6, 26.0),
                        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                            book.rect(
                                Rect::new(0.0, 0.0, 2.6, 26.0),
                                alpha(VIOLET_SOFT, caret_a),
                            );
                        }),
                    ),
                ),
        );
    }

    let panel = Stack::new().push(
        Positioned::new()
            .left((CANVAS_W - 430.0) / 2.0)
            .top(398.0)
            .width(430.0)
            .height(56.0)
            .child(band),
    );

    Stack::new().push(panel).into()
}

// ── The manifest line — the facts, faint ────────────────────────────────────

fn manifest_line(t: f32) -> WidgetNode {
    let a = clamp01((t - 0.52) / 0.18);
    if a <= 0.01 {
        return Stack::new().into();
    }
    let text = format!(
        "vieww {} · {} crates · {}",
        VERSION, CRATES, TOOLCHAIN
    );

    Stack::new()
        .push(
            Positioned::new()
                .left(0.0)
                .top(476.0)
                .width(CANVAS_W)
                .height(20.0)
                .child(
                    Opacity::new(a).child(
                        Text::new(text).style(
                            TextStyle::new(13.0)
                                .monospace()
                                .letter_spacing(2.0)
                                .color(alpha(MUTED, 0.75)),
                        )
                        .align(TextAlign::Center),
                    ),
                ),
        )
        .into()
}

// ── The closing receipt — the house voice, last line ────────────────────────

fn closing_line(t: f32) -> WidgetNode {
    let a = clamp01((t - 0.80) / 0.14);
    if a <= 0.01 {
        return Stack::new().into();
    }
    Stack::new()
        .push(
            Positioned::new()
                .left(0.0)
                .top(648.0)
                .width(CANVAS_W)
                .height(16.0)
                .child(
                    Opacity::new(a).child(
                        Text::new("rendered by the film's own rasterizer · no post").style(
                            TextStyle::new(11.0)
                                .monospace()
                                .letter_spacing(1.8)
                                .color(alpha(FAINT, 0.7)),
                        )
                        .align(TextAlign::Center),
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

            // The deepest ground of the film — the end card is the darkest
            // it gets, and the quietest.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(8, 8, 11)),
                    (0.55, BG_DEEP),
                    (1.0, Color::rgb(10, 9, 14)),
                ]),
            );

            // Stars — sparsest board in the lab. The last frame's sky.
            let mut rng = Rng::new(0xE1D);
            for _ in 0..30 {
                let x = rng.f01() * w;
                let y = rng.f01() * h;
                let r = 0.4 + rng.f01() * 0.8;
                let tw = 0.5 + 0.5 * (t * 2.4 + rng.f01() * 9.0).sin();
                book.circle(Offset::new(x, y), r, alpha(Color::WHITE, 0.03 + 0.05 * tw));
            }

            // The horizon glow — one, low, wide, violet: the film's floor.
            book.layer(1.0, 44.0, None, |inner| {
                inner.circle(
                    Offset::new(w * 0.5, h * 1.02),
                    w * 0.42,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.10)),
                        (1.0, alpha(VIOLET, 0.0)),
                    ]),
                );
            });

            // The sting's single flash — the whole board lifts for a beat.
            let env = sting_env(t);
            if env > 0.01 {
                book.rect(
                    Rect::new(0.0, 0.0, w, h),
                    alpha(tint(VIOLET, 0.5), 0.05 * env),
                );
            }

            // The vignette — deeper than the lab default: the end card
            // closes in.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.5), 0.72).with_dither().with_stops(&[
                    (0.45, alpha(Color::BLACK, 0.0)),
                    (1.0, alpha(Color::BLACK, 0.55)),
                ]),
            );
        }),
    );

    Stack::new()
        .push(Positioned::fill().child(bg))
        .push(wordmark(t))
        .push(install_line(t))
        .push(manifest_line(t))
        .push(closing_line(t))
        .into()
}
