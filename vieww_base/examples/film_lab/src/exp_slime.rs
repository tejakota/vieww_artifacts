//! exp_slime — *the transport axis.* The network nobody designed.
//!
//! In 2010 Tero, Takagi, Saigusa, Ito, Bebber, Fricker, Yumiki, Kobayashi
//! and Nakagaki put oat flakes on a wet dish in the pattern of the towns
//! around Tokyo and let *Physarum polycephalum* — one cell, no brain, no
//! blueprint — grow across it. In twenty-six hours it built the Tokyo rail
//! network. The paper's second half is the part that matters to a machine:
//! they wrote down what the organism is *doing*, and it is four lines of
//! arithmetic.
//!
//! **The model, exactly as the paper states it.** The plasmodium is a graph
//! of tubes. Each tube has a length L and a conductivity D. Protoplasm flows
//! through it by Poiseuille's law, Q = D·(p_i − p_j)/L, with the pressures p
//! fixed by Kirchhoff's current law at every node. Then the one adaptive
//! rule — the whole of the organism's intelligence:
//!
//! ```text
//!     dD/dt = f(|Q|) − D,     f(Q) = Q^γ / (1 + Q^γ)
//! ```
//!
//! *Tubes that carry flow thicken; tubes that do not, vanish.* Nothing else.
//! No search, no cost function, no shortest-path algorithm anywhere in this
//! file. Sources and sinks cycle through the twelve food nodes, and the
//! network is what is left when the arithmetic stops changing it.
//!
//! **The receipt closes four books, all from the grown network's own
//! arrays:**
//! - **Kirchhoff's law**, as a residual: the largest |ΣQ| at any interior
//!   node, which says whether the flow solve converged or merely finished.
//! - **TL/MST** — total tube length against the minimum spanning tree over
//!   the same twelve sources. The MST is the cheapest network that connects
//!   them and the worst one to live in; Tero's organism sits a few percent
//!   above it. This one measures its own.
//! - **Fault tolerance**, measured by cutting: every surviving tube is
//!   severed in turn and the sources re-flooded. An MST fails every such
//!   cut by construction. The number printed is what this network survives.
//! - **The pruning**, counted: how many of the lattice's tubes are left, and
//!   how much of the total conductivity the thickest twentieth now holds.
//!
//! **The incident, logged.** This plate was first built as Jones's *agent*
//! model — 12,000 particles with three sensors each, the version that makes
//! the desktop wallpapers. Five parameter sets in, it had produced three
//! different beautiful things and not one network: a vertical column (food
//! *added* to rather than *held at* a level, so twelve flakes became twelve
//! runaway suns), a pair of self-reinforcing ropes that never found a flake,
//! and finally a handsome reticulated vein system that ignored the food
//! entirely. The connectivity receipt said so every time — 3/12, 1/12, 6/12,
//! 1/12 — while every one of those frames would have passed an audit by eye.
//! The agent model's trail feedback is winner-take-all at any density this
//! bench can afford. The Tokyo paper's *other* model does not have that
//! failure mode, because the flow it adapts to is a solved conservation law
//! rather than an emergent stampede. **The rewrite is the finding.**

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{PaintWith, Painting, Text};

use crate::film_lib::{alpha, mix, Rng, AMBER, CYAN, CYAN_SOFT, INK, MINT, MUTED, VIOLET};

/// Film-time this experiment spans.
pub(crate) const SECONDS: f32 = 13.0;

// ── The lattice ─────────────────────────────────────────────────────────────

const COLS: usize = 33;
const ROWS: usize = 19;
const NODES: usize = COLS * ROWS;

