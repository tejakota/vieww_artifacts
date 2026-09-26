//! exp_ant — *the emergence axis.* Four rules, then a highway.
//!
//! Langton's ant, 1986: a cell flips colour where it stands, turns left or
//! right by which colour it found, steps one. That is the whole machine —
//! and for the first ten thousand steps it makes symmetric chaos, then,
//! with no signal anyone can point to, it locks into a periodic diagonal
//! **highway of 104 steps** and builds it forever. Emergence with a
//! capital E: the highway is in no rule, yet it is what the rules are for.
//!
//! This plate replays 14,000 steps from the empty board every frame (the
//! ant's world is a pure function of the step count), the trail drawn as
//! ink that brightens with recency, the ant itself a hot amber dot. The
//! receipt measures the machine's own biography from the same replay:
//! **the step of the last fresh cell ever visited (the end of exploring,
//! the birth of the highway), the highway's period found by the smallest
//! shift P whose positions repeat for a long run — the machine's own
//! cycle, counted, not asserted — and the drift per period.** The
//! exploration timeline collapses to zero in the corner: the signature
//! of order nobody designed.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook,
    TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, mix, AMBER, CYAN, INK, MUTED, VIOLET};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The machine ─────────────────────────────────────────────────────────────

/// The board (torus).
const N: usize = 384;

/// Total steps the film replays.
const STEPS: usize = 14000;

/// One replay to step k. Returns (board, last-visit step per cell, ant
/// position and direction at step k, fresh-cell timeline per 250 steps).
fn replay(k: usize) -> (Vec<u8>, Vec<i32>, (usize, usize, u8), Vec<u32>) {
    let mut cells = vec![0u8; N * N];
    let mut last_visit = vec![i32::MIN; N * N];
    let mut fresh = vec![0u32; (STEPS / 250) + 2];
    let (mut x, mut y) = (N / 2, N / 2);
    // direction: 0 up, 1 right, 2 down, 3 left — turns encoded per rule
    let mut d: u8 = 0;
    for i in 0..k.min(STEPS) {
        let idx = y * N + x;
        // flip the cell, turn left on white (0), right on black (1)
        if cells[idx] == 0 {
            cells[idx] = 1;
            d = (d + 3) % 4;
        } else {
            cells[idx] = 0;
            d = (d + 1) % 4;
        }
        // step (torus)
        match d {
            0 => y = (y + N - 1) % N,
            1 => x = (x + 1) % N,
            2 => y = (y + 1) % N,
            _ => x = (x + N - 1) % N,
        }
        let nidx = y * N + x;
        if last_visit[nidx] == i32::MIN {
            fresh[i / 250] += 1;
        }
        last_visit[nidx] = i as i32;
    }
    (cells, last_visit, (x, y, d), fresh)
}

/// The machine's biography, computed once: (last fresh step, highway
/// period, drift per period, fresh-per-250-steps series, coverage at end).
fn biography() -> (usize, usize, (i32, i32), Vec<u32>, usize) {
    // full replay, keeping the position trace
    let mut cells = vec![0u8; N * N];
    let mut visited = vec![false; N * N];
    let mut fresh: Vec<u32> = vec![0; (STEPS / 250) + 2];
    let mut trace: Vec<(usize, usize)> = Vec::with_capacity(STEPS);
    let (mut x, mut y) = (N / 2, N / 2);
    let mut d: u8 = 0;
    let mut last_fresh = 0usize;
    for i in 0..STEPS {
        let idx = y * N + x;
        if cells[idx] == 0 {
            cells[idx] = 1;
            d = (d + 3) % 4;
        } else {
            cells[idx] = 0;
            d = (d + 1) % 4;
        }
        match d {
            0 => y = (y + N - 1) % N,
            1 => x = (x + 1) % N,
            2 => y = (y + 1) % N,
            _ => x = (x + N - 1) % N,
        }
        let nidx = y * N + x;
        if !visited[nidx] {
            visited[nidx] = true;
            fresh[i / 250] += 1;
            last_fresh = i;
        }
        trace.push((x, y));
    }
    // HIGHWAY PERIOD — the displacement test. A highway MOVES: the ant is
    // back at the same phase every P steps, displaced by a constant vector.
    // Position-equality can never hold for a moving machine — the first cut
    // tested exactly that and its receipt honestly printed period 0 (a bug
    // caught by its own instrument). The test here: the smallest P whose
    // per-period displacement is the SAME vector across a 400-step run that
    // reaches the end.
    let mut period = 0usize;
    let mut drift = (0i32, 0i32);
    'outer: for p in 2..400usize {
        let tail_start = STEPS - 400;
        let v = (
            trace[STEPS - 1].0 as i32 - trace[STEPS - 1 - p].0 as i32,
            trace[STEPS - 1].1 as i32 - trace[STEPS - 1 - p].1 as i32,
        );
        let ok = (tail_start..STEPS - 1).all(|n| {
            (
                trace[n + 1].0 as i32 - trace[n + 1 - p].0 as i32,
                trace[n + 1].1 as i32 - trace[n + 1 - p].1 as i32,
            ) == v
        });
        if ok {
            period = p;
            drift = v;
            break 'outer;
        }
    }
    let coverage = visited.iter().filter(|&&v| v).count();
    (last_fresh, period, drift, fresh, coverage)
}

// ── The frame ───────────────────────────────────────────────────────────────

const X0: f32 = 128.0;
const Y0: f32 = 140.0;
const CELL: f32 = 1.16;

