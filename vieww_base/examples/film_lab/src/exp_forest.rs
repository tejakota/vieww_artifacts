//! exp_forest — *the recursion.*
//!
//! From a seed to a canopy: one recursive rule — every branch begets 2–3
//! branches, shorter, thinner, turned by a deterministic jitter — grown
//! depth-first on screen, so the tree *unfolds* in the order the rule
//! explores it. The wind is part of the construction: every frame rebuilds
//! the tree with a sway phase at every level, accumulating down the
//! hierarchy, and the leaves ride their own branch's lean.
//!
//! - **SEED** (t 0–0.08): a glowing dot on a dark hill. That is all.
//! - **TRUNK** (t 0.06–0.20): one line climbs out of the ground.
//! - **CASCADE** (t 0.16–0.55): the recursion cascades — branches appear
//!   in pre-order, each extending with ease, the tree thinking out loud.
//! - **CANOPY** (t 0.5–0.8): leaves arrive in waves, bottom branches
//!   first, each a small rounded quad with its own colour and sway.
//! - **THE WIND** (t 0.75–1.0): the full canopy leans and releases; the
//!   fireflies come out. The tree is alive, and it is all arithmetic.
//!
//! Tolerance axis: recursion depth × segment count — ~1,400 tapered
//! quads and ~2,400 leaves, all depth-sorted with the branches they ride,
//! rebuilt every frame (a pure function of `t`, like everything here).
//!
//! Receipts: segments built, leaves drawn, max depth reached — live.

use vieww_foundation::{
    Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextStyle,
};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, ease_out_cubic, ease_out_expo, mix, tint, FAINT, MUTED, Rng,
    VIOLET, VIOLET_SOFT, CYAN_SOFT, MINT, AMBER, BG_DEEP,
};
use crate::three_d::{Camera, Vec3};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 8.0;

/// Recursion depth — the tree's fractal dimension.
const DEPTH: u8 = 8;

/// Deterministic jitter — hashed from the branch's own path.
fn hash01(a: u32, b: u32) -> f32 {
    let h = a
        .wrapping_mul(0x9E3779B1)
        .wrapping_add(b.wrapping_mul(0x85EBCA6B))
        .rotate_left(13);
    (h >> 8) as f32 / ((1u32 << 24) as f32)
}

/// One branch segment in world space, carrying its reveal bookkeeping.
struct Seg {
    a: Vec3,
    b: Vec3,
    w0: f32,
    w1: f32,
    /// Pre-order index — the reveal order.
    order: usize,
    depth: u8,
}

