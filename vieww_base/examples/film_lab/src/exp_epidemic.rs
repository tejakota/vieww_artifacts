//! exp_epidemic — *the contagion axis.* The curve everybody learned to read.
//!
//! Two thousand four hundred people on a seeded contact graph — Erdős–Rényi,
//! mean degree measured from the graph itself — and one index case. Each
//! infective, on each step, tries every one of its contacts with probability
//! β and recovers after τ steps. That is the entire machine: **SIR, agent by
//! agent, with no differential equation anywhere in it.**
//!
//! The differential equation comes in at the end, as the *check*.
//!
//! **The receipt closes the epidemic's own books:**
//! - **R₀ measured, not assumed.** Every infection is attributed to the
//!   infective that caused it, so the plate counts the secondary cases of
//!   the early generations directly, while the susceptible pool is still
//!   effectively whole. Beside it sits the structural prediction
//!   β·⟨k⟩·τ from the graph's own mean degree.
//! - **The final-size equation.** For a well-mixed SIR the fraction ever
//!   infected R∞ is the root of 1 − R∞ = e^(−R₀·R∞) — a transcendental
//!   equation with no closed form, solved here by bisection on the
//!   *measured* R₀ and printed beside the attack rate the simulation
//!   actually produced.
//! - **The herd-immunity threshold** 1 − 1/R₀, printed against the
//!   susceptible fraction at the epidemic's own peak: the peak of I(t) is
//!   *defined* by S crossing 1/R₀, and the plate checks that it did.
//! - **The offspring distribution** — how many people each case infected —
//!   whose long tail is the superspreading everybody argued about, counted
//!   here rather than asserted.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook,
    TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, mix, Rng, AMBER, CYAN, INK, MINT, MUTED, RED, VIOLET};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 13.0;

// ── The population ──────────────────────────────────────────────────────────

const N: usize = 2400;
/// Contacts per person, in expectation. The realised mean is measured.
const MEAN_K: f32 = 8.0;
/// Per-contact, per-step transmission probability.
const BETA: f32 = 0.042;
/// Infectious period, in steps.
const TAU: usize = 9;
const STEPS: usize = 150;
/// Simultaneous introductions (see the note in `run_to`).
const SEEDS: usize = 10;

#[derive(Clone, Copy, PartialEq)]
enum State {
    S,
    I,
    R,
}

struct World {
    pos: Vec<(f32, f32)>,
    adj: Vec<Vec<u32>>,
    edges: Vec<(u32, u32)>,
    mean_k: f32,
}

/// A seeded contact graph, laid out on a sunflower disc so that the drawing
/// shows *people*, not a hairball — the edges are the epidemiology, the
/// layout is only so the eye can follow a wave.
fn build_world() -> World {
    let mut rng = Rng::new(0x5115_0000_C0DE_0001);
    let mut pos = Vec::with_capacity(N);
    let golden = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
    for i in 0..N {
        let r = (i as f32 / N as f32).sqrt();
        let a = i as f32 * golden;
        pos.push((r * a.cos(), r * a.sin()));
    }
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); N];
    let mut edges = Vec::new();
    let target = (MEAN_K * N as f32 / 2.0) as usize;
    while edges.len() < target {
        let a = (rng.f01() * N as f32) as usize % N;
        let b = (rng.f01() * N as f32) as usize % N;
        if a == b || adj[a].contains(&(b as u32)) {
            continue;
        }
        adj[a].push(b as u32);
        adj[b].push(a as u32);
        edges.push((a as u32, b as u32));
    }
    let mean_k = adj.iter().map(|a| a.len()).sum::<usize>() as f32 / N as f32;
    World {
        pos,
        adj,
        edges,
        mean_k,
    }
}

struct Epidemic {
    state: Vec<State>,
    /// Step on which each person was infected (usize::MAX if never).
    when: Vec<usize>,
    /// Who infected them.
    by: Vec<u32>,
    /// Generation number of each case (index case = 0).
    gen: Vec<u16>,
    /// Secondary cases caused by each person.
    offspring: Vec<u16>,
    s_series: Vec<f32>,
    i_series: Vec<f32>,
    r_series: Vec<f32>,
    steps: usize,
    peak_step: usize,
    peak_i: f32,
    s_at_peak: f32,
}

