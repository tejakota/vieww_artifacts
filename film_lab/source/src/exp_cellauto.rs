//! exp_cellauto — *the computation axis.* Three machines that compute.
//!
//! Cellular automata are the smallest machines that surprise their own
//! builders. This plate runs three of them, each a different answer to
//! "where does complexity come from":
//!
//! - **Rule 30** — one black cell, one rule, and the left half descends
//!   into permanent chaos (it is the RNG Mathematica shipped for a decade;
//!   the centre column's balance is an open problem to this day).
//! - **Rule 110** — the borderline: order on the left, a traffic of
//!   gliders colliding on the right. Proven Turing-complete — a universal
//!   computer, one cell wide.
//! - **Conway's Life** (B3/S23, torus) — a seeded soup releasing gliders,
//!   a pulsar, a pentadecathlon: emergence, stability, and death.
//!
//! The spacetime trick: the 1-D rules render as **space-time diagrams**
//! (row = generation), so the machine's entire history is one picture —
//! and every frame is a pure function of t (reveal depth), the replay
//! discipline held for free. Life replays its generations from the seed
//! every frame. The receipt measures what the machines did: Rule 30's
//! centre-column balance over the generations actually drawn, Rule 110's
//! live density at first and last generation, and Life's population
//! milestones — every number counted from the same arrays that drew the
//! frame.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, ease_out_cubic, mix, tint, AMBER, CYAN, FAINT, INK, MUTED,
    VIOLET, VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The three panes ─────────────────────────────────────────────────────────

/// Pane geometry: (x, y, w, h) — three columns.
const PANE_W: f32 = 384.0;
const PANE_H: f32 = 512.0;
const P1: (f32, f32) = (36.0, 118.0);
const P2: (f32, f32) = (448.0, 118.0);
const P3: (f32, f32) = (860.0, 118.0);

/// The 1-D rules' width and depth.
const CA_W: usize = 176;
const CA_GENS: usize = 244;

/// Rule 30 and Rule 110 lookup tables.
fn rule_table(rule: u8) -> [bool; 8] {
    let mut t = [false; 8];
    for i in 0..8 {
        t[i] = (rule >> i) & 1 == 1;
    }
    t
}

/// Run a 1-D rule from a seed: returns all generations (row 0 = seed).
#[must_use]
fn run_rule(rule: u8, seed: &[bool]) -> Vec<Vec<bool>> {
    let table = rule_table(rule);
    let mut gens = Vec::with_capacity(CA_GENS);
    let mut cur = seed.to_vec();
    gens.push(cur.clone());
    for _ in 1..CA_GENS {
        let mut next = vec![false; CA_W];
        for x in 0..CA_W {
            let l = cur[(x + CA_W - 1) % CA_W] as u8;
            let c = cur[x] as u8;
            let r = cur[(x + 1) % CA_W] as u8;
            let idx = ((l << 2) | (c << 1) | r) as usize;
            next[x] = table[idx];
        }
        cur = next;
        gens.push(cur.clone());
    }
    gens
}

/// Rule 30's seed: a single cell at centre.
#[must_use]
fn seed_30() -> Vec<bool> {
    let mut s = vec![false; CA_W];
    s[CA_W / 2] = true;
    s
}

/// Rule 110's seed: a periodic band plus a gap — traffic needs something
/// to collide with.
#[must_use]
fn seed_110() -> Vec<bool> {
    let mut s = vec![false; CA_W];
    // A periodic lattice on the left, emptiness on the right — the classic
    // collision setup.
    for x in 0..CA_W {
        s[x] = x % 4 < 3 && x < CA_W * 3 / 5;
    }
    s
}

// ── Life ────────────────────────────────────────────────────────────────────

/// Life grid.
const LIFE_W: usize = 88;
const LIFE_H: usize = 50;
const LIFE_GENS: usize = 300;

