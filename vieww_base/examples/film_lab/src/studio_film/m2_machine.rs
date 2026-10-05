//! Movement II · THE ENGINE, shown rather than diagrammed — the machine
//! room and the mark in real 3D.
//!
//! * **Z10C · the machine room.** The film lab's plates are not pictures
//!   of vieww output; they are vieww programs. Twelve of them run *live
//!   inside this frame* — each one the plate's own `frame(t)` widget tree,
//!   mounted in this film's tree, laid out by this film's layout pass and
//!   rasterised by this film's rasteriser. The scene opens on one of them
//!   (the galaxy, ~60k shapes a frame) filling the body, pulls back, and
//!   the other eleven arrive around it. The numbers on the tiles are each
//!   plate's own census (`film_lab/renders/<plate>/metrics.txt`), read at
//!   render time — not typed.
//! * **Z10D · the mark in 3D.** `vieww-3d` — the scene graph, the
//!   depth-buffered rasteriser, the shadow map — renders the vieww mark
//!   (the same OBJ the beyond-UI card loads through `vieww-mesh`) on a
//!   floor, with forty-nine small blocks in orbit around it: one per crate
//!   in the workspace manifest. The camera moves; the light moves; the
//!   triangle and fragment counts on screen are the renderer's own
//!   `RenderStats` for the frame you are looking at.

use std::sync::OnceLock;

use vieww_foundation::{Color, Gradient, Offset, Rect, Size, Sketchbook, TextAlign};
use vieww_widget::prelude::*;
use vieww_widget::{Clip, Transformed};

use crate::film_lib::{clamp01, ease_in_out, ease_out_cubic, ease_out_expo};
use crate::product_film as pf;
use super::{ACCENT, BG_DEEP, BRAND_FAR, BRAND_NEAR, CANVAS, INK, MUTED, SYN_TYPE};

// ── Z10C · the machine room ─────────────────────────────────────────────────

/// A lab plate: its name, its frame function, its own loop length.
type PlateFn = fn(f32) -> WidgetNode;

/// The twelve on the wall, in reading order. Chosen for range — light,
/// orbits, optics, a black hole, a galaxy, chaos, interference, a
/// quasicrystal — and for what each costs a frame (every one renders in
/// well under a frame-budget's multiple on the lab's own receipt).
fn wall() -> [(&'static str, PlateFn, f32); 12] {
    [
        ("eclipse", crate::exp_eclipse::frame, crate::exp_eclipse::SECONDS),
        ("orrery", crate::exp_orrery::frame, crate::exp_orrery::SECONDS),
        ("bubble", crate::exp_bubble::frame, crate::exp_bubble::SECONDS),
        ("cymatics", crate::exp_cymatics::frame, crate::exp_cymatics::SECONDS),
        ("blackhole", crate::exp_blackhole::frame, crate::exp_blackhole::SECONDS),
        ("galaxy", crate::exp_galaxy::frame, crate::exp_galaxy::SECONDS),
        ("prism", crate::exp_prism::frame, crate::exp_prism::SECONDS),
        ("lorenz", crate::exp_lorenz::frame, crate::exp_lorenz::SECONDS),
        ("aurora", crate::exp_aurora::frame, crate::exp_aurora::SECONDS),
        ("ink", crate::exp_ink::frame, crate::exp_ink::SECONDS),
        ("startrail", crate::exp_startrail::frame, crate::exp_startrail::SECONDS),
        ("crystal", crate::exp_crystal::frame, crate::exp_crystal::SECONDS),
    ]
}

/// The plate the scene opens on, full-body, before the pull-back.
const HERO: usize = 5;

/// The wall's geometry, in this scene's world.
const COLS: usize = 4;
const TILE_W: f32 = 420.0;
const TILE_H: f32 = 236.25;
const GAP: f32 = 20.0;
const WALL_X: f32 = 90.0;
const WALL_Y: f32 = 250.0;

fn cell(i: usize) -> Rect {
    let (c, r) = (i % COLS, i / COLS);
    let x = WALL_X + c as f32 * (TILE_W + GAP);
    let y = WALL_Y + r as f32 * (TILE_H + GAP);
    Rect::new(x, y, x + TILE_W, y + TILE_H)
}

/// The hero's opening rect: the wall's whole height at 16:9, centred on it.
fn hero_rect() -> Rect {
    let wall_w = COLS as f32 * (TILE_W + GAP) - GAP;
    let wall_h = 3.0 * (TILE_H + GAP) - GAP;
    let w = wall_h * 16.0 / 9.0;
    let x = WALL_X + (wall_w - w) * 0.5;
    Rect::new(x, WALL_Y, x + w, WALL_Y + wall_h)
}

/// A plate's own census — shapes per frame — from its receipt on disk.
/// `None` when the receipt is missing (a stripped checkout): the tile
/// then says nothing rather than a number nobody measured.
fn plate_shapes(name: &str) -> Option<u64> {
    static CACHE: OnceLock<std::collections::HashMap<String, Option<u64>>> = OnceLock::new();
    let map = CACHE.get_or_init(|| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../film_lab/renders");
        wall()
            .iter()
            .map(|(n, _, _)| {
                let text = std::fs::read_to_string(root.join(n).join("metrics.txt")).unwrap_or_default();
                let get = |k: &str| {
                    text.lines()
                        .find_map(|l| l.strip_prefix(k))
                        .and_then(|v| v.trim().parse::<u64>().ok())
                };
                let per = match (get("shapes="), get("frames=")) {
                    (Some(s), Some(f)) if f > 0 => Some(s / f),
                    _ => None,
                };
                ((*n).to_string(), per)
            })
            .collect()
    });
    map.get(name).copied().flatten()
}

