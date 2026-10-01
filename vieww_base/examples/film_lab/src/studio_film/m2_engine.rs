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
use crate::three_d::Vec3;
use super::filmkit as fk;
use super::{ACCENT, ACCENT_DEEP, BG_DEEP, BRAND_FAR, BRAND_NEAR, CANVAS, ENGINE, INK, LEDGER, MUTED, SYN_TYPE, CERT_CRATES};

/// The engine's headline crates — the ones a film can name without
/// lying. The full count is the workspace's own 49 (see Z19, which shows
/// every one of them by name); these twelve are the load-bearing ones
/// the graph can name at label size.
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

/// The crate graph's centre and its two rings.
const GRAPH_C: Offset = Offset::new(1370.0, 610.0);
const RING_IN: f32 = 130.0;
const RING_OUT: f32 = 250.0;

// ── Z04 · the_engine ────────────────────────────────────────────────────────

pub fn the_engine(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = Stack::new();

    // The room.
    super::frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            let _ = (w, h);
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(13, 12, 17)),
                    (0.6, BG_DEEP),
                    (1.0, Color::rgb(11, 10, 15)),
                ]),
            );
            pf::vignette(book, w, h, 0.5);
        }),
    )));
    // (The glow that once sat behind the graph's centre is gone with the
    // film's radial glows — a blurred halo bands on the encode. The heart
    // of the engine is the mark and its ring, drawn crisp below.)

    // The wordmark — left third. "vieww", the engine's name, with its
    // claim underneath.
    let name_a = ease_out_expo(clamp01(t / 0.12));
    if name_a > 0.01 {
        let rise = (1.0 - name_a) * 24.0;
        stack = stack.push(
            Positioned::new()
                .left(180.0)
                .top(380.0 + rise)
                .width(760.0)
                .height(300.0)
                .child(Opacity::new(name_a).child(
                    Flex::column()
                        .spacing(18.0)
                        .push(
                            Text::new("vieww".to_string())
                                .style(pf::geist(110.0).bold().letter_spacing(2.0).color(pf::alpha(INK, 0.98))),
                        )
                        .push(
                            Text::new("one engine for every screen".to_string())
                                .style(pf::geist_mono(26.0).letter_spacing(2.0).color(pf::alpha(ACCENT, 0.95))),
                        )
                        .push(
                            Text::new("no browser · no translator · built in Rust".to_string())
                                .style(pf::geist_mono(20.0).letter_spacing(1.2).color(pf::alpha(MUTED, 0.92))),
                        ),
                )),
        );
    }

    // The machine room — real 3D cubes orbiting the core in true
    // perspective, passing in front of and behind the graph; they
    // settle in as the crate count completes.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let heart = ease_out_expo(clamp01(t / 0.20));
            fk::cube_orbit(book, s, Offset::new(GRAPH_C.dx, GRAPH_C.dy + 20.0), sec, heart * 0.95, Color::rgb(11, 10, 15), clamp01((t - 0.52) / 0.38));
        }),
    )));
    // The graph — right two-thirds: a centre node, two rings of crates,
    // edges drawing outward on a stagger.
    // The graph — right half: a centre node, two rings of crates, edges
    // drawing outward on a stagger. Painted in the canvas's own
    // coordinates, so the labels below can use the very same trig.
    stack = stack.push(Positioned::fill().child(
        Painting::sized(CANVAS, PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            let (cx, cy) = (GRAPH_C.dx, GRAPH_C.dy);
            let heart = ease_out_expo(clamp01(t / 0.20));
            for (r, al) in [(RING_IN, 0.06), (RING_OUT, 0.05)] {
                if heart > 0.2 {
                    book.ring(Offset::new(cx, cy), r, 1.0, pf::alpha(Color::WHITE, al));
                }
            }
            if heart > 0.01 {
                let pulse = 1.0 + 0.05 * (sec * 1.4).sin();
                // The core is the vieww mark itself, flat, inside its ring
                // — and a second, wider ring where the old halo was: the
                // heart drawn, not blurred.
                book.ring(Offset::new(cx, cy), 86.0 * pulse, 1.1, pf::alpha(BRAND_NEAR, 0.30 * heart));
                super::models::draw_mark_2d(book, Offset::new(cx, cy), 34.0 * (0.7 + 0.3 * heart) * pulse, heart);
                book.ring(Offset::new(cx, cy), 60.0 * heart, 1.4, pf::alpha(BRAND_FAR, 0.6 * heart));
            }
            for (i, (_, phase)) in GRAPH.iter().enumerate() {
                let p = ease_out_cubic(clamp01((t - 0.16 - i as f32 * 0.045) / 0.16));
                if p <= 0.01 {
                    continue;
                }
                let ring = if i % 2 == 0 { RING_OUT } else { RING_IN };
                let (dx, dy) = (phase.cos(), phase.sin());
                let end = Offset::new(cx + dx * ring * p, cy + dy * ring * p);
                let mut path = Path::new();
                path.move_to(Offset::new(cx, cy)).line_to(end);
                book.stroke(path, pf::alpha(BRAND_FAR, 0.30 * p), 1.3);
                let ride = ((t * 0.5 + i as f32 * 0.13) % 1.0) * p;
                book.circle(
                    Offset::new(cx + (end.dx - cx) * ride, cy + (end.dy - cy) * ride),
                    2.6,
                    pf::alpha(BRAND_FAR, 0.8 * p),
                );
                let last = i == GRAPH.len() - 1;
                book.circle(end, 8.0, pf::alpha(if last { ACCENT } else { SYN_TYPE }, 0.92 * p));
                if last {
                    book.ring(end, 14.0, 1.3, pf::alpha(ACCENT, 0.5 * p));
                }
            }
        })),
    ));
    // The crates' labels — outside their node, along the same ray.
    for (i, (name, phase)) in GRAPH.iter().enumerate() {
        let p = ease_out_cubic(clamp01((t - 0.20 - i as f32 * 0.045) / 0.14));
        if p <= 0.01 {
            continue;
        }
        let ring = if i % 2 == 0 { RING_OUT } else { RING_IN };
        let (dx, dy) = (phase.cos(), phase.sin());
        let r = ring + 22.0;
        let (x, y) = (GRAPH_C.dx + dx * r, GRAPH_C.dy + dy * r);
        let (left, align) = if dx > 0.35 {
            (x, TextAlign::Left)
        } else if dx < -0.35 {
            (x - 180.0, TextAlign::Right)
        } else {
            (x - 90.0, TextAlign::Center)
        };
        let top = if dx.abs() <= 0.35 { if dy < 0.0 { y - 30.0 } else { y - 2.0 } } else { y - 16.0 };
        let last = i == GRAPH.len() - 1;
        stack = stack.push(super::frame::label(
            left, top, 180.0, 32.0,
            name.to_string(),
            pf::geist_mono(19.0).letter_spacing(0.8).color(pf::alpha(if last { ACCENT } else { INK }, if last { 0.98 } else { 0.8 })),
            align, p,
        ));
    }

    // The crate count — the workspace's own number, counting up.
    let count_p = clamp01((t - 0.55) / 0.30);
    if count_p > 0.01 {
        let n = pf::count_up(CERT_CRATES as u64, count_p);
        stack = stack.push(super::frame::label(
            180.0, 690.0, 700.0, 60.0,
            format!("{n} crates · one core"),
            pf::geist(40.0).bold().color(pf::alpha(INK, 0.95)),
            TextAlign::Left, count_p.min(1.0),
        ));
    }

    // The film's voice.
    stack = stack.push(super::frame::caption("Meet vieww — the engine underneath.", 1002.0, clamp01((t - 0.10) / 0.10)));
    stack = stack.push(super::frame::caption("Describe your interface once. vieww draws it, pixel for pixel, everywhere.", 966.0, clamp01((t - 0.60) / 0.12)));
    stack.into()
}

