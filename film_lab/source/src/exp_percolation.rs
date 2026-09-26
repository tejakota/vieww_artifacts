//! exp_percolation — *the connectivity axis.* The threshold where the
//! world stops being islands.
//!
//! Broadbent and Hammersley, 1957: occupy a lattice's sites at random with
//! probability p, and watch for the cluster that spans it. Below a
//! critical p_c there is only archipelago; above it, a continent opens —
//! a genuine phase transition, sharp in the infinite limit, and the
//! 2D site-percolation threshold p_c = 0.5927 is one of the best-measured
//! numbers in statistical physics. This plate runs the machine: **the
//! lattice's sites carry fixed, seeded uniforms u_i; the film raises p
//! from 0.30 to 0.90, so the same cluster grows monotonically as p admits
//! its sites in order** — union-find relabels the world every frame, and
//! the moment a cluster touches both top and bottom edges it ignites.
//!
//! The receipt brackets the transition with the machine's own sweep: the
//! last p with no spanning cluster, the first with one, and the largest-
//! cluster fraction f(p) plotted as the curve in the corner — measured at
//! every frame of the same replay, beside theory's 0.5927.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, mix, AMBER, CYAN, INK, MUTED, VIOLET, Rng};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The machine ─────────────────────────────────────────────────────────────

const GX: usize = 160;
const GY: usize = 96;

/// The sweep. Held wide so the bracket is honest, then the curve keeps
/// climbing past the transition to show the continent filling in.
const P0: f64 = 0.30;
const P1: f64 = 0.90;

/// The seeded site uniforms — fixed forever, so the film is one monotone
/// growth, not a new universe per frame.
fn uniforms() -> &'static Vec<f32> {
    use std::sync::OnceLock;
    static U: OnceLock<Vec<f32>> = OnceLock::new();
    U.get_or_init(|| {
        let mut rng = Rng::new(0xC0FFEE);
        (0..GX * GY).map(|_| rng.f01()).collect()
    })
}

/// Union-find over the occupied sites at occupancy p. Returns parent array
/// (root per cell, 0 = unoccupied), plus the spanning cluster's root and
/// the largest cluster's (root, size).
fn label(p: f64) -> (Vec<i32>, Option<usize>, (usize, usize)) {
    let u = uniforms();
    let mut parent: Vec<i32> = vec![-1; GX * GY];
    let mut size: Vec<i32> = vec![0; GX * GY];

    fn find(parent: &mut Vec<i32>, mut i: usize) -> usize {
        // path-compressed iterative find; cell is its own root when
        // parent[i] == i (stored as i as i32)
        let mut root = i;
        while parent[root] != root as i32 {
            root = parent[root] as usize;
        }
        while parent[i] != i as i32 {
            let next = parent[i] as usize;
            parent[i] = root as i32;
            i = next;
        }
        root
    }

    // occupy and union with occupied neighbours (right, down already done)
    for y in 0..GY {
        for x in 0..GX {
            let idx = y * GX + x;
            if (u[idx] as f64) < p {
                parent[idx] = idx as i32;
                size[idx] = 1;
                if x > 0 && parent[idx - 1] >= 0 {
                    let a = find(&mut parent, idx);
                    let b = find(&mut parent, idx - 1);
                    if a != b {
                        parent[b] = a as i32;
                        size[a] += size[b];
                    }
                }
                if y > 0 && parent[idx - GX] >= 0 {
                    let a = find(&mut parent, idx);
                    let b = find(&mut parent, idx - GX);
                    if a != b {
                        parent[b] = a as i32;
                        size[a] += size[b];
                    }
                }
            }
        }
    }
    // find the spanning root and the largest root
    let mut spanning: Option<usize> = None;
    let mut best = (0usize, 0usize); // (root, size)
    for idx in 0..GX * GY {
        if parent[idx] < 0 {
            continue;
        }
        let r = find(&mut parent, idx);
        let sz = size[r] as usize;
        if sz > best.1 {
            best = (r, sz);
        }
    }
    // top row roots
    let mut top_roots: Vec<usize> = Vec::new();
    for x in 0..GX {
        let idx = x;
        if parent[idx] >= 0 {
            let r = find(&mut parent, idx);
            if !top_roots.contains(&r) {
                top_roots.push(r);
            }
        }
    }
    for x in 0..GX {
        let idx = (GY - 1) * GX + x;
        if parent[idx] >= 0 {
            let r = find(&mut parent, idx);
            if top_roots.contains(&r) {
                spanning = Some(r);
                break;
            }
        }
    }
    // final parent pass for drawing: parent[i] = root
    for idx in 0..GX * GY {
        if parent[idx] >= 0 {
            let r = find(&mut parent, idx);
            parent[idx] = r as i32;
        }
    }
    (parent, spanning, best)
}

