//! B03 · THE RASTERIZER — the engine's proof, one glyph at a time.
//! 1:38–1:52.
//!
//! The claim every framework makes and almost none can show: *a
//! renderer that goes all the way down to the pixels.* The camera dives
//! into a single glyph — the "R" of "vieww" — and shows what "down to
//! the pixels" actually means: the outline, the coverage ramp where the
//! curve crosses pixel boundaries, the anti-aliased rim, the 1/255
//! bound the GPU's glyph atlas is verified against. The glyph is drawn
//! by the renderer being filmed — the film's central honesty: the
//! instrument films itself.
//!
//! The distance drops hard through this scene — 8,400 → 2,600 ms.

use vieww_foundation::{Color, Offset, Rect, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;

use super::{
    Ctx, ENGINE, INK, MUTED, W, alpha, caption, clamp01, distance_chip, grain, ground,
    progress_rail, tint, vignette, xywh,
};
use crate::film_lib::{Rng, ease_out_cubic};

/// The glyph study's geometry — center frame, large.
const GX: f32 = 960.0;
const GY: f32 = 440.0;
const GS: f32 = 300.0;

/// The pixel grid's pitch at full zoom.
const PITCH: f32 = 30.0;

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let frame_i = (ctx.abs * 60.0) as u64;

    // The zoom — 0 = the glyph at reading size, 1 = the pixel grid.
    let zoom = ease_out_cubic(clamp01((t - 0.12) / 0.4));
    let pitch = 8.0 + zoom * (PITCH - 8.0);

    let room = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            ground(book, w, h);
            vignette(book, w, h, 0.55);
            grain(book, w, h, frame_i, 0.3);
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(room));

    // The glyph study — one painting, three layers of zoom.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            // ── Layer 1: the pixel grid, arriving with the zoom ──
            let grid_a = clamp01((zoom - 0.25) / 0.6);
            if grid_a > 0.02 {
                let half = GS * 0.9;
                let mut x = GX - half;
                while x < GX + half {
                    book.line(
                        Offset::new(x, GY - half),
                        Offset::new(x, GY + half),
                        alpha(Color::WHITE, 0.07 * grid_a),
                        1.0,
                    );
                    x += pitch;
                }
                let mut y = GY - half;
                while y < GY + half {
                    book.line(
                        Offset::new(GX - half, y),
                        Offset::new(GX + half, y),
                        alpha(Color::WHITE, 0.07 * grid_a),
                        1.0,
                    );
                    y += pitch;
                }
            }

            // ── Layer 2: the glyph's coverage, as filled cells ──
            // A stylised "R" as a coverage bitmap: cells inside the
            // outline fill solid; boundary cells fill by fraction.
            // The deterministic field below is the glyph's shape.
            let glyph_cells = |ci: usize, cj: usize| -> f32 {
                // The "R" in a 9×12 cell grid, as run-length rows.
                const ROWS: [&str; 12] = [
                    "  ####  ",
                    " #    # ",
                    " #    # ",
                    " #    # ",
                    " ###### ",
                    " #  #   ",
                    " #   #  ",
                    " #    # ",
                    " #    # ",
                    " #    # ",
                    "        ",
                    "        ",
                ];
                let row = ROWS.get(cj).copied().unwrap_or("");
                let ch = row.chars().nth(ci).unwrap_or(' ');
                if ch == '#' {
                    1.0
                } else {
                    0.0
                }
            };
            let cells = 8;
            let cell = pitch;
            let ox = GX - cells as f32 * cell * 0.5;
            let oy = GY - 12.0 * cell * 0.5;
            let show_a = ease_out_cubic(clamp01((t - 0.05) / 0.3));
            for cj in 0..12 {
                for ci in 0..cells {
                    // The coverage — a soft boundary: cells adjacent to
                    // filled cells get fractional coverage, the AA rim.
                    let own = glyph_cells(ci, cj);
                    let mut cov = own;
                    if own < 1.0 {
                        // Neighbour fraction — the rim.
                        let neighbours = [
                            glyph_cells(ci.saturating_sub(1), cj),
                            glyph_cells((ci + 1).min(cells - 1), cj),
                            glyph_cells(ci, cj.saturating_sub(1)),
                            glyph_cells(ci, (cj + 1).min(11)),
                        ];
                        cov = neighbours.iter().sum::<f32>() * 0.22;
                    }
                    if cov <= 0.01 {
                        continue;
                    }
                    // Boundary cells that are the AA rim glow slightly
                    // cyan — the ramp made visible.
                    let is_rim = own < 1.0 && cov > 0.0;
                    let c = if is_rim { alpha(ENGINE, 0.75) } else { alpha(tint(INK, 0.0), 0.85) };
                    book.rrect(
                        xywh(ox + ci as f32 * cell + 1.0, oy + cj as f32 * cell + 1.0, cell - 2.0, cell - 2.0),
                        2.0,
                        alpha(c, cov * show_a),
                    );
                }
            }

            // ── Layer 3: the outline, over the cells, dissolving with zoom ──
            let outline_a = show_a * (1.0 - grid_a * 0.85);
            if outline_a > 0.02 {
                book.stroke_rrect(
                    xywh(ox, oy, cells as f32 * cell, 12.0 * cell),
                    6.0,
                    alpha(ENGINE, 0.5 * outline_a),
                    1.4,
                );
            }

            // The rim readout — a magnifier ring calling out one boundary
            // cell with its coverage fraction, the 1/255 bound named.
            let rim_a = clamp01((t - 0.55) / 0.15) * grid_a;
            if rim_a > 0.02 {
                // One rim cell, magnified in a corner callout.
                let rx = GX + 180.0;
                let ry = GY - 120.0;
                book.ring(Offset::new(rx, ry), 46.0, 1.6, alpha(ENGINE, 0.7 * rim_a));
                book.ring(Offset::new(rx, ry), 62.0, 1.0, alpha(ENGINE, 0.3 * rim_a));
                // The fraction — one boundary cell's coverage.
                book.rrect(xywh(rx - 14.0, ry - 14.0, 28.0, 28.0), 4.0, alpha(ENGINE, 0.5 * rim_a));
            }
        }),
    )));

    // The readout lines — the claim, quantified, left of the study.
    let read_a = clamp01((t - 0.3) / 0.2);
    if read_a > 0.01 {
        for (i, line) in [
            "the outline crosses the pixel grid",
            "the coverage ramp is the anti-aliasing",
            "the GPU's atlas is verified to 1/255 of this",
        ]
        .iter()
        .enumerate()
        {
            // Annotation, not world: the dive scales the study, and the
            // film's reading of it has to stay put and stay legible.
            stack = stack.push(super::chrome(
                Positioned::new()
                    .left(280.0)
                    .top(300.0 + i as f32 * 34.0)
                    .width(620.0)
                    .height(28.0)
                    .child(
                        super::Opacity::new(read_a * (1.0 - i as f32 * 0.2)).child(
                            Text::new(line.to_string())
                                .style(
                                    TextStyle::new(16.0)
                                        .monospace()
                                        .letter_spacing(1.2)
                                        .color(alpha(MUTED, 0.9)),
                                )
                                .align(TextAlign::Left),
                        ),
                    )
                    .into(),
            ));
        }
    }

    // The scanner bar — a beam sweeping the study, once, mid-scene: the
    // census's own instrument, visualised.
    let scan_p = clamp01((t - 0.5) / 0.3);
    if (0.0..1.0).contains(&scan_p) && scan_p > 0.0 {
        let sx = GX - 260.0 + scan_p * 520.0;
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                book.layer(1.0, 14.0, None, |g| {
                    g.rect(xywh(sx - 60.0, GY - 250.0, 120.0, 500.0), alpha(ENGINE, 0.06));
                });
                book.rrect(xywh(sx - 1.0, GY - 250.0, 2.0, 500.0), 1.0, alpha(tint(ENGINE, 0.3), 0.8));
            }),
        )));
    }

    // The captions.
    stack = stack.push(super::act_chip("MOVEMENT II", "THE ENGINE", 1.0));
    stack = stack.push(caption(
        "\u{201c}all the way down to the pixels\u{201d} — filmed at the bottom",
        1002.0,
        clamp01((t - 0.05) / 0.12),
    ));
    stack = stack.push(caption(
        "the glyph on screen is drawn by the renderer it is auditing",
        966.0,
        clamp01((t - 0.6) / 0.12),
    ));

    stack = stack.push(distance_chip(ctx.abs, clamp01(t / 0.1)));
    stack = stack.push(progress_rail(ctx.abs));

    let _ = Rng::new(0);
    let _ = Rect::new(0.0, 0.0, 0.0, 0.0);
    let _ = W;
    stack.into()
}
