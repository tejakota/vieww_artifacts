//! S03 · RUST LOCKOUT — the pivot. 0:20–0:28.
//!
//! "Rust has the power to fix this" — and the scene agrees: a gear, the
//! community's own sigil, assembles itself from stroke paths and starts to
//! turn, beautiful and fast. Then the old world answers with four iron
//! bars: setup cost, compile overhead, no preview layer, the toolchain
//! maze. The cage closes, the light dims, the gear grinds to a halt. The
//! scene ends almost dark — except one violet spark that refuses to go out
//! (S04's first frame).

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{alpha, clamp01, ease_out_cubic, mix, spring_out, tint, xywh, FAINT, INK, MUTED, RED, AMBER, Rng, VIOLET, VIOLET_SOFT};

use super::{Ctx};

/// The gear's center.
const CX: f32 = 960.0;
const CY: f32 = 520.0;
/// The gear's outer radius.
const R_OUT: f32 = 180.0;

/// The gear as a path — a cog with 12 teeth, one closed outline.
fn gear_path(cx: f32, cy: f32, r_out: f32, teeth: usize) -> vieww_foundation::Path {
    let mut p = vieww_foundation::Path::new();
    let r_in = r_out * 0.82;
    let steps = teeth * 4;
    for i in 0..=steps {
        let seg = i % 4; // 0,1 = outer (tooth), 2,3 = inner (gap)
        let r = if seg < 2 { r_out } else { r_in };
        let a = i as f32 / steps as f32 * std::f32::consts::TAU;
        let pt = Offset::new(cx + a.cos() * r, cy + a.sin() * r);
        if i == 0 {
            p.move_to(pt);
        } else {
            p.line_to(pt);
        }
    }
    p.close();
    p
}

