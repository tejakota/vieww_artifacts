//! exp_hero — *the worst frame the film could plausibly ask for, at true
//! master resolution: 1920×1080.* The budget plate.
//!
//! Everything the bench proved, in one composition, every pixel of it
//! rasterized by vieww and nothing in post:
//!
//! - a vignetted stage and a **projected floor grid** — `Transform3`
//!   (U-04), lines projected two points at a time;
//! - **eleven `Plus`-blended volumetric shafts through one blurred group**
//!   (U-01 economy, U-06 pricing) with their floor pools;
//! - **6,000 lit dust motes**, each reading its own depth in the beam cones;
//! - a **swept tube knot of 3,200 depth-sorted, lit, gradient-filled
//!   quads** — the manual-projection `three_d` module, Blinn-Phong glint,
//!   depth fog, painter's order;
//! - a **marching-squares liquid mass** with under-glow — contours chained
//!   into closed loops (U-20), glass-filled;
//! - the **wordmark as real glyph outlines** (the U-03 door) with a
//!   phase-advancing, re-sorted chrome ramp (U-08) and a mirrored,
//!   ground-faded reflection;
//! - a **frosted caption panel through a genuine backdrop blur** —
//!   `Filtered::with_backdrop`, the one thing in the tree that reads the
//!   real destination.
//!
//! The receipt is the frame's own budget: shapes, layers, and render
//! milliseconds at master resolution — in `metrics.txt`, measured by the
//! harness, printed nowhere by hand. A 10,800-frame master of frames like
//! this one is a render-time plan, not a capability question.

use std::sync::OnceLock;

use vieww_foundation::{
    BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle, Transform,
    Transform3,
};
use vieww_widget::prelude::*;
use vieww_widget::{Filtered, Opacity, Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, mix, tint, BG_DEEP, FAINT, INK, MUTED, Rng, VIOLET, VIOLET_DEEP, VIOLET_SOFT,
};
use crate::three_d::{draw_mesh, Camera, Mesh, MeshStyle, Vec3};

use crate::exp_wordmark::{chrome_stops, Mark as Wordmark};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 10.0;

/// Master resolution — the whole point of this plate.
pub const HERO_W: f32 = 1920.0;
pub const HERO_H: f32 = 1080.0;
pub const HERO: Size = Size::new(HERO_W, HERO_H);

/// The shaft count and dust count — printed from the code's own values.
const SHAFTS: usize = 11;
const MOTES: usize = 6000;

// ── The knot — a swept tube, built once ─────────────────────────────────────

/// Build the (2,3) torus knot as a tube mesh: 200 curve samples × 16 ring
/// segments = 3,200 quads, each carrying its own color stop.
fn knot_mesh() -> Mesh {
    let samples = 200;
    let ring = 16;
    let big_r = 1.5f32;
    let small_r = 0.62f32;
    let lift = 1.15f32;
    let tube = 0.17f32;

    // Curve points.
    let curve = (0..=samples)
        .map(|i| {
            let t = i as f32 / samples as f32 * std::f32::consts::TAU;
            Vec3::new(
                (big_r + small_r * (3.0 * t).cos()) * (2.0 * t).cos(),
                small_r * (3.0 * t).sin() * lift,
                (big_r + small_r * (3.0 * t).cos()) * (2.0 * t).sin(),
            )
        })
        .collect::<Vec<_>>();

    // Parallel-transport a normal around the curve for stable rings.
    let mut normals: Vec<Vec3> = Vec::with_capacity(curve.len());
    let mut n = Vec3::new(0.0, 1.0, 0.0);
    for i in 0..curve.len() {
        let tangent = curve[(i + 1) % curve.len()]
            .sub(curve[(i + curve.len() - 1) % curve.len()])
            .norm();
        // Project the previous normal off the tangent, renormalize.
        n = n.sub(tangent.scale(n.dot(tangent))).norm();
        if !n.len().is_finite() || n.len() < 1e-6 {
            n = Vec3::new(0.0, 1.0, 0.0);
        }
        normals.push(n);
    }

    let mut mesh = Mesh::new();

    // Vertices: rings around each curve point.
    for (i, p) in curve.iter().enumerate() {
        let tangent = curve[(i + 1) % curve.len()]
            .sub(curve[(i + curve.len() - 1) % curve.len()])
            .norm();
        let n = normals[i];
        let binorm = tangent.cross(n).norm();
        for j in 0..ring {
            let a = j as f32 / ring as f32 * std::f32::consts::TAU;
            let offset = n.scale(a.cos() * tube).add(binorm.scale(a.sin() * tube));
            let v = p.add(offset);
            mesh.verts.push(v);
        }
    }

    // Quads between consecutive rings, wrapped.
    for i in 0..curve.len() {
        let i_next = (i + 1) % curve.len();
        for j in 0..ring {
            let j_next = (j + 1) % ring;
            let a = i * ring + j;
            let b = i * ring + j_next;
            let c = i_next * ring + j_next;
            let d = i_next * ring + j;
            let frac = i as f32 / curve.len() as f32;
            let color = if frac < 0.5 {
                mix(VIOLET_DEEP, VIOLET_SOFT, frac * 2.0)
            } else {
                mix(VIOLET_SOFT, VIOLET, (frac - 0.5) * 2.0)
            };
            mesh.push_quad(a, b, c, d, color);
        }
    }
    mesh
}

