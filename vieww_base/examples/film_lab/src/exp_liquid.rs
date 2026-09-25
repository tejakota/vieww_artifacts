//! exp_liquid — *U-13's generator geometry: the contour, not the rectangle.*
//! The marching-squares plate.
//!
//! An organic liquid mass — six metaballs on Lissajous orbits, one of them
//! falling in as the drop — rendered by **marching squares over the implicit
//! field**, then **chained into closed loops**. The chaining is the plate's
//! spine: a hundred loose two-point segments inside a single `fill` is a
//! hundred *open* subpaths, and a fill of open subpaths is nothing at all —
//! U-20's correction, done the way that cannot fail.
//!
//! No `Path::intersect` exists (U-13: correctly) and none is needed: convex
//! clipping built the shatter's cells, and an implicit field builds this —
//! two different generators, both in the example where they belong.
//!
//! The fill is a vertical glass gradient across the mass's own measured
//! bounds; a highlight rides inside the mass through the same clip-window
//! pattern the shatter used (U-11); an under-glow sits beneath it in a
//! Plus-blended blurred group. The receipt is the algorithm's own census —
//! cells evaluated, segments emitted, loops closed, the drop's merge — every
//! number counted by the pass that produced it.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Path, Rect, Size, Sketchbook,
    TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, ease_out_cubic, mix, spring_out, tint, BG_DEEP, CANVAS, FAINT, INK, MUTED,
    VIOLET, VIOLET_SOFT, CYAN_SOFT,
};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 10.0;

// ── The field ───────────────────────────────────────────────────────────────

/// One metaball: a Lissajous orbit and a breathing radius.
struct Ball {
    /// Orbit center.
    cx: f32,
    cy: f32,
    /// Orbit amplitudes.
    ax: f32,
    ay: f32,
    /// Orbit frequencies.
    fx: f32,
    fy: f32,
    /// Phase.
    ph: f32,
    /// Base radius²  (the field uses r² directly).
    r2: f32,
    /// Radius breathing amplitude, fraction of r.
    breath: f32,
}

const REGION: (f32, f32, f32, f32) = (330.0, 170.0, 990.0, 590.0);

/// The metaballs, deterministically.
fn balls() -> Vec<Ball> {
    vec![
        Ball { cx: 640.0, cy: 380.0, ax: 120.0, ay: 60.0, fx: 0.7, fy: 1.1, ph: 0.0,
            r2: 92.0_f32.powi(2), breath: 0.10 },
        Ball { cx: 560.0, cy: 340.0, ax: 90.0, ay: 70.0, fx: 1.3, fy: 0.8, ph: 1.9,
            r2: 70.0_f32.powi(2), breath: 0.14 },
        Ball { cx: 700.0, cy: 420.0, ax: 80.0, ay: 55.0, fx: 0.9, fy: 1.4, ph: 3.7,
            r2: 64.0_f32.powi(2), breath: 0.12 },
        Ball { cx: 610.0, cy: 430.0, ax: 70.0, ay: 45.0, fx: 1.7, fy: 1.2, ph: 5.1,
            r2: 55.0_f32.powi(2), breath: 0.16 },
        Ball { cx: 690.0, cy: 330.0, ax: 60.0, ay: 40.0, fx: 1.1, fy: 1.7, ph: 2.6,
            r2: 48.0_f32.powi(2), breath: 0.18 },
        // The drop — handled separately, falls in at t≈0.15.
        Ball { cx: 640.0, cy: 140.0, ax: 0.0, ay: 0.0, fx: 0.0, fy: 0.0, ph: 0.0,
            r2: 40.0_f32.powi(2), breath: 0.0 },
    ]
}

/// The drop's fall: from above the region into the mass, t in [0.12, 0.30].
fn drop_y(t: f32) -> f32 {
    let tau = clamp01((t - 0.12) / 0.18);
    140.0 + ease_out_cubic(tau) * 240.0
}

