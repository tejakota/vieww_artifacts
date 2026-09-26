//! exp_sorting — *the algorithm axis.* Six machines, one permutation.
//!
//! The oldest art in computing: put a shuffled array in order, and count
//! what it costs. This plate runs **six sorts on the same 96-value seeded
//! permutation — bubble, insertion, selection, shell (Ciura gaps), quick
//! (middle pivot), merge (bottom-up) — each recorded to a full operation
//! trace, and replayed from it every frame**, all six advancing on one
//! shared operation clock: one comparison or one write per tick per
//! machine. The fast machines finish early and hold their sorted glow
//! while bubble sort grinds on — O(n²) made visible as loneliness.
//!
//! The receipt is the scoreboard, measured from the traces the machines
//! themselves executed: **comparisons and writes per algorithm, the
//! completion order, and the n·log₂n line the fast family hugs** — this
//! seeded permutation prints its own numbers. Bars carry the live state:
//! compared = amber flash, written = violet, finished = mint, settling
//! into the sorted gradient.

use std::sync::OnceLock;

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, mix, AMBER, CYAN, INK, MUTED, VIOLET, MINT, Rng};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The machines ────────────────────────────────────────────────────────────

const N: usize = 96;

/// One recorded operation, with the indices it touched.
#[derive(Clone, Copy)]
enum Op {
    Cmp(usize, usize),
    Write(usize, usize), // the two slots a swap wrote
    Done,
}

/// One machine's biography: name, the op trace, the array state after
/// every op (snapshots, so any frame is an index — replay is lookup),
/// and the counted totals.
struct Machine {
    name: &'static str,
    ops: Vec<Op>,
    states: Vec<[u8; N]>,
    cmp_n: usize,
    wr_n: usize,
}

/// The seeded permutation every machine must put in order.
fn perm() -> &'static [u8; N] {
    static P: OnceLock<[u8; N]> = OnceLock::new();
    P.get_or_init(|| {
        let mut rng = Rng::new(0x5027_C0DE);
        let mut v: Vec<u8> = (0..N as u8).collect();
        for i in (1..N).rev() {
            let j = (rng.f01() * (i + 1) as f32).floor() as usize;
            v.swap(i, j);
        }
        let mut out = [0u8; N];
        out.copy_from_slice(&v);
        out
    })
}

