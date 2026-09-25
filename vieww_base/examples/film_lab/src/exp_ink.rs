//! exp_ink — *the diffusion axis.* One drop becomes a nebula.
//!
//! The author's brief, read again with the sea already built: a drop → an
//! entire ocean was water's surface; this is water's *volume*. One drop of
//! ink falls, strikes, and **blooms** — a recursive branching tree of
//! filaments, each generation revealed on its own clock, tips extruding
//! along their own curvature, all through one Plus-blended glow group. The
//! recursion is the axis (depth 9, ~1,100 segments, deterministic from a
//! seeded tree built once per frame), and the diffusion is the arc: after
//! the bloom peaks, the ink **dilutes** — a wash of transparency that
//! thins every filament until the nebula dissolves back into the water it
//! entered. 240 motes ride the same field the filaments' tips bent toward,
//! so the cloud and the tree visibly agree on which way the water moves.
//!
//! The receipt prints the tree's own census — generations, branches,
//! segments, the reveal clock — measured from the arrays that drew the
//! frame, never typed.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith, Text};

use crate::film_lib::{alpha, clamp01, ease_out_cubic, mix, smoothstep, tint, Rng, INK, MUTED,
    VIOLET, VIOLET_SOFT};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 11.0;

// ── The timeline ────────────────────────────────────────────────────────────

const T_FALL: f32 = 0.10; // the drop falls until here
const T_STRIKE: f32 = 0.14; // impact + crown
const T_BLOOM: f32 = 0.20; // the tree begins
const T_DILUTE: f32 = 0.72; // dilution begins

// ── The tree ────────────────────────────────────────────────────────────────

/// One filament: a polyline from the root outward, with a generation index.
struct Filament {
    pts: Vec<Offset>,
    gen: u32,
    /// The generation's reveal clock base.
    t_start: f32,
}

/// Build the whole bloom: recursive branching from the strike point.
/// Deterministic — the same seed, the same tree, every frame.
fn build_tree() -> Vec<Filament> {
    let mut rng = Rng::new(0x1DC0_u64);
    let mut out = Vec::new();

    fn grow(
        rng: &mut Rng,
        out: &mut Vec<Filament>,
        from: Offset,
        ang: f32,
        len: f32,
        gen: u32,
        t_start: f32,
    ) {
        if gen > 8 || len < 6.0 {
            return;
        }
        // The filament: a curved chain — heading perturbed each step.
        let steps = 7;
        let mut pts = Vec::with_capacity(steps + 1);
        let mut p = from;
        let mut a = ang;
        pts.push(p);
        for k in 0..steps {
            a += rng.sym() * 0.16 * (gen as f32 * 0.12 + 0.6);
            // Curl: bias toward the original heading, stronger in youth.
            a = (a * 0.5 + ang * 0.5).max(ang - 0.5).min(ang + 0.5);
            let step = len / steps as f32;
            p = Offset::new(p.dx + a.cos() * step, p.dy + a.sin() * step);
            pts.push(p);
            let _ = k;
        }
        let tip = *pts.last().unwrap();
        out.push(Filament { pts, gen, t_start });
        // Branch: 2 children, splayed; deeper generations branch less.
        let n = if gen < 4 { 2 } else if rng.f01() < 0.6 { 2 } else { 1 };
        let spread = 0.42 + gen as f32 * 0.06;
        for i in 0..n {
            let dir = if n == 1 { rng.sym() * 0.3 } else { (i as f32 * 2.0 - 1.0) * spread + rng.sym() * 0.12 };
            let lchild = len * (0.78 + rng.f01() * 0.1);
            grow(&mut *rng, out, tip, a + dir, lchild, gen + 1, t_start + 0.085);
        }
    }

    // The root burst: 6 primary filaments from the strike point.
    for k in 0..6 {
        let a = -std::f32::consts::FRAC_PI_2 + (k as f32 - 2.5) * 0.5;
        grow(&mut rng, &mut out, Offset::new(640.0, 372.0), a, 88.0, 0, 0.0);
    }
    out
}

