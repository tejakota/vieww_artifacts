//! Movement II · THE ENGINE — what `vieww` is, and why it answers the
//! first movement's question.
//!
//! Three scenes, 28 seconds. The relief begins here, quietly: the
//! crate graph assembles, the frame pipeline spends its budget
//! deliberately, and the animation system shows itself as the
//! primitive it is — every curve drawn live by the same closed-form
//! functions the engine itself ships.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextAlign};
use vieww_widget::prelude::*;

use crate::film_lib::{clamp01, ease_in_out, ease_out_cubic, ease_out_expo, spring_out};
use crate::product_film as pf;
use super::filmkit as fk;
use super::{ACCENT, ACCENT_DEEP, BG_DEEP, BRAND_FAR, BRAND_NEAR, CANVAS, ENGINE, INK, LEDGER, MUTED, SYN_TYPE, CERT_CRATES};

/// The engine's headline crates — the ones a film can name without
/// lying. The full count is the workspace's own 36.
const GRAPH: [(&str, f32); 12] = [
    ("foundation", 0.0),
    ("text", 0.52),
    ("animation", 1.05),
    ("gestures", 1.57),
    ("widget", 2.09),
    ("element", 2.62),
    ("render", 3.14),
    ("paint", 3.67),
    ("scene", 4.19),
    ("effects", 4.71),
    ("platform", 5.24),
    ("studio", 5.76),
];

/// The frame budget, in milliseconds — the planner's whole religion.
const BUDGET_MS: f32 = 16.6;

// ── Z04 · the_engine ────────────────────────────────────────────────────────

