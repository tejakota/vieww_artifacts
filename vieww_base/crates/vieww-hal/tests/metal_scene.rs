//! `MetalSceneRenderer` against the CPU rasterizer — the Metal twin of
//! `tests/vulkan_scene.rs`, and the first thing that ever *executes* it:
//! there is no software Metal, so the `framework-checks` macOS runner is
//! where these run.
//!
//! The oracle, the tolerance shape and the edge rule are the Vulkan suite's,
//! verbatim — see that file for why the comparison is `NativeRenderer` and
//! not a stored image, and why "interiors must match, edges may not" is the
//! assertion: the same reasoning, one backend later. What is *different* is
//! the reach: this file pins the port's first-increment claims (the shared
//! traversal through Metal pipelines, the target format, the readback
//! conversion, the retained-damage path), not Vulkan's full matrix — the
//! compositor suite's scenes (every blend mode, blur, mask and shadow) come
//! across as the port is validated on hardware, which is what the Vulkan
//! backend's own history did too.
//!
//! `#[ignore]`d as every hardware-gated suite here is:
//! `cargo test -p vieww-hal --features metal -- --ignored`.

#![cfg(feature = "metal")]

use vieww_foundation::{Color, Rect, Transform};
use vieww_gpu::Planner;
use vieww_hal::metal::{MetalDevice, MetalSceneRenderer};
use vieww_paint::native::NativeRenderer;
use vieww_paint::{Canvas, Clip, Command, Paint, Scene};

const WIDTH: u32 = 120;
const HEIGHT: u32 = 90;

/// One Metal device at a time for the whole file, as the Vulkan suites
/// serialise theirs: the runner's GPU is paravirtual, and these tests each
/// bring up a device.
static ONE_DEVICE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The guard is returned, not dropped here: it has to be held for as long as
/// the device lives.
type Exclusive = std::sync::MutexGuard<'static, ()>;

fn try_renderer() -> Option<(Exclusive, MetalDevice, MetalSceneRenderer)> {
    let guard = ONE_DEVICE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let device = match MetalDevice::new() {
        Ok(device) => device,
        Err(error) => {
            eprintln!("skipping: no Metal device available ({error})");
            return None;
        }
    };
    let renderer = MetalSceneRenderer::new(&device).expect("the pipelines should build");
    Some((guard, device, renderer))
}

fn fill_rect(scene: &mut Scene, rect: Rect, color: Color) {
    scene.push_command(Command::FillRect {
        rect,
        paint: Paint::solid(color),
        transform: Transform::IDENTITY,
        clip: Clip::NONE,
    });
}

/// The CPU rasterizer's answer for the same scene.
fn on_cpu(scene: &Scene, base: Color) -> Vec<u8> {
    let mut renderer = NativeRenderer::new();
    let (pixels, _) = renderer
        .render_to_pixels(scene, WIDTH, HEIGHT, base)
        .expect("the CPU rasterizer should render this scene");
    pixels.data().to_vec()
}

/// Count pixels where the two renderers disagree, excluding pixels that sit
/// on a shape's antialiased edge — `tests/vulkan_scene.rs`'s
/// `interior_mismatches`, verbatim: an edge is a property of the reference
/// picture, not of the diff.
fn interior_mismatches(gpu: &[u8], cpu: &[u8], tolerance: u8) -> usize {
    assert_eq!(gpu.len(), cpu.len(), "same dimensions");
    let w = WIDTH as usize;
    let h = HEIGHT as usize;
    let channel = |buffer: &[u8], index: usize, c: usize| buffer[index * 4 + c];
    let differs = |index: usize| {
        (0..4).any(|c| channel(gpu, index, c).abs_diff(channel(cpu, index, c)) > tolerance)
    };
    let reference_edge = |x: usize, y: usize| {
        let index = y * w + x;
        (-1i64..=1).any(|dy| {
            (-1i64..=1).any(|dx| {
                let nx = x as i64 + dx;
                let ny = y as i64 + dy;
                if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                    return false;
                }
                let neighbour = ny as usize * w + nx as usize;
                (0..4).any(|c| channel(cpu, index, c).abs_diff(channel(cpu, neighbour, c)) > 8)
            })
        })
    };

    let mut count = 0;
    for y in 0..h {
        for x in 0..w {
            if differs(y * w + x) && !reference_edge(x, y) {
                count += 1;
            }
        }
    }
    count
}