// ── Z05 · the_pipeline ──────────────────────────────────────────────────────

/// The pipeline's stations — what a frame walks through, and what each
/// stage is *for*. Five stops, one budget.
const STOPS: [(&str, &str); 5] = [
    ("your layout", "describe"),
    ("what changed", "compare"),
    ("what to draw", "plan"),
    ("drawing", "paint"),
    ("screen", "show"),
];

pub fn the_pipeline(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = Stack::new();

    // The room.
    super::frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            let _ = (w, h);
            book.rect(Rect::new(0.0, 0.0, s.width, s.height), BG_DEEP);
            pf::vignette(book, s.width, s.height, 0.5);
        }),
    )));
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {


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
        let pi = i;
        stack = stack.push(
            Positioned::new()
                .left(x)
                .top(322.0 + rise)
                .width(264.0)
                .height(160.0)
                .child(Opacity::new(p).child(Painting::sized(
                    Size::new(264.0, 160.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        book.shadow(pf::xywh(0.0, 6.0, 264.0, 150.0), 16.0, vieww_foundation::Shadow::new(pf::alpha(Color::BLACK, 0.4), Offset::new(0.0, 8.0), 22.0));
                        book.rrect(pf::xywh(0.0, 0.0, 264.0, 150.0), 16.0, pf::alpha(pf::SURFACE, 0.94));
                        book.stroke_rrect(pf::xywh(0.0, 0.0, 264.0, 150.0), 16.0, pf::alpha(if pi == 4 { ACCENT } else { Color::WHITE }, if pi == 4 { 0.40 } else { 0.08 }), 1.1);
                        // A spine of stage dots — five stages, five lights,
                        // centred under the station's name.
                        for d in 0..5 {
                            book.circle(
                                Offset::new(132.0 - 24.0 + d as f32 * 12.0, 126.0),
                                3.0,
                                pf::alpha(if d <= pi { BRAND_FAR } else { pf::FAINT }, 0.75),
                            );
                        }
                    }),
                ))),
        );
        stack = stack.push(super::frame::label(x, 322.0 + rise + 26.0, 264.0, 40.0, title.to_string(),
            pf::geist(28.0).bold().color(pf::alpha(INK, 0.96)), TextAlign::Center, p));
        stack = stack.push(super::frame::label(x, 322.0 + rise + 68.0, 264.0, 30.0, sub.to_string(),
            pf::geist_mono(19.0).letter_spacing(0.8).color(pf::alpha(if i == 4 { ACCENT } else { ENGINE }, 0.92)), TextAlign::Center, p));
        // The connector to the next station — a slight arc that draws
        // itself, then flows. The line is the pipeline.
        if i < STOPS.len() - 1 {
            let wire_p = ease_out_cubic(clamp01((t - t0 - 0.08) / 0.16));
            if wire_p > 0.01 {
                let x0 = x + 264.0;
                let from = Offset::new(x0, 394.0);
                let to = Offset::new(x0 + 72.0, 394.0);
                let phase = t * 2.0 + i as f32 * 0.7;
                stack = stack.push(Positioned::new().left(x0 - 8.0).top(362.0).width(96.0).height(64.0).child(
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

    // The packets — small lights riding the line, on a fixed beat. They
    // wait for the line to exist: the stations land first (the last of
    // them settles at ~7.9 s), then the traffic starts. Riding the wire
    // from the scene's first frame put moving circles on a dark screen
    // no box had arrived at yet — traffic before the road. The gate runs
    // on `sec`, the real clock, not the scene's normalised `t`.
    let line_ready = clamp01((sec - 8.4) / 0.6);
    if line_ready > 0.01 {
        let beat = (sec - 8.4) * 4.0;
        let packets = 3;
        stack = stack.push(Positioned::new().left(150.0).top(284.0).width(1680.0).height(676.0).child(
            Painting::sized(Size::new(1680.0, 1080.0), PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                for k in 0..packets {
                    let ph = beat + k as f32 * 0.33;
                    let pos = ph.fract();
                    let x = pos * (5.0 * 336.0 - 72.0);
                    let a = (pos * 4.0).min(1.0) * ((1.0 - pos) * 4.0).min(1.0) * line_ready;
                    if a <= 0.01 {
                        continue;
                    }
                    // A crisp signal dot — core plus ring, no blur.
                    book.ring(Offset::new(x, 110.0), 8.0, 1.1, pf::alpha(BRAND_FAR, 0.55 * a));
                    book.circle(Offset::new(x, 110.0), 3.4, pf::alpha(BRAND_FAR, 0.9 * a));
                }
            })),
        ));
    }

    // The budget bar — the worst certified frame against the 16.6 ms
    // budget. Real numbers, from the repo's own audit.
    let bar_p = ease_out_expo(clamp01((t - 0.55) / 0.25));
    if bar_p > 0.01 {
        let total_w = 1560.0;
        let worst = pf::CERT_WORST_MS / BUDGET_MS;
        let p95 = pf::CERT_P95_MS / BUDGET_MS;
        stack = stack.push(Positioned::new().left(180.0).top(566.0).width(1560.0).height(120.0).child(
            Painting::sized(Size::new(1560.0, 120.0), PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                // The track.
                book.rrect(pf::xywh(0.0, 40.0, total_w, 14.0), 7.0, pf::alpha(Color::WHITE, 0.05));
                // The budget's end — a hairline the bar must not cross.
                book.line(Offset::new(total_w - 1.0, 30.0), Offset::new(total_w - 1.0, 64.0), pf::alpha(pf::FAINT, 0.8), 1.0);
                // p95, then worst — two fills, one truth.
                book.rrect(pf::xywh(0.0, 40.0, total_w * p95 * bar_p, 14.0), 7.0, pf::alpha(ENGINE, 0.75));
                book.rrect(pf::xywh(0.0, 40.0, total_w * worst * bar_p, 14.0), 7.0, pf::alpha(BRAND_FAR, 0.35));
            })),
        ));
        stack = stack.push(super::frame::label(180.0, 536.0, 1560.0, 36.0, "time allowed for one smooth frame — 16.6 ms".to_string(),
            pf::geist_mono(22.0).letter_spacing(1.6).color(pf::alpha(INK, 0.9)), TextAlign::Left, 1.0));
        let row = [
            (format!("typical {:.1} ms", pf::CERT_P95_MS), ENGINE, 0.0),
            (format!("slowest {:.1} ms", pf::CERT_WORST_MS), BRAND_FAR, 260.0),
            ("measured on a small 2-core machine".to_string(), pf::FAINT, 560.0),
        ];
        for (text, color, dx) in row {
            stack = stack.push(super::frame::label(180.0 + dx, 650.0, 700.0, 32.0, text,
                pf::geist_mono(20.0).letter_spacing(0.6).color(pf::alpha(color, 0.95)), TextAlign::Left, 1.0));
        }
    }

    // The damage callout — one dirty rect, one repaint.
    let dmg_p = ease_out_expo(clamp01((t - 0.70) / 0.2));
    if dmg_p > 0.01 {
        stack = stack.push(Positioned::new().left(180.0).top(722.0).width(1560.0).height(190.0).child(
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
        stack = stack.push(super::frame::label(180.0, 918.0, 1560.0, 32.0,
            "only the part that changed is redrawn — never the whole screen".to_string(),
            pf::geist_mono(20.0).letter_spacing(0.6).color(pf::alpha(ACCENT, 0.95)), TextAlign::Left, dmg_p));
    }

    // The film's voice.
    stack = stack.push(super::frame::caption("Smooth means every frame arrives on time.", 1002.0, clamp01((t - 0.10) / 0.10)));
    stack.into()
}

// ── Z06 · the_motion ────────────────────────────────────────────────────────

/// The three curves — name, function, and the claim each one makes.
/// The functions are the engine's own (`film_lib` mirrors
/// `vieww_animation`'s closed forms), so what is drawn is what ships.
const LANES: [&str; 3] = ["glide — slows to a stop", "spring — a little bounce", "smooth — eases in and out"];

pub fn the_motion(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = Stack::new();

    // The room.
    super::frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            let _ = (w, h);
            book.rect(Rect::new(0.0, 0.0, s.width, s.height), BG_DEEP);
            pf::vignette(book, s.width, s.height, 0.52);
        }),
    )));
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {


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
        let lane_y = 330.0 + i as f32 * 178.0;
        let (x0, y0, w, h) = (410.0, lane_y, 720.0, 112.0);
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
        // The lane's name — left of its curve: the function, then what it
        // feels like, on two lines that cannot wrap into each other.
        let (func, feel) = LANES[i].split_once(" — ").unwrap_or((LANES[i], ""));
        stack = stack.push(super::frame::label(96.0, lane_y + 20.0, 300.0, 36.0, func.to_string(),
            pf::geist_mono(24.0).letter_spacing(0.6).color(pf::alpha(INK, 0.96)), TextAlign::Left, p));
        stack = stack.push(super::frame::label(96.0, lane_y + 58.0, 300.0, 30.0, feel.to_string(),
            pf::geist_mono(19.0).letter_spacing(0.8).color(pf::alpha(if i == 1 { ACCENT } else { ENGINE }, 0.92)), TextAlign::Left, p));
    }

    // The specimens — one card per lane, driven by that lane's own
    // easing along a shared rail, on a shared 2.4 s loop. The curve
    // above says what the easing is; the card says what it feels like.
    let spec_p = clamp01((t - 0.30) / 0.18);
    if spec_p > 0.01 {
        let loop_u = clamp01(((sec % 2.4) / 1.55).min(1.0));
        stack = stack.push(Positioned::fill().child(Painting::sized(
            CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                for i in 0..3 {
                    let lane_y = 330.0 + i as f32 * 178.0;
                    let (rx0, rx1) = (1270.0, 1776.0);
                    let cy = lane_y + 56.0;
                    let a = spec_p;
                    // The rail the specimen travels, and its two stops.
                    book.rect(
                        Rect::new(rx0, cy - 0.5, rx1, cy + 0.5),
                        pf::alpha(Color::WHITE, 0.10 * a),
                    );
                    for x in [rx0, rx1] {
                        book.rect(
                            Rect::new(x - 0.75, cy - 26.0, x + 0.75, cy + 26.0),
                            pf::alpha(Color::WHITE, 0.16 * a),
                        );
                    }
                    let v = match i {
                        0 => ease_out_cubic(loop_u),
                        1 => spring_out(loop_u, 8.0, 0.55).clamp(-0.18, 1.32),
                        _ => ease_in_out(loop_u),
                    };
                    let color = if i == 1 { ACCENT } else { ENGINE };
                    let cw = 86.0;
                    let cx = rx0 + (rx1 - rx0 - cw) * v;
                    // The trail — where the specimen has just been,
                    // which is the part of an easing you cannot draw
                    // on an axis.
                    for k in 1..5 {
                        let back = clamp01(loop_u - k as f32 * 0.035);
                        let bv = match i {
                            0 => ease_out_cubic(back),
                            1 => spring_out(back, 8.0, 0.55).clamp(-0.18, 1.32),
                            _ => ease_in_out(back),
                        };
                        let bx = rx0 + (rx1 - rx0 - cw) * bv;
                        book.rrect(
                            pf::xywh(bx, cy - 21.0, cw, 42.0),
                            11.0,
                            pf::alpha(color, 0.06 * a * (5 - k) as f32 / 5.0),
                        );
                    }
                    book.rrect(pf::xywh(cx, cy - 21.0, cw, 42.0), 11.0, pf::alpha(color, 0.85 * a));
                    book.stroke_rrect(
                        pf::xywh(cx, cy - 21.0, cw, 42.0),
                        11.0,
                        pf::alpha(Color::WHITE, 0.18 * a),
                        1.0,
                    );
                }
            }),
        )));
        // The rail's one label — said once, over the top lane.
        stack = stack.push(super::frame::label(1270.0, 284.0, 520.0, 30.0, "one clock, three feels".to_string(),
            pf::geist_mono(19.0).letter_spacing(1.2).color(pf::alpha(MUTED, 0.92)), TextAlign::Left, spec_p));
    }

    // The claim — the API spelled out, under the specimens.
    let claim_p = clamp01((t - 0.55) / 0.2);
    if claim_p > 0.01 {
        stack = stack.push(super::frame::label(96.0, 850.0, 1680.0, 34.0,
            "glides, springs and curves — all built in".to_string(),
            pf::geist_mono(22.0).letter_spacing(0.8).color(pf::alpha(SYN_TYPE, 0.96)), TextAlign::Center, claim_p));
        stack = stack.push(super::frame::label(96.0, 890.0, 1680.0, 30.0,
            "and they play back exactly the same, every time".to_string(),
            pf::geist_mono(19.0).letter_spacing(0.8).color(pf::alpha(MUTED, 0.92)), TextAlign::Center, claim_p));
    }

    // The film's voice.
    stack = stack.push(super::frame::caption("Motion is built in, not bolted on.", 1002.0, clamp01((t - 0.10) / 0.10)));
    stack.into()
}