pub fn the_engine(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = Stack::new();

    // The room.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(13, 12, 17)),
                    (0.6, BG_DEEP),
                    (1.0, Color::rgb(11, 10, 15)),
                ]),
            );
            // One glow behind the graph's centre — the engine's heart.
            let heart = ease_out_expo(clamp01(t / 0.20));
            if heart > 0.01 {
                let pulse = 1.0 + 0.06 * (sec * 1.4).sin();
                book.layer(heart, 52.0, None, |glow_book| {
                    glow_book.circle(Offset::new(w * 0.72, h * 0.52), 120.0 * pulse, pf::alpha(BRAND_NEAR, 0.13));
                });
            }
            // The machine room — real 3D boxes orbiting the heart, in
            // true perspective, drawn by the framework's own mesh
            // pipeline. The engine has depth because it has parts.
            fk::cube_orbit(
                book,
                s,
                Offset::new(w * 0.74, h * 0.52),
                sec,
                heart * 0.95,
                Color::rgb(11, 10, 15),
            );
            pf::vignette(book, w, h, 0.5);
        }),
    )));

    // The wordmark — left third. "vieww", the engine's name, with its
    // claim underneath.
    let name_a = ease_out_expo(clamp01(t / 0.12));
    if name_a > 0.01 {
        let rise = (1.0 - name_a) * 24.0;
        stack = stack.push(
            Positioned::new()
                .left(180.0)
                .top(400.0 + rise)
                .width(620.0)
                .height(240.0)
                .child(Opacity::new(name_a).child(
                    Flex::column()
                        .spacing(18.0)
                        .push(
                            Text::new("vieww".to_string())
                                .style(pf::geist(110.0).bold().letter_spacing(2.0).color(pf::alpha(INK, 0.98))),
                        )
                        .push(
                            Text::new("a UI engine, in Rust".to_string())
                                .style(pf::geist_mono(19.0).letter_spacing(2.2).color(pf::alpha(ACCENT, 0.9))),
                        )
                        .push(
                            Text::new("no browser · no bridge · no js runtime".to_string())
                                .style(pf::geist_mono(14.0).letter_spacing(1.6).color(pf::alpha(MUTED, 0.9))),
                        ),
                )),
        );
    }

    // The graph — right two-thirds: a centre node, two rings of crates,
    // edges drawing outward on a stagger.
    stack = stack.push(Positioned::new().left(1000.0).top(60.0).width(840.0).height(960.0).child(
        Painting::sized(Size::new(840.0, 960.0), PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            let cx = 420.0;
            let cy = 480.0;
            let heart = ease_out_expo(clamp01(t / 0.20));
            // The rings — faint guides.
            for (r, al) in [(140.0, 0.05), (260.0, 0.04)] {
                if heart > 0.2 {
                    book.ring(Offset::new(cx, cy), r, 1.0, pf::alpha(Color::WHITE, al));
                }
            }
            // The centre — the engine itself.
            if heart > 0.01 {
                let pulse = 1.0 + 0.05 * (sec * 1.4).sin();
                book.circle(Offset::new(cx, cy), 26.0 * heart * pulse, pf::alpha(BRAND_NEAR, 0.9));
                book.ring(Offset::new(cx, cy), 36.0 * heart, 1.4, pf::alpha(BRAND_FAR, 0.6 * heart));
            }
            // The crates — each edge draws, then its node blooms; a
            // rider pulses outward along the edge — the engine's
            // heartbeat, one pulse per crate.
            for (i, (name, phase)) in GRAPH.iter().enumerate() {
                let p = ease_out_cubic(clamp01((t - 0.16 - i as f32 * 0.045) / 0.16));
                if p <= 0.01 {
                    continue;
                }
                let ring = if i % 2 == 0 { 260.0 } else { 140.0 };
                let ang = *phase;
                let (dx, dy) = (ang.cos(), ang.sin());
                let end = Offset::new(cx + dx * ring * p, cy + dy * ring * p);
                // The edge.
                let mut path = Path::new();
                path.move_to(Offset::new(cx, cy)).line_to(end);
                book.stroke(path, pf::alpha(BRAND_FAR, 0.28 * p), 1.2);
                // The pulse riding the edge, always in motion.
                let ride = ((t * 0.5 + i as f32 * 0.13) % 1.0) * p;
                book.circle(
                    Offset::new(cx + (end.dx - cx) * ride, cy + (end.dy - cy) * ride),
                    2.2,
                    pf::alpha(BRAND_FAR, 0.8 * p),
                );
                // The node.
                book.circle(end, 6.5, pf::alpha(if i == GRAPH.len() - 1 { ACCENT } else { SYN_TYPE }, 0.9 * p));
                if i == GRAPH.len() - 1 {
                    book.ring(end, 11.0, 1.2, pf::alpha(ACCENT, 0.5 * p));
                }
                let _ = name; // labels are widgets, below
            }
        })),
    ));
    // The crates' labels — text, placed at the same angles the painting
    // used. One truth: the same `GRAPH`, the same trig.
    for (i, (name, phase)) in GRAPH.iter().enumerate() {
        let p = ease_out_cubic(clamp01((t - 0.20 - i as f32 * 0.045) / 0.14));
        if p <= 0.01 {
            continue;
        }
        let ring = if i % 2 == 0 { 260.0 } else { 140.0 };
        let (dx, dy) = (phase.cos(), phase.sin());
        let x = 1000.0 + 420.0 + dx * ring;
        let y = 60.0 + 480.0 + dy * ring;
        let align_right = dx < -0.1;
        stack = stack.push(
            Positioned::new()
                .left(x - (if align_right { 150.0 } else { 6.0 }))
                .top(y - 10.0)
                .width(150.0)
                .height(20.0)
                .child(Opacity::new(p).child(
                    Text::new(name.to_string())
                        .style(pf::geist_mono(13.0).letter_spacing(1.0).color(pf::alpha(if i == GRAPH.len() - 1 { ACCENT } else { MUTED }, 0.95)))
                        .align(if align_right { TextAlign::Right } else { TextAlign::Left }),
                )),
        );
    }

    // The crate count — the workspace's own number, counting up.
    let count_p = clamp01((t - 0.55) / 0.30);
    if count_p > 0.01 {
        let n = pf::count_up(CERT_CRATES as u64, count_p);
        stack = stack.push(
            Positioned::new()
                .left(180.0)
                .top(700.0)
                .width(620.0)
                .height(60.0)
                .child(Opacity::new(count_p.min(1.0)).child(
                    Text::new(format!("{n} crates · one core"))
                        .style(pf::geist(34.0).bold().color(pf::alpha(INK, 0.95))),
                )),
        );
    }

    // The film's voice.
    stack = stack.push(pf::caption("vieww — drawn once, rasterized anywhere.", 1002.0, clamp01((t - 0.10) / 0.10)));
    stack = stack.push(pf::caption("one core: widget → element → scene → native pixels.", 966.0, clamp01((t - 0.60) / 0.12)));
    stack = stack.push(pf::chrome(super::progress_rail(ctx.abs)));
    stack.into()
}