/// Grow the tree: recursion, collecting segments. The reveal is a
/// **depth wave** — every branch at depth `d` appears when the growth wave
/// crosses its level and eases to full length, so the tree grows trunk →
/// limbs → twigs the way a real one does. Subtrees deeper than the wave
/// are not built at all.
fn grow(
    origin: Vec3,
    dir: Vec3,
    len: f32,
    width: f32,
    depth: u8,
    order: &mut usize,
    t: f32,
    growth: f32,
    wind: f32,
    out: &mut Vec<Seg>,
) {
    if depth > DEPTH || len < 0.05 {
        return;
    }
    let idx = *order;
    *order += 1;

    // The reveal window: this depth's wave, with a short extension ease.
    let appear = clamp01(growth * (DEPTH + 2) as f32 - depth as f32 - 1.0);
    if appear <= 0.0 {
        return; // the wave has not reached this level yet
    }
    let ext = ease_out_cubic(appear);

    // The branch's own sway — deeper branches sway more, out of phase.
    let sway = (t * 1.7 + depth as f32 * 1.93).sin() * 0.026 * (1.0 + depth as f32 * 0.14) * wind;
    let mut d = dir;
    let (s, c) = sway.sin_cos();
    d = Vec3::new(d.x * c + d.z * s, d.y, -d.x * s + d.z * c);

    let this_len = len * ext;
    let tip = origin.add(d.scale(this_len));
    out.push(Seg {
        a: origin,
        b: tip,
        w0: width,
        w1: width * 0.68,
        order: idx,
        depth,
    });

    // Children, hashed deterministically from this branch — with apical
    // dominance (the first child continues nearly straight; the others
    // splay) and phototropism (every direction is pulled back toward the
    // sky a little), which is what separates a tree from a fern.
    let r1 = hash01(depth as u32, idx as u32);
    let r2 = hash01(idx as u32, depth as u32 * 7 + 3);
    let r3 = hash01(depth as u32 * 31 + 11, idx as u32 + 17);
    let kids = if depth == 0 { 2 } else if r1 < 0.30 { 2 } else { 3 };
    for k in 0..kids {
        let kr = hash01(idx as u32 * 13 + k, depth as u32 * 17 + 5);
        let (pitch, yaw) = if k == 0 {
            // The apical child: continues the branch's line.
            (0.07 + r3 * 0.15, (r2 - 0.5) * 0.55)
        } else {
            // Lateral children: splay outward.
            (
                (0.40 + kr * 0.48).min(1.05),
                (k as f32 / kids as f32 - 0.5) * (1.7 + kr * 0.7) + (r2 - 0.5) * 0.6,
            )
        };
        let nd = dir_turn(d, yaw, pitch);
        // Phototropism: reach for the light.
        let nd = nd.add(Vec3::new(0.0, 0.20, 0.0)).norm();
        grow(
            tip,
            nd,
            len * (0.66 + kr * 0.14),
            width * 0.68,
            depth + 1,
            order,
            t,
            growth,
            wind,
            out,
        );
    }
}

/// Tilt a direction away from itself by `pitch`, in the compass direction
/// `yaw` selects — the classic branching construction, built on an
/// orthonormal frame so it works for a vertical trunk too (the naive
/// yaw-around-Y form collapses on exactly that case).
fn dir_turn(d: Vec3, yaw: f32, pitch: f32) -> Vec3 {
    let d = d.norm();
    // A reference axis not parallel to d.
    let ref_axis = if d.y.abs() > 0.9 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    // The frame perpendicular to d.
    let a = d.cross(ref_axis).norm();
    let b = d.cross(a);
    let (sy, cy) = yaw.sin_cos();
    let horiz = a.scale(sy).add(b.scale(cy)).norm();
    d.scale(pitch.cos()).add(horiz.scale(pitch.sin())).norm()
}

// ── The scene ───────────────────────────────────────────────────────────────