/// The implicit field at (x, y) — metaball sum, drop included.
fn field(x: f32, y: f32, t: f32) -> f32 {
    let mut sum = 0.0;
    for (i, b) in balls().iter().enumerate() {
        let (bx, by) = if i == 5 {
            (b.cx, drop_y(t))
        } else {
            let s = t * SECONDS + b.ph;
            (b.cx + b.ax * (s * b.fx).sin(), b.cy + b.ay * (s * b.fy).sin())
        };
        let breathe = 1.0 + b.breath * (t * SECONDS * 1.3 + b.ph).sin();
        let r2 = b.r2 * breathe * breathe;
        let d2 = (x - bx).powi(2) + (y - by).powi(2);
        sum += r2 / (d2 + 120.0);
    }
    sum
}

// ── Marching squares — segments, then the chaining pass (U-20) ─────────────

/// One contour segment: endpoints plus the grid edges they lie on (for
/// exact chaining — no float proximity, the edge id IS the junction).
#[derive(Clone, Copy)]
struct Seg {
    a: (f32, f32),
    b: (f32, f32),
    /// Edge keys: (kind, i, j) — 0 = horizontal (top edge of cell i,j),
    /// 1 = vertical (left edge).
    ka: (u8, i32, i32),
    kb: (u8, i32, i32),
}

/// The contour at threshold `iso`, with the cell census for the receipt.
fn contour(t: f32, iso: f32) -> (Vec<Seg>, usize, usize) {
    let cell = 9.0;
    let (x0, y0, x1, y1) = REGION;
    let nx = ((x1 - x0) / cell).round() as i32;
    let ny = ((y1 - y0) / cell).round() as i32;

    // Corner samples — (nx+1)×(ny+1).
    let mut vals = vec![0.0f32; ((nx + 1) * (ny + 1)) as usize];
    for j in 0..=ny {
        for i in 0..=nx {
            let x = x0 + i as f32 * cell;
            let y = y0 + j as f32 * cell;
            vals[(j * (nx + 1) + i) as usize] = field(x, y, t);
        }
    }

    let at = |i: i32, j: i32| vals[(j * (nx + 1) + i) as usize];
    let mut segs = Vec::new();
    let mut cells_evaluated = 0usize;
    let mut cells_crossed = 0usize;

    for j in 0..ny {
        for i in 0..nx {
            cells_evaluated += 1;
            let v00 = at(i, j) - iso;
            let v10 = at(i + 1, j) - iso;
            let v11 = at(i + 1, j + 1) - iso;
            let v01 = at(i, j + 1) - iso;
            let code = (v00 > 0.0) as u8 | ((v10 > 0.0) as u8) << 1
                | ((v11 > 0.0) as u8) << 2 | ((v01 > 0.0) as u8) << 3;
            if code == 0 || code == 15 {
                continue;
            }
            cells_crossed += 1;

            // Edge interpolation points (in canvas space) + their grid keys.
            let px = |i: i32| x0 + i as f32 * cell;
            let py = |j: i32| y0 + j as f32 * cell;
            // top edge between (i,j)-(i+1,j): kind 0
            let top = || {
                let k = v00 / (v00 - v10);
                ((px(i) + k * cell, py(j)), (0u8, i, j))
            };
            // right edge between (i+1,j)-(i+1,j+1): kind 1 at (i+1, j)
            let right = || {
                let k = v10 / (v10 - v11);
                ((px(i + 1), py(j) + k * cell), (1u8, i + 1, j))
            };
            // bottom edge between (i,j+1)-(i+1,j+1): kind 0 at (i, j+1)
            let bottom = || {
                let k = v01 / (v01 - v11);
                ((px(i) + k * cell, py(j + 1)), (0u8, i, j + 1))
            };
            // left edge between (i,j)-(i,j+1): kind 1 at (i, j)
            let left = || {
                let k = v00 / (v00 - v01);
                ((px(i), py(j) + k * cell), (1u8, i, j))
            };

            let mut emit = |a: ((f32, f32), (u8, i32, i32)),
                            b: ((f32, f32), (u8, i32, i32))| {
                segs.push(Seg {
                    a: a.0,
                    b: b.0,
                    ka: a.1,
                    kb: b.1,
                });
            };

            // The 16 cases. Saddle (5/10): disambiguate by center value.
            match code {
                1 | 14 => emit(left(), top()),
                2 | 13 => emit(top(), right()),
                3 | 12 => emit(left(), right()),
                4 | 11 => emit(right(), bottom()),
                6 | 9 => emit(top(), bottom()),
                7 | 8 => emit(left(), bottom()),
                5 => {
                    let center = (v00 + v10 + v11 + v01) / 4.0;
                    if center > 0.0 {
                        emit(left(), top());
                        emit(right(), bottom());
                    } else {
                        emit(left(), bottom());
                        emit(top(), right());
                    }
                }
                10 => {
                    let center = (v00 + v10 + v11 + v01) / 4.0;
                    if center > 0.0 {
                        emit(top(), right());
                        emit(left(), bottom());
                    } else {
                        emit(left(), top());
                        emit(right(), bottom());
                    }
                }
                _ => unreachable!("case 0/15 handled above"),
            }
        }
    }
    (segs, cells_evaluated, cells_crossed)
}

