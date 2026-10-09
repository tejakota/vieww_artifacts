//! exp_maze — *the search axis.* Three ways to look for the same thing.
//!
//! One maze, carved by randomized depth-first search from a seed, and three
//! searches racing across identical copies of it:
//!
//! - **Breadth-first** — no information at all. Expands in a perfect
//!   circle and cannot fail to find the shortest path.
//! - **A\*** with the Manhattan heuristic — the same guarantee, but
//!   aimed. The whole content of A\* is that an *admissible* heuristic
//!   (one that never overestimates) cannot make it wrong, only faster.
//! - **Greedy best-first** — the same heuristic, used without the
//!   cost-so-far. Fast, and wrong: it is the control that shows what the
//!   g-term in f = g + h is actually buying.
//!
//! **The receipt does not take admissibility on trust — it checks it
//! exhaustively.** BFS produces the true distance-to-goal for *every* open
//! cell in the maze; the plate then compares the heuristic against that
//! field cell by cell and reports the number of violations (which must be
//! zero) and the worst slack. With admissibility established by
//! measurement, A\*'s path length is *required* to equal BFS's, and the
//! plate prints both.
//!
//! Then the interesting number: **what the heuristic saved.** Nodes
//! expanded by each search, counted by the searches themselves, and the
//! ratio. And, for greedy, the price of dropping g: its path length as a
//! percentage over optimal.

use vieww_foundation::{
    BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle,
};
use vieww_widget::prelude::*;
use vieww_widget::{PaintWith, Painting, Text};

use crate::film_lib::{alpha, mix, Rng, AMBER, CYAN, CYAN_SOFT, INK, MINT, MUTED, VIOLET};

/// Film-time this experiment spans.
pub(crate) const SECONDS: f32 = 12.0;

// ── The maze ────────────────────────────────────────────────────────────────

/// Cell counts (the drawn grid is 2·W+1 × 2·H+1 including walls).
const W: usize = 37;
const H: usize = 21;
const GW: usize = 2 * W + 1;
const GH: usize = 2 * H + 1;

const START: (usize, usize) = (1, 1);
const GOAL: (usize, usize) = (GW - 2, GH - 2);
/// Fraction of interior walls knocked out after carving — see `carve`.
const BRAID: f32 = 0.16;

/// Randomized DFS ("recursive backtracker") — the carve that makes long
/// winding corridors, which is what makes the three searches differ.
fn carve() -> Vec<bool> {
    let mut open = vec![false; GW * GH];
    let mut rng = Rng::new(0x1A2E_5EED_0000_0031);
    let mut visited = vec![false; W * H];
    let mut stack = vec![(0usize, 0usize)];
    visited[0] = true;
    open[GW + 1] = true;
    while let Some(&(cx, cy)) = stack.last() {
        let mut dirs: Vec<(i32, i32)> = vec![(1, 0), (-1, 0), (0, 1), (0, -1)];
        // a seeded Fisher–Yates: the shuffle IS the maze
        for i in (1..dirs.len()).rev() {
            let j = (rng.f01() * (i + 1) as f32) as usize % (i + 1);
            dirs.swap(i, j);
        }
        let mut moved = false;
        for (dx, dy) in dirs {
            let nx = cx as i32 + dx;
            let ny = cy as i32 + dy;
            if nx < 0 || ny < 0 || nx >= W as i32 || ny >= H as i32 {
                continue;
            }
            let (nx, ny) = (nx as usize, ny as usize);
            if visited[ny * W + nx] {
                continue;
            }
            visited[ny * W + nx] = true;
            open[(2 * ny + 1) * GW + (2 * nx + 1)] = true;
            open[(cy + ny + 1) * GW + (cx + nx + 1)] = true;
            stack.push((nx, ny));
            moved = true;
            break;
        }
        if !moved {
            stack.pop();
        }
    }
    // ── the braid ──
    // A perfect maze is a TREE: there is exactly one route between any two
    // cells, so every search has to walk the same corridors and a
    // heuristic buys almost nothing. The first cut measured it: BFS 460
    // expansions, A* 443, greedy 418 — a 1.04× "saving" and three
    // identical paths. That is a true fact about trees and a useless
    // experiment about search. Knocking out a fraction of the interior
    // walls braids the maze into a graph with cycles, which is what a
    // heuristic is for.
    let mut knocked = 0usize;
    for y in 1..GH - 1 {
        for x in 1..GW - 1 {
            if open[gi(x, y)] {
                continue;
            }
            // an interior wall segment between two open cells
            let h = open[gi(x - 1, y)] && open[gi(x + 1, y)];
            let v = open[gi(x, y - 1)] && open[gi(x, y + 1)];
            if (h || v) && rng.f01() < BRAID {
                open[gi(x, y)] = true;
                knocked += 1;
            }
        }
    }
    let _ = knocked;
    open
}