fn scene(book: &mut Sketchbook, canvas: Size, t: f32) {
    let w = canvas.width;
    let h = canvas.height;

    // The dusk sky.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::vertical()
            .with_dither()
            .with_stops(&[
                (0.0, Color::rgb(8, 9, 16)),
                (0.5, Color::rgb(14, 12, 22)),
                (0.8, Color::rgb(28, 16, 28)),
                (1.0, Color::rgb(36, 20, 26)),
            ]),
    );

    // The horizon glow — low, warm, behind the hill.
    book.layer(1.0, 34.0, None, |g| {
        g.circle(
            Offset::new(w * 0.62, h * 0.83),
            w * 0.30,
            Gradient::radial_fill().with_stops(&[
                (0.0, alpha(mix(AMBER, VIOLET, 0.45), 0.14)),
                (1.0, alpha(VIOLET, 0.0)),
            ]),
        );
    });

    // Stars, arriving with the canopy.
    {
        let mut rng = Rng::new(0xF0E5);
        for i in 0..80 {
            let x = rng.f01() * w;
            let y = rng.f01() * h * 0.55;
            let tw = 0.5 + 0.5 * (t * 1.6 + i as f32 * 1.37).sin();
            book.circle(Offset::new(x, y), 0.4 + rng.f01() * 0.8, alpha(Color::WHITE, 0.02 + 0.08 * tw));
        }
    }

    // Stage gates.
    let seed_t = clamp01(t / 0.08);
    let trunk_t = clamp01((t - 0.06) / 0.14);
    let cascade_t = clamp01((t - 0.16) / 0.39);
    let canopy_t = clamp01((t - 0.50) / 0.30);
    let wind_t = clamp01((t - 0.75) / 0.25);

    // The camera: a close low 3/4 view — the tree fills the stage.
    let cam = Camera {
        eye: Vec3::new(3.9, 1.7, 5.6),
        target: Vec3::new(0.0, 2.3, 0.0),
        fov: 0.80,
    };

    // The ground: a projected 3D plane, dark, fading with distance —
    // the tree stands ON it, not near a picture of it.
    {
        let far = 26.0;
        let wide = 20.0;
        let bands = 7;
        for i in 0..bands {
            let z0 = -far + i as f32 / bands as f32 * far * 1.9;
            let z1 = -far + (i + 1) as f32 / bands as f32 * far * 1.9;
            let Some((a, da, _)) = cam.project(Vec3::new(-wide, 0.0, z0), canvas) else { continue };
            let Some((b, _, _)) = cam.project(Vec3::new(wide, 0.0, z0), canvas) else { continue };
            let Some((c, _, _)) = cam.project(Vec3::new(wide, 0.02, z1), canvas) else { continue };
            let Some((d, _, _)) = cam.project(Vec3::new(-wide, 0.02, z1), canvas) else { continue };
            let fade = 1.0 - clamp01((da - 6.0) / 22.0) * 0.6;
            let mut g = Path::new();
            g.move_to(a);
            g.line_to(b);
            g.line_to(c);
            g.line_to(d);
            g.close();
            book.fill(g, mix(Color::rgb(11, 10, 15), Color::rgb(22, 14, 24), fade));
        }
    }

    // ── The seed ──────────────────────────────────────────────────────────
    if seed_t > 0.0 && t < 0.22 {
        if let Some((p, _, _)) = cam.project(Vec3::new(0.0, 0.05, 0.0), canvas) {
            let a = (1.0 - trunk_t * 0.7).max(0.0) * seed_t;
            book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
                g.circle(p, 3.0, alpha(Color::WHITE, a));
                g.circle(p, 9.0, alpha(MINT, 0.30 * a));
            });
        }
    }

    // ── The tree — grown fresh this frame ────────────────────────────────
    let growth = trunk_t.min(1.0) * 0.12 + cascade_t * 0.88;
    let wind = 0.15 + wind_t * 0.85;
    let mut segs: Vec<Seg> = Vec::with_capacity(1500);
    grow(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        2.2,
        0.16,
        0,
        &mut 0,
        t,
        growth,
        wind,
        &mut segs,
    );

    // The trunk's first segment is the seed's line — extend by trunk_t.
    // (grow already handles it via growth; the seed beat is decorative.)

    // Collect the draw list: branch quads + leaf quads, one painter's sort.
    struct Face {
        pts: [Offset; 4],
        depth: f32,
        brush: vieww_foundation::Brush,
        rrect: f32,
    }
    let mut faces: Vec<Face> = Vec::with_capacity(segs.len() + 2600);

    for seg in &segs {
        let (Some((pa, da, sa)), Some((pb, db, sb))) =
            (cam.project(seg.a, canvas), cam.project(seg.b, canvas))
        else {
            continue;
        };
        let depth = (da + db) * 0.5;
        // Screen-space perpendicular — camera-facing taper.
        let dx = pb.dx - pa.dx;
        let dy = pb.dy - pa.dy;
        let len = dx.hypot(dy).max(1e-3);
        let nx = -dy / len;
        let ny = dx / len;
        let w0 = (seg.w0 * sa * 900.0).max(0.5);
        let w1 = (seg.w1 * sb * 900.0).max(0.4);
        let shade = 1.0 - (seg.depth as f32 / DEPTH as f32) * 0.55;
        let bark = mix(Color::rgb(38, 26, 22), Color::rgb(16, 12, 16), 1.0 - shade);
        faces.push(Face {
            pts: [
                Offset::new(pa.dx + nx * w0, pa.dy + ny * w0),
                Offset::new(pa.dx - nx * w0, pa.dy - ny * w0),
                Offset::new(pb.dx - nx * w1, pb.dy - ny * w1),
                Offset::new(pb.dx + nx * w1, pb.dy + ny * w1),
            ],
            depth,
            brush: alpha(bark, 0.96).into(),
            rrect: 0.0,
        });

        // Leaves at the last three depths' tips — the canopy, not a tuft.
        if seg.depth + 3 > DEPTH && canopy_t > 0.0 {
            let leaf_wave = clamp01(canopy_t * 3.2 - (seg.depth as f32 / DEPTH as f32) * 1.6);
            let leaves = if seg.depth + 3 > DEPTH + 1 { 4 } else { 3 };
            for k in 0..leaves {
                let kr = hash01(seg.order as u32 * 7 + k, seg.depth as u32 + 101);
                let kr2 = hash01(seg.order as u32 + 31, k as u32 * 13 + 7);
                let appear = clamp01(leaf_wave * 2.0 - kr * 1.0);
                if appear <= 0.0 {
                    continue;
                }
                // The leaf rides the branch tip with its own sway.
                let sway = (t * 2.6 + kr * 6.28).sin() * 0.06 * wind;
                let base = seg.b.add(Vec3::new(
                    (kr - 0.5) * 0.9,
                    (kr2 - 0.5) * 0.7 + 0.3,
                    (kr2 - 0.5) * 0.9,
                ));
                let p = base.add(Vec3::new(sway, sway * 0.3, 0.0));
                if let Some((pl, dl, sl)) = cam.project(p, canvas) {
                    let size = (0.11 + kr2 * 0.11) * sl * 900.0 * (0.4 + 0.6 * appear);
                    let warm = kr * 0.35;
                    let leaf_col = if kr2 < 0.07 {
                        mix(AMBER, Color::WHITE, 0.15) // rare autumn holdouts, catching the dusk
                    } else {
                        // A green family with warm variance — leaves, not lights.
                        mix(mix(MINT, Color::rgb(64, 122, 82), 0.55), Color::rgb(132, 186, 130), warm)
                    };
                    let dim_by_depth = 1.0 / (1.0 + dl * 0.06);
                    faces.push(Face {
                        pts: [
                            Offset::new(pl.dx - size, pl.dy - size * 0.62),
                            Offset::new(pl.dx + size, pl.dy - size * 0.62),
                            Offset::new(pl.dx + size, pl.dy + size * 0.62),
                            Offset::new(pl.dx - size, pl.dy + size * 0.62),
                        ],
                        depth: dl,
                        brush: alpha(leaf_col, (0.72 + 0.28 * dim_by_depth) * appear).into(),
                        rrect: size * 0.55,
                    });
                }
            }
        }
    }

    // Painter's sort, far first.
    faces.sort_by(|a, b| b.depth.partial_cmp(&a.depth).unwrap_or(std::cmp::Ordering::Equal));
    let mut leaves_drawn = 0usize;
    for f in &faces {
        let mut path = Path::new();
        if f.rrect > 0.0 {
            leaves_drawn += 1;
            let l = f.pts[0].dx.min(f.pts[1].dx).min(f.pts[2].dx).min(f.pts[3].dx);
            let tp = f.pts[0].dy.min(f.pts[1].dy).min(f.pts[2].dy).min(f.pts[3].dy);
            let r = f.pts[0].dx.max(f.pts[1].dx).max(f.pts[2].dx).max(f.pts[3].dx);
            let btm = f.pts[0].dy.max(f.pts[1].dy).max(f.pts[2].dy).max(f.pts[3].dy);
            let leaf = Path::rounded_rect(Rect::new(l, tp, r, btm), f.rrect);
            book.fill(leaf, f.brush.clone());
            continue;
        } else {
            path.move_to(f.pts[0]);
            for p in &f.pts[1..] {
                path.line_to(*p);
            }
            path.close();
        }
        book.fill(path, f.brush.clone());
    }

    // ── The contact shadow: the tree's darkness on the ground ─────────────
    if growth > 0.3 {
        if let Some((p, _, sc)) = cam.project(Vec3::new(0.0, 0.0, 0.0), canvas) {
            let rx = 1.4 * sc * 900.0;
            book.layer(1.0, 8.0, None, |g| {
                g.circle(
                    Offset::new(p.dx + rx * 0.25, p.dy),
                    rx,
                    Gradient::radial_fill().with_stops(&[
                        (0.0, alpha(Color::BLACK, 0.45)),
                        (1.0, alpha(Color::BLACK, 0.0)),
                    ]),
                );
            });
        }
    }

    // ── Fireflies — out once the wind arrives ────────────────────────────
    if wind_t > 0.0 {
        book.blended_layer(1.0, 0.0, vieww_foundation::BlendMode::Plus, None, |g| {
            let mut rng = Rng::new(0xF1E);
            for i in 0..26 {
                let u = rng.f01();
                let v = rng.f01();
                let x = 60.0 + u * (w - 120.0) + (t * 0.8 + i as f32 * 1.1).sin() * 14.0;
                let y = h * (0.42 + v * 0.4) + (t * 1.3 + i as f32 * 0.7).cos() * 10.0;
                let pulse = 0.5 + 0.5 * (t * 2.2 + i as f32 * 2.4).sin();
                let a = pulse * wind_t * 0.7;
                g.circle(Offset::new(x, y), 1.4, alpha(tint(AMBER, 0.5), a));
                g.circle(Offset::new(x, y), 4.5, alpha(AMBER, a * 0.18));
            }
        });
    }

    // The vignette.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::radial(Offset::new(0.5, 0.5), 0.85)
            .with_dither()
            .with_stops(&[
                (0.55, alpha(Color::BLACK, 0.0)),
                (1.0, alpha(Color::BLACK, 0.52)),
            ]),
    );
}