/// The four bars — the lockout, in the reference's own words.
const BARS: [(&str, f32); 4] = [
    ("setup cost", 0.30),
    ("compile overhead", 0.40),
    ("no preview layer", 0.50),
    ("the toolchain maze", 0.60),
];

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;

    let mut stack = Stack::new();

    // The ground.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            super::ground(book, w, h);
            super::stars(book, w, h, 0x3A17, 60, t, 0.07);
            super::vignette(book, w, h, 0.55);
        }),
    )));

    // The gear's assembly: stroke-on self-drawing (dash progress), then it
    // spins. Its spin *decays* as the bars land — the cage slowing it down.
    let draw_p = ease_out_cubic(clamp01(t / 0.22));
    // The angular position: fast until the first bar, decaying after.
    let decay_start = 0.30;
    let spin_t = if t < decay_start {
        sec * 1.4
    } else {
        let over = (t - decay_start) / (1.0 - decay_start);
        1.4 * (sec.min(decay_start * 8.0)) + (1.0 - ease_out_cubic(over)) * (t - decay_start) * 8.0 * 1.4
    };
    let dim = 1.0 - 0.72 * ease_out_cubic(clamp01((t - 0.34) / 0.30));
    let gear_col = mix(alpha(VIOLET_SOFT, 0.95), alpha(mix(VIOLET, Color::BLACK, 0.5), 0.9), 1.0 - dim);

    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            // The gear — a closed path, rotated by Transform.
            let gear = gear_path(0.0, 0.0, R_OUT, 12);
            let tr = vieww_foundation::Transform::rotate(spin_t)
                .then(vieww_foundation::Transform::translate(Offset::new(CX, CY)));
            book.transformed(tr, |g| {
                // The self-drawing outline — one dash of the whole length.
                g.stroke_styled(
                    gear.clone(),
                    gear_col,
                    5.0,
                    vieww_foundation::StrokeStyle::rounded().dash(vieww_foundation::Dash::new(vec![
                        2600.0 * draw_p,
                        2600.0,
                    ])),
                );
                // The hub — a smaller cog inside, counter-rotating.
                let hub = gear_path(0.0, 0.0, R_OUT * 0.45, 8);
                g.stroke_styled(
                    hub,
                    alpha(gear_col_color(), 0.75 * draw_p),
                    3.0,
                    vieww_foundation::StrokeStyle::rounded(),
                );
                g.circle(Offset::new(0.0, 0.0), 16.0, alpha(gear_col_color(), 0.9 * draw_p));
            });
            // The glow the gear casts while it still turns.
            let glow_a = dim * (0.16 + 0.05 * (sec * 3.0).sin());
            if glow_a > 0.01 {
                super::glow(book, CX, CY, 360.0, VIOLET, glow_a);
            }
            // The spark — refuses the dark (S04's fuse). Ignites as the last
            // bar lands and the gear stalls.
        }),
    )));

    // The bars — vertical iron slabs slamming around the gear.
    for (i, (label, at)) in BARS.iter().enumerate() {
        let slam = spring_out(clamp01((t - at) / 0.16), 14.0, 0.62);
        if slam <= 0.0 {
            continue;
        }
        // Four bars around the gear: left, right, top, bottom — a cage.
        let (x, y, w, h) = match i {
            0 => (CX - 460.0, CY - 300.0, 120.0 * slam.max(0.05), 600.0),
            1 => (CX + 340.0 + 120.0 * (1.0 - slam), CY - 300.0, 120.0 * slam.max(0.05), 600.0),
            2 => (CX - 460.0, CY - 420.0, 940.0, 110.0 * slam.max(0.05)),
            _ => (CX - 460.0, CY + 310.0 + 110.0 * (1.0 - slam), 940.0, 110.0 * slam.max(0.05)),
        };
        let label = *label;
        let i = i as f32;
        stack = stack.push(
            Positioned::new()
                .left(x)
                .top(y)
                .width(w.max(1.0))
                .height(h.max(1.0))
                .child(Painting::sized(
                    Size::new(w.max(1.0), h.max(1.0)),
                    PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                        book.rrect(xywh(0.0, 0.0, s.width, s.height), 10.0, alpha(Color::rgb(24, 24, 28), 0.97));
                        book.stroke_rrect(
                            xywh(0.0, 0.0, s.width, s.height),
                            10.0,
                            alpha(mix(RED, MUTED, 0.55), 0.5),
                            1.4,
                        );
                        // Rivets — two rows, the industrial grammar.
                        let (rx, ry, rw, rh) = (14.0, 14.0, (s.width - 28.0).max(1.0), (s.height - 28.0).max(1.0));
                        for k in 0..5 {
                            let fx = k as f32 / 4.0;
                            let fy = if k % 2 == 0 { 0.0 } else { 1.0 };
                            let px = rx + rw * (if s.width > s.height { fx } else { fy * 0.5 + 0.25 });
                            let py = ry + rh * (if s.width > s.height { fy * 0.5 + 0.25 } else { fx });
                            book.circle(Offset::new(px, py), 3.2, alpha(Color::WHITE, 0.10));
                        }
                        let _ = i;
                    }),
                )),
        );
        // The label — set into the bar.
        let (lx, ly) = match i as usize {
            0 => (x + 18.0, y + 40.0),
            1 => (x + 12.0, y + 40.0),
            2 => (x + 40.0, y + 34.0),
            _ => (x + 40.0, y + 30.0),
        };
        stack = stack.push(
            Positioned::new()
                .left(lx)
                .top(ly)
                .width(420.0)
                .height(28.0)
                .child(
                    Text::new(label)
                        .style(TextStyle::new(21.0).monospace().letter_spacing(2.4).color(alpha(tint(RED, 0.05), 0.9))),
                ),
        );
    }

    // The claim + the pivot caption.
    stack = stack.push(
        Positioned::new()
            .left(0.0)
            .top(150.0)
            .width(1920.0)
            .height(50.0)
            .child(
                Text::new("rust has the power to fix this")
                    .style(TextStyle::new(34.0).letter_spacing(2.0).color(alpha(INK, 0.95)))
                    .align(TextAlign::Center),
            ),
    );
    let pivot_a = clamp01((t - 0.66) / 0.14);
    if pivot_a > 0.0 {
        stack = stack.push(
            Positioned::new()
                .left(0.0)
                .top(200.0)
                .width(1920.0)
                .height(40.0)
                .child(Opacity::new(pivot_a).child(
                    Text::new("…but the old world locked the door")
                        .style(TextStyle::new(22.0).monospace().letter_spacing(2.6).color(alpha(MUTED, 0.9)))
                        .align(TextAlign::Center),
                )),
        );
    }

    // The spark — S04's fuse. Tiny, defiant, at the gear's hub.
    let spark_t = clamp01((t - 0.78) / 0.18);
    if spark_t > 0.0 {
        let breathe = 0.5 + 0.5 * (sec * 6.0).sin();
        let r = 3.0 + spark_t * 5.0 + breathe * 1.6;
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                super::glow(book, CX, CY, 120.0 + spark_t * 160.0, VIOLET, 0.30 + 0.22 * breathe);
                book.circle(Offset::new(CX, CY), r, alpha(tint(VIOLET_SOFT, 0.5), 1.0));
            }),
        )));
    }

    stack = stack.push(super::caption(
        "power was never the problem. the door was.",
        1000.0,
        clamp01((t - 0.70) / 0.12),
    ));

    stack.into()
}

/// The gear's color, split out so both closures agree.
fn gear_col_color() -> Color {
    VIOLET_SOFT
}
