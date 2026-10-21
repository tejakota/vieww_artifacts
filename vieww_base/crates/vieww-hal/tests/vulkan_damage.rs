//! `SceneRenderer` redrawing only a frame's damage, against itself redrawing
//! everything.
//!
//! # What "right" means here
//!
//! A damaged frame keeps the previous frame's pixels outside the damage and
//! redraws inside it. If that is correct, it is *the same frame* a full
//! redraw of the new scene produces — byte for byte, because it is the same
//! renderer on the same device running the same shaders over the same
//! pixels. So unlike the CPU-parity suites there is no tolerance: any
//! difference is a pixel the damaged path got wrong.
//!
//! The scenes are built to break the ways clipping can go wrong: a blurred
//! layer and a shadow straddling the damage (their blurs read pixels outside
//! it), a blend-mode layer (it snapshots the frame and reads it back), and a
//! backdrop filter both inside the damage and reaching outside it (outside,
//! the frame target holds last frame's *finished* picture, so the renderer
//! must refuse the damage and redraw everything).
//!
//! `#[ignore]`d like the other Vulkan suites: run with
//! `cargo test -p vieww-hal --features vulkan -- --ignored --test-threads=1`.

#![cfg(feature = "vulkan")]

use vieww_foundation::{BlendMode, Color, ImageFilter, Offset, Path, Rect, Shadow, Transform};
use vieww_gpu::{PixelRect, Planner};
use vieww_hal::vulkan::{Redrawn, SceneRenderer, VulkanDevice};
use vieww_paint::{Clip, Command, Paint, Scene};

const WIDTH: u32 = 128;
const HEIGHT: u32 = 96;

/// One Vulkan device at a time — see `vulkan_scene.rs` for the lavapipe
/// crash this guards against.
static ONE_DEVICE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Gpu {
    _lock: std::sync::MutexGuard<'static, ()>,
    renderer: SceneRenderer,
    _device: VulkanDevice,
}

fn open_gpu() -> Option<Gpu> {
    let lock = ONE_DEVICE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let device = match VulkanDevice::new() {
        Ok(device) => device,
        Err(error) => {
            eprintln!("skipping: no Vulkan device available ({error})");
            return None;
        }
    };
    let renderer = SceneRenderer::new(&device).expect("the pipelines should build");
    Some(Gpu {
        _lock: lock,
        renderer,
        _device: device,
    })
}

fn fill(scene: &mut Scene, rect: Rect, color: Color) {
    scene.push_command(Command::FillRect {
        rect,
        paint: Paint::solid(color),
        transform: Transform::IDENTITY,
        clip: Clip::NONE,
    });
}

fn push_layer(scene: &mut Scene, bounds: Rect, blend: BlendMode, filter: ImageFilter, clip: Clip) {
    scene.push_command(Command::PushLayer {
        bounds,
        alpha: 0.9,
        blend,
        clip,
        filter,
    });
}

fn rounded_clip(rect: Rect, radius: f32) -> Clip {
    let mut clip = Clip::NONE;
    clip.add_path(Path::rounded_rect(rect, radius));
    clip
}

/// A checker of saturated colours, so a blur or blend that reads the wrong
/// pixels shows it.
fn checker(scene: &mut Scene) {
    for row in 0..6 {
        for col in 0..8 {
            let color = match (row + col) % 4 {
                0 => Color::rgba(230, 60, 40, 255),
                1 => Color::rgba(40, 170, 90, 255),
                2 => Color::rgba(50, 90, 220, 255),
                _ => Color::rgba(240, 220, 60, 255),
            };
            #[expect(clippy::cast_precision_loss, reason = "small grid indices")]
            let (x, y) = (col as f32 * 16.0, row as f32 * 16.0);
            fill(scene, Rect::new(x, y, x + 16.0, y + 16.0), color);
        }
    }
}

/// Where the moving chip is drawn in each frame.
const CHIP_BEFORE: Rect = Rect::new(8.0, 60.0, 40.0, 84.0);
const CHIP_AFTER: Rect = Rect::new(44.0, 30.0, 76.0, 54.0);

