//! exp_truss — *the statics axis.* The bridge that shows its own thinking.
//!
//! A Pratt truss, statically determinate, solved by the **method of
//! joints** every frame: the reactions from overall equilibrium, then
//! joint by joint, two equations each, a 2×2 solve per pair of unknowns —
//! the same procedure a civil engineer does by hand, run at frame rate.
//! Every member's force is a number from that solve, and the number IS
//! the rendering: **tension members glow amber-red, compression members
//! cool cyan-blue, width scales with |force|**. A train crosses; the
//! force map flows through the structure in real time — load entering at
//! a chord, splitting at the first panel point, the diagonals carrying it
//! home to the piers.
//!
//! The receipt closes the discipline's own books: **ΣFy = 0 checked from
//! the solved reactions against the applied loads** (the residual printed
//! to machine precision), the determinacy identity 2j − m − r = 0 counted
//! from the model's own arrays, and the extremes — the most-loaded
//! tension and compression members, measured, named, valued — as the
//! train passes mid-span.

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, ease_in_out, mix, tint, AMBER, CYAN, INK,
    MUTED, RED, VIOLET};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 12.0;

// ── The structure ───────────────────────────────────────────────────────────

/// Panels across the span.
const PANELS: usize = 8;

/// Truss geometry (px): bottom chord y, top chord y, x span.
const Y_BOT: f32 = 560.0;
const Y_TOP: f32 = 400.0;
const X0: f32 = 200.0;
const X1: f32 = 1080.0;

/// Joints: bottom chord 0..=PANELS (indices), top chord PANELS+1..=2·PANELS−1.
/// Returns joint positions.
#[must_use]
fn joints() -> Vec<(f32, f32)> {
    let mut v = Vec::new();
    let dx = (X1 - X0) / PANELS as f32;
    for i in 0..=PANELS {
        v.push((X0 + i as f32 * dx, Y_BOT)); // bottom: 0..=8
    }
    for i in 1..PANELS {
        v.push((X0 + i as f32 * dx, Y_TOP)); // top: 9..=15 (7 joints)
    }
    v
}

/// Members as joint-index pairs, typed: chord / vertical / diagonal.
#[derive(Clone)]
struct Member {
    a: usize,
    b: usize,
    kind: &'static str,
}

#[must_use]
fn members() -> Vec<Member> {
    let mut v = Vec::new();
    let bot = |i: usize| i; // bottom joint i
    let top = |i: usize| PANELS + 1 + (i - 1); // top joint above panel point i (1..=7)
    // Bottom chord: bot(i)-bot(i+1).
    for i in 0..PANELS {
        v.push(Member { a: bot(i), b: bot(i + 1), kind: "bottom" });
    }
    // Top chord: top(i)-top(i+1).
    for i in 1..PANELS - 1 {
        v.push(Member { a: top(i), b: top(i + 1), kind: "top" });
    }
    // Verticals: bot(i)-top(i).
    for i in 1..PANELS {
        v.push(Member { a: bot(i), b: top(i), kind: "vertical" });
    }
    // End posts: bot(0)-top(1), bot(N)-top(N−1).
    v.push(Member { a: bot(0), b: top(1), kind: "diagonal" });
    v.push(Member { a: bot(PANELS), b: top(PANELS - 1), kind: "diagonal" });
    // Pratt diagonals: one per interior panel, sloping down toward
    // mid-span from each end (the Pratt signature: diagonals in tension).
    for i in 1..PANELS / 2 {
        v.push(Member { a: top(i), b: bot(i + 1), kind: "diagonal" }); // left half, down-right
        v.push(Member { a: top(PANELS - i), b: bot(PANELS - i - 1), kind: "diagonal" }); // right
    }
    v
}

// ── The loads and the solve ─────────────────────────────────────────────────

/// Joint loads at time t: self-weight on every joint (small, down) plus
/// the train — two axles of weight W at deck position p(t), interpolated
/// onto the neighbouring bottom joints.
#[must_use]
fn loads(t: f32) -> (Vec<f32>, f32) {
    let js = joints();
    let n = js.len();
    // All forces in the geometry's own convention: y-down positive, so
    // gravity loads are POSITIVE and the (upward) reactions come out
    // negative. The first cut mixed up-positive loads with y-down unit
    // vectors — every member force came out mirrored (top chord in
    // "tension"), found by checking the textbook answer.
    let mut f = vec![0.0_f32; n];
    for f_i in f.iter_mut() {
        *f_i += 1.2;
    }
    // The train: crosses from left to right, easing in and out.
    let p = ease_in_out(t); // 0..1 across the span
    let span = X1 - X0;
    let axle1 = X0 + p * span - 46.0;
    let axle2 = X0 + p * span + 46.0;
    let w_axle = 9.0;
    let dx = span / PANELS as f32;
    for &ax in &[axle1, axle2] {
        let u = (ax - X0) / dx;
        let i = u.floor() as i64;
        let frac = u - i as f32;
        for (k, w) in [(i, (1.0 - frac)), (i + 1, frac)] {
            if (0..=PANELS as i64).contains(&k) {
                f[k as usize] += w_axle * w;
            }
        }
    }
    (f, p)
}

