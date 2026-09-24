//! exp_light — the calibration frame: "Light." from the author's sheet-vf,
//! recreated through vieww's own rasterizer.
//!
//! **What the reference cell contains** (VLM read of sheet-vf, cell 4):
//! a multi-layer gradient mesh — "liquid light or nebula" in deep blues,
//! purples, pinks — with a translucent glassmorphism card, tiny mono tags
//! ("material", "curved"), massive "Light." type, and a subtext line.
//!
//! **What this experiment proves:** the framework's gradient + blur + text
//! surface reaches the author's aesthetic floor *before* we try to exceed it.
//! If this frame reads as "at basic", round two aims higher with confidence.

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook};
use vieww_widget::prelude::*;
use vieww_widget::{Painting, PaintWith};

use crate::film_lib::{
    alpha, clamp01, ease_in_out, mix, CANVAS, CANVAS_H, CANVAS_W, CYAN, CYAN_SOFT, INK, MAGENTA,
    MUTED, Rng, VIOLET, VIOLET_DEEP, VIOLET_SOFT,
};

/// Film-time this experiment spans.
pub const SECONDS: f32 = 4.0;

/// The nebula — layered radial gradients inside one painter, with a blurred
/// core group for that "liquid light" softness. Pure function of `t`.
fn nebula(book: &mut Sketchbook, canvas: Size, t: f32) {
    let w = canvas.width;
    let h = canvas.height;
    let flat = std::env::var("FILM_BISECT")
        .map(|v| v.split(',').any(|s| s.trim() == "flatlayers"))
        .unwrap_or(false);

    // The ground: near-black, slightly lifted at the top-left.
    book.rect(
        Rect::new(0.0, 0.0, w, h),
        Gradient::linear(Offset::new(0.0, 0.0), Offset::new(1.0, 1.0))
            .with_dither()
            .with_stops(&[
                (0.0, Color::rgb(14, 12, 24)),
                (0.55, Color::rgb(8, 8, 13)),
                (1.0, Color::rgb(5, 5, 9)),
            ]),
    );

    // The nebula core: bright blobs, drawn as one blurred layer.
    let drift = (t * std::f32::consts::TAU).sin();
    let drift2 = (t * std::f32::consts::TAU + 1.7).sin();
    let nebula_blobs = |book: &mut Sketchbook| {
        blob(book, w * 0.26 + drift * 26.0, h * 0.40, w * 0.38, MAGENTA, 0.26);
        blob(book, w * 0.70 + drift2 * 20.0, h * 0.28, w * 0.34, VIOLET, 0.30);
        blob(book, w * 0.52, h * 0.62 - drift * 16.0, w * 0.44, VIOLET_DEEP, 0.34);
        blob(book, w * 0.80, h * 0.52, w * 0.26, CYAN, 0.20);
    };
    if flat {
        nebula_blobs(book);
    } else {
        book.layer(1.0, 42.0, None, nebula_blobs);
    }

    // A second, sharper veil of color over the blur — the "mesh" feel.
    let veil = |book: &mut Sketchbook| {
        blob(book, w * 0.42 - drift2 * 18.0, h * 0.46, w * 0.26, CYAN_SOFT, 0.10);
        blob(book, w * 0.60 + drift * 12.0, h * 0.40, w * 0.20, VIOLET_SOFT, 0.12);
    };
    if flat {
        veil(book);
    } else {
        book.layer(1.0, 14.0, None, veil);
    }

    // The heart: a deep-purple core directly behind the card, so the glass
    // has something bright to refract. This is what the reference has and
    // round two lacked — the card is a light source, not a sticker.
    book.circle(
        Offset::new(w * 0.5, h * 0.5),
        w * 0.30,
        Gradient::radial_fill()
            .with_dither()
            .with_stops(&[
                (0.0, alpha(mix(VIOLET_DEEP, VIOLET, 0.4), 0.85)),
                (0.6, alpha(VIOLET_DEEP, 0.55)),
                (1.0, alpha(VIOLET_DEEP, 0.0)),
            ]),
    );

    // Star-dust: deterministic pinpoints, most visible in the dark corners.
    let mut rng = Rng::new(0x1105);
    for i in 0..230 {
        let x = rng.f01() * w;
        let y = rng.f01() * h;
        let r = 0.6 + rng.f01() * 1.1;
        let twinkle = 0.35 + 0.65 * (t * 6.0 + i as f32 * 0.7).sin().max(0.0);
        let a = 0.10 + 0.34 * twinkle;
        inner_dot(book, x, y, r, alpha(Color::WHITE, a));
    }
}