/// The test scene, with the chip at `chip`. `backdrop` places a
/// backdrop-blur panel; everything else is the same in every frame.
fn scene(chip: Rect, backdrop: Option<Rect>) -> Scene {
    let mut scene = Scene::default();
    checker(&mut scene);

    // A blurred layer straddling the chip's new position.
    push_layer(
        &mut scene,
        Rect::new(60.0, 4.0, 124.0, 50.0),
        BlendMode::Normal,
        ImageFilter::blur(3.0),
        Clip::NONE,
    );
    fill(
        &mut scene,
        Rect::new(68.0, 10.0, 116.0, 40.0),
        Color::rgba(20, 20, 30, 255),
    );
    scene.push_command(Command::PopLayer);

    // A shadow straddling it too.
    scene.push_command(Command::DrawShadow {
        rect: Rect::new(30.0, 8.0, 58.0, 36.0),
        radius: 6.0,
        shadow: Shadow::new(Color::rgba(10, 20, 40, 160), Offset::new(0.0, 6.0), 10.0),
        transform: Transform::IDENTITY,
        clip: Clip::NONE,
    });

    // A blend-mode layer, which snapshots the frame and reads it back.
    push_layer(
        &mut scene,
        Rect::new(40.0, 44.0, 96.0, 92.0),
        BlendMode::Multiply,
        ImageFilter::NONE,
        Clip::NONE,
    );
    fill(
        &mut scene,
        Rect::new(44.0, 48.0, 92.0, 88.0),
        Color::rgba(250, 140, 30, 200),
    );
    scene.push_command(Command::PopLayer);

    // A top-level layer far from every change: a damaged frame skips it.
    push_layer(
        &mut scene,
        Rect::new(100.0, 70.0, 124.0, 92.0),
        BlendMode::Normal,
        ImageFilter::NONE,
        Clip::NONE,
    );
    fill(
        &mut scene,
        Rect::new(102.0, 72.0, 122.0, 90.0),
        Color::rgba(255, 255, 255, 255),
    );
    scene.push_command(Command::PopLayer);

    fill(&mut scene, chip, Color::rgba(250, 250, 250, 255));

    if let Some(panel) = backdrop {
        push_layer(
            &mut scene,
            panel,
            BlendMode::Normal,
            ImageFilter::backdrop_blur(3.0),
            rounded_clip(panel, 6.0),
        );
        fill(&mut scene, panel, Color::rgba(255, 255, 255, 80));
        scene.push_command(Command::PopLayer);
    }
    scene
}

/// The chip's old and new rectangles, rounded out with a margin for
/// antialiasing — what a frame's damage would name for that move.
fn chip_damage() -> Vec<PixelRect> {
    [CHIP_BEFORE, CHIP_AFTER]
        .into_iter()
        .map(|r| {
            PixelRect::round_out(Rect::new(
                r.left - 2.0,
                r.top - 2.0,
                r.right + 2.0,
                r.bottom + 2.0,
            ))
        })
        .collect()
}

fn render(
    gpu: &mut Gpu,
    planner: &mut Planner,
    scene: &Scene,
    clear: Color,
    damage: Option<&[PixelRect]>,
) -> (Vec<u8>, Redrawn) {
    #[expect(clippy::cast_precision_loss, reason = "small test extents")]
    let plan = planner.plan(scene, WIDTH as f32, HEIGHT as f32);
    assert!(plan.is_complete(), "plan has gaps {:?}", plan.unsupported);
    gpu.renderer
        .render_planned_damaged(planner, &plan, WIDTH, HEIGHT, clear, damage)
        .expect("the frame should render")
}

/// Pixels where `a` and `b` differ, and the first one.
fn differences(a: &[u8], b: &[u8]) -> (usize, Option<(usize, usize)>) {
    let mut count = 0;
    let mut first = None;
    for (i, (x, y)) in a
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0)
        .enumerate()
    {
        if x != y {
            count += 1;
            first.get_or_insert((i % WIDTH as usize, i / WIDTH as usize));
        }
    }
    (count, first)
}