#[test]
#[ignore = "needs a Metal GPU — the framework-checks macOS runner; run with `cargo test --features metal -- --ignored`"]
fn an_empty_scene_is_the_clear_colour_exactly() {
    let Some((_lock, _device, mut renderer)) = try_renderer() else {
        return;
    };
    let clear = Color::rgba(20, 30, 40, 255);
    let mut planner = Planner::new();
    let plan = planner.plan(&Scene::default(), WIDTH as f32, HEIGHT as f32);
    assert!(plan.is_complete(), "an empty scene has nothing unsupported");

    let pixels = renderer
        .render_planned(&planner, &plan, WIDTH, HEIGHT, clear)
        .expect("render should succeed");

    for pixel in pixels.as_chunks::<4>().0 {
        assert_eq!(pixel, &[20, 30, 40, 255], "every pixel is the clear colour");
    }
}

#[test]
#[ignore = "needs a Metal GPU — the framework-checks macOS runner; run with `cargo test --features metal -- --ignored`"]
fn one_rectangle_lands_where_the_geometry_says() {
    let Some((_lock, _device, mut renderer)) = try_renderer() else {
        return;
    };
    let base = Color::WHITE;
    let mut scene = Scene::default();
    fill_rect(
        &mut scene,
        Rect::new(20.0, 20.0, 80.0, 60.0),
        Color::rgba(255, 0, 0, 255),
    );
    let mut planner = Planner::new();
    let plan = planner.plan(&scene, WIDTH as f32, HEIGHT as f32);

    let gpu = renderer
        .render_planned(&planner, &plan, WIDTH, HEIGHT, base)
        .expect("render should succeed");
    let cpu = on_cpu(&scene, base);

    let mismatches = interior_mismatches(&gpu, &cpu, 2);
    assert_eq!(
        mismatches, 0,
        "GPU and CPU must agree away from shape edges; {mismatches} interior pixels differed"
    );
}

#[test]
#[ignore = "needs a Metal GPU — the framework-checks macOS runner; run with `cargo test --features metal -- --ignored`"]
fn overlapping_translucent_fills_composite_like_the_cpu_rasterizer() {
    let Some((_lock, _device, mut renderer)) = try_renderer() else {
        return;
    };
    let base = Color::WHITE;
    let mut scene = Scene::default();
    fill_rect(
        &mut scene,
        Rect::new(10.0, 10.0, 70.0, 60.0),
        Color::rgba(200, 40, 40, 255),
    );
    // Half-transparent, and overlapping the first: this is the case where a
    // wrong blend factor produces a picture that still looks plausible.
    fill_rect(
        &mut scene,
        Rect::new(40.0, 30.0, 110.0, 80.0),
        Color::rgba(40, 80, 220, 128),
    );
    let mut planner = Planner::new();
    let plan = planner.plan(&scene, WIDTH as f32, HEIGHT as f32);
    assert!(plan.is_complete());

    let gpu = renderer
        .render_planned(&planner, &plan, WIDTH, HEIGHT, base)
        .expect("render should succeed");
    let cpu = on_cpu(&scene, base);

    let mismatches = interior_mismatches(&gpu, &cpu, 2);
    assert_eq!(
        mismatches, 0,
        "GPU and CPU must agree away from shape edges; {mismatches} interior pixels differed"
    );
}

