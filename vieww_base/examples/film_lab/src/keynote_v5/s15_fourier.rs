//! S15 · FOURIER CANVAS — the reference's second stress plate. 2:20–2:30.
//!
//! *"The Fourier Canvas: render thousands of active shape geometry runs
//! per frame in a scroll view without breaking a sweat or dropping
//! frames."*
//!
//! The fourier.say buffer opens; the preview becomes the synthesis canvas:
//! a closed signature line — the film's own 'w' — decomposed by a real
//! DFT into a choir of rotating vectors that chain tip-to-tail and walk
//! the line back into existence, ink-first (the fourier plate's grammar,
//! at studio scale). Beneath the machine, the scroll view: rows of active
//! geometry runs streaming past, dash-marching, the run counter climbing
//! with the film's own clock.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, ease_out_back, tint, AMBER, CYAN, CYAN_SOFT, MUTED, VIOLET,
    VIOLET_SOFT,
};

use super::studio;
use super::{group_commas, Ctx};

// ── The signature + its DFT ─────────────────────────────────────────────────

/// The source: a stylised 'w' as a closed polygon — three valleys, the
/// wordmark's initial, closed by the sky. Local 320×220 box.
const SIG: [(f32, f32); 20] = [
    (10.0, 40.0),
    (46.0, 180.0),
    (66.0, 208.0),
    (86.0, 180.0),
    (110.0, 70.0),
    (128.0, 40.0),
    (146.0, 70.0),
    (168.0, 176.0),
    (188.0, 206.0),
    (206.0, 176.0),
    (232.0, 66.0),
    (248.0, 40.0),
    (264.0, 72.0),
    (282.0, 160.0),
    (300.0, 148.0),
    (292.0, 96.0),
    (262.0, 34.0),
    (180.0, 10.0),
    (92.0, 22.0),
    (34.0, 52.0),
];

/// One Fourier coefficient: amplitude, phase, frequency index.
#[derive(Clone)]
struct Coef {
    r: f32,
    a0: f32,
    k: i32,
}

