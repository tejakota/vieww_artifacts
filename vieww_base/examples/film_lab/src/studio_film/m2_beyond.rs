//! Z10B · beyond UI — four more things the engine draws, each one running
//! live in its own card, each one driven by the crate that ships it:
//!
//! | card | crate | what runs |
//! |---|---|---|
//! | 3D models | `vieww-mesh` | the vieww mark, loaded from OBJ, lit per face |
//! | Physics | `vieww-physics` | a rigid-body world, replayed to this frame |
//! | Characters | `vieww-animation` | a skeleton posed by a keyframed clip |
//! | Charts | `vieww-widget` | `DonutChart` + `ScatterChart`, over this film's own census |
//!
//! No number on this scene is typed: the triangle count is the loaded
//! mesh's, the donut is the census's draw mix, and every scatter dot is
//! one real frame of this film, timed.

use vieww_foundation::{Color, Offset, Rect, Shadow, Size, Sketchbook, TextAlign};
use vieww_widget::prelude::*;
use vieww_widget::{Clip, DonutChart, ScatterChart, Theme, ThemeData};

use super::models::{self, Drop, Jar, Look, Pose3};
use super::space::View;
use super::{
    ACCENT, BG_DEEP, BRAND_FAR, BRAND_NEAR, CANVAS, INK, MUTED, SYN_MACRO, SYN_TYPE, TERM_GREEN,
};
use crate::film_lib::{clamp01, ease_out_cubic, ease_out_expo};
use crate::product_film as pf;
use crate::three_d::{Camera, Vec3};

const CARD_W: f32 = 400.0;
const CARD_H: f32 = 560.0;
const CARD_GAP: f32 = 36.0;
const CARD_TOP: f32 = 300.0;

fn card_rect(k: usize) -> Rect {
    let total = 4.0 * CARD_W + 3.0 * CARD_GAP;
    let left = (pf::W - total) * 0.5 + k as f32 * (CARD_W + CARD_GAP);
    Rect::new(left, CARD_TOP, left + CARD_W, CARD_TOP + CARD_H)
}

/// Each card's demo area: under the title, above the caption.
fn stage(k: usize) -> Rect {
    let r = card_rect(k);
    Rect::new(r.left + 24.0, r.top + 84.0, r.right - 24.0, r.top + 440.0)
}

const TITLES: [&str; 4] = ["3D models", "Physics", "Characters", "Charts"];