/// Frame one in full, frame two with `damage`; then frame two in full on a
/// fresh renderer. Returns what the damaged frame redrew.
fn damaged_matches_full(
    name: &str,
    before: &Scene,
    after: &Scene,
    damage: &[PixelRect],
) -> Redrawn {
    let clear = Color::WHITE;
    let Some(mut gpu) = open_gpu() else {
        return Redrawn::Full;
    };
    let mut planner = Planner::new();
    let (_, first) = render(&mut gpu, &mut planner, before, clear, None);
    assert_eq!(
        first,
        Redrawn::Full,
        "{name}: a first frame redraws everything"
    );
    let (damaged, redrawn) = render(&mut gpu, &mut planner, after, clear, Some(damage));
    drop(gpu);

    let Some(mut fresh) = open_gpu() else {
        return redrawn;
    };
    let mut planner = Planner::new();
    let (full, _) = render(&mut fresh, &mut planner, after, clear, None);
    let (count, first) = differences(&damaged, &full);
    assert_eq!(
        count, 0,
        "{name}: the damaged frame differs from a full redraw at {count} pixels, first at {first:?}"
    );
    redrawn
}

#[test]
#[ignore = "needs a Vulkan loader + ICD; run with `cargo test -p vieww-hal --features vulkan -- --ignored --test-threads=1`"]
fn a_damaged_frame_is_the_full_frame_through_blurs_shadows_and_blends() {
    let damage = chip_damage();
    let redrawn = damaged_matches_full(
        "moving chip",
        &scene(CHIP_BEFORE, None),
        &scene(CHIP_AFTER, None),
        &damage,
    );
    let area: u64 = damage
        .iter()
        .map(|r| u64::from(r.width()) * u64::from(r.height()))
        .sum();
    assert_eq!(
        redrawn,
        Redrawn::Damage { pixels: area },
        "the damage was usable, so only it should have been redrawn"
    );
}

#[test]
#[ignore = "needs a Vulkan loader + ICD; run with `cargo test -p vieww-hal --features vulkan -- --ignored --test-threads=1`"]
fn a_backdrop_filter_inside_the_damage_keeps_it() {
    // The panel sits wholly inside one damage rectangle, over the chip's
    // new position: what it samples is all redrawn this frame.
    let panel = Rect::new(48.0, 34.0, 72.0, 50.0);
    let redrawn = damaged_matches_full(
        "backdrop inside",
        &scene(CHIP_BEFORE, Some(panel)),
        &scene(CHIP_AFTER, Some(panel)),
        &chip_damage(),
    );
    assert!(
        matches!(redrawn, Redrawn::Damage { .. }),
        "a backdrop wholly inside the damage should not force a full redraw: {redrawn:?}"
    );
}

#[test]
#[ignore = "needs a Vulkan loader + ICD; run with `cargo test -p vieww-hal --features vulkan -- --ignored --test-threads=1`"]
fn a_backdrop_filter_reaching_outside_the_damage_redraws_everything() {
    // The panel overlaps the damage but extends past it: outside the damage
    // the frame target holds last frame's finished picture, which a backdrop
    // must not sample.
    let panel = Rect::new(30.0, 20.0, 110.0, 70.0);
    let redrawn = damaged_matches_full(
        "backdrop outside",
        &scene(CHIP_BEFORE, Some(panel)),
        &scene(CHIP_AFTER, Some(panel)),
        &chip_damage(),
    );
    assert_eq!(redrawn, Redrawn::Full);
}

#[test]
#[ignore = "needs a Vulkan loader + ICD; run with `cargo test -p vieww-hal --features vulkan -- --ignored --test-threads=1`"]
fn pixels_outside_the_damage_are_last_frames() {
    // The proof that damage is honoured rather than ignored: with nothing
    // damaged, a frame of a *different* scene must come out as the previous
    // frame, untouched.
    let Some(mut gpu) = open_gpu() else {
        return;
    };
    let mut planner = Planner::new();
    let (before, _) = render(
        &mut gpu,
        &mut planner,
        &scene(CHIP_BEFORE, None),
        Color::WHITE,
        None,
    );
    let (after, redrawn) = render(
        &mut gpu,
        &mut planner,
        &scene(CHIP_AFTER, None),
        Color::WHITE,
        Some(&[]),
    );
    assert_eq!(redrawn, Redrawn::Damage { pixels: 0 });
    assert_eq!(
        differences(&before, &after).0,
        0,
        "nothing was damaged, so nothing may change"
    );
}