#[test]
#[ignore = "needs a Metal GPU — the framework-checks macOS runner; run with `cargo test --features metal -- --ignored`"]
fn a_group_opacity_layer_composites_like_the_cpu_rasterizer() {
    let Some((_lock, _device, mut renderer)) = try_renderer() else {
        return;
    };
    let base = Color::WHITE;
    let mut scene = Scene::default();
    let layer = Rect::new(10.0, 10.0, 100.0, 70.0);
    scene.push_layer(layer, 0.5, vieww_foundation::BlendMode::Normal);
    fill_rect(
        &mut scene,
        Rect::new(20.0, 20.0, 80.0, 60.0),
        Color::rgba(255, 0, 0, 255),
    );
    fill_rect(
        &mut scene,
        Rect::new(40.0, 30.0, 90.0, 55.0),
        Color::rgba(0, 0, 255, 255),
    );
    scene.pop_layer();

    let mut planner = Planner::new();
    let plan = planner.plan(&scene, WIDTH as f32, HEIGHT as f32);
    assert!(
        plan.is_complete(),
        "a plain group-opacity layer is in reach"
    );
    assert!(
        plan.target_depth >= 1,
        "the layer needs an offscreen target"
    );

    let gpu = renderer
        .render_planned(&planner, &plan, WIDTH, HEIGHT, base)
        .expect("render should succeed");
    let cpu = on_cpu(&scene, base);

    let mismatches = interior_mismatches(&gpu, &cpu, 2);
    assert_eq!(
        mismatches, 0,
        "GPU and CPU must agree away from shape edges; {mismatches} interior pixels differed"
    );
}

#[test]
#[ignore = "needs a Metal GPU — the framework-checks macOS runner; run with `cargo test --features metal -- --ignored`"]
fn the_renderer_refuses_a_plan_it_cannot_draw() {
    let Some((_lock, _device, mut renderer)) = try_renderer() else {
        return;
    };
    // A shaped clip: expressible by the CPU rasterizer, not by a scissor —
    // the same refusal the Vulkan suite pins, one backend later.
    let mut clip = Clip::NONE;
    clip.add_rect(Rect::new(10.0, 10.0, 90.0, 70.0));
    let mut rounded = vieww_foundation::Path::new();
    rounded.move_to(vieww_foundation::Offset::new(20.0, 20.0));
    rounded.cubic_to(
        vieww_foundation::Offset::new(60.0, 10.0),
        vieww_foundation::Offset::new(80.0, 40.0),
        vieww_foundation::Offset::new(40.0, 60.0),
    );
    rounded.close();
    clip.add_path(rounded);

    let mut scene = Scene::default();
    scene.push_command(Command::FillRect {
        rect: Rect::new(0.0, 0.0, 120.0, 90.0),
        paint: Paint::solid(Color::rgba(200, 30, 30, 255)),
        transform: Transform::IDENTITY,
        clip,
    });

    let mut planner = Planner::new();
    let plan = planner.plan(&scene, WIDTH as f32, HEIGHT as f32);
    assert!(!plan.is_complete());

    let refused = renderer.render_planned(&planner, &plan, WIDTH, HEIGHT, Color::WHITE);
    assert!(
        refused.is_err(),
        "an incomplete plan must be refused, not partly drawn"
    );
    let message = refused.unwrap_err().to_string();
    assert!(
        message.contains("shaped-clip"),
        "the error should name what stopped it, got: {message}"
    );
}

