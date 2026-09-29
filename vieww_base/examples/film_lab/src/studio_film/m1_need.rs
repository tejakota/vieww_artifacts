//! Movement I · THE NEED — why another UI/UX framework at all.
//!
//! Three scenes, 28 seconds. The register is curiosity shading into
//! need: one picture every product wants to ship, four ports that
//! each ship something slightly else, the tolls every port pays, and
//! then the question the whole film exists to ask.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Shadow, Sketchbook, TextAlign};
use vieww_widget::prelude::*;

use crate::film_lib::{clamp01, ease_in_out, ease_out_cubic, ease_out_expo, Rng};
use crate::product_film as pf;
use super::filmkit as fk;
use super::{
    BRAND_FAR, BRAND_NEAR, PORTS, ACCENT, BG_DEEP, BREAK_RED, CANVAS, INK, MUTED, SYN_TYPE, W,
};

// ── Z01 · the_same_picture ──────────────────────────────────────────────────

/// The design's own geometry — the one card every port is supposed to
/// ship. Centered on the canvas, below the caption band.
// Re-pitched into the body band (300–900): the header and footer are
// the film's own type, and nothing in a scene may draw into them.
const CARD: Rect = Rect::new(740.0, 344.0, 1180.0, 844.0);

/// The four ports' stations, clockwise from top-left.
const STATIONS: [Rect; 4] = [
    Rect::new(112.0, 300.0, 424.0, 472.0),
    Rect::new(1496.0, 300.0, 1808.0, 472.0),
    Rect::new(112.0, 764.0, 424.0, 936.0),
    Rect::new(1496.0, 764.0, 1808.0, 936.0),
];

/// Where each rail leaves the truth card's edge.
const CARD_ANCHORS: [Offset; 4] = [
    Offset::new(740.0, 420.0),
    Offset::new(1180.0, 420.0),
    Offset::new(740.0, 768.0),
    Offset::new(1180.0, 768.0),
];

/// What each port gets wrong — the drift is specific, not generic.
const FLAWS: [&str; 4] = [
    "hue drifted, chrome added",
    "spacing cramped, radius gone",
    "radius a pill, button overflows",
    "brand rendered as grey",
];

pub fn the_same_picture(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = Stack::new();

    // The room — the film's warmest dark; the picture deserves a stage.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(16, 14, 18)),
                    (0.55, BG_DEEP),
                    (1.0, Color::rgb(13, 11, 16)),
                ]),
            );
            // A soft stage light behind the truth card — one blurred
            // group, breathing with the card it lifts.
            let stage_a = ease_out_cubic(clamp01(t / 0.10));
            if stage_a > 0.01 {
                let pulse = 1.0 + 0.10 * (sec * 1.5).sin();
                book.layer(stage_a, 46.0, None, |glow_book| {
                    glow_book.rrect(
                        pf::xywh(
                            CARD.left - 60.0 * pulse,
                            CARD.top - 50.0 * pulse,
                            CARD.width() + 120.0 * pulse,
                            CARD.height() + 100.0 * pulse,
                        ),
                        46.0,
                        pf::alpha(BRAND_NEAR, 0.16),
                    );
                });
            }
            pf::stars(book, w, h, 0x5E51, 60, t, 0.05);
            pf::vignette(book, w, h, 0.5);
        }),
    )));

    // The truth card — one design, drawn once. It arrives on a
    // decelerate and then holds; the breathing lives in the light
    // behind it, where breathing is free.
    let card_in = ease_out_expo(clamp01(t / 0.14));
    if card_in > 0.01 {
        stack = stack.push(
            Transformed::translate(Offset::new(0.0, (1.0 - card_in) * 26.0))
                .child(Opacity::new(card_in).child(Painting::sized(
                    CANVAS,
                    PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                        draw_truth_card(book, CARD, card_in);
                    }),
                ))),
        );
    }

    // The rails and the ports — one picture, four translations, each
    // one slightly else. Rails draw first; the port card fades in
    // behind the arriving rail; the flaw names itself last.
    for (i, station) in STATIONS.iter().enumerate() {
        let rail_t0 = 0.14 + i as f32 * 0.06;
        let rail_p = ease_out_cubic(clamp01((t - rail_t0) / 0.16));
        let card_p = ease_out_cubic(clamp01((t - rail_t0 - 0.05) / 0.18));
        let flaw_p = clamp01((t - 0.52 - i as f32 * 0.05) / 0.12);
        if rail_p <= 0.01 && card_p <= 0.01 {
            continue;
        }
        let (anchor, port, flaw) = (CARD_ANCHORS[i], PORTS[i], FLAWS[i]);
        let station = *station;
        stack = stack.push(Painting::sized(CANVAS, PaintWith::new(
            move |book: &mut Sketchbook, _s: Size| {
                // The rail — a cubic that leaves the card's edge and
                // lands on the station, drawn to its own progress; once
                // landed, it *flows*: the picture is in transit, and
                // the dashes say so.
                if rail_p > 0.01 {
                    let pts = fk::thread_pts(anchor, station_center(station), -150.0);
                    fk::grow_stroke(book, &pts, rail_p, ACCENT, 1.5, 0.30 + 0.45 * rail_p);
                    let flow = clamp01((rail_p - 0.85) * 7.0);
                    if flow > 0.01 {
                        fk::flow_along(book, &pts, rail_t0 + t * 1.0 + i as f32 * 1.3, ACCENT, flow * 0.8, 1.3, 0.16);
                    }
                }
                // The port card — the same picture, translated.
                if card_p > 0.01 {
                    draw_port_card(book, station, i, card_p);
                }
            },
        )));
        // The port's name and its flaw — text, so the drift is legible.
        if card_p > 0.01 {
            let rise = (1.0 - card_p) * 10.0;
            stack = stack.push(
                Positioned::new()
                    .left(station.left + 16.0)
                    .top(station.top + 8.0 + rise)
                    .width(station.width() - 24.0)
                    .height(44.0)
                    .child(Opacity::new(card_p).child(
                        Flex::column()
                            .spacing(3.0)
                            .push(
                                Text::new(port.to_string())
                                    .style(pf::geist_mono(14.0).letter_spacing(2.0).color(pf::alpha(INK, 0.92))),
                            )
                            .push(
                                Text::new(flaw.to_string())
                                    .style(pf::geist_mono(12.0).letter_spacing(0.6).color(pf::alpha(BREAK_RED, 0.16 + 0.74 * flaw_p))),
                            ),
                    )),
            );
        }
    }

    // The tally — the ports' rewrite counter, top-right, ticking with
    // each rail that lands.
    let ports_in = STATIONS
        .iter()
        .enumerate()
        .filter(|(i, _)| t > 0.14 + *i as f32 * 0.06 + 0.05)
        .count();
    if ports_in > 0 {
        stack = stack.push(
            Positioned::new()
                .left(620.0)
                .top(866.0)
                .width(680.0)
                .height(32.0)
                .child(
                    Text::new(format!("rewrite № {ports_in} of 4 — same picture, four codes"))
                        .style(pf::geist_mono(18.0).letter_spacing(1.4).color(pf::alpha(MUTED, 0.9)))
                        .align(TextAlign::Center),
                ),
        );
    }

    // The film's voice.
    stack = stack.push(pf::caption("one picture. five codebases.", 1002.0, clamp01((t - 0.10) / 0.10)));
    stack = stack.push(pf::caption("every release, shipped five times — and none of them the same.", 966.0, clamp01((t - 0.55) / 0.12)));
    stack = stack.push(pf::chrome(super::progress_rail(ctx.abs)));
    stack.into()
}

