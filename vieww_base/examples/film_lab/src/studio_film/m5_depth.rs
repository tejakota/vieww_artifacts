//! Movement V, deepened — the scene the release earned.
//!
//! **Z20B · the flight.** The studio becomes the mark: three mini screens
//! hand their shared elements to `vieww-element`'s flight machinery —
//! `SharedRegistry` → `SharedFlight::between` → the overlay — so the
//! wordmark chip, the accent dot and the device chip fly to their endcard
//! homes as rectangles interpolated by the framework's own `Flight`,
//! landing (the receipt measures it) at 0.0 px from where they were
//! recorded. Behind them, `vieww-animation`'s own `Duplicator` rings the
//! stage with rotating copies and a `ParticleField` throws the send-off
//! sparks — both sampled at the film's clock, both the framework's own
//! arithmetic.

use std::time::Duration;

use vieww_foundation::{Color, Offset, Rect, Size, Sketchbook, TextAlign};
use vieww_widget::prelude::*;

use super::{
    frame, ACCENT, BRAND_FAR, BRAND_NEAR, CANVAS, INK, LEDGER, MUTED, SURFACE, SURFACE_2,
    SYN_COMMENT, SYN_TYPE, TERM_GREEN,
};
use crate::film_lib::{clamp01, ease_in_out, ease_out_expo};
use crate::product_film as pf;

/// The three source screens, as rects on the frame.
const SCREENS: [Rect; 3] = [
    Rect {
        left: 190.0,
        top: 300.0,
        right: 640.0,
        bottom: 660.0,
    },
    Rect {
        left: 680.0,
        top: 300.0,
        right: 1130.0,
        bottom: 660.0,
    },
    Rect {
        left: 1170.0,
        top: 300.0,
        right: 1620.0,
        bottom: 660.0,
    },
];

/// The endcard's landing zones, as rects on the frame.
const LAND_MARK: Rect = Rect {
    left: 760.0,
    top: 480.0,
    right: 1160.0,
    bottom: 600.0,
};
const LAND_ACCENT: Rect = Rect {
    left: 1140.0,
    top: 470.0,
    right: 1180.0,
    bottom: 510.0,
};
const LAND_CHIP: Rect = Rect {
    left: 820.0,
    top: 640.0,
    right: 1100.0,
    bottom: 684.0,
};

/// Where each screen's tagged element sits inside that screen.
fn source_tag_rects() -> Vec<(&'static str, Rect)> {
    let mut out = Vec::new();
    // The palette screen: the wordmark chip.
    let s = SCREENS[0];
    out.push((
        "mark",
        Rect::new(s.left + 90.0, s.top + 80.0, s.left + 330.0, s.top + 140.0),
    ));
    // The code screen: the accent dot.
    let s = SCREENS[1];
    out.push((
        "accent",
        Rect::new(s.left + 60.0, s.top + 60.0, s.left + 92.0, s.top + 92.0),
    ));
    // The device screen: the device chip.
    let s = SCREENS[2];
    out.push((
        "chip",
        Rect::new(s.left + 130.0, s.top + 90.0, s.left + 330.0, s.top + 150.0),
    ));
    out
}

