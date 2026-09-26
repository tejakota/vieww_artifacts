//! exp_dragon — *the space-filling axis.* A line that becomes an area.
//!
//! Heighway's dragon: fold a strip of paper in half, again and again —
//! each fold doubles the creases — then open every crease to a right
//! angle. Unfolded, the strip's centreline is the dragon curve: a single
//! unbroken path that never crosses itself, and in the limit **folds
//! itself into a region of positive area** — a curve of Hausdorff
//! dimension exactly 2. Beside it, Hilbert's 1891 curve: the continuous
//! surjection from the interval onto the square, the first space-filling
//! curve anyone dared to draw.
//!
//! This plate draws both as their own generation count: the dragon by 13
//! folds (8,192 turns from the paperfold parity), the Hilbert curve at
//! order 6 (4,096 cells), each self-drawing along its own length with the
//! ink cycling through the spectrum. The receipt is measured from the
//! raster itself: **the box-counting dimension of the dragon's ink —
//! occupied boxes at ε = 4, 8, 16, 32, 64 px, counted from the output
//! buffer, least-squares on log N vs log(1/ε)** — the curve's dimension
//! measured from pixels, tending to the law's 2, beside the segment count
//! the drawing actually issued.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook,
    TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, mix, AMBER, CYAN, INK, MUTED, VIOLET, MINT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The curves ──────────────────────────────────────────────────────────────

/// Heighway dragon by iterative paperfolding: turns L=−1, R=+1.
/// 13 folds → 8,192 turns → 8,193 segments.
fn dragon_turns(folds: usize) -> Vec<i32> {
    let mut turns: Vec<i32> = Vec::new();
    for _ in 0..folds {
        // seq + [1] + reversed-and-negated seq
        let mut next = turns.clone();
        next.push(1);
        for &t in turns.iter().rev() {
            next.push(-t);
        }
        turns = next;
    }
    turns
}

/// Walk the turns into points, normalised into a box (centred, aspect kept).
fn dragon_points(folds: usize, bounds: (f32, f32, f32, f32)) -> Vec<Offset> {
    let turns = dragon_turns(folds);
    let mut x = 0.0_f32;
    let mut y = 0.0_f32;
    let mut dir = 0i32; // 0 up, 1 right, 2 down, 3 left
    let mut pts: Vec<(f32, f32)> = vec![(0.0, 0.0)];
    for &t in &turns {
        dir = (dir + t).rem_euclid(4);
        match dir {
            0 => y -= 1.0,
            1 => x += 1.0,
            2 => y += 1.0,
            _ => x -= 1.0,
        }
        pts.push((x, y));
    }
    // normalise into the bounds
    let minx = pts.iter().fold(f32::INFINITY, |m, p| m.min(p.0));
    let maxx = pts.iter().fold(f32::NEG_INFINITY, |m, p| m.max(p.0));
    let miny = pts.iter().fold(f32::INFINITY, |m, p| m.min(p.1));
    let maxy = pts.iter().fold(f32::NEG_INFINITY, |m, p| m.max(p.1));
    let (bw, bh) = (maxx - minx, maxy - miny);
    let (bx, by, bwd, bht) = bounds;
    let s = (bwd / bw).min(bht / bh);
    let ox = bx + (bwd - bw * s) / 2.0;
    let oy = by + (bht - bh * s) / 2.0;
    pts.iter()
        .map(|&(px, py)| Offset::new(ox + (px - minx) * s, oy + (py - miny) * s))
        .collect()
}

/// Hilbert curve of order `n` as a point list in unit square coords.
fn hilbert_points(n: usize) -> Vec<(f32, f32)> {
    let mut pts: Vec<(f32, f32)> = Vec::new();
    // the classic recursive walk with (x, y, dx, dy as int state)
    let side = 1usize << n;
    // d2xy: index → (x, y) on the Hilbert curve (the compact standard)
    let mut idx = 0usize;
    while idx < side * side {
        // d2xy
        let mut rx;
        let mut ry;
        let mut t = idx;
        let mut x = 0usize;
        let mut y = 0usize;
        let mut s = 1usize;
        while s < side {
            rx = 1 & (t / 2);
            ry = 1 & (t ^ rx);
            // rotate
            let (rot_x, rot_y) = rot(s, x, y, rx, ry);
            x = rot_x + s * rx;
            y = rot_y + s * ry;
            t /= 4;
            s *= 2;
        }
        pts.push((x as f32 / side as f32, y as f32 / side as f32));
        idx += 1;
    }
    pts
}

fn rot(s: usize, x: usize, y: usize, rx: usize, ry: usize) -> (usize, usize) {
    if ry == 0 {
        if rx == 1 {
            return (s - 1 - x, s - 1 - y);
        }
        return (y, x);
    }
    (x, y)
}