#[test]
#[ignore = "needs a Vulkan loader + ICD; run with `cargo test -p vieww-hal --features vulkan -- --ignored --test-threads=1`"]
fn a_new_clear_colour_or_size_redraws_everything() {
    let Some(mut gpu) = open_gpu() else {
        return;
    };
    let mut planner = Planner::new();
    let frame = scene(CHIP_BEFORE, None);
    let damage = chip_damage();
    render(&mut gpu, &mut planner, &frame, Color::WHITE, None);
    let (_, redrawn) = render(&mut gpu, &mut planner, &frame, Color::BLACK, Some(&damage));
    assert_eq!(
        redrawn,
        Redrawn::Full,
        "a new clear colour changes every pixel"
    );

    // A different size: the targets are rebuilt and hold nothing.
    #[expect(clippy::cast_precision_loss, reason = "small test extents")]
    let plan = planner.plan(&frame, (WIDTH / 2) as f32, (HEIGHT / 2) as f32);
    let (_, redrawn) = gpu
        .renderer
        .render_planned_damaged(
            &planner,
            &plan,
            WIDTH / 2,
            HEIGHT / 2,
            Color::BLACK,
            Some(&damage),
        )
        .expect("the frame should render");
    assert_eq!(
        redrawn,
        Redrawn::Full,
        "a resized frame has nothing to keep"
    );
}

#[test]
#[ignore = "needs a Vulkan loader + ICD; run with `cargo test -p vieww-hal --features vulkan -- --ignored --test-threads=1`"]
fn overlapping_damage_rectangles_draw_each_pixel_once() {
    // Damage rectangles from a frame overlap freely. Their overlap runs
    // through the translucent Multiply layer: a renderer that draws once per
    // rectangle blends those pixels twice.
    let damage = [
        PixelRect::round_out(Rect::new(40.0, 26.0, 80.0, 58.0)),
        PixelRect::round_out(Rect::new(60.0, 40.0, 100.0, 70.0)),
        PixelRect::round_out(Rect::new(6.0, 58.0, 42.0, 86.0)),
    ];
    let redrawn = damaged_matches_full(
        "overlapping damage",
        &scene(CHIP_BEFORE, None),
        &scene(CHIP_AFTER, None),
        &damage,
    );
    // Each pixel counted once: the union, not the sum.
    let union = 40 * 32 + 40 * 30 - 20 * 18 + 36 * 28;
    assert_eq!(redrawn, Redrawn::Damage { pixels: union });
}

#[test]
#[ignore = "needs a Vulkan loader + ICD; run with `cargo test -p vieww-hal --features vulkan -- --ignored --test-threads=1`"]
fn gpu_time_is_split_by_the_kind_of_work_that_spent_it() {
    let Some(mut gpu) = open_gpu() else { return };
    let mut planner = Planner::new();

    // Blurred layer, shadow, blend layer, plain geometry: every kind.
    render(
        &mut gpu,
        &mut planner,
        &scene(CHIP_AFTER, None),
        Color::BLACK,
        None,
    );
    let Some(busy) = gpu.renderer.last_pass_times() else {
        eprintln!("skipping: this queue records no timestamps");
        return;
    };
    let cpu = gpu.renderer.last_timings();
    assert!(busy.filters > std::time::Duration::ZERO, "{busy:?}");
    assert!(busy.shadows > std::time::Duration::ZERO, "{busy:?}");
    assert!(busy.layers > std::time::Duration::ZERO, "{busy:?}");
    // The GPU's time for the frame lies inside the CPU's view of it: from
    // the submit to the fence. A millisecond of slack for the two clocks.
    assert!(
        busy.total() <= cpu.submit + cpu.wait + std::time::Duration::from_millis(1),
        "GPU {busy:?} against CPU submit {:?} + wait {:?}",
        cpu.submit,
        cpu.wait
    );

    // Plain geometry only: nothing may be charged to filters or shadows.
    let mut plain = Scene::default();
    checker(&mut plain);
    render(&mut gpu, &mut planner, &plain, Color::BLACK, None);
    let quiet = gpu.renderer.last_pass_times().expect("measured above");
    assert_eq!(quiet.filters, std::time::Duration::ZERO, "{quiet:?}");
    assert_eq!(quiet.shadows, std::time::Duration::ZERO, "{quiet:?}");
    assert_eq!(quiet.layers, std::time::Duration::ZERO, "{quiet:?}");
    assert!(quiet.draw > std::time::Duration::ZERO, "{quiet:?}");
}
