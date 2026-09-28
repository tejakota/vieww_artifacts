//! S19 · NINETY PLATES — the determinism wall. 3:02–3:13.
//!
//! The reference's "Ultimate Flex": a grid of the lab's deterministic
//! render plates — **90 plates**, the plan's own count — every cell a
//! live vignette animated by the film's clock, every one re-renderable
//! to the byte on a given bench. The receipts beside them: flat RSS over
//! long playback (38.3 → 39.0 MiB over 256 frames — longplay's own
//! metrics), pixel-predictable outputs, and the plates' names — the
//! lab's real registry, verbatim.

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{alpha, clamp01, ease_out_back, ease_out_cubic, mix, tint, xywh, FAINT, INK, MUTED, Rng, VIOLET, VIOLET_SOFT, CYAN, CYAN_SOFT, MINT, AMBER, MAGENTA};

use super::{Ctx};

/// The plate wall — 90 names, the lab's real registry (the plan §2.3's
/// "90 plates"; the count is the array's own length, counted).
const PLATES: [&str; 90] = [
    "light", "mesh", "ocean", "kinetic", "circuit", "globe", "receipts", "aurora",
    "spring", "scrub", "damage", "rackfocus", "morph", "endcard", "wordmark", "beams",
    "liquid", "unfold", "shatter", "settle", "ghosts", "dolly", "currents", "probe",
    "avatar", "fadeaway", "sea", "tesseract", "blackhole", "galaxy", "forest", "city",
    "typo", "mandel", "hero4k", "han", "megapath", "longplay", "swarm", "blendmatrix",
    "filterstack", "shadowplay", "prism", "eclipse", "cymatics", "harmony", "fourier", "startrail",
    "bubble", "orrery", "storm", "kaleido", "ink", "quantum", "smoke", "threebody",
    "turing", "galton", "caustics", "ising", "crystal", "neural", "truss", "cellauto",
    "collider", "sandpile", "percolation", "ant", "dla", "lorenz", "doublepend", "penrose",
    "dragon", "gw", "foucault", "rainbow", "sorting", "gas", "evolve", "slime",
    "epidemic", "traffic", "huffman", "maze", "newton", "planck", "excite", "pidigits",
    "pathtrace", "hero",
];

/// The wall's geometry — 10 × 9.
const COLS: usize = 10;
const CELL: f32 = 132.0;
const GAP: f32 = 8.0;

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let wall_x = 210.0;
    let wall_y = 200.0;

    let mut stack = Stack::new();

    // The ground.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            super::ground(book, w, h);
            super::stars(book, w, h, 0x9012, 60, t, 0.06);
            super::vignette(book, w, h, 0.55);
        }),
    )));

    // The wall — one paint pass, every cell a live vignette.
    let arrive = ease_out_back(clamp01(t / 0.22));
    if arrive > 0.0 {
        let cells = PLATES.to_vec();
        stack = stack.push(Positioned::new()
            .left(wall_x)
            .top(wall_y)
            .width(COLS as f32 * (CELL + GAP))
            .height((PLATES.len() / COLS) as f32 * (CELL + GAP) + 26.0)
            .child(Opacity::new(clamp01(arrive * 1.4)).child(Painting::sized(
                Size::new(COLS as f32 * (CELL + GAP), (PLATES.len() / COLS) as f32 * (CELL + GAP) + 26.0),
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    for (idx, _name) in cells.iter().enumerate() {
                        let gx = idx % COLS;
                        let gy = idx / COLS;
                        let x = gx as f32 * (CELL + GAP);
                        let y = gy as f32 * (CELL + GAP);
                        // The cell — dark glass.
                        book.rrect(xywh(x, y, CELL, CELL), 8.0, alpha(Color::rgb(13, 14, 19), 0.95));
                        book.stroke_rrect(xywh(x, y, CELL, CELL), 8.0, alpha(Color::WHITE, 0.07), 1.0);
                        // The vignette — one of nine engines, by index.
                        let vx = x + 8.0;
                        let vy = y + 8.0;
                        let vw = CELL - 16.0;
                        let vh = CELL - 34.0;
                        match idx % 9 {
                            0 => v_spiral(book, vx, vy, vw, vh, sec, idx),
                            1 => v_grid_zoom(book, vx, vy, vw, vh, sec, idx),
                            2 => v_tree_fractal(book, vx, vy, vw, vh, sec, idx),
                            3 => v_orbit(book, vx, vy, vw, vh, sec, idx),
                            4 => v_wave_field(book, vx, vy, vw, vh, sec, idx),
                            5 => v_flow(book, vx, vy, vw, vh, sec, idx),
                            6 => v_mosaic(book, vx, vy, vw, vh, sec, idx),
                            7 => v_rings(book, vx, vy, vw, vh, sec, idx),
                            _ => v_attractor(book, vx, vy, vw, vh, sec, idx),
                        }
                        // The name rail — a light bar under the vignette
                        // (the names ride as text above, one run per cell).
                        book.rect(xywh(x + 8.0, y + CELL - 20.0, (CELL - 16.0) * 0.7, 2.0), alpha(VIOLET_SOFT, 0.4));
                    }
                }),
            ))));
        // The names — tiny mono labels, one per cell (the plate registry,
        // printed as the wall's own legend).
        for (idx, name) in PLATES.iter().enumerate() {
            let gx = idx % COLS;
            let gy = idx / COLS;
            let x = wall_x + gx as f32 * (CELL + GAP);
            let y = wall_y + gy as f32 * (CELL + GAP) + CELL - 20.0;
            stack = stack.push(
                Positioned::new()
                    .left(x + 7.0)
                    .top(y)
                    .width(CELL - 12.0)
                    .height(16.0)
                    .child(Opacity::new(clamp01(arrive * 1.4)).child(
                        Text::new(*name)
                            .style(TextStyle::new(9.5).monospace().color(alpha(INK, 0.72))),
                    )),
            );
        }
    }

    // The headline + receipts.
    let head_a = clamp01((t - 0.06) / 0.14);
    stack = stack.push(
        Positioned::new()
            .left(0.0)
            .top(122.0)
            .width(1920.0)
            .height(44.0)
            .child(Opacity::new(head_a).child(
                Text::new(format!("{} deterministic plates", PLATES.len()))
                    .style(TextStyle::new(34.0).letter_spacing(1.5).color(alpha(INK, 0.96)))
                    .align(TextAlign::Center),
            )),
    );
    let rec_a = clamp01((t - 0.44) / 0.14);
    if rec_a > 0.0 {
        for (i, line) in [
            "pixel-predictable · re-rendered to the byte on a named bench",
            "flat RSS over long playback — 38.3 → 39.0 MiB, 256 frames (longplay's own receipt)",
        ]
        .iter()
        .enumerate()
        {
            stack = stack.push(
                Positioned::new()
                    .left(0.0)
                    .top(946.0 + i as f32 * 30.0)
                    .width(1920.0)
                    .height(26.0)
                    .child(Opacity::new(rec_a).child(
                        Text::new(*line)
                            .style(TextStyle::new(16.0).monospace().letter_spacing(1.6).color(alpha(MUTED, 0.9)))
                            .align(TextAlign::Center),
                    )),
            );
        }
    }

    stack = stack.push(super::caption(
        "every plate a receipt — sheet, gif, metrics, the three-artifact set",
        1000.0,
        clamp01((t - 0.58) / 0.14),
    ));

    stack.into()
}

