//! What a general path that changes every frame costs the GPU path — the
//! measurement PENDING §2.1 asked for before any MSAA work.
//!
//! ```console
//! cargo run --release -p vieww-gpu --example anim_paths
//! ```
//!
//! A non-analytic shape is antialiased on the GPU with the CPU rasterizer's
//! own coverage, cached in the mask atlas by content. A shape that changes
//! every frame misses that cache every frame, which costs two things:
//!
//! 1. **CPU coverage** — flatten, rasterise and pack the patch. Measured here
//!    as the planner's time for the frame, minus the same scene held still.
//! 2. **Upload** — the bytes a backend sends to bring its mask texture up to
//!    date, as `MaskAtlas::changes_since` reports them: the changed
//!    rectangles, or the whole texture after a compaction or grow. (Before
//!    `changes_since` existed, every version change was a whole-texture
//!    upload: 4.2 MB a frame for one rotating panel, 16.8 MB for all three.)
//!
//! Each scenario runs 120 frames at 1366x679 and reports medians, after a
//! 10-frame warm-up so a steady scene's caches are already full.

use std::time::Instant;

use vieww_foundation::{Color, Offset, Path, Rect, Transform};
use vieww_gpu::{MaskChanges, Planner};
use vieww_paint::{Clip, Command, Paint, Scene, Stroke};

const W: f32 = 1366.0;
const H: f32 = 679.0;
const FRAMES: usize = 120;
const WARMUP: usize = 10;

fn star(points: usize, outer: f32, inner: f32) -> Path {
    let mut p = Path::new();
    for i in 0..points * 2 {
        let r = if i % 2 == 0 { outer } else { inner };
        let a = i as f32 * std::f32::consts::PI / points as f32;
        let pt = Offset::new(r * a.cos(), r * a.sin());
        if i == 0 {
            p.move_to(pt);
        } else {
            p.line_to(pt);
        }
    }
    p.close();
    p
}

/// A wave across most of the window, drawn up to `progress` of its segments.
fn wave(progress: f32) -> Path {
    let mut p = Path::new();
    let segments = 24;
    let shown = ((segments as f32 * progress).ceil() as usize).max(1);
    p.move_to(Offset::new(60.0, 340.0));
    for i in 0..shown {
        let x0 = 60.0 + i as f32 * 52.0;
        let dir = if i % 2 == 0 { -1.0 } else { 1.0 };
        p.cubic_to(
            Offset::new(x0 + 17.0, 340.0 + dir * 140.0),
            Offset::new(x0 + 35.0, 340.0 + dir * 140.0),
            Offset::new(x0 + 52.0, 340.0),
        );
    }
    p
}

#[derive(Clone, Copy)]
enum Shape {
    Star,
    Stroke,
    Panel,
}

fn scene(shapes: &[Shape], t: f32) -> Scene {
    let mut scene = Scene::new();
    scene.push_command(Command::FillPath {
        path: Path::rect(Rect::new(0.0, 0.0, W, H)),
        paint: Paint::solid(Color::rgba(245, 244, 240, 255)),
        transform: Transform::IDENTITY,
        clip: Clip::NONE,
    });
    for shape in shapes {
        match shape {
            Shape::Star => scene.push_command(Command::FillPath {
                path: star(12, 160.0, 70.0),
                paint: Paint::solid(Color::rgba(200, 80, 40, 255)),
                transform: Transform::rotate(t)
                    .then(Transform::translate(Offset::new(300.0, 340.0))),
                clip: Clip::NONE,
            }),
            Shape::Stroke => scene.push_command(Command::StrokePath {
                path: wave(0.2 + 0.8 * (t * 0.5).fract()),
                stroke: Stroke::new(3.0),
                paint: Paint::solid(Color::rgba(20, 60, 160, 255)),
                transform: Transform::IDENTITY,
                clip: Clip::NONE,
            }),
            Shape::Panel => scene.push_command(Command::FillPath {
                path: Path::rounded_rect(Rect::new(-300.0, -180.0, 300.0, 180.0), 24.0),
                paint: Paint::solid(Color::rgba(40, 140, 90, 230)),
                transform: Transform::rotate(t * 0.3)
                    .then(Transform::translate(Offset::new(900.0, 340.0))),
                clip: Clip::NONE,
            }),
        }
    }
    scene
}

struct Run {
    plan_ms: f64,
    upload_bytes: u64,
    mean_upload: f64,
    full_uploads: usize,
    side: u32,
    masks: usize,
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn run(shapes: &[Shape], animate: bool) -> Run {
    let mut planner = Planner::new();
    let mut times = Vec::new();
    let mut uploads = Vec::new();
    // A backend's view of the texture: the version it last uploaded.
    let mut held: Option<u64> = None;
    let mut fulls = 0;
    for frame in 0..WARMUP + FRAMES {
        let t = if animate {
            frame as f32 * (1.0 / 60.0) * 1.7
        } else {
            0.4
        };
        let scene = scene(shapes, t);
        let start = Instant::now();
        let plan = planner.plan(&scene, W, H);
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        assert!(plan.is_complete(), "{:?}", plan.unsupported);
        let atlas = planner.mask_atlas();
        let full = u64::from(atlas.side()) * u64::from(atlas.side());
        let bytes = match atlas.changes_since(held) {
            MaskChanges::None => 0,
            MaskChanges::Full => {
                if frame >= WARMUP {
                    fulls += 1;
                }
                full
            }
            MaskChanges::Regions(rects) => rects
                .iter()
                .map(|[_, _, w, h]| u64::from(*w) * u64::from(*h))
                .sum(),
        };
        held = Some(atlas.version());
        if frame >= WARMUP {
            times.push(elapsed);
            uploads.push(bytes as f64);
        }
    }
    Run {
        plan_ms: median(times),
        mean_upload: uploads.iter().sum::<f64>() / uploads.len() as f64,
        full_uploads: fulls,
        upload_bytes: median(uploads) as u64,
        side: planner.mask_atlas().side(),
        masks: planner.mask_atlas().len(),
    }
}

fn main() {
    let cases: [(&str, &[Shape]); 4] = [
        ("rotating 12-point star, r=160", &[Shape::Star]),
        ("3px stroke drawn along a wave", &[Shape::Stroke]),
        ("rotating 600x360 rounded panel", &[Shape::Panel]),
        ("all three", &[Shape::Star, Shape::Stroke, Shape::Panel]),
    ];
    println!(
        "{:<34} {:>9} {:>9} {:>10} {:>11} {:>11} {:>6} {:>8} {:>6}",
        "scenario (120 frames)",
        "still ms",
        "moving ms",
        "coverage",
        "upload med",
        "upload mean",
        "fulls",
        "atlas",
        "masks"
    );
    for (name, shapes) in cases {
        let still = run(shapes, false);
        let moving = run(shapes, true);
        println!(
            "{:<34} {:>9.3} {:>9.3} {:>8.3}ms {:>8.2} MB {:>8.2} MB {:>6} {:>6}^2 {:>6}",
            name,
            still.plan_ms,
            moving.plan_ms,
            moving.plan_ms - still.plan_ms,
            moving.upload_bytes as f64 / 1e6,
            moving.mean_upload / 1e6,
            moving.full_uploads,
            moving.side,
            moving.masks,
        );
        assert_eq!(still.upload_bytes, 0, "a still scene must upload nothing");
    }
}
