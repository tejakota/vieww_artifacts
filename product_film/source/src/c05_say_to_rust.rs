//! C05 · SAY TO RUST — a second language, one real compile. 2:50–3:06.
//!
//! The film opens `counter.say` and types the program in bursts — an
//! English-facing screen language: *keep a whole number called count…*
//! Then it presses the real **Render** — and the real pipeline runs:
//! say-codegen turns the program into the Rust a person would have
//! written, the studio's `rustc` compiles it to a `cdylib`, the studio
//! `dlopen`s it, and the compiled counter mounts in the preview. The
//! film taps **Add one** — a real tap, on a really compiled screen —
//! and the count goes 0 → 1.
//!
//! The pipeline's receipt rides the scene as a chip rail: say → rust →
//! cdylib → dlopen → preview, each lighting as it happens.

use vieww_foundation::TextAlign;
use vieww_widget::prelude::*;
use vieww_widget::WidgetNode;

use super::{
    ACCENT, Ctx, INK, LEDGER, SYN_FUNCTION, SYN_KEYWORD, SYN_MACRO, SYN_TYPE, W, alpha, caption,
    chip_row, clamp01, studio_chrome, tap_ring_at,
};
use super::script::TAP_ADD_ONE;

/// The pipeline's stages, and the film-times they light at.
const STAGES: [(&str, f32); 5] = [
    ("say", 176.2),
    ("rust", 176.7),
    ("cdylib", 177.2),
    ("dlopen", 177.6),
    ("preview", 178.0),
];

