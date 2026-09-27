//! S03 · THE TITLE — Ki's close. 0:20–0:26.
//!
//! The drop falls; where it lands, **vieww** springs up — the wordmark
//! drop (E-03) with the underline's overshoot (E-04), at master scale.
//! One drop became a word. The film's genesis image: everything else in
//! the next 154 seconds grows from this landing.
//!
//! The springs are the receipt: underdamped, one arrival, two visible
//! bounces — and the receipt constants ride the motion (ω/ζ printed by
//! the curve itself in the spring plate; here the curve *is* the film).

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle, FontWeight};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{alpha, clamp01, ease_out_cubic, mix, spring_out, tint, xywh, BG_DEEP, FAINT, INK, MUTED, Rng, VIOLET, VIOLET_SOFT};

use super::{Ctx};

// ── The two springs — the same constants the spring plate proved ───────────

const DROP_OMEGA: f32 = 6.2;
const DROP_ZETA: f32 = 0.52;
const LINE_OMEGA: f32 = 13.5;
const LINE_ZETA: f32 = 0.40;

/// The drop's fall starts where S02's condensation left it.
const DROP_X: f32 = 960.0;
const DROP_Y0: f32 = 396.0;
/// The floor — where the mass lands.
const FLOOR_Y: f32 = 716.0;
/// The wordmark's rest position.
const MARK_TOP: f32 = 300.0;
const MARK_SIZE: f32 = 168.0;
const MARK: &str = "vieww";

/// When the drop lands (in-scene fraction) and the mark fires.
const LAND_T: f32 = 0.34;
const MARK_T0: f32 = 0.36;
const LINE_T0: f32 = 0.52;

fn drop_s(t: f32) -> f32 {
    spring_out(clamp01((t - MARK_T0) / 0.58), DROP_OMEGA, DROP_ZETA)
}

