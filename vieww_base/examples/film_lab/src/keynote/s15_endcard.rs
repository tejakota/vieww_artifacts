//! S15 · THE END CARD — the close, and the audit. 2:39–2:52.
//!
//! The wordmark, still — it does not drop (S03 already dropped it); this
//! is the settled mark, wide-tracked, permanent. `vieww.dev`; the install
//! line, typed; and **the manifest bars** — frames, shapes, glyph runs,
//! layers, blurred layers, stroked paths — the film's own audit, every
//! number counted by the census pass from the command streams the film
//! itself emitted, comma-grouped, never typed.
//!
//! At +4s the sting (K5): *"this film was rendered with vieww."* — one
//! violet accent, once. Then the bench line, the quiet hold. The last
//! frame is the film's most patient.

use vieww_foundation::{
    Color, FontWeight, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle,
};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{alpha, clamp01, ease_out_cubic, mix, smoothstep, spring_out, tint, xywh, BG_DEEP, FAINT, INK, MUTED, Rng, VIOLET, VIOLET_SOFT};

use super::{ease_out_type, group_commas, Ctx};

const MARK: &str = "vieww";
const MARK_SIZE: f32 = 128.0;
const MARK_Y: f32 = 168.0;
const INSTALL: &str = "cargo add vieww";

/// The sting fires at +4s of 13 (K5 at 2:43).
const STING_T: f32 = 4.0 / 13.0;

/// The manifest rows — what the renderer can count, counted.
const ROWS: [(&str, f32); 6] = [
    ("frames", 1.0),
    ("shapes", 1.0),
    ("glyph runs", 1.0),
    ("layers", 1.0),
    ("blurred layers", 1.0),
    ("stroked paths", 1.0),
];