fn knot() -> &'static Mesh {
    static KNOT: OnceLock<Mesh> = OnceLock::new();
    KNOT.get_or_init(knot_mesh)
}

// ── The shafts (X-05, at hero scale) ────────────────────────────────────────

#[derive(Clone)]
struct Shaft {
    angle: f32,
    spread: f32,
    gain: f32,
    phase: f32,
}

fn shafts() -> Vec<Shaft> {
    let mut rng = Rng::new(0x915);
    (0..SHAFTS)
        .map(|i| {
            let frac = i as f32 / (SHAFTS - 1) as f32;
            Shaft {
                angle: (-0.72 + 1.15 * frac) + rng.sym() * 0.03,
                spread: 30.0 + rng.f01() * 40.0,
                gain: 0.5 + rng.f01() * 0.5,
                phase: rng.f01() * std::f32::consts::TAU,
            }
        })
        .collect()
}

const PIVOT: Offset = Offset::new(900.0, 165.0);
const FLOOR: f32 = 1005.0;

fn beam_light(x: f32, y: f32, sway: f32, fan: &[Shaft]) -> f32 {
    let dx = x - PIVOT.dx;
    let dy = (y - PIVOT.dy).max(1.0);
    let m_angle = (dx / dy).atan();
    let m_dist = (dx * dx + dy * dy).sqrt();
    let mut best = 0.0f32;
    for s in fan {
        let a = s.angle + sway * 0.08;
        let d = (m_angle - a).abs();
        if d < 0.05 {
            let core = 1.0 - d / 0.05;
            let falloff = (1.0 - m_dist / 1250.0).clamp(0.0, 1.0);
            best = best.max(core * core * falloff * s.gain);
        }
    }
    best
}

// ── The liquid (compact marching squares, at hero scale) ────────────────────

const LQ_REGION: (f32, f32, f32, f32) = (1150.0, 430.0, 1680.0, 900.0);

fn lq_field(x: f32, y: f32, t: f32) -> f32 {
    let mut sum = 0.0;
    let balls = [
        (1385.0, 640.0, 150.0, 80.0, 0.6, 1.0, 0.0, 92.0),
        (1310.0, 600.0, 110.0, 90.0, 1.2, 0.8, 1.9, 70.0),
        (1450.0, 690.0, 100.0, 70.0, 0.9, 1.3, 3.7, 62.0),
        (1360.0, 700.0, 90.0, 60.0, 1.6, 1.1, 5.1, 55.0),
        (1435.0, 585.0, 80.0, 55.0, 1.0, 1.6, 2.6, 48.0),
    ];
    for (cx, cy, ax, ay, fx, fy, ph, r) in balls {
        let s = t * SECONDS + ph;
        let bx = cx + ax * (s * fx).sin();
        let by = cy + ay * (s * fy).sin();
        let breathe = 1.0 + 0.12 * (t * SECONDS * 1.3 + ph).sin();
        let r2 = (r * breathe).powi(2);
        let d2 = (x - bx).powi(2) + (y - by).powi(2);
        sum += r2 / (d2 + 120.0);
    }
    sum
}