// ── The nine vignette engines ───────────────────────────────────────────────

fn v_spiral(b: &mut Sketchbook, x: f32, y: f32, w: f32, h: f32, sec: f32, seed: usize) {
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    let mut rng = Rng::new(0x500 + seed as u64);
    for _ in 0..40 {
        let r = rng.f01().sqrt() * w.min(h) * 0.5;
        let a = sec * 0.8 + rng.f01() * std::f32::consts::TAU;
        b.circle(Offset::new(cx + a.cos() * r, cy + a.sin() * r * 0.6), 0.8, alpha(VIOLET_SOFT, 0.6));
    }
}

fn v_grid_zoom(b: &mut Sketchbook, x: f32, y: f32, w: f32, h: f32, sec: f32, seed: usize) {
    let n = 10;
    let cw = w / n as f32;
    let chh = h / (n / 2) as f32;
    let ph = sec * 0.6 + seed as f32;
    for gy in 0..(n / 2) {
        for gx in 0..n {
            let v = ((gx + gy) as f32 * 0.7 + ph).sin();
            b.rect(
                xywh(x + gx as f32 * cw, y + gy as f32 * chh, cw - 1.0, chh - 1.0),
                alpha(mix(MAGENTA, CYAN, (v + 1.0) * 0.5), 0.5),
            );
        }
    }
}

/// A recursive branch draw — closures can't recurse, so it's a fn.
fn branch(b: &mut Sketchbook, x0: f32, y0: f32, ang: f32, len: f32, depth: u32, sway: f32) {
    let x1 = x0 + ang.sin() * len;
    let y1 = y0 - ang.cos() * len;
    b.line(Offset::new(x0, y0), Offset::new(x1, y1), alpha(MINT, 0.16 * depth as f32 + 0.12), 1.0);
    if depth > 1 {
        let l = len * 0.72;
        branch(b, x1, y1, ang - 0.5 + sway, l, depth - 1, sway);
        branch(b, x1, y1, ang + 0.5 - sway, l, depth - 1, sway);
    }
}

fn v_tree_fractal(b: &mut Sketchbook, x: f32, y: f32, w: f32, h: f32, sec: f32, _seed: usize) {
    let sway = (sec * 1.2).sin() * 0.08;
    branch(b, x + w * 0.5, y + h, 0.0, h * 0.42, 5, sway);
}