/// Z20B — the flight: elements fly, the studio becomes the mark.
pub(crate) fn the_flight(ctx: &pf::Ctx) -> WidgetNode {
    use vieww_animation::duplicator::{Behaviour, Distribution, Duplicator, Falloff};
    use vieww_animation::particles::ParticleField;
    use vieww_element::{SharedFlight, SharedRegistry, SharedTag};

    let sec = ctx.sec;
    let t_stars = ctx.t;
    let mut stack = Stack::new();

    frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            book.rect(Rect::new(0.0, 0.0, s.width, s.height), Color::rgb(7, 6, 10));
            pf::stars(book, s.width, s.height, 0xF11E, 70, t_stars, 0.06);
            pf::vignette(book, s.width, s.height, 0.55);
        }),
    )));

    // The flight's clock: hold on the sources, then fly, then land.
    let fly_t = ease_in_out(clamp01((sec - 3.2) / 4.2));
    let appear = ease_out_expo(clamp01(sec / 1.4));

    // ── The three source screens, fading as their elements leave. ──────
    let screen_fade = 1.0 - 0.85 * fly_t;
    let screen_titles = ["the palette", "the code", "the device"];
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            if appear <= 0.01 {
                return;
            }
            for (k, s) in SCREENS.iter().enumerate() {
                let a = clamp01((appear - 0.1 * k as f32) / 0.6) * screen_fade;
                if a <= 0.01 {
                    continue;
                }
                book.rrect(*s, 14.0, pf::alpha(SURFACE, 0.85 * a));
                book.stroke_rrect(*s, 14.0, pf::alpha(SYN_COMMENT, 0.35 * a), 1.3);
                // Each screen's content, schematic: bars and marks.
                let inner = Rect::new(s.left + 24.0, s.top + 54.0, s.right - 24.0, s.bottom - 24.0);
                for r in 0..4 {
                    book.rrect(
                        Rect::new(
                            inner.left,
                            inner.top + 200.0 + r as f32 * 34.0,
                            inner.left + (inner.width()) * (0.9 - 0.18 * r as f32),
                            inner.top + 216.0 + r as f32 * 34.0,
                        ),
                        5.0,
                        pf::alpha(SURFACE_2, 0.9 * a),
                    );
                }
                book.rrect(
                    Rect::new(
                        inner.left,
                        inner.top + 60.0,
                        inner.left + 96.0,
                        inner.top + 88.0,
                    ),
                    5.0,
                    pf::alpha([BRAND_NEAR, SYN_TYPE, TERM_GREEN][k], 0.5 * a),
                );
            }
        }),
    )));
    for (k, title) in screen_titles.iter().enumerate() {
        let a = clamp01((appear - 0.1 * k as f32) / 0.6) * screen_fade;
        if a <= 0.01 {
            continue;
        }
        let s = SCREENS[k];
        stack = stack.push(frame::label(
            s.left + 24.0,
            s.top + 18.0,
            s.width() - 48.0,
            26.0,
            title.to_string(),
            pf::geist(17.0).color(pf::alpha(MUTED, 0.95)),
            TextAlign::Left,
            a,
        ));
    }

    // ── The registry: record where the tags are, on both layouts. ──────
    let registry = SharedRegistry::new();
    for (tag, r) in source_tag_rects() {
        registry.record(SharedTag::new(tag), r);
    }
    let from = registry.snapshot();
    registry.record(SharedTag::new("mark"), LAND_MARK);
    registry.record(SharedTag::new("accent"), LAND_ACCENT);
    registry.record(SharedTag::new("chip"), LAND_CHIP);
    let to = registry.snapshot();
    let flight = SharedFlight::between(&from, &to);

    // The landing receipt, measured from the flights themselves.
    let mut land_err = 0.0_f32;
    for tag in flight.tags() {
        if let (Some(f), Some(dst)) = (flight.get(&tag), to.get(&tag)) {
            let end = f.rect_at(1.0);
            land_err = land_err
                .max((end.left - dst.left).abs())
                .max((end.top - dst.top).abs())
                .max((end.right - dst.right).abs())
                .max((end.bottom - dst.bottom).abs());
        }
    }

    // ── The endcard, materialising as the flights land. ────────────────
    let end_a = clamp01((sec - 5.6) / 1.4);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            if end_a <= 0.01 {
                return;
            }
            // The landing zones' ghost frames — where home is.
            for (r, col) in [
                (LAND_MARK, BRAND_FAR),
                (LAND_ACCENT, ACCENT),
                (LAND_CHIP, SYN_TYPE),
            ] {
                book.stroke_rrect(r, 10.0, pf::alpha(col, 0.35 * end_a), 1.2);
            }
        }),
    )));

    // ── The flights themselves — the framework's overlay. ──────────────
    let overlay = flight.overlay(fly_t, |tag| {
        // Each element, built at its destination size (the overlay scales).
        match tag.as_str() {
            "mark" => {
                let mut card = Stack::new();
                card = card.push(Positioned::fill().child(Painting::sized(
                    Size::new(LAND_MARK.width(), LAND_MARK.height()),
                    PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                        book.rrect(
                            Rect::new(0.0, 0.0, s.width, s.height),
                            14.0,
                            pf::alpha(SURFACE, 0.95),
                        );
                        book.rect(
                            Rect::new(18.0, 22.0, s.width - 18.0, 34.0),
                            pf::alpha(INK, 0.92),
                        );
                        book.rect(
                            Rect::new(18.0, 44.0, s.width * 0.62, 56.0),
                            pf::alpha(BRAND_NEAR, 0.92),
                        );
                    }),
                )));
                card.into()
            }
            "accent" => {
                let mut card = Stack::new();
                card = card.push(Positioned::fill().child(Painting::sized(
                    Size::new(LAND_ACCENT.width(), LAND_ACCENT.height()),
                    PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                        book.circle(
                            Offset::new(s.width * 0.5, s.height * 0.5),
                            s.width * 0.42,
                            pf::alpha(ACCENT, 0.95),
                        );
                        book.ring(
                            Offset::new(s.width * 0.5, s.height * 0.5),
                            s.width * 0.48,
                            2.0,
                            pf::alpha(ACCENT, 0.5),
                        );
                    }),
                )));
                card.into()
            }
            _ => {
                let mut card = Stack::new();
                card = card.push(Positioned::fill().child(Painting::sized(
                    Size::new(LAND_CHIP.width(), LAND_CHIP.height()),
                    PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                        book.rrect(
                            Rect::new(0.0, 0.0, s.width, s.height),
                            9.0,
                            pf::alpha(SYN_TYPE, 0.3),
                        );
                        book.stroke_rrect(
                            Rect::new(0.0, 0.0, s.width, s.height),
                            9.0,
                            pf::alpha(SYN_TYPE, 0.8),
                            1.6,
                        );
                        book.rect(
                            Rect::new(10.0, s.height * 0.4, s.width - 10.0, s.height * 0.6),
                            pf::alpha(SYN_TYPE, 0.5),
                        );
                    }),
                )));
                card.into()
            }
        }
    });
    stack = stack.push(overlay);

    // ── The send-off: the framework's own duplicator and particles. ────
    let ring_d = Duplicator::new(Distribution::Radial {
        count: 28,
        radius: 300.0,
        orient: true,
    })
    .behaviour(Behaviour::Rotate(0.5), Falloff::None)
    .behaviour(Behaviour::Scale(0.4), Falloff::Index { from: 1.0, to: 0.5 })
    .behaviour(Behaviour::Fade(0.6), Falloff::Index { from: 0.1, to: 0.7 });
    let instances = ring_d.evaluate(sec);
    let sparks = ParticleField::new(110.0, 2.4)
        .origin(Offset::new(960.0, 560.0))
        .direction(-std::f32::consts::FRAC_PI_2)
        .spread(0.7)
        .speed(150.0, 340.0)
        .gravity(Offset::new(0.0, 70.0))
        .size(3.2, 0.4)
        .color(pf::alpha(BRAND_FAR, 0.9), pf::alpha(BRAND_FAR, 0.0))
        .seed(0x5E2D);
    let particles = sparks.sample(Duration::from_secs_f32(sec));
    let (n_inst, n_part) = (instances.len(), particles.len());
    let ring_a = clamp01((sec - 1.8) / 1.2);
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            if ring_a <= 0.01 {
                return;
            }
            // The duplicator's ring — twenty-eight copies, rotating home.
            for inst in &instances {
                let p = Offset::new(960.0 + inst.position.0, 560.0 + inst.position.1);
                let a = ring_a * inst.opacity;
                if a <= 0.02 {
                    continue;
                }
                let col = pf::mix(BRAND_NEAR, BRAND_FAR, inst.value);
                let r = 7.0 * inst.scale;
                book.rrect(
                    Rect::new(p.dx - r, p.dy - r * 0.5, p.dx + r, p.dy + r * 0.5),
                    2.0,
                    pf::alpha(col, 0.7 * a),
                );
            }
            // The particle send-off.
            for pt in &particles {
                let a = f32::from(pt.color.a) / 255.0 * 0.8;
                if a <= 0.02 {
                    continue;
                }
                book.circle(pt.position, pt.size, pf::alpha(BRAND_FAR, a));
            }
        }),
    )));

    stack = stack.push(frame::caption(
        "The studio becomes the mark.",
        1002.0,
        clamp01((sec - 0.2) / 0.5),
    ));
    stack = stack.push(frame::caption(
        "Elements fly — nothing fades; every element knows where it lives.",
        966.0,
        clamp01((sec - 5.0) / 0.6),
    ));
    let flight_line = format!("{} flights · land at {:.1} px", flight.len(), land_err);
    let ring_line = format!("{} copies · {} sparks", n_inst, n_part);
    stack = stack.push(frame::receipts(
        &[
            ("vieww-element · shared flights", ACCENT),
            (flight_line.as_str(), SYN_TYPE),
            (ring_line.as_str(), LEDGER),
        ],
        0.0,
        0.0,
        clamp01((sec - 7.2) / 0.6),
    ));
    frame::boxed(Rect::new(
        SCREENS[0].left,
        SCREENS[0].top,
        SCREENS[2].right,
        SCREENS[2].bottom + 120.0,
    ));
    stack.into()
}