fn trace_all() -> &'static Vec<Machine> {
    static T: OnceLock<Vec<Machine>> = OnceLock::new();
    T.get_or_init(|| {
        let mut out: Vec<Machine> = Vec::new();

        // ── bubble ──
        {
            let mut a = *perm();
            let mut ops = Vec::new();
            let mut states: Vec<[u8; N]> = Vec::new();
            let (mut c, mut w) = (0usize, 0usize);
            states.push(a);
            for i in 0..N - 1 {
                for j in 0..N - 1 - i {
                    ops.push(Op::Cmp(j, j + 1));
                    c += 1;
                    states.push(a);
                    if a[j] > a[j + 1] {
                        a.swap(j, j + 1);
                        ops.push(Op::Write(j, j + 1));
                        w += 2;
                        states.push(a);
                    }
                }
            }
            ops.push(Op::Done);
            states.push(a);
            out.push(Machine { name: "BUBBLE", ops, states, cmp_n: c, wr_n: w });
        }
        // ── insertion ──
        {
            let mut a = *perm();
            let mut ops = Vec::new();
            let mut states = vec![a];
            let (mut c, mut w) = (0usize, 0usize);
            for i in 1..N {
                let mut j = i;
                while j > 0 {
                    ops.push(Op::Cmp(j - 1, j));
                    c += 1;
                    states.push(a);
                    if a[j - 1] > a[j] {
                        a.swap(j - 1, j);
                        ops.push(Op::Write(j - 1, j));
                        w += 2;
                        states.push(a);
                        j -= 1;
                    } else {
                        break;
                    }
                }
            }
            ops.push(Op::Done);
            states.push(a);
            out.push(Machine { name: "INSERTION", ops, states, cmp_n: c, wr_n: w });
        }
        // ── selection ──
        {
            let mut a = *perm();
            let mut ops = Vec::new();
            let mut states = vec![a];
            let (mut c, mut w) = (0usize, 0usize);
            for i in 0..N - 1 {
                let mut m = i;
                for j in i + 1..N {
                    ops.push(Op::Cmp(j, m));
                    c += 1;
                    states.push(a);
                    if a[j] < a[m] {
                        m = j;
                    }
                }
                if m != i {
                    a.swap(i, m);
                    ops.push(Op::Write(i, m));
                    w += 2;
                    states.push(a);
                }
            }
            ops.push(Op::Done);
            states.push(a);
            out.push(Machine { name: "SELECTION", ops, states, cmp_n: c, wr_n: w });
        }
        // ── shell (Ciura gaps) ──
        {
            let mut a = *perm();
            let mut ops = Vec::new();
            let mut states = vec![a];
            let (mut c, mut w) = (0usize, 0usize);
            for gap in [132usize, 57, 23, 10, 4, 1] {
                if gap >= N {
                    continue;
                }
                for i in gap..N {
                    let mut j = i;
                    while j >= gap {
                        ops.push(Op::Cmp(j - gap, j));
                        c += 1;
                        states.push(a);
                        if a[j - gap] > a[j] {
                            a.swap(j - gap, j);
                            ops.push(Op::Write(j - gap, j));
                            w += 2;
                            states.push(a);
                            j -= gap;
                        } else {
                            break;
                        }
                    }
                }
            }
            ops.push(Op::Done);
            states.push(a);
            out.push(Machine { name: "SHELL", ops, states, cmp_n: c, wr_n: w });
        }
        // ── quick (middle pivot) ──
        {
            let mut a = *perm();
            let mut ops = Vec::new();
            let mut states = vec![a];
            let (mut c, mut w) = (0usize, 0usize);
            fn qs(a: &mut [u8; N], lo: i64, hi: i64, ops: &mut Vec<Op>,
                  states: &mut Vec<[u8; N]>, c: &mut usize, w: &mut usize) {
                if lo >= hi {
                    return;
                }
                let mid = (lo + hi) / 2;
                let pivot = a[mid as usize];
                let (mut i, mut j) = (lo, hi);
                loop {
                    while a[i as usize] < pivot {
                        ops.push(Op::Cmp(i as usize, mid as usize));
                        *c += 1;
                        states.push(*a);
                        i += 1;
                    }
                    ops.push(Op::Cmp(i as usize, mid as usize));
                    *c += 1;
                    states.push(*a);
                    while a[j as usize] > pivot {
                        ops.push(Op::Cmp(j as usize, mid as usize));
                        *c += 1;
                        states.push(*a);
                        j -= 1;
                    }
                    ops.push(Op::Cmp(j as usize, mid as usize));
                    *c += 1;
                    states.push(*a);
                    if i <= j {
                        if i != j {
                            a.swap(i as usize, j as usize);
                            ops.push(Op::Write(i as usize, j as usize));
                            *w += 2;
                            states.push(*a);
                        }
                        i += 1;
                        j -= 1;
                    } else {
                        break;
                    }
                }
                qs(a, lo, j, ops, states, c, w);
                qs(a, i, hi, ops, states, c, w);
            }
            qs(&mut a, 0, N as i64 - 1, &mut ops, &mut states, &mut c, &mut w);
            ops.push(Op::Done);
            states.push(a);
            out.push(Machine { name: "QUICK", ops, states, cmp_n: c, wr_n: w });
        }
        // ── merge (bottom-up) ──
        {
            let mut a = *perm();
            let mut ops = Vec::new();
            let mut states = vec![a];
            let (mut c, mut w) = (0usize, 0usize);
            let mut width = 1usize;
            while width < N {
                let mut lo = 0usize;
                while lo < N {
                    let mid = (lo + width).min(N);
                    let hi = (lo + 2 * width).min(N);
                    let mut merged: Vec<u8> = Vec::with_capacity(hi - lo);
                    let (mut i, mut j) = (lo, mid);
                    while i < mid && j < hi {
                        ops.push(Op::Cmp(i, j));
                        c += 1;
                        states.push(a);
                        if a[i] <= a[j] {
                            merged.push(a[i]);
                            i += 1;
                        } else {
                            merged.push(a[j]);
                            j += 1;
                        }
                    }
                    while i < mid {
                        merged.push(a[i]);
                        i += 1;
                    }
                    while j < hi {
                        merged.push(a[j]);
                        j += 1;
                    }
                    for (k, &v) in merged.iter().enumerate() {
                        if a[lo + k] != v {
                            a[lo + k] = v;
                            ops.push(Op::Write(lo + k, lo + k));
                            w += 1;
                            states.push(a);
                        }
                    }
                    lo += 2 * width;
                }
                width *= 2;
            }
            ops.push(Op::Done);
            states.push(a);
            out.push(Machine { name: "MERGE", ops, states, cmp_n: c, wr_n: w });
        }
        out
    })
}

// ── The frame ───────────────────────────────────────────────────────────────

/// 2×3 panel grid.
const PANELS: [(f32, f32, f32, f32); 6] = [
    (64.0, 150.0, 368.0, 168.0),
    (472.0, 150.0, 368.0, 168.0),
    (880.0, 150.0, 368.0, 168.0),
    (64.0, 372.0, 368.0, 168.0),
    (472.0, 372.0, 368.0, 168.0),
    (880.0, 372.0, 368.0, 168.0),
];

