//! exp_blendmatrix — *the blend-mode axis.* The full alchemy tour.
//!
//! Fifteen cinematic blend modes plus a Normal reference, every one drawn
//! in the same frame: a 4×4 grid where each cell composites the same
//! luminous subject over the same patterned ground through **one**
//! [`BlendMode`], and a big stage that tours them one at a time — the
//! active mode's name printed beside it, its grid cell lit.
//!
//! This is the plate the non-separable family has been waiting for: Hue,
//! Saturation, Colour and Luminosity take a different code path than the
//! per-channel modes, and no scene before this had ever pushed all four
//! (or ColourDodge, or Exclusion) through the native renderer in one
//! frame. Sixteen [`blended_layer`] groups per frame — the layer economy
//! measured at breadth, the price U-01's fix paid.
//!
//! The receipt prints the tour position and the group count; the audit
//! reads the grid. If two cells render identically the axis has found a
//! bug, and that receipt goes to todo-upgrades.

use vieww_foundation::{BlendMode, Color, FontFamily, Gradient, Offset, Path, Rect, Size,
    Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, mix, AMBER, BG_DEEP, FAINT, INK, MUTED, VIOLET, VIOLET_SOFT,
    CYAN, CYAN_SOFT, MINT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 10.0;

/// The tour: Normal as reference, then every cinematic mode.
const MODES: [(BlendMode, &str); 16] = [
    (BlendMode::Normal, "Normal (ref)"),
    (BlendMode::Multiply, "Multiply"),
    (BlendMode::Screen, "Screen"),
    (BlendMode::Overlay, "Overlay"),
    (BlendMode::Darken, "Darken"),
    (BlendMode::Lighten, "Lighten"),
    (BlendMode::ColorDodge, "ColorDodge"),
    (BlendMode::ColorBurn, "ColorBurn"),
    (BlendMode::HardLight, "HardLight"),
    (BlendMode::SoftLight, "SoftLight"),
    (BlendMode::Difference, "Difference"),
    (BlendMode::Exclusion, "Exclusion"),
    (BlendMode::Hue, "Hue"),
    (BlendMode::Saturation, "Saturation"),
    (BlendMode::Color, "Color"),
    (BlendMode::Luminosity, "Luminosity"),
];

// ── Geometry ─────────────────────────────────────────────────────────────────

/// The grid: 4×4 cells, top-right.
const GX: f32 = 570.0;
const GY: f32 = 140.0;
const CW: f32 = 166.0;
const CH: f32 = 136.0;
const GAP: f32 = 2.0;

/// The stage: big, left.
const SX: f32 = 40.0;
const SY: f32 = 140.0;
const SW: f32 = 500.0;
const SH: f32 = 550.0;

// ── The scene pieces ────────────────────────────────────────────────────────

/// The ground every mode is tested against — bands, a wedge, a warm corner:
/// brightness variety is what makes the modes disagree.
fn draw_ground(book: &mut Sketchbook, x0: f32, y0: f32, w: f32, h: f32, t: f32) {
    book.rect(
        Rect::new(x0, y0, x0 + w, y0 + h),
        Gradient::vertical().with_dither().with_stops(&[
            (0.0, Color::rgb(34, 22, 54)),
            (0.55, Color::rgb(18, 14, 30)),
            (1.0, Color::rgb(10, 8, 16)),
        ]),
    );
    // Luminance bands — the tones Multiply, Screen, Darken and Lighten
    // argue over.
    let bands = 7;
    let band_h = h / bands as f32;
    for k in 0..bands {
        let lum = 0.5 + 0.5 * ((k as f32 / bands as f32) * std::f32::consts::PI
            + t * 0.8).sin();
        book.rect(
            Rect::new(x0, y0 + k as f32 * band_h, x0 + w, y0 + (k + 1) as f32 * band_h - 1.0),
            alpha(Color::rgb(210, 205, 225), 0.05 + 0.22 * lum),
        );
    }
    // The cyan wedge — a hue the non-separable modes have opinions about.
    let mut wedge = Path::new();
    wedge.move_to(Offset::new(x0 + w * 0.62, y0));
    wedge.line_to(Offset::new(x0 + w, y0));
    wedge.line_to(Offset::new(x0 + w, y0 + h));
    wedge.line_to(Offset::new(x0 + w * 0.42, y0 + h));
    wedge.close();
    book.fill(wedge, alpha(CYAN, 0.16));
    // A warm corner (so Hue/Colour have a second hue to carry).
    let mut corner = Path::new();
    corner.move_to(Offset::new(x0, y0 + h));
    corner.line_to(Offset::new(x0 + w * 0.38, y0 + h));
    corner.line_to(Offset::new(x0, y0 + h * 0.34));
    corner.close();
    book.fill(corner, alpha(AMBER, 0.13));
    // Deterministic bright specks — small lights for the dodges to bite on.
    let mut rng = crate::film_lib::Rng::new(0xA1CE);
    for _ in 0..9 {
        let sx = x0 + rng.f01() * w;
        let sy = y0 + rng.f01() * h;
        let r = 1.2 + rng.f01() * 2.0;
        book.circle(Offset::new(sx, sy), r, alpha(Color::rgb(235, 230, 240), 0.5));
    }
}

/// The subject every mode carries — a small sun: rays, core, ring.
fn draw_subject(book: &mut Sketchbook, cx: f32, cy: f32, r: f32, t: f32) {
    let spin = t * 1.4;
    // The rays.
    for k in 0..12 {
        let a = spin + k as f32 * std::f32::consts::TAU / 12.0;
        let len = r * (1.18 + 0.24 * ((k as f32 * 1.7 + t * 6.0).sin()));
        let mut ray = Path::new();
        ray.move_to(Offset::new(cx + (a - 0.10).cos() * r * 0.72, cy + (a - 0.10).sin() * r * 0.72));
        ray.line_to(Offset::new(cx + a.cos() * len, cy + a.sin() * len));
        ray.line_to(Offset::new(cx + (a + 0.10).cos() * r * 0.72, cy + (a + 0.10).sin() * r * 0.72));
        ray.close();
        book.fill(ray, alpha(AMBER, 0.30));
    }
    // The core.
    book.circle(
        Offset::new(cx, cy),
        r * 0.78,
        Gradient::radial_fill().with_dither().with_stops(&[
            (0.0, Color::rgb(255, 248, 228)),
            (0.3, alpha(AMBER, 0.95)),
            (0.75, alpha(VIOLET, 0.7)),
            (1.0, alpha(Color::rgb(30, 16, 44), 0.0)),
        ]),
    );
    // The ring.
    book.ring(Offset::new(cx, cy), r * 1.02, 1.6, alpha(VIOLET_SOFT, 0.7));
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    // The tour: one mode per 1/16 of the plate.
    let slot = ((t * 16.0).floor() as usize).min(15);
    let (featured, featured_name) = MODES[slot];

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The house ground.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(7, 7, 10)),
                    (0.7, BG_DEEP),
                    (1.0, Color::rgb(4, 4, 6)),
                ]),
            );

            // ── THE STAGE — the featured mode, large ──
            draw_ground(book, SX, SY, SW, SH, t);
            let scx = SX + SW * 0.5;
            let scy = SY + SH * 0.46;
            book.blended_layer(1.0, 0.0, featured, None, |g| {
                draw_subject(g, scx, scy, 128.0, t);
            });
            // Stage frame.
            book.stroke_rrect(
                Rect::new(SX, SY, SX + SW, SY + SH),
                10.0,
                alpha(VIOLET_SOFT, 0.35),
                1.4,
            );

            // ── THE GRID — all sixteen, always ──
            for (i, (mode, _name)) in MODES.iter().enumerate() {
                let col = (i % 4) as f32;
                let row = (i / 4) as f32;
                let x0 = GX + col * (CW + GAP);
                let y0 = GY + row * (CH + GAP);
                draw_ground(book, x0, y0, CW, CH, t);
                let ccx = x0 + CW * 0.5;
                let ccy = y0 + CH * 0.44;
                let clip = {
                    let mut p = Path::new();
                    p.move_to(Offset::new(x0, y0));
                    p.line_to(Offset::new(x0 + CW, y0));
                    p.line_to(Offset::new(x0 + CW, y0 + CH));
                    p.line_to(Offset::new(x0, y0 + CH));
                    p.close();
                    p
                };
                book.blended_layer(1.0, 0.0, *mode, Some(clip), |g| {
                    draw_subject(g, ccx, ccy, 34.0, t);
                });
                // The active cell's frame.
                let active = i == slot;
                book.stroke_rrect(
                    Rect::new(x0, y0, x0 + CW, y0 + CH),
                    3.0,
                    alpha(if active { VIOLET_SOFT } else { FAINT },
                        if active { 0.95 } else { 0.30 }),
                    if active { 1.8 } else { 0.8 },
                );
            }

            // The tour strip under the stage — 16 ticks, the slot lit.
            for k in 0..16 {
                let kx = SX + 12.0 + k as f32 * ((SW - 24.0) / 15.0);
                let lit = k == slot;
                book.rect(
                    Rect::new(kx - 2.5, SY + SH + 16.0, kx + 2.5, SY + SH + 30.0),
                    alpha(if lit { VIOLET_SOFT } else { FAINT }, if lit { 0.8 } else { 0.28 }),
                );
            }
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));

    // The featured mode's name, set large beside the stage.
    stack = stack.push(
        Positioned::new()
            .left(SX + 4.0)
            .top(34.0)
            .width(SW)
            .height(96.0)
            .child(
                Text::new(featured_name.to_string()).style(
                    TextStyle::new(64.0)
                        .family(FontFamily::Named("LXGW WenKai"))
                        .color(alpha(INK, 0.94)),
                ),
            ),
    );
    // The counter under the name.
    stack = stack.push(
        Positioned::new()
            .left(SX + 6.0)
            .top(102.0)
            .width(SW)
            .height(20.0)
            .child(
                Text::new(format!("{}/16 · {} groups/frame", slot + 1, 17)).style(
                    TextStyle::new(13.0)
                        .monospace()
                        .letter_spacing(2.0)
                        .color(alpha(MUTED, 0.9)),
                ),
            ),
    );

    // The grid's cell labels.
    for (i, (_mode, name)) in MODES.iter().enumerate() {
        let col = (i % 4) as f32;
        let row = (i / 4) as f32;
        let x0 = GX + col * (CW + GAP);
        let y0 = GY + row * (CH + GAP);
        let active = i == slot;
        stack = stack.push(
            Positioned::new()
                .left(x0 + 8.0)
                .top(y0 + CH - 17.0)
                .width(CW - 10.0)
                .height(14.0)
                .child(
                    Text::new((*name).to_string()).style(
                        TextStyle::new(10.5)
                            .monospace()
                            .color(alpha(if active { INK } else { MUTED },
                                if active { 0.95 } else { 0.75 })),
                    ),
                ),
        );
    }

    stack.push(receipt_panel(t, slot)).into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(t: f32, slot: usize) -> WidgetNode {
    let lines = [
        "BLENDMATRIX · THE BLEND-MODE AXIS".to_string(),
        format!("tour {}/16 · {} this frame", slot + 1, MODES[slot].1),
        "15 cinematic + Normal · 4 non-separable (H,S,C,L)".to_string(),
        format!("17 blended groups/frame · 1 stage + 16 cells"),
        "subject over shared ground · only the verb changes".to_string(),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 724.0 - 132.0;

    let strip = Painting::sized(
        Size::new(400.0, 40.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            book.rrect(
                Rect::new(0.0, 0.0, 400.0, 40.0),
                8.0,
                alpha(Color::rgb(16, 16, 21), 0.88),
            );
            book.stroke_rrect(
                Rect::new(0.0, 0.0, 400.0, 40.0),
                8.0,
                alpha(Color::WHITE, 0.08),
                1.0,
            );
            // The four non-separable slots, marked.
            for k in [12usize, 13, 14, 15] {
                let x = 12.0 + k as f32 * ((400.0 - 24.0) / 15.0);
                book.circle(Offset::new(x, 20.0), 3.0, alpha(MINT, 0.8));
            }
            let px = 12.0 + slot as f32 * ((400.0 - 24.0) / 15.0);
            book.line(Offset::new(px, 8.0), Offset::new(px, 32.0), alpha(INK, 0.85), 1.2);
        }),
    );

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(420.0)
                .height(15.0)
                .child(
                    Text::new(line.clone()).style(
                        TextStyle::new(if i == 0 { 12.0 } else { 11.0 })
                            .monospace()
                            .letter_spacing(if i == 0 { 1.8 } else { 0.0 })
                            .color(alpha(if i == 0 { MUTED } else { mix(MUTED, INK, 0.4) }, 0.95)),
                    ),
                ),
        );
    }
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 84.0)
            .width(400.0)
            .height(40.0)
            .child(strip),
    );

    stack.into()
}
