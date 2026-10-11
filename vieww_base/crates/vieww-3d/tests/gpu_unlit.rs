//! `GpuRenderer` against a real Vulkan device, and against the CPU
//! rasterizer that is this crate's reference for what a frame looks like.
//!
//! # What is being asserted, and why it is shaped this way
//!
//! The GPU path's whole job is to be **held to the CPU's pixels** — that is
//! the "CPU renderer as the pixel oracle" line in `TODO.md`, and it is the
//! same assertion `vieww-hal`'s own `vulkan_scene` suite makes for the 2D
//! path. Two rasterizers will never agree on the *edge* pixels of a
//! silhouette (sample positions differ by design), so the meaningful claim
//! is: **interiors match, and disagreements hug boundaries.** The
//! `interior_mismatches` helper below is that claim, written the same way
//! `vulkan_scene.rs` writes it — edge classification comes from the
//! reference image, never from the diff (see that file for the mutation
//! experiment that explains why).
//!
//! # `#[ignore]`, one device, one thread
//!
//! Same reasons as every other Vulkan suite: the tests need a loader and an
//! ICD (`lavapipe` or real hardware), and concurrent devices on Mesa's
//! software rasterizer SIGSEGV inside `libvulkan_lvp.so` — run with
//! `cargo test -p vieww-3d --features vulkan -- --ignored --test-threads=1`.

#![cfg(feature = "vulkan")]

use vieww_3d::geometry;
use vieww_3d::gpu::GpuRenderer;
use vieww_3d::math::{Quat, Vec3};
use vieww_3d::render::Renderer;
use vieww_3d::scene::{Camera, Content, Material, Node, Rgb, Scene};
use vieww_foundation::Color;

const WIDTH: u32 = 160;
const HEIGHT: u32 = 120;

/// One Vulkan device at a time, for the whole file (see the module doc).
static ONE_DEVICE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn try_gpu() -> Option<(std::sync::MutexGuard<'static, ()>, GpuRenderer)> {
    let guard = ONE_DEVICE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let renderer = GpuRenderer::new(WIDTH, HEIGHT);
    if !renderer.is_gpu() {
        eprintln!("skipping: no Vulkan device available");
        return None;
    }
    Some((guard, renderer))
}