/// Compact contour: segments with grid-edge keys (see exp_liquid for the
/// full commentary; this is the same pass at hero scale and coarser grid).
fn lq_contour(t: f32) -> Vec<((f32, f32), (u8, i32, i32), (f32, f32), (u8, i32, i32))> {
    let iso = 1.0;
    let cell = 13.0;
    let (x0, y0, x1, y1) = LQ_REGION;
    let nx = ((x1 - x0) / cell).round() as i32;
    let ny = ((y1 - y0) / cell).round() as i32;
    let mut vals = vec![0.0f32; ((nx + 1) * (ny + 1)) as usize];
    for j in 0..=ny {
        for i in 0..=nx {
            vals[(j * (nx + 1) + i) as usize] =
                lq_field(x0 + i as f32 * cell, y0 + j as f32 * cell, t);
        }
    }
    let at = |i: i32, j: i32| vals[(j * (nx + 1) + i) as usize] - iso;
    let mut segs = Vec::new();
    for j in 0..ny {
        for i in 0..nx {
            let v00 = at(i, j);
            let v10 = at(i + 1, j);
            let v11 = at(i + 1, j + 1);
            let v01 = at(i, j + 1);
            let code = (v00 > 0.0) as u8 | ((v10 > 0.0) as u8) << 1
                | ((v11 > 0.0) as u8) << 2 | ((v01 > 0.0) as u8) << 3;
            if code == 0 || code == 15 {
                continue;
            }
            let px = |i: i32| x0 + i as f32 * cell;
            let py = |j: i32| y0 + j as f32 * cell;
            let top = || {
                let k = v00 / (v00 - v10);
                ((px(i) + k * cell, py(j)), (0u8, i, j))
            };
            let right = || {
                let k = v10 / (v10 - v11);
                ((px(i + 1), py(j) + k * cell), (1u8, i + 1, j))
            };
            let bottom = || {
                let k = v01 / (v01 - v11);
                ((px(i) + k * cell, py(j + 1)), (0u8, i, j + 1))
            };
            let left = || {
                let k = v00 / (v00 - v01);
                ((px(i), py(j) + k * cell), (1u8, i, j))
            };
            let mut emit = |a: ((f32, f32), (u8, i32, i32)), b: ((f32, f32), (u8, i32, i32))| {
                segs.push((a.0, a.1, b.0, b.1));
            };
            match code {
                1 | 14 => emit(left(), top()),
                2 | 13 => emit(top(), right()),
                3 | 12 => emit(left(), right()),
                4 | 11 => emit(right(), bottom()),
                6 | 9 => emit(top(), bottom()),
                7 | 8 => emit(left(), bottom()),
                5 => emit(left(), top()),
                10 => emit(top(), right()),
                _ => unreachable!(),
            }
        }
    }
    segs
}

/// Chain into closed loops — U-20, the pass that is not optional.
fn lq_chain(
    segs: &[((f32, f32), (u8, i32, i32), (f32, f32), (u8, i32, i32))],
) -> Vec<Vec<(f32, f32)>> {
    use std::collections::HashMap;
    let mut by_key: HashMap<(u8, i32, i32), Vec<usize>> = HashMap::new();
    for (i, s) in segs.iter().enumerate() {
        by_key.entry(s.1).or_default().push(i);
    }
    let mut used = vec![false; segs.len()];
    let mut loops = Vec::new();
    for start in 0..segs.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let s = &segs[start];
        let mut pts = vec![s.0, s.2];
        let closing = s.1;
        let mut cursor = s.3;
        let mut guard = 0usize;
        while cursor != closing && guard < 8192 {
            guard += 1;
            let next = by_key
                .get(&cursor)
                .and_then(|c| c.iter().find(|i| !used[**i]).copied());
            let Some(ni) = next else { break };
            used[ni] = true;
            let n = &segs[ni];
            if n.1 == cursor {
                pts.push(n.2);
                cursor = n.3;
            } else {
                pts.push(n.0);
                cursor = n.1;
            }
        }
        if pts.len() >= 3 {
            loops.push(pts);
        }
    }
    loops
}

// ── The board ───────────────────────────────────────────────────────────────