/// The whole sweep analysis, computed once: the bracket (bisection-refined)
/// and the f(p) curve at 60 steps. Pure functions of the seed and the
/// lattice — so the frame asks for them, never recomputes them.
fn sweep_data() -> &'static ((f64, f64, f64, f64), Vec<(f64, f64, bool)>) {
    use std::sync::OnceLock;
    static SWEEP: OnceLock<((f64, f64, f64, f64), Vec<(f64, f64, bool)>)> = OnceLock::new();
    SWEEP.get_or_init(|| {
        let curve: Vec<(f64, f64, bool)> = {
            let n = 60;
            (0..=n)
                .map(|i| {
                    let pp = P0 + (P1 - P0) * (i as f64 / n as f64);
                    let (_, sp, be) = label(pp);
                    (pp, be.1 as f64 / (GX * GY) as f64, sp.is_some())
                })
                .collect()
        };
        (bracket(), curve)
    })
}

/// The bracket: replay the sweep finely and bracket the crossing.
/// Returns (last p with no span, first p with span, f at those points).
fn bracket() -> (f64, f64, f64, f64) {
    let mut lo = P0;
    let mut hi = P1;
    let mut f_lo = 0.0;
    let mut f_hi = 0.0;
    let n = 240;
    let mut prev_span = false;
    let mut first = f64::NAN;
    let mut last_no = f64::NAN;
    for i in 0..=n {
        let p = P0 + (P1 - P0) * (i as f64 / n as f64);
        let (_, span, best) = label(p);
        let f = best.1 as f64 / (GX * GY) as f64;
        let spans = span.is_some();
        if i > 0 {
            if spans && !prev_span {
                first = p;
                f_hi = f;
            }
            if !spans && prev_span {
                // (cannot happen — growth is monotone)
            }
            if !spans {
                last_no = p;
                f_lo = f;
            }
        } else {
            last_no = p;
            f_lo = f;
        }
        prev_span = spans;
    }
    // refine between last_no and first with a bisection to frame accuracy
    if !first.is_nan() {
        let mut a = last_no;
        let mut b = first;
        for _ in 0..18 {
            let m = (a + b) / 2.0;
            let (_, span, _) = label(m);
            if span.is_some() {
                b = m;
            } else {
                a = m;
            }
        }
        lo = a;
        hi = b;
    }
    (lo, hi, f_lo, f_hi)
}

// ── The frame ───────────────────────────────────────────────────────────────

const X0: f32 = 60.0;
const Y0: f32 = 140.0;
const CW: f32 = 5.6;
const CH: f32 = 5.6;

