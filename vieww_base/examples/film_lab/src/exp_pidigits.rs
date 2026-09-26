//! exp_pidigits — *the counting axis.* π, banged out of two blocks.
//!
//! Gregory Galperin, 2003. Put a block of mass m against a wall. Slide a
//! block of mass M at it. Every collision — block on block, block on wall —
//! is perfectly elastic, there is no friction, and nothing else happens.
//! **Count the collisions.**
//!
//! ```text
//!     M/m = 1        →  3 collisions
//!     M/m = 100      →  31
//!     M/m = 10 000   →  314
//!     M/m = 10⁶      →  3 141
//!     M/m = 10⁸      →  31 415
//!     M/m = 10¹⁰     →  314 159
//! ```
//!
//! There is no circle anywhere in the problem. π arrives because the two
//! conservation laws — momentum, a line, and energy, an ellipse — turn the
//! collision sequence into a sequence of *reflections*, and reflections in
//! a circle are rotations by a fixed angle θ. The count is how many turns
//! of θ fit inside π: **N = ⌊π/θ⌋**, with θ = arctan√(m/M).
//!
//! **The plate does not assume any of that.** It runs the mechanics —
//! event-driven, exact collision times, the elastic two-body formulas —
//! and *counts*. Then it prints, for each mass ratio:
//! - **the collisions the simulation actually had**,
//! - **⌊π/θ⌋** with θ = arctan√(m/M) computed from the masses, and
//! - **the digits of π**, so the reader can check all three against each
//!   other by eye.
//!
//! And then it turns the machine around: **π ≈ N·θ**, where N is the count
//! the blocks produced and θ is their own angle. The error is printed. The
//! blocks are a π-computing device with an accuracy of one part in N.
//!
//! **The books that must close while it runs:** kinetic energy and
//! momentum-with-the-wall. Energy is conserved by every collision and
//! the plate prints |ΔE/E| over the whole run — at 314,159 collisions in
//! f64 that number is the honest statement of how much arithmetic the
//! demonstration survived.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook,
    TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, mix, AMBER, CYAN, CYAN_SOFT, INK, MINT, MUTED, RED, VIOLET};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 13.0;

/// The digits of π, for the comparison column. Not used in any
/// calculation — the plate computes π from the collisions instead.
const PI_DIGITS: &str = "3141592653";

/// The small block's mass, and its width in simulation units.
const M_SMALL: f64 = 1.0;
const W_SMALL: f64 = 0.55;
const W_BIG: f64 = 1.6;
/// The big block's starting position and speed (toward the wall).
const X_BIG0: f64 = 7.0;
const V_BIG0: f64 = -1.0;
const X_SMALL0: f64 = 2.2;

/// The ratio the animation plays out. 10⁴ gives 314 collisions — enough
/// to see the small block become a blur and the big one turn around, few
/// enough to watch.
const SHOW_N: u32 = 2;

struct Run {
    collisions: u64,
    /// (time, x_small, x_big, v_small, v_big) at every event, for drawing.
    events: Vec<(f64, f64, f64, f64, f64)>,
    e0: f64,
    e1: f64,
    /// The phase-space angle θ = arctan√(m/M).
    theta: f64,
    total_time: f64,
}

/// Galperin's count, ⌈π/θ⌉ − 1.
///
/// The obvious spelling is ⌊π/θ⌋ and it is wrong in exactly one case,
/// which the plate met on its first run: at M/m = 1, θ = π/4 and π/θ = 4
/// exactly, so the floor says 4 while the blocks collide 3 times. The
/// last reflection would land the state *on* the boundary rather than
/// past it, and a state on the boundary is a block that has escaped.
/// ⌈π/θ⌉ − 1 agrees with the floor everywhere else and is right here too.
fn galperin(theta: f64) -> u64 {
    (std::f64::consts::PI / theta).ceil() as u64 - 1
}

