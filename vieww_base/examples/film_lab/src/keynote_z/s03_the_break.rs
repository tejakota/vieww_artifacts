//! S03 · THE BREAK — the stall hardens into a fracture, and the first
//! light gets in. 0:19–0:26.
//!
//! The terminal at 99%, frozen. The scanlines crawl harder. Then the
//! screen cracks: a hairline of light runs down the frame, the panel
//! shears into drifting tiles (the old world's UI, come apart at its
//! seams), and the question returns — typed fast, in the film's own
//! voice now, not the terminal's: *what if the wait was the work?*
//!
//! The crack is the spark's birth canal: S04 opens at the moment this
//! line of light becomes a point of it.

use vieww_foundation::{Color, Gradient, Offset, Path, Rect, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, Painting, PaintWith};

use crate::film_lib::{Rng, ease_out_cubic};
use super::{AMBER, CANVAS, Ctx, H, INK, MUTED, RED, W, alpha, caption, clamp01, flash, glow, grain, ground, light_rays, mix, mono_w, scanbands, tint, vignette, xywh};


/// The question, again — this time the film asks it.
const QUESTION: &str = "what if the wait was the work?";

/// The terminal's frozen geometry (carried from S02).
const PX: f32 = 260.0;
const PY: f32 = 170.0;
const PW: f32 = 1400.0;
const PH: f32 = 660.0;
/// The shatter grid.
const COLS: usize = 9;
const ROWS: usize = 5;