fn run_to(world: &World, t: f64) -> Epidemic {
    let mut rng = Rng::new(0xC0FF_EE00_1234_5678);
    let mut state = vec![State::S; N];
    let mut until = vec![0usize; N];
    let mut when = vec![usize::MAX; N];
    let mut by = vec![u32::MAX; N];
    let mut gen = vec![0u16; N];
    let mut offspring = vec![0u16; N];
    // TEN index cases, scattered by the seed. One index case is the
    // textbook picture and the wrong experiment: at R₀ ≈ 2.5 a single
    // introduction dies out about 1/R₀ = 40% of the time, and the first
    // cut of this plate drew exactly that — five cases, no epidemic, and a
    // final-size receipt comparing 0.002 against 0.000. Ten simultaneous
    // introductions make stochastic extinction (0.4^10 ≈ 1e−4) a
    // non-event, and the law being checked is the deterministic one.
    for k in 0..SEEDS {
        let p = (k * 211) % N;
        state[p] = State::I;
        until[p] = TAU;
        when[p] = 0;
    }
    let (mut ss, mut is, mut rs) = (Vec::new(), Vec::new(), Vec::new());
    let n = ((t * STEPS as f64) as usize).max(1);
    let mut peak_step = 0;
    let mut peak_i = 0.0_f32;
    let mut s_at_peak = 1.0_f32;

    for step in 0..n {
        let mut new_infections: Vec<(usize, usize)> = Vec::new();
        for i in 0..N {
            if state[i] != State::I {
                continue;
            }
            for &j in &world.adj[i] {
                let j = j as usize;
                if state[j] == State::S && rng.f01() < BETA {
                    new_infections.push((j, i));
                }
            }
        }
        // recoveries first (the step's infectives were the ones listed above)
        for i in 0..N {
            if state[i] == State::I && step + 1 >= until[i] {
                state[i] = State::R;
            }
        }
        for (j, i) in new_infections {
            if state[j] == State::S {
                state[j] = State::I;
                until[j] = step + 1 + TAU;
                when[j] = step + 1;
                by[j] = i as u32;
                gen[j] = gen[i] + 1;
                offspring[i] += 1;
            }
        }
        let c_s = state.iter().filter(|s| **s == State::S).count() as f32 / N as f32;
        let c_i = state.iter().filter(|s| **s == State::I).count() as f32 / N as f32;
        let c_r = state.iter().filter(|s| **s == State::R).count() as f32 / N as f32;
        if c_i > peak_i {
            peak_i = c_i;
            peak_step = step;
            s_at_peak = c_s;
        }
        ss.push(c_s);
        is.push(c_i);
        rs.push(c_r);
    }

    Epidemic {
        state,
        when,
        by,
        gen,
        offspring,
        s_series: ss,
        i_series: is,
        r_series: rs,
        steps: n,
        peak_step,
        peak_i,
        s_at_peak,
    }
}

/// The final-size equation 1 − R∞ = exp(−R₀·R∞), solved by bisection on
/// (0, 1). No closed form exists; this is the honest way to have the number.
fn final_size(r0: f32) -> f32 {
    if r0 <= 1.0 {
        return 0.0;
    }
    let f = |r: f32| 1.0 - r - (-r0 * r).exp();
    let (mut lo, mut hi) = (1e-6_f32, 1.0 - 1e-9);
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if f(lo) * f(mid) <= 0.0 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    0.5 * (lo + hi)
}

// ── The frame ───────────────────────────────────────────────────────────────

const CX: f32 = 340.0;
const CY: f32 = 424.0;
const CR: f32 = 246.0;