// ── The frame ───────────────────────────────────────────────────────────────

/// Two stages: dragon left, Hilbert right.
const DR: (f32, f32, f32, f32) = (70.0, 150.0, 560.0, 500.0);
const HI: (f32, f32, f32, f32) = (680.0, 150.0, 520.0, 500.0);

pub fn frame(t: f32) -> WidgetNode {
    let dragon = dragon_points(13, DR);
    let hilbert = hilbert_points(6);
    let n_seg = dragon.len().saturating_sub(1);
    let n_hilbert = hilbert.len();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — blueprint dark.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(6, 6, 10)),
                    (1.0, Color::rgb(11, 11, 16)),
                ]),
            );

            // stage backings
            book.rrect(
                Rect::new(DR.0 - 20.0, DR.1 - 20.0, DR.0 + DR.2 + 20.0, DR.1 + DR.3 + 20.0),
                12.0,
                alpha(Color::rgb(12, 12, 18), 0.96),
            );
            book.rrect(
                Rect::new(HI.0 - 20.0, HI.1 - 20.0, HI.0 + HI.2 + 20.0, HI.1 + HI.3 + 20.0),
                12.0,
                alpha(Color::rgb(12, 12, 18), 0.96),
            );

            // ── The dragon: self-drawing, ink cycling the spectrum ──
            let n_seg = dragon.len().saturating_sub(1);
    let n_hilbert = hilbert.len();
            let reveal = ((t * 1.0).clamp(0.0, 1.0) * n_seg as f32).round() as usize;
            book.blended_layer(0.55, 0.0, BlendMode::Plus, None, |g| {
                // the glow pass — chunked so the stroke stays one verb
                for (chunk_start, chunk) in dragon[..reveal.min(dragon.len())]
                    .chunks(64)
                    .enumerate()
                {
                    if chunk.len() < 2 {
                        continue;
                    }
                    let hue = (chunk_start as f32 * 64.0 / n_seg as f32);
                    let col = spec(hue);
                    let mut path = Path::new();
                    path.move_to(chunk[0]);
                    for p in &chunk[1..] {
                        path.line_to(*p);
                    }
                    g.stroke(path, alpha(col, 0.22), 4.2);
                }
            });
            for (chunk_start, chunk) in dragon[..reveal.min(dragon.len())]
                .chunks(64)
                .enumerate()
            {
                if chunk.len() < 2 {
                    continue;
                }
                let hue = (chunk_start as f32 * 64.0 / n_seg as f32);
                let col = spec(hue);
                let mut path = Path::new();
                path.move_to(chunk[0]);
                for p in &chunk[1..] {
                    path.line_to(*p);
                }
                book.stroke(path, alpha(mix(col, INK, 0.25), 0.95), 1.8);
            }
            // the drawing tip
            if reveal > 0 && reveal < dragon.len() {
                let tip = dragon[reveal];
                book.circle(tip, 3.2, INK);
            }
            // the start point, marked
            book.ring(dragon[0], 4.0, 1.4, alpha(MUTED, 0.8));

            // ── The Hilbert curve: self-drawing in violet→mint ──
            let hn = hilbert.len();
            let hreveal = ((t * 1.15).clamp(0.0, 1.0) * (hn - 1) as f32).round() as usize;
            let map = |(ux, uy): (f32, f32)| {
                Offset::new(HI.0 + ux * HI.2, HI.1 + (1.0 - uy) * HI.3)
            };
            book.blended_layer(0.5, 0.0, BlendMode::Plus, None, |g| {
                for (cs, chunk) in hilbert[..hreveal.min(hn)].chunks(64).enumerate() {
                    if chunk.len() < 2 {
                        continue;
                    }
                    let hue = (cs as f32 * 64.0 / hn as f32);
                    let col = mix(VIOLET, MINT, hue);
                    let mut path = Path::new();
                    path.move_to(map(chunk[0]));
                    for p in &chunk[1..] {
                        path.line_to(map(*p));
                    }
                    g.stroke(path, alpha(col, 0.20), 4.0);
                }
            });
            for (cs, chunk) in hilbert[..hreveal.min(hn)].chunks(64).enumerate() {
                if chunk.len() < 2 {
                    continue;
                }
                let hue = (cs as f32 * 64.0 / hn as f32);
                let col = mix(VIOLET, MINT, hue);
                let mut path = Path::new();
                path.move_to(map(chunk[0]));
                for p in &chunk[1..] {
                    path.line_to(map(*p));
                }
                book.stroke(path, alpha(mix(col, INK, 0.2), 0.95), 1.6);
            }
            if hreveal > 0 && hreveal < hilbert.len() {
                book.circle(map(hilbert[hreveal]), 3.0, INK);
            }

            // ── The box-counting instrument, drawn over the dragon's own
            // corner: the ε-grid the probe will count with, live on the ink
            // (the real counts and the fitted D print from the raster)
            let gx = DR.0 + DR.2 - 150.0;
            let gy = DR.1 + 30.0;
            let gs = 30.0_f32; // the shown ε
            for i in 0..5 {
                let x = gx + i as f32 * gs;
                book.line(
                    Offset::new(x, gy),
                    Offset::new(x, gy + 5.0 * gs),
                    alpha(AMBER, 0.28),
                    0.8,
                );
            }
            for j in 0..5 {
                let y = gy + j as f32 * gs;
                book.line(
                    Offset::new(gx, y),
                    Offset::new(gx + 5.0 * gs, y),
                    alpha(AMBER, 0.28),
                    0.8,
                );
            }
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(n_seg, n_hilbert, 1 << 6));
    stack.into()
}