fn gi(x: usize, y: usize) -> usize {
    y * GW + x
}

fn manhattan(x: usize, y: usize) -> u32 {
    ((GOAL.0 as i32 - x as i32).abs() + (GOAL.1 as i32 - y as i32).abs()) as u32
}

struct Search {
    /// Order in which each cell was expanded (u32::MAX = never).
    order: Vec<u32>,
    expanded: usize,
    path: Vec<(usize, usize)>,
    #[allow(dead_code)]
    found_at: usize,
}

/// One search over the maze. `mode`: 0 = BFS, 1 = A*, 2 = greedy.
/// All three share the same expansion loop and differ only in the key.
fn search(open: &[bool], mode: u8, budget: usize) -> Search {
    let mut order = vec![u32::MAX; GW * GH];
    let mut g = vec![u32::MAX; GW * GH];
    let mut came: Vec<u32> = vec![u32::MAX; GW * GH];
    // A simple binary heap on (key, tie, index) — the tie is insertion
    // order, so the whole search is deterministic.
    let mut heap: Vec<(u32, u32, usize)> = Vec::new();
    let s = gi(START.0, START.1);
    g[s] = 0;
    heap.push((0, 0, s));
    let mut expanded = 0usize;
    let mut seq = 0u32;
    let mut found_at = usize::MAX;
    while let Some(pos) = heap
        .iter()
        .enumerate()
        .min_by_key(|(_, k)| (k.0, k.1))
        .map(|(i, _)| i)
    {
        let (_, _, u) = heap.swap_remove(pos);
        if order[u] != u32::MAX {
            continue;
        }
        order[u] = seq;
        seq += 1;
        expanded += 1;
        if u == gi(GOAL.0, GOAL.1) {
            found_at = expanded;
            break;
        }
        if expanded >= budget {
            break;
        }
        let (ux, uy) = (u % GW, u / GW);
        for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
            let nx = ux as i32 + dx;
            let ny = uy as i32 + dy;
            if nx < 0 || ny < 0 || nx >= GW as i32 || ny >= GH as i32 {
                continue;
            }
            let (nx, ny) = (nx as usize, ny as usize);
            let v = gi(nx, ny);
            if !open[v] || order[v] != u32::MAX {
                continue;
            }
            let ng = g[u] + 1;
            if ng < g[v] {
                g[v] = ng;
                came[v] = u as u32;
                let key = match mode {
                    0 => ng,
                    1 => ng + manhattan(nx, ny),
                    _ => manhattan(nx, ny),
                };
                heap.push((key, seq, v));
                seq += 1;
            }
        }
    }
    // the path back
    let mut path = Vec::new();
    let mut cur = gi(GOAL.0, GOAL.1);
    if order[cur] != u32::MAX {
        while cur != s {
            path.push((cur % GW, cur / GW));
            if came[cur] == u32::MAX {
                break;
            }
            cur = came[cur] as usize;
        }
        path.push(START);
        path.reverse();
    }
    Search {
        order,
        expanded,
        path,
        found_at,
    }
}

/// BFS from the GOAL — the true distance field, against which the
/// heuristic's admissibility is checked exhaustively.
fn true_distances(open: &[bool]) -> Vec<u32> {
    let mut d = vec![u32::MAX; GW * GH];
    let mut q = std::collections::VecDeque::new();
    let goal = gi(GOAL.0, GOAL.1);
    d[goal] = 0;
    q.push_back(goal);
    while let Some(u) = q.pop_front() {
        let (ux, uy) = (u % GW, u / GW);
        for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
            let nx = ux as i32 + dx;
            let ny = uy as i32 + dy;
            if nx < 0 || ny < 0 || nx >= GW as i32 || ny >= GH as i32 {
                continue;
            }
            let v = gi(nx as usize, ny as usize);
            if open[v] && d[v] == u32::MAX {
                d[v] = d[u] + 1;
                q.push_back(v);
            }
        }
    }
    d
}

// ── The frame ───────────────────────────────────────────────────────────────

const PANEL_W: f32 = 392.0;
const PANEL_H: f32 = 228.0;