pub fn frame(t: f32) -> WidgetNode {
    let fan = shafts();
    let word = Wordmark::get();
    let knot = knot();
    let lq_loops = lq_chain(&lq_contour(t));

    let board = Painting::sized(
        HERO,
        PaintWith::new({
            let fan = fan.clone();
            let lq_loops = lq_loops.clone();
            move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The stage.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(6, 6, 10)),
                    (0.5, BG_DEEP),
                    (1.0, Color::rgb(9, 9, 14)),
                ]),
            );

            // Stars.
            let mut rng = Rng::new(0x916);
            for _ in 0..110 {
                let x = rng.f01() * w;
                let y = rng.f01() * h;
                book.circle(Offset::new(x, y), 0.4 + rng.f01() * 0.9, alpha(Color::WHITE, 0.035));
            }

            // ── The floor grid (Transform3) ──
            let horizon = 780.0;
            let grid_t = Transform3::translation(0.0, -horizon, 0.0)
                .then(Transform3::perspective(1100.0))
                .then(Transform3::translation(0.0, horizon, 0.0));
            let fade = clamp01(t / 0.10);
            for zi in 0..14 {
                let z = 30.0 + zi as f32 * 95.0;
                for xi in -13..=13 {
                    let x = xi as f32 * 150.0;
                    if let (Some(a), Some(b)) = (
                        grid_t.project(Offset::new(x, 1010.0), z),
                        grid_t.project(Offset::new(x, 1010.0), z + 95.0),
                    ) {
                        let deep = 1.0 - z / 1400.0;
                        book.line(a, b, alpha(VIOLET, 0.22 * fade * deep), 1.0);
                    }
                }
                if let (Some(a), Some(b)) = (
                    grid_t.project(Offset::new(-1950.0, 1010.0), z),
                    grid_t.project(Offset::new(1950.0, 1010.0), z),
                ) {
                    let deep = 1.0 - z / 1400.0;
                    book.line(a, b, alpha(FAINT, 0.14 * fade * deep), 1.0);
                }
            }

            // ── The shafts: one Plus-blended blurred group (U-01/U-06) ──
            let sway = (t * 0.8).sin();
            let breath = 0.84 + 0.16 * (t * 1.6).sin();
            book.blended_layer(fade, 12.0, BlendMode::Plus, None, |g| {
                for s in &fan {
                    let a = s.angle + sway * 0.08;
                    let (sin, cos) = a.sin_cos();
                    let len = (FLOOR - PIVOT.dy) / cos.max(0.2);
                    let foot_x = PIVOT.dx + sin * len;
                    let apex = 2.5;
                    let mut quad = Path::new();
                    quad.move_to(Offset::new(PIVOT.dx - apex, PIVOT.dy));
                    quad.line_to(Offset::new(PIVOT.dx + apex, PIVOT.dy));
                    quad.line_to(Offset::new(foot_x + s.spread, FLOOR));
                    quad.line_to(Offset::new(foot_x - s.spread, FLOOR));
                    quad.close();
                    let bright = 0.15 * s.gain * breath;
                    g.fill(
                        quad,
                        Gradient::vertical().with_dither().with_stops(&[
                            (0.0, alpha(mix(Color::WHITE, VIOLET_SOFT, 0.25), bright * 1.5)),
                            (0.35, alpha(VIOLET_SOFT, bright)),
                            (1.0, alpha(VIOLET, bright * 0.12)),
                        ]),
                    );
                    g.circle(
                        Offset::new(foot_x, FLOOR + 8.0),
                        s.spread * 1.9,
                        Gradient::radial_fill().with_dither().with_stops(&[
                            (0.0, alpha(mix(VIOLET_SOFT, Color::WHITE, 0.2), bright)),
                            (1.0, alpha(VIOLET, 0.0)),
                        ]),
                    );
                }
            });

            // ── The dust: 6,000 motes, lit by their own cone depth ──
            book.blended_layer(fade, 0.0, BlendMode::Plus, None, |g| {
                let mut rng = Rng::new(0x917);
                for _ in 0..MOTES {
                    let x0 = rng.f01() * w;
                    let depth = 0.25 + rng.f01() * 0.75;
                    let y0 = rng.f01() * h;
                    let x = (x0 + t * 40.0 * depth) % w;
                    let y = y0 + t * 14.0 * depth;
                    let lit = beam_light(x, y, sway, &fan);
                    if lit <= 0.015 {
                        continue;
                    }
                    g.circle(
                        Offset::new(x, y),
                        0.7 + depth * 1.5,
                        alpha(tint(VIOLET_SOFT, lit * 0.55), (lit * 1.15).min(1.0) * breath),
                    );
                }
            });

            // ── The source core ──
            book.blended_layer(1.0, 0.0, BlendMode::Plus, None, |g| {
                g.circle(PIVOT, 6.0 * breath, alpha(Color::WHITE, 0.95 * fade));
                g.circle(PIVOT, 14.0, alpha(tint(VIOLET_SOFT, 0.5), 0.6 * fade));
            });
            book.layer(1.0, 16.0, None, |g| {
                g.circle(
                    PIVOT,
                    54.0 + 9.0 * (t * 1.6).sin(),
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(tint(Color::WHITE, 0.2), 0.28 * fade * breath)),
                        (0.45, alpha(VIOLET_SOFT, 0.11 * fade)),
                        (1.0, alpha(VIOLET, 0.0)),
                    ]),
                );
            });

            // ── The knot: 3,200 quads through the manual camera ──
            let spin = t * 0.55;
            let mut cam = Camera {
                eye: Vec3::new(0.0, 1.9, 6.4),
                target: Vec3::new(0.62, 0.12, 0.0),
                fov: 0.92,
            };
            // Slow orbital drift — the camera is the film's eye.
            let orb = (t * 0.22).sin() * 0.55;
            cam.eye = Vec3::new(6.4 * (0.35 + orb).sin(), 1.9, 6.4 * (0.35 + orb).cos());
            // The knot's own rotation about Y.
            let mut rotated = Mesh::new();
            rotated.verts = knot
                .verts
                .iter()
                .map(|v| Vec3::new(v.x * spin.cos() + v.z * spin.sin(), v.y,
                    -v.x * spin.sin() + v.z * spin.cos()))
                .collect();
            rotated.quads = knot.quads.clone();
            rotated.colors = knot.colors.clone();
            let style = MeshStyle {
                specular: 0.55,
                shininess: 34.0,
                ramp_light: 0.16,
                ramp_dark: 0.34,
                edge_alpha: 0.10,
                ..MeshStyle::default()
            };
            // Shift the knot into frame-left position via an eye offset —
            // cheap and honest: the camera looks at it.
            cam.target = Vec3::new(0.62, 0.12, 0.0);
            draw_mesh(book, &rotated, &cam, size, &style);

            // ── The liquid mass: chained contours, glass fill, under-glow ──
            if !lq_loops.is_empty() {
                let mut bmin = (f32::MAX, f32::MAX);
                let mut bmax = (f32::MIN, f32::MIN);
                for l in &lq_loops {
                    for p in l {
                        bmin.0 = bmin.0.min(p.0);
                        bmin.1 = bmin.1.min(p.1);
                        bmax.0 = bmax.0.max(p.0);
                        bmax.1 = bmax.1.max(p.1);
                    }
                }
                book.blended_layer(1.0, 18.0, BlendMode::Plus, None, |g| {
                    g.circle(
                        Offset::new((bmin.0 + bmax.0) * 0.5, bmax.1 + 16.0),
                        (bmax.0 - bmin.0) * 0.55,
                        Gradient::radial_fill().with_dither().with_stops(&[
                            (0.0, alpha(VIOLET, 0.20)),
                            (1.0, alpha(VIOLET, 0.0)),
                        ]),
                    );
                });
                for l in &lq_loops {
                    let mut p = Path::new();
                    p.move_to(Offset::new(l[0].0, l[0].1));
                    for q in &l[1..] {
                        p.line_to(Offset::new(q.0, q.1));
                    }
                    p.close();
                    book.layer(1.0, 0.0, None, |g| {
                        g.fill(
                            p.clone(),
                            Gradient::vertical().with_dither().with_stops(&[
                                (0.0, alpha(mix(VIOLET_SOFT, Color::WHITE, 0.25), 0.50)),
                                (0.45, alpha(VIOLET, 0.44)),
                                (1.0, alpha(mix(VIOLET, BG_DEEP, 0.45), 0.54)),
                            ]),
                        );
                    });
                    book.stroke(p, alpha(tint(VIOLET_SOFT, 0.35), 0.5), 1.5);
                }
            }

            // ── The wordmark: outline door + chrome, placed left-low ──
            let phase = (t * SECONDS / 6.0).fract();
            let mark_left = 140.0;
            let mark_y = 812.0;
            // The Mark's letters carry their baseline at exp_wordmark::BASELINE
            // baked in — offset by it, then the board's own line.
            let placed = word.all.transformed(Transform::translate(Offset::new(
                mark_left,
                mark_y - crate::exp_wordmark::BASELINE,
            )));
            let (stops, _wrapped) = chrome_stops(phase);
            let mut chrome = Gradient::horizontal().with_dither();
            chrome = chrome.with_stops(&stops);
            let fill_a = clamp01(t / 0.18) * clamp01((0.92 - t) / 0.08 + 1.0).min(1.0);
            book.layer(fill_a, 0.0, None, |g| {
                g.fill(placed.clone(), chrome);
            });
            // The reflection — the placed path, mirrored about the floor line.
            let mirror = Transform::scale(1.0, -1.0)
                .then(Transform::translate(Offset::new(0.0, 2.0 * (mark_y + 8.0))));
            let flipped = placed.clone().transformed(mirror);
            book.layer(fill_a * 0.8, 3.0, None, |g| {
                g.fill(flipped, alpha(mix(VIOLET_SOFT, Color::WHITE, 0.2), 0.13));
            });
            book.rect(
                Rect::new(0.0, mark_y + 24.0, w, mark_y + 150.0),
                Gradient::vertical().with_stops(&[
                    (0.0, alpha(BG_DEEP, 0.0)),
                    (0.5, alpha(BG_DEEP, 0.7)),
                    (1.0, alpha(BG_DEEP, 1.0)),
                ]),
            );
            book.line(
                Offset::new(mark_left - 40.0, mark_y + 5.0),
                Offset::new(mark_left + word.advance + 40.0, mark_y + 5.0),
                alpha(FAINT, 0.3 * fill_a),
                1.0,
            );

            // The floor line.
            book.line(
                Offset::new(0.0, FLOOR),
                Offset::new(w, FLOOR),
                alpha(FAINT, 0.14 * fade),
                1.0,
            );

            // The vignette — heavier: a stage, not a page.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.46), 0.95).with_dither().with_stops(&[
                    (0.45, alpha(Color::BLACK, 0.0)),
                    (1.0, alpha(Color::BLACK, 0.48)),
                ]),
            );
        }}),
    );

    Stack::new()
        .push(Positioned::fill().child(board))
        .push(caption_panel(t))
        .into()
}