#[test]
#[ignore = "needs a Metal GPU — the framework-checks macOS runner; run with `cargo test --features metal -- --ignored`"]
fn the_renderer_is_reusable_across_frames_and_sizes() {
    let Some((_lock, _device, mut renderer)) = try_renderer() else {
        return;
    };
    let mut scene = Scene::default();
    fill_rect(
        &mut scene,
        Rect::new(5.0, 5.0, 40.0, 40.0),
        Color::rgba(255, 255, 0, 255),
    );
    let mut planner = Planner::new();
    let plan = planner.plan(&scene, WIDTH as f32, HEIGHT as f32);

    // Several frames at one size: the point of the persistent pipelines.
    for _ in 0..8 {
        let pixels = renderer
            .render_planned(&planner, &plan, WIDTH, HEIGHT, Color::rgba(0, 0, 0, 255))
            .expect("every frame should render");
        assert_eq!(pixels.len(), (WIDTH * HEIGHT * 4) as usize);
    }

    // Then a resize, which must rebuild only the size-dependent objects.
    let bigger = renderer
        .render_planned(
            &planner,
            &plan,
            WIDTH * 2,
            HEIGHT * 2,
            Color::rgba(0, 0, 0, 255),
        )
        .expect("a resize should not break the renderer");
    assert_eq!(bigger.len(), (WIDTH * 2 * HEIGHT * 2 * 4) as usize);

    // And back down again, which is the case a naive cache gets wrong.
    let smaller = renderer
        .render_planned(
            &planner,
            &plan,
            WIDTH / 2,
            HEIGHT / 2,
            Color::rgba(0, 0, 0, 255),
        )
        .expect("shrinking should work too");
    assert_eq!(smaller.len(), (WIDTH / 2 * (HEIGHT / 2) * 4) as usize);
}

#[test]
#[ignore = "needs a Metal GPU — the framework-checks macOS runner; run with `cargo test --features metal -- --ignored`"]
fn a_damaged_frame_keeps_the_previous_frame_outside_the_damage() {
    let Some((_lock, _device, mut renderer)) = try_renderer() else {
        return;
    };
    let base = Color::WHITE;
    let mut first = Scene::default();
    fill_rect(
        &mut first,
        Rect::new(0.0, 0.0, 120.0, 90.0),
        Color::rgba(30, 30, 200, 255),
    );
    fill_rect(
        &mut first,
        Rect::new(10.0, 10.0, 40.0, 40.0),
        Color::rgba(255, 0, 0, 255),
    );
    let mut planner = Planner::new();
    let plan_first = planner.plan(&first, WIDTH as f32, HEIGHT as f32);
    let whole_first = renderer
        .render_planned(&planner, &plan_first, WIDTH, HEIGHT, base)
        .expect("the first frame should render");

    // The same scene with only the small square moved, and a damage rect
    // that covers exactly the square's old position — not the new one.
    let mut second = Scene::default();
    fill_rect(
        &mut second,
        Rect::new(0.0, 0.0, 120.0, 90.0),
        Color::rgba(30, 30, 200, 255),
    );
    fill_rect(
        &mut second,
        Rect::new(60.0, 50.0, 90.0, 80.0),
        Color::rgba(255, 0, 0, 255),
    );
    let plan_second = planner.plan(&second, WIDTH as f32, HEIGHT as f32);
    let damage = [vieww_gpu::PixelRect {
        x0: 5,
        y0: 5,
        x1: 45,
        y1: 45,
    }];
    let (damaged_pixels, redrawn) = renderer
        .render_planned_damaged(&planner, &plan_second, WIDTH, HEIGHT, base, Some(&damage))
        .expect("the damaged frame should render");
    assert_eq!(
        redrawn.pixels(WIDTH, HEIGHT),
        40 * 40,
        "the damage is the only thing the frame redrew"
    );

    // The retained contract, in bytes. Outside the damage the target was
    // kept untouched, so it holds *exactly* the first frame's pixels — the
    // same target, the same bytes.
    for y in 0..HEIGHT as usize {
        for x in 0..WIDTH as usize {
            let inside = (5..45).contains(&x) && (5..45).contains(&y);
            if inside {
                continue;
            }
            let index = (y * WIDTH as usize + x) * 4;
            assert_eq!(
                &damaged_pixels[index..index + 4],
                &whole_first[index..index + 4],
                "pixel ({x}, {y}) outside the damage is the previous frame, byte for byte"
            );
        }
    }
    // Inside the damage the second scene was drawn — and the second scene
    // there is plain background, so the old square is genuinely gone.
    for y in 5..45usize {
        for x in 5..45usize {
            let index = (y * WIDTH as usize + x) * 4;
            assert_eq!(
                &damaged_pixels[index..index + 4],
                &[30, 30, 200, 255],
                "pixel ({x}, {y}) inside the damage is the new scene's background"
            );
        }
    }
}