pub(crate) fn beyond_ui(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let probe = ctx.probe;
    let mut stack = Stack::new();

    super::frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            // A flat deep ground — the four cards and their shadows carry
            // the depth; the soft violet halo that once sat here is gone
            // with the film's radial glows (see `mod.rs`).
            book.rect(Rect::new(0.0, 0.0, s.width, s.height), BG_DEEP);
            pf::vignette(book, s.width, s.height, 0.5);
        }),
    )));

    let appear = move |k: usize| ease_out_cubic(clamp01((t - 0.03 - k as f32 * 0.05) / 0.12));

    // The cards themselves, and every live demo drawn inside them.
    let (tris, _verts) = models::mesh_counts(models::mark_mesh());
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            for k in 0..4 {
                let a = appear(k);
                if a <= 0.01 {
                    continue;
                }
                let r = card_rect(k);
                let r = Rect::new(
                    r.left,
                    r.top + (1.0 - a) * 24.0,
                    r.right,
                    r.bottom + (1.0 - a) * 24.0,
                );
                book.shadow(
                    r,
                    22.0,
                    Shadow::new(
                        pf::alpha(Color::BLACK, 0.45 * a),
                        Offset::new(0.0, 10.0),
                        26.0,
                    ),
                );
                book.rrect(r, 22.0, pf::alpha(pf::SURFACE, 0.96 * a));
                book.stroke_rrect(r, 22.0, pf::alpha(Color::WHITE, 0.06 * a), 1.0);
                let st = stage(k);
                let st = Rect::new(
                    st.left,
                    st.top + (1.0 - a) * 24.0,
                    st.right,
                    st.bottom + (1.0 - a) * 24.0,
                );
                book.rrect(st, 14.0, pf::alpha(Color::rgb(14, 13, 19), a));
                match k {
                    0 => draw_model(book, st, sec, a),
                    1 => draw_physics(book, st, sec, a),
                    2 => draw_character(book, st, sec, a),
                    _ => {}
                }
            }
        }),
    )));

    // Titles and captions.
    let facts: [(&str, String); 4] = [
        (
            "Load 3D models (OBJ, STL).",
            format!("the vieww mark · {tris} triangles"),
        ),
        (
            "Things fall, bounce and settle.",
            "real rigid-body physics".to_string(),
        ),
        (
            "Characters that move on bones.",
            "a skeleton + keyframes".to_string(),
        ),
        (
            "Charts that match your theme.",
            "drawn from this film's own data".to_string(),
        ),
    ];
    for k in 0..4 {
        let a = appear(k);
        if a <= 0.01 {
            continue;
        }
        let r = card_rect(k);
        let dy = (1.0 - a) * 24.0;
        stack = stack.push(super::frame::label(
            r.left + 28.0,
            r.top + 22.0 + dy,
            CARD_W - 56.0,
            48.0,
            TITLES[k].to_string(),
            pf::geist(32.0).bold().color(pf::alpha(INK, 0.97)),
            TextAlign::Left,
            a,
        ));
        let dot = [BRAND_FAR, SYN_MACRO, TERM_GREEN, SYN_TYPE][k];
        stack = stack.push(
            Positioned::new()
                .left(r.right - 46.0)
                .top(r.top + 38.0 + dy)
                .width(16.0)
                .height(16.0)
                .child(Opacity::new(a).child(Painting::sized(
                    Size::new(16.0, 16.0),
                    PaintWith::new(move |b: &mut Sketchbook, _s: Size| {
                        b.circle(Offset::new(8.0, 8.0), 7.0, dot);
                    }),
                ))),
        );
        stack = stack.push(super::frame::label(
            r.left + 28.0,
            r.top + 458.0 + dy,
            CARD_W - 56.0,
            36.0,
            facts[k].0.to_string(),
            pf::geist(21.0).color(pf::alpha(INK, 0.92)),
            TextAlign::Left,
            a,
        ));
        stack = stack.push(super::frame::label(
            r.left + 28.0,
            r.top + 496.0 + dy,
            CARD_W - 56.0,
            30.0,
            facts[k].1.clone(),
            pf::geist_mono(17.0).color(pf::alpha(MUTED, 0.95)),
            TextAlign::Left,
            a,
        ));
    }

    // The charts card: a real DonutChart and ScatterChart from
    // vieww-widget, under a dark theme so they wear the film's colours.
    let a = appear(3);
    if a > 0.01 {
        stack = charts(stack, probe, t, a);
    }

    super::frame::headline(
        "More than screens: 3D, physics, characters and charts.",
        clamp01((t - 0.02) / 0.10),
    );
    stack = stack.push(super::frame::caption(
        "All built in — and each one is running live in this frame.",
        966.0,
        clamp01((t - 0.10) / 0.10),
    ));
    stack.into()
}

// ── 3D models ──────────────────────────────────────────────────────────────

fn draw_model(book: &mut Sketchbook, st: Rect, sec: f32, a: f32) {
    let c = Offset::new(
        (st.left + st.right) * 0.5,
        (st.top + st.bottom) * 0.5 - 10.0,
    );
    let view = View {
        cam: Camera {
            eye: Vec3::new(0.0, 0.0, -1200.0),
            target: Vec3::ZERO,
            fov: 2.0 * (540.0f32 / 1200.0).atan(),
        },
        canvas: Size::new(pf::W, pf::H),
        centre: c,
    };
    // A soft floor shadow, then the mark turning above it — a crisp
    // ring where a halo once sat: the model lit per face needs no bloom.
    book.layer(a, 16.0, None, |b| {
        b.rrect(
            pf::xywh(c.dx - 90.0, c.dy + 128.0, 180.0, 18.0),
            9.0,
            pf::alpha(Color::BLACK, 0.8),
        );
    });
    book.ring(c, 132.0, 1.2, pf::alpha(BRAND_NEAR, 0.22 * a));
    models::draw_mesh(
        book,
        &view,
        models::mark_mesh(),
        Pose3 {
            scale: 105.0,
            pitch: 0.22 * (sec * 0.8).sin(),
            yaw: sec * 0.9,
            at: Vec3::ZERO,
        },
        Look::brand(a),
    );
}

// ── Physics ────────────────────────────────────────────────────────────────

fn physics_jar(st: Rect) -> Jar {
    Jar {
        left: st.left + 18.0,
        top: st.top + 20.0,
        right: st.right - 18.0,
        bottom: st.bottom - 18.0,
    }
}