// ── Z05 · the_pipeline ──────────────────────────────────────────────────────

/// The pipeline's stations — what a frame walks through, and what each
/// stage is *for*. Five stops, one budget.
const STOPS: [(&str, &str); 5] = [
    ("widget tree", "compose"),
    ("element tree", "reconcile"),
    ("scene", "commands"),
    ("rasterizer", "4×4 coverage"),
    ("pixels", "the picture"),
];

pub fn the_pipeline(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;

    let mut stack = Stack::new();

    // The room.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            book.rect(Rect::new(0.0, 0.0, s.width, s.height), BG_DEEP);
            pf::vignette(book, s.width, s.height, 0.5);
        }),
    )));

    // The stations — five panels on one line, each arriving with its
    // connector; packets travel the line the whole scene.
    for (i, (title, sub)) in STOPS.iter().enumerate() {
        let t0 = 0.10 + i as f32 * 0.09;
        let p = ease_out_expo(clamp01((t - t0) / 0.20));
        if p <= 0.01 {
            continue;
        }
        let x = 150.0 + i as f32 * 336.0;
        let rise = (1.0 - p) * 18.0;
        let (title, sub) = (*title, *sub);
        // Each station lands as a slab in perspective — tilted in, then
        // resting at its own small yaw. The pipeline has sides.
        let yaw = (if i % 2 == 0 { -1.0 } else { 1.0 }) * (0.26 * (1.0 - p) + 0.07);
        let pitch = 0.05;
        let pi = i;
        stack = stack.push(
            Positioned::new()
                .left(x)
                .top(300.0 + rise)
                .width(264.0)
                .height(150.0)
                .child(Opacity::new(p).child(Painting::sized(
                    Size::new(264.0, 150.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        // The body, projected. The shadow stays flat —
                        // light doesn't tilt with the card.
                        book.shadow(pf::xywh(0.0, 6.0, 264.0, 150.0), 16.0, vieww_foundation::Shadow::new(pf::alpha(Color::BLACK, 0.4), Offset::new(0.0, 8.0), 22.0));
                        pf::panel_3d(book, vieww_foundation::Rect::new(0.0, 0.0, 264.0, 150.0), yaw, pitch, 1050.0, 1.0, |b| {
                            b.rrect(pf::xywh(0.0, 0.0, 264.0, 150.0), 16.0, pf::alpha(pf::SURFACE, 0.94));
                            b.stroke_rrect(pf::xywh(0.0, 0.0, 264.0, 150.0), 16.0, pf::alpha(if pi == 4 { ACCENT } else { Color::WHITE }, if pi == 4 { 0.35 } else { 0.06 }), 1.1);
                            // A spine of stage dots — five stages, five lights.
                            for d in 0..5 {
                                b.circle(
                                    Offset::new(22.0 + d as f32 * 12.0, 128.0),
                                    3.0,
                                    pf::alpha(if d <= pi { BRAND_FAR } else { pf::FAINT }, 0.7),
                                );
                            }
                        });
                    }),
                ))),
        );
        stack = stack.push(
            Positioned::new()
                .left(x + 20.0)
                .top(340.0)
                .width(224.0)
                .height(80.0)
                .child(Opacity::new(p).child(
                    Flex::column()
                        .spacing(6.0)
                        .push(
                            Text::new(title.to_string())
                                .style(pf::geist(22.0).bold().color(pf::alpha(INK, 0.95))),
                        )
                        .push(
                            Text::new(sub.to_string())
                                .style(pf::geist_mono(13.0).letter_spacing(1.4).color(pf::alpha(if i == 4 { ACCENT } else { ENGINE }, 0.85))),
                        ),
                )),
        );
        // The connector to the next station — a slight arc that draws
        // itself, then flows. The line is the pipeline.
        if i < STOPS.len() - 1 {
            let wire_p = ease_out_cubic(clamp01((t - t0 - 0.08) / 0.16));
            if wire_p > 0.01 {
                let x0 = x + 264.0;
                let from = Offset::new(x0, 372.0);
                let to = Offset::new(x0 + 72.0, 372.0);
                let phase = t * 2.0 + i as f32 * 0.7;
                stack = stack.push(Positioned::new().left(x0 - 8.0).top(340.0).width(96.0).height(64.0).child(
                    Painting::sized(Size::new(96.0, 64.0), PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        let pts = fk::thread_pts(
                            Offset::new(8.0, 32.0),
                            Offset::new(80.0, 32.0),
                            14.0,
                        );
                        fk::grow_stroke(book, &pts, wire_p, BRAND_FAR, 2.0, 0.55);
                        let flow = clamp01((wire_p - 0.8) * 5.0);
                        if flow > 0.01 {
                            fk::flow_along(book, &pts, phase, BRAND_FAR, flow * 0.8, 1.5, 0.4);
                        }
                        let _ = (from, to);
                    })),
                ));
            }
        }
    }

    // The packets — small lights riding the line, on a fixed beat.
    let beat = t * 4.0;
    let packets = 3;
    stack = stack.push(Positioned::new().left(150.0).top(0.0).width(1680.0).height(1080.0).child(
        Painting::sized(Size::new(1680.0, 1080.0), PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            for k in 0..packets {
                let ph = beat + k as f32 * 0.33;
                let pos = ph.fract();
                let x = pos * (5.0 * 336.0 - 72.0);
                let a = (pos * 4.0).min(1.0) * ((1.0 - pos) * 4.0).min(1.0);
                if a <= 0.01 {
                    continue;
                }
                book.layer(a, 8.0, None, |b| {
                    b.circle(Offset::new(x, 372.0), 5.0, pf::alpha(BRAND_FAR, 0.9));
                });
            }
        })),
    ));

    // The budget bar — the worst certified frame against the 16.6 ms
    // budget. Real numbers, from the repo's own audit.
    let bar_p = ease_out_expo(clamp01((t - 0.55) / 0.25));
    if bar_p > 0.01 {
        let total_w = 1560.0;
        let worst = pf::CERT_WORST_MS / BUDGET_MS;
        let p95 = pf::CERT_P95_MS / BUDGET_MS;
        stack = stack.push(Positioned::new().left(180.0).top(560.0).width(1560.0).height(120.0).child(
            Painting::sized(Size::new(1560.0, 120.0), PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                // The track.
                book.rrect(pf::xywh(0.0, 40.0, total_w, 14.0), 7.0, pf::alpha(Color::WHITE, 0.05));
                // The budget's end — a hairline the bar must not cross.
                book.line(Offset::new(total_w - 1.0, 30.0), Offset::new(total_w - 1.0, 64.0), pf::alpha(pf::FAINT, 0.8), 1.0);
                // p95, then worst — two fills, one truth.
                book.rrect(pf::xywh(0.0, 40.0, total_w * p95 * bar_p, 14.0), 7.0, pf::alpha(ENGINE, 0.75));
                book.rrect(pf::xywh(0.0, 40.0, total_w * worst * bar_p, 14.0), 7.0, pf::alpha(ACCENT_DEEP, 0.30));
            })),
        ));
        stack = stack.push(
            Positioned::new()
                .left(180.0)
                .top(520.0)
                .width(1560.0)
                .height(32.0)
                .child(
                    Text::new("the frame budget — 16.6 ms".to_string())
                        .style(pf::geist_mono(15.0).letter_spacing(2.2).color(pf::alpha(MUTED, 0.9))),
                ),
        );
        stack = stack.push(
            Positioned::new()
                .left(180.0)
                .top(588.0)
                .width(1560.0)
                .height(90.0)
                .child(
                    Flex::row()
                        .spacing(26.0)
                        .push(
                            Text::new(format!("p95 {:.1} ms", pf::CERT_P95_MS))
                                .style(pf::geist_mono(14.0).letter_spacing(1.0).color(pf::alpha(ENGINE, 0.95))),
                        )
                        .push(
                            Text::new(format!("worst {:.1} ms", pf::CERT_WORST_MS))
                                .style(pf::geist_mono(14.0).letter_spacing(1.0).color(pf::alpha(ACCENT_DEEP, 0.95))),
                        )
                        .push(
                            Text::new("measured on a 2-core container".to_string())
                                .style(pf::geist_mono(13.0).letter_spacing(0.8).color(pf::alpha(pf::FAINT, 0.9))),
                        ),
                ),
        );
    }

    // The damage callout — one dirty rect, one repaint.
    let dmg_p = ease_out_expo(clamp01((t - 0.70) / 0.2));
    if dmg_p > 0.01 {
        stack = stack.push(Positioned::new().left(180.0).top(740.0).width(1560.0).height(190.0).child(
            Painting::sized(Size::new(1560.0, 190.0), PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                // The window — faint content lines, mostly asleep.
                book.rrect(pf::xywh(0.0, 0.0, 1560.0, 190.0), 14.0, pf::alpha(pf::SURFACE, 0.5 * dmg_p));
                for row in 0..5 {
                    let y = 26.0 + row as f32 * 32.0;
                    book.rrect(pf::xywh(28.0, y, 620.0 - (row as f32 * 60.0), 9.0), 4.5, pf::alpha(Color::WHITE, 0.05 * dmg_p));
                }
                // The dirty rect — the only thing the frame repaints.
                let blink = 0.55 + 0.45 * (t * 6.0).sin();
                book.rrect(pf::xywh(920.0, 40.0, 320.0, 110.0), 10.0, pf::alpha(ACCENT_DEEP, 0.10 * dmg_p));
                book.stroke_rrect(pf::xywh(920.0, 40.0, 320.0, 110.0), 10.0, pf::alpha(ACCENT, blink * dmg_p), 1.6);
                // Its hatch — the damage overlay's own visual grammar.
                for hx in 0..8 {
                    let x0 = 932.0 + hx as f32 * 38.0;
                    let mut path = Path::new();
                    path.move_to(Offset::new(x0, 148.0)).line_to(Offset::new((x0 + 40.0).min(1228.0), 52.0));
                    book.stroke(path, pf::alpha(ACCENT, 0.16 * blink * dmg_p), 1.0);
                }
            })),
        ));
        stack = stack.push(
            Positioned::new()
                .left(180.0)
                .top(880.0)
                .width(900.0)
                .height(28.0)
                .child(Opacity::new(dmg_p).child(
                    Text::new("damage, not full-screen repaints — one dirty rect, one redraw".to_string())
                        .style(pf::geist_mono(14.0).letter_spacing(1.0).color(pf::alpha(ACCENT, 0.9))),
                )),
        );
    }

    // The film's voice.
    stack = stack.push(pf::caption("a frame is a budget. the planner spends it.", 1002.0, clamp01((t - 0.10) / 0.10)));
    stack = stack.push(pf::chrome(super::progress_rail(ctx.abs)));
    stack.into()
}

