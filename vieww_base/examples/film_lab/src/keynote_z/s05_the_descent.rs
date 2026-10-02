//! S05 · THE DESCENT — the camera dives into the spark, through the
//! machine's architecture. 0:37–0:45.
//!
//! The wordmark's world recedes and the film travels *down*: receding
//! grid planes rush past on a parallax dolly, each plane one layer of
//! the stack (widgets · element · render · paint), labeled as it
//! crosses the camera. The dive is continuous — no cuts, only depth —
//! and it lands on S06's crate grid, the architecture seen whole.
//!
//! The layer names are the workspace's own: the four planes the film
//! passes are exactly the four crates a frame travels through
//! (`vieww-widget` → `vieww-element` → `vieww-render` → `vieww-paint`).

use vieww_foundation::{Color, Offset, Size, Sketchbook, TextAlign, TextStyle};
use vieww_widget::prelude::*;
use vieww_widget::{Opacity, PaintWith, Painting};

use super::{
    alpha, caption, clamp01, tint, xywh, Ctx, CYAN, H, INK, MUTED, VIOLET, VIOLET_SOFT, W,
};
use crate::film_lib::{ease_in_out, ease_out_cubic};

/// The four planes the dive passes — the frame's own path through the
/// stack, named as it crosses each.
const PLANES: [&str; 4] = [
    "vieww-widget",
    "vieww-element",
    "vieww-render",
    "vieww-paint",
];

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let frame_i = (ctx.abs * 60.0) as u64;

    // The dive — continuous, accelerating slightly, then slowing into
    // the landing (S06's grid).
    let dive = ease_in_out(t);

    // The camera — the descent's velocity, in plane-depths per scene.
    // The parallax stars shear upward as we fall.
    let cam_y = dive * 420.0;

    let mut stack = Stack::new();

    // The shaft — the dark the camera falls through, stars rushing up.
    stack = stack.push(Positioned::fill().child(Painting::sized(
        super::CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            let w = s.width;
            let h = s.height;
            // A deeper ground than the sky's — we are inside the machine.
            book.rect(
                xywh(0.0, 0.0, w, h),
                vieww_foundation::Gradient::vertical()
                    .with_dither()
                    .with_stops(&[
                        (0.0, Color::rgb(8, 8, 12)),
                        (0.5, Color::rgb(6, 6, 10)),
                        (1.0, Color::rgb(9, 8, 14)),
                    ]),
            );
            // Stars, rushing upward — the fall made visible.
            super::stars_parallax(book, w, h, 0x05DE, 110, t, 0.14, 0.0, cam_y);
            super::vignette(book, w, h, 0.55);
            super::grain(book, w, h, frame_i, 0.4);
        }),
    )));

    // The receding planes — five grid planes at staggered depths, all
    // drifting toward the camera as the dive advances; each crosses the
    // lens and is replaced by the next. The vanishing point breathes.
    for (i, name) in PLANES.iter().enumerate() {
        // When this plane crosses the camera's depth — the label moment.
        let z_center = 0.12 + i as f32 * 0.24;
        let z = (z_center - dive * 1.1).rem_euclid(1.0);
        // The label's moment: the plane is near the camera.
        let near = 1.0 - (z - 0.15).abs().min(1.0);
        let label_a = clamp01((near - 0.55) / 0.45) * clamp01((0.98 - near) / 0.1) * 2.2;
        let planes = Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                super::grid_plane(book, s.width, s.height, z, dive, 0.85, CYAN);
                // The plane's edge glow — a horizon line, cyan, deep.
                let vpy = s.height * 0.42;
                let scale = 1.0 - z * 0.86;
                let gw = s.width * 1.8 * scale;
                let y = vpy + s.height * 1.4 * scale * 0.5;
                book.rect(
                    xywh(s.width * 0.5 - gw * 0.5, y, gw, 2.0),
                    alpha(tint(CYAN, 0.2), 0.10 + 0.25 * (1.0 - z)),
                );
            }),
        );
        stack = stack.push(Positioned::fill().child(planes));

        // The label — the crate name, riding the plane, blooming as the
        // plane crosses the lens and dimming as it passes behind.
        if label_a > 0.02 {
            let lx = W * 0.5 + (i as f32 - 1.5) * 320.0;
            let ly = 220.0 + i as f32 * 130.0;
            let label = Painting::sized(
                Size::new(620.0, 90.0),
                PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
                    // The tick — a bracket, the inspector's grammar.
                    book.line(
                        Offset::new(10.0, 12.0),
                        Offset::new(10.0, 74.0),
                        alpha(CYAN, label_a * 0.8),
                        2.4,
                    );
                    super::glow(book, 60.0, 45.0, 160.0, CYAN, label_a * 0.25);
                }),
            );
            stack = stack.push(
                Positioned::new()
                    .left(lx)
                    .top(ly)
                    .width(620.0)
                    .height(90.0)
                    .child(label),
            );
            stack = stack.push(
                Positioned::new()
                    .left(lx + 26.0)
                    .top(ly + 22.0)
                    .width(420.0)
                    .height(40.0)
                    .child(
                        Opacity::new(clamp01(label_a)).child(
                            Text::new(*name)
                                .style(
                                    TextStyle::new(26.0)
                                        .monospace()
                                        .letter_spacing(2.6)
                                        .color(alpha(tint(CYAN, 0.25), 0.98)),
                                )
                                .align(TextAlign::Left),
                        ),
                    ),
            );
        }
    }

    // The spark, falling with us — smaller now, our traveling companion
    // through the machine. It leads the dive.
    let spark_r = 14.0 - 6.0 * dive;
    let spark_y = 300.0 + dive * 480.0;
    let light = Painting::sized(
        Size::new(120.0, 120.0),
        PaintWith::new(move |book: &mut Sketchbook, _s: Size| {
            super::spark(book, 60.0, 60.0, spark_r, sec, 1.0, VIOLET_SOFT);
        }),
    );
    stack = stack.push(
        Positioned::new()
            .left(W * 0.5 - 60.0)
            .top(spark_y - 60.0)
            .width(120.0)
            .height(120.0)
            .child(light),
    );

    // The landing — the last quarter: the grid flattens into S06's
    // isometric plane, rising to meet the camera.
    if t > 0.72 {
        let land = clamp01((t - 0.72) / 0.28);
        let landing = Painting::sized(
            super::CANVAS,
            PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                // A floor of grid lines, rising: the crate city's ground,
                // arriving from below.
                let h = s.height;
                let rise = 1.0 - ease_out_cubic(land);
                let y_floor = h * (1.0 + rise);
                for j in 0..8 {
                    let y = y_floor - j as f32 * 90.0 * (1.0 - rise * 0.6);
                    let half = 500.0 + j as f32 * 240.0;
                    book.line(
                        Offset::new(W * 0.5 - half, y),
                        Offset::new(W * 0.5 + half, y),
                        alpha(VIOLET, 0.16 * land * (1.0 - j as f32 / 10.0)),
                        1.2,
                    );
                }
                super::glow(book, W * 0.5, y_floor, 700.0, VIOLET, 0.16 * land);
            }),
        );
        stack = stack.push(Positioned::fill().child(landing));
    }

    // The captions — the descent's beats.
    stack = stack.push(super::act_chip(
        "II",
        "THE MACHINE",
        clamp01((sec - 0.3) / 0.5),
    ));
    stack = stack.push(caption(
        "beneath the light — the machine",
        1002.0,
        clamp01((t - 0.06) / 0.12),
    ));
    stack = stack.push(caption(
        "one frame's path: widget → element → render → paint",
        966.0,
        clamp01((t - 0.40) / 0.12),
    ));

    let _ = (INK, MUTED, H, ease_out_cubic(t));

    stack.into()
}