fn station_center(r: Rect) -> Offset {
    Offset::new((r.left + r.right) * 0.5, (r.top + r.bottom) * 0.5)
}

/// The truth card — the design, drawn once: gradient header, avatar,
/// three list rows, one accent button. `a` is its arrival.
fn draw_truth_card(book: &mut Sketchbook, r: Rect, a: f32) {
    // The card's shadow — one soft lift.
    book.shadow(
        pf::xywh(r.left, r.top + 6.0, r.width(), r.height()),
        22.0,
        Shadow::new(pf::alpha(Color::BLACK, 0.59 * a), Offset::new(0.0, 10.0), 34.0),
    );
    let body = pf::xywh(r.left, r.top, r.width(), r.height());
    book.rrect(body, 22.0, pf::alpha(pf::SURFACE, a));
    book.stroke_rrect(body, 22.0, pf::alpha(Color::WHITE, 0.04 * a), 1.0);
    // The header — the brand's own gradient, clipped to the card's
    // top corners by a zero-blur layer with a rounded clip.
    let header_h = 150.0;
    book.layer(
        a,
        0.0,
        Some(Path::rounded_rect(body, 22.0)),
        |clipped| {
            clipped.rrect(
                pf::xywh(r.left, r.top, r.width(), header_h),
                0.0,
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, BRAND_FAR),
                    (1.0, BRAND_NEAR),
                ]),
            );
        },
    );
    // Title lines on the header.
    book.rrect(pf::xywh(r.left + 28.0, r.top + 42.0, 190.0, 14.0), 7.0, pf::alpha(Color::WHITE, 0.85 * a));
    book.rrect(pf::xywh(r.left + 28.0, r.top + 68.0, 120.0, 10.0), 5.0, pf::alpha(Color::WHITE, 0.47 * a));
    // The avatar.
    book.circle(
        Offset::new(r.left + r.width() - 58.0, r.top + header_h - 30.0),
        26.0,
        pf::alpha(Color::WHITE, 0.24 * a),
    );
    // Three list rows on the body.
    for row in 0..3 {
        let y = r.top + header_h + 36.0 + row as f32 * 78.0;
        let row_rect = pf::xywh(r.left + 28.0, y, r.width() - 56.0, 58.0);
        book.rrect(row_rect, 12.0, pf::alpha(pf::SURFACE_2, a));
        book.circle(Offset::new(row_rect.left + 28.0, y + 29.0), 14.0, pf::alpha(BRAND_NEAR, 0.55 * a));
        book.rrect(pf::xywh(row_rect.left + 56.0, y + 15.0, 150.0, 9.0), 4.5, pf::alpha(pf::MUTED, 0.7 * a));
        book.rrect(pf::xywh(row_rect.left + 56.0, y + 32.0, 96.0, 8.0), 4.0, pf::alpha(pf::FAINT, 0.6 * a));
    }
    // The accent button.
    let btn = pf::xywh(r.left + 28.0, r.top + r.height() - 84.0, r.width() - 56.0, 52.0);
    book.rrect(btn, 14.0, Gradient::horizontal().with_dither().with_stops(&[
        (0.0, BRAND_NEAR),
        (1.0, BRAND_FAR),
    ]));
    book.rrect(pf::xywh(btn.left + btn.width() * 0.5 - 44.0, btn.top + 19.0, 88.0, 12.0), 6.0, pf::alpha(Color::WHITE, 0.9 * a));
}