pub fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let frame_i = (ctx.abs * 60.0) as u64;

    // The break's phases: the freeze (0–0.22) · the crack (0.22–0.5) ·
    // the shear (0.5–0.78) · the flood (0.78–1).
    let freeze = clamp01(t / 0.22);
    let crack = clamp01((t - 0.20) / 0.26);
    let shear = clamp01((t - 0.48) / 0.28);
    let flood = clamp01((t - 0.76) / 0.24);

    // Desaturation — the old world's color draining as it breaks.
    let desat = 0.35 + 0.65 * (1.0 - crack);

    let mut stack = Stack::new();

    // The ground — colder now, flattening toward gray as the color drains.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            let gray = |v: u8, a: f32| alpha(Color::rgb(v, v, v), a);
            book.rect(
                Rect::new(0.0, 0.0, w, h),
                Gradient::vertical().with_dither().with_stops(&[
                    (0.0, mix(Color::rgb(11, 11, 12), Color::rgb(12, 12, 13), desat)),
                    (0.7, mix(Color::rgb(9, 10, 10), Color::rgb(11, 11, 12), desat)),
                    (1.0, mix(Color::rgb(13, 11, 11), Color::rgb(12, 12, 13), desat)),
                ]),
            );
            // The scanlines — crawling harder as the break comes.
            super::scanbands(book, w, h, t, 0.5 + crack * 0.4, Color::rgb(63, 185, 80));
            super::vignette(book, w, h, 0.6);
            super::grain(book, w, h, frame_i, 0.7);
            let _ = gray;
        }),
    )));

    // The terminal, as tiles. Each tile shears away from the crack with
    // distance-proportional speed and a per-tile jitter — the old UI
    // coming apart along its own grid, which is the honest way for a UI
    // to break (the renderer's grid is the only seam it has).
    let tw = PW / COLS as f32;
    let th = PH / ROWS as f32;
    let tile_a = freeze * (1.0 - shear * 0.9);
    if tile_a > 0.02 {
        let tiles = Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                let mut rng = Rng::new(0x9A03);
                for row in 0..ROWS {
                    for col in 0..COLS {
                        let cx = PX + (col as f32 + 0.5) * tw;
                        let cy = PY + (row as f32 + 0.5) * th;
                        // The shear: tiles flee the crack line (center col).
                        let side = if col as f32 + 0.5 < COLS as f32 * 0.5 { -1.0 } else { 1.0 };
                        let dist = ((col as f32 + 0.5) - COLS as f32 * 0.5).abs() / (COLS as f32 * 0.5);
                        let jx = rng.sym() * 30.0;
                        let jy = rng.sym() * 22.0 + shear * 40.0;
                        let dx = side * dist * shear * (140.0 + 160.0 * rng.f01()) + jx * shear;
                        let dy = jy * shear + rng.sym() * 4.0;
                        // Dim terminal green body, going gray with desat.
                        let body = mix(Color::rgb(13, 16, 14), Color::rgb(14, 14, 15), desat);
                        book.rrect(
                            xywh(cx - tw * 0.5 + dx, cy - th * 0.5 + dy, tw - 3.0, th - 3.0),
                            5.0,
                            alpha(body, tile_a),
                        );
                        // A dying phosphor edge on the tiles nearest the crack.
                        if dist < 0.35 {
                            book.stroke_rrect(
                                xywh(cx - tw * 0.5 + dx, cy - th * 0.5 + dy, tw - 3.0, th - 3.0),
                                5.0,
                                alpha(mix(AMBER, Color::rgb(120, 120, 120), desat), tile_a * 0.5),
                                1.0,
                            );
                        }
                    }
                }
            }),
        );
        stack = stack.push(Positioned::fill().child(tiles));
    }

    // The frozen artifacts, fading with the shear — the stalled bar and
    // the trembling spinner, ghosts of S02's last frame.
    if freeze > 0.5 && shear < 0.9 {
        let ghost = (1.0 - shear).max(0.0);
        let bar = Painting::sized(
            Size::new(1300.0, 14.0),
            PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                book.stroke_rrect(xywh(0.0, 0.0, 1300.0, 12.0), 6.0, alpha(Color::WHITE, 0.10 * ghost), 1.0);
                book.rrect(xywh(0.0, 0.0, 1287.0, 12.0), 6.0, alpha(mix(RED, Color::rgb(150, 90, 90), desat * 0.6), ghost));
            }),
        );
        stack = stack.push(Positioned::new().left(300.0).top(760.0).width(1300.0).height(14.0).child(bar));
        stack = stack.push(
            Positioned::new().left(300.0).top(790.0).width(900.0).height(24.0)
                .child(Opacity::new(ghost).child(
                    Text::new("41.3 MB · 99% · compiling the world")
                        .style(TextStyle::new(18.0).monospace().letter_spacing(1.0).color(alpha(tint(RED, 0.25), 0.9))),
                )),
        );
    }

    // THE CRACK — a jagged hairline of light running down the frame,
    // widening with the shear and flooding at the end. Drawn as a path
    // of light with its own glow: the first frame of the film's
    // protagonist.
    if crack > 0.01 {
        let crack_w = (0.8 + crack * 2.6 + shear * 4.0 + flood * 8.0).min(16.0);
        let crack_a = crack * (1.0 - flood * 0.35);
        let light = Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                let h = s.height;
                // The crack's spine — deterministic jitter, top to bottom.
                let mut path = Path::new();
                let steps = 14;
                let mut rng = Rng::new(0xC4ACC);
                let mut pts: Vec<Offset> = Vec::with_capacity(steps + 1);
                for i in 0..=steps {
                    let y = h * (i as f32 / steps as f32).clamp(0.04, 0.96);
                    let x = W * 0.5 + rng.sym() * (16.0 + crack * 30.0);
                    pts.push(Offset::new(x, y));
                }
                path.move_to(pts[0]);
                for pt in pts.iter().skip(1) {
                    path.line_to(*pt);
                }
                // The glow — the light pouring through.
                super::glow(book, W * 0.5, h * 0.5, 300.0 + flood * 500.0, super::SPARK_C, 0.10 * crack + 0.20 * flood);
                // The body — hot core over warm edge.
                book.stroke_styled(
                    path.clone(),
                    alpha(tint(super::SPARK_C, 0.55), crack_a),
                    crack_w + 3.0,
                    vieww_foundation::StrokeStyle::rounded(),
                );
                book.stroke_styled(
                    path,
                    alpha(Color::WHITE, crack_a * 0.95),
                    crack_w * 0.5,
                    vieww_foundation::StrokeStyle::rounded(),
                );
                // God rays through the widening seam.
                if flood > 0.05 {
                    super::light_rays(book, W * 0.5, h * 0.5, 20.0, 1100.0, 0.0, t * 0.2, flood * 0.5, super::SPARK_C);
                }
            }),
        );
        stack = stack.push(Positioned::fill().child(light));
    }

    // The question — typed fast, the film's voice, INK on the dark.
    let type_p = clamp01((t - 0.52) / 0.30);
    let typed_n = (QUESTION.chars().count() as f32 * ease_out_cubic(type_p)).round() as usize;
    let shown: String = QUESTION.chars().take(typed_n).collect();
    if typed_n > 0 {
        let size = 30.0;
        let x0 = W * 0.5 - mono_w(size, QUESTION.chars().count()) * 0.5;
        stack = stack.push(
            Positioned::new()
                .left(x0)
                .top(596.0)
                .width(mono_w(size, QUESTION.chars().count()) + 40.0)
                .height(40.0)
                .child(
                    Text::new(shown)
                        .style(TextStyle::new(size).monospace().letter_spacing(2.0).color(alpha(INK, 0.95)))
                        .align(TextAlign::Left),
                ),
        );
        let q_on = (sec * 3.0).fract() < 0.55;
        if q_on && typed_n < QUESTION.chars().count() {
            stack = stack.push(
                Positioned::new()
                    .left(x0 + mono_w(size, typed_n) + 4.0)
                    .top(602.0)
                    .width(13.0)
                    .height(30.0)
                    .child(Container::new().color(alpha(super::SPARK_C, 0.9)).radius(1.5)),
            );
        }
    }

    // The flood — the last breath of the scene: light fills the frame and
    // hands S04 its first frame.
    if flood > 0.3 {
        let fl = (flood - 0.3) / 0.7;
        stack = stack.push(Positioned::fill().child(Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                super::flash(book, s.width, s.height, fl * fl, super::SPARK_C);
            }),
        )));
    }

    // The captions — the break's two beats.
    stack = stack.push(caption(
        "99% — and there it stays",
        1002.0,
        clamp01((t - 0.06) / 0.10),
    ));
    if crack > 0.3 {
        stack = stack.push(caption(
            "and the first light gets in",
            964.0,
            clamp01((t - 0.36) / 0.12),
        ));
    }
    let _ = (MUTED, H, ease_out_cubic(t));

    stack.into()
}
