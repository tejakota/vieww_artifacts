//! **S04 · THE MARK** — the title card, and Act I's twist.
//!
//! The answer to S03's question arrives as a name. **viewwstudio** drops in
//! on a spring, letter-group by letter-group, and settles over a lineage
//! rule that resolves into the film's second line: *built on vieww*. Both
//! names appear together in the film's first title, because the hero is the
//! studio and the studio is not alone.
//!
//! The craft on screen, all of it the framework's own:
//! - **A real spring.** The drop is `kit::spring` — an analytic underdamped
//!   settle, so the frame is a pure function of its own time and the bounce
//!   is a physical curve rather than an eased guess.
//! - **A reflection**, not a gradient pretending to be one: the same text
//!   node, flipped through a `Transformed`, blurred by `Filtered::blur`, and
//!   faded out by a ground ramp painted over it.
//! - **A specular band** that crosses the letters once, as a `Plus`-blended
//!   group — one layer for the whole sweep, because blur is priced per layer.
//! - **A convergence**: the particles that drifted through Act I fly in and
//!   are consumed by the mark, which is the visual form of the argument that
//!   the waiting becomes the working.
//!
//! The twist (K1): the last beat states plainly that the wait the film just
//! spent a minute on is not a law of nature. Then the studio opens.

use vieww_foundation::{BlendMode, Color, FontWeight, Gradient, Offset, Rect, Size, Sketchbook, Transform};
use vieww_widget::prelude::*;
use vieww_widget::{Filtered, Opacity, Transformed};

use crate::film::Ctx;
use crate::kit::{
    alpha, bump, caption, clamp01, dust, ease_out_cubic, ease_out_expo, grain, ground, horizon, mix,
    painter, seg, smoothstep, spring, sting, tint, vignette, xywh, Rng, Type, CYAN, CYAN_SOFT, H,
    INK, INK_SOFT, MUTED, VIOLET, VIOLET_SOFT, W,
};
use crate::studio::compose;

/// The mark, and the line beneath it.
const MARK: &str = "viewwstudio";
const UNDER: &str = "built on vieww";

/// Where the mark's baseline lands.
const MARK_Y: f32 = H * 0.30;
const MARK_SIZE: f32 = 132.0;

/// The beats, in scene-seconds.
const DROP_T: f32 = 0.55;
const UNDER_T: f32 = 2.05;
const RULE_T: f32 = 1.75;
const SPEC_T: f32 = 3.05;
const TWIST_T: f32 = 8.10;