/// A port card — the same picture after its translation, each wrong in
/// its own way. `i` indexes [`PORTS`].
fn draw_port_card(book: &mut Sketchbook, r: Rect, i: usize, a: f32) {
    let body = pf::xywh(r.left, r.top, r.width(), r.height());
    book.shadow(
        pf::xywh(r.left, r.top + 4.0, r.width(), r.height()),
        14.0,
        Shadow::new(pf::alpha(Color::BLACK, 0.43 * a), Offset::new(0.0, 6.0), 18.0),
    );
    match i {
        // web · react — the hue drifts toward blue and a browser chrome
        // appears that the design never asked for.
        0 => {
            book.rrect(body, 8.0, pf::alpha(pf::SURFACE, a));
            book.rrect(pf::xywh(r.left, r.top, r.width(), 30.0), 8.0, pf::alpha(Color::rgb(38, 40, 48), a));
            for dot in 0..3 {
                book.circle(
                    Offset::new(r.left + 16.0 + dot as f32 * 16.0, r.top + 15.0),
                    4.5,
                    pf::alpha(pf::FAINT, a),
                );
            }
            book.rrect(pf::xywh(r.left + 16.0, r.top + 44.0, r.width() - 32.0, 34.0), 6.0, pf::alpha(SYN_TYPE, 0.5 * a));
            for row in 0..2 {
                book.rrect(
                    pf::xywh(r.left + 16.0, r.top + 92.0 + row as f32 * 34.0, r.width() - 32.0, 22.0),
                    4.0,
                    pf::alpha(pf::SURFACE_2, a),
                );
            }
            book.rrect(pf::xywh(r.left + 16.0, r.top + r.height() - 34.0, r.width() - 32.0, 22.0), 4.0, pf::alpha(SYN_TYPE, 0.6 * a));
        }
        // android — the spacing collapses and the radius disappears.
        1 => {
            book.rrect(body, 0.0, pf::alpha(pf::SURFACE, a));
            book.rrect(pf::xywh(r.left + 14.0, r.top + 14.0, r.width() - 28.0, 30.0), 0.0, pf::alpha(BRAND_NEAR, 0.5 * a));
            for row in 0..3 {
                book.rrect(
                    pf::xywh(r.left + 14.0, r.top + 50.0 + row as f32 * 30.0, r.width() - 28.0, 26.0),
                    0.0,
                    pf::alpha(pf::SURFACE_2, a),
                );
            }
            book.rrect(pf::xywh(r.left + 14.0, r.top + r.height() - 26.0, r.width() - 28.0, 18.0), 0.0, pf::alpha(BRAND_NEAR, 0.65 * a));
        }
        // ios — everything is a pill, and the button overflows the card.
        2 => {
            book.rrect(body, 44.0, pf::alpha(pf::SURFACE, a));
            book.rrect(pf::xywh(r.left + 20.0, r.top + 18.0, r.width() - 40.0, 44.0), 22.0, pf::alpha(BRAND_FAR, 0.5 * a));
            for row in 0..2 {
                book.rrect(
                    pf::xywh(r.left + 20.0, r.top + 76.0 + row as f32 * 40.0, r.width() - 40.0, 30.0),
                    15.0,
                    pf::alpha(pf::SURFACE_2, a),
                );
            }
            // The overflow — the button escapes the card's right edge.
            book.rrect(
                pf::xywh(r.left + 20.0, r.top + r.height() - 44.0, r.width() - 4.0, 30.0),
                15.0,
                pf::alpha(BRAND_NEAR, 0.65 * a),
            );
        }
        // desktop · qt — the brand renders as grey, all corners square.
        _ => {
            book.rrect(body, 4.0, pf::alpha(Color::rgb(44, 44, 46), a));
            book.rrect(pf::xywh(r.left + 14.0, r.top + 12.0, r.width() - 28.0, 26.0), 2.0, pf::alpha(Color::rgb(88, 88, 92), a));
            for row in 0..2 {
                book.rrect(
                    pf::xywh(r.left + 14.0, r.top + 46.0 + row as f32 * 32.0, r.width() - 28.0, 24.0),
                    2.0,
                    pf::alpha(Color::rgb(58, 58, 62), a),
                );
            }
            book.rrect(pf::xywh(r.left + 14.0, r.top + r.height() - 34.0, r.width() - 28.0, 22.0), 2.0, pf::alpha(Color::rgb(120, 120, 126), a));
        }
    }
    // The arrival — a hairline that cools as the card settles.
    if a < 0.98 {
        let halo = pf::xywh(r.left, r.top, r.width(), r.height());
        book.stroke_rrect(halo, 20.0, pf::alpha(ACCENT, 0.5 * (1.0 - a)), 1.2);
    }
}

// ── Z02 · the_tolls ─────────────────────────────────────────────────────────

/// The tolls — what every port pays. One gate per line, each paid in
/// the currency the first scene established: rewrites, drift, jank.
const GATES: [(&str, &str); 4] = [
    ("60 fps on a mid phone", "dropped frames"),
    ("motion design", "bolted on"),
    ("accessibility", "later"),
    ("one brand, every device", "re-translated"),
];

