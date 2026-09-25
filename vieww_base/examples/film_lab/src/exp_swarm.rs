//! exp_swarm — *the simulation axis.* A murmuration, computed, not faked.
//!
//! Every crowd before this plate was closed-form: `galaxy`'s stars moved on
//! analytic curves, `city`'s windows answered a hash. This one is **2,400
//! boids advanced by a real flocking integrator** — separation, alignment,
//! cohesion, a travelling roost, a predator's pass — thirty steps of
//! dt = 1/48 s **per frame**, with a spatial hash so the neighbour field is
//! O(n), not O(n²). The build/raster split the harness now measures is
//! this plate's reason for existing: the frame budget's simulation half
//! had never been on a receipt before.
//!
//! The scene: dusk, a dead tree on a rise, the starling cloud wheeling
//! between sky and land, wheeling harder when the kite crosses. Fast
//! birds bucket into three direction-blurred `Filtered` groups (the
//! ghosts' economy); the slow belly of the flock stays crisp.

use std::sync::Mutex;

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Filtered, Painting, PaintWith, Text};

use crate::film_lib::{alpha, mix, Rng, BG_DEEP, INK, MUTED, VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 10.0;

/// The flock's size — the axis.
const N: usize = 2400;

/// Simulation step, seconds (the integrator's own clock).
const DT: f32 = 1.0 / 48.0;

/// Perception radius (alignment + cohesion), px.
const PERCEPTION: f32 = 30.0;

/// Separation radius, px.
const SEP_R: f32 = 13.0;

/// Speed bounds, px/s.
const V_MIN: f32 = 46.0;
const V_MAX: f32 = 135.0;

/// The spatial hash's cell — the perception radius, so the 3×3 neighbourhood
/// is exact and nothing is missed.
const CELL: f32 = PERCEPTION;

// ── The simulation ──────────────────────────────────────────────────────────

struct Swarm {
    pos: Vec<[f32; 2]>,
    vel: Vec<[f32; 2]>,
    seedz: Vec<f32>,
    sim_t: f32,
    steps: u64,
    queries: u64,
    last_t: f32,
}

static SWARM: Mutex<Option<Swarm>> = Mutex::new(None);

fn init_swarm() -> Swarm {
    let mut rng = Rng::new(0x5A71);
    let mut pos = Vec::with_capacity(N);
    let mut vel = Vec::with_capacity(N);
    let mut seedz = Vec::with_capacity(N);
    for _ in 0..N {
        // The flock starts as one loose cloud, mid-sky, drifting.
        let a = rng.f01() * std::f32::consts::TAU;
        let r = rng.f01().sqrt();
        pos.push([640.0 + (a.cos() * r) * 190.0, 250.0 + (a.sin() * r) * 110.0]);
        let sp = 70.0 + rng.f01() * 50.0;
        let d = rng.f01() * std::f32::consts::TAU;
        vel.push([d.cos() * sp, d.sin() * sp]);
        seedz.push(rng.f01());
    }
    Swarm { pos, vel, seedz, sim_t: 0.0, steps: 0, queries: 0, last_t: -1.0 }
}

/// The roost attractor — a slow lissajous the cloud follows.
#[must_use]
fn roost_at(sim_t: f32) -> [f32; 2] {
    [
        640.0 + (sim_t * 0.55).sin() * 360.0,
        262.0 + (sim_t * 0.37).cos() * 96.0,
    ]
}

/// The predator — one pass, left to right, mid-plate.
#[must_use]
fn predator_at(sim_t: f32) -> Option<[f32; 2]> {
    let start = 4.2;
    let dur = 2.0;
    if sim_t < start || sim_t > start + dur {
        return None;
    }
    let u = (sim_t - start) / dur;
    Some([120.0 + u * 1040.0, 220.0 + (u * 6.2832 * 1.5).sin() * 60.0])
}

/// Advance the sim to `target_t` (film seconds), stepping `DT`.
/// Returns (steps_this_call, neighbour_queries_this_call).
fn advance(target_t: f32) -> (u64, u64) {
    let mut guard = SWARM.lock().expect("swarm");
    let mut sw = guard.take().unwrap_or_else(init_swarm);

    // A re-run rewinds t — restart the world, keep the determinism.
    if target_t < sw.sim_t - 1e-6 {
        sw = init_swarm();
    }
    let mut queries = 0u64;
    let steps_before = sw.steps;

    while sw.sim_t < target_t - 1e-9 {
        let sim_t = sw.sim_t;

        // The spatial hash, rebuilt each step.
        let (gw, gh) = ((1280.0 / CELL).ceil() as usize, (560.0 / CELL).ceil() as usize);
        let mut grid: Vec<Vec<u32>> = vec![Vec::new(); gw * gh];
        for (i, p) in sw.pos.iter().enumerate() {
            let gx = ((p[0] / CELL).floor() as usize).min(gw - 1);
            let gy = ((p[1] / CELL).floor() as usize).min(gh - 1);
            grid[gy * gw + gx].push(i as u32);
        }

        let roost = roost_at(sim_t);
        let pred = predator_at(sim_t);

        let mut acc = vec![[0.0f32; 2]; N];
        for i in 0..N {
            let pi = sw.pos[i];
            let gx = ((pi[0] / CELL).floor() as usize).min(gw - 1);
            let gy = ((pi[1] / CELL).floor() as usize).min(gh - 1);
            let (mut sep_x, mut sep_y) = (0.0f32, 0.0f32);
            let (mut ali_x, mut ali_y) = (0.0f32, 0.0f32);
            let (mut coh_x, mut coh_y) = (0.0f32, 0.0f32);
            let mut n = 0usize;
            for yy in gy.saturating_sub(1)..=(gy + 1).min(gh - 1) {
                for xx in gx.saturating_sub(1)..=(gx + 1).min(gw - 1) {
                    for &j in &grid[yy * gw + xx] {
                        let jj = j as usize;
                        if jj == i {
                            continue;
                        }
                        queries += 1;
                        let d0 = sw.pos[jj][0] - pi[0];
                        let d1 = sw.pos[jj][1] - pi[1];
                        let d2 = d0 * d0 + d1 * d1;
                        if d2 > PERCEPTION * PERCEPTION {
                            continue;
                        }
                        n += 1;
                        ali_x += sw.vel[jj][0];
                        ali_y += sw.vel[jj][1];
                        coh_x += sw.pos[jj][0];
                        coh_y += sw.pos[jj][1];
                        if d2 < SEP_R * SEP_R && d2 > 1e-4 {
                            let d = d2.sqrt();
                            sep_x -= d0 / d * (SEP_R - d);
                            sep_y -= d1 / d * (SEP_R - d);
                        }
                    }
                }
            }
            let mut ax = 0.0f32;
            let mut ay = 0.0f32;
            if n > 0 {
                ax += sep_x * 1.9;
                ay += sep_y * 1.9;
                ax += (ali_x / n as f32 - sw.vel[i][0]) * 1.05;
                ay += (ali_y / n as f32 - sw.vel[i][1]) * 1.05;
                ax += (coh_x / n as f32 - pi[0]) * 0.0032;
                ay += (coh_y / n as f32 - pi[1]) * 0.0032;
            }
            // The roost — gentle.
            ax += (roost[0] - pi[0]) * 0.0021;
            ay += (roost[1] - pi[1]) * 0.0021;
            // The predator — overwhelming, close-in only.
            if let Some(pr) = pred {
                let dx = pi[0] - pr[0];
                let dy = pi[1] - pr[1];
                let d2 = dx * dx + dy * dy;
                if d2 < 150.0 * 150.0 {
                    let d = d2.sqrt().max(1.0);
                    let k = (1.0 - d / 150.0) * 900.0;
                    ax += dx / d * k;
                    ay += dy / d * k;
                }
            }
            acc[i] = [ax, ay];
        }

        // Integrate: semi-implicit Euler, clamped speed.
        for i in 0..N {
            sw.vel[i][0] = (sw.vel[i][0] + acc[i][0] * DT).clamp(-V_MAX, V_MAX);
            sw.vel[i][1] = (sw.vel[i][1] + acc[i][1] * DT).clamp(-V_MAX, V_MAX);
            let sp = (sw.vel[i][0].powi(2) + sw.vel[i][1].powi(2)).sqrt();
            if sp < V_MIN {
                let k = V_MIN / sp.max(1e-3);
                sw.vel[i][0] *= k;
                sw.vel[i][1] *= k;
            }
            sw.pos[i][0] += sw.vel[i][0] * DT;
            sw.pos[i][1] += sw.vel[i][1] * DT;
            // Soft walls.
            if sw.pos[i][0] < 24.0 {
                sw.vel[i][0] += (24.0 - sw.pos[i][0]) * 4.0 * DT * 60.0;
            }
            if sw.pos[i][0] > 1256.0 {
                sw.vel[i][0] -= (sw.pos[i][0] - 1256.0) * 4.0 * DT * 60.0;
            }
            if sw.pos[i][1] < 24.0 {
                sw.vel[i][1] += (24.0 - sw.pos[i][1]) * 4.0 * DT * 60.0;
            }
            if sw.pos[i][1] > 556.0 {
                sw.vel[i][1] -= (sw.pos[i][1] - 556.0) * 4.0 * DT * 60.0;
            }
        }
        sw.sim_t += DT;
        sw.steps += 1;
    }

    let steps = sw.steps - steps_before;
    sw.queries += queries;
    let out = (steps, queries);
    *guard = Some(sw);
    out
}

/// The snapshot the renderer draws from (pos + vel, cloned under the lock).
fn snapshot(target_t: f32) -> (Vec<[f32; 2]>, Vec<[f32; 2]>, Vec<f32>, u64, u64) {
    let (steps, queries) = advance(target_t);
    let guard = SWARM.lock().expect("swarm");
    let sw = guard.as_ref().expect("swarm live");
    (
        sw.pos.clone(),
        sw.vel.clone(),
        sw.seedz.clone(),
        steps,
        sw.queries,
    )
}

// ── The frame ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let film = t * SECONDS;
    let (pos, vel, seedz, steps, queries) = snapshot(film);

    // The bucket assignment: by velocity angle, the ghosts' economy. Slow
    // birds (below the blur speed) stay crisp.
    let mut buckets: [Vec<usize>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut crisp: Vec<usize> = Vec::new();
    let mut speed_sum = 0.0f32;
    for (i, v) in vel.iter().enumerate() {
        let sp = (v[0].powi(2) + v[1].powi(2)).sqrt();
        speed_sum += sp;
        if sp < 88.0 {
            crisp.push(i);
        } else {
            let ang = v[1].atan2(v[0]);
            let b = (((ang + std::f32::consts::PI) / std::f32::consts::TAU) * 3.0).floor() as usize;
            buckets[(b as i32).rem_euclid(3) as usize].push(i);
        }
    }
    let mean_speed = speed_sum / N as f32;

    // The blur rides the mean speed — the flock's own weather.
    let sigma = 1.2 + (mean_speed - V_MIN) / (V_MAX - V_MIN) * 3.4;

    // One bird as a widget subtree: an oriented chevron whose depth-plane
    // alpha and size come from its own seed.
    let bird_node = |i: usize| -> WidgetNode {
        let p = pos[i];
        let v = vel[i];
        let sp = (v[0].powi(2) + v[1].powi(2)).sqrt().max(1e-3);
        let hx = v[0] / sp;
        let hy = v[1] / sp;
        let z = seedz[i]; // fake depth: 0 far, 1 near
        let len = 3.4 + z * 3.8;
        let tipx = p[0] + hx * len;
        let tipy = p[1] + hy * len;
        let bx = p[0] - hx * len * 0.45;
        let by = p[1] - hy * len * 0.45;
        let px = -hy;
        let py = hx;
        let wing = len * 0.62;
        let bird = Painting::sized(
            Size::new(18.0, 18.0),
            PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                // Local coordinates: the bird centred at (9, 9).
                let (ox, oy) = (9.0, 9.0);
                let mut path = Path::new();
                path.move_to(Offset::new(ox + (tipx - p[0]), oy + (tipy - p[1])));
                path.line_to(Offset::new(ox + (bx - p[0]) + px * wing, oy + (by - p[1]) + py * wing));
                path.line_to(Offset::new(ox + (bx - p[0]) - px * wing * 0.55, oy + (by - p[1]) - py * wing * 0.55));
                // Closed — the U-22 lesson on its second working day: an
                // open triangle fills chord-closed (pixel-correct) but the
                // census counts it, and 2,411 of them per frame is the
                // noise that hides the petal. The close is free.
                path.close();
                book.fill(path, alpha(INK, 0.42 + 0.5 * z));
            }),
        );
        Positioned::new()
            .left(p[0] - 9.0)
            .top(p[1] - 9.0)
            .width(18.0)
            .height(18.0)
            .child(bird)
            .into()
    };

    // The board — dusk, hills, the dead tree, the predator.
    let pred = predator_at(film);
    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The dusk sky.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(9, 7, 16)),
                    (0.5, Color::rgb(26, 18, 36)),
                    (0.78, Color::rgb(66, 38, 52)),
                    (1.0, Color::rgb(16, 12, 20)),
                ]),
            );
            // A low sun's last warmth on the horizon.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.24, 0.86), 0.42).with_dither().with_stops(&[
                    (0.0, alpha(Color::rgb(224, 120, 70), 0.16)),
                    (1.0, alpha(Color::rgb(224, 120, 70), 0.0)),
                ]),
            );

            // The far hills.
            let mut hills = Path::new();
            hills.move_to(Offset::new(0.0, 574.0));
            let mut rng = Rng::new(0x4411);
            for k in 0..16 {
                let x = k as f32 * 88.0;
                hills.line_to(Offset::new(x, 566.0 - rng.f01() * 30.0));
            }
            hills.line_to(Offset::new(1280.0, 578.0));
            hills.line_to(Offset::new(1280.0, 720.0));
            hills.line_to(Offset::new(0.0, 720.0));
            hills.close();
            book.fill(hills, alpha(Color::rgb(13, 10, 17), 1.0));

            // The dead tree on its rise — a fixed silhouette, the roost.
            let tree_root = Offset::new(1084.0, 596.0);
            let mut tree_rng = Rng::new(0x7EE);
            let mut branch = |book: &mut Sketchbook, from: Offset, dir: f32, len: f32, wdt: f32,
                 depth: u32| {
                let mut stack = Vec::new();
                stack.push((from, dir, len, wdt, depth));
                while let Some((f0, dr, ln, wd, dp)) = stack.pop() {
                    let to = Offset::new(f0.dx + dr.cos() * ln, f0.dy + dr.sin() * ln);
                    let mut p = Path::new();
                    p.move_to(f0);
                    p.line_to(to);
                    book.stroke(p, alpha(Color::rgb(8, 6, 10), 0.96), wd);
                    if dp > 0 {
                        let nch = if dp == 4 { 3 } else { 2 };
                        for _ in 0..nch {
                            let nd = dr + tree_rng.sym() * 0.65;
                            stack.push((to, nd, ln * (0.62 + tree_rng.f01() * 0.16), wd * 0.62,
                                dp - 1));
                        }
                    }
                }
            };
            branch(book, tree_root, -std::f32::consts::FRAC_PI_2, 84.0, 7.0, 5);

            // The ground.
            book.rect(Rect::new(0.0, 596.0, w, h), alpha(Color::rgb(10, 8, 12), 1.0));

            // The predator — a dark kite with a pale belly when it passes.
            if let Some(pr) = pred {
                book.fill(
                    {
                        let mut p = Path::new();
                        p.move_to(Offset::new(pr[0] - 15.0, pr[1] + 4.0));
                        p.line_to(Offset::new(pr[0], pr[1] - 7.0));
                        p.line_to(Offset::new(pr[0] + 15.0, pr[1] + 4.0));
                        p.line_to(Offset::new(pr[0], pr[1] + 1.0));
                        p.close();
                        p
                    },
                    alpha(Color::rgb(12, 10, 12), 1.0),
                );
            }

            // A vignette.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.5), 0.85).with_dither().with_stops(&[
                    (0.0, alpha(Color::BLACK, 0.0)),
                    (1.0, alpha(Color::BLACK, 0.32)),
                ]),
            );
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));

    // The crisp belly of the flock.
    let mut crisp_stack = Stack::new();
    for &i in &crisp {
        crisp_stack = crisp_stack.push(bird_node(i));
    }
    stack = stack.push(crisp_stack);

    // The blurred buckets.
    for (b, list) in buckets.iter().enumerate() {
        if list.is_empty() {
            continue;
        }
        let mut inner = Stack::new();
        for &i in list {
            inner = inner.push(bird_node(i));
        }
        let bucket_angle = (b as f32 + 0.5) / 3.0 * std::f32::consts::TAU
            - std::f32::consts::PI;
        stack = stack.push(
            Positioned::fill().child(
                Filtered::new()
                    .with_blur(sigma)
                    .with_blur_angle(bucket_angle)
                    .child(inner),
            ),
        );
    }

    stack.push(receipt_panel(t, steps, queries, mean_speed, pred.is_some())).into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(t: f32, steps: u64, queries: u64, mean_speed: f32, pred_on: bool) -> WidgetNode {
    let lines = [
        "SWARM · THE SIMULATION AXIS · 2,400 BOIDS".to_string(),
        format!("steps this frame {steps} · dt {DT:.3} s · neighbour queries {queries}"),
        format!("mean speed {mean_speed:.0} px/s (clamp {V_MIN:.0}-{V_MAX:.0})"),
        format!("hash cell {CELL:.0} px · perception {PERCEPTION:.0} px · sep {SEP_R:.0} px"),
        format!("predator {} · build vs raster in metrics", if pred_on { "IN THE FLOCK" } else { "away" }),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 566.0;
    const P_W: f32 = 400.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(P_W)
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

    // The instrument: the speed histogram, drawn live — the flock's own
    // weather gauge, ten bins from V_MIN to V_MAX.
    let strip = Painting::sized(
        Size::new(P_W, 40.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            book.rrect(
                Rect::new(0.0, 0.0, P_W, 40.0),
                8.0,
                alpha(Color::rgb(16, 16, 21), 0.88),
            );
            book.stroke_rrect(
                Rect::new(0.0, 0.0, P_W, 40.0),
                8.0,
                alpha(Color::WHITE, 0.08),
                1.0,
            );
            let guard = SWARM.lock().expect("swarm");
            let sw = guard.as_ref().expect("swarm live");
            let mut bins = [0u32; 10];
            for v in &sw.vel {
                let sp = (v[0].powi(2) + v[1].powi(2)).sqrt();
                let b = (((sp - V_MIN) / (V_MAX - V_MIN)) * 10.0).floor().clamp(0.0, 9.0) as usize;
                bins[b] += 1;
            }
            let peak = bins.iter().copied().max().unwrap_or(1).max(1) as f32;
            for (k, c) in bins.iter().enumerate() {
                let x = 12.0 + k as f32 * ((P_W - 24.0) / 10.0);
                let hgt = 26.0 * *c as f32 / peak;
                book.rect(
                    Rect::new(x, 33.0 - hgt, x + (P_W - 24.0) / 10.0 - 3.0, 33.0),
                    alpha(VIOLET_SOFT, 0.55),
                );
            }
            // The blur threshold tick.
            let tx = 12.0 + ((88.0 - V_MIN) / (V_MAX - V_MIN)).clamp(0.0, 1.0) * (P_W - 24.0);
            book.line(Offset::new(tx, 8.0), Offset::new(tx, 33.0), alpha(INK, 0.6), 1.0);
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 84.0)
            .width(P_W)
            .height(40.0)
            .child(strip),
    );

    stack.into()
}
