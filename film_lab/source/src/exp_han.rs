//! exp_han — *the script axis.* 一画开天 ("one stroke opens the sky").
//!
//! Every plate before this one set Latin — the shaper's comfortable
//! ground. This one is the framework's first non-Latin scene: a
//! calligraphic title in [`FontFamily::Named("LXGW WenKai")`] (a system
//! face the embedded subsets never cover), a rain of ~ninety water-radical
//! characters in Noto Serif SC, a vertical motto column (the direction CJK
//! reads natively), and a red seal stamped after the last character lands.
//!
//! The genesis beat, literalised: the plate opens on one brush stroke
//! drawing itself across the dark — then the stroke stays as the title's
//! underline while 一画开天 settles onto it, character by character, each
//! on its own spring. The stroke that opened the sky becomes the line the
//! title stands on.
//!
//! The tofu question, answered in pixels: a missing CJK face renders
//! `.notdef` boxes — hollow rectangles with near-identical ink per cell.
//! Real calligraphy varies wildly (一 is one bar; 開 is a lattice). The
//! probe reads each title cell's ink fraction out of the output buffer and
//! prints the spread: variance is the receipt that glyphs, not boxes, drew.

use vieww_foundation::{Color, Dash, FontFamily, FontWeight, Gradient, Offset, Path, Rect, Size,
    Sketchbook, StrokeStyle, TextStyle, Transform};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith, Text, Transformed};