pub fn frame(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let t = ctx.t;
    let frame_i = ctx.frame;

    // The drop: a spring settle, sampled analytically.
    let drop = spring(seg(sec, DROP_T, DROP_T + 1.5), 13.0, 0.36);
    let mark_a = ease_out_cubic(seg(sec, DROP_T, DROP_T + 0.34));
    let dy = (1.0 - drop) * -150.0;

    let under_a = ease_out_cubic(seg(sec, UNDER_T, UNDER_T + 0.8));
    let rule = ease_out_expo(seg(sec, RULE_T, RULE_T + 1.2));
    let spec = seg(sec, SPEC_T, SPEC_T + 1.15);
    let twist = smoothstep(seg(sec, TWIST_T, TWIST_T + 1.1));
    let out = smoothstep(seg(t, 0.93, 1.0));

    // The convergence: Act I's dust flies into the mark as it lands.
    let converge = smoothstep(seg(sec, 0.0, 1.9));

    let bg = painter(move |book: &mut Sketchbook, size: Size| {
        let (w, h) = (size.width, size.height);
        ground(book, size, sec, 0.55 + 0.45 * smoothstep(seg(sec, 0.0, 3.0)));

        // The floor the mark stands on — a wide, low violet horizon that
        // brightens as the name settles.
        horizon(book, size, VIOLET, 0.55 + 0.45 * drop);

        // The impact: on the frame the spring first reaches the baseline, a
        // ring of light leaves the mark. Drawn from the spring's own
        // overshoot, so the flash and the bounce cannot disagree.
        let impact = bump(seg(sec, DROP_T + 0.30, DROP_T + 0.95), 0.0, 1.0);
        if impact > 0.01 {
            book.blended_layer(1.0, 30.0, BlendMode::Plus, None, |g| {
                g.rrect(
                    xywh(w * 0.5 - 560.0 * impact, MARK_Y + MARK_SIZE * 1.20, 1120.0 * impact, 4.0),
                    2.0,
                    Gradient::horizontal().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.0)),
                        (0.5, alpha(tint(VIOLET_SOFT, 0.4), 0.8 * impact)),
                        (1.0, alpha(CYAN, 0.0)),
                    ]),
                );
            });
        }

        // The lineage rule: violet at the studio's end, cyan at the
        // framework's. It is the same gradient that will run under the end
        // card, so the film's two names always share one line.
        if rule > 0.01 {
            let rw = 560.0 * rule;
            let rx = w * 0.5 - rw * 0.5;
            let ry = MARK_Y + MARK_SIZE * 1.20;
            book.rrect(
                xywh(rx, ry, rw, 3.0),
                1.5,
                Gradient::horizontal().with_dither().with_stops(&[
                    (0.0, alpha(VIOLET, 0.0)),
                    (0.28, alpha(VIOLET, 0.95)),
                    (0.72, alpha(mix(VIOLET, CYAN, 0.6), 0.9)),
                    (1.0, alpha(CYAN, 0.0)),
                ]),
            );
        }

        // The specular band — one Plus group crossing the letters once. A
        // narrow bright parallelogram, clipped to the mark's band.
        if spec > 0.004 && spec < 1.0 {
            let x = -420.0 + (w + 840.0) * ease_out_cubic(spec);
            book.blended_layer(1.0, 26.0, BlendMode::Plus, None, |g| {
                let band = xywh(x, MARK_Y - 30.0, 180.0, MARK_SIZE * 1.5);
                g.rect(
                    band,
                    Gradient::horizontal().with_dither().with_stops(&[
                        (0.0, alpha(Color::WHITE, 0.0)),
                        (0.5, alpha(Color::WHITE, 0.17 * (1.0 - (spec - 0.5).abs() * 1.4))),
                        (1.0, alpha(Color::WHITE, 0.0)),
                    ]),
                );
            });
        }

        // The convergence field: particles from the corners drawn toward the
        // mark's centre, consumed on arrival. One additive group.
        if converge < 0.999 {
            let mut rng = Rng::new(0x00C0_1FE7_u64);
            book.blended_layer(1.0, 6.0, BlendMode::Plus, None, |g| {
                for _ in 0..120 {
                    let a0 = rng.f01() * std::f32::consts::TAU;
                    let r0 = 460.0 + rng.f01() * 720.0;
                    let lag = rng.f01() * 0.45;
                    let p = clamp01((converge - lag) / (1.0 - lag).max(0.05));
                    let e = ease_out_cubic(p);
                    let r = r0 * (1.0 - e);
                    let cx = w * 0.5 + a0.cos() * r;
                    let cy = MARK_Y + MARK_SIZE * 0.4 + a0.sin() * r * 0.52;
                    let fade = (1.0 - e).powf(0.6) * (0.35 + 0.65 * rng.f01());
                    g.circle(
                        Offset::new(cx, cy),
                        0.8 + 1.6 * (1.0 - e),
                        alpha(if rng.f01() > 0.7 { CYAN_SOFT } else { VIOLET_SOFT }, 0.5 * fade),
                    );
                }
            });
        }

        // The reflection's ground: a ramp painted over the flipped text, so
        // the mirror fades into the floor instead of ending at an edge.
        let refl_top = MARK_Y + MARK_SIZE * 1.62;
        book.rect(
            xywh(0.0, refl_top, w, h - refl_top),
            Gradient::vertical().with_dither().with_stops(&[
                (0.0, alpha(Color::rgb(6, 7, 11), 0.10)),
                (0.45, alpha(Color::rgb(6, 7, 11), 0.86)),
                (1.0, alpha(Color::rgb(5, 6, 10), 1.0)),
            ]),
        );

        // The twist's own light: as the last line lands, the frame lifts
        // once — the film's first genuinely bright moment.
        let st = sting(sec - TWIST_T, 1.5);
        if st > 0.01 {
            book.rect(Rect::new(0.0, 0.0, w, h), alpha(tint(VIOLET, 0.55), 0.05 * st));
        }

        dust(book, size, sec, 44, VIOLET_SOFT, 0.7);
        grain(book, size, frame_i, 0.014, 340);
        vignette(book, size, 1.0);
        if out > 0.004 {
            book.rect(Rect::new(0.0, 0.0, w, h), alpha(Color::BLACK, out));
        }
    });

    // The mark itself, and its reflection. The reflection is the *same*
    // node, flipped and blurred by the framework — not a second drawing.
    let mark_node = |a: f32, c: Color, size: f32, track: f32| -> WidgetNode {
        Type::new(MARK)
            .size(size)
            .weight(FontWeight::Medium)
            .track(track)
            .leading(1.0)
            .color(alpha(c, a))
            .center()
            .banner(0.0)
            .width(W)
            .into()
    };

    let mut nodes: Vec<WidgetNode> = Vec::new();

    // The bloom behind the letters: the same text, blurred, additive. This
    // is why the mark glows without a glow asset.
    if mark_a > 0.02 {
        nodes.push(
            Stack::new()
                .push(
                    Positioned::new()
                        .left(0.0)
                        .top(MARK_Y + dy)
                        .width(W)
                        .height(MARK_SIZE * 1.6)
                        .child(
                            Opacity::new(0.42 * mark_a * (1.0 - out))
                                .blend(BlendMode::Plus)
                                .child(
                                    Filtered::blur(16.0)
                                        .child(mark_node(1.0, VIOLET_SOFT, MARK_SIZE, 1.0)),
                                ),
                        ),
                )
                .into(),
        );
    }

    // The reflection.
    if drop > 0.35 {
        let refl_a = 0.115 * smoothstep(seg(sec, DROP_T + 0.5, DROP_T + 1.8)) * (1.0 - out);
        nodes.push(
            Stack::new()
                .push(
                    Positioned::new()
                        .left(0.0)
                        .top(MARK_Y + (MARK_SIZE + 8.0) * 2.0 + 26.0)
                        .width(W)
                        .height(MARK_SIZE * 1.6)
                        .child(
                            Opacity::new(refl_a).child(
                                Transformed::new(Transform::scale(1.0, -1.0)).child(
                                    Filtered::blur(4.6).child(mark_node(1.0, INK_SOFT, MARK_SIZE, 1.0)),
                                ),
                            ),
                        ),
                )
                .into(),
        );
    }

    // The mark.
    nodes.push(
        Stack::new()
            .push(
                Positioned::new()
                    .left(0.0)
                    .top(MARK_Y + dy)
                    .width(W)
                    .height(MARK_SIZE * 1.6)
                    .child(mark_node(
                        0.99 * mark_a * (1.0 - out),
                        mix(INK, tint(VIOLET_SOFT, 0.3), 0.10 + 0.18 * bump(spec, 0.0, 1.0)),
                        MARK_SIZE,
                        1.0,
                    )),
            )
            .into(),
    );

    // *built on vieww* — the second name, in the framework's own colour.
    nodes.push(
        Type::new(UNDER)
            .mono()
            .size(21.0)
            .track(7.0)
            .color(alpha(CYAN, 0.9 * under_a * (1.0 - out)))
            .center()
            .banner(MARK_Y + MARK_SIZE * 1.34)
            .into(),
    );

    // The twist, stated: the wait was a choice, and the choice has been
    // unmade. One line, and it is the last thing Act I says.
    nodes.push(
        Type::new("the wait was never the work")
            .size(40.0)
            .light()
            .track(0.8)
            .color(alpha(INK, 0.95 * twist * (1.0 - out)))
            .center()
            .banner(H * 0.745)
            .into(),
    );
    nodes.push(
        Type::new("so we stopped paying for it")
            .mono()
            .size(16.0)
            .track(3.4)
            .color(alpha(VIOLET_SOFT, 0.85 * smoothstep(seg(sec, TWIST_T + 0.9, TWIST_T + 2.0)) * (1.0 - out)))
            .center()
            .banner(H * 0.808)
            .into(),
    );

    nodes.push(caption(
        "the release · viewwstudio",
        smoothstep(seg(sec, 2.6, 3.6)) * (1.0 - smoothstep(seg(sec, 7.2, 8.0))) * 0.9,
    ));
    let _ = MUTED;
    compose(bg, nodes)
}