// ── Z07 · the_layers ────────────────────────────────────────────────────────

/// **One core: widget → element → scene → pixels.** Z06 named the engine;
/// this scene opens it. Four planes in true perspective, stacked back to
/// front, each one a real stage of the framework — and a single change
/// falling through all four, so the stack is shown working rather than
/// labelled.
///
/// The planes are drawn with the framework's own quadrant projector, so
/// the depth is projection and not a skew.
pub fn the_layers(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let mut stack = Stack::new();

    /// (name, what it holds, colour, the crate it lives in)
    const LAYERS: [(&str, &str, Color, &str); 4] = [
        ("widgets", "what you describe", ACCENT, "vieww-widget"),
        ("elements", "what it remembers", SYN_TYPE, "vieww-element"),
        ("scene", "what to draw", LEDGER, "vieww-paint"),
        ("pixels", "what you see", INK, "vieww-render"),
    ];
    /// The camera the stack is seen through.
    fn layers_view() -> super::space::View {
        super::space::View {
            cam: crate::three_d::Camera {
                eye: Vec3::new(0.0, 900.0, -1250.0),
                target: Vec3::new(0.0, 0.0, 0.0),
                fov: 0.62,
            },
            canvas: CANVAS,
            centre: Offset::new(640.0, 610.0),
        }
    }

    // The drop — one change falling through the four planes, on a loop,
    // so the relationship between them is a motion and not a diagram.
    let drop_u = ((sec - 2.0).max(0.0) / 3.2).fract();
    let dropping = sec > 2.0;

    super::frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            let _ = (w, h);
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(13, 12, 18)),
                    (0.56, BG_DEEP),
                    (1.0, Color::rgb(10, 9, 13)),
                ]),
            );
            pf::stars(book, w, h, 0x1A4E, 38, t, 0.05);
            pf::vignette(book, w, h, 0.5);
        }),
    )));
    // The stack, in true perspective: four floors seen from above and to
    // one side, turning slowly, so it reads as a thing with depth rather
    // than as four cards. Every shape on a floor is projected point by
    // point (`space`), so nothing tears.
    let yaw = -0.42 + 0.10 * (sec * 0.35).sin();
    let view = layers_view();
    let floors: Vec<super::space::Plane> = (0..4)
        .map(|i| {
            let p = ease_out_expo(clamp01((t - 0.08 - i as f32 * 0.075) / 0.22));
            let y = 225.0 - i as f32 * 150.0 + (1.0 - p) * 90.0;
            super::space::Plane::floor(Vec3::new(0.0, y, 0.0), yaw)
        })
        .collect();
    let floors_paint = floors.clone();
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            use super::space as sp;
            let lit_i = if dropping { Some(((drop_u * 4.0) as usize).min(3)) } else { None };
            // Bottom floor first: the view is from above, so each floor
            // covers the one beneath it.
            for i in (0..4).rev() {
                let p = ease_out_expo(clamp01((t - 0.08 - i as f32 * 0.075) / 0.22));
                if p <= 0.01 {
                    continue;
                }
                let color = LAYERS[i].2;
                let f = &floors_paint[i];
                let body = Rect::new(-320.0, -200.0, 320.0, 200.0);
                let lit = lit_i == Some(i);
                sp::rrect(book, &view, f, body, 22.0, pf::alpha(Color::rgb(0x17, 0x14, 0x1F), 0.94 * p));
                sp::rrect_stroke(book, &view, f, body, 22.0, pf::alpha(color, (if lit { 0.95 } else { 0.55 }) * p), if lit { 2.4 } else { 1.5 });
                match i {
                    0 => {
                        for k in 0..4 {
                            let (cx, cy) = ((k % 2) as f32, (k / 2) as f32);
                            let r = Rect::new(-280.0 + cx * 290.0, -160.0 + cy * 170.0, -30.0 + cx * 290.0, -30.0 + cy * 170.0);
                            sp::rrect(book, &view, f, r, 14.0, pf::alpha(color, 0.24 * p));
                            sp::rrect_stroke(book, &view, f, r, 14.0, pf::alpha(color, 0.7 * p), 1.3);
                        }
                    }
                    1 => {
                        let root = (-230.0, 0.0);
                        let kids = [(20.0, -130.0), (120.0, -60.0), (220.0, 10.0), (120.0, 90.0), (20.0, 150.0)];
                        for kid in kids {
                            sp::stroke(book, &view, f, &[root, kid], false, pf::alpha(color, 0.6 * p), 1.4);
                            sp::disc(book, &view, f, kid, 13.0, pf::alpha(color, 0.9 * p));
                        }
                        sp::disc(book, &view, f, root, 18.0, pf::alpha(color, 0.95 * p));
                    }
                    2 => {
                        for k in 0..5 {
                            let y = -150.0 + k as f32 * 72.0;
                            let w = 220.0 + ((k * 137) % 330) as f32;
                            sp::rrect(book, &view, f, Rect::new(-280.0, y, -280.0 + w, y + 26.0), 13.0, pf::alpha(color, 0.5 * p));
                        }
                    }
                    _ => {
                        for gy in 0..6 {
                            for gx in 0..10 {
                                if ((gx * 7 + gy * 13) % 5) >= 3 {
                                    continue;
                                }
                                let r = Rect::new(-285.0 + gx as f32 * 57.0, -175.0 + gy as f32 * 60.0, -285.0 + gx as f32 * 57.0 + 46.0, -175.0 + gy as f32 * 60.0 + 48.0);
                                sp::fill(book, &view, f, &[(r.left, r.top), (r.right, r.top), (r.right, r.bottom), (r.left, r.bottom)],
                                    pf::alpha(color, (0.16 + 0.08 * ((gx + gy) % 3) as f32) * p));
                            }
                        }
                    }
                }
            }
            // The change, falling through the floors' centres.
            if dropping {
                let seg = (drop_u * 4.0).min(3.999);
                let i = seg as usize;
                let fr = ease_in_out(seg - i as f32);
                let y0 = 225.0 - i as f32 * 150.0 + 30.0;
                let y1 = y0 - 150.0;
                let p = Vec3::new(0.0, y0 + (y1 - y0) * fr, 0.0);
                sp::glow_point(book, &view, p, 7.0, BRAND_NEAR, 1.0);
            }
        }),
    )));

    // The names, right of the stack — each level with its own floor's
    // right edge, joined to it by a leader.
    for (i, (name, holds, color, _)) in LAYERS.iter().enumerate() {
        let p = ease_out_cubic(clamp01((t - 0.12 - i as f32 * 0.075) / 0.20));
        if p <= 0.01 {
            continue;
        }
        let Some(edge) = view.pt(floors[i].at(320.0, 0.0)) else { continue };
        let x = 1260.0;
        let y = edge.dy;
        let lit = dropping && ((drop_u * 4.0) as usize) == i;
        let col = *color;
        stack = stack.push(Positioned::fill().child(Painting::sized(CANVAS, PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            book.line(Offset::new(edge.dx + 10.0, edge.dy), Offset::new(x - 24.0, y), pf::alpha(col, 0.45 * p), 1.2);
            book.circle(Offset::new(edge.dx + 6.0, edge.dy), 4.0, pf::alpha(col, 0.9 * p));
        }))));
        stack = stack.push(super::frame::label(x, y - 46.0, 600.0, 52.0, (*name).to_string(),
            pf::geist(42.0).bold().letter_spacing(-0.4).color(pf::alpha(if lit { INK } else { *color }, 0.97)),
            TextAlign::Left, p));
        stack = stack.push(super::frame::label(x, y + 6.0, 600.0, 32.0, (*holds).to_string(),
            pf::geist_mono(22.0).color(pf::alpha(MUTED, 0.95)),
            TextAlign::Left, p));
    }

    super::frame::caption("From your layout to the screen — four steps, one engine.", 1002.0, clamp01((t - 0.06) / 0.10));
    super::frame::caption("No browser and no translator anywhere in between.", 966.0, clamp01((t - 0.58) / 0.10));
    stack.into()
}