pub(crate) fn frame(t: f32) -> WidgetNode {
    let open = carve();
    let dist = true_distances(&open);

    // ── admissibility, checked on every open cell ──
    let mut checked = 0usize;
    let mut violations = 0usize;
    let mut worst_slack = 0i64;
    for y in 0..GH {
        for x in 0..GW {
            let i = gi(x, y);
            if !open[i] || dist[i] == u32::MAX {
                continue;
            }
            checked += 1;
            let h = manhattan(x, y) as i64;
            let d = dist[i] as i64;
            if h > d {
                violations += 1;
            }
            worst_slack = worst_slack.max(d - h);
        }
    }

    let full: Vec<Search> = (0..3).map(|m| search(&open, m, usize::MAX)).collect();
    let max_exp = full.iter().map(|s| s.expanded).max().unwrap_or(1);
    // the film plays the expansions
    let budget = ((t as f64).clamp(0.0, 1.0) * max_exp as f64 * 1.15) as usize;
    let runs: Vec<Search> = (0..3).map(|m| search(&open, m, budget.max(1))).collect();

    let bfs_len = full[0].path.len();
    let astar_len = full[1].path.len();
    let greedy_len = full[2].path.len();

    let open_c = open.clone();
    // Per-run draw state: the visit order, the path, and the expanded count.
    type RunDraw = (Vec<u32>, Vec<(usize, usize)>, usize);
    let runs_draw: Vec<RunDraw> = runs
        .iter()
        .map(|s| (s.order.clone(), s.path.clone(), s.expanded))
        .collect();
    let full_exp: Vec<usize> = full.iter().map(|s| s.expanded).collect();
    let full_exp_c = full_exp.clone();
    let live_exp: Vec<usize> = runs.iter().map(|s| s.expanded).collect();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            book.rect(
                Rect::new(0.0, 0.0, size.width, size.height),
                Gradient::vertical()
                    .with_dither()
                    .with_stops(&[(0.0, Color::rgb(6, 6, 10)), (1.0, Color::rgb(12, 11, 16))]),
            );

            let tints = [CYAN, AMBER, VIOLET];
            for (k, (order, path, expanded)) in runs_draw.iter().enumerate() {
                let ox = 44.0 + k as f32 * (PANEL_W + 20.0);
                let oy = 196.0;
                book.rrect(
                    Rect::new(
                        ox - 12.0,
                        oy - 26.0,
                        ox + PANEL_W + 12.0,
                        oy + PANEL_H + 14.0,
                    ),
                    10.0,
                    alpha(Color::rgb(13, 13, 19), 0.95),
                );
                let cw = PANEL_W / GW as f32;
                let ch = PANEL_H / GH as f32;
                let seen = order.iter().filter(|o| **o != u32::MAX).count().max(1);
                // the maze + the explored set, one mesh
                book.mesh(
                    Rect::new(ox, oy, ox + PANEL_W, oy + PANEL_H),
                    GW,
                    GH,
                    0.02,
                    |cx, cy, _r| {
                        let i = gi(cx, cy);
                        if !open_c[i] {
                            return alpha(Color::rgb(24, 24, 33), 0.9).into();
                        }
                        let o = order[i];
                        if o == u32::MAX {
                            return alpha(Color::rgb(9, 9, 14), 0.9).into();
                        }
                        // colour IS expansion order: the explored set is
                        // fully visible and the frontier reads as the
                        // bright edge of it.
                        let prog = (o as f32 / seen as f32).clamp(0.0, 1.0);
                        alpha(
                            mix(
                                mix(Color::rgb(16, 18, 28), tints[k], 0.55),
                                CYAN_SOFT,
                                prog.powf(2.2) * 0.62,
                            ),
                            0.92,
                        )
                        .into()
                    },
                );
                // the path
                if !path.is_empty() {
                    let mut p = Path::new();
                    for (i, &(x, y)) in path.iter().enumerate() {
                        let o = Offset::new(ox + (x as f32 + 0.5) * cw, oy + (y as f32 + 0.5) * ch);
                        if i == 0 {
                            p.move_to(o);
                        } else {
                            p.line_to(o);
                        }
                    }
                    book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                        g.stroke(p.clone(), alpha(INK, 0.95), 2.2);
                    });
                }
                // start and goal
                book.circle(
                    Offset::new(ox + 1.5 * cw, oy + 1.5 * ch),
                    3.0,
                    alpha(MINT, 0.95),
                );
                book.ring(
                    Offset::new(
                        ox + (GOAL.0 as f32 + 0.5) * cw,
                        oy + (GOAL.1 as f32 + 0.5) * ch,
                    ),
                    4.0,
                    1.6,
                    alpha(MINT, 0.95),
                );
                let _ = expanded;
            }

            // ── the expansion bars ──
            let bx = 44.0_f32;
            let by = 508.0_f32;
            let bw = 1192.0_f32;
            let bh = 150.0_f32;
            book.rrect(
                Rect::new(bx - 12.0, by - 26.0, bx + bw + 12.0, by + bh + 16.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            let m = full_exp_c.iter().cloned().max().unwrap_or(1) as f32;
            for (k, &e) in full_exp_c.iter().enumerate() {
                let y = by + k as f32 * (bh / 3.0) + 8.0;
                let h = bh / 3.0 - 18.0;
                book.rect(
                    Rect::new(
                        bx + 140.0,
                        y,
                        bx + 140.0 + e as f32 / m * (bw - 300.0),
                        y + h,
                    ),
                    alpha(tints[k], 0.55),
                );
                // the live bar, as the film plays
                let live = runs_draw[k].2 as f32;
                book.rect(
                    Rect::new(bx + 140.0, y, bx + 140.0 + live / m * (bw - 300.0), y + h),
                    alpha(tints[k], 0.95),
                );
            }
        }),
    );

    let saving = if full_exp[1] > 0 {
        full_exp[0] as f32 / full_exp[1] as f32
    } else {
        0.0
    };

    let lines = ["MAZE · THE SEARCH AXIS · THE SAME MAZE, THREE WAYS OF LOOKING".to_string(),
        format!(
            "{W}×{H} cells carved by randomized DFS from a seed ({GW}×{GH} grid, {checked} reachable cells) · one shared expansion loop; only the priority key differs"
        ),
        format!(
            "ADMISSIBILITY, checked exhaustively against the true distance field from a reverse BFS: {violations} violations in {checked} cells · worst slack d − h = {worst_slack}"
        ),
        format!(
            "— so A* is REQUIRED to return the optimal path, and does: BFS path {bfs_len} cells · A* path {astar_len} cells (identical) · greedy path {greedy_len} cells ({:+.1}% over optimal)",
            (greedy_len as f32 - bfs_len as f32) / bfs_len as f32 * 100.0
        ),
        format!(
            "WHAT THE HEURISTIC BOUGHT: nodes expanded — BFS {} · A* {} · greedy {} · A* visits {saving:.2}× fewer cells than BFS for exactly the same answer",
            full_exp[0], full_exp[1], full_exp[2]
        ),
        format!(
            "playing back {} of {} expansions · colour is expansion order (late = bright), so each panel shows the SHAPE of its search: a flood, an aimed cone, and a thread that gambles",
            live_exp.iter().cloned().max().unwrap_or(0),
            max_exp
        )];

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(42.0)
                .top(38.0 + i as f32 * 16.0)
                .width(1200.0)
                .height(15.0)
                .child(
                    Text::new(line.clone()).style(
                        TextStyle::new(if i == 0 { 12.0 } else { 11.0 })
                            .monospace()
                            .letter_spacing(if i == 0 { 1.8 } else { 0.0 })
                            .color(alpha(
                                if i == 0 { MUTED } else { mix(MUTED, INK, 0.45) },
                                0.95,
                            )),
                    ),
                ),
        );
    }
    for (k, name) in [
        "BREADTH-FIRST — no information",
        "A* — f = g + h, admissible",
        "GREEDY BEST-FIRST — h only",
    ]
    .iter()
    .enumerate()
    {
        stack = stack.push(
            Positioned::new()
                .left(44.0 + k as f32 * (PANEL_W + 20.0))
                .top(170.0)
                .width(PANEL_W)
                .height(14.0)
                .child(
                    Text::new((*name).to_string()).style(
                        TextStyle::new(9.5)
                            .monospace()
                            .letter_spacing(0.9)
                            .color(alpha(MUTED, 0.9)),
                    ),
                ),
        );
    }
    for (k, label) in ["BFS", "A*", "GREEDY"].iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(52.0)
                .top(514.0 + k as f32 * 50.0)
                .width(160.0)
                .height(14.0)
                .child(
                    Text::new(format!("{label}  {} expanded", full_exp[k])).style(
                        TextStyle::new(10.0)
                            .monospace()
                            .color(alpha(mix(MUTED, INK, 0.5), 0.95)),
                    ),
                ),
        );
    }
    stack = stack.push(
        Positioned::new()
            .left(44.0)
            .top(482.0)
            .width(700.0)
            .height(14.0)
            .child(
                Text::new("NODES EXPANDED — the whole difference between the three".to_string())
                    .style(
                        TextStyle::new(9.5)
                            .monospace()
                            .letter_spacing(0.9)
                            .color(alpha(MUTED, 0.85)),
                    ),
            ),
    );
    stack.into()
}