/// The sting's envelope — fast attack, underdamped ring-out.
fn sting_env(t: f32) -> f32 {
    spring_out(clamp01((t - STING_T) / 0.12), 11.0, 0.34)
        * (-((t - STING_T).max(0.0) * 1.6)).exp()
}

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let p = ctx.probe;
    let env = sting_env(t);

    let values = [
        p.frames,
        p.shapes,
        p.glyph_runs,
        p.layers,
        p.filtered,
        p.strokes,
    ];

    let mut stack = Stack::new();

    // The wordmark — present from frame 1; the end card inherits the mark.
    let mark_a = 0.86 + 0.14 * smoothstep(clamp01(t / 0.16));
    stack = stack.push(
        Positioned::new().left(0.0).top(MARK_Y).width(1920.0).height(170.0).child(
            Opacity::new(mark_a).child(
                Text::new(MARK)
                    .style(
                        TextStyle::new(MARK_SIZE)
                            .weight(FontWeight::Regular)
                            .letter_spacing(18.0)
                            .color(mix(alpha(INK, 1.0), tint(VIOLET_SOFT, 0.25), env * 0.6)),
                    )
                    .align(TextAlign::Center),
            ),
        ),
    );

    // The underline — quiet, then the sting sweeps through it, once.
    let rest_w = 760.0;
    stack = stack.push(
        Positioned::new()
            .left((1920.0 - rest_w) / 2.0)
            .top(MARK_Y + 158.0)
            .width(rest_w + 4.0)
            .height(12.0)
            .child(Painting::sized(Size::new(rest_w + 4.0, 12.0), PaintWith::new(
                move |book: &mut Sketchbook, _s: Size| {
                    book.rrect(xywh(0.0, 0.0, rest_w, 3.0), 1.5, alpha(Color::WHITE, 0.10));
                    if env > 0.01 {
                        let sweep = clamp01((t - STING_T) / 0.19);
                        let head = rest_w * ease_out_cubic(sweep);
                        let tail = (head - 180.0 * (1.0 - sweep)).max(0.0);
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
                },
            ))),
    );

    // vieww.dev — under the mark, early and quiet.
    let dev_a = clamp01((t - 0.06) / 0.10);
    if dev_a > 0.0 {
        stack = stack.push(
            Positioned::new().left(0.0).top(MARK_Y + 186.0).width(1920.0).height(30.0).child(
                Opacity::new(dev_a).child(
                    Text::new("vieww.dev")
                        .style(TextStyle::new(24.0).monospace().letter_spacing(6.0).color(alpha(MUTED, 0.9)))
                        .align(TextAlign::Center),
                ),
            ),
        );
    }

    // The install line — typed, in a copy-me panel.
    let typed = ((INSTALL.chars().count() as f32)
        * ease_out_type(clamp01((t - 0.10) / 0.16)))
    .round() as usize;
    let typed = typed.min(INSTALL.chars().count());
    let visible: String = INSTALL.chars().take(typed).collect();
    let panel_w = 560.0;
    stack = stack
        .push(
            Positioned::new()
                .left((1920.0 - panel_w) / 2.0)
                .top(MARK_Y + 240.0)
                .width(panel_w)
                .height(64.0)
                .child(Painting::sized(Size::new(panel_w, 64.0), PaintWith::new(
                    move |book: &mut Sketchbook, _s: Size| {
                        book.rrect(xywh(0.0, 0.0, panel_w, 64.0), 10.0, alpha(Color::rgb(20, 19, 28), 0.75));
                        book.stroke_rrect(xywh(0.5, 0.5, panel_w - 1.0, 63.0), 10.0, alpha(Color::WHITE, 0.08), 1.0);
                    },
                ))),
        )
        .push(
            Positioned::new()
                .left((1920.0 - panel_w) / 2.0)
                .top(MARK_Y + 258.0)
                .width(panel_w)
                .height(34.0)
                .child(
                    Text::new(visible)
                        .style(TextStyle::new(26.0).monospace().weight(FontWeight::Medium).color(alpha(INK, 0.95)))
                        .align(TextAlign::Center),
                ),
        );

    // The sting's line — the contract, center-frame (K5).
    let sting_a = clamp01((t - STING_T - 0.06) / 0.14);
    if sting_a > 0.0 {
        stack = stack.push(
            Positioned::new().left(0.0).top(496.0).width(1920.0).height(44.0).child(
                Opacity::new(sting_a).child(
                    Text::new("this film was rendered with vieww")
                        .style(TextStyle::new(30.0).monospace().letter_spacing(4.0).color(tint(VIOLET_SOFT, 0.2)))
                        .align(TextAlign::Center),
                ),
            ),
        );
    }

    // The manifest bars — the film's own audit, counted by the census.
    let bars_t0 = 0.44f32;
    if t > bars_t0 {
        for (i, (label, _)) in ROWS.iter().enumerate() {
            let row_t = clamp01((t - bars_t0 - i as f32 * 0.055) / 0.20);
            if row_t <= 0.0 {
                continue;
            }
            let value = values[i];
            let shown = (value as f32 * spring_out(row_t, 12.0, 0.72)) as u64;
            // Bar length — log-scaled (the rows span four orders of
            // magnitude; a linear bar would be a lie of emphasis).
            let frac = value.max(1) as f32;
            let bar = ((frac.log10() - 3.0) / 3.6).clamp(0.04, 1.0) * spring_out(row_t, 12.0, 0.72);
            let y = 580.0 + i as f32 * 46.0;
            stack = stack
                .push(
                    Positioned::new().left(430.0).top(y - 4.0).width(280.0).height(28.0).child(
                        Text::new(*label)
                            .style(TextStyle::new(19.0).monospace().letter_spacing(1.6).color(alpha(MUTED, 0.9)))
                            .align(TextAlign::Right),
                    ),
                )
                .push(
                    Positioned::new().left(740.0).top(y + 4.0).width(660.0).height(12.0).child(
                        Painting::sized(Size::new(660.0, 12.0), PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                            book.rrect(xywh(0.0, 0.0, 660.0, 6.0), 3.0, alpha(Color::WHITE, 0.06));
                            book.rrect(
                                xywh(0.0, 0.0, 660.0 * bar, 6.0),
                                3.0,
                                alpha(VIOLET_SOFT, 0.85),
                            );
                        })),
                    ),
                )
                .push(
                    Positioned::new().left(1420.0).top(y - 4.0).width(300.0).height(28.0).child(
                        Text::new(group_commas(shown))
                            .style(TextStyle::new(20.0).monospace().color(alpha(INK, 0.9)))
                            .align(TextAlign::Left),
                    ),
                );
        }
    }

    // The bench line — per-bench determinism names its bench (ledger 12).
    let bench_a = clamp01((t - 0.80) / 0.10);
    if bench_a > 0.0 {
        let bench = if p.bench.is_empty() { "census pending".to_string() } else { p.bench.clone() };
        let frames_txt = format!("10,800 frames @ 60 · {}", bench);
        let _ = frames_txt;
        stack = stack.push(
            Positioned::new().left(0.0).top(906.0).width(1920.0).height(24.0).child(
                Opacity::new(bench_a).child(
                    Text::new(format!("{} · {}", group_commas(p.frames.max(10800)), bench))
                        .style(TextStyle::new(15.0).monospace().letter_spacing(2.2).color(alpha(FAINT, 0.8)))
                        .align(TextAlign::Center),
                ),
            ),
        );
    }

    // The closing receipt — the house voice, last line.
    let close_a = clamp01((t - 0.86) / 0.10);
    if close_a > 0.0 {
        stack = stack.push(
            Positioned::new().left(0.0).top(1004.0).width(1920.0).height(20.0).child(
                Opacity::new(close_a).child(
                    Text::new("rendered by the film's own rasterizer · no post")
                        .style(TextStyle::new(14.0).monospace().letter_spacing(2.4).color(alpha(FAINT, 0.7)))
                        .align(TextAlign::Center),
                ),
            ),
        );
    }

    // The ground — the deepest, the quietest.
    let bg = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(8, 8, 11)),
                    (0.55, BG_DEEP),
                    (1.0, Color::rgb(10, 9, 14)),
                ]),
            );
            let mut rng = Rng::new(0xE1D);
            for _ in 0..40 {
                let x = rng.f01() * w;
                let y = rng.f01() * h;
                let r = 0.4 + rng.f01() * 0.8;
                let tw = 0.5 + 0.5 * (t * 2.4 + rng.f01() * 9.0).sin();
                book.circle(Offset::new(x, y), r, alpha(Color::WHITE, 0.03 + 0.05 * tw));
            }
            // The horizon glow — one, low, wide, violet: the film's floor.
            book.layer(1.0, 48.0, None, |g| {
                g.circle(
                    Offset::new(w * 0.5, h * 1.02),
                    w * 0.42,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.10)),
                        (1.0, alpha(VIOLET, 0.0)),
                    ]),
                );
            });
            // The sting's single flash — the whole board lifts, once.
            if env > 0.01 {
                book.rect(Rect::new(0.0, 0.0, w, h), alpha(tint(VIOLET, 0.5), 0.05 * env));
                super::glow(book, w * 0.5, MARK_Y + 80.0, 420.0 + 120.0 * env, VIOLET, 0.16 * env);
            }
            super::vignette(book, w, h, 0.55);
        }),
    );

    Stack::new()
        .push(Positioned::fill().child(bg))
        .push(stack)
        .into()
}