fn physics_drops(st: Rect) -> Vec<Drop> {
    let jar = physics_jar(st);
    let mut rng = crate::film_lib::Rng::new(0x9B11);
    (0..26)
        .map(|k| {
            let r = 16.0 + rng.f01() * 10.0;
            Drop {
                at: 1.4 + k as f32 * 0.22,
                x: jar.left + r + 2.0 + rng.f01() * (jar.right - jar.left - 2.0 * r - 4.0),
                r,
                gate: k % 4,
            }
        })
        .collect()
}

fn draw_physics(book: &mut Sketchbook, st: Rect, sec: f32, a: f32) {
    let jar = physics_jar(st);
    let drops = physics_drops(st);
    let colors = [BRAND_FAR, SYN_MACRO, SYN_TYPE, TERM_GREEN];
    for (di, at) in models::coins_at(jar, &drops, sec) {
        let d = drops[di];
        let fade = clamp01((at.dy - (jar.top - 20.0)) / 30.0) * a;
        let col = colors[d.gate];
        book.circle(at, d.r, pf::alpha(pf::mix(col, Color::BLACK, 0.3), fade));
        book.circle(at, d.r - 2.5, pf::alpha(col, fade));
        book.circle(
            Offset::new(at.dx - d.r * 0.35, at.dy - d.r * 0.4),
            d.r * 0.22,
            pf::alpha(Color::WHITE, 0.4 * fade),
        );
    }
}

// ── Characters ─────────────────────────────────────────────────────────────

fn draw_character(book: &mut Sketchbook, st: Rect, sec: f32, a: f32) {
    let origin = Offset::new((st.left + st.right) * 0.5, st.top + 214.0);
    let scale = 1.05;
    let (segs, head) = models::figure_at(sec, origin, scale);
    // Ground shadow.
    book.layer(a, 12.0, None, |b| {
        b.rrect(
            pf::xywh(origin.dx - 70.0, st.top + 330.0, 140.0, 14.0),
            7.0,
            pf::alpha(Color::BLACK, 0.8),
        );
    });
    // Drawn opaque inside one layer, so overlapping limbs don't show
    // through each other while the card fades in.
    book.layer(a, 0.0, None, |book| {
        // The body: thick, rounded limbs over the bones.
        let body = pf::alpha(TERM_GREEN, 1.0);
        for (p0, p1, w) in &segs {
            book.line(*p0, *p1, body, *w * 1.6);
            book.circle(*p0, *w * 0.8, body);
            book.circle(*p1, *w * 0.8, body);
        }
        book.circle(head, 27.0 * scale, body);
        book.circle(
            Offset::new(head.dx + 8.0, head.dy - 4.0),
            3.2,
            Color::rgb(10, 30, 16),
        );
        // The bones themselves, drawn thin on top — the skeleton the
        // pose is computed on.
        for (p0, p1, _) in &segs {
            book.line(*p0, *p1, pf::alpha(Color::WHITE, 0.75), 1.6);
            book.circle(*p0, 3.2, pf::alpha(Color::WHITE, 0.9));
        }
    });
}

// ── Charts ─────────────────────────────────────────────────────────────────

const CHART_COLORS: [Color; 5] = [BRAND_NEAR, SYN_TYPE, SYN_MACRO, TERM_GREEN, ACCENT];

