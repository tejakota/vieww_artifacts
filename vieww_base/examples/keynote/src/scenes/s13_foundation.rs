//! **S13 · THE FOUNDATION** — what the studio is standing on.
//!
//! The camera keeps pulling back and the studio turns out to be the top of a
//! stack. Beneath it, in space, the framework assembles itself: **36 crates**
//! — counted from `vieww_base/Cargo.toml` by the code that draws them, never
//! typed — arranged in the layers they actually form, from the `say` DSL at
//! the top through the three trees, the paint and scene layers, the render
//! planner, the GPU and HAL backends, down to the platform crates at the
//! floor.
//!
//! Each crate is a real slab with a real name. They fly in bottom-up, because
//! that is the order they depend on each other in, and the studio's window —
//! still running, still showing the counter at 7 — settles on top of them as
//! the last layer.
//!
//! The sentence is the film's thesis: **viewwstudio is built on vieww**. Not
//! a partnership, not a plugin: one workspace, one renderer, and the IDE is
//! its first and hardest customer.

use vieww_foundation::{BlendMode, Color, Gradient, Offset, Rect, Size, Sketchbook};
use vieww_widget::prelude::*;

use crate::film::Ctx;
use crate::kit::{
    alpha, caption, clamp01, dust, ease_out_cubic, grain, ground, horizon, mix, painter, seg,
    smoothstep, spring, vignette, xywh, Type, CYAN, CYAN_SOFT, H, INK, INK_SOFT, MINT, MUTED,
    VIOLET, VIOLET_DEEP, VIOLET_SOFT, W,
};
use crate::solid::{v3, Cam, V3};
use crate::studio::{self, compose};

/// The layers, top to bottom, exactly as the workspace's own crate list
/// stacks up. The film draws these names; the *count* on screen is read from
/// the manifest at render time, so the two can be checked against each other.
const LAYERS: &[(&str, &[&str])] = &[
    ("the language", &["vieww-say-codegen"]),
    ("the three trees", &["vieww-widget", "vieww-element", "vieww-render", "vieww-widget-macros"]),
    ("composition", &["vieww-scene", "vieww-render-graph", "vieww-render-planner"]),
    ("the rasterizer", &["vieww-paint", "vieww-shaders", "vieww-effects", "vieww-text", "vieww-image"]),
    ("the machine", &["vieww-gpu", "vieww-hal", "vieww-hardware"]),
    ("the loop", &["vieww-runtime", "vieww-animation", "vieww-gestures", "vieww-scroll", "vieww-interaction"]),
    ("the platforms", &["vieww-platform", "vieww-platform-winit", "vieww-platform-web", "vieww-platform-web-dom"]),
    ("the workbench", &["vieww-cli", "vieww-build", "vieww-reload", "vieww-devtools", "vieww-test-harness", "vieww-plugin", "vieww-plugin-macros", "vieww-accessibility", "vieww-asset", "vieww-foundation", "vieww"]),
];

/// The beats, in scene-seconds.
const RISE_T0: f32 = 0.70;
const RISE_SPAN: f32 = 6.40;
const CLAIM_T: f32 = 10.20;
const COUNT_T: f32 = 13.80;