/// Run the mechanics for a mass ratio of 100^n, event by event. Returns
/// the collision count and (when `record`) the full event list.
fn run(n: u32, record: bool, stop_after: Option<u64>) -> Run {
    let big = M_SMALL * 100.0_f64.powi(n as i32);
    let mut x1 = X_SMALL0; // small block's left face
    let mut x2 = X_BIG0; // big block's left face
    let mut v1 = 0.0_f64;
    let mut v2 = V_BIG0;
    let e0 = 0.5 * M_SMALL * v1 * v1 + 0.5 * big * v2 * v2;
    let mut t = 0.0_f64;
    let mut events: Vec<(f64, f64, f64, f64, f64)> = Vec::new();
    if record {
        events.push((t, x1, x2, v1, v2));
    }
    let mut collisions = 0u64;
    loop {
        if let Some(limit) = stop_after {
            if collisions >= limit {
                break;
            }
        }
        // time to the wall (x1 = 0) and to block contact (x1 + w1 = x2)
        let t_wall = if v1 < 0.0 { -x1 / v1 } else { f64::INFINITY };
        let gap = x2 - (x1 + W_SMALL);
        let closing = v1 - v2;
        let t_hit = if closing > 0.0 {
            gap / closing
        } else {
            f64::INFINITY
        };
        // The stopping condition is *physical*, not a collision budget:
        // the sequence ends when the small block is no longer chasing and
        // the big one is escaping. Both t's infinite is exactly that.
        if !t_wall.is_finite() && !t_hit.is_finite() {
            break;
        }
        let dt = t_wall.min(t_hit);
        if !dt.is_finite() || dt < 0.0 {
            break;
        }
        x1 += v1 * dt;
        x2 += v2 * dt;
        t += dt;
        if t_wall <= t_hit {
            v1 = -v1;
            x1 = 0.0;
        } else {
            // elastic, equal-and-opposite impulse
            let m1 = M_SMALL;
            let m2 = big;
            let u1 = ((m1 - m2) * v1 + 2.0 * m2 * v2) / (m1 + m2);
            let u2 = ((m2 - m1) * v2 + 2.0 * m1 * v1) / (m1 + m2);
            v1 = u1;
            v2 = u2;
            x1 = x2 - W_SMALL;
        }
        collisions += 1;
        if record && events.len() < 400_000 {
            events.push((t, x1, x2, v1, v2));
        }
    }
    let e1 = 0.5 * M_SMALL * v1 * v1 + 0.5 * big * v2 * v2;
    Run {
        collisions,
        events,
        e0,
        e1,
        theta: (M_SMALL / big).sqrt().atan(),
        total_time: t,
    }
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    // ── the table: every ratio, run to completion ──
    let table: Vec<(u32, u64, u64, f64)> = (0..=5)
        .map(|n| {
            let r = run(n, false, None);
            let predicted = galperin(r.theta);
            (n, r.collisions, predicted, r.theta)
        })
        .collect();

    // ── the animation: one ratio, played out ──
    let full = run(SHOW_N, true, None);
    let shown = ((t as f64).clamp(0.0, 1.0) * full.collisions as f64) as u64;
    let live = run(SHOW_N, true, Some(shown.max(1)));

    let big_mass = M_SMALL * 100.0_f64.powi(SHOW_N as i32);
    let (_, x1, x2, v1, v2) = *live
        .events
        .last()
        .unwrap_or(&(0.0, X_SMALL0, X_BIG0, 0.0, V_BIG0));

    // π, computed from the blocks: N·θ
    let pi_from_blocks: Vec<(u32, f64)> = table
        .iter()
        .map(|&(n, c, _, th)| (n, c as f64 * th))
        .collect();
    let best = pi_from_blocks.last().copied().unwrap_or((0, 0.0));

    let de = ((full.e1 - full.e0) / full.e0).abs();
    let biggest = run(5, false, None);
    let de_big = ((biggest.e1 - biggest.e0) / biggest.e0).abs();

    let events = live.events.clone();
    let theta = full.theta;
    let table_c = table.clone();

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            book.rect(
                Rect::new(0.0, 0.0, size.width, size.height),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(6, 6, 10)),
                    (1.0, Color::rgb(12, 11, 16)),
                ]),
            );

            // ── the apparatus ──
            let ax = 60.0_f32;
            let ay = 214.0_f32;
            let aw = 660.0_f32;
            let ah = 190.0_f32;
            book.rrect(
                Rect::new(ax - 16.0, ay - 28.0, ax + aw + 16.0, ay + ah + 20.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            let scale = aw / 9.0;
            let floor = ay + ah - 14.0;
            // the wall
            book.rect(
                Rect::new(ax - 10.0, ay + 10.0, ax, floor),
                alpha(MUTED, 0.75),
            );
            book.line(
                Offset::new(ax, floor),
                Offset::new(ax + aw, floor),
                alpha(MUTED, 0.5),
                1.2,
            );
            // the two blocks — area is mass, on a log scale, or the big one
            // would be a mile wide at 10¹⁰
            let h_small = 34.0_f32;
            let h_big = 34.0 + 16.0 * (SHOW_N as f32 + 1.0);
            book.rect(
                Rect::new(
                    ax + (x1 as f32) * scale,
                    floor - h_small,
                    ax + ((x1 + W_SMALL) as f32) * scale,
                    floor,
                ),
                alpha(CYAN_SOFT, 0.92),
            );
            book.rect(
                Rect::new(
                    ax + (x2 as f32) * scale,
                    floor - h_big,
                    ax + ((x2 + W_BIG) as f32) * scale,
                    floor,
                ),
                alpha(VIOLET, 0.92),
            );
            // the small block's recent path — the blur it becomes
            book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                let tail = events.len().saturating_sub(260);
                for e in &events[tail..] {
                    g.line(
                        Offset::new(ax + (e.1 as f32) * scale, floor - 6.0),
                        Offset::new(ax + (e.1 as f32) * scale, floor - h_small),
                        alpha(CYAN, 0.06),
                        1.0,
                    );
                }
            });

            // ── the phase circle: the proof, drawn ──
            //
            // In coordinates (√m·v₁, √M·v₂) energy conservation is a circle
            // and each collision is a REFLECTION in one of two lines. The
            // chord sequence therefore steps around the circle by a
            // constant angle 2θ, and the run ends when it has swept π.
            let cx = 900.0_f32;
            let cy = 360.0_f32;
            let cr = 150.0_f32;
            book.rrect(
                Rect::new(cx - 190.0, cy - 190.0, cx + 190.0, cy + 190.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            book.ring(Offset::new(cx, cy), cr, 1.0, alpha(MUTED, 0.35));
            // the two mirrors: v₁ = 0 (the wall) and the equal-velocity line
            book.line(
                Offset::new(cx, cy - cr),
                Offset::new(cx, cy + cr),
                alpha(MINT, 0.35),
                1.0,
            );
            let m_ang = theta as f32;
            book.line(
                Offset::new(cx - cr * m_ang.cos(), cy - cr * m_ang.sin()),
                Offset::new(cx + cr * m_ang.cos(), cy + cr * m_ang.sin()),
                alpha(AMBER, 0.35),
                1.0,
            );
            // the actual state points, from the run
            let r0 = (2.0 * full.e0).sqrt();
            let mut p = Path::new();
            for (i, e) in events.iter().enumerate() {
                let px = cx + (M_SMALL.sqrt() * e.3 / r0) as f32 * cr;
                let py = cy - (big_mass.sqrt() * e.4 / r0) as f32 * cr;
                let o = Offset::new(px, py);
                if i == 0 {
                    p.move_to(o);
                } else {
                    p.line_to(o);
                }
            }
            book.stroke(p, alpha(CYAN, 0.22), 0.8);
            if let Some(e) = events.last() {
                book.circle(
                    Offset::new(
                        cx + (M_SMALL.sqrt() * e.3 / r0) as f32 * cr,
                        cy - (big_mass.sqrt() * e.4 / r0) as f32 * cr,
                    ),
                    3.4,
                    alpha(AMBER, 0.95),
                );
            }

            // ── the table ──
            let tx = 60.0_f32;
            let ty = 470.0_f32;
            let tw = 660.0_f32;
            let th = 196.0_f32;
            book.rrect(
                Rect::new(tx - 16.0, ty - 28.0, tx + tw + 16.0, ty + th + 16.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            // a bar per row, length ∝ log(collisions)
            let maxlog = (table_c.last().map(|r| r.1).unwrap_or(1) as f32).ln().max(1.0);
            for (i, &(_, c, pred, _)) in table_c.iter().enumerate() {
                let y = ty + 8.0 + i as f32 * (th / table_c.len() as f32);
                let h = th / table_c.len() as f32 - 12.0;
                let w = (c as f32).ln() / maxlog * (tw - 400.0);
                book.rect(
                    Rect::new(tx + 372.0, y, tx + 372.0 + w, y + h),
                    alpha(if c == pred { MINT } else { RED }, 0.55),
                );
            }
        }),
    );

    let lines = vec![
        "PIDIGITS · THE COUNTING AXIS · π, BANGED OUT OF TWO BLOCKS (GALPERIN, 2003)".to_string(),
        format!(
            "elastic collisions only, event-driven with exact collision times · small block m = {M_SMALL}, big block M = 100ⁿ·m · no circle appears anywhere in the mechanics"
        ),
        format!(
            "PLAYING n = {SHOW_N} (M/m = {:.0}): collision {shown} of {} · small block v₁ = {v1:+.4}, big block v₂ = {v2:+.4} · the run ends when the big block outruns the small one",
            big_mass, full.collisions
        ),
        format!(
            "θ = arctan√(m/M) = {theta:.3e} rad · ⌈π/θ⌉ − 1 = {} · the machine counted {} — the count IS the digits",
            galperin(theta),
            full.collisions
        ),
        format!(
            "π FROM THE BLOCKS (N·θ, the count times its own angle): {}",
            pi_from_blocks
                .iter()
                .map(|(n, p)| format!("n={n}: {p:.6}"))
                .collect::<Vec<_>>()
                .join("  ")
        ),
        format!(
            "— the best of them, n = {}, gives π = {:.9} against 3.141592653… : error {:.2e}, which is {:.1e} of π. The blocks are accurate to one part in N.",
            best.0,
            best.1,
            (best.1 - std::f64::consts::PI).abs(),
            (best.1 - std::f64::consts::PI).abs() / std::f64::consts::PI
        ),
        format!(
            "THE BOOKS: |ΔE/E| = {de:.2e} over the n = {SHOW_N} run, and {de_big:.2e} over the n = 5 run's {} collisions in f64 — no energy was invented anywhere in {} digits of π",
            biggest.collisions,
            PI_DIGITS.len().min(6)
        ),
    ];

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(42.0)
                .top(32.0 + i as f32 * 15.0)
                .width(1200.0)
                .height(15.0)
                .child(
                    Text::new(line.clone()).style(
                        TextStyle::new(if i == 0 { 12.0 } else { 11.0 })
                            .monospace()
                            .letter_spacing(if i == 0 { 1.8 } else { 0.0 })
                            .color(alpha(if i == 0 { MUTED } else { mix(MUTED, INK, 0.45) }, 0.95)),
                    ),
                ),
        );
    }
    // the table's text rows
    for (i, &(n, c, pred, _)) in table.iter().enumerate() {
        let y = 470.0 + 8.0 + i as f32 * (196.0 / table.len() as f32);
        stack = stack.push(
            Positioned::new().left(66.0).top(y).width(440.0).height(14.0).child(
                Text::new(format!(
                    "M/m = 100^{n} = 10^{:<2}  counted {c:>7}  ⌈π/θ⌉−1 {pred:>7}  π: {}",
                    2 * n,
                    &PI_DIGITS[..(n as usize + 1).min(PI_DIGITS.len())]
                ))
                .style(
                    TextStyle::new(10.0)
                        .monospace()
                        .color(alpha(if c == pred { mix(MINT, INK, 0.4) } else { RED }, 0.95)),
                ),
            ),
        );
    }
    for (x, y, s) in [
        (60.0_f32, 186.0_f32, "THE APPARATUS — wall, small block, big block · nothing else".to_string()),
        (748.0, 186.0, "THE PHASE CIRCLE — energy is the circle, each collision a reflection".to_string()),
        (60.0, 442.0, "EVERY RATIO, RUN TO COMPLETION — counted vs ⌈π/θ⌉−1 vs the digits".to_string()),
    ] {
        stack = stack.push(
            Positioned::new().left(x).top(y).width(560.0).height(14.0).child(
                Text::new(s).style(
                    TextStyle::new(9.5)
                        .monospace()
                        .letter_spacing(0.9)
                        .color(alpha(MUTED, 0.85)),
                ),
            ),
        );
    }
    stack.into()
}
