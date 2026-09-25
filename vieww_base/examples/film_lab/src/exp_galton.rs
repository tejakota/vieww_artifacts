//! exp_galton — *the central-limit axis.* The theorem you can hear.
//!
//! Francis Galton's 1889 bean machine: balls drop through a lattice of
//! pegs, each peg flipping the ball left or right, and the bins below
//! assemble a bell curve — the binomial distribution, and (by de Moivre's
//! limit) the Gaussian, arriving one ball at a time. This plate is that
//! machine, exactly: **6,000 balls, 24 rows, every ball's path a
//! deterministic bit stream from its own index** — no state, no memory,
//! every frame a pure function of t.
//!
//! The bell curve is not drawn by hope: the theoretical binomial
//! p(k) = C(24,k)/2²⁴ is computed from the same peg count that flipped
//! the balls, scaled to the landed population, and stroked over the bins
//! in amber — the theorem and the machine, overlaid. The receipt measures
//! their agreement: each bin's |measured − theory|, the maximum of it, and
//! the width of the landed pile against σ = √(np(1−p)) = √6 — every number
//! counted from the arrays that drew the frame.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle,
    StrokeStyle, Dash};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, mix, tint, AMBER, CYAN,
    INK, MUTED};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The machine ─────────────────────────────────────────────────────────────

/// Peg rows — the binomial's n.
const ROWS: usize = 24;

/// Balls — the population that makes the bell visible.
const BALLS: usize = 6000;

/// Time (in t-fraction) for one ball to traverse the WHOLE peg ladder.
/// (The first cut treated this as per-ROW time — the full ladder then took
/// 24×0.16 = 3.84 timelines and no ball ever landed; the receipt's
/// "landed 0 · airborne 6000" said so to our face.)
const FALL_SPAN: f32 = 0.16;

/// When the last ball is released (t-fraction) — the stream's span.
const RELEASE_SPAN: f32 = 0.62;

/// One ball: its per-row directions (0 = left, 1 = right), precomputed
/// from the seeded RNG — the path is the ball's identity.
struct Ball {
    dirs: Vec<u8>,
    /// Release time (t-fraction).
    depart: f32,
}


fn balls() -> Vec<Ball> {
    let mut rng = crate::film_lib::Rng::new(0xBEA4);
    (0..BALLS)
        .map(|i| {
            let dirs = (0..ROWS).map(|_| if rng.f01() < 0.5 { 0 } else { 1 }).collect();
            Ball {
                dirs,
                depart: i as f32 / BALLS as f32 * RELEASE_SPAN,
            }
        })
        .collect()
}

/// The ball's state at film-fraction t: how many pegs it has passed and
/// its bin index if landed. Returns (rows_done ∈ [0, ROWS], progress
/// through the current row ∈ [0,1), landed).
#[must_use]
fn ball_state(b: &Ball, t: f32) -> (usize, f32, bool) {
    // Rows fallen = elapsed / full-ladder-time × rows — the whole ladder
    // costs FALL_SPAN, not FALL_SPAN per row.
    let travel = (t - b.depart) / FALL_SPAN * ROWS as f32;
    if travel <= 0.0 {
        return (0, 0.0, false);
    }
    if travel >= ROWS as f32 {
        return (ROWS, 0.0, true);
    }
    let done = travel.floor() as usize;
    (done, travel - done as f32, false)
}

/// The bin a ball lands in (number of rights) — its path's sum.
#[must_use]
fn bin_of(b: &Ball) -> usize {
    b.dirs.iter().map(|&d| d as usize).sum()
}

/// The binomial coefficient C(n, k) — exact in u64 for n = 24.
#[must_use]
fn binom(n: u64, k: u64) -> u64 {
    if k > n {
        return 0;
    }
    let k = k.min(n - k);
    let mut acc = 1u64;
    for i in 0..k {
        acc = acc * (n - i) / (i + 1);
    }
    acc
}

// ── The geometry ────────────────────────────────────────────────────────────

/// The machine's window: pegs occupy the top, bins the bottom.
const WIN: (f32, f32, f32, f32) = (250.0, 96.0, 780.0, 596.0); // x, y, w, h

/// Peg row spacing.
const ROW_H: f32 = 15.5;

/// Bin count (0..=24 rights → 25 bins).
const BINS: usize = ROWS + 1;