pub fn frame(t: f32) -> WidgetNode {
    let machines = trace_all();
    // the shared operation clock: every machine gets the same ops per second
    let max_total = machines.iter().map(|m| m.ops.len()).max().unwrap_or(1) as f32;
    let ops_done = (t * max_total).round() as usize;

    // the frame's own data: state and flash per machine, from the traces
    let per: Vec<(&[u8; N], Op, bool)> = machines
        .iter()
        .map(|m| {
            let k = ops_done.min(m.states.len() - 1);
            let last_op = if k > 0 { m.ops[k - 1] } else { Op::Done };
            let done = m.ops.len() <= ops_done + 1;
            (&m.states[k], last_op, done)
        })
        .collect();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — the machine hall.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(7, 7, 10)),
                    (1.0, Color::rgb(12, 12, 17)),
                ]),
            );

            // the shared clock spine, under the captions
            let spine_y = 128.0;
            book.rrect(Rect::new(64.0, spine_y, 1216.0, spine_y + 3.0), 1.5, Color::rgb(24, 24, 32));
            book.rrect(Rect::new(64.0, spine_y, 64.0 + 1152.0 * t, spine_y + 3.0), 1.5, alpha(VIOLET, 0.9));
            // each machine's finish mark on the spine
            for (i, m) in machines.iter().enumerate() {
                let frac = (m.ops.len() as f32 / max_total).min(1.0);
                let fx = 64.0 + 1152.0 * frac;
                book.rrect(Rect::new(fx - 1.0, spine_y - 4.0, fx + 1.0, spine_y + 7.0), 1.0,
                           alpha(if i % 2 == 0 { CYAN } else { AMBER }, 0.85));
            }

            for (i, &(px, py, pw, ph)) in PANELS.iter().enumerate() {
                let (arr, last_op, done) = per[i];
                book.rrect(
                    Rect::new(px - 10.0, py - 26.0, px + pw + 10.0, py + ph + 26.0),
                    10.0,
                    alpha(Color::rgb(12, 12, 18), 0.95),
                );
                if done {
                    book.stroke_rrect(
                        Rect::new(px - 10.0, py - 26.0, px + pw + 10.0, py + ph + 26.0),
                        10.0,
                        alpha(MINT, 0.5),
                        1.2,
                    );
                }
                // bars
                let bw = pw / N as f32;
                let base = py + ph;
                let (f0, f1, is_write) = match last_op {
                    Op::Cmp(a, b) => (Some(a), Some(b), false),
                    Op::Write(a, b) => (Some(a), Some(b), true),
                    Op::Done => (None, None, false),
                };
                for (bi, &v) in arr.iter().enumerate() {
                    let bh = (v as f32 + 1.0) / N as f32 * ph;
                    let hot = f0 == Some(bi) || f1 == Some(bi);
                    let col = if done {
                        mix(MINT, CYAN, v as f32 / N as f32)
                    } else if hot && is_write {
                        VIOLET
                    } else if hot {
                        AMBER
                    } else {
                        Color::rgb(52, 56, 74)
                    };
                    book.rrect(
                        Rect::new(px + bi as f32 * bw, base - bh, px + (bi + 1) as f32 * bw - 0.8, base),
                        1.0,
                        col,
                    );
                }
                // name plate line + live counters ride the caption overlay
            }
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    // per-panel plates: name + live counters, riding the overlay
    for (i, &(px, py, pw, _)) in PANELS.iter().enumerate() {
        let m = &machines[i];
        let done = m.ops.len() <= ops_done + 1;
        let label = if done {
            format!("{} · {}c/{}w · sorted", m.name, m.cmp_n, m.wr_n)
        } else {
            format!("{} · running…", m.name)
        };
        stack = stack.push(
            Positioned::new()
                .left(px)
                .top(py - 22.0)
                .width(pw)
                .height(15.0)
                .child(
                    Text::new(label).style(
                        TextStyle::new(11.0)
                            .monospace()
                            .letter_spacing(1.2)
                            .color(alpha(if done { MINT } else { MUTED }, 0.95)),
                    ),
                ),
        );
    }
    stack = stack.push(receipt_panel(ops_done, max_total));
    stack.into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(ops_done: usize, max_total: f32) -> WidgetNode {
    let machines = trace_all();
    // completion order, measured from the traces
    let mut order: Vec<(usize, usize, &'static str)> = machines
        .iter()
        .map(|m| (m.ops.len(), m.cmp_n, m.name))
        .collect();
    order.sort_by_key(|(len, _, _)| *len);
    let order_line: String = order
        .iter()
        .map(|(_, _, n)| format!("{n}"))
        .collect::<Vec<_>>()
        .join(" < ");
    let counts_line: String = machines
        .iter()
        .map(|m| format!("{} {}c/{}w", m.name, m.cmp_n, m.wr_n))
        .collect::<Vec<_>>()
        .join(" · ");

    let lines = [
        "SORTING · THE ALGORITHM AXIS · SIX MACHINES, ONE PERMUTATION".to_string(),
        format!(
            "the same seeded 96-value permutation · one shared op clock · ops {ops_done} of {:.0}",
            max_total
        ),
        format!("COMPLETION ORDER (from the traces): {order_line}"),
        format!("COMPARISONS: {}", {
            let mut s: Vec<String> = order
                .iter()
                .map(|&(_, c, n)| format!("{n} {c}"))
                .collect();
            s.sort();
            s.join(" · ")
        }),
        format!("writes: {counts_line}"),
        "amber = compared · violet = written · mint ring + gradient = finished and holding".to_string(),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(1000.0)
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