pub fn the_tolls(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;

    let mut stack = Stack::new();

    // The room — colder than Z01: vertical light shafts, one blurred
    // group (the blur economy: group early, blur once).
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            book.rect(Rect::new(0.0, 0.0, w, h), BG_DEEP);
            book.layer(0.5, 40.0, None, |shafts| {
                for (x, wd, al) in [(280.0, 130.0, 0.05), (960.0, 220.0, 0.035), (1660.0, 130.0, 0.05)] {
                    shafts.rect(pf::xywh(x - wd * 0.5, 0.0, wd, h), pf::alpha(ACCENT, al));
                }
            });
            pf::vignette(book, w, h, 0.55);
        }),
    )));

    // The gauge — the film's cost meter, an arc filling toward red as
    // the gates pay.
    let paid_n = GATES.iter().enumerate().filter(|(i, _)| t > 0.14 + *i as f32 * 0.14 + 0.22).count();
    let paid = paid_n as f32;
    stack = stack.push(Positioned::new().left(1330.0).top(370.0).width(400.0).height(400.0).child(
        Painting::sized(Size::new(400.0, 400.0), PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            let cx = 200.0;
            let cy = 200.0;
            // The track — three quarters of a turn, from twelve o'clock.
            let start = -std::f32::consts::FRAC_PI_2;
            let full = std::f32::consts::TAU * 0.75;
            book.arc(Offset::new(cx, cy), 150.0, 10.0, start, full, pf::alpha(Color::WHITE, 0.06));
            // The fill — a slice per paid toll, amber shading to red.
            if paid > 0.01 {
                let sweep = full * (paid / 4.0);
                book.arc(
                    Offset::new(cx, cy),
                    150.0,
                    10.0,
                    start,
                    sweep,
                    Gradient::sweep(Offset::new(0.5, 0.5), start, start + sweep)
                        .with_stops(&[(0.0, pf::SYN_MACRO), (1.0, BREAK_RED)]),
                );
            }
            // The gauge's orbit — cost particles circling the dial, the
            // meter alive between payments.
            fk::orbit_dots(
                book,
                Offset::new(cx, cy),
                168.0,
                168.0,
                5,
                t * 10.0,
                0.55,
                BREAK_RED,
                0.30 + 0.45 * (paid / 4.0),
                2.2,
            );
        })),
    ));
    // The gauge's numerals — a widget over the painting.
    stack = stack.push(
        Positioned::new()
            .left(1330.0)
            .top(522.0)
            .width(400.0)
            .height(80.0)
            .child(
                Flex::column()
                    .push(
                        Text::new(format!("{paid_n} of 4 tolls"))
                            .style(pf::geist(30.0).bold().color(pf::alpha(INK, 0.95)))
                            .align(TextAlign::Center),
                    )
                    .push(
                        Text::new("paid on every port, every release".to_string())
                            .style(pf::geist_mono(13.0).letter_spacing(1.2).color(pf::alpha(MUTED, 0.85)))
                            .align(TextAlign::Center),
                    ),
            ),
    );

    // The gates — each bar slides in, holds, then pays.
    for (i, (gate, toll)) in GATES.iter().enumerate() {
        let t0 = 0.14 + i as f32 * 0.14;
        let in_p = ease_out_cubic(clamp01((t - t0) / 0.18));
        let pay_p = clamp01((t - t0 - 0.22) / 0.10);
        if in_p <= 0.01 {
            continue;
        }
        let y = 320.0 + i as f32 * 134.0;
        let gate_w = 980.0 * ease_out_expo(in_p);
        let (gate, toll) = (*gate, *toll);
        let flash = if pay_p > 0.0 && pay_p < 1.0 { 1.0 - pay_p } else { 0.0 };
        // The toll, paid, flows to the gauge — every cost travels.
        let flow_a = clamp01((pay_p - 0.6) * 2.5);
        if flow_a > 0.01 {
            let from = Offset::new(160.0 + gate_w, y + 48.0);
            let to = Offset::new(1368.0, 452.0 + i as f32 * 58.0);
            let phase = t * 1.0 + i as f32 * 0.9;
            stack = stack.push(Positioned::fill().child(
                Painting::sized(CANVAS, PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    fk::flow_thread(book, from, to, 40.0, phase, BREAK_RED, flow_a * 0.75, 1.4);
                })),
            ));
        }
        stack = stack.push(
            Positioned::new()
                .left(160.0)
                .top(y)
                .width(1060.0)
                .height(96.0)
                .child(Painting::sized(Size::new(1060.0, 96.0), PaintWith::new(
                    move |book: &mut Sketchbook, _s: Size| {
                        // The gate's bar.
                        let w = gate_w.max(0.0);
                        book.rrect(pf::xywh(0.0, 8.0, w, 80.0), 14.0, pf::alpha(pf::SURFACE, 0.92));
                        book.stroke_rrect(pf::xywh(0.0, 8.0, w, 80.0), 14.0, pf::alpha(Color::WHITE, 0.05), 1.0);
                        // The pay flash — one warm bloom on the gate.
                        if flash > 0.01 && w > 220.0 {
                            book.layer(flash * 0.8, 18.0, None, |b| {
                                b.rrect(pf::xywh(w - 200.0, 8.0, 200.0, 80.0), 14.0, pf::alpha(BREAK_RED, 0.35 * flash));
                            });
                        }
                        // The toll's stamp, right-aligned inside the bar.
                        if pay_p > 0.5 && w > 220.0 {
                            let stamp_a = (pay_p - 0.5) * 2.0;
                            book.rrect(pf::xywh(w - 208.0, 26.0, 186.0, 44.0), 9.0, pf::alpha(BREAK_RED, 0.14 * stamp_a));
                            book.stroke_rrect(pf::xywh(w - 208.0, 26.0, 186.0, 44.0), 9.0, pf::alpha(BREAK_RED, 0.65 * stamp_a), 1.2);
                        }
                    },
                ))),
        );
        // The gate's words.
        stack = stack.push(
            Positioned::new()
                .left(160.0 + 28.0)
                .top(y + 32.0)
                .width(560.0)
                .height(48.0)
                .child(Opacity::new(in_p).child(
                    Text::new(gate.to_string())
                        .style(pf::geist(22.0).color(pf::alpha(INK, 0.92))),
                )),
        );
        if pay_p > 0.5 && gate_w > 220.0 {
            stack = stack.push(
                Positioned::new()
                    .left(160.0 + gate_w - 208.0 + 18.0)
                    .top(y + 44.0)
                    .width(180.0)
                    .height(28.0)
                    .child(Opacity::new((pay_p - 0.5) * 2.0).child(
                        Text::new(toll.to_string())
                            .style(pf::geist_mono(13.0).letter_spacing(1.0).color(BREAK_RED)),
                    )),
            );
        }
    }

    // The film's voice.
    stack = stack.push(pf::caption("the gaps are not bugs. they are the architecture.", 1002.0, clamp01((t - 0.10) / 0.10)));
    stack = stack.push(pf::caption("every port pays again — in rewrites, drift, and dropped frames.", 966.0, clamp01((t - 0.62) / 0.12)));
    stack = stack.push(pf::chrome(super::progress_rail(ctx.abs)));
    stack.into()
}