/// Tero's exponent. γ > 1 makes the rule bistable — a tube either wins its
/// traffic or dies — and γ = 1 gives a network that never prunes.
const GAMMA: f32 = 1.8;
const DT: f32 = 0.12;
const I0: f32 = 2.0;
/// Seven complete cycles of the 66 town pairs. A step count that is not a
/// multiple of 66 leaves the last partial cycle's pairs over-served, which
/// is a bias with no physical meaning; 7 × 66 removes it.
const STEPS: usize = 462;
/// Below this a tube is dead — not drawn, not measured, not walked.
const ALIVE: f32 = 0.035;

/// The twelve food nodes, as lattice coordinates: a ring, a centre, and
/// three off-ring towns, so the network has real choices about detours.
const FOOD: [(usize, usize); 12] = [
    (16, 9),
    (4, 3),
    (16, 2),
    (28, 3),
    (30, 9),
    (28, 15),
    (16, 16),
    (4, 15),
    (2, 9),
    (9, 6),
    (23, 12),
    (24, 5),
];

fn nid(c: usize, r: usize) -> usize {
    r * COLS + c
}

fn node_xy(i: usize) -> (f32, f32) {
    ((i % COLS) as f32, (i / COLS) as f32)
}

#[derive(Clone)]
struct Edge {
    a: usize,
    b: usize,
    len: f32,
    d: f32,
    q: f32,
}

/// The lattice: 4-neighbour tubes plus both diagonals, so a route is not
/// forced into staircases and the geometry it finds is its own.
fn build_edges(rng: &mut Rng) -> Vec<Edge> {
    let mut edges = Vec::new();
    for r in 0..ROWS {
        for c in 0..COLS {
            for (dc, dr) in [(1i32, 0i32), (0, 1), (1, 1), (1, -1)] {
                let nc = c as i32 + dc;
                let nr = r as i32 + dr;
                if nc < 0 || nr < 0 || nc >= COLS as i32 || nr >= ROWS as i32 {
                    continue;
                }
                let a = nid(c, r);
                let b = nid(nc as usize, nr as usize);
                let len = ((dc * dc + dr * dr) as f32).sqrt();
                edges.push(Edge {
                    a,
                    b,
                    len,
                    // A seeded spread of initial conductivities: a perfectly
                    // uniform lattice is a symmetric fixed point, and the
                    // model would sit in it forever.
                    d: 0.55 + 0.9 * rng.f01(),
                    q: 0.0,
                });
            }
        }
    }
    edges
}

/// Solve Kirchhoff's law for the node pressures by Gauss–Seidel, warm-started
/// from the previous step's field (each solve is a small perturbation of the
/// last, so 46 sweeps from a warm start beat 300 from zero). Returns the
/// largest residual |ΣQ| at an interior node — the honesty of the solve.
fn solve_pressures(
    edges: &[Edge],
    adj: &[Vec<usize>],
    p: &mut [f32],
    source: usize,
    sink: usize,
    sweeps: usize,
) -> f32 {
    p[sink] = 0.0;
    for _ in 0..sweeps {
        for i in 0..NODES {
            if i == sink {
                p[i] = 0.0;
                continue;
            }
            let mut num = if i == source { I0 } else { 0.0 };
            let mut den = 0.0;
            for &e in &adj[i] {
                let ed = &edges[e];
                let j = if ed.a == i { ed.b } else { ed.a };
                let k = ed.d / ed.len;
                num += k * p[j];
                den += k;
            }
            if den > 1e-9 {
                // Successive over-relaxation: plain Gauss–Seidel at 46
                // sweeps left the residual at 2.2e−1 against an injected
                // current of I0 = 2 — eleven percent of the flow the solve
                // was supposed to conserve, which makes the adaptation
                // step's Q a fiction. ω = 1.85 buys two orders of
                // magnitude for the same sweeps.
                p[i] += 1.85 * (num / den - p[i]);
            }
        }
    }
    let mut worst = 0.0_f32;
    for i in 0..NODES {
        if i == source || i == sink {
            continue;
        }
        let mut sum = 0.0;
        for &e in &adj[i] {
            let ed = &edges[e];
            let j = if ed.a == i { ed.b } else { ed.a };
            sum += ed.d * (p[i] - p[j]) / ed.len;
        }
        worst = worst.max(sum.abs());
    }
    worst
}