pub fn frame(t: f32) -> WidgetNode {
    let k = (t * STEPS as f32).round() as usize;
    let (cells, last_visit, (ax, ay, ad), fresh) = replay(k);
    let (last_fresh, period, drift, fresh_series, coverage) = biography();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — graph paper in the dark.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(6, 6, 10)),
                    (1.0, Color::rgb(11, 11, 16)),
                ]),
            );
            book.rrect(
                Rect::new(X0 - 20.0, Y0 - 20.0, X0 + N as f32 * CELL + 20.0,
                          Y0 + N as f32 * CELL + 20.0),
                12.0,
                alpha(Color::rgb(12, 12, 18), 0.96),
            );

            // ── The trail: one rect per lit cell, brightness = recency ──
            // Only lit cells draw — the ant's ink IS its history.
            let now = k as i32;
            for y in 0..N {
                for x in 0..N {
                    let idx = y * N + x;
                    if cells[idx] == 0 {
                        continue;
                    }
                    // recency: visited within the last 600 steps is hot
                    let age = (now - last_visit[idx]).max(0);
                    let heat = (1.0 - (age as f32 / 600.0).min(1.0)).max(0.0);
                    let col = if heat > 0.55 {
                        mix(VIOLET, CYAN, heat)
                    } else if heat > 0.12 {
                        mix(Color::rgb(34, 30, 52), VIOLET, heat * 2.4)
                    } else {
                        Color::rgb(34, 30, 52)
                    };
                    book.rect(
                        Rect::new(
                            X0 + x as f32 * CELL,
                            Y0 + y as f32 * CELL,
                            X0 + (x + 1) as f32 * CELL + 0.35,
                            Y0 + (y + 1) as f32 * CELL + 0.35,
                        ),
                        col,
                    );
                }
            }

            // ── The ant: a hot dot with its heading tick ──
            let (axf, ayf) = (X0 + ax as f32 * CELL + CELL / 2.0, Y0 + ay as f32 * CELL + CELL / 2.0);
            book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                g.ring(Offset::new(axf, ayf), 9.0, 7.0, alpha(AMBER, 0.30));
            });
            book.circle(Offset::new(axf, ayf), 3.4, AMBER);
            let (dx, dy) = match ad {
                0 => (0.0, -6.5),
                1 => (6.5, 0.0),
                2 => (0.0, 6.5),
                _ => (-6.5, 0.0),
            };
            book.line(
                Offset::new(axf, ayf),
                Offset::new(axf + dx as f32, ayf + dy as f32),
                alpha(Color::rgb(255, 232, 190), 0.95),
                1.6,
            );

            // ── The exploration timeline: fresh cells per 250 steps ──
            let px0 = 990.0;
            let py0 = 170.0;
            let pw = 250.0;
            let ph = 160.0;
            book.rrect(
                Rect::new(px0 - 16.0, py0 - 24.0, px0 + pw + 16.0, py0 + ph + 30.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.94),
            );
            let peak = fresh_series.iter().copied().max().unwrap_or(1) as f32;
            let n_bars = fresh.len().min(fresh_series.len());
            let bw = pw / n_bars as f32;
            for (i, &c) in fresh_series[..n_bars].iter().enumerate() {
                let bh = (c as f32 / peak * (ph - 14.0)).max(if c > 0 { 1.5 } else { 0.0 });
                book.rrect(
                    Rect::new(
                        px0 + i as f32 * bw,
                        py0 + ph - bh,
                        px0 + (i + 1) as f32 * bw + 0.4,
                        py0 + ph,
                    ),
                    1.0,
                    alpha(VIOLET, 0.85),
                );
            }
            // the highway onset: an amber hairline where exploring ends
            let onset_x = px0 + (last_fresh as f32 / STEPS as f32) * pw;
            book.line(
                Offset::new(onset_x, py0 - 12.0),
                Offset::new(onset_x, py0 + ph),
                alpha(AMBER, 0.8),
                1.2,
            );

            // ── The drift rose, below: the highway's direction ──
            let (rx, ry) = (1110.0, 470.0);
            if period > 0 {
                book.ring(Offset::new(rx, ry), 34.0, 1.2, alpha(MUTED, 0.6));
                let ang = (drift.1 as f32).atan2(drift.0 as f32);
                let len = ((drift.0 as f32).hypot(drift.1 as f32) / (period as f32 / 104.0).max(1.0) * 26.0).min(40.0);
                book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                    g.line(
                        Offset::new(rx, ry),
                        Offset::new(rx + ang.cos() * len, ry + ang.sin() * len),
                        alpha(CYAN, 0.85),
                        2.6,
                    );
                });
                book.circle(Offset::new(rx, ry), 2.6, CYAN);
            }
            let _ = coverage;
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(k, last_fresh, period, drift, coverage));
    stack.into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(
    k: usize,
    last_fresh: usize,
    period: usize,
    drift: (i32, i32),
    coverage: usize,
) -> WidgetNode {
    let lines = [
        "ANT · THE EMERGENCE AXIS · LANGTON, 1986".to_string(),
        format!(
            "flip the cell, turn by its colour, step one · {N}×{N} board · {STEPS} steps replayed from empty every frame"
        ),
        format!(
            "step {k} · cells ever visited {coverage} ({:.1} frac) · ink brightness = visit recency",
            coverage as f64 / (N * N) as f64
        ),
        format!(
            "EXPLORATION ENDS at step {last_fresh} — the last fresh cell (amber line) · then the highway, forever"
        ),
        format!(
            "HIGHWAY PERIOD: {period} steps · constant displacement ({}, {}) per period — the machine’s own cycle, counted",
            drift.0, drift.1
        ),
        "the highway is in none of the rules — it is what the rules are for".to_string(),
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