pub fn frame(t: f32) -> WidgetNode {
    let all = balls();

    // The census: landed balls per bin at this t (measured, not assumed).
    let mut counts = vec![0usize; BINS];
    let mut in_flight = 0usize;
    for b in &all {
        let (done, _prog, landed) = ball_state(b, t);
        if landed {
            counts[bin_of(b)] += 1;
        } else if done > 0 || t >= b.depart {
            in_flight += 1;
        }
    }
    let landed_n: usize = counts.iter().sum();

    // The theory: binomial scaled to the landed population.
    let theory: Vec<f64> = (0..BINS)
        .map(|k| binom(ROWS as u64, k as u64) as f64 / 2f64.powi(ROWS as i32))
        .collect();

    // Measured σ vs theory σ, from the same counts.
    let (m_sigma, t_sigma) = {
        let n = landed_n.max(1) as f64;
        let mean: f64 = counts
            .iter()
            .enumerate()
            .map(|(k, &c)| k as f64 * c as f64)
            .sum::<f64>()
            / n;
        let var: f64 = counts
            .iter()
            .enumerate()
            .map(|(k, &c)| (k as f64 - mean).powi(2) * c as f64)
            .sum::<f64>()
            / n;
        (
            var.sqrt(),
            (ROWS as f64 * 0.25f64).sqrt(),
        )
    };
    // The bins' worst disagreement with theory, in balls.
    let max_dev = counts
        .iter()
        .enumerate()
        .map(|(k, &c)| (c as f64 - theory[k] * landed_n as f64).abs())
        .fold(0.0_f64, f64::max);

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;
            let (wx, wy, ww, wh) = WIN;

            // The ground — a Victorian instrument parlour, in lab colours.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(7, 7, 10)),
                    (1.0, Color::rgb(12, 12, 16)),
                ]),
            );

            // The frame: a glass case, wood-dark rails.
            book.rrect(
                Rect::new(wx - 24.0, wy - 24.0, wx + ww + 24.0, wy + wh + 24.0),
                14.0,
                alpha(Color::rgb(26, 22, 18), 0.96),
            );
            book.stroke_rrect(
                Rect::new(wx - 24.0, wy - 24.0, wx + ww + 24.0, wy + wh + 24.0),
                14.0,
                alpha(tint(AMBER, 0.2), 0.30),
                1.2,
            );

            // The hopper: where the stream enters.
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                g.circle(
                    Offset::new(wx + ww * 0.5, wy - 6.0),
                    26.0,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(tint(AMBER, 0.45), 0.5)),
                        (1.0, alpha(AMBER, 0.0)),
                    ]),
                );
            });
            // The funnel walls.
            book.line(
                Offset::new(wx + ww * 0.5 - 70.0, wy - 24.0),
                Offset::new(wx + ww * 0.5 - 14.0, wy + 6.0),
                alpha(tint(AMBER, 0.25), 0.5),
                2.2,
            );
            book.line(
                Offset::new(wx + ww * 0.5 + 70.0, wy - 24.0),
                Offset::new(wx + ww * 0.5 + 14.0, wy + 6.0),
                alpha(tint(AMBER, 0.25), 0.5),
                2.2,
            );

            // ── The peg lattice — Pascal's triangle, exactly ─────────
            // Row r has r+1 pegs at x = cx + (i − r/2)·bin_w: each ball
            // with `rights` so far bounces off peg #rights in every row,
            // moving ±bin_w/2 — the lattice IS the binomial's sample path.
            let bin_w = ww / BINS as f32;
            let cx = wx + ww * 0.5;
            let peg_y = |r: usize| wy + 26.0 + r as f32 * ROW_H;
            for r in 0..ROWS {
                for i in 0..=r {
                    let x = cx + (i as f32 - r as f32 / 2.0) * bin_w;
                    book.circle(Offset::new(x, peg_y(r)), 2.1, alpha(tint(AMBER, 0.28), 0.75));
                }
            }

            // ── The balls ──────────────────────────────────────────
            // In flight: falling peg-to-peg, ±bin_w/2 per row — the
            // sample path itself. Landed: stacked as sand in the bins.
            let floor_y = wy + wh - 22.0;
            let max_count = counts.iter().copied().max().unwrap_or(1).max(1) as f32;
            let level_h = 128.0 / max_count;
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                for b in all.iter() {
                    let (done, prog, landed) = ball_state(b, t);
                    if landed {
                        continue;
                    }
                    let rights: usize = b.dirs[..done.min(ROWS)].iter().map(|&d| d as usize).sum();
                    let r = done.min(ROWS - 1);
                    // At peg #rights of row r, lurching through the
                    // current deflection — the path IS the position.
                    let dir = b.dirs[r] as f32 * 2.0 - 1.0;
                    let x = cx + (rights as f32 - r as f32 / 2.0) * bin_w
                        + dir * prog * bin_w * 0.5;
                    let y = peg_y(r) + prog * ROW_H;
                    let a = 0.30 + 0.5 * (1.0 - prog);
                    g.rect(
                        Rect::new(x - 1.4, y - 1.4, x + 1.4, y + 1.4),
                        alpha(tint(AMBER, 0.5), a),
                    );
                }

                // Landed: sand columns, normalised to the tallest bin.
                let mut stack = vec![0usize; BINS];
                for b in all.iter() {
                    if !ball_state(b, t).2 {
                        continue;
                    }
                    let k = bin_of(b);
                    let layer = stack[k];
                    stack[k] += 1;
                    // 14 sub-columns of jitter width per bin, set by
                    // landing order — deterministic sand.
                    let (sub, lev) = (layer % 14, layer / 14);
                    let x = wx + (k as f32 + 0.12) * bin_w
                        + sub as f32 * bin_w * 0.76 / 13.0;
                    let y = floor_y - (lev as f32 + 1.0) * level_h;
                    let heat = (lev as f32 * level_h / 128.0).min(1.0);
                    g.rect(
                        Rect::new(x - 0.9, y - 0.9, x + 0.9, y + 0.9),
                        alpha(mix(AMBER, tint(AMBER, 0.55), heat), 0.85),
                    );
                }

                // The theory curve, over the bins: predicted count per
                // bin under the SAME normalisation as the sand.
                if landed_n > 24 {
                    let mut path = Path::new();
                    for (k, &p) in theory.iter().enumerate() {
                        let predicted = p * landed_n as f64;
                        let height = (predicted as f32 / max_count) * 128.0;
                        let x = wx + (k as f32 + 0.5) * bin_w;
                        let y = floor_y - height;
                        if k == 0 {
                            path.move_to(Offset::new(x, y));
                        } else {
                            path.line_to(Offset::new(x, y));
                        }
                    }
                    g.stroke_styled(
                        path,
                        alpha(tint(CYAN, 0.35), 0.95),
                        2.0,
                        StrokeStyle::rounded().dash(Dash::even(7.0)),
                    );
                }
            });

            // ── The bins: glass dividers ───────────────────────────────
            for k in 0..=BINS {
                let x = wx + k as f32 * bin_w;
                book.line(
                    Offset::new(x, floor_y - 128.0),
                    Offset::new(x, floor_y + 2.0),
                    alpha(tint(AMBER, 0.2), 0.16),
                    1.0,
                );
            }
            // The floor.
            book.line(
                Offset::new(wx, floor_y + 2.0),
                Offset::new(wx + ww, floor_y + 2.0),
                alpha(tint(AMBER, 0.3), 0.5),
                2.0,
            );

            // The σ ruler: a horizontal bracket at ±t_sigma bins.
            let sx = t_sigma as f32 * bin_w;
            let sy = floor_y - 148.0;
            book.line(
                Offset::new(cx - sx, sy),
                Offset::new(cx + sx, sy),
                alpha(CYAN, 0.55),
                1.2,
            );
            for dir in [-1.0_f32, 1.0] {
                book.line(
                    Offset::new(cx + dir * sx, sy - 4.0),
                    Offset::new(cx + dir * sx, sy + 4.0),
                    alpha(CYAN, 0.55),
                    1.2,
                );
            }
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(
        landed_n,
        in_flight,
        m_sigma,
        t_sigma,
        max_dev,
    ));
    stack.into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(
    landed: usize,
    flying: usize,
    m_sigma: f64,
    t_sigma: f64,
    max_dev: f64,
) -> WidgetNode {
    let lines = [
        "GALTON · THE CENTRAL-LIMIT AXIS · THE BEAN MACHINE".to_string(),
        format!(
            "{BALLS} balls · {ROWS} peg rows · every path a deterministic bit stream from its index"
        ),
        format!(
            "landed {landed} · airborne {flying} · bins {} · p = ½, so σ_theory = √(n·p·(1−p)) = {t_sigma:.3}",
            ROWS + 1
        ),
        format!(
            "σ_measured {m_sigma:.3} ({:+.1}% vs theory) · worst bin deviation {max_dev:.1} balls",
            (m_sigma - t_sigma) / t_sigma * 100.0
        ),
        "the dashed curve is C(24,k)/2²⁴ — the theorem, walking into the bins".to_string(),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(760.0)
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