// ── The frosted caption panel — a genuine backdrop blur ─────────────────────

fn caption_panel(t: f32) -> WidgetNode {
    let settle = clamp01((t - 0.05) / 0.20);

    // The panel's own card + text, drawn OVER the blurred backdrop.
    let card = Painting::sized(
        Size::new(560.0, 132.0),
        PaintWith::new(move |book: &mut Sketchbook, sz: Size| {
            book.rrect(
                Rect::new(0.0, 0.0, sz.width, sz.height),
                12.0,
                alpha(Color::rgb(20, 18, 30), 0.34),
            );
            book.stroke_rrect(
                Rect::new(0.0, 0.0, sz.width, sz.height),
                12.0,
                alpha(Color::WHITE, 0.14),
                1.2,
            );
            // The accent rule.
            book.rrect(
                Rect::new(0.0, 0.0, 220.0, 3.0),
                1.5,
                alpha(VIOLET_SOFT, 0.85 * settle),
            );
        }),
    );

    let caption = Stack::new()
        .push(
            Positioned::new()
                .left(24.0)
                .top(16.0)
                .width(512.0)
                .height(20.0)
                .child(
                    Text::new("the launch film · S08 — the single-take spine").style(
                        TextStyle::new(15.0)
                            .color(alpha(INK, 0.95 * settle))
                            .letter_spacing(0.6),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(24.0)
                .top(44.0)
                .width(512.0)
                .height(16.0)
                .child(
                    Text::new(format!(
                        "{} shafts · one Plus group · {} motes · 3200 knot quads",
                        SHAFTS, MOTES
                    ))
                    .style(
                        TextStyle::new(11.5)
                            .monospace()
                            .color(alpha(MUTED, 0.95 * settle)),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(24.0)
                .top(64.0)
                .width(512.0)
                .height(16.0)
                .child(
                    Text::new("contour chained · glyph outlines · backdrop blur".to_string())
                        .style(
                            TextStyle::new(11.5)
                                .monospace()
                                .color(alpha(MUTED, 0.95 * settle)),
                        ),
                ),
        )
        .push(
            Positioned::new()
                .left(24.0)
                .top(96.0)
                .width(512.0)
                .height(18.0)
                .child(
                    Text::new("every pixel rasterised by vieww — nothing in post".to_string())
                        .style(
                            TextStyle::new(11.5)
                                .monospace()
                                .color(alpha(VIOLET_SOFT, 0.9 * settle)),
                        ),
                ),
        );

    // The genuine backdrop blur: the panel filters what is painted beneath
    // it — grid, beams, dust — once, before its own content paints on top.
    let frosted = Filtered::new()
        .with_blur(8.0)
        .with_backdrop()
        .tint(mix(VIOLET_DEEP, BG_DEEP, 0.5), 0.30)
        .child(
            Stack::new()
                .push(Positioned::fill().child(card))
                .push(caption),
        );

    Stack::new()
        .push(
            Positioned::new()
                .left(1200.0)
                .top(918.0)
                .width(560.0)
                .height(132.0)
                .child(Opacity::new(settle).child(frosted)),
        )
        .into()
}
