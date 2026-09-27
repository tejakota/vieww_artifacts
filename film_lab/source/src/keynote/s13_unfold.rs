//! S13 · THE UNFOLD — the reveal proper. 2:21–2:35.
//!
//! Eight storyboard frames compressed to six, E-20 at film scale, in the
//! inherited layer palette (description `#58a6ff` · identity `#3fb950` ·
//! geometry `#ffa657` · signal `#bc8cff` · damage `#f85149`):
//!
//! - **F1 the return** — the counter full-frame, flat, reading **7**; the
//!   session line arrives node by node.
//! - **F2 the turn** — ghosts part by pixels; the line extends into an
//!   axis. *"what you watched, from outside."*
//! - **F3 depth opens** — the pull-back; a floor plane arrives.
//! - **F4 the unfold** — three planes fan, labelled. *"description ·
//!   identity · geometry."*
//! - **F5 receipts in space** — build counts bloom; the damage rect
//!   pulses. *"every number was measured."*
//! - **F6 the signal** — one node apart from all three trees, one
//!   tether, reading **7**. *"state lives outside the tree."*
//!
//! Annotations bloom in sequence, never simultaneously; key info stays
//! center-frame; every beat captioned (§4.1's disciplines, inherited).

use vieww_foundation::{
    Color, FontWeight, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle, Transform,
    Transform3,
};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};
use vieww_widget::Transformed;

use crate::film_lib::{alpha, clamp01, ease_out_expo, mix, spring_out, tint, xywh, BG_DEEP, FAINT, INK, MUTED, VIOLET, VIOLET_SOFT};

use super::{
    backdrop, caption_center, chip, group_commas, C_DAMAGE, C_DESC, C_GEOM, C_IDENT, C_SIGNAL,
    Ctx,
};

/// The F-gates, in scene fractions (durations 3+1+2+3+2+3 of 14s).
const F2: f32 = 0.214;
const F3: f32 = 0.286;
const F4: f32 = 0.43;
const F5: f32 = 0.64;
const F6: f32 = 0.79;

const FOCAL: f32 = 1500.0;
const SPINE_Y: f32 = 660.0;

/// Scale about a point.
fn scale_about(cx: f32, cy: f32, s: f32) -> Transform {
    Transform::translate(Offset::new(cx * (1.0 - s), cy * (1.0 - s)))
        .then(Transform::scale(s, s))
}

/// One fanned plane: its flat rect, rest tilt, and stagger.
struct Plane {
    rest: Rect,
    tilt_rest: f32,
    stagger: f32,
    label: &'static str,
    color: Color,
}

fn planes() -> Vec<Plane> {
    vec![
        Plane { rest: Rect::new(468.0, 452.0, 828.0, SPINE_Y), tilt_rest: -0.46, stagger: 0.0, label: "description", color: C_DESC },
        Plane { rest: Rect::new(852.0, 452.0, 1212.0, SPINE_Y), tilt_rest: -0.58, stagger: 0.08, label: "identity", color: C_IDENT },
        Plane { rest: Rect::new(1236.0, 452.0, 1596.0, SPINE_Y), tilt_rest: -0.70, stagger: 0.16, label: "geometry", color: C_GEOM },
    ]
}