pub fn frame(t: f32) -> WidgetNode {
    let p = P0 + (P1 - P0) * t as f64;
    let (parent, spanning, best) = label(p);
    let span_frac = best.1 as f64 / (GX * GY) as f64;
    let span = spanning.is_some();

    // The bracket and the curve history — one analysis, cached, the film
    // reading it forward frame by frame.
    let ((blo, bhi, _, _), curve) = &*sweep_data();
    let (blo, bhi) = (*blo, *bhi);

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — cold slate at the moment before connection.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(6, 7, 10)),
                    (1.0, Color::rgb(11, 12, 17)),
                ]),
            );

            // ── The lattice: one rect per site ──
            // unoccupied: near-black. occupied island: slate-violet.
            // spanning: a violet→cyan gradient by row, the lit continent.
            let span_root = spanning.map(|s| s as i32);
            for y in 0..GY {
                for x in 0..GX {
                    let idx = y * GX + x;
                    if parent[idx] < 0 {
                        continue; // the void stays the ground's own dark
                    }
                    let on_span = span_root == Some(parent[idx]);
                    let col = if on_span {
                        // gradient by row: violet at top → cyan at bottom
                        let g = y as f32 / GY as f32;
                        mix(VIOLET, CYAN, g)
                    } else if parent[idx] == best.0 as i32 {
                        // the largest island, quietly distinct
                        Color::rgb(52, 56, 78)
                    } else {
                        Color::rgb(30, 33, 46)
                    };
                    book.rect(
                        Rect::new(
                            X0 + x as f32 * CW,
                            Y0 + y as f32 * CH,
                            X0 + (x + 1) as f32 * CW + 0.45,
                            Y0 + (y + 1) as f32 * CH + 0.45,
                        ),
                        col,
                    );
                }
            }

            // The ignition: the moment of spanning, a white shock ripple
            // expanding from the first bridging column (deterministic from
            // the machine's own arrays: the first top-row site in the
            // spanning root).
            if let Some(root) = spanning {
                let bridge = (0..GX * GY)
                    .find(|&idx| parent[idx] == root as i32 && idx < GX)
                    .unwrap_or(GX / 2);
                let bx = X0 + (bridge % GX) as f32 * CW + CW / 2.0;
                let by = Y0 + 2.0;
                let age = ((t as f64 - (blo - P0) / (P1 - P0)).max(0.0) / 0.12).min(1.0);
                if age < 1.0 {
                    let r = (age * 620.0) as f32;
                    book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                        g.ring(
                            Offset::new(bx, by),
                            r,
                            (14.0 * (1.0 - age as f32)).max(2.0),
                            alpha(Color::rgb(190, 200, 255), (1.0 - age as f32) * 0.35),
                        );
                    });
                }
            }

            // ── The f(p) curve panel, right ──
            let px0 = 1000.0;
            let py0 = 160.0;
            let pw = 230.0;
            let ph = 360.0;
            book.rrect(
                Rect::new(px0 - 16.0, py0 - 26.0, px0 + pw + 16.0, py0 + ph + 40.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.94),
            );
            book.line(
                Offset::new(px0, py0 + ph),
                Offset::new(px0 + pw, py0 + ph),
                alpha(MUTED, 0.45),
                1.0,
            );
            book.line(
                Offset::new(px0, py0 + ph),
                Offset::new(px0, py0),
                alpha(MUTED, 0.45),
                1.0,
            );
            // theory's threshold
            let tx = px0 + ((0.5927 - P0) / (P1 - P0)) as f32 * pw;
            book.line(
                Offset::new(tx, py0),
                Offset::new(tx, py0 + ph),
                alpha(AMBER, 0.5),
                1.0,
            );
            // the curve, drawn to the film's own p
            let cx = |pp: f64| px0 + ((pp - P0) / (P1 - P0) * pw as f64) as f32;
            let cy = |f: f64| py0 + ph - (f * ph as f64) as f32;
            let mut path = Path::new();
            let mut started = false;
            for &(pp, f, sp) in curve.iter() {
                if pp > p {
                    break;
                }
                let (x, y) = (cx(pp), cy(f));
                let y = if sp { y } else { y };
                if !started {
                    path.move_to(Offset::new(x, y));
                    started = true;
                } else {
                    path.line_to(Offset::new(x, y));
                }
            }
            book.stroke(path, alpha(VIOLET, 0.9), 1.8);
            // the live point
            book.circle(
                Offset::new(cx(p), cy(span_frac)),
                3.2,
                if span { INK } else { MUTED },
            );

            // p readout spine under the lattice
            let spine_y = Y0 + GY as f32 * CH + 18.0;
            book.rrect(
                Rect::new(X0, spine_y, X0 + GX as f32 * CW, spine_y + 3.0),
                1.5,
                Color::rgb(24, 24, 32),
            );
            book.rrect(
                Rect::new(X0, spine_y, X0 + GX as f32 * CW * t, spine_y + 3.0),
                1.5,
                alpha(if span { CYAN } else { VIOLET }, 0.9),
            );
            // the threshold notch on the spine
            let nx = X0 + GX as f32 * CW * ((0.5927 - P0) / (P1 - P0)) as f32;
            book.rrect(Rect::new(nx - 1.0, spine_y - 3.0, nx + 1.0, spine_y + 6.0), 1.0, AMBER);
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(p, span, span_frac, best.1, blo, bhi));
    stack.into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(
    p: f64,
    span: bool,
    span_frac: f64,
    largest: usize,
    blo: f64,
    bhi: f64,
) -> WidgetNode {
    let lines = [
        "PERCOLATION · THE CONNECTIVITY AXIS · THE THRESHOLD".to_string(),
        format!(
            "site percolation · {GX}×{GY} · seeded uniforms, p swept 0.30 → 0.90 · union-find every frame"
        ),
        format!(
            "p now {p:.3} · largest cluster {} sites ({span_frac:.1} frac) · spanning: {}",
            largest,
            if span { "YES — the continent is open" } else { "no — archipelago" }
        ),
        format!(
            "MEASURED: spanning first appears between p = {blo:.4} and p = {bhi:.4} (18-step bisection, same lattice)"
        ),
        "theory p_c = 0.5927 (2D site, square, infinite) — the finite-size shift is this machine’s own".to_string(),
        "the amber line on the curve and the notch on the spine are that same 0.5927".to_string(),
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