fn v_orbit(b: &mut Sketchbook, x: f32, y: f32, w: f32, h: f32, sec: f32, seed: usize) {
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    b.circle(Offset::new(cx, cy), 4.0, alpha(AMBER, 0.95));
    for i in 0..3 {
        let r = 12.0 + i as f32 * 9.0;
        let a = sec * (1.4 - i as f32 * 0.35) + seed as f32 + i as f32;
        b.stroke(super::circle_path(cx, cy, r, 24), alpha(Color::WHITE, 0.08), 1.0);
        b.circle(Offset::new(cx + a.cos() * r, cy + a.sin() * r), 2.2, alpha(CYAN_SOFT, 0.9));
    }
}

fn v_wave_field(b: &mut Sketchbook, x: f32, y: f32, w: f32, h: f32, sec: f32, seed: usize) {
    for r in 0..4 {
        let mut p = vieww_foundation::Path::new();
        for i in 0..16 {
            let px = x + i as f32 / 15.0 * w;
            let py = y + h * 0.2 + r as f32 * h * 0.2
                + (px * 0.08 + r as f32 * 1.3 + sec * 2.0 + seed as f32).sin() * h * 0.08;
            if i == 0 { p.move_to(Offset::new(px, py)); } else { p.line_to(Offset::new(px, py)); }
        }
        b.stroke(p, alpha(VIOLET, 0.4 + r as f32 * 0.1), 1.2);
    }
}

fn v_flow(b: &mut Sketchbook, x: f32, y: f32, w: f32, h: f32, sec: f32, seed: usize) {
    let mut rng = Rng::new(0xF10 + seed as u64);
    for _ in 0..18 {
        let px0 = x + rng.f01() * w;
        let py0 = y + rng.f01() * h;
        let mut p = vieww_foundation::Path::new();
        p.move_to(Offset::new(px0, py0));
        for i in 1..6 {
            let k = i as f32 / 5.0;
            let px = px0 + (sec * 20.0 + rng.f01() * 40.0 + k * 30.0).rem_euclid(w);
            let py = py0 + (px * 0.05 + sec).sin() * 6.0;
            p.line_to(Offset::new(x + (px - x).rem_euclid(w), py));
        }
        b.stroke_styled(
            p,
            alpha(CYAN_SOFT, 0.5),
            1.0,
            vieww_foundation::StrokeStyle::rounded().dash(vieww_foundation::Dash::even(4.0).offset(-sec * 18.0)),
        );
    }
}

fn v_mosaic(b: &mut Sketchbook, x: f32, y: f32, w: f32, h: f32, sec: f32, seed: usize) {
    let n = 6;
    let cw = w / n as f32;
    let chh = h / n as f32;
    let mut rng = Rng::new(0x40C + seed as u64);
    for gy in 0..n {
        for gx in 0..n {
            let lit = rng.f01() > 0.4;
            let flick = rng.f01() > 0.85 && (sec * 3.0 + rng.f01() * 7.0).sin() > 0.0;
            let on = lit != flick;
            b.rect(xywh(x + gx as f32 * cw, y + gy as f32 * chh, cw - 2.0, chh - 2.0), alpha(AMBER, if on { 0.55 } else { 0.06 }));
        }
    }
}

fn v_rings(b: &mut Sketchbook, x: f32, y: f32, w: f32, h: f32, sec: f32, seed: usize) {
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    for i in 0..4 {
        let r = (w.min(h) * 0.14) + i as f32 * (w.min(h) * 0.11);
        let phase = (sec * 0.7 + i as f32 * 0.25 + seed as f32 * 0.1).fract();
        b.ring(Offset::new(cx, cy), r * (0.6 + phase * 0.7), 1.4, alpha(VIOLET_SOFT, (1.0 - phase) * 0.5));
    }
}

fn v_attractor(b: &mut Sketchbook, x: f32, y: f32, w: f32, h: f32, sec: f32, seed: usize) {
    // A Lorenz-ish trail — the lab's favourite shape.
    let cx = x + w * 0.5;
    let cy = y + h * 0.55;
    let mut p = vieww_foundation::Path::new();
    let (mut px, mut py, mut pz) = (0.1f32, 0.0f32, 0.0f32);
    let (a, b_, c) = (10.0f32, 28.0f32, 8.0f32 / 3.0);
    let dt = 0.006;
    let skip = ((sec * 60.0) as usize + seed * 40) % 400;
    for i in 0..240 {
        let dx = a * (py - px);
        let dy = px * (b_ - pz) - py;
        let dz = px * py - c * pz;
        px += dx * dt;
        py += dy * dt;
        pz += dz * dt;
        let sx = cx + px * w * 0.055;
        let sy = cy - (pz - 24.0) * h * 0.030;
        if i == 0 || i < skip % 40 {
            p.move_to(Offset::new(sx, sy));
        } else {
            p.line_to(Offset::new(sx, sy));
        }
    }
    b.stroke(p, alpha(tint(CYAN_SOFT, 0.1), 0.75), 1.0);
}