fn plane_transform(t: f32, p: &Plane) -> Transform3 {
    let tau = clamp01((t - F4 - p.stagger * 0.4) / 0.30);
    let s = spring_out(tau, 5.6, 0.78);
    let tilt = p.tilt_rest + (1.0 - s) * (-std::f32::consts::FRAC_PI_2 + 0.06 - p.tilt_rest);
    Transform3::translation(0.0, -SPINE_Y, 0.0)
        .then(Transform3::rotation_x(tilt))
        .then(Transform3::translation(0.0, SPINE_Y, 0.0))
        .then(Transform3::perspective(FOCAL))
}

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let abs = ctx.abs;

    // F3's pull-back — the world group recedes as depth opens.
    let k = 1.0 - 0.18 * clamp01((t - F3) / 0.14).clamp(0.0, 1.0);

    let mut world = Stack::new();

    // ── F1 · the return — the witness, full-frame ──────────────────────────
    {
        let seven_a = 1.0 - clamp01((t - F2) / 0.04) * 0.86; // stays, but yields the frame
        world = world.push(
            Positioned::new().left(0.0).top(190.0).width(1920.0).height(560.0).child(
                Opacity::new(seven_a).child(
                    Text::new(ctx.ladder.max(7).to_string())
                        .style(
                            TextStyle::new(430.0)
                                .monospace()
                                .weight(FontWeight::Medium)
                                .color(INK),
                        )
                        .align(TextAlign::Center),
                ),
            ),
        );
        // The witness label — small, tracked, under the number.
        world = world.push(
            Positioned::new().left(0.0).top(770.0).width(1920.0).height(30.0).child(
                Opacity::new(seven_a * clamp01(t / 0.05)).child(
                    Text::new("the witness · seven touches · never reset")
                        .style(TextStyle::new(19.0).monospace().letter_spacing(4.0).color(alpha(MUTED, 0.85)))
                        .align(TextAlign::Center),
                ),
            ),
        );
    }

    // ── F2 · the turn — ghosts part; the axis extends ──────────────────────
    if t >= F2 {
        let u = clamp01((t - F2) / (F3 - F2));
        let part = ease_out_expo(u);
        let axis_len = 780.0 * part;
        world = world.push(
            Positioned::fill().child(Painting::sized(
                super::CANVAS,
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    // The ghosts — motion persistence, parting by pixels:
                    // short streaks radiating from center, bucketed by
                    // direction, fading as they go.
                    let mut rng = crate::film_lib::Rng::new(0xF207);
                    for i in 0..44 {
                        let ang = i as f32 / 44.0 * std::f32::consts::TAU + rng.f01() * 0.12;
                        let r0 = 120.0 + rng.f01() * 60.0 + part * 420.0;
                        let r1 = r0 + 26.0 + rng.f01() * 60.0;
                        let a = (1.0 - part) * (0.22 + rng.f01() * 0.3);
                        if a > 0.01 {
                            book.line(
                                Offset::new(960.0 + ang.cos() * r0, 540.0 + ang.sin() * r0 * 0.55),
                                Offset::new(960.0 + ang.cos() * r1, 540.0 + ang.sin() * r1 * 0.55),
                                alpha(VIOLET_SOFT, a),
                                1.6,
                            );
                        }
                    }
                    // The axis — the session line's final node, extended
                    // into a spatial axis (the ratified decision).
                    if axis_len > 0.0 {
                        book.line(
                            Offset::new(960.0 - axis_len, 540.0),
                            Offset::new(960.0 + axis_len, 540.0),
                            alpha(INK, 0.30),
                            1.4,
                        );
                        // The axis ticks — measured spacing, fading out.
                        for i in -8..=8 {
                            let x = 960.0 + i as f32 * (axis_len / 8.0);
                            book.rect(xywh(x - 0.7, 533.0, 1.4, 14.0), alpha(INK, 0.16));
                        }
                    }
                }),
            )),
        );
    }

    // ── F4 · the unfold — three planes fan from the spine ──────────────────
    if t >= F4 {
        let ps = planes();
        world = world.push(
            Positioned::fill().child(Painting::sized(
                super::CANVAS,
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    // The spine — where the planes hinge.
                    let spine_a = clamp01((t - F4) / 0.08);
                    book.line(
                        Offset::new(420.0, SPINE_Y),
                        Offset::new(1650.0, SPINE_Y),
                        alpha(Color::WHITE, 0.10 * spine_a),
                        1.0,
                    );
                    for p in ps.iter() {
                        let xf = plane_transform(t, p);
                        let Some(quad) = xf.project_rect(p.rest) else { continue };
                        let tau = clamp01((t - F4 - p.stagger * 0.4) / 0.30);
                        // The frosted body — tinted by its layer color.
                        book.layer(tau, 0.0, None, |g| {
                            g.fill(
                                quad.clone(),
                                Gradient::vertical().with_dither().with_stops(&[
                                    (0.0, alpha(mix(p.color, BG_DEEP, 0.72), 0.34)),
                                    (1.0, alpha(mix(p.color, BG_DEEP, 0.82), 0.24)),
                                ]),
                            );
                        });
                        // The lit fold edge — the spine catches light.
                        book.stroke(quad, alpha(tint(p.color, 0.25), 0.85 * tau), 1.6);
                        // Tree glyphs — small node stacks, projected
                        // pointwise through the plane's own transform.
                        let mut rng = crate::film_lib::Rng::new(0x71A3 + p.rest.left as u64);
                        for g in 0..4 {
                            let gx = p.rest.left + 34.0 + g as f32 * 82.0;
                            let rows = 2 + (rng.f01() * 3.0) as usize;
                            for r in 0..rows {
                                let gy = p.rest.top + 30.0 + r as f32 * 34.0;
                                let w = 40.0 + rng.f01() * 26.0;
                                let corners = [
                                    Offset::new(gx, gy),
                                    Offset::new(gx + w, gy),
                                    Offset::new(gx + w, gy + 16.0),
                                    Offset::new(gx, gy + 16.0),
                                ];
                                let proj: Vec<Option<Offset>> =
                                    corners.iter().map(|c| xf.project(*c, 0.0)).collect();
                                if let [Some(a), Some(b), Some(c2), Some(d)] = proj[..] {
                                    let mut path = vieww_foundation::Path::new();
                                    path.move_to(a).line_to(b).line_to(c2).line_to(d).close();
                                    book.fill(path, alpha(tint(p.color, 0.15), 0.5 * tau));
                                }
                            }
                        }
                    }
                }),
            )),
        );
        // The plane labels — the three words, in the three colors.
        let lab_a = clamp01((t - F4 - 0.14) / 0.12);
        if lab_a > 0.0 {
            let ps2 = planes();
            for p in &ps2 {
                let cx = (p.rest.left + p.rest.right) * 0.5;
                let label = p.label;
                let color = p.color;
                world = world.push(
                    Positioned::new()
                        .left(cx - 150.0)
                        .top(SPINE_Y + 26.0)
                        .width(300.0)
                        .height(30.0)
                        .child(
                            Opacity::new(lab_a).child(
                                Text::new(label)
                                    .style(TextStyle::new(24.0).monospace().letter_spacing(4.0).color(color))
                                    .align(TextAlign::Center),
                            ),
                        ),
                );
            }
        }
    }

    // ── F5 · receipts in space — the counts bloom above the planes ────────
    if t >= F5 {
        let built = (abs * 60.0) as u64; // the harness's own frame count
        let bloom = clamp01((t - F5) / 0.10);
        let pulse = 0.5 + 0.5 * ((t - F5) * 8.0).sin();
        world = world.push(
            Positioned::new().left(0.0).top(340.0).width(1920.0).height(60.0).child(
                Opacity::new(bloom).child(
                    Text::new(format!("builds {} · every number was measured", group_commas(built)))
                        .style(TextStyle::new(26.0).monospace().letter_spacing(3.0).color(alpha(INK, 0.9)))
                        .align(TextAlign::Center),
                ),
            ),
        );
        // Tap 7's damage rect — pulsing where the counter sat.
        world = world.push(
            Positioned::new().left(880.0).top(402.0).width(220.0).height(90.0).child(
                Painting::sized(Size::new(220.0, 90.0), PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    book.stroke_rrect(xywh(0.0, 0.0, 220.0, 90.0), 10.0, alpha(C_DAMAGE, 0.4 + 0.5 * pulse), 1.8);
                    book.rrect(xywh(0.0, 0.0, 220.0, 90.0), 10.0, alpha(C_DAMAGE, 0.10));
                })),
            ),
        );
    }

    // ── F6 · the signal — one node apart, one tether, reading 7 ───────────
    if t >= F6 {
        let u = clamp01((t - F6) / 0.12);
        let node = Offset::new(1560.0, 300.0);
        world = world.push(
            Positioned::fill().child(Painting::sized(
                super::CANVAS,
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    // The tether — from the counter's home to the node.
                    let mut p = vieww_foundation::Path::new();
                    p.move_to(Offset::new(960.0, 420.0));
                    p.line_to(Offset::new(1280.0, 330.0));
                    p.line_to(node);
                    book.stroke(p, alpha(C_SIGNAL, 0.6 * u), 1.6);
                    book.stroke_styled(
                        {
                            let mut q = vieww_foundation::Path::new();
                            q.move_to(Offset::new(960.0, 420.0));
                            q.line_to(Offset::new(1280.0, 330.0));
                            q.line_to(node);
                            q
                        },
                        alpha(tint(C_SIGNAL, 0.3), 0.95 * u),
                        2.0,
                        vieww_foundation::StrokeStyle::default().dash(vieww_foundation::Dash::new(vec![u * 640.0, 640.0])),
                    );
                    // The node — outside all three trees.
                    super::glow(book, node.dx, node.dy, 90.0, C_SIGNAL, 0.4 * u);
                    book.circle(node, 7.0, tint(C_SIGNAL, 0.2));
                    book.ring(node, 13.0, 1.6, alpha(C_SIGNAL, 0.75 * u));
                }),
            )),
        );
        // The signal's value — 7, read live. The only number allowed to
        // appear twice (the witness, and its source).
        world = world.push(
            Positioned::new().left(1500.0).top(212.0).width(180.0).height(90.0).child(
                Opacity::new(u).child(
                    Text::new(ctx.ladder.max(7).to_string())
                        .style(TextStyle::new(72.0).monospace().weight(FontWeight::Medium).color(tint(C_SIGNAL, 0.25)))
                        .align(TextAlign::Center),
                ),
            ),
        );
    }

    // The floor grid — F3's depth, drawn under the world group.
    let grid = if t >= F3 {
        let fade = clamp01((t - F3) / 0.10) * (1.0 - clamp01((t - 0.965) / 0.035));
        if fade > 0.0 { Some(grid_painting(fade)) } else { None }
    } else {
        None
    };

    // The session line across the bottom — F1's arrival, node by node.
    let line_frac = clamp01(t / F2);
    let session_line = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            let y = 952.0;
            let x0 = 300.0;
            let x1 = 1620.0;
            let pts = [
                Offset::new(x0, y),
                Offset::new(x0 + 300.0, y),
                Offset::new(x0 + 640.0, y - 26.0),
                Offset::new(x1, y - 26.0),
            ];
            let mut p = vieww_foundation::Path::new();
            p.move_to(pts[0]);
            for pt in pts.iter().skip(1) {
                p.line_to(*pt);
            }
            book.stroke(p.clone(), alpha(Color::WHITE, 0.06), 1.0);
            let total = 300.0 + 352.0 + 928.0;
            book.stroke_styled(
                p,
                alpha(VIOLET_SOFT, 0.85),
                2.0,
                vieww_foundation::StrokeStyle::default()
                    .dash(vieww_foundation::Dash::new(vec![total * line_frac, total])),
            );
            // The seven nodes — one per touch, at the measured anchors.
            let anchors = [
                (x0, y),
                (x0 + 300.0, y),
                (x0 + 640.0, y - 26.0),
                (x1, y - 26.0),
            ];
            for (i, &(nx, ny)) in anchors.iter().enumerate() {
                let lit = (line_frac * 7.0) as usize > i;
                book.circle(
                    Offset::new(nx, ny),
                    if lit { 5.0 } else { 3.0 },
                    if lit { alpha(VIOLET_SOFT, 0.95) } else { alpha(MUTED, 0.3) },
                );
            }
        }),
    );

    // Assemble: backdrop → grid → the receding world → captions.
    let mut stack = Stack::new().push(Positioned::fill().child(backdrop(t, 0xE20, 120)));
    if let Some(g) = grid {
        stack = stack.push(Positioned::fill().child(g));
    }
    stack = stack
        .push(Positioned::fill().child(session_line))
        .push(Positioned::fill().child(Transformed::new(scale_about(960.0, 540.0, k)).child(world)));

    // The captions — one per beat, center-frame, in sequence.
    let c2 = clamp01((t - F2 - 0.02) / 0.06) * (1.0 - clamp01((t - F3) / 0.04));
    let c4 = clamp01((t - F4 - 0.16) / 0.08) * (1.0 - clamp01((t - F5) / 0.04));
    let c5 = clamp01((t - F5 - 0.12) / 0.08) * (1.0 - clamp01((t - F6) / 0.04));
    let c6 = clamp01((t - F6 - 0.14) / 0.08);
    if c2 > 0.0 {
        stack = stack.push(caption_center("what you watched · from outside", 950.0, c2));
    }
    if c4 > 0.0 {
        stack = stack.push(caption_center("description · identity · geometry", 950.0, c4));
    }
    if c5 > 0.0 {
        stack = stack.push(caption_center("every number was measured", 950.0, c5));
    }
    if c6 > 0.0 {
        stack = stack.push(caption_center("state lives outside the tree", 950.0, c6));
    }

    stack.into()
}