// ── Z03 · the_question ──────────────────────────────────────────────────────

/// The question — the need distilled to one line, asked in the dark.
pub fn the_question(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = Stack::new();

    // The room — the film's darkest: a point of light in it, and
    // beneath it, a real 3D floor — a perspective grid the camera
    // dollies over as the question lands. Depth, not decoration.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            book.rect(Rect::new(0.0, 0.0, w, h), Color::rgb(7, 6, 9));
            pf::stars(book, w, h, 0x0A11, 44, t, 0.05);

            // The 3D floor — the camera flies forward for the whole
            // scene, in step with the 2D camera's push. Aimed slightly
            // up, so the grid lives in the lower third and the question
            // keeps the horizon to itself.
            use crate::three_d::{draw_dot3, Camera as Cam3, Vec3};
            let dolly = ease_in_out(clamp01(t / 0.92));
            let cam3 = Cam3 {
                eye: Vec3::new(0.0, 60.0, -80.0 - 520.0 * dolly),
                target: Vec3::new(0.0, 190.0, 420.0),
                fov: 0.72,
            };
            fk::grid_floor_lines(
                book,
                &cam3,
                s,
                1700.0,
                30.0,
                2100.0,
                180.0,
                BRAND_NEAR,
                0.85,
                120.0,
                1500.0,
            );
            // Motes rising off the floor — depth cues, not dust.
            let mote_a = clamp01((t - 0.12) / 0.3);
            if mote_a > 0.01 {
                for k in 0..14 {
                    let mut rng = Rng::new(0x907E ^ (k as u64).wrapping_mul(0x2545));
                    let x = (rng.f01() - 0.5) * 2400.0;
                    let z = 100.0 + rng.f01() * 1600.0;
                    let rise = ((t * 0.14 + rng.f01()) % 1.0) * 320.0;
                    let alpha = mote_a * 0.4 * (1.0 - rise / 320.0);
                    draw_dot3(
                        book,
                        Vec3::new(x, 20.0 + rise, z),
                        2.6,
                        &cam3,
                        s,
                        pf::alpha(BRAND_FAR, alpha),
                    );
                }
            }

            // The point of light — grows, then holds, breathing. It is
            // the shot's subject; it earns its luminance.
            let grow = ease_out_expo(clamp01(t / 0.22));
            let breath = 1.0 + 0.05 * (sec * 1.2).sin();
            // The charge — once the second line has landed, the point
            // gathers. This is the scene's second half, which the v2
            // cut spent holding a still frame.
            let charge = clamp01((t - 0.60) / 0.34);
            let centre = Offset::new(w * 0.5, h * 0.345);
            if grow > 0.01 {
                book.layer(grow, 60.0, None, |glow_book| {
                    glow_book.circle(
                        centre,
                        (34.0 + 46.0 * charge) * grow * breath,
                        pf::alpha(BRAND_FAR, 0.5 + 0.22 * charge),
                    );
                    glow_book.circle(
                        centre,
                        (8.0 + 7.0 * charge) * grow,
                        pf::alpha(Color::WHITE, 0.95),
                    );
                });
            }
            // Three rings leaving the point, staggered — the answer
            // beginning before the film has said what it is.
            if charge > 0.01 {
                for k in 0..3 {
                    let ph = clamp01(charge * 1.35 - k as f32 * 0.24);
                    if ph <= 0.01 || ph >= 1.0 {
                        continue;
                    }
                    let r = 44.0 + 460.0 * ease_out_expo(ph);
                    book.ring(centre, r, 1.4, pf::alpha(BRAND_FAR, 0.34 * (1.0 - ph)));
                }
            }
            pf::vignette(book, w, h, 0.62);
        }),
    )));

    // The question — types on, one line, then its consequence. The
    // caret is the house `type_on`, so it cannot drift.
    stack = stack.push(pf::type_on(
        "what if the distance were the bug?",
        pf::TypeAt::CenteredOn(W as i32 / 2),
        532.0,
        pf::geist(68.0).letter_spacing(0.4).color(pf::alpha(INK, 0.97)),
        clamp01((t - 0.16) / 0.24),
        sec,
    ));
    stack = stack.push(pf::type_on(
        "and the fix were the framework?",
        pf::TypeAt::CenteredOn(W as i32 / 2),
        636.0,
        pf::geist(62.0).letter_spacing(0.4).color(pf::alpha(ACCENT, 0.95)),
        clamp01((t - 0.44) / 0.24),
        sec - 3.5,
    ));

    stack = stack.push(pf::caption("the question every stack answers differently.", 966.0, clamp01((t - 0.72) / 0.12)));
    stack = stack.push(pf::chrome(super::progress_rail(ctx.abs)));
    stack.into()
}