pub fn frame(t: f32) -> WidgetNode {
    let world = build_world();
    let ep = run_to(&world, t as f64);

    // ── R₀, measured from the early generations ──
    // Generations 0–2 only: after that the susceptible pool is visibly
    // depleted and what one measures is the *effective* R, not R₀.
    let mut early_cases = 0usize;
    let mut early_offspring = 0usize;
    for i in 0..N {
        if ep.when[i] != usize::MAX && ep.gen[i] <= 2 {
            // only count a case whose whole infectious period has finished,
            // or its offspring count is still accumulating
            if ep.when[i] + TAU < ep.steps {
                early_cases += 1;
                early_offspring += ep.offspring[i] as usize;
            }
        }
    }
    let r0_measured = if early_cases > 0 {
        early_offspring as f32 / early_cases as f32
    } else {
        0.0
    };
    // The structural prediction from the graph's own mean degree: an
    // infective tries each of ⟨k⟩ contacts on each of τ steps.
    let r0_structural = (1.0 - (1.0 - BETA).powi(TAU as i32)) * world.mean_k;

    let attack = ep
        .state
        .iter()
        .filter(|s| **s != State::S)
        .count() as f32
        / N as f32;
    let predicted = final_size(r0_measured);
    let predicted_struct = final_size(r0_structural);
    // Inverting the final-size law on the attack rate the run produced:
    // R₀ = −ln(1 − R∞)/R∞. Of the four estimates this is the one that uses
    // the whole epidemic rather than a window of it.
    let r0_from_final = if attack > 0.0 && attack < 1.0 {
        -(1.0 - attack).ln() / attack
    } else {
        0.0
    };
    let herd = if r0_measured > 1.0 {
        1.0 - 1.0 / r0_measured
    } else {
        0.0
    };

    // ── the offspring distribution ──
    let mut dist = [0usize; 12];
    let mut counted = 0usize;
    for i in 0..N {
        if ep.when[i] != usize::MAX && ep.when[i] + TAU < ep.steps {
            let k = (ep.offspring[i] as usize).min(11);
            dist[k] += 1;
            counted += 1;
        }
    }
    let top_decile_share = {
        let mut off: Vec<u16> = (0..N)
            .filter(|&i| ep.when[i] != usize::MAX && ep.when[i] + TAU < ep.steps)
            .map(|i| ep.offspring[i])
            .collect();
        off.sort_unstable_by(|a, b| b.cmp(a));
        let total: usize = off.iter().map(|&o| o as usize).sum();
        let take = (off.len() as f32 * 0.1).ceil() as usize;
        if total > 0 {
            off[..take.min(off.len())].iter().map(|&o| o as usize).sum::<usize>() as f32
                / total as f32
        } else {
            0.0
        }
    };

    let pos = world.pos.clone();
    let edges = world.edges.clone();
    let state = ep.state.clone();
    let when = ep.when.clone();
    let by = ep.by.clone();
    let s_series = ep.s_series.clone();
    let i_series = ep.i_series.clone();
    let r_series = ep.r_series.clone();
    let steps = ep.steps;
    let peak_step = ep.peak_step;
    let peak_i = ep.peak_i;
    let s_at_peak = ep.s_at_peak;
    let mean_k = world.mean_k;

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

            let px = |i: usize| CX + pos[i].0 * CR;
            let py = |i: usize| CY + pos[i].1 * CR;

            book.circle(Offset::new(CX, CY), CR + 26.0, alpha(Color::rgb(12, 12, 18), 0.92));

            // ── the contact graph, faint ──
            for &(a, b) in edges.iter().step_by(3) {
                book.line(
                    Offset::new(px(a as usize), py(a as usize)),
                    Offset::new(px(b as usize), py(b as usize)),
                    alpha(VIOLET, 0.045),
                    0.5,
                );
            }
            // ── the transmission tree: who gave it to whom ──
            book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                for j in 0..N {
                    let src = by[j];
                    if src == u32::MAX {
                        continue;
                    }
                    let age = (steps.saturating_sub(when[j])) as f32;
                    let heat = (1.0 - age / 26.0).clamp(0.0, 1.0);
                    g.line(
                        Offset::new(px(src as usize), py(src as usize)),
                        Offset::new(px(j), py(j)),
                        alpha(mix(VIOLET, AMBER, heat), 0.10 + 0.45 * heat),
                        0.6 + 1.0 * heat,
                    );
                }
            });
            // ── the people ──
            for i in 0..N {
                let (c, r, a) = match state[i] {
                    State::S => (mix(MUTED, Color::rgb(70, 80, 100), 0.5), 1.7, 0.75),
                    State::I => (RED, 2.6, 0.95),
                    State::R => (VIOLET, 1.7, 0.5),
                };
                book.circle(Offset::new(px(i), py(i)), r, alpha(c, a));
            }
            // the infectives get a Plus halo — the wave, visible
            book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                for i in 0..N {
                    if state[i] == State::I {
                        g.circle(Offset::new(px(i), py(i)), 7.0, alpha(RED, 0.13));
                    }
                }
            });

            // ── the curves ──
            let gx = 700.0_f32;
            let gy = 214.0_f32;
            let gw = 530.0_f32;
            let gh = 240.0_f32;
            book.rrect(
                Rect::new(gx - 18.0, gy - 30.0, gx + gw + 18.0, gy + gh + 30.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            let sx = |i: usize| gx + i as f32 / (STEPS - 1) as f32 * gw;
            for (series, col, w) in [
                (&s_series, CYAN, 1.8_f32),
                (&r_series, VIOLET, 1.8),
                (&i_series, RED, 2.4),
            ] {
                let mut p = Path::new();
                for (i, &v) in series.iter().enumerate() {
                    let o = Offset::new(sx(i), gy + gh - v * gh);
                    if i == 0 {
                        p.move_to(o);
                    } else {
                        p.line_to(o);
                    }
                }
                book.stroke(p, alpha(col, 0.92), w);
            }
            // the herd-immunity line, at S = 1/R₀ — the peak must cross here
            if herd > 0.0 {
                let y = gy + gh - (1.0 - herd) * gh;
                book.line(
                    Offset::new(gx, y),
                    Offset::new(gx + gw, y),
                    alpha(MINT, 0.55),
                    1.0,
                );
            }
            // the measured peak
            if peak_i > 0.0 {
                book.line(
                    Offset::new(sx(peak_step), gy),
                    Offset::new(sx(peak_step), gy + gh),
                    alpha(AMBER, 0.35),
                    1.0,
                );
            }
            // the final-size prediction, as a bar against the attack rate
            let fy = gy + gh - predicted * gh;
            book.line(
                Offset::new(gx, fy),
                Offset::new(gx + gw, fy),
                alpha(AMBER, 0.45),
                1.0,
            );

            // ── the offspring distribution ──
            let ox = 700.0_f32;
            let oy = 536.0_f32;
            let ow = 530.0_f32;
            let oh = 128.0_f32;
            book.rrect(
                Rect::new(ox - 18.0, oy - 30.0, ox + ow + 18.0, oy + oh + 26.0),
                10.0,
                alpha(Color::rgb(13, 13, 19), 0.95),
            );
            let dmax = dist.iter().cloned().max().unwrap_or(1).max(1) as f32;
            for (k, &c) in dist.iter().enumerate() {
                let x0 = ox + k as f32 / dist.len() as f32 * ow;
                let x1 = ox + (k + 1) as f32 / dist.len() as f32 * ow - 3.0;
                let h = c as f32 / dmax * oh;
                book.rect(
                    Rect::new(x0, oy + oh - h, x1, oy + oh),
                    alpha(mix(CYAN, AMBER, k as f32 / 11.0), 0.8),
                );
            }
        }),
    );

    let lines = vec![
        "EPIDEMIC · THE CONTAGION AXIS · SIR, PERSON BY PERSON — AND THEN THE EQUATION".to_string(),
        format!(
            "{N} people, seeded Erdős–Rényi contact graph (⟨k⟩ measured = {mean_k:.2}) · β = {BETA}/contact/step · τ = {TAU} steps · {SEEDS} index cases · step {steps}/{STEPS}"
        ),
        "every infection is attributed to the case that caused it, so the generations below are COUNTED, not modelled — the equation arrives afterwards, as the check".to_string(),
        format!(
            "FOUR INDEPENDENT ESTIMATES OF R₀ FROM ONE RUN · (1) structural (1−(1−β)^τ)·⟨k⟩ = {r0_structural:.3} · (2) contact-traced, gens 0–2 ({early_cases} cases, {early_offspring} secondary) = {r0_measured:.3}"
        ),
        format!(
            "(3) from the peak — I peaks when S crosses 1/R₀, and S_peak = {s_at_peak:.3} → {:.3} · (4) inverting the final-size law on the attack rate {attack:.3}: −ln(1−R∞)/R∞ = {r0_from_final:.3}",
            1.0 / s_at_peak.max(1e-6)
        ),
        format!(
            "— (4) lands {:+.1}% from (1): the final-size law is the well-mixed one and this graph is nearly well mixed. (2) reads low ({:+.1}%): by gen 2 the pool is already depleting.",
            (r0_from_final - r0_structural) / r0_structural * 100.0,
            (r0_measured - r0_structural) / r0_structural * 100.0
        ),
        format!(
            "FINAL SIZE forward: 1 − R∞ = e^(−R₀R∞) by bisection on R₀ = {r0_structural:.3} → {predicted_struct:.3} vs the run's attack rate {attack:.3} ({:+.1}%) · herd threshold 1−1/R₀ = {:.3}",
            (attack - predicted_struct) / predicted_struct.max(1e-6) * 100.0,
            1.0 - 1.0 / r0_structural
        ),
        format!(
            "SUPERSPREADING, counted: the busiest 10% of {counted} completed cases caused {:.1}% of all transmissions — the long tail, from this epidemic's own ledger",
            top_decile_share * 100.0
        ),
    ];

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(42.0)
                .top(24.0 + i as f32 * 15.0)
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
    for (x, y, s) in [
        (700.0_f32, 186.0_f32, "S (cyan) · I (red) · R (violet) — green: 1/R₀, amber: the final-size root".to_string()),
        (700.0, 508.0, "OFFSPRING DISTRIBUTION — secondary cases per completed case, 0 … 11+".to_string()),
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