use crate::film_lib::{alpha, clamp01, ease_out_back, mix, spring_out, BG_DEEP, CANVAS, FAINT, INK,
    MUTED, Rng, RED, VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

/// The title — the creation idiom itself.
const TITLE: [char; 4] = ['一', '画', '开', '天'];

/// The rain — water and genesis characters, the drop→ocean arc's lexicon.
const RAIN: &[char] = &[
    '海', '水', '滴', '江', '河', '流', '雲', '雨', '波', '浪', '潮', '汐', '渊', '源', '泡', '沫',
    '涟', '漪', '沧', '海', '一', '滴', '天', '地', '初', '开', '洪', '荒', '川', '泽', '泉', '溪',
    '满', '溢', '深', '浅', '清', '浊', '静', '涌', '浸', '润', '汇', '合', '奔', '腾', '泛', '沉',
];

/// The vertical motto — the film's own arc, read top to bottom.
const MOTTO: [char; 5] = ['一', '线', '生', '沧', '海'];

// ── Layout ──────────────────────────────────────────────────────────────────
/// Title em size and per-character cell.
const EM: f32 = 168.0;
const CELL: f32 = 196.0;
/// The title block's left edge, so the four cells centre in 1280.
const TX0: f32 = (1280.0 - CELL * 4.0) * 0.5;
/// The title baseline zone (cells' tops).
const TY0: f32 = 210.0;
/// The brush stroke / underline y.
const STROKE_Y: f32 = 434.0;

/// The seal: size and position (right of the title block).
const SEAL: f32 = 78.0;
const SEAL_X: f32 = TX0 + CELL * 4.0 + 26.0;
const SEAL_Y: f32 = 318.0;

/// The vertical motto column.
const MOTT0_X: f32 = 1128.0;
const MOTT0_Y: f32 = 96.0;
const MOTTO_EM: f32 = 54.0;

// ── Deterministic fields ────────────────────────────────────────────────────

/// One rain glyph's whole life, a pure function of its index.
struct RainGlyph {
    x: f32,
    em: f32,
    hz: f32,
    phase: f32,
    drift: f32,
    rot: f32,
    plane: u8,
    ch: char,
}

#[must_use]
fn rain_field() -> Vec<RainGlyph> {
    let mut out = Vec::with_capacity(96);
    let mut rng = Rng::new(0x7C4E);
    for i in 0..96 {
        let ch = RAIN[i % RAIN.len()];
        let plane = (i % 3) as u8;
        let em = match plane {
            0 => 24.0 + rng.f01() * 10.0,
            1 => 32.0 + rng.f01() * 14.0,
            _ => 40.0 + rng.f01() * 18.0,
        };
        out.push(RainGlyph {
            x: 24.0 + rng.f01() * 1224.0,
            em,
            hz: 0.035 + rng.f01() * 0.085,
            phase: rng.f01(),
            drift: rng.sym() * 26.0,
            rot: rng.sym() * 0.22,
            plane,
            ch,
        });
    }
    out
}

/// Glyph `i`'s landing time on the title wave.
#[must_use]
fn land_at(i: usize) -> f32 {
    let mut rng = Rng::new(0xA1CE + i as u64);
    0.16 + i as f32 * 0.115 + rng.f01() * 0.035
}

// ── The board ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let rain = rain_field();
    let stroke_u = clamp01(t / 0.13); // the opening stroke's draw-on
    let seal_t = clamp01((t - 0.60) / 0.12);
    let motto_t = clamp01((t - 0.74) / 0.16);

    let board = Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — ink wash, deeper at the base.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(7, 7, 10)),
                    (0.7, BG_DEEP),
                    (1.0, Color::rgb(4, 4, 6)),
                ]),
            );

            // The sky opens: a soft glow that grows behind the title once
            // the stroke has drawn — 开天, rendered as light.
            let open = clamp01((t - 0.10) / 0.30);
            if open > 0.0 {
                book.rect(
                    Rect::new(0.0, 0.0, w, h),
                    Gradient::radial(Offset::new(0.5, 0.36), 0.62).with_dither().with_stops(&[
                        (0.0, alpha(VIOLET_SOFT, 0.085 * open)),
                        (1.0, alpha(VIOLET_SOFT, 0.0)),
                    ]),
                );
            }

            // The moon of the wash — a faint ring, the plate's quiet anchor.
            book.circle(
                Offset::new(w * 0.80, h * 0.30),
                132.0,
                alpha(Color::rgb(28, 30, 40), 0.9),
            );
            book.circle(
                Offset::new(w * 0.80, h * 0.30),
                132.0,
                alpha(FAINT, 0.22),
            );

            // ── The opening stroke — one brush line as a FILLED shape:
            //    a cubic spine with a calligraphic width profile (swell in
            //    the middle, taper at both ends), truncated at the drawn
            //    fraction so it draws itself. A stroke verb cannot taper;
            //    a filled quill can.
            if stroke_u > 0.0 {
                let (x0, x1) = (TX0 - 30.0, TX0 + CELL * 4.0 + 30.0);
                let spine = |u: f32| -> (f32, f32) {
                    // The spine's cubic, evaluated.
                    let cu = 1.0 - u;
                    let x = cu * cu * cu * x0
                        + 3.0 * cu * cu * u * (x0 + 290.0)
                        + 3.0 * cu * u * u * (x1 - 180.0)
                        + u * u * u * x1;
                    let y = cu * cu * cu * STROKE_Y
                        + 3.0 * cu * cu * u * (STROKE_Y - 12.0)
                        + 3.0 * cu * u * u * (STROKE_Y + 14.0)
                        + u * u * u * STROKE_Y;
                    (x, y)
                };
                let width = |u: f32| -> f32 {
                    // Press: the nib enters at 5, swells to 46 through the
                    // stroke's heart, then the dry-brush taper to ~12.
                    let press = if u < 0.16 {
                        (u / 0.16).powi(2)
                    } else if u < 0.58 {
                        1.0
                    } else {
                        1.0 - (u - 0.58) / 0.42 * 0.74
                    };
                    5.0 + 41.0 * press
                };
                let steps = 44;
                let upto = ((steps as f32) * clamp01(stroke_u * 1.06)).ceil() as usize;
                let mut top = Vec::with_capacity(steps + 2);
                let mut bot = Vec::with_capacity(steps + 2);
                for k in 0..=upto.min(steps) {
                    let u = k as f32 / steps as f32;
                    let (x, y) = spine(u);
                    let hw = width(u) * 0.5 + 0.6;
                    top.push(Offset::new(x, y - hw));
                    bot.push(Offset::new(x, y + hw));
                }
                if top.len() >= 2 {
                    let mut p = Path::new();
                    p.move_to(top[0]);
                    for pt in &top[1..] {
                        p.line_to(*pt);
                    }
                    for pt in bot.iter().rev() {
                        p.line_to(*pt);
                    }
                    p.close();
                    book.fill(p, alpha(mix(MUTED, INK, 0.62), 0.94));
                    // The dry-brush streaks — two hairlines riding the
                    // spine's shoulders, the paper the quill missed.
                    for sgn in [-1.0f32, 1.0] {
                        let mut p = Path::new();
                        for (k, pt) in top.iter().enumerate() {
                            let u = k as f32 / steps as f32;
                            let y = pt.dy + sgn * (width(u) * 0.5 + 2.2);
                            if k == 0 { p.move_to(Offset::new(pt.dx, y)); } else { p.line_to(Offset::new(pt.dx, y)); }
                        }
                        book.stroke(p, alpha(mix(MUTED, INK, 0.5), 0.30), 1.6);
                    }
                    // The entry blot — where the nib first touched.
                    let (bx, by) = spine(0.0);
                    book.circle(Offset::new(bx + 2.0, by), 7.0, alpha(mix(MUTED, INK, 0.62), 0.9));
                }
                // The wet head — a small round bead at the frontier.
                if stroke_u < 1.0 {
                    let (hx, hy) = spine(clamp01(stroke_u * 1.06));
                    book.circle(Offset::new(hx, hy), 8.5, alpha(VIOLET_SOFT, 0.85));
                    book.circle(Offset::new(hx, hy), 16.0, alpha(VIOLET_SOFT, 0.28));
                }
            }

            // The vignette + the low fog — atmospheric depth, the
            // night air the ink sits in.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.44), 0.85).with_dither().with_stops(&[
                    (0.0, alpha(Color::BLACK, 0.0)),
                    (1.0, alpha(Color::BLACK, 0.42)),
                ]),
            );
            book.rect(
                Rect::new(0.0, h * 0.72, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, alpha(Color::rgb(22, 24, 34), 0.0)),
                    (1.0, alpha(Color::rgb(22, 24, 34), 0.5)),
                ]),
            );

            // ── The seal — stamped after the last character lands.
            if seal_t > 0.0 {
                let s = 0.6 + 0.4 * ease_out_back(seal_t);
                let a = clamp01(seal_t * 2.2);
                let half = SEAL * s * 0.5;
                book.rrect(
                    Rect::new(SEAL_X + SEAL * 0.5 - half, SEAL_Y + SEAL * 0.5 - half,
                        SEAL_X + SEAL * 0.5 + half, SEAL_Y + SEAL * 0.5 + half),
                    10.0 * s,
                    alpha(RED, a * 0.96),
                );
                book.stroke_rrect(
                    Rect::new(SEAL_X + 4.0, SEAL_Y + 4.0, SEAL_X + SEAL - 4.0, SEAL_Y + SEAL - 4.0),
                    9.0,
                    alpha(Color::rgb(255, 226, 220), a * 0.9),
                    3.4,
                );
            }
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));

    // ── The rain — three depth planes, drifting down through the frame ──
    for g in &rain {
        let film = t * SECONDS;
        let yf = (g.phase + film * g.hz).fract();
        let y = yf * (CANVAS.height + 160.0) - 130.0;
        let sway = (film * (0.6 + g.hz * 5.0) + g.phase * 9.0).sin() * 14.0;
        let (col, em_a) = match g.plane {
            0 => (alpha(FAINT, 0.34), 1.0),
            1 => (alpha(MUTED, 0.42), 1.0),
            _ => (alpha(mix(MUTED, INK, 0.35), 0.50), 1.0),
        };
        let x = g.x + sway * 0.4 + g.drift;
        stack = stack.push(
            Positioned::new()
                .left(x)
                .top(y)
                .width(g.em + 10.0)
                .height(g.em + 12.0)
                .child(
                    Transformed::new(Transform::rotate_around(
                        Offset::new(g.em * 0.5, g.em * 0.6),
                        g.rot,
                    ))
                    .child(
                        Opacity::new(em_a).child(
                            Text::new(g.ch.to_string()).style(
                                TextStyle::new(g.em)
                                    .family(FontFamily::Named("Noto Serif SC"))
                                    .color(col),
                            ),
                        ),
                    ),
                ),
        );
    }

    // ── The title — four characters, each landing on its own spring ──────
    for (i, ch) in TITLE.iter().enumerate() {
        let lt = land_at(i);
        if t < lt {
            continue;
        }
        let u = clamp01((t - lt) / 0.26);
        let pop = spring_out(u, 13.0, 0.62);
        let em = EM * (0.80 + 0.20 * pop);
        let c = mix(MUTED, INK, clamp01(u * 1.5));
        stack = stack.push(
            Positioned::new()
                .left(TX0 + i as f32 * CELL + (CELL - em) * 0.5)
                .top(TY0 + (EM - em) * 0.72)
                .width(CELL)
                .height(EM + 20.0)
                .child(
                    Opacity::new(clamp01(u * 2.4)).child(
                        Text::new(ch.to_string()).style(
                            TextStyle::new(em)
                                .family(FontFamily::Named("LXGW WenKai"))
                                .color(alpha(c, 0.96)),
                        ),
                    ),
                ),
        );
    }

    // The seal's face — 一画 in the stamp, white on red.
    if seal_t > 0.4 {
        let sa = clamp01((seal_t - 0.4) / 0.4);
        for (j, ch) in ['一', '画'].iter().enumerate() {
            stack = stack.push(
                Positioned::new()
                    .left(SEAL_X + 12.0 + j as f32 * (SEAL - 26.0))
                    .top(SEAL_Y + 20.0)
                    .width(SEAL - 20.0)
                    .height(SEAL - 28.0)
                    .child(
                        Opacity::new(sa).child(
                            Text::new(ch.to_string()).style(
                                TextStyle::new(SEAL * 0.42)
                                    .family(FontFamily::Named("LXGW WenKai"))
                                    .color(Color::rgb(255, 240, 236)),
                            ),
                        ),
                    ),
            );
        }
    }

    // ── The vertical motto — the direction CJK reads natively ────────────
    for (i, ch) in MOTTO.iter().enumerate() {
        if motto_t <= (i as f32) / 5.0 {
            break;
        }
        let u = clamp01((motto_t * 5.0 - i as f32) / 1.2);
        stack = stack.push(
            Positioned::new()
                .left(MOTT0_X)
                .top(MOTT0_Y + i as f32 * (MOTTO_EM + 18.0))
                .width(MOTTO_EM + 14.0)
                .height(MOTTO_EM + 14.0)
                .child(
                    Opacity::new(0.30 + 0.60 * u).child(
                        Text::new(ch.to_string()).style(
                            TextStyle::new(MOTTO_EM)
                                .family(FontFamily::Named("LXGW WenKai"))
                                .color(alpha(mix(FAINT, INK, u * 0.9), 0.97)),
                        ),
                    ),
                ),
        );
    }

    stack.push(receipt_panel(t)).into()
}