fn line_s(t: f32) -> f32 {
    spring_out(clamp01((t - LINE_T0) / 0.40), LINE_OMEGA, LINE_ZETA)
}

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;

    // The falling drop — gravity from S02's last position. The fall
    // accelerates (t²), stretches with speed, and dies at impact.
    let fall_u = clamp01(t / LAND_T);
    let fall_y = DROP_Y0 + (FLOOR_Y - DROP_Y0) * fall_u * fall_u;
    let impact = 1.0 - clamp01((t - LAND_T) / 0.06);

    // The mark's spring progress.
    let s = drop_s(t);

    let mut stack = Stack::new();

    // The drop itself (visible until it lands).
    if t < LAND_T {
        let speed = fall_u;
        let stretch = 1.0 + speed * 0.8;
        let dr = 13.0 * (1.0 - 0.3 * speed);
        stack = stack.push(
            Positioned::fill().child(Painting::sized(
                super::CANVAS,
                PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                    book.layer(1.0, 7.0, None, |g| {
                        g.circle(
                            Offset::new(DROP_X, fall_y),
                            dr * 3.2,
                            Gradient::radial_fill().with_dither().with_stops(&[
                                (0.0, alpha(VIOLET, 0.28)),
                                (1.0, alpha(VIOLET, 0.0)),
                            ]),
                        );
                    });
                    // The stretched body.
                    let mut p = vieww_foundation::Path::new();
                    let r = dr;
                    let sy = stretch;
                    p.move_to(Offset::new(DROP_X, fall_y - r * 2.1 * sy));
                    p.line_to(Offset::new(DROP_X + r * 0.95, fall_y - r * 0.15));
                    p.line_to(Offset::new(DROP_X, fall_y + r * 1.05));
                    p.line_to(Offset::new(DROP_X - r * 0.95, fall_y - r * 0.15));
                    p.close();
                    g_fill_drop(book, p);
                    book.circle(
                        Offset::new(DROP_X - r * 0.3, fall_y - r * 0.5),
                        r * 0.2,
                        alpha(Color::WHITE, 0.7),
                    );
                }),
            )),
        );
    }

    // The impact — flash, rings, and a burst of dust flying outward.
    if impact > 0.0 && t >= LAND_T {
        let ring_r = 26.0 + 300.0 * (1.0 - impact);
        stack = stack.push(
            Positioned::fill().child(Painting::sized(
                super::CANVAS,
                PaintWith::new(move |book: &mut Sketchbook, _sz: Size| {
                    let c = Offset::new(DROP_X, FLOOR_Y);
                    // The rings — two, expanding and fading.
                    book.ring(c, ring_r, 2.4, alpha(VIOLET_SOFT, impact * 0.55));
                    book.ring(c, ring_r * 0.66, 1.2, alpha(MUTED, impact * 0.35));
                    // The floor flash.
                    book.layer(1.0, 14.0, None, |g| {
                        g.circle(
                            c,
                            90.0 + 140.0 * (1.0 - impact),
                            Gradient::radial_fill().with_dither().with_stops(&[
                                (0.0, alpha(tint(VIOLET_SOFT, 0.3), 0.4 * impact)),
                                (1.0, alpha(VIOLET, 0.0)),
                            ]),
                        );
                    });
                    // Dust burst — deterministic motes, flying outward.
                    let mut rng = Rng::new(0x300D);
                    for _ in 0..26 {
                        let ang = rng.f01() * std::f32::consts::TAU;
                        let dist = (30.0 + rng.f01() * 220.0) * (1.0 - impact * 0.3);
                        let r = 1.0 + rng.f01() * 2.2;
                        book.circle(
                            Offset::new(c.dx + ang.cos() * dist, c.dy + ang.sin() * dist * 0.42 - dist * 0.16),
                            r,
                            alpha(VIOLET_SOFT, impact * (0.25 + rng.f01() * 0.4)),
                        );
                    }
                }),
            )),
        );
    }

    // The wordmark — springs up from the impact. Squash and stretch with
    // the spring's own velocity; the bounce is visible twice.
    if s > 0.0 {
        let y = MARK_TOP - 420.0 * (1.0 - s);
        let vel = (s - 1.0).abs().min(0.22);
        let squash = 1.0 - vel * 0.5;
        let box_h = 210.0 * squash;
        let box_w = 1050.0 * (1.0 + vel * 0.3);

        // The ground shadow — firms as the mark nears.
        let near = clamp01(1.0 - (MARK_TOP - y) / 420.0);
        stack = stack.push(
            Positioned::new()
                .left(DROP_X - box_w * 0.5)
                .top(FLOOR_Y + 24.0)
                .width(box_w)
                .height(80.0)
                .child(Painting::sized(Size::new(box_w, 80.0), PaintWith::new(
                    move |book: &mut Sketchbook, _sz: Size| {
                        book.circle(
                            Offset::new(box_w * 0.5, 30.0),
                            120.0 + 220.0 * near,
                            Gradient::radial_fill().with_dither().with_stops(&[
                                (0.0, alpha(Color::BLACK, 0.30 * near + 0.05)),
                                (1.0, alpha(Color::BLACK, 0.0)),
                            ]),
                        );
                    },
                ))),
        );

        stack = stack.push(
            Positioned::new()
                .left(DROP_X - box_w * 0.5)
                .top(y)
                .width(box_w)
                .height(box_h.max(1.0))
                .child(
                    Text::new(MARK)
                        .style(
                            TextStyle::new(MARK_SIZE)
                                .weight(FontWeight::Regular)
                                .letter_spacing(10.0)
                                .color(INK),
                        )
                        .align(TextAlign::Center),
                ),
        );
    }

    // The underline — overshoots on a stiffer spring (E-04).
    let u = line_s(t);
    if u > 0.001 {
        let rest_w = 760.0;
        let w = rest_w * u;
        stack = stack.push(
            Positioned::new()
                .left(DROP_X - 380.0)
                .top(FLOOR_Y + 8.0)
                .width((w.max(2.0)) + 4.0)
                .height(12.0)
                .child(Painting::sized(Size::new(w.max(2.0) + 4.0, 12.0), PaintWith::new(
                    move |book: &mut Sketchbook, _sz: Size| {
                        book.rrect(
                            xywh(0.0, 0.0, w, 5.0),
                            2.5,
                            Gradient::horizontal().with_dither().with_stops(&[
                                (0.0, alpha(VIOLET, 0.95)),
                                (0.55, alpha(VIOLET_SOFT, 0.85)),
                                (1.0, alpha(VIOLET_SOFT, 0.12)),
                            ]),
                        );
                    },
                ))),
        );
    }

    // The tagline — arrives only after both springs settle.
    let tag_a = clamp01((t - 0.68) / 0.18);
    if tag_a > 0.01 {
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(FLOOR_Y + 64.0)
                .width(1920.0)
                .height(30.0)
                .child(
                    Opacity::new(tag_a).child(
                        Text::new("the ui runtime that renders its own film")
                            .style(TextStyle::new(22.0).monospace().letter_spacing(5.0).color(alpha(MUTED, 0.85)))
                            .align(TextAlign::Center),
                    ),
                ),
        );
    }

    // The ground — quiet, the wait is over.
    let bg = Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, size: Size| {
            let w = size.width;
            let h = size.height;
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, Color::rgb(9, 9, 13)),
                    (0.6, BG_DEEP),
                    (1.0, Color::rgb(12, 11, 18)),
                ]),
            );
            // Sparse stars — calmer than S01.
            let mut rng = Rng::new(0x502);
            for _ in 0..64 {
                let x = rng.f01() * w;
                let y = rng.f01() * h;
                let r = 0.4 + rng.f01() * 0.9;
                let tw = 0.5 + 0.5 * (t * 3.0 + rng.f01() * 9.0).sin();
                book.circle(Offset::new(x, y), r, alpha(Color::WHITE, 0.03 + 0.07 * tw));
            }
            // The horizon glow — under the mark, wide and low.
            book.layer(1.0, 44.0, None, |g| {
                g.circle(
                    Offset::new(w * 0.5, h * 0.78),
                    w * 0.30,
                    Gradient::radial_fill().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.12)),
                        (1.0, alpha(VIOLET, 0.0)),
                    ]),
                );
            });
            // The floor line — where the mass landed.
            book.line(
                Offset::new(DROP_X - 560.0, FLOOR_Y + 12.0),
                Offset::new(DROP_X + 560.0, FLOOR_Y + 12.0),
                alpha(FAINT, 0.22),
                1.0,
            );
            super::vignette(book, w, h, 0.45);
        }),
    );

    Stack::new()
        .push(Positioned::fill().child(bg))
        .push(stack)
        .into()
}

/// The drop's body — gradient-filled teardrop.
fn g_fill_drop(book: &mut Sketchbook, p: vieww_foundation::Path) {
    book.fill(
        p,
        Gradient::linear(Offset::new(DROP_X, DROP_Y0), Offset::new(DROP_X, FLOOR_Y)).with_dither().with_stops(&[
            (0.0, alpha(tint(VIOLET_SOFT, 0.55), 0.98)),
            (0.5, alpha(VIOLET, 0.92)),
            (1.0, alpha(mix(VIOLET, Color::BLACK, 0.35), 0.92)),
        ]),
    );
}
