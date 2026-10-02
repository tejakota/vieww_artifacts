//! S08 · THE RASTERIZER — the engine's own light: a lens over the
//! pixels, and the arithmetic made visible. 1:04–1:15.
//!
//! The machine's core, staged as an instrument: a great lens floats over
//! a letterform (the engine's own "V"); through the lens the edge is
//! *arithmetic* — a sub-pixel coverage grid, every cell a measured alpha.
//! Beside it, the same edge at 1×: aliased, the old world's stair. The
//! **28 blend modes** orbit the lens as a ring of chips (names verbatim
//! from `vieww-paint`'s compositing module), the readout naming each as
//! the ring turns. Beneath, a gradient bar sweeps its 16 stops.
//!
//! The receipts, all quoted from the engine's own docs: 4×
//! supersampling · 28 blend modes · 16-stop gradients · the hairline
//! floor at 0.10 px.

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use super::{
    alpha, caption, clamp01, mix, tint, xywh, Ctx, CYAN, FAINT, INK, MINT, MUTED, VIOLET,
    VIOLET_SOFT, W,
};
use crate::film_lib::ease_out_cubic;

/// The 28 blend modes — verbatim from `vieww-paint/src/native/color.rs`
/// ("Premultiplied compositing and all 28 blend modes"). The ring shows
/// all 28; the readout names them as it turns.
const BLENDS: [&str; 28] = [
    "Normal",
    "Clear",
    "Src",
    "Dst",
    "DstOver",
    "SrcIn",
    "DstIn",
    "SrcOut",
    "DstOut",
    "SrcAtop",
    "DstAtop",
    "Xor",
    "Plus",
    "Multiply",
    "Screen",
    "Overlay",
    "Darken",
    "Lighten",
    "ColorDodge",
    "ColorBurn",
    "HardLight",
    "SoftLight",
    "Difference",
    "Exclusion",
    "Hue",
    "Saturation",
    "Color",
    "Luminosity",
];

/// The lens's center and radius.
const LX: f32 = 700.0;
const LY: f32 = 540.0;
const LR: f32 = 330.0;

