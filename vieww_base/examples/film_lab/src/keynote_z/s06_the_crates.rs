//! S06 · THE CRATES — the architecture, seen whole: a living city of
//! thirty-six crates. 0:45–0:55.
//!
//! The landing pad of the descent: the 36-crate workspace as a glowing
//! grid of blocks, grouped and color-coded by layer — foundation,
//! element, render, paint, platform, tools — with light pulses
//! traveling the dependency edges (the build graph, breathing). The
//! count rolls up to 36, derived at runtime from the manifest list
//! this scene draws (never typed): *the engine counts itself.*
//!
//! Each block carries its crate's real name, verbatim from
//! `vieww_base/Cargo.toml`. The pulses are the workload — this is the
//! architecture shot as infrastructure, not diagram: beveled blocks,
//! elevation shadows, connection glow, and a slow orbiting camera
//! drift so the city never sits still.

use vieww_foundation::{Color, Offset, Path, Rect, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{Rng, ease_out_cubic};
use super::{AMBER, CANVAS, CYAN, CYAN_SOFT, Ctx, FAINT, H, INK, MAGENTA, MINT, MUTED, VIOLET, VIOLET_SOFT, W, alpha, aurora, caption, clamp01, count_up, glow, grain, ground, mix, stars_parallax, tint, vignette, xywh};


/// The grid: 9 × 4 = 36 — exactly the crate count, no padding, no gaps.
const COLS: usize = 9;
const ROWS: usize = 4;

/// The layer bands — the grid's rows, color-coded by responsibility.
/// Row order follows the stack: foundation at the bottom, the product
/// facade at the top.
const ROW_COLORS: [Color; 4] = [CYAN, VIOLET, MAGENTA, MINT];
const ROW_LABELS: [&str; 4] = ["foundation · paint · render", "platform · graphics · text", "ui · motion · access", "product · tools · codegen"];

/// Which grid cell each crate occupies (row-major, matching `CRATES`).
fn cell(i: usize) -> (usize, usize) {
    (i % COLS, i / COLS)
}

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let frame_i = (ctx.abs * 60.0) as u64;
    let n = super::CRATES.len() as u64;

    // The arrival — blocks rise from the floor in a diagonal cascade.
    let cascade = |col: usize, row: usize| -> f32 {
        let order = (col + row) as f32 / (COLS + ROWS - 2) as f32;
        clamp01((t - 0.05 - order * 0.45) / 0.30)
    };

    // The count — rolls up with the cascade's leading edge, then rests.
    let count = count_up(n, clamp01((t - 0.10) / 0.55));

    // The camera — a slow drift: the city seen slightly from above and
    // orbiting a degree or two, forever.
    let cam = (t * 0.10).sin() * 30.0;

    // The grid's geometry.
    let gx0 = 260.0;
    let gy0 = 320.0;
    let gw = (W - 520.0);
    let gh = 480.0;
    let cw = gw / COLS as f32;
    let ch = gh / ROWS as f32;

    let mut stack = Stack::new();

    // The sky — the machine's deep register with the aurora, calmer now.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            super::ground(book, w, h);
            super::stars_parallax(book, w, h, 0x61C4, 90, t, 0.10, cam, 0.0);
            super::aurora(book, w, h, t * 0.7, 0x0607, 0.5);
            super::vignette(book, w, h, 0.5);
            super::grain(book, w, h, frame_i, 0.4);
            // The floor glow — the city's light pooling.
            super::glow(book, w * 0.5, h * 0.72, 700.0, VIOLET, 0.10);
        }),
    )));

    // The dependency pulses — light traveling the edges between blocks.
    // Drawn beneath the blocks (the wiring under the floor), a fixed
    // wiring plan with pulses at deterministic phases.
    let wiring = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            let edges: [(usize, usize); 14] = [
                (0, 3), (0, 2), (1, 6), (3, 10), (9, 10), (2, 10),
                (10, 13), (12, 13), (14, 24), (15, 24), (24, 25),
                (25, 22), (25, 27), (13, 27),
            ];
            let center = |i: usize| {
                let (col, row) = cell(i);
                Offset::new(
                    gx0 + (col as f32 + 0.5) * cw,
                    gy0 + (row as f32 + 0.5) * ch,
                )
            };
            for (a, b) in edges {
                let pa = center(a);
                let pb = center(b);
                // The wire — a soft bezier sag beneath the blocks.
                let mid = Offset::new((pa.dx + pb.dx) * 0.5, (pa.dy + pb.dy) * 0.5 + 26.0);
                let mut path = Path::new();
                path.move_to(pa);
                path.cubic_to(
                    Offset::new(pa.dx, mid.dy),
                    Offset::new(pb.dx, mid.dy),
                    pb,
                );
                book.stroke(path, alpha(CYAN, 0.10), 1.0);
                // The pulse — a bright dot riding the wire.
                for k in 0..2 {
                    let phase = ((sec * 0.35) + (a + k) as f32 * 0.37).fract();
                    let px = pa.dx + (pb.dx - pa.dx) * phase;
                    let py = pa.dy + (pb.dy - pa.dy) * phase + 26.0 * (4.0 * phase * (1.0 - phase));
                    book.circle(Offset::new(px, py), 2.2, alpha(tint(CYAN, 0.4), 0.8));
                }
            }
        }),
    );
    stack = stack.push(Positioned::fill().child(wiring));

    // The blocks — beveled, shadowed, labeled; a hover-wave sweeps the
    // grid mid-scene (a build passing over the architecture).
    let wave = (t * 0.8 - 0.15).fract();
    for (i, name) in super::CRATES.iter().enumerate() {
        let (col, row) = cell(i);
        let arrive = cascade(col, row);
        if arrive <= 0.01 {
            continue;
        }
        let color = ROW_COLORS[row];
        let x = gx0 + col as f32 * cw;
        let y = gy0 + row as f32 * ch;
        let rise = (1.0 - ease_out_cubic(arrive)) * 40.0;
        // The hover wave — a moving band of brightness.
        let wave_pos = wave * (COLS + ROWS) as f32;
        let dist = ((col + row) as f32 - wave_pos).abs();
        let hover = (1.0 - (dist / 3.0).min(1.0)).max(0.0);
        let block = Painting::sized(
            Size::new(cw - 10.0, ch - 10.0),
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let bw = cw - 10.0;
                let bh = ch - 10.0;
                // The elevation shadow — the block sits on the floor.
                book.rrect(xywh(3.0, 5.0, bw - 6.0, bh), 8.0, alpha(Color::BLACK, 0.45));
                // The body — dark glass with a colored floor.
                book.rrect(
                    xywh(0.0, 0.0, bw, bh),
                    8.0,
                    vieww_foundation::Gradient::vertical().with_dither().with_stops(&[
                        (0.0, alpha(Color::rgb(16, 16, 21), 0.96)),
                        (0.7, alpha(mix(Color::rgb(16, 16, 21), color, 0.10), 0.96)),
                        (1.0, alpha(mix(Color::rgb(16, 16, 21), color, 0.30 + hover * 0.30), 0.98)),
                    ]),
                );
                // The top bevel — a hairline of light on the leading edge.
                book.line(Offset::new(8.0, 1.5), Offset::new(bw - 8.0, 1.5), alpha(tint(color, 0.3), 0.35 + hover * 0.4), 1.0);
                // The border.
                book.stroke_rrect(xywh(0.0, 0.0, bw, bh), 8.0, alpha(color, 0.22 + hover * 0.45), 1.0);
                // The crate's index tick — top-left, tiny, the census's
                // fingerprint.
                book.rrect(xywh(7.0, 7.0, 3.0, 10.0), 1.5, alpha(color, 0.5));
            }),
        );
        stack = stack.push(
            Positioned::new()
                .left(x + 5.0)
                .top(y + 5.0 + rise)
                .width(cw - 10.0)
                .height(ch - 10.0)
                .child(Opacity::new(arrive).child(block)),
        );
        // The label — the crate's real name, truncating honestly.
        let short: String = if name.len() > 16 {
            let mut s: String = name.chars().take(14).collect();
            s.push('~');
            s
        } else {
            name.to_string()
        };
        stack = stack.push(
            Positioned::new()
                .left(x + 14.0)
                .top(y + 14.0 + rise)
                .width(cw - 20.0)
                .height(ch - 24.0)
                .child(Opacity::new(arrive).child(
                    Text::new(short)
                        .style(
                            TextStyle::new(13.5)
                                .monospace()
                                .letter_spacing(0.4)
                                .color(alpha(tint(INK, 0.0), 0.80 + hover * 0.18)),
                        )
                        .align(TextAlign::Left),
                )),
        );
    }

    // The row labels — the architecture's strata, right margin.
    for (row, label) in ROW_LABELS.iter().enumerate() {
        let la = clamp01((t - 0.45 - row as f32 * 0.06) / 0.16);
        if la <= 0.01 {
            continue;
        }
        let y = gy0 + row as f32 * ch + ch * 0.5 - 10.0;
        stack = stack.push(
            Positioned::new()
                .left(W - 244.0)
                .top(y)
                .width(228.0)
                .height(22.0)
                .child(Opacity::new(la).child(
                    Text::new(*label)
                        .style(TextStyle::new(13.0).monospace().letter_spacing(1.4).color(alpha(ROW_COLORS[row], 0.75)))
                        .align(TextAlign::Right),
                )),
        );
    }

    // The count — top-right, rolling: the engine counting itself.
    let count_a = clamp01((t - 0.10) / 0.10);
    stack = stack.push(
        Positioned::new()
            .left(W - 460.0)
            .top(150.0)
            .width(300.0)
            .height(84.0)
            .child(Opacity::new(count_a).child(Painting::sized(
                Size::new(300.0, 84.0),
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    book.rrect(xywh(0.0, 8.0, 300.0, 60.0), 10.0, alpha(Color::rgb(16, 16, 21), 0.85));
                    book.stroke_rrect(xywh(0.0, 8.0, 300.0, 60.0), 10.0, alpha(VIOLET_SOFT, 0.30), 1.2);
                }),
            ))),
    );
    stack = stack.push(
        Positioned::new()
            .left(W - 444.0)
            .top(18.0 + 150.0)
            .width(290.0)
            .height(50.0)
            .child(Opacity::new(count_a).child(
                Text::new(format!("{} crates", count))
                    .style(TextStyle::new(34.0).monospace().weight(vieww_foundation::FontWeight::Medium).letter_spacing(1.5).color(alpha(INK, 0.97)))
                    .align(TextAlign::Left),
            )),
    );
    stack = stack.push(
        Positioned::new()
            .left(W - 444.0)
            .top(56.0 + 150.0)
            .width(290.0)
            .height(24.0)
            .child(Opacity::new(count_a).child(
                Text::new("counted from the workspace manifest")
                    .style(TextStyle::new(13.0).monospace().letter_spacing(1.0).color(alpha(MUTED, 0.85)))
                    .align(TextAlign::Left),
            )),
    );

    // The title — the architecture's name.
    let title_a = clamp01((t - 0.05) / 0.14);
    stack = stack.push(
        Positioned::new()
            .left(258.0)
            .top(150.0)
            .width(900.0)
            .height(60.0)
            .child(Opacity::new(title_a).child(
                Text::new("the architecture")
                    .style(TextStyle::new(40.0).weight(vieww_foundation::FontWeight::Medium).letter_spacing(2.0).color(alpha(INK, 0.97)))
                    .align(TextAlign::Left),
            )),
    );

    // The captions — the architecture's beats.
    stack = stack.push(caption(
        "thirty-six crates — the workspace manifest, counted live",
        1002.0,
        clamp01((t - 0.20) / 0.12),
    ));
    stack = stack.push(caption(
        "one renderer beneath them all — the pulses are the workload",
        966.0,
        clamp01((t - 0.58) / 0.12),
    ));

    let _ = (H, FAINT, AMBER, Rng::new(0), Rect::new(0.0, 0.0, 1.0, 1.0), MINT);

    stack.into()
}
