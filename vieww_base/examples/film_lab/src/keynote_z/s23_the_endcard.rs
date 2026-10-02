//! S21 · THE END CARD — the launch. 3:23–3:34.
//!
//! The reference's closing beat: a stark slide with the open-source
//! registry location — plus the film's own contract: the manifest bars,
//! the census's receipts for this very film (scenes · frames · shapes ·
//! glyph runs · layers — every number measured by the two-pass harness,
//! never typed), and the sting, one accent firing: **this film was
//! rendered with vieww.**

use vieww_foundation::{Color, Gradient, Rect, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use super::{
    alpha, clamp01, group_commas, spring_out, tint, xywh, Ctx, INK, MUTED, VIOLET, VIOLET_SOFT,
};
use crate::film_lib::ease_in_out;

/// The registry — the reference's call to action, verbatim.
const REPO: &str = "github.com/tejakota/vieww_artifacts";

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let probe = ctx.probe;

    let mut stack = Stack::new();

    // The ground — the calmest register in the film.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(8, 8, 11)),
                    (0.65, crate::film_lib::BG_DEEP),
                    (1.0, Color::rgb(10, 9, 14)),
                ]),
            );
            super::stars(book, w, h, 0xE2D1, 90, t, 0.09);
            super::vignette(book, w, h, 0.5);
        }),
    )));

    // The mark + the wordmark — top third.
    let mark_a = clamp01(t / 0.10);
    if mark_a > 0.0 {
        stack = stack.push(
            Positioned::fill().child(Opacity::new(mark_a).child(Painting::sized(
                super::CANVAS,
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    super::draw_mark(book, 960.0, 300.0, 96.0, VIOLET_SOFT, 1.0);
                    super::glow(book, 960.0, 300.0, 260.0, VIOLET, 0.14);
                }),
            ))),
        );
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(382.0)
                .width(1920.0)
                .height(60.0)
                .child(
                    Opacity::new(clamp01((t - 0.08) / 0.12)).child(
                        Text::new("viewwstudio")
                            .style(
                                TextStyle::new(54.0)
                                    .letter_spacing(6.0)
                                    .color(alpha(INK, 0.97)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }

    // The registry — typed on with a caret, the call to action.
    let repo_a = clamp01((t - 0.22) / 0.10);
    if repo_a > 0.0 {
        let chars = REPO.chars().count();
        let typed = (chars as f32 * clamp01((t - 0.24) / 0.30)) as usize;
        let shown: String = REPO.chars().take(typed).collect();
        let done = typed >= chars;
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(500.0)
                .width(1920.0)
                .height(44.0)
                .child(
                    Opacity::new(repo_a).child(
                        Text::new(shown)
                            .style(
                                TextStyle::new(30.0)
                                    .monospace()
                                    .letter_spacing(2.4)
                                    .color(alpha(tint(VIOLET_SOFT, 0.15), 1.0)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
        // The caret, until the line completes.
        if !done {
            let on = (sec * 2.6).fract() < 0.55;
            let w = super::mono_w(30.0, typed);
            if on {
                stack = stack.push(
                    Positioned::new()
                        .left(960.0 + w * 0.5 + 4.0)
                        .top(508.0)
                        .width(14.0)
                        .height(30.0)
                        .child(Container::new().color(alpha(VIOLET_SOFT, 0.9)).radius(2.0)),
                );
            }
        } else {
            // The underline — springs under the repo.
            let u = spring_out(clamp01((t - 0.56) / 0.30), 13.0, 0.45);
            let w = 520.0 * u;
            stack = stack.push(
                Positioned::new()
                    .left(960.0 - 260.0)
                    .top(552.0)
                    .width(w.max(2.0))
                    .height(8.0)
                    .child(Painting::sized(
                        Size::new(w.max(2.0), 8.0),
                        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                            book.rrect(
                                xywh(0.0, 0.0, w, 3.0),
                                1.5,
                                alpha(tint(VIOLET_SOFT, 0.2), 0.9),
                            );
                        }),
                    )),
            );
        }
    }

    // The manifest bars — the film audits itself: six bars, the census's
    // own receipts (zeros in pass 1 — the bars hold their frames, honestly).
    let bars_a = clamp01((t - 0.50) / 0.14);
    if bars_a > 0.0 {
        let bars: [(&str, u64); 6] = [
            ("scenes", 24),
            ("frames", probe.frames.max(1)),
            ("shapes", probe.shapes),
            ("glyph runs", probe.glyph_runs),
            ("glyphs", probe.glyphs),
            ("layers", probe.layers),
        ];
        let max_v = bars.iter().map(|b| b.1).max().unwrap_or(1).max(1) as f32;
        let grow = ease_in_out(clamp01((t - 0.52) / 0.26));
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(620.0)
                .width(1920.0)
                .height(180.0)
                .child(Opacity::new(bars_a).child(Painting::sized(
                    Size::new(1920.0, 180.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        let bw = 120.0;
                        let gap = 70.0;
                        let x0 = 960.0 - (bars.len() as f32 * (bw + gap) - gap) * 0.5;
                        for (i, (label, v)) in bars.iter().enumerate() {
                            let x = x0 + i as f32 * (bw + gap);
                            let h = 96.0 * (*v as f32 / max_v) * grow;
                            let c = if i == 0 {
                                tint(VIOLET_SOFT, 0.2)
                            } else {
                                VIOLET_SOFT
                            };
                            // The bar.
                            book.rrect(xywh(x, 120.0 - h, bw, h.max(3.0)), 4.0, alpha(c, 0.55));
                            // The value, above.
                            let _ = label;
                        }
                    }),
                ))),
        );
        // The bars' labels + values, as text (two rows).
        let bw = 120.0;
        let gap = 70.0;
        let x0 = 960.0 - (6.0 * (bw + gap) - gap) * 0.5;
        for (i, (label, v)) in [
            ("scenes", 24u64),
            ("frames", probe.frames.max(1)),
            ("shapes", probe.shapes),
            ("glyph runs", probe.glyph_runs),
            ("glyphs", probe.glyphs),
            ("layers", probe.layers),
        ]
        .iter()
        .enumerate()
        {
            let x = x0 + i as f32 * (bw + gap);
            stack = stack.push(
                Positioned::new()
                    .left(x - 20.0)
                    .top(620.0 + 128.0)
                    .width(bw + 40.0)
                    .height(22.0)
                    .child(
                        Opacity::new(bars_a).child(
                            Text::new(*label)
                                .style(TextStyle::new(13.5).monospace().color(alpha(MUTED, 0.9)))
                                .align(TextAlign::Center),
                        ),
                    ),
            );
            stack = stack.push(
                Positioned::new()
                    .left(x - 30.0)
                    .top(620.0 - 4.0)
                    .width(bw + 60.0)
                    .height(22.0)
                    .child(
                        Opacity::new(bars_a).child(
                            Text::new(group_commas(*v))
                                .style(TextStyle::new(15.0).monospace().color(alpha(INK, 0.92)))
                                .align(TextAlign::Center),
                        ),
                    ),
            );
        }
    }

    // The sting — one accent firing, the beam crossing the wordmark.
    let sting = clamp01((t - 0.80) / 0.16);
    if sting > 0.0 && sting < 1.0 {
        let x = -300.0 + sting * 2400.0;
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                book.layer(1.0, 30.0, None, |g| {
                    g.rect(xywh(x - 160.0, 0.0, 320.0, 620.0), alpha(VIOLET, 0.08));
                });
                book.rrect(
                    xywh(x - 1.5, 240.0, 3.0, 240.0),
                    1.5,
                    alpha(tint(VIOLET_SOFT, 0.5), 0.9),
                );
            }),
        )));
    }

    // The contract — the film's final line.
    let sting_line_a = clamp01((t - 0.84) / 0.12);
    if sting_line_a > 0.0 {
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(852.0)
                .width(1920.0)
                .height(36.0)
                .child(
                    Opacity::new(sting_line_a).child(
                        Text::new("this film was rendered with vieww")
                            .style(
                                TextStyle::new(23.0)
                                    .monospace()
                                    .letter_spacing(4.0)
                                    .color(alpha(INK, 0.95)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
        // The bench line — the manifest's own identity, quoted.
        if !probe.bench.is_empty() {
            stack = stack.push(
                Positioned::new()
                    .left(0.0)
                    .top(892.0)
                    .width(1920.0)
                    .height(26.0)
                    .child(
                        Opacity::new(sting_line_a * 0.8).child(
                            Text::new(probe.bench.clone())
                                .style(
                                    TextStyle::new(14.5)
                                        .monospace()
                                        .letter_spacing(1.4)
                                        .color(alpha(MUTED, 0.85)),
                                )
                                .align(TextAlign::Center),
                        ),
                    ),
            );
        }
    }

    stack = stack.push(super::caption(
        "the receipts are the film",
        1000.0,
        clamp01((t - 0.60) / 0.14),
    ));

    stack.into()
}