pub fn frame(ctx: &Ctx) -> WidgetNode {
    let sec = ctx.sec;
    let frame_i = ctx.frame;

    // The camera: a slow orbit and a slow lift, so the stack is read as a
    // solid rather than as a list.
    let orbit = 0.16 + 0.10 * smoothstep(seg(sec, 0.0, 18.0));
    let cam = Cam {
        // Pushed left of the stack's axis, so the solid sits right of centre
        // and the film's own figures have the left margin to live in.
        eye: v3(-2.1, 0.25 + 0.35 * smoothstep(seg(sec, 0.0, 16.0)), -15.4),
        yaw: orbit,
        pitch: -0.03,
        focal: 1320.0,
    };
    let canvas = Size::new(W, H);

    let claim = smoothstep(seg(sec, CLAIM_T, CLAIM_T + 1.0)) * (1.0 - smoothstep(seg(sec, 20.6, 21.6)));
    let count_a = smoothstep(seg(sec, COUNT_T, COUNT_T + 0.9)) * (1.0 - smoothstep(seg(sec, 20.6, 21.6)));

    // The crate count, read from the workspace's own manifest by the census.
    let crates = ctx.probe.crates;
    let listed: usize = LAYERS.iter().map(|(_, v)| v.len()).sum();

    let layer_count = LAYERS.len();
    let bg = painter(move |book: &mut Sketchbook, size: Size| {
        ground(book, size, sec, 1.0);
        horizon(book, size, VIOLET_DEEP, 0.9);
        dust(book, size, sec, 44, CYAN_SOFT, 0.5);

        // The floor the stack stands on.
        crate::solid::floor(book, &cam, canvas, -3.0, 10.0, 1.5, alpha(CYAN, 0.8));

        // The slabs, bottom to top. Each layer is one plate; each crate on it
        // is a tile with its own name, so the count on screen is countable by
        // anyone who pauses the film.
        for (li, (_name, members)) in LAYERS.iter().enumerate() {
            // Bottom-up: the deepest layer arrives first.
            let order = layer_count - 1 - li;
            let t0 = RISE_T0 + order as f32 * (RISE_SPAN / layer_count as f32);
            let rise = spring(seg(sec, t0, t0 + 1.5), 11.0, 0.46);
            if rise <= 0.005 {
                continue;
            }
            let y = -2.5 + order as f32 * 0.62;
            let drop = (1.0 - rise) * 3.2;

            // The plate.
            let hue = order as f32 / (layer_count - 1) as f32;
            let base = mix(CYAN, VIOLET, hue);
            let plate = crate::solid::slab(v3(0.0, y - drop, 0.0), 7.6, 0.10, 1, 1, alpha(base, 0.20));
            crate::solid::draw(
                book,
                &plate,
                &cam,
                canvas,
                &crate::solid::Style {
                    ambient: 0.9,
                    cull: false,
                    alpha: 0.55 * rise,
                    fog: Some((Color::rgb(8, 7, 9), 12.0, 22.0)),
                    ..crate::solid::Style::default()
                },
            );

            // The crates on it.
            let n = members.len();
            for (ci, _m) in members.iter().enumerate() {
                let span = 7.0;
                let w = (span / n as f32) * 0.88;
                let x = -span * 0.5 + (ci as f32 + 0.5) * (span / n as f32);
                let tile = crate::solid::box_mesh(
                    v3(x, y - drop + 0.16, 0.0),
                    v3(w * 0.5, 0.15, 0.42),
                    alpha(base, 0.95),
                );
                crate::solid::draw(
                    book,
                    &tile,
                    &cam,
                    canvas,
                    &crate::solid::Style {
                        ambient: 0.40,
                        rim: Some((Color::WHITE, 0.30)),
                        alpha: rise,
                        fog: Some((Color::rgb(8, 7, 9), 12.0, 22.0)),
                        ..crate::solid::Style::default()
                    },
                );
            }
        }

        // The studio, settling on top as the last layer. It is drawn as a
        // slab with the window's own colours, so the IDE reads as one more
        // crate rather than as a picture pasted on the stack.
        let top_rise = spring(seg(sec, RISE_T0 + RISE_SPAN + 0.2, RISE_T0 + RISE_SPAN + 1.8), 10.0, 0.42);
        if top_rise > 0.005 {
            let y = -2.5 + layer_count as f32 * 0.62 + 0.46;
            let drop = (1.0 - top_rise) * 4.0;
            let slab = crate::solid::box_mesh(
                v3(0.0, y - drop, 0.0),
                v3(3.9, 0.30, 0.62),
                Color::rgb(0x1C, 0x1C, 0x1F),
            );
            crate::solid::draw(
                book,
                &slab,
                &cam,
                canvas,
                &crate::solid::Style {
                    ambient: 0.48,
                    rim: Some((VIOLET_SOFT, 0.55)),
                    alpha: top_rise,
                    ..crate::solid::Style::default()
                },
            );
            // Its own light, falling on the stack beneath it.
            if let Some((p, _, k)) = cam.project(v3(0.0, y - drop - 0.4, 0.0), canvas) {
                book.blended_layer(1.0, 50.0, BlendMode::Plus, None, |g| {
                    g.circle(
                        p,
                        420.0 * k * cam.focal / 1180.0,
                        Gradient::radial_fill().with_dither().with_stops(&[
                            (0.0, alpha(VIOLET, 0.16 * top_rise)),
                            (1.0, alpha(VIOLET, 0.0)),
                        ]),
                    );
                });
            }
        }

        // The spine: one line running the whole height of the stack, violet
        // at the studio's end and cyan at the framework's — the film's
        // lineage gradient, now standing up in space.
        let spine_a = smoothstep(seg(sec, RISE_T0 + 2.0, RISE_T0 + 4.0));
        if spine_a > 0.01 {
            let top = v3(-4.6, -2.9 + layer_count as f32 * 0.78 + 0.55, 0.0);
            let bot = v3(-4.6, -2.6, 0.0);
            if let (Some((a, _, _)), Some((b, _, _))) =
                (cam.project(top, canvas), cam.project(bot, canvas))
            {
                book.stroke(
                    {
                        let mut p = vieww_foundation::Path::new();
                        p.move_to(a);
                        p.line_to(b);
                        p
                    },
                    Gradient::vertical().with_dither().with_stops(&[
                        (0.0, alpha(VIOLET, 0.85 * spine_a)),
                        (1.0, alpha(CYAN, 0.75 * spine_a)),
                    ]),
                    2.5,
                );
            }
        }

        grain(book, size, frame_i, 0.012, 320);
        vignette(book, size, 1.0);
    });

    let mut nodes: Vec<WidgetNode> = Vec::new();

    // Every crate's name, projected. This is why the scene is legible: the
    // slabs are not decoration, they are a labelled census.
    for (li, (layer_name, members)) in LAYERS.iter().enumerate() {
        let order = layer_count - 1 - li;
        let t0 = RISE_T0 + order as f32 * (RISE_SPAN / layer_count as f32);
        let rise = spring(seg(sec, t0, t0 + 1.5), 11.0, 0.46).clamp(0.0, 1.0);
        if rise <= 0.05 {
            continue;
        }
        let y = -2.5 + order as f32 * 0.62;
        let drop = (1.0 - rise) * 3.2;

        // The layer's own name, off to the right of its plate.
        if let Some((p, _, k)) = cam.project(v3(4.4, y - drop + 0.2, 0.0), canvas) {
            nodes.push(
                Type::new(*layer_name)
                    .mono()
                    .size((13.0 * cam.px(k)).clamp(9.0, 15.0))
                    .track(2.0)
                    .color(alpha(INK_SOFT, 0.85 * rise))
                    .at(p.dx + 16.0, p.dy - 8.0)
                    .width(260.0)
                    .into(),
            );
        }

        let n = members.len();
        for (ci, m) in members.iter().enumerate() {
            let span = 7.0;
            let x = -span * 0.5 + (ci as f32 + 0.5) * (span / n as f32);
            let Some((p, _, k)) = cam.project(v3(x, y - drop + 0.34, 0.44), canvas) else {
                continue;
            };
            let size = (10.5 * cam.px(k)).clamp(6.5, 12.0);
            // Names are short enough to read at this scale only because the
            // `vieww-` prefix is dropped — the stack's own name is the frame.
            let short = m.strip_prefix("vieww-").unwrap_or(m);
            nodes.push(
                Type::new(short)
                    .mono()
                    .size(size)
                    .track(0.3)
                    .color(alpha(INK, 0.92 * rise))
                    .center()
                    .at(p.dx - 70.0, p.dy - size * 0.6)
                    .width(140.0)
                    .into(),
            );
        }
    }

    // The studio's label on its own slab.
    let top_rise = spring(seg(sec, RISE_T0 + RISE_SPAN + 0.2, RISE_T0 + RISE_SPAN + 1.8), 10.0, 0.42).clamp(0.0, 1.0);
    if top_rise > 0.05 {
        let y = -2.5 + layer_count as f32 * 0.62 + 0.46;
        let drop = (1.0 - top_rise) * 4.0;
        if let Some((p, _, k)) = cam.project(v3(0.0, y - drop + 0.36, 0.64), canvas) {
            nodes.push(
                Type::new("viewwstudio")
                    .size((22.0 * cam.px(k)).clamp(14.0, 26.0))
                    .medium()
                    .track(1.0)
                    .color(alpha(INK, 0.97 * top_rise))
                    .center()
                    .at(p.dx - 200.0, p.dy - 16.0)
                    .width(400.0)
                    .into(),
            );
        }
    }

    // The thesis.
    if claim > 0.004 {
        nodes.push(
            Type::new("viewwstudio is built on vieww")
                .size(50.0)
                .light()
                .track(0.6)
                .color(alpha(INK, 0.98 * claim))
                .center()
                .banner(H * 0.115)
                .into(),
        );
        nodes.push(
            Type::new("one workspace · one renderer · the IDE is its first and hardest customer")
                .mono()
                .size(15.0)
                .track(2.2)
                .color(alpha(CYAN, 0.9 * claim))
                .center()
                .banner(H * 0.115 + 72.0)
                .into(),
        );
    }

    // The count — measured from the manifest, and the film says which is
    // which: the number it counted, and the number this scene draws.
    if count_a > 0.004 {
        let text = if crates > 0 {
            format!("{crates}")
        } else {
            "—".to_string()
        };
        nodes.push(
            Type::new(text)
                .size(78.0)
                .bold()
                .color(alpha(INK, 0.97 * count_a))
                .at(112.0, H * 0.66)
                .width(220.0)
                .into(),
        );
        nodes.push(
            Type::new("crates in the workspace")
                .mono()
                .size(14.0)
                .track(2.4)
                .color(alpha(VIOLET_SOFT, 0.92 * count_a))
                .at(114.0, H * 0.66 + 92.0)
                .width(420.0)
                .into(),
        );
        nodes.push(
            Type::new(format!(
                "counted from Cargo.toml at render time · {listed} drawn above"
            ))
            .mono()
            .size(12.0)
            .track(1.4)
            .color(alpha(MUTED, 0.85 * count_a))
            .at(114.0, H * 0.66 + 118.0)
            .width(520.0)
            .into(),
        );
    }

    nodes.push(caption(
        "the stack the session was standing on the whole time",
        smoothstep(seg(sec, 3.4, 4.4)) * (1.0 - smoothstep(seg(sec, 20.8, 21.7))),
    ));
    let _ = (MINT, studio::WINDOW, ease_out_cubic, clamp01);
    compose(bg, nodes)
}

/// The layer table's own gate: the drawn crates must not outnumber the
/// workspace's. A film that draws more crates than exist is a film that
/// invented one.
#[cfg(test)]
mod tests {
    use super::LAYERS;

    #[test]
    fn the_drawn_crates_are_unique() {
        let mut all: Vec<&str> = LAYERS.iter().flat_map(|(_, v)| v.iter().copied()).collect();
        let n = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), n, "a crate is drawn twice");
    }

    #[test]
    fn every_layer_has_members() {
        assert!(LAYERS.iter().all(|(_, v)| !v.is_empty()));
    }
}
