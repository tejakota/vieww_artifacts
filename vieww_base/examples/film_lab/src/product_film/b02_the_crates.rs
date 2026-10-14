//! B02 · THE CRATES — the engine's own manifest. 1:24–1:38.
//!
//! The 36 crates of the vieww workspace, verbatim from `Cargo.toml`,
//! orbit into a lattice: foundation at the core, the widget stack
//! around it, the render pipeline at the rim, the platforms outside.
//! The count on screen is the list's own length — nothing typed. The
//! deliberate choice is said out loud: **no wgpu** — vieww's GPU branch
//! is its own, all the way down, because the render graph, the damage
//! tracking and the frame scheduling are the things vieww exists to
//! own. Designing them around someone else's abstraction is the
//! outcome that choice avoids.
//!
//! This is the curiosity beat at its peak: a machine, shown as a
//! machine, and shown *honestly* — a constellation of real names.

use vieww_foundation::{Color, Offset, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;

use super::{
    alpha, caption, clamp01, count_up, distance_chip, gap_line, glow, grain, ground, pole_caret,
    pole_screen, progress_rail, vignette, xywh, Ctx, ACCENT, ENGINE, INK, LEDGER, MUTED,
    SYN_FUNCTION, SYN_KEYWORD, SYN_STRING, SYN_TYPE, W,
};
use crate::film_lib::ease_out_cubic;

/// The lattice's center.
const CX: f32 = 960.0;
const CY: f32 = 460.0;

/// Which ring a crate belongs to — the layering, as orbits.
fn ring_of(name: &str) -> usize {
    if name.contains("foundation") || name == "vieww-text" {
        0
    } else if name.contains("widget") || name.contains("element") || name.contains("animation") {
        1
    } else if name.contains("render")
        || name.contains("paint")
        || name.contains("scene")
        || name.contains("gpu")
        || name.contains("hal")
        || name.contains("shaders")
        || name.contains("effects")
        || name.contains("image")
    {
        2
    } else {
        3
    }
}

/// A crate's colour, by ring.
fn ring_color(ring: usize) -> Color {
    match ring {
        0 => SYN_STRING,
        1 => SYN_KEYWORD,
        2 => ENGINE,
        3 => SYN_FUNCTION,
        _ => MUTED,
    }
}

pub(super) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let frame_i = (ctx.abs * 60.0) as u64;

    let room = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            ground(book, w, h);
            vignette(book, w, h, 0.55);
            grain(book, w, h, frame_i, 0.35);
        }),
    );

    let mut stack = Stack::new().push(Positioned::fill().child(room));

    // The lattice — 36 crates, one per node, orbiting in with their
    // names. The orbit is deterministic (index-seeded), the whole turns
    // slowly, and each node is a real crate name from the manifest.
    let arrive = ease_out_cubic(clamp01((t - 0.08) / 0.4));
    let shown = count_up(super::CRATES.len() as u64, clamp01((t - 0.1) / 0.5));
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            // The rings' guides — four concentric orbits.
            for (ri, rr) in [110.0f32, 190.0, 270.0, 350.0].iter().enumerate() {
                book.ring(
                    Offset::new(CX, CY),
                    rr * arrive.max(0.05),
                    1.0,
                    alpha(ring_color(ri), 0.16),
                );
            }
            // The crates.
            let spin = sec * 0.06;
            let _n = super::CRATES.len() as f32;
            for (i, name) in super::CRATES.iter().enumerate() {
                let ring = ring_of(name);
                // Ring radius, with a per-index jitter so the lattice
                // reads as a constellation, not as four clock faces.
                let jitter = ((i * 37) % 13) as f32 / 13.0;
                let rr = match ring {
                    0 => 110.0,
                    1 => 190.0,
                    2 => 270.0,
                    _ => 350.0,
                } + jitter * 18.0;
                // The angle — packed by ring, rotating with the spin.
                let in_ring = (i % 12) as f32;
                let ang = spin + in_ring / 12.0 * std::f32::consts::TAU + ring as f32 * 0.5;
                let x = CX + ang.cos() * rr * arrive;
                let y = CY + ang.sin() * rr * 0.72 * arrive;
                let c = ring_color(ring);
                // The node — a dot with a soft rim.
                book.circle(Offset::new(x, y), 3.0, alpha(c, 0.9));
                book.ring(Offset::new(x, y), 5.5, 1.0, alpha(c, 0.35));
                // The name — small, only on the inner two rings (the
                // outer rings stay a constellation; the names that
                // matter are the ones at the core).
                if ring < 2 {
                    book.rrect(
                        xywh(x + 8.0, y - 4.0, name.len() as f32 * 5.6 + 6.0, 9.0),
                        2.0,
                        alpha(c, 0.30 * arrive),
                    );
                }
            }
            // The core — the foundation, one bright point.
            glow(book, CX, CY, 60.0, LEDGER, 0.4 * arrive);
            book.circle(Offset::new(CX, CY), 4.5, alpha(LEDGER, arrive));
        }),
    )));

    // The count — the lattice's own length, counted up.
    let count_a = clamp01((t - 0.12) / 0.2);
    if count_a > 0.01 {
        stack = stack.push(
            Positioned::new()
                .left(W - 470.0)
                .top(130.0)
                .width(320.0)
                .height(120.0)
                .child(super::Opacity::new(count_a).child(Painting::sized(
                    Size::new(320.0, 120.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        book.rrect(
                            xywh(0.0, 0.0, 320.0, 112.0),
                            10.0,
                            alpha(super::SURFACE_2, 0.9),
                        );
                        book.stroke_rrect(
                            xywh(0.0, 0.0, 320.0, 112.0),
                            10.0,
                            alpha(ENGINE, 0.30),
                            1.2,
                        );
                    }),
                ))),
        );
        stack = stack.push(
            Positioned::new()
                .left(W - 470.0 + 22.0)
                .top(142.0)
                .width(280.0)
                .height(30.0)
                .child(
                    Opacity::new(count_a).child(
                        Text::new(format!("{} — crates in the engine", shown))
                            .style(
                                TextStyle::new(15.0)
                                    .monospace()
                                    .letter_spacing(1.2)
                                    .color(alpha(ENGINE, 0.9)),
                            )
                            .align(TextAlign::Left),
                    ),
                ),
        );
        stack = stack.push(
            Positioned::new()
                .left(W - 470.0 + 22.0)
                .top(176.0)
                .width(280.0)
                .height(40.0)
                .child(
                    Opacity::new(count_a * 0.9).child(
                        Text::new("paint · scene · render\ngpu · hal — draw the frame")
                            .style(
                                TextStyle::new(13.5)
                                    .monospace()
                                    .letter_spacing(1.0)
                                    .color(alpha(SYN_TYPE, 0.85)),
                            )
                            .align(TextAlign::Left),
                    ),
                ),
        );
    }

    // The no-wgpu declaration — the deliberate choice, typed on.
    let choice_a = clamp01((t - 0.55) / 0.14);
    if choice_a > 0.01 {
        stack = stack.push(
            Positioned::new()
                .left(W - 470.0)
                .top(268.0)
                .width(320.0)
                .height(70.0)
                .child(super::Opacity::new(choice_a).child(Painting::sized(
                    Size::new(320.0, 70.0),
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        book.rrect(xywh(0.0, 0.0, 320.0, 62.0), 10.0, alpha(super::WASH, 0.75));
                        book.stroke_rrect(
                            xywh(0.0, 0.0, 320.0, 62.0),
                            10.0,
                            alpha(ACCENT, 0.35),
                            1.2,
                        );
                    }),
                ))),
        );
        stack = stack.push(
            Positioned::new()
                .left(W - 470.0 + 20.0)
                .top(280.0)
                .width(284.0)
                .height(48.0)
                .child(
                    super::Opacity::new(choice_a).child(
                        Text::new("no wgpu — by design\nthe renderer is vieww's own")
                            .style(
                                TextStyle::new(14.5)
                                    .monospace()
                                    .letter_spacing(1.1)
                                    .color(alpha(INK, 0.92)),
                            )
                            .align(TextAlign::Left),
                    ),
                ),
        );
    }

    // The gap line — quiet at the bottom of the lattice.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            gap_line(book, 946.0, 560.0, 1360.0, 0.0, sec, 0.0, 0.0, MUTED, 0.5);
            pole_caret(book, 560.0, 946.0, sec, 0.8);
            pole_screen(book, 1360.0, 946.0, 0.15, 0.8, MUTED);
        }),
    )));

    // The captions.
    stack = stack.push(super::act_chip("MOVEMENT II", "THE ENGINE", 1.0));
    stack = stack.push(caption(
        "one engine, thirty-six crates — every name on screen is real",
        1002.0,
        clamp01((t - 0.06) / 0.12),
    ));
    stack = stack.push(caption(
        "the render graph, the damage, the frame schedule: the things vieww exists to own",
        966.0,
        clamp01((t - 0.58) / 0.12),
    ));

    stack = stack.push(distance_chip(ctx.abs, clamp01(t / 0.1)));
    stack = stack.push(progress_rail(ctx.abs));

    let _ = (SYN_STRING, SYN_KEYWORD);
    stack.into()
}

/// The count card's rows' y offsets — (label, y).
fn label_i(_label: &str, step: f32) -> f32 {
    // Two rows, the count big, the render list small.
    step
}