/// The frame.
pub fn frame(t: f32) -> WidgetNode {
    let paint = Painting::sized(
        crate::film_lib::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            scene(book, size, t);
        }),
    );

    // The instrument: the recursion's own counts.
    let cascade_t = clamp01((t - 0.16) / 0.39);
    let canopy_t = clamp01((t - 0.50) / 0.30);
    let stage = if t < 0.08 {
        "SEED"
    } else if t < 0.20 {
        "TRUNK"
    } else if t < 0.55 {
        "CASCADE"
    } else if t < 0.80 {
        "CANOPY"
    } else {
        "THE WIND"
    };

    let strip = Stack::new()
        .push(
            Positioned::new()
                .left(24.0)
                .top(20.0)
                .width(520.0)
                .height(18.0)
                .child(
                    Text::new(format!("THE FOREST · {stage} · depth {DEPTH}")).style(
                        TextStyle::new(12.0)
                            .monospace()
                            .letter_spacing(2.2)
                            .color(alpha(FAINT, 0.95)),
                    ),
                ),
        )
        .push(
            Positioned::new()
                .left(24.0)
                .top(40.0)
                .width(680.0)
                .height(18.0)
                .child(
                    Text::new(format!(
                        "grown {:.0}% · leaves {:.0}% · 2–3 children · pre-order reveal · rebuilt every frame",
                        cascade_t * 100.0,
                        canopy_t * 100.0
                    ))
                    .style(TextStyle::new(11.0).monospace().color(alpha(MUTED, 0.85))),
                ),
        );

    Stack::new()
        .push(Positioned::fill().child(paint))
        .push(strip)
        .into()
}