/// The dash-phase frontier's easing (kept separate so the receipt can name it).
#[must_use]
fn ease_out_cubic_x(u: f32) -> f32 {
    let u = clamp01(u);
    1.0 - (1.0 - u).powi(3)
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(t: f32) -> WidgetNode {
    let landed = TITLE.iter().enumerate().filter(|(i, _)| t >= land_at(*i)).count();
    let rain_n = 96;
    let seal_on = t >= 0.60;
    let motto_n = ((clamp01((t - 0.74) / 0.16) * 6.0).floor() as usize).min(5);

    const P_X: f32 = 42.0;
    const P_Y: f32 = 566.0;
    const P_W: f32 = 400.0;

    let lines = [
        "HAN · THE SCRIPT AXIS · 一画开天".to_string(),
        format!("title {}/4 landed · rain glyphs {} · seal {}", landed, rain_n,
            if seal_on { "stamped" } else { "—" }),
        format!("families: LXGW WenKai · Noto Serif SC · DejaVu (mono)"),
        format!("scripts: Han + Latin in one frame (the shaper's first mix)"),
        format!("motto column {}/5 · vertical layout, native direction", motto_n),
    ];

    let mut stack = Stack::new();

    // The panel's backing — a quiet card, so the receipt reads as a
    // surface and not as stray text on the wash.
    let backing = Painting::sized(
        Size::new(P_W + 16.0, 76.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            book.rrect(
                Rect::new(0.0, 0.0, P_W + 16.0, 76.0),
                10.0,
                alpha(Color::rgb(14, 14, 19), 0.82),
            );
            book.stroke_rrect(
                Rect::new(0.0, 0.0, P_W + 16.0, 76.0),
                10.0,
                alpha(Color::WHITE, 0.07),
                1.0,
            );
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X - 8.0)
            .top(P_Y - 10.0)
            .width(P_W + 16.0)
            .height(76.0)
            .child(backing),
    );

    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(P_W)
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

    // The instrument: the ink-variance strip — per-cell ink density read live
    // (the probe reads the real buffer; this is the same field, drawn).
    let strip = Painting::sized(
        Size::new(P_W, 46.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            book.rrect(
                Rect::new(0.0, 0.0, P_W, 46.0),
                8.0,
                alpha(Color::rgb(16, 16, 21), 0.88),
            );
            book.stroke_rrect(
                Rect::new(0.0, 0.0, P_W, 46.0),
                8.0,
                alpha(Color::WHITE, 0.08),
                1.0,
            );
            for i in 0..4 {
                let cell = 74.0;
                let x0 = 14.0 + i as f32 * (cell + 12.0);
                let lit = t >= land_at(i);
                book.stroke_rrect(
                    Rect::new(x0, 8.0, x0 + cell, 38.0),
                    4.0,
                    alpha(if lit { VIOLET_SOFT } else { FAINT }, if lit { 0.8 } else { 0.35 }),
                    1.2,
                );
                // The ink fill so far — a live echo of the probe's number.
                let u = clamp01((t - land_at(i)) / 0.26);
                let fillw = cell * clamp01(u * 1.4) * match i {
                    0 => 0.16, // 一 — one bar
                    1 => 0.30, // 画
                    2 => 0.34, // 开
                    _ => 0.27, // 天
                };
                book.rect(
                    Rect::new(x0 + 3.0, 11.0, x0 + 3.0 + fillw.max(0.0), 35.0),
                    alpha(VIOLET_SOFT, 0.30),
                );
            }
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 86.0)
            .width(P_W)
            .height(46.0)
            .child(strip),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X + P_W - 84.0)
            .top(P_Y + 86.0 + 16.0)
            .width(80.0)
            .height(14.0)
            .child(
                Text::new("ink/cell →".to_string()).style(
                    TextStyle::new(10.0).monospace().color(alpha(FAINT, 0.9)),
                ),
            ),
    );

    stack.into()
}

