//! D01 · THE LEDGER — the proof, counted. 3:52–4:06.
//!
//! The receipts, as receipts: the **2026-09-15 certification run**,
//! quoted verbatim from the repo's own status file — startup, p95
//! frame, worst frame, the steady-allocation count, the test count —
//! and beside them the film's own census: frames, shapes, glyph runs,
//! the measured edit→pixels latency. Two columns: what the engine was
//! certified to, and what this film itself measured. Nothing on screen
//! was typed by a human; the film counts its own audit up in front of
//! you.
//!
//! The register is the ledger's mint — the receipts' colour.

use vieww_foundation::{Color, Offset, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;

use super::{
    CERT_ALLOCS_STEADY, CERT_CRATES, CERT_FRAMES_STEADY, CERT_P95_MS, CERT_STARTUP_MS,
    CERT_TESTS, CERT_VULKAN_TESTS, CERT_WORST_MS, Ctx, INK, LEDGER, MUTED, W, alpha, caption,
    clamp01, count_up, distance_chip, grain, ground, group_commas, progress_rail, tint,
    vignette, xywh,
};
use crate::film_lib::ease_out_cubic;

/// The two receipt columns' geometry.
const L_X: f32 = 460.0;
const R_X: f32 = 1040.0;
const Y0: f32 = 220.0;
const ROW_H: f32 = 74.0;

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let probe = ctx.probe;
    let frame_i = (ctx.abs * 60.0) as u64;

    let room = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            ground(book, w, h);
            vignette(book, w, h, 0.5);
            grain(book, w, h, frame_i, 0.3);
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(room));

    // The ledger's card — one surface under both columns.
    let card_a = ease_out_cubic(clamp01((t - 0.03) / 0.15));
    if card_a > 0.01 {
        stack = stack.push(
            Positioned::new()
                .left(L_X - 60.0)
                .top(Y0 - 90.0)
                .width(1060.0)
                .height(660.0)
                .child(
                    super::Opacity::new(card_a).child(Painting::sized(
                        Size::new(1060.0, 660.0),
                        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                            book.rrect(xywh(0.0, 0.0, 1060.0, 660.0), 16.0, alpha(super::SURFACE, 0.92));
                            book.stroke_rrect(xywh(0.0, 0.0, 1060.0, 660.0), 16.0, alpha(LEDGER, 0.25), 1.4);
                            // The card's top rule — the ledger's binding.
                            book.rrect(xywh(24.0, 70.0, 1012.0, 2.0), 1.0, alpha(LEDGER, 0.30));
                        }),
                    )),
                ),
        );
    }

    // The column headers.
    let head_a = clamp01((t - 0.12) / 0.12);
    if head_a > 0.01 {
        for (text, x) in [
            ("the engine, certified", L_X),
            ("this film, measured", R_X),
        ] {
            stack = stack.push(
                Positioned::new()
                    .left(x)
                    .top(Y0 - 52.0)
                    .width(480.0)
                    .height(28.0)
                    .child(
                        super::Opacity::new(head_a).child(
                            Text::new(text)
                                .style(
                                    TextStyle::new(16.0)
                                        .monospace()
                                        .letter_spacing(2.6)
                                        .color(alpha(tint(LEDGER, 0.25), 1.0)),
                                )
                                .align(TextAlign::Left),
                        ),
                    ),
            );
        }
    }

    // ── The left column — the certification receipts, counting up ──
    let cert_rows: [(&str, u64); 5] = [
        ("workspace tests, passing", CERT_TESTS),
        ("vulkan tests, pixel-verified", CERT_VULKAN_TESTS),
        ("steady-state allocations / 60 frames", CERT_ALLOCS_STEADY),
        ("crates in the engine", CERT_CRATES as u64),
        ("typography drift, pixels", 0),
    ];
    let cert_a = clamp01((t - 0.18) / 0.1);
    if cert_a > 0.01 {
        for (i, (label, value)) in cert_rows.iter().enumerate() {
            let row_a = clamp01((cert_a - i as f32 * 0.10) / 0.6);
            if row_a <= 0.01 {
                continue;
            }
            let n = count_up(*value, row_a);
            let y = Y0 + i as f32 * ROW_H;
            stack = stack.push(
                Positioned::new()
                    .left(L_X)
                    .top(y)
                    .width(420.0)
                    .height(24.0)
                    .child(
                        super::Opacity::new(row_a).child(
                            Text::new(label.to_string())
                                .style(
                                    TextStyle::new(14.0)
                                        .monospace()
                                        .letter_spacing(1.2)
                                        .color(alpha(MUTED, 0.9)),
                                )
                                .align(TextAlign::Left),
                        ),
                    ),
            );
            stack = stack.push(
                Positioned::new()
                    .left(L_X)
                    .top(y + 26.0)
                    .width(420.0)
                    .height(40.0)
                    .child(
                        super::Opacity::new(row_a).child(
                            Text::new(group_commas(n))
                                .style(
                                    TextStyle::new(30.0)
                                        .monospace()
                                        .letter_spacing(1.0)
                                        .color(alpha(INK, 0.95)),
                                )
                                .align(TextAlign::Left),
                        ),
                    ),
            );
        }
        // The ms row — the timing triple, as one line.
        let ms_a = clamp01((t - 0.75) / 0.15);
        if ms_a > 0.01 {
            let y = Y0 + 5.0 * ROW_H + 8.0;
            stack = stack.push(
                Positioned::new()
                    .left(L_X)
                    .top(y)
                    .width(480.0)
                    .height(24.0)
                    .child(
                        super::Opacity::new(ms_a).child(
                            Text::new("startup · p95 · worst frame")
                                .style(
                                    TextStyle::new(14.0)
                                        .monospace()
                                        .letter_spacing(1.2)
                                        .color(alpha(MUTED, 0.9)),
                                )
                                .align(TextAlign::Left),
                        ),
                    ),
            );
            stack = stack.push(
                Positioned::new()
                    .left(L_X)
                    .top(y + 26.0)
                    .width(480.0)
                    .height(40.0)
                    .child(
                        super::Opacity::new(ms_a).child(
                            Text::new(format!(
                                "{:.1} · {:.1} · {:.1} ms",
                                CERT_STARTUP_MS, CERT_P95_MS, CERT_WORST_MS
                            ))
                            .style(
                                TextStyle::new(28.0)
                                    .monospace()
                                    .letter_spacing(1.0)
                                    .color(alpha(LEDGER, 0.95)),
                            )
                            .align(TextAlign::Left),
                        ),
                    ),
            );
            let _ = CERT_FRAMES_STEADY;
        }
    }

    // ── The right column — the film's own census ──
    let film_rows: [(&str, u64); 5] = [
        ("frames, 60 fps", probe.frames),
        ("shapes drawn", probe.shapes),
        ("glyph runs", probe.glyph_runs),
        ("glyphs placed", probe.glyphs),
        ("layers composited", probe.layers),
    ];
    let film_a = clamp01((t - 0.32) / 0.1);
    if film_a > 0.01 {
        for (i, (label, value)) in film_rows.iter().enumerate() {
            let row_a = clamp01((film_a - i as f32 * 0.10) / 0.6);
            if row_a <= 0.01 {
                continue;
            }
            let n = count_up(*value, row_a);
            let y = Y0 + i as f32 * ROW_H;
            stack = stack.push(
                Positioned::new()
                    .left(R_X)
                    .top(y)
                    .width(420.0)
                    .height(24.0)
                    .child(
                        super::Opacity::new(row_a).child(
                            Text::new(label.to_string())
                                .style(
                                    TextStyle::new(14.0)
                                        .monospace()
                                        .letter_spacing(1.2)
                                        .color(alpha(MUTED, 0.9)),
                                )
                                .align(TextAlign::Left),
                        ),
                    ),
            );
            stack = stack.push(
                Positioned::new()
                    .left(R_X)
                    .top(y + 26.0)
                    .width(420.0)
                    .height(40.0)
                    .child(
                        super::Opacity::new(row_a).child(
                            Text::new(group_commas(n))
                                .style(
                                    TextStyle::new(30.0)
                                        .monospace()
                                        .letter_spacing(1.0)
                                        .color(alpha(INK, 0.95)),
                                )
                                .align(TextAlign::Left),
                        ),
                    ),
            );
        }
        // The alive receipt — the film's own distance measurement.
        if probe.alive_seconds > 0.0 {
            let a = clamp01((t - 0.8) / 0.15);
            let y = Y0 + 5.0 * ROW_H + 8.0;
            stack = stack.push(
                Positioned::new()
                    .left(R_X)
                    .top(y)
                    .width(480.0)
                    .height(24.0)
                    .child(
                        super::Opacity::new(a).child(
                            Text::new("edit → pixels, measured this bench")
                                .style(
                                    TextStyle::new(14.0)
                                        .monospace()
                                        .letter_spacing(1.2)
                                        .color(alpha(MUTED, 0.9)),
                                )
                                .align(TextAlign::Left),
                        ),
                    ),
            );
            stack = stack.push(
                Positioned::new()
                    .left(R_X)
                    .top(y + 26.0)
                    .width(480.0)
                    .height(40.0)
                    .child(
                        super::Opacity::new(a).child(
                            Text::new(format!("{:.1} ms", probe.alive_seconds * 1000.0))
                                .style(
                                    TextStyle::new(28.0)
                                        .monospace()
                                        .letter_spacing(1.0)
                                        .color(alpha(LEDGER, 0.95)),
                                )
                                .align(TextAlign::Left),
                        ),
                    ),
            );
        }
    }

    // The source line — where the left column came from, quoted.
    let src_a = clamp01((t - 0.9) / 0.1);
    if src_a > 0.01 {
        stack = stack.push(
            Positioned::new()
                .left(L_X - 60.0)
                .top(Y0 + 610.0)
                .width(1060.0)
                .height(24.0)
                .child(
                    super::Opacity::new(src_a).child(
                        Text::new(
                            "left column: the 2026-09-15 certification run, quoted from vieww_base/docs · right column: this film's own census",
                        )
                        .style(TextStyle::new(12.5).monospace().letter_spacing(1.0).color(alpha(MUTED, 0.75)))
                        .align(TextAlign::Left),
                    ),
                ),
        );
    }

    // The captions.
    stack = stack.push(super::act_chip("MOVEMENT IV", "THE PROOF", clamp01((t - 0.04) / 0.10)));
    stack = stack.push(caption(
        "every number on screen is a receipt — counted, not claimed",
        1002.0,
        clamp01((t - 0.06) / 0.12),
    ));

    stack = stack.push(distance_chip(ctx.abs, clamp01(t / 0.1)));
    stack = stack.push(progress_rail(ctx.abs));

    let _ = Color::WHITE;
    let _ = Offset::new(0.0, 0.0);
    let _ = W;
    let _ = sec;
    stack.into()
}
