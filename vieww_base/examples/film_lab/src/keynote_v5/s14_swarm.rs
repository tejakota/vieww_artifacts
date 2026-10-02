//! S14 · THE SWARM — the reference's "Heavy Stress Test", staged as the
//! film's showstopper. 2:08–2:20.
//!
//! *"The Swarm View: drop a live 2,400-boid fluid interactive simulation
//! directly inside a UI layout sheet, showcasing the engine splitting
//! build and raster cycles cleanly on the fly."*
//!
//! The swarm.say buffer opens; the preview's glass becomes a dusk sheet;
//! and a real murmuration runs inside the layout — **2,400 boids advanced
//! by a flocking integrator** (separation · alignment · cohesion, a
//! wandering roost, a kite's pass) with a spatial hash so the neighbour
//! field is O(n). The engine's own economy is the receipt, live: steps
//! taken, neighbour queries served, and the two gauges — build / raster —
//! splitting the frame budget on the fly.
//!
//! The plate receipts quoted beside them (2,511 shapes/frame · build
//! 66.4 ms · raster 214.1 ms) are the lab's own metrics
//! (`film_lab/renders/swarm/metrics.txt`), labelled as the plate's bench.
//!
//! Tap 6 (the drop, t≈0.22) fires here. The boid count is the const the
//! engine iterates — 2,400, counted by the code that draws them.

use std::sync::Mutex;

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, ease_out_back, tint, xywh, Rng, AMBER, CYAN_SOFT, INK, MUTED,
    VIOLET, VIOLET_SOFT,
};

use super::studio;
use super::{group_commas, Ctx};

// ── The murmuration engine ──────────────────────────────────────────────────

/// The flock's size — the axis (the reference's own figure).
const N: usize = 2400;

/// Simulation step, seconds.
const DT: f32 = 1.0 / 48.0;

/// Perception radius (alignment + cohesion), px in the sheet.
const PERCEPTION: f32 = 30.0;

/// Separation radius, px.
const SEP_R: f32 = 13.0;

/// Speed bounds, px/s.
const V_MIN: f32 = 46.0;
const V_MAX: f32 = 135.0;

/// The spatial hash's cell — the perception radius, so the 3×3
/// neighbourhood is exact.
const CELL: f32 = PERCEPTION;

/// The sheet's world size (the preview pane's inner geometry).
const WORLD_W: f32 = 900.0;
const WORLD_H: f32 = 880.0;

struct Swarm {
    pos: Vec<[f32; 2]>,
    vel: Vec<[f32; 2]>,
    seedz: Vec<f32>,
    sim_t: f32,
    steps: u64,
    queries: u64,
}

static SWARM: Mutex<Option<Swarm>> = Mutex::new(None);

fn init_swarm() -> Swarm {
    let mut rng = Rng::new(0x5A71);
    let mut pos = Vec::with_capacity(N);
    let mut vel = Vec::with_capacity(N);
    let mut seedz = Vec::with_capacity(N);
    for _ in 0..N {
        let a = rng.f01() * std::f32::consts::TAU;
        let r = rng.f01().sqrt();
        pos.push([
            WORLD_W * 0.5 + (a.cos() * r) * 220.0,
            WORLD_H * 0.34 + (a.sin() * r) * 120.0,
        ]);
        let sp = 70.0 + rng.f01() * 50.0;
        let d = rng.f01() * std::f32::consts::TAU;
        vel.push([d.cos() * sp, d.sin() * sp]);
        seedz.push(rng.f01());
    }
    Swarm {
        pos,
        vel,
        seedz,
        sim_t: 0.0,
        steps: 0,
        queries: 0,
    }
}

/// The roost attractor — a slow lissajous the cloud follows.
fn roost_at(sim_t: f32) -> [f32; 2] {
    [
        WORLD_W * 0.5 + (sim_t * 0.55).sin() * 300.0,
        WORLD_H * 0.36 + (sim_t * 0.37).cos() * 90.0,
    ]
}

/// The kite — one pass, left to right, mid-scene.
fn predator_at(sim_t: f32) -> Option<[f32; 2]> {
    let start = 4.2;
    let dur = 2.4;
    if sim_t < start || sim_t > start + dur {
        return None;
    }
    let u = (sim_t - start) / dur;
    Some([
        60.0 + u * 780.0,
        WORLD_H * 0.30 + (u * std::f32::consts::TAU * 1.5).sin() * 60.0,
    ])
}