/// Count pixels where `gpu` and `cpu` disagree, excluding pixels the
/// reference itself has a coverage/colour transition at — an edge is a
/// property of the picture, not of the diff. Mirrors `vulkan_scene.rs`'s
/// helper of the same name (it cannot be imported from a sibling crate's
/// test binary; if the two ever drift, the drift is itself a bug).
fn interior_mismatches(gpu: &[u8], cpu: &[u8], tolerance: u8) -> usize {
    assert_eq!(gpu.len(), cpu.len(), "same dimensions");
    let w = WIDTH as usize;
    let h = HEIGHT as usize;
    let channel = |b: &[u8], i: usize, c: usize| b[i * 4 + c];
    let differs =
        |i: usize| (0..4).any(|c| channel(gpu, i, c).abs_diff(channel(cpu, i, c)) > tolerance);
    let reference_edge = |x: usize, y: usize| {
        let i = y * w + x;
        (-1i64..=1i64).any(|dy| {
            (-1i64..=1i64).any(|dx| {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                    return false;
                }
                let n = ny as usize * w + nx as usize;
                (0..4).any(|c| channel(cpu, i, c).abs_diff(channel(cpu, n, c)) > 8)
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

/// The oracle: the CPU renderer at `samples = 1`. Supersampling is the one
/// setting where the two rasterizers are *allowed* to differ (it is a CPU
/// resolve, not a GPU one), so parity is asserted at the shared setting.
fn on_cpu(scene: &mut Scene, camera: &Camera) -> Vec<u8> {
    let (image, _) = Renderer::new(WIDTH, HEIGHT)
        .samples(1)
        .render(scene, camera);
    image.pixels().to_vec()
}

/// Three boxes a camera can see at once: pure primary colours exercise the
/// sRGB curve's straight and gamma segments at once, one box is rotated so
/// its faces are not axis-aligned, and two boxes overlap in screen space so
/// the depth test has a contest to arbitrate.
fn box_scene() -> (Scene, Camera) {
    let mut scene = Scene::new();
    scene.background = Rgb::new(0.05, 0.1, 0.2);

    let mut red = Node::new(
        "red",
        Content::mesh(
            geometry::box_mesh(1.0, 1.0, 1.0),
            Material::basic(Color::RED),
        ),
    );
    red.position = Vec3::new(-1.1, 0.0, 0.0);
    scene.add(red, None);

    // Mid-grey (0.5 linear) rather than a `Color::GRAY` constant that does
    // not exist: 0.5 sits on the sRGB curve's gamma segment, where a
    // wrong-side-of-the-curve bug actually shows.
    let mut grey = Node::new(
        "grey",
        Content::mesh(
            geometry::box_mesh(1.0, 1.4, 1.0),
            Material::basic(Color::rgb(128, 128, 128)),
        ),
    );
    grey.position = Vec3::new(0.9, 0.1, -1.0);
    scene.add(grey, None);

    // Rotated: its faces land at angles no axis-aligned box produces, so a
    // mis-y-flip or mis-remapped depth range moves interiors, not edges.
    let mut white = Node::new(
        "white",
        Content::mesh(
            geometry::box_mesh(0.9, 0.9, 0.9),
            Material::basic(Color::WHITE),
        ),
    );
    white.position = Vec3::new(0.2, 0.3, 1.2);
    white.rotation = Quat::from_euler(0.6, 0.5, 0.2);
    scene.add(white, None);

    let camera = Camera::perspective(Vec3::new(0.0, 1.6, 4.2), Vec3::ZERO, 0.9);
    (scene, camera)
}

#[test]
#[ignore = "needs a Vulkan loader + ICD (lavapipe or a real GPU); run with `cargo test --features vulkan -- --ignored`"]
fn the_unlit_pass_matches_the_cpu_oracle_in_interiors() {
    let Some((_lock, gpu)) = try_gpu() else {
        return;
    };
    let (mut scene, camera) = box_scene();

    let (image, stats) = gpu.render(&mut scene, &camera);
    let cpu = on_cpu(&mut scene, &camera);

    assert_eq!(image.width(), WIDTH);
    assert_eq!(image.height(), HEIGHT);
    let mismatches = interior_mismatches(image.pixels(), &cpu, 2);
    assert_eq!(
        mismatches, 0,
        "GPU and CPU must agree away from silhouettes; {mismatches} interior pixels differed"
    );
    // The packing pass counts what it packed, and only what it packed.
    assert_eq!(stats.objects, 3);
    assert_eq!(stats.triangles, 36);
    assert_eq!(stats.culled_objects, 0);
}

#[test]
#[ignore = "needs a Vulkan loader + ICD (lavapipe or a real GPU); run with `cargo test --features vulkan -- --ignored`"]
fn an_empty_scene_is_the_encoded_background() {
    let Some((_lock, gpu)) = try_gpu() else {
        return;
    };
    let mut scene = Scene::new();
    scene.background = Rgb::new(0.05, 0.1, 0.2);
    let camera = Camera::perspective(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, 0.8);

    let (image, _) = gpu.render(&mut scene, &camera);
    let cpu = on_cpu(&mut scene, &camera);

    // Exactly, not approximately: nothing was drawn, so the only pixels are
    // the clear colour's, and the clear and `to_image` must encode it the
    // same way.
    assert_eq!(image.pixels(), &cpu[..]);
}

#[test]
#[ignore = "needs a Vulkan loader + ICD (lavapipe or a real GPU); run with `cargo test --features vulkan -- --ignored`"]
fn frustum_culled_geometry_is_not_packed_but_still_matches() {
    let Some((_lock, gpu)) = try_gpu() else {
        return;
    };
    let (mut scene, camera) = box_scene();
    // Behind the camera (which sits at z = 4.2 looking toward the origin):
    // the CPU counts it culled, the GPU pass must agree in its stats and in
    // its pixels (nothing of it can be drawn).
    let id = scene.add(
        Node::new(
            "behind",
            Content::mesh(
                geometry::box_mesh(1.0, 1.0, 1.0),
                Material::basic(Color::BLUE),
            ),
        ),
        None,
    );
    scene.node_mut(id).position = Vec3::new(0.0, 0.0, 12.0);

    let (image, stats) = gpu.render(&mut scene, &camera);
    let cpu = on_cpu(&mut scene, &camera);

    assert_eq!(stats.objects, 4);
    assert_eq!(stats.culled_objects, 1);
    let mismatches = interior_mismatches(image.pixels(), &cpu, 2);
    assert_eq!(
        mismatches, 0,
        "a culled box changes neither image; {mismatches} interior pixels differed"
    );
}