struct Network {
    edges: Vec<Edge>,
    steps: usize,
    residual: f32,
}

fn grow_to(t: f64) -> Network {
    let mut rng = Rng::new(0x7EA0_1234_5678_9ABD);
    let mut edges = build_edges(&mut rng);
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); NODES];
    for (i, e) in edges.iter().enumerate() {
        adj[e.a].push(i);
        adj[e.b].push(i);
    }
    // EVERY unordered pair of towns, in order. The first cut used
    // (s, 5s+3) mod 12, which looks like it walks the towns and does not:
    // the offset (4s+3) mod 12 takes exactly three values, so 30 of the 66
    // pairs were never demanded and the network came out in two pieces —
    // 7/12 towns reachable, printed on the plate's own face.
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for i in 0..FOOD.len() {
        for j in (i + 1)..FOOD.len() {
            pairs.push((i, j));
        }
    }
    let mut p = vec![0.0_f32; NODES];
    let n = ((t * STEPS as f64) as usize).max(1);
    let mut residual = 0.0;
    for s in 0..n {
        // Source and sink walk the food nodes: the organism is fed at one
        // town and drained at another, and over a full cycle every town has
        // been both. (Tero's paper does exactly this — a single fixed pair
        // grows one road and nothing else.)
        let (i, j) = pairs[s % pairs.len()];
        let source = nid(FOOD[i].0, FOOD[i].1);
        let sink = nid(FOOD[j].0, FOOD[j].1);
        residual = solve_pressures(
            &edges,
            &adj,
            &mut p,
            source,
            sink,
            if s == 0 { 400 } else { 90 },
        );
        for e in edges.iter_mut() {
            e.q = e.d * (p[e.a] - p[e.b]) / e.len;
            let aq = e.q.abs().powf(GAMMA);
            let f = aq / (1.0 + aq);
            e.d += DT * (f - e.d);
            if e.d < 1e-4 {
                e.d = 1e-4;
            }
        }
    }
    Network {
        edges,
        steps: n,
        residual,
    }
}

// ── The instruments ─────────────────────────────────────────────────────────

/// How many food nodes are reachable from FOOD[0] through LIVING tubes only.
fn reachable(edges: &[Edge], skip: Option<usize>) -> usize {
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); NODES];
    for (i, e) in edges.iter().enumerate() {
        if e.d < ALIVE || Some(i) == skip {
            continue;
        }
        adj[e.a].push(e.b);
        adj[e.b].push(e.a);
    }
    let start = nid(FOOD[0].0, FOOD[0].1);
    let mut seen = vec![false; NODES];
    let mut stack = vec![start];
    seen[start] = true;
    while let Some(u) = stack.pop() {
        for &v in &adj[u] {
            if !seen[v] {
                seen[v] = true;
                stack.push(v);
            }
        }
    }
    FOOD.iter().filter(|&&(c, r)| seen[nid(c, r)]).count()
}

/// Prim's minimum spanning tree over the food nodes, in the lattice's own
/// Euclidean metric — the baseline every transport network is judged by.
fn mst_length() -> (f32, Vec<(usize, usize)>) {
    let pts: Vec<(f32, f32)> = FOOD.iter().map(|&(c, r)| (c as f32, r as f32)).collect();
    let n = pts.len();
    let d = |a: (f32, f32), b: (f32, f32)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
    let mut in_tree = vec![false; n];
    let mut best = vec![f32::INFINITY; n];
    let mut parent = vec![0usize; n];
    best[0] = 0.0;
    let mut total = 0.0;
    let mut edges = Vec::new();
    for _ in 0..n {
        let mut u = usize::MAX;
        for v in 0..n {
            if !in_tree[v] && (u == usize::MAX || best[v] < best[u]) {
                u = v;
            }
        }
        in_tree[u] = true;
        if best[u].is_finite() && best[u] > 0.0 {
            total += best[u];
            edges.push((parent[u], u));
        }
        for v in 0..n {
            if !in_tree[v] {
                let w = d(pts[u], pts[v]);
                if w < best[v] {
                    best[v] = w;
                    parent[v] = u;
                }
            }
        }
    }
    (total, edges)
}