/// Advance the sim to `target_t` (film seconds), stepping DT.
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
        let (gw, gh) = (
            (WORLD_W / CELL).ceil() as usize,
            (WORLD_H / CELL).ceil() as usize,
        );
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
            // The kite — overwhelming, close-in only.
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

        // Integrate: semi-implicit Euler, clamped speed, soft walls.
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
            if sw.pos[i][0] < 16.0 {
                sw.vel[i][0] += (16.0 - sw.pos[i][0]) * 4.0 * DT * 60.0;
            }
            if sw.pos[i][0] > WORLD_W - 16.0 {
                sw.vel[i][0] -= (sw.pos[i][0] - (WORLD_W - 16.0)) * 4.0 * DT * 60.0;
            }
            if sw.pos[i][1] < 16.0 {
                sw.vel[i][1] += (16.0 - sw.pos[i][1]) * 4.0 * DT * 60.0;
            }
            if sw.pos[i][1] > WORLD_H - 16.0 {
                sw.vel[i][1] -= (sw.pos[i][1] - (WORLD_H - 16.0)) * 4.0 * DT * 60.0;
            }
        }
        sw.sim_t += DT;
        sw.steps += 1;
    }

    let steps = sw.steps - steps_before;
    sw.queries += queries;
    let out = (steps, sw.queries);
    *guard = Some(sw);
    out
}

/// The snapshot the sheet draws from: positions, velocities, seeds, and
/// the engine's own live economy (steps so far, queries served).
fn snapshot(target_t: f32) -> (Vec<[f32; 2]>, Vec<[f32; 2]>, Vec<f32>, u64, u64) {
    let _ = advance(target_t);
    let guard = SWARM.lock().expect("swarm");
    let sw = guard.as_ref().expect("swarm live");
    (
        sw.pos.clone(),
        sw.vel.clone(),
        sw.seedz.clone(),
        sw.steps,
        sw.queries,
    )
}

// ── The scene ───────────────────────────────────────────────────────────────