// ── Z03 · the_drift ─────────────────────────────────────────────────────────

/// **The drift.** Z01 said the four ports ship *slightly* different
/// pictures. This scene says what "slightly" costs over time: the same
/// screen, four ports, six releases, and the gap widening every release
/// because nothing in any of the four stacks is the other three's
/// definition of the screen.
///
/// Drawn as four tracks across the body — one per port — each a line of
/// release marks whose vertical offset from the design's own baseline
/// *is* the drift. The baseline is dead straight and labelled; the four
/// tracks leave it and never come back.
pub fn the_drift(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let mut stack = Stack::new();

    const RELEASES: usize = 6;
    const X0: f32 = 300.0;
    const X1: f32 = 1700.0;
    const BASE_Y: f32 = 560.0;
    /// How far each port has wandered by release `k`, in pixels. Not a
    /// formula — four different stacks drift four different ways, and a
    /// smooth curve would say they drift alike.
    const DRIFT: [[f32; RELEASES]; 4] = [
        [0.0, -14.0, -26.0, -52.0, -71.0, -96.0],
        [0.0, 9.0, 24.0, 41.0, 78.0, 112.0],
        [0.0, -6.0, -18.0, -30.0, -44.0, -58.0],
        [0.0, 18.0, 30.0, 63.0, 92.0, 140.0],
    ];
    const COLORS: [Color; 4] = [SYN_TYPE, ACCENT, super::LEDGER, BREAK_RED];

    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(14, 12, 17)),
                    (0.58, BG_DEEP),
                    (1.0, Color::rgb(10, 9, 13)),
                ]),
            );

            // The design's own baseline — where every port was supposed
            // to stay. Drawn first, held all scene, never moves.
            let base_p = ease_out_expo(clamp01(t / 0.12));
            if base_p > 0.01 {
                book.rect(
                    Rect::new(X0, BASE_Y - 1.0, X0 + (X1 - X0) * base_p, BASE_Y + 1.0),
                    pf::alpha(Color::WHITE, 0.30),
                );
                for k in 0..RELEASES {
                    let x = X0 + (X1 - X0) * (k as f32 / (RELEASES - 1) as f32);
                    if x > X0 + (X1 - X0) * base_p {
                        break;
                    }
                    book.rect(pf::xywh(x - 0.75, BASE_Y - 8.0, 1.5, 16.0), pf::alpha(Color::WHITE, 0.20));
                }
            }

            // The four tracks. Each release lands as a mark; the segment
            // to it grows on its own beat; the gap to the baseline is
            // filled, because the *gap* is the subject.
            for (i, drift) in DRIFT.iter().enumerate() {
                let color = COLORS[i];
                let mut pts: Vec<Offset> = Vec::new();
                for (k, d) in drift.iter().enumerate() {
                    let x = X0 + (X1 - X0) * (k as f32 / (RELEASES - 1) as f32);
                    pts.push(Offset::new(x, BASE_Y + d));
                }
                let landed = clamp01((t - 0.16 - i as f32 * 0.03) / 0.62) * (RELEASES - 1) as f32;
                for k in 0..RELEASES - 1 {
                    let seg = clamp01(landed - k as f32);
                    if seg <= 0.001 {
                        break;
                    }
                    let a = pts[k];
                    let b = pts[k + 1];
                    let to = Offset::new(a.dx + (b.dx - a.dx) * seg, a.dy + (b.dy - a.dy) * seg);
                    book.line(a, to, pf::alpha(color, 0.85), 2.2);
                    // The gap, shaded — how far this port is from the
                    // design at this release.
                    if seg > 0.98 {
                        let hatch = (b.dy - BASE_Y).abs();
                        if hatch > 6.0 {
                            book.rect(
                                pf::xywh(b.dx - 1.0, BASE_Y.min(b.dy), 2.0, hatch),
                                pf::alpha(color, 0.22),
                            );
                        }
                        book.circle(b, 4.4, pf::alpha(color, 0.95));
                        book.circle(b, 9.0, pf::alpha(color, 0.16));
                    }
                }
                book.circle(pts[0], 4.4, pf::alpha(color, 0.9));
            }
            pf::vignette(book, w, h, 0.52);
        }),
    )));

    // The release axis, named — six releases is a year, and the film
    // should say so rather than leave six ticks to be counted.
    let axis_a = ease_out_cubic(clamp01((t - 0.10) / 0.14));
    if axis_a > 0.01 {
        for k in 0..RELEASES {
            let x = X0 + (X1 - X0) * (k as f32 / (RELEASES - 1) as f32);
            stack = stack.push(Positioned::new().left(x - 60.0).top(898.0).width(120.0).height(28.0).child(
                Opacity::new(axis_a * clamp01((t - 0.10 - k as f32 * 0.02) / 0.14)).child(
                    Text::new(format!("r{}", k + 1))
                        .style(pf::geist_mono(17.0).letter_spacing(1.6).color(pf::alpha(MUTED, 0.75)))
                        .align(TextAlign::Center),
                ),
            ));
        }
        stack = stack.push(Positioned::new().left(X0).top(928.0).width(1400.0).height(26.0).child(
            Opacity::new(axis_a).child(
                Text::new("six releases · one year".to_string())
                    .style(pf::geist_mono(15.0).letter_spacing(2.6).color(pf::alpha(pf::FAINT, 0.8)))
                    .align(TextAlign::Center),
            ),
        ));
    }

    // The baseline's own label, and the four ports'.
    let lab_a = ease_out_cubic(clamp01((t - 0.14) / 0.14));
    if lab_a > 0.01 {
        stack = stack.push(Positioned::new().left(super::MARGIN).top(BASE_Y - 16.0).width(200.0).height(30.0).child(
            Opacity::new(lab_a).child(
                Text::new("the design".to_string())
                    .style(pf::geist_mono(18.0).letter_spacing(1.6).color(pf::alpha(INK, 0.9))),
            ),
        ));
    }
    // The four labels sit at their track's own end — except where two
    // ends are closer than a line of type, which is the case for the two
    // that drift the same way. They fan to a minimum pitch, in the order
    // their tracks end, so a label still points at its own track.
    let mut ends: Vec<(usize, f32)> = (0..4).map(|i| (i, BASE_Y + DRIFT[i][RELEASES - 1])).collect();
    ends.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    let mut placed: Vec<(usize, f32)> = Vec::new();
    for (i, y) in ends {
        let y = match placed.last() {
            Some((_, prev)) if y - prev < 40.0 => prev + 40.0,
            _ => y,
        };
        placed.push((i, y));
    }
    for (i, port) in PORTS.iter().enumerate() {
        let a = ease_out_cubic(clamp01((t - 0.22 - i as f32 * 0.05) / 0.16));
        if a <= 0.01 {
            continue;
        }
        let end_y = placed.iter().find(|(k, _)| *k == i).map(|(_, y)| *y).unwrap_or(BASE_Y);
        stack = stack.push(Positioned::new().left(X1 + 18.0).top(end_y - 14.0).width(220.0).height(30.0).child(
            Opacity::new(a).child(
                Text::new((*port).to_string())
                    .style(pf::geist_mono(17.0).letter_spacing(1.2).color(pf::alpha(COLORS[i], 0.95))),
            ),
        ));
    }

    // The count — pixels of drift, summed, arriving as the tracks land.
    let sum_a = clamp01((t - 0.56) / 0.14);
    if sum_a > 0.01 {
        let total: f32 = DRIFT.iter().map(|d| d[RELEASES - 1].abs()).sum();
        let shown = pf::count_up(total as u64, clamp01((t - 0.56) / 0.26));
        stack = stack.push(Positioned::new().left(super::MARGIN).top(306.0).width(760.0).height(96.0).child(
            Opacity::new(sum_a).child(
                Text::new(format!("{shown} px apart"))
                    .style(pf::geist(76.0).bold().letter_spacing(-1.0).color(pf::alpha(INK, 0.97))),
            ),
        ));
        stack = stack.push(Positioned::new().left(super::MARGIN).top(406.0).width(900.0).height(30.0).child(
            Opacity::new(sum_a).child(
                Text::new("after a year — and not one of the four is wrong on its own platform.".to_string())
                    .style(pf::geist_mono(18.0).letter_spacing(1.1).color(pf::alpha(MUTED, 0.9))),
            ),
        ));
    }

    pf::caption("the same screen, four ports, six releases.", 1002.0, clamp01((t - 0.06) / 0.10));
    pf::caption("nobody drifts on purpose. everybody drifts.", 966.0, clamp01((t - 0.60) / 0.10));
    stack = stack.push(pf::chrome(super::progress_rail(ctx.abs)));
    let _ = sec;
    stack.into()
}