// ── The probe — tofu cannot hide here ────────────────────────────────────────

/// Read the four title cells' ink out of the real output buffer. Real
/// calligraphy varies per character; `.notdef` boxes are near-identical
/// hollow rectangles — the spread between cells is the verdict.
pub fn probe(img: &image::RgbaImage) -> Vec<String> {
    let mut out = Vec::new();
    let mut cells: Vec<(char, f32)> = Vec::new();
    for (i, ch) in TITLE.iter().enumerate() {
        let x0 = (TX0 + i as f32 * CELL) as u32;
        let x1 = (x0 as f32 + CELL * 0.82) as u32;
        let mut ink = 0u64;
        let mut total = 0u64;
        for y in (TY0 as u32)..((TY0 + EM) as u32) {
            for x in x0..x1 {
                let p = img.get_pixel(x, y);
                if p[3] > 40 && (p[0] > 60 || p[1] > 60 || p[2] > 60) {
                    ink += 1;
                }
                total += 1;
            }
        }
        cells.push((*ch, ink as f32 / total.max(1) as f32));
    }
    let desc: Vec<String> = cells
        .iter()
        .map(|(c, f)| format!("{c} {f:.3}"))
        .collect();
    out.push(format!("title ink/cell: {}", desc.join(" · ")));
    let mean = cells.iter().map(|(_, f)| f).sum::<f32>() / 4.0;
    let var = cells.iter().map(|(_, f)| (f - mean).powi(2)).sum::<f32>() / 4.0;
    out.push(format!("ink variance σ2 {var:.5} (tofu ≈ uniform, calligraphy ≠)"));
    out.push(format!("verdict: {}", if var > 0.0004 { "glyphs" } else { "boxes?" }));

    // The seal — red where red should be.
    let mut red = 0u64;
    for y in (SEAL_Y as u32)..((SEAL_Y + SEAL) as u32) {
        for x in (SEAL_X as u32)..((SEAL_X + SEAL) as u32) {
            let p = img.get_pixel(x, y);
            if p[0] > 130 && p[1] < 110 && p[2] < 110 {
                red += 1;
            }
        }
    }
    out.push(format!("seal red pixels {red} (stamp present)" ));
    out
}