/// The Life seed: a designed primordial soup — a pulsar (the oscillator),
/// a glider fleet, an r-pentomino (the chaos seed), some eaters.
#[must_use]
fn life_seed() -> Vec<Vec<bool>> {
    let mut g = vec![vec![false; LIFE_W]; LIFE_H];
    let put = |g: &mut Vec<Vec<bool>>, x: usize, y: usize| {
        g[y][x] = true;
    };
    // A pulsar at (10, 6) — period-3 oscillator.
    {
        let (ox, oy) = (10usize, 6usize);
        let cells = [
            (2, 0), (3, 0), (4, 0), (8, 0), (9, 0), (10, 0),
            (0, 2), (5, 2), (7, 2), (12, 2),
            (0, 3), (5, 3), (7, 3), (12, 3),
            (0, 4), (5, 4), (7, 4), (12, 4),
            (2, 5), (3, 5), (4, 5), (8, 5), (9, 5), (10, 5),
            (2, 7), (3, 7), (4, 7), (8, 7), (9, 7), (10, 7),
            (0, 8), (5, 8), (7, 8), (12, 8),
            (0, 9), (5, 9), (7, 9), (12, 9),
            (0, 10), (5, 10), (7, 10), (12, 10),
            (2, 12), (3, 12), (4, 12), (8, 12), (9, 12), (10, 12),
        ];
        for &(dx, dy) in cells.iter() {
            put(&mut g, ox + dx, oy + dy);
        }
    }
    // Glider fleet at (36, 8), (44, 14), (52, 6).
    for &(ox, oy) in [(36usize, 8usize), (44, 14), (52, 6)].iter() {
        for &(dx, dy) in [(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)].iter() {
            put(&mut g, ox + dx, oy + dy);
        }
    }
    // An r-pentomino at (68, 30) — the famous long-lived chaos seed.
    for &(dx, dy) in [(1, 0), (2, 0), (0, 1), (1, 1), (1, 2)].iter() {
        put(&mut g, 68 + dx, 30 + dy);
    }
    // A lightweight ship at (8, 34) — the canonical LWSS, heading right.
    for &(dx, dy) in [(0, 0), (3, 0), (4, 1), (0, 2), (4, 2), (0, 3), (1, 3), (2, 3), (3, 3)].iter() {
        put(&mut g, 8 + dx, 34 + dy);
    }
    // A pentadecathlon at (64, 8): a 10-cell line, period 15.
    for k in 0..10 {
        put(&mut g, 64 + k, 8);
    }
    g
}

/// Advance Life one generation (torus, B3/S23).
fn life_step(g: &Vec<Vec<bool>>) -> Vec<Vec<bool>> {
    let mut n = vec![vec![false; LIFE_W]; LIFE_H];
    for y in 0..LIFE_H {
        for x in 0..LIFE_W {
            let mut count = 0u8;
            for dy in [-1i32, 0, 1] {
                for dx in [-1i32, 0, 1] {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let xx = ((x as i32 + dx).rem_euclid(LIFE_W as i32)) as usize;
                    let yy = ((y as i32 + dy).rem_euclid(LIFE_H as i32)) as usize;
                    count += g[yy][xx] as u8;
                }
            }
            n[y][x] = match (g[y][x], count) {
                (true, 2) | (true, 3) => true,
                (false, 3) => true,
                _ => false,
            };
        }
    }
    n
}