// ── The frame ───────────────────────────────────────────────────────────────

const DXO: f32 = 60.0;
const DYO: f32 = 168.0;
const DW: f32 = 1000.0;
const DH: f32 = 448.0;

pub(crate) fn frame(t: f32) -> WidgetNode {
    let net = grow_to(t as f64);
    let edges = net.edges;
    let steps = net.steps;

    let tl: f32 = edges.iter().filter(|e| e.d >= ALIVE).map(|e| e.len).sum();
    let (mst, mst_edges) = mst_length();
    let connected = reachable(&edges, None);

    let live_ids: Vec<usize> = edges
        .iter()
        .enumerate()
        .filter(|(_, e)| e.d >= ALIVE)
        .map(|(i, _)| i)
        .collect();
    let mut survived = 0usize;
    for &i in &live_ids {
        if reachable(&edges, Some(i)) == connected {
            survived += 1;
        }
    }
    let tolerance = if live_ids.is_empty() {
        0.0
    } else {
        survived as f32 / live_ids.len() as f32
    };

    // The conductivity concentration: what fraction of all D sits in the
    // thickest 5% of tubes. A fresh lattice starts near 5%; a network ends
    // far above it, and the number is the pruning, measured.
    let mut ds: Vec<f32> = edges.iter().map(|e| e.d).collect();
    ds.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let top = (ds.len() as f32 * 0.05).ceil() as usize;
    let concentration = ds[..top].iter().sum::<f32>() / ds.iter().sum::<f32>().max(1e-6);

    let total_tubes = edges.len();
    let n_live = live_ids.len();
    let residual = net.residual;
    let qmax = edges.iter().map(|e| e.q.abs()).fold(1e-6_f32, f32::max);

    let draw_edges = edges.clone();
    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            book.rect(
                Rect::new(0.0, 0.0, size.width, size.height),
                Gradient::vertical()
                    .with_dither()
                    .with_stops(&[(0.0, Color::rgb(5, 5, 9)), (1.0, Color::rgb(11, 10, 16))]),
            );
            book.rrect(
                Rect::new(DXO - 16.0, DYO - 16.0, DXO + DW + 16.0, DYO + DH + 16.0),
                12.0,
                alpha(Color::rgb(12, 12, 18), 0.96),
            );
            let sx = DW / (COLS - 1) as f32;
            let sy = DH / (ROWS - 1) as f32;
            let px = |i: usize| DXO + node_xy(i).0 * sx;
            let py = |i: usize| DYO + node_xy(i).1 * sy;

            // ── the dying lattice ──
            for e in draw_edges.iter().filter(|e| e.d < ALIVE) {
                let k = (e.d / ALIVE).clamp(0.0, 1.0);
                if k < 0.10 {
                    continue;
                }
                book.line(
                    Offset::new(px(e.a), py(e.a)),
                    Offset::new(px(e.b), py(e.b)),
                    alpha(VIOLET, 0.05 + 0.13 * k),
                    0.8,
                );
            }

            // ── the MST, the baseline, drawn under the organism ──
            for &(a, b) in &mst_edges {
                let (ac, ar) = FOOD[a];
                let (bc, br) = FOOD[b];
                book.line(
                    Offset::new(DXO + ac as f32 * sx, DYO + ar as f32 * sy),
                    Offset::new(DXO + bc as f32 * sx, DYO + br as f32 * sy),
                    alpha(MINT, 0.22),
                    1.0,
                );
            }

            // ── the living network: width IS conductivity, colour IS flow ──
            book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                for e in draw_edges.iter().filter(|e| e.d >= ALIVE) {
                    let w = 1.0 + 7.0 * e.d.clamp(0.0, 1.0).powf(0.75);
                    let f = (e.q.abs() / qmax).clamp(0.0, 1.0).powf(0.45);
                    let c = mix(VIOLET, mix(CYAN_SOFT, AMBER, f), 0.55 + 0.45 * f);
                    g.line(
                        Offset::new(px(e.a), py(e.a)),
                        Offset::new(px(e.b), py(e.b)),
                        alpha(c, 0.30 + 0.60 * e.d.clamp(0.0, 1.0)),
                        w,
                    );
                }
            });

            // ── the towns ──
            for &(c, r) in FOOD.iter() {
                let o = Offset::new(DXO + c as f32 * sx, DYO + r as f32 * sy);
                book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                    g.circle(o, 13.0, alpha(CYAN, 0.10));
                });
                book.ring(o, 7.0, 1.6, alpha(INK, 0.85));
            }

            // ── the conductivity spectrum, bottom strip ──
            let bx = DXO;
            let by = DYO + DH + 48.0;
            let bw = DW;
            let bh = 30.0;
            let mut sorted: Vec<f32> = draw_edges.iter().map(|e| e.d).collect();
            sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
            for (i, d) in sorted.iter().enumerate().step_by(2) {
                let x = bx + i as f32 / sorted.len() as f32 * bw;
                let h = d.clamp(0.0, 1.0) * bh;
                book.rect(
                    Rect::new(x, by + bh - h, x + 1.6, by + bh),
                    alpha(if *d >= ALIVE { AMBER } else { VIOLET }, 0.65),
                );
            }
            book.line(
                Offset::new(bx, by + bh - ALIVE * bh),
                Offset::new(bx + bw, by + bh - ALIVE * bh),
                alpha(MUTED, 0.5),
                1.0,
            );
        }),
    );

    let lines = ["SLIME · THE TRANSPORT AXIS · PHYSARUM BUILDS THE NETWORK (TERO ET AL., 2010)".to_string(),
        format!(
            "dD/dt = f(|Q|) − D, f(Q) = Q^{GAMMA}/(1+Q^{GAMMA}) · Q = D·Δp/L (Poiseuille) · p from Kirchhoff's law at all {NODES} nodes · {total_tubes} tubes, 8-neighbour lattice"
        ),
        format!(
            "replayed from the seed every frame · {steps}/{STEPS} adaptation steps · source and sink walk the {} towns · NO search, no cost function, no shortest-path code anywhere",
            FOOD.len()
        ),
        format!(
            "KIRCHHOFF RESIDUAL max|ΣQ| = {residual:.2e} at any interior node ({:.3}% of the injected I0 = {I0}) · tubes surviving {n_live}/{total_tubes} ({:.1}%) · the thickest 5% now hold {:.1}% of all conductivity",
            residual / I0 * 100.0,
            n_live as f32 / total_tubes as f32 * 100.0,
            concentration * 100.0
        ),
        format!(
            "THE TOKYO COMPARISON: total tube length TL = {tl:.1} vs the minimum spanning tree over the same towns MST = {mst:.1} · TL/MST = {:.3} · towns reachable {connected}/{}",
            tl / mst.max(1e-6),
            FOOD.len()
        ),
        format!(
            "FAULT TOLERANCE by cutting: each of the {n_live} living tubes severed in turn, the towns re-flooded · {:.1}% of cuts change nothing. A minimum spanning tree survives none of them.",
            tolerance * 100.0
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
    stack = stack.push(
        Positioned::new()
            .left(DXO)
            .top(DYO + DH + 24.0)
            .width(760.0)
            .height(14.0)
            .child(
                Text::new(
                    "CONDUCTIVITY SPECTRUM — every tube, sorted · the line is the living threshold"
                        .to_string(),
                )
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