fn blob(book: &mut Sketchbook, x: f32, y: f32, r: f32, color: Color, a: f32) {
    book.circle(
        Offset::new(x, y),
        r,
        Gradient::radial_fill().with_stops(&[
            (0.0, alpha(color, a)),
            (0.55, alpha(color, a * 0.45)),
            (1.0, alpha(color, 0.0)),
        ]),
    );
}

fn inner_dot(book: &mut Sketchbook, x: f32, y: f32, r: f32, color: Color) {
    book.circle(Offset::new(x, y), r, color);
}

/// The type's glow — a blurred gradient blob behind the wordmark, breathing.
fn glow(book: &mut Sketchbook, canvas: Size, t: f32) {
    let breathe = 0.5 + 0.5 * (t * std::f32::consts::TAU * 0.75).sin();
    let cx = canvas.width * 0.5;
    let cy = canvas.height * 0.5 + 6.0;
    let flat = std::env::var("FILM_BISECT")
        .map(|v| v.split(',').any(|s| s.trim() == "flatlayers"))
        .unwrap_or(false);
    // The bloom: big, colored, breathing — brighter than round two dared.
    let bloom = |book: &mut Sketchbook| {
        blob(
            book,
            cx,
            cy,
            canvas.width * (0.22 + 0.05 * breathe),
            VIOLET_SOFT,
            0.55,
        );
        blob(book, cx - 100.0, cy - 60.0, canvas.width * 0.12, CYAN_SOFT, 0.30);
        blob(book, cx + 130.0, cy - 20.0, canvas.width * 0.10, MAGENTA, 0.22);
        blob(book, cx + 40.0, cy + 80.0, canvas.width * 0.09, mix(MAGENTA, VIOLET, 0.5), 0.18);
    };
    if flat {
        bloom(book);
    } else {
        book.layer(1.0, 38.0 + breathe * 26.0, None, bloom);
    }
}

/// The rim: light escaping around the card's edges — the "light source" read.
/// Drawn above the card so it is not eaten by the backdrop filter.
fn rim(book: &mut Sketchbook, canvas: Size, t: f32, card: Rect) {
    let breathe = 0.5 + 0.5 * (t * std::f32::consts::TAU * 0.6).sin();
    // Light escaping the perimeter: bright strokes hugging the card edge,
    // blurred — the blur spreads inward onto the glass (a lit rim) and
    // outward into the void (the bloom). Strokes, not a filled halo: the
    // card's interior stays clean.
    book.layer(1.0, 36.0, None, |inner| {
        let e1 = Rect::new(card.left - 5.0, card.top - 5.0, card.right + 5.0, card.bottom + 5.0);
        inner.stroke_rrect(e1, 26.0, alpha(VIOLET_SOFT, 0.60 + 0.10 * breathe), 3.5);
        let e2 = Rect::new(card.left - 14.0, card.top - 14.0, card.right + 14.0, card.bottom + 14.0);
        inner.stroke_rrect(e2, 34.0, alpha(mix(VIOLET, CYAN, 0.35), 0.34), 2.5);
        let e3 = Rect::new(card.left - 26.0, card.top - 26.0, card.right + 26.0, card.bottom + 26.0);
        inner.stroke_rrect(e3, 46.0, alpha(VIOLET, 0.16), 2.0);
        // Accent light pooling at two corners, as if the nebula touches there.
        inner.circle(
            Offset::new(card.left + 24.0, card.bottom - 18.0),
            110.0,
            Gradient::radial_fill().with_stops(&[
                (0.0, alpha(CYAN_SOFT, 0.22)),
                (1.0, alpha(CYAN, 0.0)),
            ]),
        );
        inner.circle(
            Offset::new(card.right - 30.0, card.top + 22.0),
            130.0,
            Gradient::radial_fill().with_stops(&[
                (0.0, alpha(MAGENTA, 0.18)),
                (1.0, alpha(MAGENTA, 0.0)),
            ]),
        );
    });
    let _ = canvas;
}