/// Solve the truss by the method of joints. Returns member forces (tension
/// +), the two reactions, and the ΣFy residual (equilibrium, checked).
#[must_use]
fn solve(t: f32) -> (Vec<f32>, [f32; 2], f32) {
    let js = joints();
    let ms = members();
    let (f, _) = loads(t);
    let n = js.len();
    let m = ms.len();

    // Unknowns: member forces (m) + reactions at joint 0 and joint PANELS.
    // Solve joint by joint: start at the supported joints, peel joints
    // with ≤2 unknown members. Determinate trusses always allow it.
    let mut member_force = vec![f32::NAN; m];
    let mut solved = vec![false; n];
    let mut reactions = [0.0_f32; 2];

    // Reactions from INDEPENDENT moment equations: the right reaction
    // from ΣM about the left support, the left from ΣM about the right —
    // so the ΣFy residual below is a genuine check, not a tautology (the
    // first cut derived one reaction from the other, making the printed
    // residual identically zero — a check that cannot fail checks nothing).
    let total: f32 = f.iter().sum();
    let span = X1 - X0;
    let m_a: f32 = f
        .iter()
        .enumerate()
        .map(|(i, &fi)| fi * (js[i].0 - X0))
        .sum();
    let m_b: f32 = f
        .iter()
        .enumerate()
        .map(|(i, &fi)| fi * (X1 - js[i].0))
        .sum();
    reactions[1] = -m_a / span;
    reactions[0] = -m_b / span;
    // ΣFy residual (y-down): reactions (negative) + loads (positive) ≈ 0.
    let resid = reactions[0] + reactions[1] + total;

    // Member unknowns per joint: track which members are unsolved.
    let mut unknown: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (mi, mem) in ms.iter().enumerate() {
        if member_force[mi].is_nan() {
            unknown[mem.a].push(mi);
            unknown[mem.b].push(mi);
        }
    }
    // Externals per joint: load + reactions at the two ends.
    let mut ext = f;
    ext[0] += reactions[0];
    ext[PANELS] += reactions[1];

    loop {
        // Find a joint with exactly ≤2 unknown members and ≥1 solved
        // member or external context — standard peel.
        let mut progress = false;
        for j in 0..n {
            if solved[j] {
                continue;
            }
            unknown[j].retain(|&mi| member_force[mi].is_nan());
            let k = unknown[j].len();
            if k == 0 {
                solved[j] = true;
                progress = true;
                continue;
            }
            if k > 2 {
                continue;
            }
            // Two equations: ΣFx = 0, ΣFy = 0. Known members contribute.
            // Build the 2×k system.
            let mut a_mat = [[0.0_f32; 2]; 2];
            let mut rhs = [-ext[j]; 2];
            // Contributions of already-solved members at this joint —
            // and ONLY members that touch it. The first cut accumulated a
            // force for every solved member in the truss, connected or
            // not, along a direction invented for the occasion; the
            // phantom forces compounded joint by joint until member
            // forces read 90× the applied load. Found by reading the
            // receipt and refusing to believe it.
            for (mi, mem) in ms.iter().enumerate() {
                if member_force[mi].is_nan() || (mem.a != j && mem.b != j) {
                    continue;
                }
                let other = if mem.a == j { mem.b } else { mem.a };
                let (ux, uy) = unit(js[j], js[other]);
                // Force on joint from member: tension pulls toward `other`.
                let fmag = member_force[mi];
                rhs[0] -= fmag * ux;
                rhs[1] -= fmag * uy;
            }
            for (col, &mi) in unknown[j].iter().enumerate() {
                let mem = &ms[mi];
                let other = if mem.a == j { mem.b } else { mem.a };
                let (ux, uy) = unit(js[j], js[other]);
                a_mat[0][col] = ux;
                a_mat[1][col] = uy;
            }
            // Solve 2×k.
            if k == 1 {
                let [ax, ay] = [a_mat[0][0], a_mat[1][0]];
                let denom = ax * ax + ay * ay;
                if denom > 1e-12 {
                    member_force[unknown[j][0]] = (rhs[0] * ax + rhs[1] * ay) / denom;
                    solved[j] = true;
                    progress = true;
                }
            } else {
                // 2×2: [[a, b], [c, d]]
                let (a, b) = (a_mat[0][0], a_mat[0][1]);
                let (c, d) = (a_mat[1][0], a_mat[1][1]);
                let det = a * d - b * c;
                if det.abs() > 1e-12 {
                    let x = (rhs[0] * d - rhs[1] * b) / det;
                    let y_ = (a * rhs[1] - c * rhs[0]) / det;
                    member_force[unknown[j][0]] = x;
                    member_force[unknown[j][1]] = y_;
                    solved[j] = true;
                    progress = true;
                }
            }
        }
        if !progress {
            break;
        }
        if member_force.iter().all(|&v| !v.is_nan()) {
            break;
        }
    }
    // Any unsolved member (shouldn't happen in a determinate truss):
    // record it as 0 to keep rendering total.
    for v in member_force.iter_mut() {
        if v.is_nan() {
            *v = 0.0;
        }
    }
    (member_force, reactions, resid)
}