/// The readout panel's geometry (right side).
const RX: f32 = 1290.0;
const RY: f32 = 210.0;
const RW: f32 = 420.0;
const RH: f32 = 300.0;

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let frame_i = (ctx.abs * 60.0) as u64;

    // The arrival — the lens scales in, the grid zooms, the ring spins up.
    let lens_in = ease_out_cubic(clamp01((t - 0.04) / 0.20));
    let zoom = 0.35 + 0.65 * ease_out_cubic(clamp01((t - 0.30) / 0.24));
    // The readout index — one blend mode named per 0.38 s, cycling.
    let readout_i = (((sec - 1.0).max(0.0) / 0.38) as usize) % BLENDS.len();
    let ring_rot = sec * 0.10;

    let mut stack = Stack::new();

    // The ground — the engine's register: cyan-cool, deep.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            super::ground(book, w, h);
            super::stars(book, w, h, 0x0808, 60, t, 0.08);
            // A faint blueprint grid behind everything — the engine room.
            let step = 120.0;
            let mut x = (t * 12.0) % step;
            while x < w {
                book.line(
                    Offset::new(x, 0.0),
                    Offset::new(x, h),
                    alpha(CYAN, 0.035),
                    1.0,
                );
                x += step;
            }
            let mut y = (t * 8.0) % step;
            while y < h {
                book.line(
                    Offset::new(0.0, y),
                    Offset::new(w, y),
                    alpha(CYAN, 0.035),
                    1.0,
                );
                y += step;
            }
            super::vignette(book, w, h, 0.5);
            super::grain(book, w, h, frame_i, 0.35);
        }),
    )));

    // THE LETTERFORM — the engine's own "V", huge, behind the lens. It
    // breathes: the glyph's weight modulates with the frame's cadence.
    let glyph_a = lens_in * 0.16;
    stack = stack.push(
        Positioned::new()
            .left(LX - 300.0)
            .top(300.0)
            .width(600.0)
            .height(500.0)
            .child(
                Opacity::new(1.0).child(
                    Text::new("V")
                        .style(
                            TextStyle::new(430.0)
                                .monospace()
                                .weight(vieww_foundation::FontWeight::Medium)
                                .color(alpha(INK, glyph_a.max(0.03))),
                        )
                        .align(TextAlign::Center),
                ),
            ),
    );

    // THE LENS — a great circle over the glyph's edge. Inside it, the
    // sub-pixel truth: a coverage grid where each cell's alpha is the
    // measured coverage of the glyph's edge crossing it. Outside the
    // lens, the edge is a smooth vector; inside, it is arithmetic.
    let lens = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            // The lens glow and rim.
            super::glow(book, LX, LY, LR * 1.15, CYAN, 0.10 * lens_in);
            book.stroke(circle_lens(), alpha(tint(CYAN, 0.25), 0.6 * lens_in), 2.4);
            book.stroke(circle_lens_inner(), alpha(CYAN, 0.14 * lens_in), 1.0);
            // The lens's glass — a faint radial tint.
            book.circle(
                Offset::new(LX, LY),
                LR,
                Gradient::radial(Offset::new(0.5, 0.5), 1.0)
                    .with_dither()
                    .with_stops(&[
                        (0.0, alpha(CYAN, 0.05 * lens_in)),
                        (0.8, alpha(CYAN, 0.0)),
                        (1.0, alpha(CYAN, 0.10 * lens_in)),
                    ]),
            );
            // The magnifier handle ticks — the inspector's brackets on
            // the lens rim.
            for k in 0..4 {
                let a = std::f32::consts::FRAC_PI_2 * k as f32 + ring_rot * 0.4;
                let p0 = Offset::new(LX + a.cos() * (LR + 10.0), LY + a.sin() * (LR + 10.0));
                let p1 = Offset::new(LX + a.cos() * (LR + 26.0), LY + a.sin() * (LR + 26.0));
                book.line(p0, p1, alpha(CYAN, 0.7 * lens_in), 2.6);
            }
            // THE COVERAGE GRID — inside the lens: the glyph's edge as
            // cells. A diagonal edge crossing the lens; each cell's alpha
            // is its coverage of the lower-left half-plane. The grid
            // resolution grows with the zoom: the closer you look, the
            // more the edge becomes numbers.
            let res = (6.0 + 10.0 * zoom) as usize;
            let cell = (LR * 1.5) / res as f32;
            let gx0 = LX - cell * res as f32 * 0.5;
            let gy0 = LY - cell * res as f32 * 0.5;
            // The edge — a line through the lens, slowly rotating.
            let edge_a = -0.5 + (sec * 0.05).sin() * 0.2;
            let nx = edge_a.cos();
            let ny = edge_a.sin();
            let c = 0.0; // edge passes through the lens center
            for iy in 0..res {
                for ix in 0..res {
                    let cx = gx0 + (ix as f32 + 0.5) * cell;
                    let cy = gy0 + (iy as f32 + 0.5) * cell;
                    // Signed distance of the cell center from the edge.
                    let sd = (cx - LX) * nx + (cy - LY) * ny - c;
                    // Coverage — the fraction of the cell inside (a
                    // clamped linear ramp, the honest 1-D approximation).
                    let cov = (0.5 - sd / cell).clamp(0.0, 1.0);
                    if cov > 0.02 && cov < 0.98 {
                        // Only the boundary cells carry the arithmetic —
                        // the AA band, the whole point of the scene.
                        book.rrect(
                            xywh(
                                cx - cell * 0.5 + 0.6,
                                cy - cell * 0.5 + 0.6,
                                cell - 1.2,
                                cell - 1.2,
                            ),
                            2.0,
                            alpha(tint(VIOLET_SOFT, 0.2), cov * 0.85),
                        );
                    } else if cov >= 0.98 {
                        // Inside — the glyph's body, dim.
                        book.rrect(
                            xywh(
                                cx - cell * 0.5 + 0.6,
                                cy - cell * 0.5 + 0.6,
                                cell - 1.2,
                                cell - 1.2,
                            ),
                            2.0,
                            alpha(INK, 0.10),
                        );
                    }
                }
            }
            // The grid's hairlines.
            for ix in 0..=res {
                let x = gx0 + ix as f32 * cell;
                book.line(
                    Offset::new(x, gy0),
                    Offset::new(x, gy0 + cell * res as f32),
                    alpha(Color::WHITE, 0.05),
                    0.8,
                );
            }
            for iy in 0..=res {
                let y = gy0 + iy as f32 * cell;
                book.line(
                    Offset::new(gx0, y),
                    Offset::new(gx0 + cell * res as f32, y),
                    alpha(Color::WHITE, 0.05),
                    0.8,
                );
            }
        }),
    );
    stack = stack.push(Positioned::fill().child(lens));

    // THE RING — 28 blend modes orbiting the lens, one chip each, the
    // active one bright with its name in the readout.
    let ring = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            let rr = LR + 90.0;
            for (i, _name) in BLENDS.iter().enumerate() {
                let a = ring_rot + i as f32 / BLENDS.len() as f32 * std::f32::consts::TAU;
                let px = LX + a.cos() * rr;
                let py = LY + a.sin() * rr * 0.86; // a slight ellipse — perspective
                let active = i == readout_i;
                let size = if active { 7.0 } else { 4.0 };
                book.circle(
                    Offset::new(px, py),
                    size,
                    alpha(
                        if active { tint(CYAN, 0.5) } else { CYAN },
                        if active { 1.0 } else { 0.55 },
                    ),
                );
                if active {
                    super::glow(book, px, py, 70.0, CYAN, 0.35);
                }
            }
        }),
    );
    stack = stack.push(Positioned::fill().child(ring));

    // THE READOUT — the comparison panel, right side: 4× supersampled
    // vs 1× aliased, side by side, with the blend mode's name beneath.
    let panel_a = ease_out_cubic(clamp01((t - 0.22) / 0.16));
    if panel_a > 0.01 {
        // The panel body.
        stack = stack.push(
            Positioned::new()
                .left(RX)
                .top(RY)
                .width(RW)
                .height(RH)
                .child(
                    Opacity::new(panel_a).child(
                        Container::new()
                            .color(alpha(Color::rgb(16, 16, 21), 0.92))
                            .radius(14.0)
                            .border(vieww_foundation::Border::new(alpha(CYAN, 0.28), 1.2)),
                    ),
                ),
        );
        // The two edges — drawn in a painting: left, the 4× coverage
        // ramp; right, the 1× hard stair. Same edge, two worlds.
        let edges = Painting::sized(
            Size::new(RW - 40.0, 180.0),
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let _w = RW - 40.0;
                let h = 180.0;
                let edge = clamp01((t - 0.35) / 0.20);
                // LEFT — the 4× supersampled edge: a column of cells
                // whose alpha is the measured coverage ramp.
                let cw = 22.0;
                let ch = 22.0;
                for iy in 0..(h as usize / ch as usize) {
                    for ix in 0..2usize {
                        let cx = 20.0 + ix as f32 * cw;
                        let cy = 14.0 + iy as f32 * ch;
                        // The edge crosses at x = 20 + cw (vertical-ish,
                        // slanted by one cell over the height).
                        let edge_x = 30.0 + (cy / h) * 18.0;
                        let cov = ((edge_x - (cx + cw * 0.5)) / cw + 0.5).clamp(0.0, 1.0) * edge;
                        if cov > 0.02 {
                            book.rrect(
                                xywh(cx, cy, cw - 2.0, ch - 2.0),
                                2.0,
                                alpha(VIOLET_SOFT, cov * 0.9),
                            );
                        }
                        book.stroke_rrect(
                            xywh(cx, cy, cw - 2.0, ch - 2.0),
                            2.0,
                            alpha(Color::WHITE, 0.05),
                            0.6,
                        );
                    }
                }
                // RIGHT — the 1× aliased edge: the same slant, hard
                // pixels. Binary coverage: the stair the ramp replaces.
                let base = 250.0;
                for iy in 0..(h as usize / ch as usize) {
                    let cy = 14.0 + iy as f32 * ch;
                    let edge_x_row = base + 10.0 + (cy / h) * 18.0;
                    for ix in 0..2usize {
                        let cx = base + ix as f32 * cw;
                        let filled = (cx + cw * 0.5) < edge_x_row;
                        if filled {
                            book.rrect(
                                xywh(cx, cy, cw - 2.0, ch - 2.0),
                                2.0,
                                alpha(MUTED, 0.55 * edge),
                            );
                        }
                        book.stroke_rrect(
                            xywh(cx, cy, cw - 2.0, ch - 2.0),
                            2.0,
                            alpha(Color::WHITE, 0.05),
                            0.6,
                        );
                    }
                }
                // The labels — beneath each column, in the panel body.
            }),
        );
        stack = stack.push(
            Positioned::new()
                .left(RX + 20.0)
                .top(RY + 60.0)
                .width(RW - 40.0)
                .height(180.0)
                .child(Opacity::new(panel_a).child(edges)),
        );
        // The panel's labels.
        for (label, x, color) in [
            ("4× supersampled", RX + 20.0, VIOLET_SOFT),
            ("1× aliased", RX + 240.0, MUTED),
        ] {
            stack = stack.push(
                Positioned::new()
                    .left(x)
                    .top(RY + 244.0)
                    .width(190.0)
                    .height(22.0)
                    .child(
                        Opacity::new(panel_a).child(
                            Text::new(label)
                                .style(
                                    TextStyle::new(14.0)
                                        .monospace()
                                        .letter_spacing(1.2)
                                        .color(alpha(color, 0.9)),
                                )
                                .align(TextAlign::Left),
                        ),
                    ),
            );
        }
        // The blend readout — the active mode's name, cycling.
        stack = stack.push(
            Positioned::new()
                .left(RX + 20.0)
                .top(RY + 16.0)
                .width(RW - 40.0)
                .height(28.0)
                .child(
                    Opacity::new(panel_a).child(
                        Text::new(format!(
                            "blend · {} · {}/28",
                            BLENDS[readout_i],
                            readout_i + 1
                        ))
                        .style(
                            TextStyle::new(17.0)
                                .monospace()
                                .letter_spacing(1.6)
                                .color(alpha(tint(CYAN, 0.3), 1.0)),
                        )
                        .align(TextAlign::Left),
                    ),
                ),
        );
    }

    // THE GRADIENT BAR — 16 stops, sweeping, beneath the lens. Each stop
    // is a real gradient stop; the bar animates its ramps.
    let bar_a = ease_out_cubic(clamp01((t - 0.44) / 0.16));
    if bar_a > 0.01 {
        let bar = Painting::sized(
            Size::new(620.0, 40.0),
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                // The 16-stop ramp — a hue walk from violet through cyan
                // to mint, the stops breathing in position.
                let n = 16;
                let mut stops: Vec<(f32, Color)> = Vec::with_capacity(n);
                for i in 0..n {
                    let u = i as f32 / (n - 1) as f32;
                    let wobble = (u * 8.0 + sec * 0.7).sin() * 0.012;
                    let color = mix(mix(VIOLET, CYAN, u * 1.4), MINT, (u - 0.6).max(0.0) * 1.8);
                    stops.push(((u + wobble).clamp(0.0, 1.0), color));
                }
                book.rrect(
                    xywh(0.0, 0.0, 620.0, 16.0),
                    8.0,
                    Gradient::horizontal().with_dither().with_stops(&stops),
                );
                // The stop ticks — the 16 pins, riding the ramp.
                for (u, _c) in stops.iter() {
                    book.line(
                        Offset::new(u * 620.0, -5.0),
                        Offset::new(u * 620.0, 22.0),
                        alpha(Color::WHITE, 0.35),
                        1.0,
                    );
                    book.circle(Offset::new(u * 620.0, -7.0), 2.2, alpha(Color::WHITE, 0.6));
                }
            }),
        );
        stack = stack.push(
            Positioned::new()
                .left(400.0)
                .top(860.0)
                .width(620.0)
                .height(40.0)
                .child(Opacity::new(bar_a).child(bar)),
        );
        stack = stack.push(
            Positioned::new()
                .left(400.0)
                .top(906.0)
                .width(620.0)
                .height(22.0)
                .child(
                    Opacity::new(bar_a).child(
                        Text::new("16 gradient stops · animated ramps · dithered")
                            .style(
                                TextStyle::new(14.0)
                                    .monospace()
                                    .letter_spacing(1.4)
                                    .color(alpha(MUTED, 0.9)),
                            )
                            .align(TextAlign::Left),
                    ),
                ),
        );
    }

    // The captions — the engine's beats.
    stack = stack.push(caption(
        "the engine draws its own light — 4× supersampled",
        1002.0,
        clamp01((t - 0.18) / 0.12),
    ));
    stack = stack.push(caption(
        "28 blend modes · 16-stop gradients · hairlines to 0.10 px",
        966.0,
        clamp01((t - 0.52) / 0.12),
    ));

    let _ = (
        FAINT,
        Rect::new(0.0, 0.0, 1.0, 1.0),
        W,
        super::H,
        INK,
        lens_in,
    );

    stack.into()
}

/// The lens's outer rim, as a path.
fn circle_lens() -> vieww_foundation::Path {
    super::circle_path(LX, LY, LR, 96)
}

/// The lens's inner rim (the glass's edge), as a path.
fn circle_lens_inner() -> vieww_foundation::Path {
    super::circle_path(LX, LY, LR - 9.0, 96)
}