// ── Z06 · the_motion ────────────────────────────────────────────────────────

/// The three curves — name, function, and the claim each one makes.
/// The functions are the engine's own (`film_lib` mirrors
/// `vieww_animation`'s closed forms), so what is drawn is what ships.
const LANES: [&str; 3] = ["ease_out_cubic — decelerate", "spring_out — overshoot", "ease_in_out — mirror"];

pub fn the_motion(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = Stack::new();

    // The room.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            book.rect(Rect::new(0.0, 0.0, s.width, s.height), BG_DEEP);
            pf::vignette(book, s.width, s.height, 0.52);
        }),
    )));

    // The lanes — each a curve drawn live, a ball riding it, and the
    // API names that produce exactly this behaviour.
    for i in 0..3 {
        let t0 = 0.10 + i as f32 * 0.12;
        let p = ease_out_expo(clamp01((t - t0) / 0.22));
        if p <= 0.01 {
            continue;
        }
        let lane_y = 240.0 + i as f32 * 210.0;
        let (x0, y0, w, h) = (400.0, lane_y, 900.0, 130.0);
        let lane_i = i;
        stack = stack.push(Positioned::new().left(x0 - 40.0).top(y0 - 46.0).width(w + 320.0).height(h + 90.0).child(
            Painting::sized(Size::new(w + 320.0, h + 90.0), PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let ox = 40.0;
                let oy = 46.0;
                // The lane's baseline and end tick.
                book.line(Offset::new(ox, oy + h), Offset::new(ox + w, oy + h), pf::alpha(Color::WHITE, 0.08), 1.0);
                book.line(Offset::new(ox + w, oy + h), Offset::new(ox + w, oy), pf::alpha(Color::WHITE, 0.08), 1.0);
                // The curve, drawn to its own progress — one self-
                // drawing stroke, then a comet riding it at the film's
                // own clock.
                let steps = 72;
                let mut curve: Vec<Offset> = Vec::new();
                for s_i in 0..=steps {
                    let u = s_i as f32 / steps as f32;
                    let v = match lane_i {
                        0 => ease_out_cubic(u),
                        1 => spring_out(u, 8.0, 0.55).clamp(-0.15, 1.35),
                        _ => ease_in_out(u),
                    };
                    // Map v (which may overshoot) into the lane.
                    let y = oy + h - v * h * 0.72 - h * 0.14;
                    curve.push(Offset::new(ox + u * w, y.max(oy - 26.0)));
                }
                fk::grow_stroke(book, &curve, p, if lane_i == 1 { ACCENT } else { ENGINE }, 2.4, 0.85);
                // The ball — with its trail, so the easing is legible
                // as motion, not just position.
                let u = t;
                let v = match lane_i {
                    0 => ease_out_cubic(u.clamp(0.0, 1.0)),
                    1 => spring_out(u.clamp(0.0, 1.0), 8.0, 0.55).clamp(-0.15, 1.35),
                    _ => ease_in_out(u.clamp(0.0, 1.0)),
                };
                let _ = v;
                fk::rider(book, &curve, u.clamp(0.0, 1.0), if lane_i == 1 { ACCENT } else { SYN_TYPE }, 5.5, 0.95);
                // The scrub window — one cycle of the beat, restarted
                // every 1.6 s so the ball never stops teaching.
                let _ = sec;
            })),
        ));
        // The lane's name — left of its curve.
        stack = stack.push(
            Positioned::new()
                .left(150.0)
                .top(lane_y + 10.0)
                .width(240.0)
                .height(56.0)
                .child(Opacity::new(p).child(
                    Flex::column().spacing(4.0).push(
                        Text::new(LANES[i].to_string())
                            .style(pf::geist_mono(15.0).letter_spacing(1.1).color(pf::alpha(INK, 0.92))),
                    ),
                )),
        );
    }

    // The claim — right column, the API spelled out.
    let claim_p = clamp01((t - 0.55) / 0.2);
    if claim_p > 0.01 {
        stack = stack.push(
            Positioned::new()
                .left(1420.0)
                .top(300.0)
                .width(340.0)
                .height(360.0)
                .child(Opacity::new(claim_p).child(
                    Flex::column()
                        .spacing(14.0)
                        .push(
                            Text::new("Tween · Curve".to_string())
                                .style(pf::geist_mono(16.0).letter_spacing(1.2).color(pf::alpha(SYN_TYPE, 0.95))),
                        )
                        .push(
                            Text::new("Spring · closed form".to_string())
                                .style(pf::geist_mono(16.0).letter_spacing(1.2).color(pf::alpha(ACCENT, 0.95))),
                        )
                        .push(
                            Text::new("Ticker · one clock".to_string())
                                .style(pf::geist_mono(16.0).letter_spacing(1.2).color(pf::alpha(LEDGER, 0.95))),
                        )
                        .push(
                            Text::new("deterministic — the same\nframe, every render".to_string())
                                .style(pf::geist_mono(13.0).letter_spacing(0.8).color(pf::alpha(MUTED, 0.9))),
                        ),
                )),
        );
    }

    // The film's voice.
    stack = stack.push(pf::caption("motion is not a library here. it is a primitive.", 1002.0, clamp01((t - 0.10) / 0.10)));
    stack = stack.push(pf::chrome(super::progress_rail(ctx.abs)));
    stack.into()
}