fn unit(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let m = (dx * dx + dy * dy).sqrt().max(1e-9);
    (dx / m, dy / m)
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let js = joints();
    let ms = members();
    let (forces, reactions, resid) = solve(t);
    let (_, train_p) = loads(t);

    // The extremes, measured from the solved array.
    let (mut max_t, mut max_c) = (0.0_f32, 0.0_f32);
    let (mut name_t, mut name_c) = (String::new(), String::new());
    for (i, &f) in forces.iter().enumerate() {
        if f > max_t {
            max_t = f;
            name_t = format!("{} #{}", ms[i].kind, i);
        }
        if f < max_c {
            max_c = f;
            name_c = format!("{} #{}", ms[i].kind, i);
        }
    }

    let ms_draw = ms.clone();
    let forces_draw = forces.clone();
    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground — an engineer's blue hour.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(8, 9, 13)),
                    (1.0, Color::rgb(12, 13, 18)),
                ]),
            );

            // The river below — the reason there's a bridge.
            book.rect(
                Rect::new(0.0, Y_BOT + 54.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, alpha(CYAN, 0.10)),
                    (1.0, alpha(Color::rgb(3, 10, 20), 0.9)),
                ]),
            );
            // A few water lines.
            for k in 0..5 {
                let y = Y_BOT + 70.0 + k as f32 * 16.0;
                book.line(
                    Offset::new(120.0 + (k * 37) as f32, y),
                    Offset::new(300.0 + (k * 53) as f32, y),
                    alpha(tint(CYAN, 0.3), 0.12),
                    1.0,
                );
                book.line(
                    Offset::new(760.0 + (k * 41) as f32, y + 4.0),
                    Offset::new(1000.0 + (k * 29) as f32, y + 4.0),
                    alpha(tint(CYAN, 0.3), 0.10),
                    1.0,
                );
            }

            // The piers.
            for &(px0, _) in &[js[0], js[PANELS]] {
                book.rect(Rect::new(px0 - 16.0, Y_BOT, px0 + 16.0, Y_BOT + 58.0), Color::rgb(30, 32, 40));
                book.stroke_rrect(
                    Rect::new(px0 - 16.0, Y_BOT, px0 + 16.0, Y_BOT + 58.0),
                    2.0,
                    alpha(Color::WHITE, 0.08),
                    1.0,
                );
            }

            // ── The members: force as colour and width ─────────────────
            // Tension (+): amber → red with magnitude. Compression (−):
            // cyan → deep blue. The number is the colour.
            let fmag = |f: f32| (f.abs() / 42.0).clamp(0.0, 1.0);
            for (i, mem) in ms_draw.iter().enumerate() {
                let f = forces_draw[i];
                let m = fmag(f);
                let col = if f >= 0.0 {
                    mix(AMBER, RED, m)
                } else {
                    mix(CYAN, Color::rgb(20, 40, 120), m)
                };
                let width = 2.0 + 4.5 * m;
                book.line(
                    Offset::new(js[mem.a].0, js[mem.a].1),
                    Offset::new(js[mem.b].0, js[mem.b].1),
                    alpha(col, 0.55 + 0.45 * m),
                    width,
                );
            }

            // The joints: pins.
            for &(jx, jy) in js.iter() {
                book.circle(Offset::new(jx, jy), 4.4, Color::rgb(18, 19, 26));
                book.ring(Offset::new(jx, jy), 4.4, 1.2, alpha(INK, 0.85));
            }

            // The deck: the rail the train rides.
            book.line(
                Offset::new(X0 - 30.0, Y_BOT + 10.0),
                Offset::new(X1 + 30.0, Y_BOT + 10.0),
                alpha(mix(MUTED, INK, 0.3), 0.8),
                3.0,
            );

            // ── The train ──────────────────────────────────────────────
            let tx = X0 + train_p * (X1 - X0);
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                // Body: three cars, violet-lit.
                g.rrect(
                    Rect::new(tx - 92.0, Y_BOT - 40.0, tx + 10.0, Y_BOT - 4.0),
                    6.0,
                    alpha(mix(VIOLET, Color::rgb(30, 24, 48), 0.35), 0.95),
                );
                g.rrect(
                    Rect::new(tx + 16.0, Y_BOT - 34.0, tx + 92.0, Y_BOT - 4.0),
                    5.0,
                    alpha(mix(VIOLET, Color::rgb(30, 24, 48), 0.5), 0.95),
                );
                // Windows.
                for k in 0..4 {
                    g.rect(
                        Rect::new(tx - 82.0 + k as f32 * 22.0, Y_BOT - 32.0,
                                  tx - 70.0 + k as f32 * 22.0, Y_BOT - 20.0),
                        alpha(tint(CYAN, 0.5), 0.8),
                    );
                }
                // The headlight.
                g.circle(
                    Offset::new(tx + 94.0, Y_BOT - 18.0),
                    12.0,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(tint(AMBER, 0.5), 0.7)),
                        (1.0, alpha(AMBER, 0.0)),
                    ]),
                );
                // Axle loads: arrows into the deck at the two bogies.
                for &ax in &[tx - 46.0, tx + 46.0] {
                    g.line(
                        Offset::new(ax, Y_BOT - 6.0),
                        Offset::new(ax, Y_BOT + 6.0),
                        alpha(tint(AMBER, 0.4), 0.9),
                        2.2,
                    );
                }
            });

            // ── The force legend: a small ramp ─────────────────────────
            const G_X: f32 = 90.0;
            const G_Y: f32 = 150.0;
            book.rrect(
                Rect::new(G_X - 14.0, G_Y - 26.0, G_X + 150.0, G_Y + 66.0),
                8.0,
                alpha(Color::rgb(12, 12, 17), 0.92),
            );
            for k in 0..48 {
                let u = k as f32 / 47.0;
                let col = if u < 0.5 {
                    mix(CYAN, Color::rgb(20, 40, 120), u * 2.0)
                } else {
                    mix(AMBER, RED, (u - 0.5) * 2.0)
                };
                book.rect(
                    Rect::new(G_X + u * 130.0, G_Y, G_X + u * 130.0 + 3.0, G_Y + 12.0),
                    col,
                );
            }
            book.line(
                Offset::new(G_X + 65.0, G_Y - 5.0),
                Offset::new(G_X + 65.0, G_Y + 17.0),
                alpha(Color::WHITE, 0.4),
                1.0,
            );
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack = stack.push(receipt_panel(&ms, &forces, reactions, resid, &name_t, max_t, &name_c, max_c));
    stack.into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(
    ms: &[Member],
    _forces: &[f32],
    reactions: [f32; 2],
    resid: f32,
    name_t: &str,
    max_t: f32,
    name_c: &str,
    max_c: f32,
) -> WidgetNode {
    let m = ms.len();
    let j = joints().len();
    let lines = [
        "TRUSS · THE STATICS AXIS · METHOD OF JOINTS, LIVE".to_string(),
        format!(
            "Pratt truss · {} joints · {} members · 2j − m − r = {} (determinate: {})",
            j,
            m,
            2 * j as i32 - m as i32 - 3,
            2 * j as i32 - m as i32 - 3 == 0
        ),
        format!(
            "reactions L {:.3}↑ R {:.3}↑ · ΣFy residual {resid:+.2e} — equilibrium, checked",
            -reactions[0], -reactions[1]
        ),
        format!(
            "max tension {} {max_t:+.2} · max compression {} {max_c:+.2} (from the solve)",
            name_t, name_c
        ),
        "colour IS force: amber-red pulls, cyan-blue pushes — the bridge's own thinking".to_string(),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(860.0)
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