/// The perspective floor grid — unfold's grammar, full-frame.
fn grid_painting(fade: f32) -> WidgetNode {
    Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            const HORIZON: f32 = 700.0;
            const FLOOR_Y: f32 = 980.0;
            let grid_t = Transform3::translation(0.0, -HORIZON, 0.0)
                .then(Transform3::perspective(FOCAL))
                .then(Transform3::translation(0.0, HORIZON, 0.0));
            for zi in 0..9 {
                let z = 60.0 + zi as f32 * 110.0;
                for xi in -7..=7 {
                    let x = xi as f32 * 150.0;
                    let a = grid_t.project(Offset::new(x, FLOOR_Y), z);
                    let b = grid_t.project(Offset::new(x, FLOOR_Y), z + 110.0);
                    if let (Some(a), Some(b)) = (a, b) {
                        let deep = 1.0 - z / 1000.0;
                        book.line(a, b, alpha(VIOLET, 0.09 * fade * deep), 1.0);
                    }
                }
                if let (Some(a), Some(b)) = (
                    grid_t.project(Offset::new(-1050.0, FLOOR_Y), z),
                    grid_t.project(Offset::new(1050.0, FLOOR_Y), z),
                ) {
                    let deep = 1.0 - z / 1000.0;
                    book.line(a, b, alpha(FAINT, 0.06 * fade * deep), 1.0);
                }
            }
        }),
    )
    .into()
}