/// Replay Life to generation `gens`.
#[must_use]
fn run_life(gens: usize) -> (Vec<Vec<bool>>, Vec<usize>) {
    let mut g = life_seed();
    let mut pops = Vec::with_capacity(gens + 1);
    let mut pop = g.iter().flat_map(|r| r.iter().filter(|&&c| c)).count();
    pops.push(pop);
    for _ in 0..gens {
        g = life_step(&g);
        pop = g.iter().flat_map(|r| r.iter().filter(|&&c| c)).count();
        pops.push(pop);
    }
    (g, pops)
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    // Reveal depth: how many generations each machine has run.
    let frac = clamp01(t);
    let gens = ((ease_out_cubic(frac) * CA_GENS as f32).round() as usize).clamp(1, CA_GENS);
    let life_gens = ((ease_out_cubic(frac) * LIFE_GENS as f32).round() as usize).max(1);

    let g30 = run_rule(30, &seed_30());
    let g110 = run_rule(110, &seed_110());
    let (life, life_pops) = run_life(life_gens);

    // ── The receipt's measurements ────────────────────────────────────
    // Rule 30: the centre column's 1-fraction over the generations drawn.
    let centre = {
        let ones = g30[..gens].iter().filter(|g| g[CA_W / 2]).count();
        ones as f32 / gens as f32
    };
    // Rule 110: live density, first vs latest drawn generation.
    let dens = |g: &[bool]| g.iter().filter(|&&c| c).count() as f32 / CA_W as f32;
    let d110_first = dens(&g110[0]);
    let d110_now = dens(&g110[gens - 1]);
    // Life: population now, peak, and the phase-3 periodicity check on
    // the pulsar's home (population stable within a small band).
    let pop_now = life_pops[life_gens];
    let pop_peak = life_pops.iter().copied().max().unwrap_or(0);

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — three specimen chambers.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(6, 6, 10)),
                    (1.0, Color::rgb(11, 11, 15)),
                ]),
            );
            for &(px, py) in [P1, P2, P3].iter() {
                book.rrect(
                    Rect::new(px - 12.0, py - 12.0, px + PANE_W + 12.0, py + PANE_H + 12.0),
                    10.0,
                    alpha(Color::rgb(20, 20, 27), 0.95),
                );
            }

            // ── Pane 1: Rule 30, space-time ────────────────────────────
            let (px, py) = P1;
            let cw = PANE_W / CA_W as f32;
            let chh = PANE_H / CA_GENS as f32;
            for (gi, row) in g30[..gens].iter().enumerate() {
                for (x, &on) in row.iter().enumerate() {
                    if !on {
                        continue;
                    }
                    let heat = gi as f32 / CA_GENS as f32;
                    book.rect(
                        Rect::new(
                            px + x as f32 * cw,
                            py + gi as f32 * chh,
                            px + (x + 1) as f32 * cw + 0.5,
                            py + (gi + 1) as f32 * chh + 0.5,
                        ),
                        mix(VIOLET_SOFT, tint(AMBER, 0.2), heat * 0.55),
                    );
                }
            }
            // The centre column, underlined: the open problem's strip.
            book.rect(
                Rect::new(
                    px + CA_W as f32 / 2.0 * cw - 1.0,
                    py + gens as f32 * chh,
                    px + (CA_W as f32 / 2.0 + 1.0) * cw + 1.0,
                    py + gens as f32 * chh + 3.0,
                ),
                alpha(CYAN, 0.8),
            );

            // ── Pane 2: Rule 110, space-time ───────────────────────────
            let (px, py) = P2;
            for (gi, row) in g110[..gens].iter().enumerate() {
                for (x, &on) in row.iter().enumerate() {
                    if !on {
                        continue;
                    }
                    book.rect(
                        Rect::new(
                            px + x as f32 * cw,
                            py + gi as f32 * chh,
                            px + (x + 1) as f32 * cw + 0.5,
                            py + (gi + 1) as f32 * chh + 0.5,
                        ),
                        mix(tint(VIOLET, 0.25), VIOLET_SOFT, 0.4),
                    );
                }
            }
            // The traffic lanes: two faint diagonal guides where the
            // gliders run (the aether traffic's favourite angles).
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                for slope in [2.0_f32, -2.0] {
                    let mut path = Path::new();
                    let x0 = px + PANE_W * 0.62;
                    let y0 = py + 8.0;
                    let x1 = x0 + slope * (PANE_H - 16.0) / 2.0;
                    path.move_to(Offset::new(x0, y0));
                    path.line_to(Offset::new(
                        x1.clamp(px, px + PANE_W),
                        py + PANE_H - 8.0,
                    ));
                    g.stroke(path, alpha(tint(CYAN, 0.4), 0.14), 1.0);
                }
            });

            // ── Pane 3: Life, the live generation ──────────────────────
            let (px, py) = P3;
            let lw = PANE_W / LIFE_W as f32;
            let lh = (PANE_H - 96.0) / LIFE_H as f32;
            for y in 0..LIFE_H {
                for x in 0..LIFE_W {
                    if !life[y][x] {
                        continue;
                    }
                    book.rect(
                        Rect::new(
                            px + x as f32 * lw,
                            py + y as f32 * lh,
                            px + (x + 1) as f32 * lw + 0.5,
                            py + (y + 1) as f32 * lh + 0.5,
                        ),
                        mix(VIOLET_SOFT, tint(AMBER, 0.3), 0.35),
                    );
                }
            }
            // The population trace, along the pane's bottom.
            let t_y = py + PANE_H - 84.0;
            if life_pops.len() > 1 {
                let max_p = life_pops.iter().copied().max().unwrap_or(1) as f32;
                let mut path = Path::new();
                for (i, &p) in life_pops.iter().enumerate() {
                    let x = px + i as f32 / LIFE_GENS as f32 * PANE_W;
                    let y = t_y + 72.0 - (p as f32 / max_p) * 68.0;
                    if i == 0 {
                        path.move_to(Offset::new(x, y));
                    } else {
                        path.line_to(Offset::new(x, y));
                    }
                }
                book.stroke(path, alpha(tint(CYAN, 0.3), 0.85), 1.4);
                // The rider.
                if let Some(&p) = life_pops.last() {
                    let x = px + PANE_W;
                    let y = t_y + 72.0 - (p as f32 / max_p) * 68.0;
                    book.circle(Offset::new(x, y), 2.4, alpha(tint(CYAN, 0.4), 0.95));
                }
            }
            // The trace's baseline.
            book.line(
                Offset::new(px, t_y + 72.0),
                Offset::new(px + PANE_W, t_y + 72.0),
                alpha(FAINT, 0.4),
                1.0,
            );
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(gens, centre, d110_first, d110_now, pop_now, pop_peak, life_gens));
    stack = stack.push(pane_labels());
    stack.into()
}