/// THE CHAINING PASS (U-20): loose segments → closed loops. Segments join
/// exactly at shared grid-edge keys — no float tolerance anywhere.
fn chain(segs: &[Seg]) -> Vec<Vec<(f32, f32)>> {
    use std::collections::HashMap;
    // key → list of (segment index, which endpoint)
    let mut by_key: HashMap<(u8, i32, i32), Vec<(usize, u8)>> = HashMap::new();
    for (i, s) in segs.iter().enumerate() {
        by_key.entry(s.ka).or_default().push((i, 0));
        by_key.entry(s.kb).or_default().push((i, 1));
    }

    let mut used = vec![false; segs.len()];
    let mut loops = Vec::new();

    for start in 0..segs.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut pts = vec![segs[start].a, segs[start].b];
        // Walk forward from the b endpoint until back at start's a key.
        let closing_key = segs[start].ka;
        let mut cursor = segs[start].kb;
        let mut guard = 0usize;
        while cursor != closing_key && guard < 4096 {
            guard += 1;
            // Any unused segment at this key?
            let next = by_key
                .get(&cursor)
                .and_then(|cands| {
                    cands
                        .iter()
                        .find(|(i, _)| !used[*i])
                        .copied()
                });
            let Some((ni, _which)) = next else { break };
            used[ni] = true;
            let s = &segs[ni];
            // Enter through the matching key, continue from the other end.
            if s.ka == cursor {
                pts.push(s.b);
                cursor = s.kb;
            } else {
                pts.push(s.a);
                cursor = s.ka;
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
    let iso = 1.0;
    let (segs, cells, crossed) = contour(t, iso);
    let loops = chain(&segs);

    let board = Painting::sized(
        CANVAS,
        PaintWith::new({
            let loops = loops.clone();
            move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The ground.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(7, 8, 12)),
                    (0.6, BG_DEEP),
                    (1.0, Color::rgb(10, 10, 15)),
                ]),
            );

            // Quiet stars.
            let mut rng = crate::film_lib::Rng::new(0x714);
            for _ in 0..56 {
                let x = rng.f01() * w;
                let y = rng.f01() * h;
                book.circle(Offset::new(x, y), 0.4 + rng.f01() * 0.7, alpha(Color::WHITE, 0.035));
            }

            // The mass's measured bounds — the gradient maps to the geometry.
            let mut bmin = (f32::MAX, f32::MAX);
            let mut bmax = (f32::MIN, f32::MIN);
            for l in &loops {
                for p in l {
                    bmin.0 = bmin.0.min(p.0);
                    bmin.1 = bmin.1.min(p.1);
                    bmax.0 = bmax.0.max(p.0);
                    bmax.1 = bmax.1.max(p.1);
                }
            }

            // The under-glow — Plus-blended, blurred, beneath the mass.
            if bmax.1 > f32::MIN {
                book.blended_layer(1.0, 16.0, BlendMode::Plus, None, |g| {
                    g.circle(
                        Offset::new((bmin.0 + bmax.0) * 0.5, bmax.1 + 14.0),
                        (bmax.0 - bmin.0) * 0.55,
                        Gradient::radial_fill().with_dither().with_stops(&[
                            (0.0, alpha(VIOLET, 0.22)),
                            (1.0, alpha(VIOLET, 0.0)),
                        ]),
                    );
                });
            }

            // The mass itself: each closed loop a Path, filled.
            let fade = clamp01(t / 0.08);
            for l in &loops {
                let mut p = Path::new();
                p.move_to(Offset::new(l[0].0, l[0].1));
                for q in &l[1..] {
                    p.line_to(Offset::new(q.0, q.1));
                }
                p.close();

                let y0 = if bmin.1.is_finite() { bmin.1 } else { REGION.1 };
                let y1 = if bmax.1.is_finite() { bmax.1 } else { REGION.3 };

                // Glass body — vertical, across the mass's own bounds.
                book.layer(fade, 0.0, None, |g| {
                    g.fill(
                        p.clone(),
                        Gradient::vertical().with_dither().with_stops(&[
                            (0.0, alpha(mix(VIOLET_SOFT, Color::WHITE, 0.25), 0.42)),
                            (0.45, alpha(VIOLET, 0.36)),
                            (1.0, alpha(mix(VIOLET, BG_DEEP, 0.45), 0.46)),
                        ]),
                    );
                });

                // The highlight — a rider inside the mass (U-11's window,
                // smallest form: the clip IS the mass).
                let hx = (bmin.0 + bmax.0) * 0.5 + (t * SECONDS * 0.9).sin() * 60.0;
                book.layer(fade, 0.0, Some(p.clone()), |g| {
                    g.circle(
                        Offset::new(hx, (y0 + y1) * 0.38),
                        (bmax.0 - bmin.0).min(240.0) * 0.28,
                        Gradient::radial_fill().with_dither().with_stops(&[
                            (0.0, alpha(tint(VIOLET_SOFT, 0.55), 0.28)),
                            (1.0, alpha(VIOLET, 0.0)),
                        ]),
                    );
                });

                // Surface tension — the rim.
                book.stroke(p.clone(), alpha(tint(VIOLET_SOFT, 0.35), 0.55 * fade), 1.6);
            }

            // The drop, before it merges — a bead falling toward the field.
            let dtau = clamp01((t - 0.04) / 0.10);
            if dtau > 0.0 && t < 0.30 {
                let dy = drop_y(t) - 26.0;
                let stretch = 1.0 + 0.35 * (t * 30.0).sin().max(0.0) * (1.0 - t / 0.3);
                book.layer(1.0, 0.0, None, |g| {
                    g.circle(
                        Offset::new(640.0, dy),
                        17.0 / stretch,
                        Gradient::radial_fill().with_stops(&[
                            (0.0, alpha(mix(VIOLET_SOFT, Color::WHITE, 0.5), 0.75)),
                            (0.7, alpha(VIOLET, 0.55)),
                            (1.0, alpha(VIOLET, 0.0)),
                        ]),
                    );
                    g.circle(
                        Offset::new(640.0, dy - 14.0 * stretch),
                        17.0 * stretch,
                        alpha(VIOLET_SOFT, 0.18),
                    );
                });
            }

            // The vignette.
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::radial(Offset::new(0.5, 0.5), 0.9).with_dither().with_stops(&[
                    (0.55, alpha(Color::BLACK, 0.0)),
                    (1.0, alpha(Color::BLACK, 0.40)),
                ]),
            );
        }}),
    );

    Stack::new()
        .push(Positioned::fill().child(board))
        .push(receipt_panel(t, &segs, cells, crossed, &loops))
        .into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(
    t: f32,
    segs: &[Seg],
    cells: usize,
    crossed: usize,
    loops: &[Vec<(f32, f32)>],
) -> WidgetNode {
    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;
    const P_W: f32 = 340.0;

    let chain_pts: usize = loops.iter().map(|l| l.len()).sum();

    let lines = [
        "U-13 · LIQUID · MARCHING SQUARES (U-20 chained)".to_string(),
        format!("cells {} · crossed {} · segments {}", cells, crossed, segs.len()),
        format!("loops {} · chain pts {} · all closed", loops.len(), chain_pts),
        format!("balls 6 + drop · iso 1.00 · cell 9px"),
        format!("drop {} at y {:.0}", if t < 0.30 { "falling" } else { "merged" }, drop_y(t)),
    ];

    let mut stack = Stack::new().push(
        Positioned::new()
            .left(P_X)
            .top(P_Y)
            .width(P_W)
            .height(18.0)
            .child(
                Text::new(lines[0].clone()).style(
                    TextStyle::new(12.0)
                        .monospace()
                        .letter_spacing(1.8)
                        .color(alpha(FAINT, 0.95)),
                ),
            ),
    );
    for (i, line) in lines.iter().enumerate().skip(1) {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + 18.0 + i as f32 * 16.0)
                .width(P_W)
                .height(15.0)
                .child(
                    Text::new(line.clone()).style(
                        TextStyle::new(11.0)
                            .monospace()
                            .color(alpha(MUTED, 0.9)),
                    ),
                ),
        );
    }

    // The instrument: the field itself, coarse-sampled, as the algorithm
    // sees it — the raw implicit function the contour walks.
    let field_map = Painting::sized(
        Size::new(P_W, 96.0),
        PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
            book.rrect(
                Rect::new(0.0, 0.0, P_W, 96.0),
                10.0,
                alpha(Color::rgb(16, 16, 21), 0.88),
            );
            book.stroke_rrect(
                Rect::new(0.0, 0.0, P_W, 96.0),
                10.0,
                alpha(Color::WHITE, 0.08),
                1.0,
            );
            // Coarse field: 34×16 samples, alpha = clamp(field).
            for j in 0..16 {
                for i in 0..34 {
                    let u = i as f32 / 34.0;
                    let v = j as f32 / 16.0;
                    let x = REGION.0 + u * (REGION.2 - REGION.0);
                    let y = REGION.1 + v * (REGION.3 - REGION.1);
                    let f = field(x, y, t);
                    let a = (f * 0.4).min(1.0);
                    if a > 0.02 {
                        let cw = (P_W - 24.0) / 34.0;
                        let ch = 84.0 / 16.0;
                        book.rect(
                            Rect::new(
                                12.0 + i as f32 * cw,
                                8.0 + j as f32 * ch,
                                12.0 + i as f32 * cw + cw,
                                8.0 + j as f32 * ch + ch,
                            ),
                            if f >= 1.0 {
                                alpha(mix(CYAN_SOFT, Color::WHITE, 0.4), 0.5)
                            } else {
                                alpha(VIOLET, a * 0.5)
                            },
                        );
                    }
                }
            }
            // The iso line marker — where the contour lives.
            book.line(
                Offset::new(12.0, 8.0 + 84.0 * 0.5),
                Offset::new(P_W - 12.0, 8.0 + 84.0 * 0.5),
                alpha(Color::WHITE, 0.12),
                1.0,
            );
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(P_X)
            .top(P_Y + 116.0)
            .width(P_W)
            .height(96.0)
            .child(field_map),
    );

    stack.into()
}