/// The vignette — a full-surface radial darkening at the corners.
fn vignette(book: &mut Sketchbook, canvas: Size) {
    book.rect(
        Rect::new(0.0, 0.0, canvas.width, canvas.height),
        Gradient::radial(Offset::new(0.5, 0.5), 0.72)
            .with_dither()
            .with_stops(&[
                (0.62, alpha(Color::BLACK, 0.0)),
                (1.0, alpha(Color::BLACK, 0.55)),
            ]),
    );
}

/// The frame: nebula → glow → glass card → rim → sweep → vignette.
///
/// `FILM_BISECT` (env) drops named layers for debugging the composition:
/// "noglow", "noglass", "norim", "nosweep" — comma-separated.
pub fn frame(t: f32) -> WidgetNode {
    let float = (t * std::f32::consts::TAU).sin() * 9.0;
    let card_y = CANVAS_H * 0.5 - 150.0 + float;
    let appear = ease_in_out(clamp01(t * 2.4));
    // A light sweep crossing the glass — the card is alive, not a sticker.
    let sweep = t * 1.4 - 0.2;

    let bisect = std::env::var("FILM_BISECT").unwrap_or_default();
    let off = |name: &str| bisect.split(',').any(|s| s.trim() == name);

    // MINIMAL REPRO: one dark Painting behind + the proven glass card.
    // If the card renders white here, the seam is Painting→backdrop-filter.
    // SWATCH: the premul forensics. A row of solid fills over a dark
    // painting, alpha descending: white 255/200/100/40/10, magenta 40,
    // radial white-40. Premul-correct vs un-premul diverge measurably.
    if off("swatch") {
        let bg = Painting::sized(
            CANVAS,
            PaintWith::new(|book: &mut Sketchbook, size: Size| {
                book.rect(
                    Rect::new(0.0, 0.0, size.width, size.height),
                    Gradient::radial_fill().with_stops(&[
                        (0.0, Color::rgb(60, 30, 120)),
                        (1.0, Color::rgb(8, 6, 14)),
                    ]),
                );
                let y = 300.0;
                let sw = 130.0;
                let xs = 40.0;
                // white at descending alpha
                for (i, a) in [1.0, 0.784, 0.392, 0.157, 0.039].iter().enumerate() {
                    book.rect(
                        Rect::new(xs + i as f32 * (sw + 20.0), y, xs + i as f32 * (sw + 20.0) + sw, y + 140.0),
                        alpha(Color::WHITE, *a),
                    );
                }
                // magenta 40
                book.rect(
                    Rect::new(xs + 5.0 * (sw + 20.0), y, xs + 5.0 * (sw + 20.0) + sw, y + 140.0),
                    alpha(MAGENTA, 0.157),
                );
                // radial white-40
                book.circle(
                    Offset::new(xs + 6.0 * (sw + 20.0) + sw / 2.0, y + 70.0),
                    sw / 2.0,
                    Gradient::radial_fill().with_stops(&[
                        (0.0, alpha(Color::WHITE, 0.157)),
                        (1.0, alpha(Color::WHITE, 0.157)),
                    ]),
                );
            }),
        );
        return Stack::new()
            .push(Positioned::fill().child(bg))
            .into();
    }

    // MICRO2: the same translucency, three ways, over a dark painting:
    // (1) widget Container color; (2) Painting solid Color fill;
    // (3) Painting gradient fill with alpha'd stops. Which goes white?
    if off("micro2") {
        let bg = Painting::sized(
            CANVAS,
            PaintWith::new(|book: &mut Sketchbook, size: Size| {
                book.rect(
                    Rect::new(0.0, 0.0, size.width, size.height),
                    Gradient::radial_fill().with_stops(&[
                        (0.0, Color::rgb(60, 30, 120)),
                        (1.0, Color::rgb(8, 6, 14)),
                    ]),
                );
                // (2) solid translucent white via the painting API
                book.rect(Rect::new(60.0, 260.0, 260.0, 460.0), alpha(Color::WHITE, 0.157));
                // (3) gradient translucent white
                book.rect(
                    Rect::new(360.0, 260.0, 560.0, 460.0),
                    Gradient::horizontal().with_stops(&[
                        (0.0, alpha(Color::WHITE, 0.157)),
                        (1.0, alpha(Color::WHITE, 0.157)),
                    ]),
                );
            }),
        );
        let widget_card = Positioned::new()
            .left(660.0)
            .top(260.0)
            .child(
                Container::new()
                    .size(200.0, 200.0)
                    .color(alpha(Color::WHITE, 0.157)),
            );
        return Stack::new()
            .push(Positioned::fill().child(bg))
            .push(widget_card)
            .into();
    }

    // MICRO: four cards over a dark widget background —
    // (a) empty, alpha 10; (b) empty, alpha 120; (c) with child, alpha 10;
    // (d) empty, alpha 10, no radius, no border. Where does white appear?
    if off("micro") {
        let bg = Container::new()
            .size(CANVAS_W, CANVAS_H)
            .gradient(
                Gradient::radial_fill().with_stops(&[
                    (0.0, Color::rgb(60, 30, 120)),
                    (1.0, Color::rgb(8, 6, 14)),
                ]),
            );
        let card = |x: f32| {
            Positioned::new()
                .left(x)
                .top(260.0)
                .child(
                    Container::new()
                        .size(200.0, 200.0)
                        .color(alpha(Color::WHITE, 0.039))
                        .radius(22.0)
                        .border(vieww_foundation::Border::new(alpha(Color::WHITE, 0.282), 1.0)),
                )
        };
        let card_child = |x: f32| {
            Positioned::new()
                .left(x)
                .top(260.0)
                .child(
                    Container::new()
                        .size(200.0, 200.0)
                        .color(alpha(Color::WHITE, 0.039))
                        .radius(22.0)
                        .border(vieww_foundation::Border::new(alpha(Color::WHITE, 0.282), 1.0))
                        .child(SizedBox::from_size(Size::new(100.0, 100.0))),
                )
        };
        let card_plain = |x: f32| {
            Positioned::new()
                .left(x)
                .top(260.0)
                .child(
                    Container::new()
                        .size(200.0, 200.0)
                        .color(alpha(Color::WHITE, 0.039)),
                )
        };
        return Stack::new()
            .push(Positioned::fill().child(bg))
            .push(card(60.0))
            .push(card(360.0))
            .push(card_child(660.0))
            .push(card_plain(960.0))
            .into();
    }

    if off("mini") {
        // mini_widbg: same card, but the background is a WIDGET (Container
        // gradient) instead of a custom Painting — the control group.
        let mini_bg: WidgetNode = if off("mini_widbg") {
            Container::new()
                .size(CANVAS_W, CANVAS_H)
                .gradient(
                    Gradient::radial_fill().with_stops(&[
                        (0.0, Color::rgb(60, 30, 120)),
                        (1.0, Color::rgb(8, 6, 14)),
                    ]),
                )
                .into()
        } else {
            Painting::sized(
                CANVAS,
                PaintWith::new(|book: &mut Sketchbook, size: Size| {
                    book.rect(
                        Rect::new(0.0, 0.0, size.width, size.height),
                        Gradient::radial_fill().with_stops(&[
                            (0.0, Color::rgb(60, 30, 120)),
                            (1.0, Color::rgb(8, 6, 14)),
                        ]),
                    );
                }),
            )
            .into()
        };
        let card = Container::new()
            .size(640.0, 300.0)
            .color(alpha(Color::WHITE, 0.039))
            .radius(22.0)
            .border(vieww_foundation::Border::new(alpha(Color::WHITE, 0.282), 1.0));
        // Stage split:
        //   mini          — backdrop + blur + tint (the white case)
        //   mini_nofilt   — no Filtered at all (control)
        //   mini_plain    — filtered, NOT backdrop (plain group blur)
        //   mini_noblur   — backdrop + tint, no blur
        let glass: WidgetNode = if off("mini_nofilt") {
            card.into()
        } else if off("mini_plain") {
            Filtered::blur(9.0)
                .tint(Color::rgb(55, 76, 112), 0.30)
                .child(card)
                .into()
        } else if off("mini_noblur") {
            Filtered::new()
                .with_backdrop()
                .tint(Color::rgb(55, 76, 112), 0.30)
                .child(card)
                .into()
        } else {
            Filtered::blur(9.0)
                .with_backdrop()
                .tint(Color::rgb(55, 76, 112), 0.30)
                .child(card)
                .into()
        };
        return Stack::new()
            .push(Positioned::fill().child(mini_bg))
            .push(
                Positioned::new()
                    .left(CANVAS_W * 0.5 - 320.0)
                    .top(CANVAS_H * 0.5 - 150.0)
                    .child(glass),
            )
            .into();
    }
    // (mini branch ends)

    let mut stack = Stack::new();

    // 1 · the nebula (always on)
    stack = stack.push(
        Positioned::fill().child(
            Painting::sized(
                CANVAS,
                PaintWith::new(move |book: &mut Sketchbook, size: Size| {
                    nebula(book, size, t);
                }),
            ),
        ),
    );

    // 2 · the bloom behind the word
    if !off("noglow") {
        stack = stack.push(
            Positioned::fill().child(
                Painting::sized(
                    CANVAS,
                    PaintWith::new(move |book: &mut Sketchbook, size: Size| {
                        glow(book, size, t);
                    }),
                )
            ),
        );
    }

    // 3 · the glass card — frosted, tinted, shadowed, bordered
    if !off("noglass") {
        // Micro-bisect modes:
        //   plasticard — the PROVEN test-premium-ui pattern (plain color, no shadow)
        //   noshadow   — gradient but no shadow
        //   nograd     — plain color + shadow, no gradient
        let plastic = off("plasticard");
        let noshadow = off("noshadow") || plastic;
        let nograd = off("nograd") || plastic;

        let body = Container::new()
            .size(640.0, 300.0)
            .radius(22.0);
        let body = if nograd {
            body.color(alpha(Color::WHITE, 0.039))
        } else {
            body.gradient(
                Gradient::vertical()
                    .with_dither()
                    .with_stops(&[
                        (0.0, alpha(mix(Color::WHITE, VIOLET_SOFT, 0.25), 0.102)),
                        (0.55, alpha(Color::WHITE, 0.039)),
                        (1.0, alpha(mix(Color::WHITE, CYAN_SOFT, 0.3), 0.063)),
                    ]),
            )
        };
        let body = if noshadow {
            body
        } else {
            body.shadow(
                vieww_foundation::Shadow::new(
                    alpha(mix(VIOLET_DEEP, Color::BLACK, 0.55), 0.529),
                    Offset::new(0.0, 26.0),
                    56.0,
                )
                .spread(6.0),
            )
        };
        let body = body
            .border(vieww_foundation::Border::new(alpha(Color::WHITE, 0.282), 1.0))
            .child(
                Container::new()
                    .padding(vieww_foundation::EdgeInsets::all(30.0))
                    .child(
                        Opacity::new(appear).child(
                            Flex::column()
                                .cross_axis_alignment(CrossAxisAlignment::Start)
                                .spacing(14.0)
                                .children(children![
                                    tags_row(),
                                    Text::new("Light.")
                                        .color(INK)
                                        .size(120.0)
                                        .bold(),
                                    Text::new(
                                        "the material beneath every surface \
                                         in this film — rendered, not faked",
                                    )
                                    .color(MUTED)
                                    .size(15.0),
                                ]),
                        ),
                    ),
            );

        stack = stack.push(
            Positioned::new()
                .left(CANVAS_W * 0.5 - 320.0)
                .top(card_y)
                .child(
                    Filtered::blur(9.0)
                        .with_backdrop()
                        .tint(Color::rgb(55, 76, 112), 0.30)
                        .child(body),
                ),
        );
    }

    // 4 · the rim — light escaping the card's edges, above the glass
    if !off("norim") {
        stack = stack.push(
            Positioned::fill().child(
                Painting::sized(
                    CANVAS,
                    PaintWith::new(move |book: &mut Sketchbook, size: Size| {
                        let card = Rect::new(
                            CANVAS_W * 0.5 - 320.0,
                            card_y,
                            CANVAS_W * 0.5 + 320.0,
                            card_y + 300.0,
                        );
                        rim(book, size, t, card);
                    }),
                )
            ),
        );
    }

    // 5 · the sweep — a reflection crossing the glass
    if !off("nosweep") {
        stack = stack.push(
            Positioned::new()
                .left(CANVAS_W * 0.5 - 320.0)
                .top(card_y)
                .child(
                    Clip::rounded(22.0).child(
                        Painting::sized(
                            Size::new(640.0, 300.0),
                            PaintWith::new(move |book: &mut Sketchbook, size: Size| {
                                if (0.0..1.0).contains(&sweep) {
                                    let x = sweep * (size.width + 260.0) - 130.0;
                                    book.layer(1.0, 0.0, None, |inner| {
                                        let mut p = vieww_foundation::Path::new();
                                        p.move_to(Offset::new(x - 130.0, size.height));
                                        p.line_to(Offset::new(x + 60.0, 0.0));
                                        p.line_to(Offset::new(x + 130.0, 0.0));
                                        p.line_to(Offset::new(x - 60.0, size.height));
                                        p.close();
                                        inner.fill(
                                            p,
                                            Gradient::horizontal().with_stops(&[
                                                (0.0, alpha(Color::WHITE, 0.0)),
                                                (0.5, alpha(Color::WHITE, 0.118)),
                                                (1.0, alpha(Color::WHITE, 0.0)),
                                            ]),
                                        );
                                    });
                                }
                            }),
                        ),
                    ),
                ),
        );
    }

    // 6 · the vignette
    stack = stack.push(
        Positioned::fill().child(
            Painting::sized(
                CANVAS,
                PaintWith::new(move |book: &mut Sketchbook, size: Size| {
                    vignette(book, size);
                }),
            ),
        ),
    );

    stack.into()
}

fn tags_row() -> WidgetNode {
    Flex::row()
        .spacing(10.0)
        .children(children![
            tag("material"),
            tag("curved"),
            tag("blurred"),
        ])
        .into()
}

fn tag(label: &str) -> WidgetNode {
    Text::new(label)
        .color(MUTED)
        .size(11.0)
        .bold()
        .into()
}

/// The path import keeps rustc honest about unused items in some builds.
#[allow(dead_code)]
fn _path_check(p: Path) -> Path {
    p
}
