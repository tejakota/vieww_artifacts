//! Movement I · THE NEED — why another UI/UX framework at all.
//!
//! Three scenes, 28 seconds. The register is curiosity shading into
//! need: one picture every product wants to ship, four ports that
//! each ship something slightly else, the tolls every port pays, and
//! then the question the whole film exists to ask.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Shadow, Sketchbook, TextAlign};
use vieww_widget::prelude::*;

use crate::film_lib::{clamp01, ease_out_cubic, ease_out_expo};
use crate::product_film as pf;
use super::{
    BRAND_FAR, BRAND_NEAR, PORTS, ACCENT, BG_DEEP, BREAK_RED, CANVAS, INK, MUTED, SYN_TYPE, W,
};

// ── Z01 · the_same_picture ──────────────────────────────────────────────────

/// The design's own geometry — the one card every port is supposed to
/// ship. Centered on the canvas, below the caption band.
const CARD: Rect = Rect::new(740.0, 280.0, 1180.0, 840.0);

/// The four ports' stations, clockwise from top-left.
const STATIONS: [Rect; 4] = [
    Rect::new(120.0, 210.0, 420.0, 410.0),
    Rect::new(1500.0, 210.0, 1800.0, 410.0),
    Rect::new(120.0, 770.0, 420.0, 970.0),
    Rect::new(1500.0, 770.0, 1800.0, 970.0),
];

/// Where each rail leaves the truth card's edge.
const CARD_ANCHORS: [Offset; 4] = [
    Offset::new(740.0, 370.0),
    Offset::new(1180.0, 370.0),
    Offset::new(740.0, 750.0),
    Offset::new(1180.0, 750.0),
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
                // lands on the station, drawn to its own progress.
                if rail_p > 0.01 {
                    let end = Offset::new(
                        anchor.dx + (station_center(station).dx - anchor.dx) * rail_p,
                        anchor.dy + (station_center(station).dy - anchor.dy) * rail_p,
                    );
                    let mid1 = Offset::new(
                        anchor.dx + (end.dx - anchor.dx) * 0.45,
                        anchor.dy - 70.0,
                    );
                    let mid2 = Offset::new(
                        anchor.dx + (end.dx - anchor.dx) * 0.55,
                        end.dy - 70.0,
                    );
                    let mut path = Path::new();
                    path.move_to(anchor).cubic_to(mid1, mid2, end);
                    book.stroke(path, pf::alpha(ACCENT, 0.16 + 0.30 * rail_p), 1.4);
                    // The rail's tip — a small landing pulse.
                    book.circle(end, 3.2, pf::alpha(ACCENT, 0.9 * rail_p));
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
                .left(W - 640.0)
                .top(150.0)
                .width(460.0)
                .height(30.0)
                .child(
                    Text::new(format!("rewrite № {ports_in} of 4 — same picture, four codes"))
                        .style(pf::geist_mono(14.0).letter_spacing(1.4).color(pf::alpha(MUTED, 0.9)))
                        .align(TextAlign::Right),
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
    stack = stack.push(Positioned::new().left(1330.0).top(250.0).width(400.0).height(400.0).child(
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
        })),
    ));
    // The gauge's numerals — a widget over the painting.
    stack = stack.push(
        Positioned::new()
            .left(1330.0)
            .top(400.0)
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
        let y = 268.0 + i as f32 * 118.0;
        let gate_w = 980.0 * ease_out_expo(in_p);
        let (gate, toll) = (*gate, *toll);
        let flash = if pay_p > 0.0 && pay_p < 1.0 { 1.0 - pay_p } else { 0.0 };
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

    // The room — the film's darkest: a point of light in it.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let (w, h) = (s.width, s.height);
            book.rect(Rect::new(0.0, 0.0, w, h), Color::rgb(7, 6, 9));
            pf::stars(book, w, h, 0x0A11, 44, t, 0.05);
            // The point of light — grows, then holds, breathing.
            let grow = ease_out_expo(clamp01(t / 0.22));
            let breath = 1.0 + 0.05 * (sec * 1.2).sin();
            if grow > 0.01 {
                book.layer(grow, 60.0, None, |glow_book| {
                    glow_book.circle(
                        Offset::new(w * 0.5, h * 0.44),
                        26.0 * grow * breath,
                        pf::alpha(BRAND_FAR, 0.30),
                    );
                    glow_book.circle(
                        Offset::new(w * 0.5, h * 0.44),
                        5.0 * grow,
                        pf::alpha(Color::WHITE, 0.8),
                    );
                });
            }
            pf::vignette(book, w, h, 0.62);
        }),
    )));

    // The question — types on, one line, then its consequence. The
    // caret is the house `type_on`, so it cannot drift.
    stack = stack.push(pf::type_on(
        "what if the distance were the bug?",
        pf::TypeAt::CenteredOn(W as i32 / 2),
        560.0,
        pf::geist(36.0).letter_spacing(1.2).color(pf::alpha(INK, 0.97)),
        clamp01((t - 0.16) / 0.24),
        sec,
    ));
    stack = stack.push(pf::type_on(
        "and the fix were the framework?",
        pf::TypeAt::CenteredOn(W as i32 / 2),
        626.0,
        pf::geist(36.0).letter_spacing(1.2).color(pf::alpha(ACCENT, 0.95)),
        clamp01((t - 0.44) / 0.24),
        sec - 3.5,
    ));

    stack = stack.push(pf::caption("the question every stack answers differently.", 966.0, clamp01((t - 0.72) / 0.12)));
    stack = stack.push(pf::chrome(super::progress_rail(ctx.abs)));
    stack.into()
}