fn charts(mut stack: Stack, probe: &pf::Probe, t: f32, a: f32) -> Stack {
    let st = stage(3);
    let dy = (1.0 - a) * 24.0;
    let fills = probe.shapes.saturating_sub(probe.strokes) as f32;
    let values: [(&'static str, f32); 5] = [
        ("fills", fills),
        ("lines", probe.strokes as f32),
        ("text", probe.glyph_runs as f32),
        ("shadows", probe.shadows as f32),
        ("layers", probe.layers as f32),
    ];
    let total: f32 = values.iter().map(|v| v.1).sum();
    // The donut sweeps in: each slice grows in turn, the rest of the
    // circle a transparent slice until the whole is drawn.
    let p = ease_out_expo(clamp01((t - 0.22) / 0.30));
    let shown = total * p;
    let mut cum = 0.0;
    let mut slices: Vec<(&'static str, f32)> = Vec::new();
    for (name, v) in values {
        slices.push((name, v.min((shown - cum).max(0.0))));
        cum += v;
    }
    slices.push(("", (total - shown).max(0.0)));
    let mut colors = CHART_COLORS.to_vec();
    colors.push(Color::rgba(0, 0, 0, 0));
    let d = 176.0;
    let (dx, dy0) = (st.left + 14.0, st.top + 14.0 + dy);
    if total > 0.0 {
        stack =
            stack.push(
                Positioned::new()
                    .left(dx)
                    .top(dy0)
                    .width(d)
                    .height(d)
                    .child(Opacity::new(a).child(Theme::new(ThemeData::dark()).child(
                        SizedBox::from_size(Size::new(d, d)).child(
                            Clip::oval().child(
                                DonutChart::new(slices).colors(colors).size(Size::new(d, d)),
                            ),
                        ),
                    ))),
            );
        let millions = total / 1.0e6;
        stack = stack.push(super::frame::label(
            dx,
            dy0 + d * 0.5 - 22.0,
            d,
            30.0,
            format!("{millions:.1}M"),
            pf::geist(24.0).bold().color(pf::alpha(INK, 0.97)),
            TextAlign::Center,
            a * p,
        ));
        stack = stack.push(super::frame::label(
            dx,
            dy0 + d * 0.5 + 6.0,
            d,
            22.0,
            "draws".to_string(),
            pf::geist_mono(14.0).color(pf::alpha(MUTED, 0.95)),
            TextAlign::Center,
            a * p,
        ));
        // The legend, large enough to read.
        for (i, (name, v)) in values.iter().enumerate() {
            let share = (v / total * 100.0).round();
            let y = dy0 + 14.0 + i as f32 * 31.0;
            let col = CHART_COLORS[i];
            stack = stack.push(
                Positioned::new()
                    .left(dx + d + 22.0)
                    .top(y + 7.0)
                    .width(12.0)
                    .height(12.0)
                    .child(Opacity::new(a).child(Painting::sized(
                        Size::new(12.0, 12.0),
                        PaintWith::new(move |b: &mut Sketchbook, _s: Size| {
                            b.rrect(pf::xywh(0.0, 0.0, 12.0, 12.0), 3.0, col);
                        }),
                    ))),
            );
            stack = stack.push(super::frame::label(
                dx + d + 42.0,
                y,
                140.0,
                26.0,
                format!("{name} {share:.0}%"),
                pf::geist_mono(16.0).color(pf::alpha(INK, 0.88)),
                TextAlign::Left,
                a * clamp01((p - 0.1 * i as f32) * 2.0),
            ));
        }
    }

    // The scatter: every raster sample the census timed.
    let samples = super::master::frame_samples();
    if !samples.is_empty() {
        let sp = clamp01((t - 0.40) / 0.20);
        let (sw, sh) = (st.width() - 64.0, 116.0);
        let (sx, sy) = (st.left + 46.0, st.bottom - sh - 36.0 + dy);
        stack = stack.push(
            Positioned::new()
                .left(sx)
                .top(sy)
                .width(sw)
                .height(sh)
                .child(
                    Opacity::new(a * sp).child(
                        Theme::new(ThemeData::dark()).child(
                            ScatterChart::new(samples.to_vec())
                                .color(pf::alpha(SYN_TYPE, 0.85))
                                .dot_radius(2.6)
                                .size(Size::new(sw, sh)),
                        ),
                    ),
                ),
        );
        stack = stack.push(
            Positioned::new()
                .left(sx - 4.0)
                .top(sy - 4.0)
                .width(sw + 8.0)
                .height(sh + 8.0)
                .child(Opacity::new(a * sp).child(Painting::sized(
                    Size::new(sw + 8.0, sh + 8.0),
                    PaintWith::new(move |b: &mut Sketchbook, _s: Size| {
                        b.line(
                            Offset::new(4.0, sh + 6.0),
                            Offset::new(sw + 4.0, sh + 6.0),
                            pf::alpha(Color::WHITE, 0.25),
                            1.0,
                        );
                        b.line(
                            Offset::new(2.0, 2.0),
                            Offset::new(2.0, sh + 6.0),
                            pf::alpha(Color::WHITE, 0.25),
                            1.0,
                        );
                    }),
                ))),
        );
        stack = stack.push(super::frame::label(
            sx,
            sy + sh + 8.0,
            sw,
            22.0,
            format!("{} frames: amount drawn →", samples.len()),
            pf::geist_mono(14.0).color(pf::alpha(MUTED, 0.95)),
            TextAlign::Right,
            a * sp,
        ));
        stack = stack.push(super::frame::label(
            st.left + 6.0,
            sy - 2.0,
            40.0,
            22.0,
            "ms".to_string(),
            pf::geist_mono(14.0).color(pf::alpha(MUTED, 0.95)),
            TextAlign::Left,
            a * sp,
        ));
    }
    stack
}