/// A spectral ink for the dragon's position along its length.
fn spec(u: f32) -> Color {
    let u = u.clamp(0.0, 1.0);
    if u < 0.5 {
        mix(VIOLET, CYAN, u * 2.0)
    } else {
        mix(CYAN, MINT, (u - 0.5) * 2.0)
    }
}

// ── The probe: box-counting dimension from the raster ───────────────────────

/// Count boxes of side ε containing dragon ink, inside the dragon's stage
/// rect only (the Hilbert stage is excluded by construction), then fit
/// log N vs log(1/ε). The dimension is measured from pixels.
pub fn probe(img: &image::RgbaImage) -> Vec<String> {
    let (iw, ih) = img.dimensions();
    let in_stage = |x: u32, y: u32| {
        (x as f32) > DR.0 && (x as f32) < DR.0 + DR.2 && (y as f32) > DR.1 && (y as f32) < DR.1 + DR.3
    };
    let mut out = Vec::new();
    let mut fit_pts: Vec<(f64, f64)> = Vec::new();
    for &eps in &[64.0_f64, 32.0, 16.0, 8.0, 4.0] {
        let mut boxes: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
        let step = eps as u32;
        let y0 = (DR.1 as u32 / step) * step;
        let x0 = (DR.0 as u32 / step) * step;
        let mut yy = y0;
        while yy < ih.min((DR.1 + DR.3) as u32) {
            let mut xx = x0;
            while xx < iw.min((DR.0 + DR.2) as u32) {
                'box2: for dy in 0..step {
                    for dx in 0..step {
                        let (px, py) = (xx + dx, yy + dy);
                        if px >= iw || py >= ih || !in_stage(px, py) {
                            continue;
                        }
                        let p = img.get_pixel(px, py);
                        let lum = (p.0[0] as u32 + p.0[1] as u32 + p.0[2] as u32) / 3;
                        if lum > 60 {
                            boxes.insert((xx / step, yy / step));
                            break 'box2;
                        }
                    }
                }
                xx += step;
            }
            yy += step;
        }
        let n = boxes.len() as f64;
        if n > 0.0 {
            out.push(format!("box-count ε={eps:.0}px: N = {n:.0}"));
            fit_pts.push(((1.0 / eps).ln(), n.ln()));
        }
    }
    let mut lines = out;
    if fit_pts.len() >= 3 {
        let n = fit_pts.len() as f64;
        let sx: f64 = fit_pts.iter().map(|p| p.0).sum();
        let sy: f64 = fit_pts.iter().map(|p| p.1).sum();
        let sxx: f64 = fit_pts.iter().map(|p| p.0 * p.0).sum();
        let sxy: f64 = fit_pts.iter().map(|p| p.0 * p.1).sum();
        let denom = n * sxx - sx * sx;
        if denom.abs() > 1e-12 {
            let slope = (n * sxy - sx * sy) / denom;
            lines.push(format!(
                "BOX-COUNTING DIMENSION: D = {slope:.3} (the law's limit: 2 — a curve with area)"
            ));
        }
    }
    lines
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(n_dragon: usize, n_hilbert: usize, order: usize) -> WidgetNode {
    let lines = [
        "DRAGON · THE SPACE-FILLING AXIS · HEIGHWAY / HILBERT".to_string(),
        format!(
            "dragon: 13 folds → {n_dragon} segments from one paperfold parity · never crosses itself"
        ),
        format!(
            "Hilbert order {order}: {n_hilbert} cells · the continuous surjection [0,1] → the square"
        ),
        "both self-draw along their own length · the ink cycles the spectrum by position".to_string(),
        "PROBE: box-count on the dragon's own stage, ε ∈ 4..64 px, read from the output buffer".to_string(),
        "log N vs log(1/ε), least squares — the dimension measured, tending to the limit 2".to_string(),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(880.0)
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

    stack.into()
}