// ── Z10 · the_type ──────────────────────────────────────────────────────────

/// **Text is not a texture.** The engine shapes its own type: six
/// embedded faces, real shaping, real metrics, the same glyphs on every
/// platform because the platform is not the one drawing them.
///
/// Shown as a specimen sheet — a size ramp that grows live, the script
/// coverage as a row of real strings, and the one receipt that matters:
/// the film's own frames were typeset by this.
pub fn the_type(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let probe = ctx.probe;
    let mut stack = Stack::new();

    super::frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            let _ = (w, h);
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(14, 13, 17)),
                    (0.56, BG_DEEP),
                    (1.0, Color::rgb(10, 9, 13)),
                ]),
            );
            pf::vignette(book, w, h, 0.5);
        }),
    )));
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            let _ = (w, h, &s);

            // The baseline grid the specimen sits on — the one place in
            // the film where the grid is the subject.
            let grid_a = clamp01((t - 0.06) / 0.16) * 0.10;
            if grid_a > 0.005 {
                for k in 0..9 {
                    let y = 316.0 + k as f32 * 76.0;
                    book.rect(pf::xywh(super::MARGIN, y, w - super::MARGIN * 2.0, 1.0), pf::alpha(Color::WHITE, grid_a));
                }
            }

        }),
    )));

    // The size ramp — one word at six sizes, each arriving on its beat,
    // all sharing a left edge so the ramp reads as a ramp.
    const RAMP: [f32; 6] = [88.0, 62.0, 44.0, 32.0, 24.0, 18.0];
    let mut y = 316.0;
    for (i, size) in RAMP.iter().enumerate() {
        let p = ease_out_expo(clamp01((t - 0.08 - i as f32 * 0.055) / 0.18));
        if p > 0.01 {
            stack = stack.push(Positioned::new().left(super::MARGIN).top(y).width(1100.0).height(size * 1.35).child(
                Opacity::new(p).child(Text::new("shaping, not blitting".to_string())
                    .style(pf::geist(*size).bold().letter_spacing(-0.5).color(pf::alpha(INK, 0.96)))),
            ));
            stack = stack.push(Positioned::new().left(1180.0).top(y + size * 0.42).width(200.0).height(26.0).child(
                Opacity::new(p * 0.75).child(Text::new(format!("{size:.0} px"))
                    .style(pf::geist_mono(19.0).letter_spacing(1.2).color(pf::alpha(MUTED, 0.9)))),
            ));
        }
        y += size * 1.12 + 28.0;
    }

    // The script coverage — real strings, not a claim about them.
    const SCRIPTS: [(&str, &str); 4] = [
        ("latin", "Hamburgefonstiv"),
        ("greek", "Ελληνικά"),
        ("cyrillic", "Кириллица"),
        ("symbols", "→ ≠ ∑ ✓ ·"),
    ];
    for (i, (name, sample)) in SCRIPTS.iter().enumerate() {
        let p = ease_out_cubic(clamp01((t - 0.44 - i as f32 * 0.05) / 0.16));
        if p <= 0.01 {
            continue;
        }
        let sy = 356.0 + i as f32 * 96.0;
        stack = stack.push(Positioned::new().left(1420.0).top(sy).width(420.0).height(44.0).child(
            Opacity::new(p).child(Text::new((*sample).to_string())
                .style(pf::geist(34.0).color(pf::alpha(INK, 0.95)))),
        ));
        stack = stack.push(Positioned::new().left(1420.0).top(sy + 44.0).width(420.0).height(28.0).child(
            Opacity::new(p * 0.8).child(Text::new((*name).to_string())
                .style(pf::geist_mono(18.0).letter_spacing(1.6).color(pf::alpha(SYN_TYPE, 0.92)))),
        ));
    }

    // The receipt — the faces this very frame was set in, counted by the
    // renderer rather than claimed by the film.
    let rec_a = clamp01((t - 0.62) / 0.14);
    if rec_a > 0.01 {
        stack = stack.push(Positioned::new().left(super::MARGIN).top(820.0).width(1760.0).height(34.0).child(
            Opacity::new(rec_a).child(Text::new(format!(
                "{} — every glyph in this film shaped by vieww-text, {} glyph runs counted",
                probe.bench,
                pf::group_commas(probe.glyph_runs)
            ))
            .style(pf::geist_mono(20.0).letter_spacing(0.4).color(pf::alpha(LEDGER, 0.95)))),
        ));
    }

    super::frame::caption("Text looks the same everywhere.", 1002.0, clamp01((t - 0.06) / 0.10));
    super::frame::caption("vieww sets its own type, so every device shows identical letters.", 966.0, clamp01((t - 0.56) / 0.10));
    let _ = sec;
    stack.into()
}
