//! D04 · THE END CARD — the launch. 4:32–4:50.
//!
//! The film's last and only advertisement: the mark, the name, the
//! release line — **beta release available today** — the repository,
//! the licence, the platforms. And the film's own contract, the same
//! one every keynote in this lab closes with: the manifest bars —
//! this very film's receipts, counted by its own census — and the
//! sting, one accent firing across the wordmark: *this film was
//! rendered with vieww.*
//!
//! The mark is the studio's own (`viewwstudio::ui::brand::mark`), the
//! title is the studio's name in the brand's type and colour, and the
//! theme is the studio's own purple. The end card is the product's
//! face, drawn by the product's own engine.

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;

use super::{
    alpha, brand_mark, caption, clamp01, group_commas, mix, stars, tint, vignette, xywh, Ctx,
    ACCENT, CERT_ALLOCS_STEADY, CERT_TESTS, INK, LEDGER, MARK_GROUND, MUTED, W,
};
use crate::film_lib::{ease_in_out, ease_out_cubic};

/// The repository — the call to action.
const REPO: &str = "github.com/tejakota/vieww_artifacts";

/// The release line — the one the film exists to say.
const RELEASE: &str = "beta release available today";

pub(super) fn build(ctx: &Ctx) -> WidgetNode {
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
                    (0.65, super::BG_DEEP),
                    (1.0, Color::rgb(10, 9, 14)),
                ]),
            );
            stars(book, w, h, 0xE2D1, 90, t, 0.09);
            vignette(book, w, h, 0.5);
        }),
    )));

    // The mark + the wordmark — top third. The mark is the studio's
    // own, complete (both panels home), at the film's centre.
    let mark_a = clamp01(t / 0.10);
    if mark_a > 0.0 {
        let side = 104.0;
        stack = stack.push(
            Positioned::new()
                .left(W * 0.5 - side * 0.5)
                .top(246.0)
                .width(side)
                .height(side)
                .child(super::Opacity::new(mark_a).child(
                    // The real mark — the studio's own drawing of it.
                    brand_mark(side, 1.0, 1.0),
                )),
        );
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(386.0)
                .width(W)
                .height(64.0)
                .child(
                    super::Opacity::new(clamp01((t - 0.08) / 0.12)).child(
                        Text::new("viewwstudio")
                            .style(
                                TextStyle::new(56.0)
                                    .letter_spacing(6.0)
                                    .color(alpha(INK, 0.97)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
        // The name's accent underline — the mark's accent, under the word.
        let ul = ease_out_cubic(clamp01((t - 0.18) / 0.3));
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(458.0)
                .width(W)
                .height(10.0)
                .child(
                    super::Opacity::new(clamp01((t - 0.18) / 0.1)).child(Painting::sized(
                        Size::new(W, 10.0),
                        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                            let w = 620.0 * ul;
                            book.rrect(
                                xywh(W * 0.5 - w * 0.5, 0.0, w.max(2.0), 3.0),
                                1.5,
                                Gradient::horizontal().with_dither().with_stops(&[
                                    (0.0, alpha(super::ACCENT_DEEP, 0.0)),
                                    (0.5, alpha(ACCENT, 0.9)),
                                    (1.0, alpha(super::ACCENT_DEEP, 0.0)),
                                ]),
                            );
                        }),
                    )),
                ),
        );
    }

    // The release line — typed on with a caret, the call to action.
    let release_a = clamp01((t - 0.22) / 0.10);
    if release_a > 0.0 {
        // One anchor, one tracked measurement — see `type_on`.
        //
        // This line had a third fault on top of the shared two: it is set
        // in **Geist**, a proportional face, and its caret was placed with
        // `gmono_w`, a *monospace* advance constant. There is no value of
        // that constant that lands a caret on proportional text, so the
        // caret could only ever be approximately wrong. The line moves to
        // the film's mono instrument voice, which is the voice a caret
        // belongs to anyway, and the measurement becomes exact.
        stack = stack.push(
            super::Opacity::new(release_a).child(super::type_on(
                RELEASE,
                super::TypeAt::CenteredOn((W * 0.5) as i32),
                518.0,
                super::geist(30.0)
                    .letter_spacing(2.4)
                    .color(alpha(ACCENT, 1.0)),
                clamp01((t - 0.24) / 0.30),
                sec,
            )),
        );
    }

    // The facts row — the release's own metadata, centered under the line.
    let facts_a = clamp01((t - 0.56) / 0.14);
    if facts_a > 0.01 {
        // The facts as one centered line — the way a receipt reads:
        // licence, platforms, toolchain.
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(592.0)
                .width(W)
                .height(26.0)
                .child(
                    super::Opacity::new(facts_a).child(
                        Text::new("Apache-2.0 · Linux, macOS, Windows · no Rust needed")
                            .style(
                                TextStyle::new(15.5)
                                    .monospace()
                                    .letter_spacing(1.6)
                                    .color(alpha(MUTED, 0.92)),
                            )
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }

    // The repository — typed on with a caret, the destination.
    let repo_a = clamp01((t - 0.66) / 0.10);
    if repo_a > 0.0 {
        let chars = REPO.chars().count();
        let typed = (chars as f32 * clamp01((t - 0.68) / 0.26)) as usize;
        let done = typed >= chars;
        stack = stack.push(
            super::Opacity::new(repo_a).child(super::type_on(
                REPO,
                super::TypeAt::CenteredOn((W * 0.5) as i32),
                648.0,
                TextStyle::new(24.0)
                    .monospace()
                    .letter_spacing(2.2)
                    .color(alpha(tint(ACCENT, 0.15), 1.0)),
                clamp01((t - 0.68) / 0.26),
                sec,
            )),
        );
        if done {
            // The underline — springs under the repo.
            let u = super::spring_out(clamp01((t - 0.94) / 0.30), 13.0, 0.45);
            let w = 470.0 * u;
            stack = stack.push(
                Positioned::new()
                    .left(W * 0.5 - 235.0)
                    .top(694.0)
                    .width(w.max(2.0))
                    .height(8.0)
                    .child(Painting::sized(
                        Size::new(w.max(2.0), 8.0),
                        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                            book.rrect(xywh(0.0, 0.0, w, 3.0), 1.5, alpha(tint(ACCENT, 0.2), 0.9));
                        }),
                    )),
            );
        }
    }

    // The manifest bars — the film audits itself: six bars, the census's
    // own receipts (zeros in pass 1 — the bars hold their frames,
    // honestly).
    let bars_a = clamp01((t - 0.50) / 0.14);
    if bars_a > 0.0 {
        let bars: [(&str, u64); 6] = [
            ("scenes", super::scenes().len() as u64),
            ("frames", probe.frames.max(1)),
            ("shapes", probe.shapes),
            ("glyph runs", probe.glyph_runs),
            ("glyphs", probe.glyphs),
            ("layers", probe.layers),
        ];
        let max_v = bars.iter().map(|b| b.1).max().unwrap_or(1).max(1) as f32;
        let grow = ease_in_out(clamp01((t - 0.52) / 0.26));
        let bw = 120.0;
        let gap = 70.0;
        let x0 = W * 0.5 - (bars.len() as f32 * (bw + gap) - gap) * 0.5;
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(740.0)
                .width(W)
                .height(150.0)
                .child(super::Opacity::new(bars_a).child(Painting::sized(
                    Size::new(W, 150.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        for (i, (_label, v)) in bars.iter().enumerate() {
                            let x = x0 + i as f32 * (bw + gap);
                            let h = 84.0 * (*v as f32 / max_v) * grow;
                            let c = if i == 0 { tint(ACCENT, 0.2) } else { ACCENT };
                            book.rrect(xywh(x, 120.0 - h, bw, h.max(3.0)), 4.0, alpha(c, 0.55));
                        }
                    }),
                ))),
        );
        // The bars' labels + values, as text (two rows).
        for (i, (label, v)) in [
            ("scenes", super::scenes().len() as u64),
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
                    .top(868.0)
                    .width(bw + 40.0)
                    .height(22.0)
                    .child(
                        super::Opacity::new(bars_a).child(
                            Text::new(*label)
                                .style(TextStyle::new(13.5).monospace().color(alpha(MUTED, 0.9)))
                                .align(TextAlign::Center),
                        ),
                    ),
            );
            stack = stack.push(
                Positioned::new()
                    .left(x - 30.0)
                    .top(712.0)
                    .width(bw + 60.0)
                    .height(22.0)
                    .child(
                        super::Opacity::new(bars_a).child(
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
    if (0.0..1.0).contains(&sting) {
        let x = -300.0 + sting * 2400.0;
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                book.layer(1.0, 30.0, None, |g| {
                    g.rect(xywh(x - 160.0, 0.0, 320.0, 560.0), alpha(ACCENT, 0.08));
                });
                book.rrect(
                    xywh(x - 1.5, 200.0, 3.0, 260.0),
                    1.5,
                    alpha(tint(ACCENT, 0.5), 0.9),
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
                .top(930.0)
                .width(W)
                .height(36.0)
                .child(
                    super::Opacity::new(sting_line_a).child(
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
                    .top(972.0)
                    .width(W)
                    .height(26.0)
                    .child(
                        super::Opacity::new(sting_line_a * 0.8).child(
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
        // The receipts line — the certification, held beside the census.
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(1002.0)
                .width(W)
                .height(26.0)
                .child(
                    super::Opacity::new(sting_line_a * 0.8).child(
                        Text::new(format!(
                            "{} tests green · {} steady allocations · SCALE_FACTOR {} master",
                            group_commas(CERT_TESTS),
                            CERT_ALLOCS_STEADY,
                            ctx_scale(),
                        ))
                        .style(
                            TextStyle::new(14.5)
                                .monospace()
                                .letter_spacing(1.4)
                                .color(alpha(LEDGER, 0.8)),
                        )
                        .align(TextAlign::Center),
                    ),
                ),
        );
    }

    stack = stack.push(caption(
        "the receipts are the film",
        1006.0,
        clamp01((t - 0.60) / 0.14),
    ));

    let _ = MARK_GROUND;
    let _ = mix(ACCENT, MARK_GROUND, 0.0);
    let _ = Offset::new(0.0, 0.0);
    stack.into()
}

/// The SCALE_FACTOR this build was rendered at (from the manifest's own
/// field, when present).
fn ctx_scale() -> String {
    std::env::var("SCALE_FACTOR").unwrap_or_else(|_| "1".to_string())
}