/// The Render tap's film-time.
const RENDER_AT: f32 = 176.2;
/// The Add-one tap's film-time.
const ADD_ONE_AT: f32 = 178.62;

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = studio_chrome(ctx);

    // The say program — quoted over the editor, the lines the film is
    // typing, in the say language's own words.
    let say_a = clamp01((t - 0.04) / 0.12);
    if say_a > 0.01 {
        let lines = [
            ("keep a whole number called count starting at 0", SYN_KEYWORD),
            ("", INK),
            ("screen \"Home\":", SYN_FUNCTION),
            ("    a column, spaced 16:", SYN_TYPE),
            ("        a heading \"Counter\"", SYN_TYPE),
            ("        a label \"Tapped \\(count) times\"", SYN_MACRO),
            ("        a button \"Add one\" which when tapped:", LEDGER),
            ("            add 1 to count", LEDGER),
        ];
        for (i, (line, color)) in lines.iter().enumerate() {
            if line.is_empty() {
                continue;
            }
            stack = stack.push(
                vieww_widget::Positioned::new()
                    .left(300.0)
                    .top(176.0 + i as f32 * 30.0)
                    .width(900.0)
                    .height(28.0)
                    .child(
                        vieww_widget::Opacity::new(say_a).child(
                            vieww_widget::Text::new(line.to_string())
                                .style(
                                    super::geist_mono(17.0)
                                        .letter_spacing(0.8)
                                        .color(alpha(*color, 0.9)),
                                )
                                .align(TextAlign::Left),
                        ),
                    ),
            );
        }
    }

    // The pipeline rail — five stages, each lighting at its time, with
    // the connecting line drawing itself between them.
    let rail_a = clamp01((t - 0.10) / 0.1);
    let ctx_abs = ctx.abs;
    if rail_a > 0.01 {
        stack = stack.push(
            vieww_widget::Positioned::new()
                .left(300.0)
                .top(470.0)
                .width(1320.0)
                .height(60.0)
                .child(
                    vieww_widget::Opacity::new(rail_a).child(Painting::sized(
                        Size::new(1320.0, 60.0),
                        PaintWith::new(move |book: &mut vieww_foundation::Sketchbook, _s: Size| {
                            let stages: [(&str, f32); 5] = [
                                ("say-codegen", 176.2),
                                ("rustc", 176.7),
                                ("cdylib", 177.2),
                                ("dlopen", 177.6),
                                ("preview", 178.0),
                            ];
                            let step = 1320.0 / (stages.len() as f32 - 1.0);
                            let lit_through = ctx_abs_stage(stages, ctx_abs);
                            // The connecting line — drawn to the furthest
                            // lit stage.
                            let full = (stages.len() - 1) as f32;
                            let lit_f = lit_through.min(full);
                            book.line(
                                Offset::new(30.0, 30.0),
                                Offset::new(30.0 + step * lit_f, 30.0),
                                alpha(LEDGER, 0.6),
                                2.0,
                            );
                            for (i, (name, at)) in stages.iter().enumerate() {
                                let x = 30.0 + step * i as f32;
                                let lit = ctx_abs >= *at;
                                let c = if lit { LEDGER } else { super::MUTED };
                                book.circle(Offset::new(x, 30.0), if lit { 6.0 } else { 4.0 }, alpha(c, if lit { 0.95 } else { 0.4 }));
                                if lit {
                                    book.ring(Offset::new(x, 30.0), 10.0, 1.2, alpha(c, 0.4));
                                }
                                let _ = name;
                            }
                        }),
                    )),
                ),
        );
        // The stage labels — as text over the painting's nodes.
        for (i, (name, at)) in [
            ("say-codegen", 176.2),
            ("rustc", 176.7),
            ("cdylib", 177.2),
            ("dlopen", 177.6),
            ("preview", 178.0),
        ]
        .iter()
        .enumerate()
        {
            let lit = ctx.abs >= *at;
            let step = 1320.0 / 4.0;
            stack = stack.push(
                vieww_widget::Positioned::new()
                    .left(300.0 + 30.0 + step * i as f32 - 70.0)
                    .top(506.0)
                    .width(140.0)
                    .height(24.0)
                    .child(
                        vieww_widget::Opacity::new(rail_a).child(
                            vieww_widget::Text::new(name.to_string())
                                .style(
                                    super::geist_mono(14.0)
                                        .letter_spacing(1.2)
                                        .color(alpha(if lit { LEDGER } else { super::MUTED }, if lit { 0.95 } else { 0.6 })),
                                )
                                .align(TextAlign::Center),
                        ),
                    ),
            );
        }
    }

    // The compile state callout — while the studio compiles, the frame
    // says what the studio is doing (its real Compiling state).
    let compiling = ctx.abs >= RENDER_AT && ctx.abs < 177.0;
    if compiling {
        stack = stack.push(
            vieww_widget::Positioned::new()
                .left(0.0)
                .top(560.0)
                .width(W)
                .height(30.0)
                .child(
                    vieww_widget::Text::new("rustc is running — a real compile, held on the film's clock")
                        .style(super::geist_mono(16.0).letter_spacing(1.6).color(alpha(SYN_FUNCTION, 0.95)))
                        .align(TextAlign::Center),
                ),
        );
    }

    // The Add-one tap — the ring blooms at the button's real position.
    let since = (ctx.abs - ADD_ONE_AT) / 1.0;
    if (0.0..1.0).contains(&since) {
        stack = stack.push(tap_ring_at(TAP_ADD_ONE, since));
    }
    // The result callout — the count, after the tap.
    if ctx.abs >= ADD_ONE_AT + 0.1 {
        let a = clamp01((ctx.abs - ADD_ONE_AT - 0.1) / 0.3);
        stack = stack.push(
            vieww_widget::Positioned::new()
                .left(0.0)
                .top(560.0)
                .width(W)
                .height(30.0)
                .child(
                    vieww_widget::Opacity::new(a).child(
                        vieww_widget::Text::new("count: 0 → 1 — a real tap, on a really compiled screen")
                            .style(super::geist_mono(16.0).letter_spacing(1.6).color(alpha(LEDGER, 0.95)))
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }

    // The captions.
    stack = stack.push(caption(
        "say — an English-facing screen language, generating the Rust you would have written",
        1002.0,
        clamp01((t - 0.05) / 0.12),
    ));
    stack = stack.push(caption(
        "render: the real compile pipeline, say-codegen to dlopen, inside the studio",
        966.0,
        clamp01((t - 0.55) / 0.12),
    ));

    // The receipt chips.
    stack = stack.push(chip_row(
        &[
            ("say → rust", SYN_KEYWORD),
            ("rustc, for real", SYN_FUNCTION),
            ("cdylib, loaded", ACCENT),
        ],
        // The editor's empty lower band — see `receipt_row`. Pinned
        // right, these rows rendered straight through the preview
        // pane's description paragraph in every studio scene.
        360.0,
        924.0,
        clamp01((sec - 1.2) / 0.5),
    ));

    let _ = STAGES;
    let _ = RENDER_AT;
    stack.into()
}

/// How many stages the film-time has lit, plus the fraction into the
/// next (for the rail's line-draw).
fn ctx_abs_stage(stages: [(&str, f32); 5], abs: f32) -> f32 {
    let mut lit = 0.0f32;
    for (i, (_, at)) in stages.iter().enumerate() {
        if abs >= *at {
            lit = i as f32;
            // The fraction into the *next* stage.
            if let Some((_, next_at)) = stages.get(i + 1) {
                let span = (next_at - at).max(0.001);
                let f = ((abs - at) / span).min(1.0);
                lit = i as f32 + f;
            }
        }
    }
    lit
}