// ── Z04 · the_bridge ────────────────────────────────────────────────────────

/// **The bridge tax.** The other three scenes cost you consistency; this
/// one costs you frames. A cross-platform stack that is not native puts a
/// boundary between the program and the pixels — a bridge, a serializer,
/// an interpreter, a DOM — and every frame has to cross it, twice.
///
/// Drawn literally: two shores, a gap between them, and packets crossing
/// on a fixed budget clock. The ones that make it land green; the ones
/// still mid-air when the frame ends fall. The counter at the bottom is
/// the frames that fell.
pub fn the_bridge(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let mut stack = Stack::new();

    const LEFT_X: f32 = 250.0;
    const RIGHT_X: f32 = 1670.0;
    const MID_Y: f32 = 590.0;
    /// The four things that sit in the gap, named.
    const TOLLBOOTHS: [(&str, f32); 4] = [
        ("serialize", 0.18),
        ("bridge", 0.40),
        ("interpret", 0.62),
        ("reflow", 0.84),
    ];

    let shore_p = ease_out_expo(clamp01(t / 0.14));
    let cross_p = clamp01((t - 0.18) / 0.10);
    // A packet every 0.42 s; a frame's budget is up 0.34 s after it left.
    let dropped = ((sec - 2.6).max(0.0) / 1.15).floor().max(0.0) as u64;

    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(15, 12, 16)),
                    (0.56, BG_DEEP),
                    (1.0, Color::rgb(10, 9, 12)),
                ]),
            );

            // The two shores — your program on the left, the screen on
            // the right, both solid, both fine. The problem is between.
            for (x, right) in [(LEFT_X, false), (RIGHT_X, true)] {
                let wdt = 210.0 * shore_p;
                let r = if right {
                    pf::xywh(x, MID_Y - 150.0, wdt, 300.0)
                } else {
                    pf::xywh(x - wdt, MID_Y - 150.0, wdt, 300.0)
                };
                book.rrect(r, 16.0, pf::alpha(Color::rgb(0x15, 0x13, 0x1A), 0.97));
                book.stroke_rrect(r, 16.0, pf::alpha(Color::WHITE, 0.08), 1.1);
                for row in 0..5 {
                    book.rrect(
                        pf::xywh(r.left + 22.0, r.top + 40.0 + row as f32 * 44.0, (r.width() - 44.0) * (0.55 + 0.12 * row as f32).min(1.0), 10.0),
                        5.0,
                        pf::alpha(if right { SYN_TYPE } else { ACCENT }, 0.45),
                    );
                }
            }

            // The gap, and what is in it.
            if cross_p > 0.01 {
                let g0 = LEFT_X + 14.0;
                let g1 = RIGHT_X - 14.0;
                book.rect(
                    pf::xywh(g0, MID_Y - 120.0, g1 - g0, 240.0),
                    Gradient::vertical().with_dither().with_stops(&[
                        (0.0, pf::alpha(BREAK_RED, 0.0)),
                        (0.5, pf::alpha(BREAK_RED, 0.055 * cross_p)),
                        (1.0, pf::alpha(BREAK_RED, 0.0)),
                    ]),
                );
                for (k, (_, at)) in TOLLBOOTHS.iter().enumerate() {
                    let a = clamp01((t - 0.20 - k as f32 * 0.05) / 0.14);
                    if a <= 0.01 {
                        continue;
                    }
                    let x = g0 + (g1 - g0) * at;
                    book.rect(pf::xywh(x - 1.0, MID_Y - 96.0, 2.0, 192.0), pf::alpha(BREAK_RED, 0.34 * a));
                    book.circle(Offset::new(x, MID_Y), 5.0, pf::alpha(BREAK_RED, 0.8 * a));
                }

                // The packets. Each leaves on its own beat and takes
                // 1.15 s to cross — longer than a frame's 16.7 ms will
                // ever be, which is the joke and also the point.
                for k in 0..9 {
                    let born = 0.9 + k as f32 * 1.15;
                    let age = sec - born;
                    if age < 0.0 || age > 1.9 {
                        continue;
                    }
                    let u = clamp01(age / 1.15);
                    let x = g0 + (g1 - g0) * u;
                    let fell = age > 1.15;
                    let fall = clamp01((age - 1.15) / 0.75);
                    let y = MID_Y + if fell { fall * fall * 280.0 } else { 0.0 };
                    let a = if fell { 1.0 - fall } else { 1.0 };
                    let color = if fell { BREAK_RED } else { SYN_TYPE };
                    book.rrect(pf::xywh(x - 18.0, y - 11.0, 36.0, 22.0), 6.0, pf::alpha(color, 0.9 * a));
                    book.rrect(pf::xywh(x - 12.0, y - 4.0, 24.0, 3.0), 1.5, pf::alpha(Color::BLACK, 0.4 * a));
                    if !fell {
                        book.rrect(pf::xywh(x - 40.0, y - 2.0, 24.0, 4.0), 2.0, pf::alpha(color, 0.25));
                    }
                }
            }
            pf::vignette(book, w, h, 0.54);
        }),
    )));

    // The shores' names, and the booths'.
    let lab = ease_out_cubic(clamp01((t - 0.10) / 0.14));
    if lab > 0.01 {
        stack = stack.push(Positioned::new().left(LEFT_X - 210.0).top(MID_Y + 168.0).width(210.0).height(30.0).child(
            Opacity::new(lab).child(Text::new("your program".to_string())
                .style(pf::geist_mono(18.0).letter_spacing(1.4).color(pf::alpha(ACCENT, 0.95)))
                .align(TextAlign::Center)),
        ));
        stack = stack.push(Positioned::new().left(RIGHT_X).top(MID_Y + 168.0).width(210.0).height(30.0).child(
            Opacity::new(lab).child(Text::new("the screen".to_string())
                .style(pf::geist_mono(18.0).letter_spacing(1.4).color(pf::alpha(SYN_TYPE, 0.95)))
                .align(TextAlign::Center)),
        ));
    }
    for (k, (name, at)) in TOLLBOOTHS.iter().enumerate() {
        let a = ease_out_cubic(clamp01((t - 0.20 - k as f32 * 0.05) / 0.16));
        if a <= 0.01 {
            continue;
        }
        let x = LEFT_X + 14.0 + (RIGHT_X - LEFT_X - 28.0) * at;
        stack = stack.push(Positioned::new().left(x - 100.0).top(MID_Y - 136.0).width(200.0).height(28.0).child(
            Opacity::new(a).child(Text::new((*name).to_string())
                .style(pf::geist_mono(17.0).letter_spacing(1.4).color(pf::alpha(BREAK_RED, 0.9)))
                .align(TextAlign::Center)),
        ));
    }

    // The tally — frames that did not make it across in time.
    let tally_a = clamp01((t - 0.30) / 0.12);
    if tally_a > 0.01 {
        stack = stack.push(Positioned::new().left(super::MARGIN).top(300.0).width(900.0).height(96.0).child(
            Opacity::new(tally_a).child(
                Text::new(format!("{dropped} frame{} dropped", if dropped == 1 { "" } else { "s" }))
                    .style(pf::geist(72.0).bold().letter_spacing(-1.0).color(pf::alpha(BREAK_RED, 0.96))),
            ),
        ));
        stack = stack.push(Positioned::new().left(super::MARGIN).top(400.0).width(1000.0).height(30.0).child(
            Opacity::new(tally_a).child(
                Text::new("every one of them crossed a boundary your program did not ask for.".to_string())
                    .style(pf::geist_mono(18.0).letter_spacing(1.1).color(pf::alpha(MUTED, 0.9))),
            ),
        ));
    }

    pf::caption("between the program and the pixels, something else.", 1002.0, clamp01((t - 0.06) / 0.10));
    pf::caption("a frame that has to be translated is a frame you can lose.", 966.0, clamp01((t - 0.52) / 0.10));
    stack = stack.push(pf::chrome(super::progress_rail(ctx.abs)));
    stack.into()
}