/// The DFT of the signature, sorted largest-first — computed once per
/// frame from the same arithmetic that draws it (20 vectors, exact).
fn coefs() -> Vec<Coef> {
    let n = SIG.len() as f32;
    let (mut cx, mut cy) = (0.0f32, 0.0f32);
    for p in SIG.iter() {
        cx += p.0;
        cy += p.1;
    }
    cx /= n;
    cy /= n;
    let mut out = Vec::new();
    for k in 0..SIG.len() {
        let (mut re, mut im) = (0.0f32, 0.0f32);
        for (j, p) in SIG.iter().enumerate() {
            let ang = -std::f32::consts::TAU * k as f32 * j as f32 / n;
            re += (p.0 - cx) * ang.cos() - (p.1 - cy) * ang.sin();
            im += (p.0 - cx) * ang.sin() + (p.1 - cy) * ang.cos();
        }
        re /= n;
        im /= n;
        let kk = k as i32;
        let k = if kk * 2 > SIG.len() as i32 {
            kk - SIG.len() as i32
        } else {
            kk
        };
        out.push(Coef {
            r: (re * re + im * im).sqrt(),
            a0: im.atan2(re),
            k,
        });
    }
    out.sort_by(|x, y| y.r.partial_cmp(&x.r).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// The buffer — the fourier's own say program.
fn fourier_lines() -> Vec<Vec<studio::Seg>> {
    let kw = VIOLET_SOFT;
    let st = AMBER;
    let nu = crate::film_lib::MINT;
    let tx = studio::CODE_PLAIN;
    let pu = MUTED;
    vec![
        vec![("keep ", kw), ("a line called ", tx), ("signature", tx)],
        vec![],
        vec![("screen ", kw), ("\"Fourier\"", st), (":", pu)],
        vec![
            ("    a scroll view, spaced ", pu),
            ("18", nu),
            (", children aligned to the start:", pu),
        ],
        vec![("        the canvas, drawing the signature", tx)],
        vec![
            ("        a choir of ", pu),
            ("20", nu),
            (" circles, chained", tx),
        ],
        vec![
            ("        a scroll of ", pu),
            ("geometry runs", tx),
            (", active", pu),
        ],
    ]
}

/// The pane's inner size.
const CW: f32 = 900.0;
const CH: f32 = 880.0;

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;
    let ladder = ctx.ladder;
    let sec = ctx.sec;

    let open = ease_out_back(clamp01((t - 0.08) / 0.16));

    // The machine's phase — the vectors rotate with the film clock.
    let phase = sec * 1.5;
    // The line draws itself once (0.30 → 0.80), then holds and glows.
    let ink_p = ease_in_out(clamp01((t - 0.28) / 0.50));
    let cs = coefs();

    // The run counter — geometry runs served by the scroll strip, climbing
    // with the film's own clock (rows × their dash segments, live).
    let rows = 9;
    let run_rate = 42.0; // runs per second, the strip's own cadence
    let runs = (sec * run_rate) as u64;

    let sheet: WidgetNode = Painting::sized(
        Size::new(CW, CH),
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            // The canvas ground.
            book.rect(
                Rect::new(0.0, 0.0, CW, CH),
                Gradient::vertical()
                    .with_dither()
                    .with_stops(&[(0.0, Color::rgb(10, 10, 14)), (1.0, Color::rgb(13, 12, 19))]),
            );
            // Fine grid.
            let mut x = 0.0;
            while x < CW {
                book.line(
                    Offset::new(x, 0.0),
                    Offset::new(x, CH * 0.62),
                    alpha(CYAN, 0.03),
                    1.0,
                );
                x += 64.0;
            }
            let mut y = 0.0;
            while y < CH * 0.62 {
                book.line(
                    Offset::new(0.0, y),
                    Offset::new(CW, y),
                    alpha(CYAN, 0.03),
                    1.0,
                );
                y += 64.0;
            }

            // ── The machine: the choir, chained tip-to-tail.
            let origin = Offset::new(CW * 0.5, CH * 0.30);
            let mut tip = origin;
            let n_circles = (cs.len() as f32 * clamp01(open * 1.2)) as usize;
            for (i, c) in cs.iter().enumerate() {
                if i >= n_circles {
                    break;
                }
                let a = c.a0 + c.k as f32 * phase;
                let next =
                    Offset::new(tip.dx + a.cos() * c.r * 1.55, tip.dy + a.sin() * c.r * 1.55);
                // The circle — faint, the machine's bones.
                book.stroke(
                    super::circle_path(tip.dx, tip.dy, c.r * 1.55, 40),
                    alpha(VIOLET, if i == 0 { 0.22 } else { 0.10 }),
                    1.0,
                );
                // The vector — the rotating arm.
                book.line(tip, next, alpha(CYAN_SOFT, 0.55), 1.2);
                book.circle(tip, 1.6, alpha(CYAN_SOFT, 0.8));
                tip = next;
            }
            // The tip's pen.
            book.circle(tip, 3.0, alpha(tint(VIOLET_SOFT, 0.4), 1.0));
            super::glow(book, tip.dx, tip.dy, 60.0, VIOLET, 0.30);

            // ── The ink: the signature traced by the sum, drawn to the
            // current phase (the line walks itself into existence).
            let mut ink = Path::new();
            let total_steps = 260;
            let drawn_steps = (total_steps as f32 * ink_p).round() as usize;
            for i in 0..=drawn_steps {
                let ph = phase - i as f32 * 0.028;
                let mut pt = origin;
                for c in cs.iter() {
                    let a = c.a0 + c.k as f32 * ph;
                    pt = Offset::new(pt.dx + a.cos() * c.r * 1.55, pt.dy + a.sin() * c.r * 1.55);
                }
                if i == 0 {
                    ink.move_to(pt);
                } else {
                    ink.line_to(pt);
                }
            }
            book.layer(1.0, 6.0, None, |g| {
                g.stroke(ink.clone(), alpha(VIOLET, 0.30), 7.0);
            });
            book.stroke(ink, alpha(tint(VIOLET_SOFT, 0.25), 0.98), 2.6);

            // ── The scroll view: rows of active geometry runs streaming
            // past beneath the machine — dash-marching polylines.
            let strip_y0 = CH * 0.66;
            let row_h = (CH - strip_y0 - 20.0) / rows as f32;
            for r in 0..rows {
                let y = strip_y0 + 10.0 + r as f32 * row_h;
                // The row's hairline rail.
                book.line(
                    Offset::new(24.0, y),
                    Offset::new(CW - 24.0, y),
                    alpha(Color::WHITE, 0.05),
                    1.0,
                );
                // The run — a waveform polyline, dash-marching.
                let mut p = Path::new();
                let scroll = sec * (60.0 + r as f32 * 9.0);
                for i in 0..64 {
                    let x = 24.0 + i as f32 / 63.0 * (CW - 48.0);
                    let wave = (x * 0.030 + r as f32 * 1.7).sin()
                        * (x * 0.011 - r as f32 * 0.9).cos()
                        * row_h
                        * 0.32;
                    let xx = x - scroll.rem_euclid(CW - 48.0);
                    let xx = if xx < 24.0 { xx + (CW - 48.0) } else { xx };
                    let pt = Offset::new(xx, y + wave);
                    if i == 0 {
                        p.move_to(pt);
                    } else {
                        p.line_to(pt);
                    }
                }
                let warm = r % 3 == 0;
                book.stroke_styled(
                    p,
                    alpha(
                        if warm { AMBER } else { CYAN_SOFT },
                        0.42 + 0.2 * ((sec * 2.0 + r as f32).sin()),
                    ),
                    1.5,
                    vieww_foundation::StrokeStyle::rounded()
                        .dash(vieww_foundation::Dash::even(7.0).offset(-sec * 26.0)),
                );
            }
        }),
    )
    .into();

    let spec = studio::Spec {
        code: studio::Code::Lines {
            lines: fourier_lines(),
            blink: ctx.sec,
        },
        app: studio::App::new(1, super::tap_pulse(abs), abs),
        preview_custom: Some(sheet),
        session_line: 1.0,
        tab: Some("fourier.say".to_string()),
        ..Default::default()
    };

    let mut stack = Stack::new().push(Positioned::fill().child(studio::studio(abs, ladder, spec)));

    // The run counter — live, climbing with the clock.
    let counter_a = clamp01((t - 0.30) / 0.14);
    if counter_a > 0.0 {
        stack = stack.push(
            Positioned::new()
                .left(studio::PV_X0 + 40.0)
                .top(studio::TITLE_H + 60.0 + studio::PV_H - 620.0)
                .width(640.0)
                .height(40.0)
                .child(Opacity::new(counter_a).child(super::chip(
                    format!("{} geometry runs served", group_commas(runs)),
                    15.0,
                    tint(CYAN_SOFT, 0.1),
                ))),
        );
    }
    // The plate receipt — quoted, labelled.
    let plate_a = clamp01((t - 0.52) / 0.14);
    if plate_a > 0.0 {
        stack = stack.push(
            Positioned::new()
                .left(studio::PV_X0 + 40.0)
                .top(studio::TITLE_H + 60.0 + studio::PV_H - 570.0)
                .width(640.0)
                .height(40.0)
                .child(Opacity::new(plate_a).child(super::chip(
                    "plate receipt · fourier — 2,004 shapes · 32 frames",
                    13.5,
                    alpha(MUTED, 0.9),
                ))),
        );
    }

    stack = stack.push(super::caption(
        "the fourier canvas — a choir of circles walks the line back",
        1000.0,
        clamp01((t - 0.20) / 0.12),
    ));
    stack = stack.push(super::caption(
        "thousands of runs per frame — the cadence never drops",
        964.0,
        clamp01((t - 0.46) / 0.12),
    ));

    stack.into()
}