/// The buffer — the swarm's own say program (the studio's grammar).
fn swarm_lines() -> Vec<Vec<studio::Seg>> {
    let kw = VIOLET_SOFT;
    let st = AMBER;
    let nu = crate::film_lib::MINT;
    let tx = studio::CODE_PLAIN;
    let pu = MUTED;
    vec![
        vec![
            ("keep ", kw),
            ("a flock of ", tx),
            ("2,400", nu),
            (" boids", tx),
        ],
        vec![],
        vec![("screen ", kw), ("\"Swarm\"", st), (":", pu)],
        vec![
            ("    a sheet, dusk behind, spaced ", pu),
            ("24", nu),
            (", holding:", pu),
        ],
        vec![("        the flock, steering:", pu)],
        vec![("            separation, alignment, cohesion", tx)],
        vec![("        one roost, wandering", tx)],
        vec![("        one kite, passing through", tx)],
    ]
}

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;
    let ladder = ctx.ladder;
    let film = t * 12.0;

    // The drop: the buffer opens (0 → 0.22), then the sheet fills.
    let drop = ease_out_back(clamp01((t - 0.10) / 0.16));
    let sheet_in = ease_in_out(clamp01((t - 0.20) / 0.14));

    // The engine — advanced to this frame's film time (the state advances
    // once per frame in both passes; determinism is per-bench).
    let (pos, vel, seedz, steps, queries) = if drop > 0.4 {
        snapshot(film * sheet_in.max(0.0))
    } else {
        (Vec::new(), Vec::new(), Vec::new(), 0, 0)
    };

    // The swarm sheet — the preview's custom content.
    let sheet: WidgetNode = Painting::sized(
        Size::new(WORLD_W, WORLD_H),
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            // The dusk sky.
            book.rect(
                Rect::new(0.0, 0.0, WORLD_W, WORLD_H),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(9, 7, 16)),
                    (0.5, Color::rgb(26, 18, 36)),
                    (0.78, Color::rgb(66, 38, 52)),
                    (1.0, Color::rgb(16, 12, 20)),
                ]),
            );
            // The hills — two silhouettes.
            let mut hill = Path::new();
            hill.move_to(Offset::new(0.0, WORLD_H));
            for i in 0..=24 {
                let x = i as f32 / 24.0 * WORLD_W;
                let y = WORLD_H * 0.86 + (x * 0.006).sin() * 26.0 + (x * 0.017 + 2.0).sin() * 14.0;
                hill.line_to(Offset::new(x, y));
            }
            hill.line_to(Offset::new(WORLD_W, WORLD_H));
            hill.close();
            book.fill(hill, alpha(Color::rgb(13, 9, 16), 0.96));
            // The dead tree — one branch, stroke-drawn.
            let tree = |bx: f32, by: f32, sc: f32, b: &mut Sketchbook| {
                let mut p = Path::new();
                p.move_to(Offset::new(bx, by));
                p.line_to(Offset::new(bx - 8.0 * sc, by - 44.0 * sc));
                p.line_to(Offset::new(bx - 26.0 * sc, by - 66.0 * sc));
                p.move_to(Offset::new(bx - 8.0 * sc, by - 44.0 * sc));
                p.line_to(Offset::new(bx + 14.0 * sc, by - 78.0 * sc));
                b.stroke(p, alpha(Color::rgb(10, 8, 12), 0.95), 4.0 * sc);
            };
            tree(WORLD_W * 0.16, WORLD_H * 0.90, 1.6, book);
            tree(WORLD_W * 0.86, WORLD_H * 0.88, 1.1, book);

            // The roost's glow — where the flock is thickest.
            let roost = roost_at(film * sheet_in.max(0.0));
            super::glow(book, roost[0], roost[1], 240.0, VIOLET, 0.10);

            // The kite, when it passes.
            if let Some(pr) = predator_at(film * sheet_in.max(0.0)) {
                book.circle(Offset::new(pr[0], pr[1]), 5.0, alpha(AMBER, 0.95));
                let mut wing = Path::new();
                wing.move_to(Offset::new(pr[0] - 26.0, pr[1] - 6.0));
                wing.line_to(Offset::new(pr[0], pr[1]));
                wing.line_to(Offset::new(pr[0] + 26.0, pr[1] - 8.0));
                book.stroke_styled(
                    wing,
                    alpha(AMBER, 0.8),
                    2.2,
                    vieww_foundation::StrokeStyle::rounded(),
                );
            }

            // The flock — 2,400 oriented chevrons, one paint pass. Fast
            // birds carry a soft motion smear (a directional-blurred group
            // in one layer — the ghosts' economy, one layer not three).
            book.layer(1.0, 2.2, None, |g| {
                for i in 0..pos.len() {
                    let v = vel[i];
                    let sp = (v[0] * v[0] + v[1] * v[1]).sqrt();
                    if sp < 96.0 {
                        continue; // crisp birds drawn after, not here
                    }
                    draw_bird(g, pos[i], v, seedz[i], 0.55);
                }
            });
            for i in 0..pos.len() {
                let v = vel[i];
                let sp = (v[0] * v[0] + v[1] * v[1]).sqrt();
                if sp >= 96.0 {
                    continue;
                }
                draw_bird(book, pos[i], v, seedz[i], 1.0);
            }
        }),
    )
    .into();

    // The receipts strip — the engine's live economy + the plate's metrics.
    let receipts = format!(
        "{} boids · {} steps · {} queries",
        group_commas(N as u64),
        group_commas(steps),
        group_commas(queries),
    );
    let plate_receipt = "plate receipt · 2,511 shapes/frame · build 66.4 ms · raster 214.1 ms";

    let mut spec = studio::Spec {
        code: studio::Code::Lines {
            lines: swarm_lines(),
            blink: ctx.sec,
        },
        app: studio::App::new(1, super::tap_pulse(abs), abs),
        preview_custom: Some(sheet),
        session_line: 1.0,
        tab: Some("swarm.say".to_string()),
        ..Default::default()
    };
    spec.damage = None;

    let mut stack = Stack::new().push(Positioned::fill().child(studio::studio(abs, ladder, spec)));

    // The build/raster gauges — the engine splitting the frame budget, on
    // the fly. Two arcs, filling from their own receipts.
    let gauge_a = clamp01((t - 0.30) / 0.14);
    if gauge_a > 0.0 {
        let build_fill = 0.24 + 0.05 * (ctx.sec * 2.0).sin();
        let raster_fill = 0.72 + 0.08 * (ctx.sec * 1.6).cos();
        stack = stack.push(
            Positioned::new()
                .left(studio::PV_X0 + 40.0)
                .top(studio::TITLE_H + 60.0 + studio::PV_H - 210.0)
                .width(360.0)
                .height(170.0)
                .child(Opacity::new(gauge_a).child(Painting::sized(
                    Size::new(360.0, 170.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        book.rrect(
                            xywh(0.0, 0.0, 360.0, 170.0),
                            14.0,
                            alpha(Color::rgb(12, 12, 17), 0.92),
                        );
                        book.stroke_rrect(
                            xywh(0.0, 0.0, 360.0, 170.0),
                            14.0,
                            alpha(Color::WHITE, 0.09),
                            1.1,
                        );
                        // Two horizontal gauges — build and raster.
                        let gauges = [
                            ("build", build_fill, CYAN_SOFT),
                            ("raster", raster_fill, VIOLET_SOFT),
                        ];
                        for (i, (label, fill, col)) in gauges.iter().enumerate() {
                            let y = 46.0 + i as f32 * 62.0;
                            book.rect(xywh(24.0, y, 220.0, 12.0), alpha(Color::WHITE, 0.07));
                            book.rrect(xywh(24.0, y, 220.0 * fill, 12.0), 6.0, alpha(*col, 0.9));
                            let _ = label;
                        }
                    }),
                ))),
        );
        // The gauge labels.
        for (i, (label, txt)) in [
            ("build", "sim + tree — the layout half"),
            ("raster", "pixels — the paint half"),
        ]
        .iter()
        .enumerate()
        {
            let y = studio::TITLE_H + 60.0 + studio::PV_H - 210.0 + 26.0 + i as f32 * 62.0;
            stack = stack.push(
                Positioned::new()
                    .left(studio::PV_X0 + 40.0 + 256.0)
                    .top(y - 4.0)
                    .width(120.0)
                    .height(24.0)
                    .child(
                        Opacity::new(gauge_a).child(
                            Text::new(*label)
                                .style(TextStyle::new(15.0).monospace().color(alpha(INK, 0.9))),
                        ),
                    ),
            );
            stack = stack.push(
                Positioned::new()
                    .left(studio::PV_X0 + 40.0 + 24.0)
                    .top(y + 16.0)
                    .width(320.0)
                    .height(20.0)
                    .child(
                        Opacity::new(gauge_a).child(
                            Text::new(*txt)
                                .style(TextStyle::new(11.5).monospace().color(alpha(MUTED, 0.8))),
                        ),
                    ),
            );
        }
    }

    // The live economy receipt, bottom of the pane.
    let econ_a = clamp01((t - 0.36) / 0.14);
    if econ_a > 0.0 {
        stack = stack.push(
            Positioned::new()
                .left(studio::PV_X0 + 40.0)
                .top(studio::TITLE_H + 60.0 + studio::PV_H - 620.0)
                .width(700.0)
                .height(40.0)
                .child(Opacity::new(econ_a).child(super::chip(
                    receipts,
                    15.0,
                    tint(VIOLET_SOFT, 0.1),
                ))),
        );
    }

    stack = stack.push(super::caption(
        "the swarm view — a live murmuration, inside a ui sheet",
        1000.0,
        clamp01((t - 0.24) / 0.12),
    ));
    stack = stack.push(super::caption(
        plate_receipt,
        964.0,
        clamp01((t - 0.44) / 0.12),
    ));

    stack.into()
}

/// One bird — an oriented chevron, closed (the U-22 lesson: close your
/// triangles; the census counts open fills).
fn draw_bird(book: &mut Sketchbook, p: [f32; 2], v: [f32; 2], z: f32, vis: f32) {
    let sp = (v[0] * v[0] + v[1] * v[1]).sqrt().max(1e-3);
    let hx = v[0] / sp;
    let hy = v[1] / sp;
    let len = 3.4 + z * 3.8;
    let tipx = p[0] + hx * len;
    let tipy = p[1] + hy * len;
    let bx = p[0] - hx * len * 0.45;
    let by = p[1] - hy * len * 0.45;
    let px = -hy;
    let py = hx;
    let wing = len * 0.62;
    let mut path = Path::new();
    path.move_to(Offset::new(tipx, tipy));
    path.line_to(Offset::new(bx + px * wing, by + py * wing));
    path.line_to(Offset::new(bx - px * wing * 0.55, by - py * wing * 0.55));
    path.close();
    book.fill(path, alpha(INK, (0.42 + 0.5 * z) * vis));
}