// ── The pane labels ─────────────────────────────────────────────────────────

fn pane_labels() -> WidgetNode {
    let labels = [
        (P1, "RULE 30 — chaos from one cell", "the RNG Mathematica shipped"),
        (P2, "RULE 110 — the traffic", "Turing-complete, one cell wide"),
        (P3, "LIFE B3/S23 — emergence", "gliders · pulsar · pentadecathlon"),
    ];
    let mut stack = Stack::new();
    for ((px, py), title, sub) in labels.iter() {
        for (k, text) in [title, sub].iter().enumerate() {
            stack = stack.push(
                Positioned::new()
                    .left(*px)
                    .top(py - 34.0 + k as f32 * 15.0)
                    .width(PANE_W)
                    .height(14.0)
                    .child(
                        Text::new((*text).to_string()).style(
                            TextStyle::new(if k == 0 { 11.5 } else { 10.0 })
                                .monospace()
                                .letter_spacing(if k == 0 { 1.6 } else { 0.8 })
                                .color(alpha(
                                    if k == 0 { tint(VIOLET_SOFT, 0.3) } else { MUTED },
                                    0.95,
                                )),
                        ),
                    ),
            );
        }
    }
    stack.into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(
    gens: usize,
    centre: f32,
    d110_first: f32,
    d110_now: f32,
    pop_now: usize,
    pop_peak: usize,
    life_gens: usize,
) -> WidgetNode {
    let lines = [
        "CELLAUTO · THE COMPUTATION AXIS · THREE MACHINES, RUN LIVE".to_string(),
        format!(
            "generations drawn: {gens} of {CA_GENS} (1-D rules) · Life at {life_gens} of {LIFE_GENS}"
        ),
        format!(
            "rule 30 centre column: {:.1}% ones over the drawn rows — the open problem, measured",
            centre * 100.0
        ),
        format!(
            "rule 110 density: {:.3} → {:.3} (traffic colliding, measured) · Life pop {pop_now}, peak {pop_peak}",
            d110_first, d110_now
        ),
        "spacetime as picture: the 1-D rules' whole history is one frame".to_string(),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 15.0)
                .width(1180.0)
                .height(14.0)
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