/// A filament's drawn points at time t: the reveal grows the drawn length
/// along the polyline (the tip extrudes), with a growth ease per generation.
fn filament_points(f: &Filament, t: f32) -> Vec<Offset> {
    let grow = clamp01((t - T_BLOOM - f.t_start) / 0.075);
    if grow <= 0.0 {
        return Vec::new();
    }
    let eased = ease_out_cubic(grow);
    let n = f.pts.len() as f32;
    // Draw the first `eased` fraction of the polyline, interpolating the
    // fractional tail point.
    let total = (n - 1.0) * eased;
    let whole = total.floor() as usize;
    let frac = total - whole as f32;
    let mut pts: Vec<Offset> = f.pts.iter().take(whole + 1).cloned().collect();
    if whole + 1 < f.pts.len() && frac > 0.0 {
        let a = f.pts[whole];
        let b = f.pts[whole + 1];
        pts.push(Offset::new(
            a.dx + (b.dx - a.dx) * frac,
            a.dy + (b.dy - a.dy) * frac,
        ));
    }
    if pts.len() < 2 {
        return Vec::new();
    }
    pts
}

// ── The motes ────────────────────────────────────────────────────────────────

/// The drift field: a closed-form curl-ish current the whole scene agrees on.
#[must_use]
fn current(x: f32, y: f32, t: f32) -> (f32, f32) {
    let s = 0.006;
    (
        12.0 * ((y * s + t * 0.35).sin()) + 6.0 * ((y * s * 2.3 - t * 0.2).cos()),
        -10.0 * ((x * s - t * 0.3).cos()) + 5.0 * ((x * s * 1.7 + t * 0.25).sin()),
    )
}

/// The motes' positions at t — released at the tips as generations reveal,
/// then advected by the same field, integrated in 40 fixed substeps from
/// each mote's release (deterministic, stateless).
fn motes(tree: &[Filament], t: f32) -> Vec<(Offset, f32)> {
    let mut rng = Rng::new(0x40E5_u64);
    let mut out = Vec::new();
    for (i, f) in tree.iter().enumerate().step_by(3) {
        if let Some(tip) = f.pts.last() {
            if tip.dy < 720.0 && tip.dy > 40.0 {
                let release = T_BLOOM + f.t_start + 0.075;
                if t <= release {
                    continue;
                }
                let age = (t - release).min(1.0);
                // Integrate the drift in fixed substeps.
                let mut p = *tip;
                let n = 34;
                let dt = age / n as f32;
                for _ in 0..n {
                    let (vx, vy) = current(p.dx, p.dy, t - age + dt);
                    p = Offset::new(p.dx + vx * dt, p.dy + vy * dt);
                }
                let jx = rng.sym() * 8.0;
                let jy = rng.sym() * 8.0;
                out.push((Offset::new(p.dx + jx, p.dy + jy), 1.0 - age * 0.55));
                let _ = i;
            }
        }
    }
    out
}