/// One live plate, laid out at its native 1280×720 and drawn scaled into
/// `r`, clipped to rounded corners.
fn live_plate(build: PlateFn, t: f32, r: Rect, a: f32) -> WidgetNode {
    let s = r.width() / crate::film_lib::CANVAS_W;
    let inner = Stack::new().push(
        Positioned::new()
            .left(0.0)
            .top(0.0)
            .width(crate::film_lib::CANVAS_W)
            .height(crate::film_lib::CANVAS_H)
            .child(Transformed::scale(s, s).child(build(t))),
    );
    Positioned::new()
        .left(r.left)
        .top(r.top)
        .width(r.width())
        .height(r.height())
        .child(Opacity::new(a.min(1.0)).child(Clip::rounded(10.0 + 4.0 * s).child(inner)))
        .into()
}

pub fn the_machine_room(ctx: &pf::Ctx) -> WidgetNode {
    let t = ctx.t;
    let sec = ctx.sec;
    let mut stack = Stack::new();

    super::frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            book.rect(Rect::new(0.0, 0.0, s.width, s.height), BG_DEEP);
            pf::vignette(book, s.width, s.height, 0.55);
        }),
    )));

    let plates = wall();
    // The pull-back: the hero leaves the full body for its cell.
    let pull = ease_in_out(clamp01((sec - 3.0) / 2.0));
    let hero_at = lerp_rect(hero_rect(), cell(HERO), pull);

    // The others arrive outward from the hero's cell, each with a small
    // rise — a wall switching on, not a grid fading up.
    let hc = cell(HERO);
    let (hx, hy) = ((hc.left + hc.right) * 0.5, (hc.top + hc.bottom) * 0.5);
    for (i, (_, build, secs)) in plates.iter().enumerate() {
        if i == HERO {
            continue;
        }
        let c = cell(i);
        let d = (((c.left + c.right) * 0.5 - hx).powi(2) + ((c.top + c.bottom) * 0.5 - hy).powi(2)).sqrt();
        let p = ease_out_expo(clamp01((sec - 3.6 - d / 1600.0) / 0.7));
        if p <= 0.01 {
            continue;
        }
        let shrink = 0.92 + 0.08 * p;
        let r = Rect::new(
            (c.left + c.right) * 0.5 - c.width() * 0.5 * shrink,
            (c.top + c.bottom) * 0.5 - c.height() * 0.5 * shrink + (1.0 - p) * 18.0,
            (c.left + c.right) * 0.5 + c.width() * 0.5 * shrink,
            (c.top + c.bottom) * 0.5 + c.height() * 0.5 * shrink + (1.0 - p) * 18.0,
        );
        // Each plate runs on its own loop, from this scene's clock.
        let pt = ((sec + i as f32 * 0.37) / secs).fract();
        stack = stack.push(live_plate(*build, pt, r, p));
    }
    // The hero last, so it is on top while it travels.
    let (_, hero_build, hero_secs) = plates[HERO];
    stack = stack.push(live_plate(hero_build, (sec / hero_secs).fract(), hero_at, 1.0));

    // The tiles' names and their own shape counts, once the wall is up.
    let label_a = clamp01((sec - 5.6) / 0.6);
    if label_a > 0.01 {
        for (i, (name, _, _)) in plates.iter().enumerate() {
            let c = cell(i);
            let text = match plate_shapes(name) {
                Some(n) => format!("{name} · {} shapes", pf::group_commas(n)),
                None => (*name).to_string(),
            };
            let a = label_a * clamp01((sec - 5.6 - i as f32 * 0.05) / 0.3);
            stack = stack.push(Positioned::new().left(c.left).top(c.bottom - 30.0).width(c.width()).height(30.0).child(
                Opacity::new(a).child(Painting::sized(Size::new(c.width(), 30.0), PaintWith::new(move |book: &mut Sketchbook, s: Size| {
                    book.rect(
                        Rect::new(0.0, 0.0, s.width, s.height),
                        Gradient::vertical().with_stops(&[(0.0, pf::alpha(Color::BLACK, 0.0)), (1.0, pf::alpha(Color::BLACK, 0.75))]),
                    );
                }))),
            ));
            stack = stack.push(super::frame::label(c.left + 12.0, c.bottom - 28.0, c.width() - 24.0, 24.0, text,
                pf::geist_mono(15.0).letter_spacing(0.4).color(pf::alpha(INK, 0.92)), TextAlign::Left, a));
        }
    }

    // The sum — every shape on the wall in one frame, from the receipts.
    let total: u64 = plates.iter().filter_map(|(n, _, _)| plate_shapes(n)).sum();
    let sum_p = clamp01((sec - 7.2) / 1.6);
    if sum_p > 0.0 && total > 0 {
        let n = pf::count_up(total, sum_p);
        stack = stack.push(super::frame::label(WALL_X, WALL_Y - 56.0, 1740.0, 40.0,
            format!("{} shapes a frame on this wall — twelve programs, one rasteriser", pf::group_commas(n)),
            pf::geist_mono(21.0).letter_spacing(0.8).color(pf::alpha(ACCENT, 0.97)), TextAlign::Left, clamp01(sum_p * 3.0)));
    }

    stack = stack.push(super::frame::caption("This is a UI framework.", 1002.0, clamp01((sec - 0.3) / 0.5)));
    stack = stack.push(super::frame::caption("Every tile is a vieww program, running live in this frame.", 966.0, clamp01((sec - 5.2) / 0.6)));
    let _ = t;
    stack.into()
}

fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    let l = |x: f32, y: f32| x + (y - x) * t;
    Rect::new(l(a.left, b.left), l(a.top, b.top), l(a.right, b.right), l(a.bottom, b.bottom))
}

// ── Z10D · the mark in 3D ───────────────────────────────────────────────────

/// The render's resolution — the image lands in the world at this size.
const R_W: u32 = 1600;
const R_H: u32 = 760;

pub fn the_mark_in_3d(ctx: &pf::Ctx) -> WidgetNode {
    use vieww_3d::{geometry, Camera, Content, Light, Mat4, Material, Node, Quat, Renderer, Rgb, Scene, Vec3};

    let sec = ctx.sec;
    let mut stack = Stack::new();

    super::frame::ground(Positioned::fill().child(Painting::sized(
        CANVAS,
        PaintWith::new(move |book: &mut Sketchbook, s: Size| {
            book.rect(Rect::new(0.0, 0.0, s.width, s.height), Color::rgb(7, 6, 10));
            pf::vignette(book, s.width, s.height, 0.6);
        }),
    )));

    // The scene, built for this frame: a pure function of the clock.
    let mut scene = Scene::new();
    // A deep room: the background and a linear fog toward it, so the
    // floor runs out into the dark instead of ending at an edge.
    let deep = Rgb::from_color(Color::rgb(9, 8, 13));
    scene.background = deep;
    scene.fog = Some((deep, 5.5, 10.5));
    let rise = ease_out_cubic(clamp01(sec / 1.6));
    // The mark turns to show its depth, then settles facing the camera.
    let spin = 0.55 * (sec * 0.55).sin() * (1.0 - 0.6 * clamp01((sec - 7.0) / 3.0));
    let mark = super::models::mark_mesh().clone();
    scene.add(
        Node::new("mark", Content::mesh(mark, Material::standard(BRAND_NEAR, 0.35, 0.32)))
            .at(Vec3::new(0.0, 1.2 + 0.06 * (sec * 1.3).sin() - 0.7 * (1.0 - rise), 0.0))
            .rotated(Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), spin))
            .scaled(Vec3::splat(1.2)),
        None,
    );
    // Forty-nine blocks in orbit — one per crate in the manifest.
    let crates = super::workspace_crates().len();
    let orbit_p = ease_out_expo(clamp01((sec - 1.2) / 2.2));
    let instances: Vec<Mat4> = (0..crates)
        .map(|k| {
            let a = k as f32 / crates as f32 * std::f32::consts::TAU + sec * 0.22;
            let ring = 2.15 + 0.35 * ((k * 7) % 5) as f32 / 4.0;
            let y = 0.55 + 0.9 * ((k * 3) % 7) as f32 / 6.0;
            let r = ring * orbit_p;
            Mat4::compose(
                Vec3::new(r * a.cos(), y * orbit_p + 0.08, r * a.sin()),
                Quat::from_axis_angle(Vec3::new(0.3, 1.0, 0.2), a * 2.0 + sec),
                Vec3::splat(0.13 * orbit_p.max(0.01)),
            )
        })
        .collect();
    scene.add(
        Node::new("crates", Content::instanced(geometry::box_mesh(1.0, 1.0, 1.0), Material::standard(BRAND_FAR, 0.1, 0.5), instances)),
        None,
    );
    let mut floor_mat = Material::standard(Color::rgb(30, 27, 40), 0.0, 0.9);
    floor_mat.receive_shadow = true;
    scene.add(Node::new("floor", Content::mesh(geometry::plane(18.0, 18.0, 1, 1), floor_mat)), None);
    scene.add(Node::new("sky", Content::Light(Light::Hemisphere {
        sky: Rgb::from_color(Color::rgb(120, 110, 170)),
        ground: Rgb::from_color(Color::rgb(20, 16, 28)),
        intensity: 0.35,
    })), None);
    let sun = 0.6 + 0.25 * (sec * 0.3).sin();
    scene.add(Node::new("key", Content::Light(Light::Directional {
        color: Rgb::new(1.0, 0.96, 0.92),
        intensity: 1.35,
        direction: Vec3::new(-sun, -1.0, -0.45),
        shadow: true,
    })), None);
    scene.add(
        Node::new("rim", Content::Light(Light::Point { color: Rgb::from_color(BRAND_FAR), intensity: 2.2, range: 9.0 }))
            .at(Vec3::new(1.8, 2.6, -2.4)),
        None,
    );

    // The camera: a slow orbit and dolly-in.
    let cam_a = -0.55 + 0.45 * ease_in_out(clamp01(sec / 11.0));
    let dist = 7.6 - 1.0 * ease_in_out(clamp01(sec / 11.0));
    let camera = Camera::perspective(
        Vec3::new(dist * cam_a.sin(), 2.1, dist * cam_a.cos()),
        Vec3::new(0.0, 1.05, 0.0),
        0.62,
    );
    let (image, stats) = Renderer::new(R_W, R_H).samples(2).render(&mut scene, &camera);

    let img_a = clamp01(sec / 0.8);
    let (x, y) = (160.0, 236.0);
    stack = stack.push(
        Positioned::new().left(x).top(y).width(R_W as f32).height(R_H as f32).child(
            Opacity::new(img_a).child(Clip::rounded(18.0).child(vieww_widget::Image::new(image).label("the vieww mark, rendered by vieww-3d"))),
        ),
    );

    // The renderer's own receipt for this frame.
    let stat_a = clamp01((sec - 2.6) / 0.6);
    if stat_a > 0.01 {
        let line = format!(
            "vieww-3d · {} triangles · {} fragments · {} crates in orbit · shadow-mapped",
            pf::group_commas(stats.triangles as u64),
            pf::group_commas(stats.fragments as u64),
            crates,
        );
        stack = stack.push(super::frame::label(x, y + R_H as f32 + 14.0, R_W as f32, 30.0, line,
            pf::geist_mono(19.0).letter_spacing(0.6).color(pf::alpha(SYN_TYPE, 0.95)), TextAlign::Left, stat_a));
    }
    let tag_a = clamp01((sec - 0.8) / 0.6);
    stack = stack.push(super::frame::chip_at(x + 22.0, y + 22.0, 380.0, 40.0, "rendered live by vieww-3d".to_string(), 19.0, BRAND_FAR, tag_a));

    stack = stack.push(super::frame::caption("Real 3D, in the same frame as your interface.", 1002.0, clamp01((sec - 0.2) / 0.5)));
    stack = stack.push(super::frame::caption("Lights, shadows, a depth buffer — the same engine, all the way down.", 966.0, clamp01((sec - 4.0) / 0.6)));
    let _ = (MUTED, INK, Offset::ZERO);
    stack.into()
}