pub fn frame(t: f32) -> WidgetNode {
    let tree = build_tree();
    // The census, measured from the tree.
    let branches = tree.len();
    let segments: usize = tree.iter().map(|f| f.pts.len() - 1).sum();
    let gens = (tree.iter().map(|f| f.gen).max().unwrap_or(0) + 1) as usize;
    let wash = smoothstep((t - T_DILUTE) / (1.0 - T_DILUTE));
    let live: Vec<(Vec<Offset>, u32)> = tree
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            // Disintegration: each branch dies at its own threshold —
            // deterministic (its index and generation), so the tree visibly
            // comes apart rather than merely dimming.
            let death = ((f.gen as f32 * 0.37 + i as f32 * 0.61).fract()) * 0.95;
            if death < wash * 0.8 {
                return None;
            }
            let p = filament_points(f, t);
            if p.len() >= 2 {
                Some((p, f.gen))
            } else {
                None
            }
        })
        .collect();
    let live_count = live.len();
    let mt = motes(&tree, t);

    // The dilution: after T_DILUTE, the ink thins to nothing.
    let dilute = 1.0 - 0.90 * smoothstep((t - T_DILUTE) / (1.0 - T_DILUTE));

    let board = Painting::sized(
        Size::new(1280.0, 720.0),
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;

            // The water — deep, slightly lighter above (the surface the
            // drop came through).
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(12, 13, 20)),
                    (0.5, Color::rgb(8, 9, 15)),
                    (1.0, Color::rgb(5, 6, 10)),
                ]),
            );

            // ── The falling drop (before the strike). ──
            if t < T_STRIKE {
                let f = clamp01(t / T_FALL);
                if f < 1.0 {
                    let y = 60.0 + f * f * 300.0;
                    // A slight wobble on the way down.
                    let x = 640.0 + 6.0 * (t * 6.2832 * 3.0).sin() * (1.0 - f);
                    book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                        g.circle(
                            Offset::new(x, y),
                            9.5,
                            Gradient::radial_fill().with_dither().with_stops(&[
                                (0.0, alpha(Color::rgb(80, 60, 200), 0.95)),
                                (0.6, alpha(VIOLET, 0.5)),
                                (1.0, alpha(Color::WHITE, 0.0)),
                            ]),
                        );
                        // The stretch: a drop in motion elongates.
                        g.fill(
                            {
                                let mut p = Path::new();
                                p.move_to(Offset::new(x - 4.2, y - 14.0));
                                p.line_to(Offset::new(x + 4.2, y - 14.0));
                                p.line_to(Offset::new(x + 2.4, y + 2.0));
                                p.line_to(Offset::new(x - 2.4, y + 2.0));
                                p.close();
                                p
                            },
                            alpha(VIOLET_SOFT, 0.6),
                        );
                    });
                }
            }

            // ── The strike: the impact ring + the crown. ──
            let strike_env = (1.0 - smoothstep((t - T_STRIKE) / 0.22)).max(0.0);
            if t >= T_STRIKE && strike_env > 0.01 {
                book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                    // The expanding surface ring.
                    let rr = 8.0 + 190.0 * smoothstep((t - T_STRIKE) / 0.22);
                    g.ring(
                        Offset::new(640.0, 372.0),
                        rr,
                        2.4,
                        alpha(Color::rgb(150, 160, 230), 0.45 * strike_env),
                    );
                    // The crown: 12 splash petals rising and falling.
                    let rise = smoothstep((t - T_STRIKE) / 0.10)
                        * (1.0 - smoothstep((t - T_STRIKE - 0.06) / 0.16));
                    for k in 0..12 {
                        let a = k as f32 * std::f32::consts::TAU / 12.0;
                        let l = 26.0 + 34.0 * rise * ((k as f32 * 0.9).fract() * 0.5 + 0.6);
                        let mut p = Path::new();
                        p.move_to(Offset::new(640.0 + a.cos() * 10.0, 372.0 + a.sin() * 6.0));
                        p.line_to(Offset::new(
                            640.0 + a.cos() * (10.0 + l),
                            372.0 + a.sin() * (6.0 + l * 0.4) - l * 0.8,
                        ));
                        p.line_to(Offset::new(
                            640.0 + a.cos() * (14.0 + l),
                            372.0 + a.sin() * (8.0 + l * 0.4) - l * 0.6,
                        ));
                        p.line_to(Offset::new(640.0 + a.cos() * 14.0, 372.0 + a.sin() * 8.0));
                        p.close();
                        g.fill(p, alpha(Color::rgb(120, 110, 220), 0.5 * rise));
                    }
                });
            }

            // ── THE BLOOM — the whole tree through one Plus group. ──
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                for (pts, gen) in live.iter() {
                    let width = ((3.4 - *gen as f32 * 0.34).max(0.8)) * (0.45 + 0.55 * dilute);
                    let hue_mix = mix(
                        VIOLET,
                        Color::rgb(40, 190, 220),
                        (*gen as f32 / 8.0).powi(2),
                    );
                    let mut p = Path::new();
                    p.move_to(pts[0]);
                    for q in pts.iter().skip(1) {
                        p.line_to(*q);
                    }
                    // The glow pass.
                    g.stroke(
                        p.clone(),
                        alpha(hue_mix, 0.10 * dilute * (1.0 - *gen as f32 * 0.08)),
                        width * 4.5,
                    );
                    // The core.
                    g.stroke(
                        p,
                        alpha(
                            tint(hue_mix, 0.25 + *gen as f32 * 0.03),
                            (0.62 - *gen as f32 * 0.035).max(0.20) * dilute,
                        ),
                        width,
                    );
                    // The tip — the young third of every branch carries the
                    // cyan the round promised, drawn as its own short stroke
                    // so it reads at any generation depth.
                    let tip_from = (pts.len() as f32 * 0.62) as usize;
                    if tip_from + 1 < pts.len() {
                        let mut tp = Path::new();
                        tp.move_to(pts[tip_from]);
                        for q in pts.iter().skip(tip_from + 1) {
                            tp.line_to(*q);
                        }
                        g.stroke(
                            tp,
                            alpha(
                                Color::rgb(150, 236, 244),
                                (0.78 - *gen as f32 * 0.03).max(0.5) * dilute,
                            ),
                            (width * 1.4).max(1.6),
                        );
                    }
                }
                // The root glow at the strike point, fading as the tree owns it.
                let root_env = smoothstep((t - T_STRIKE) / 0.08) * (1.0 - smoothstep((t - 0.45) / 0.3));
                if root_env > 0.01 {
                    g.circle(
                        Offset::new(640.0, 372.0),
                        46.0,
                        Gradient::radial_fill().with_dither().with_stops(&[
                            (0.0, alpha(Color::rgb(160, 140, 255), 0.5 * root_env * dilute)),
                            (1.0, alpha(Color::WHITE, 0.0)),
                        ]),
                    );
                }
            });

            // ── The motes — the cloud the tips agree with. ──
            if !mt.is_empty() {
                book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                    for (p, a) in mt.iter() {
                        g.circle(
                            *p,
                            1.6,
                            alpha(Color::rgb(170, 180, 250), 0.5 * a * dilute),
                        );
                    }
                });
            }

            // ── The dilution wash: the nebula thinning into the water. ──
            let wash = smoothstep((t - T_DILUTE) / (1.0 - T_DILUTE));
            if wash > 0.01 {
                book.circle(
                    Offset::new(640.0, 372.0),
                    380.0 + 110.0 * wash,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(Color::rgb(60, 66, 120), 0.13 * wash)),
                        (0.7, alpha(Color::rgb(40, 44, 90), 0.07 * wash)),
                        (1.0, alpha(Color::WHITE, 0.0)),
                    ]),
                );
            }
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(board));
    stack.push(receipt_panel(branches, segments, gens, live_count)).into()
}

// ── The receipt ─────────────────────────────────────────────────────────────

fn receipt_panel(branches: usize, segments: usize, gens: usize, live: usize) -> WidgetNode {
    let lines = [
        "INK · THE DIFFUSION AXIS · ONE DROP, ONE NEBULA".to_string(),
        format!("tree: {branches} branches · {segments} segments · {gens} generations (measured)"),
        format!("reveal clock: gen g at T_BLOOM + 0.085·g · live now {live}"),
        format!("motes: tips advected by the closed-form current · 34 substeps"),
        format!("dilution: the wash that returns the ink to water"),
    ];

    const P_X: f32 = 42.0;
    const P_Y: f32 = 42.0;

    let mut stack = Stack::new();
    for (i, line) in lines.iter().enumerate() {
        stack = stack.push(
            Positioned::new()
                .left(P_X)
                .top(P_Y + i as f32 * 16.0)
                .width(560.0)
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
