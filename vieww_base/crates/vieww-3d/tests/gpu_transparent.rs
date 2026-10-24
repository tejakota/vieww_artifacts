//! The transparent pass of `GpuRenderer` against the CPU oracle: a
//! translucent surface blending over an opaque one, two translucent
//! surfaces stacking in depth order, and the property the whole split
//! exists for — a translucent fragment does not write depth, so what is
//! behind it shows through, while an opaque fragment of the *same* item
//! still does.
//!
//! Same claim and same helper as `gpu_lit.rs` — **interiors match**, and
//! disagreements may only sit where the reference image itself has an edge.
//!
//! Run with `cargo test -p vieww-3d --features vulkan -- --ignored --test-threads=1`.

#![cfg(feature = "vulkan")]

use vieww_3d::geometry;
use vieww_3d::gpu::GpuRenderer;
use vieww_3d::math::{Quat, Vec3};
use vieww_3d::render::Renderer;
use vieww_3d::scene::{Camera, Content, Light, Material, Node, Rgb, Scene};
use vieww_foundation::Color;

const WIDTH: u32 = 160;
const HEIGHT: u32 = 120;

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

/// `gpu_unlit.rs`'s helper, verbatim (a sibling test binary cannot import it).
fn interior_mismatches(gpu: &[u8], cpu: &[u8], tolerance: u8) -> usize {
    assert_eq!(gpu.len(), cpu.len(), "same dimensions");
    let w = WIDTH as usize;
    let h = HEIGHT as usize;
    let channel = |b: &[u8], i: usize, c: usize| b[i * 4 + c];
    let differs =
        |i: usize| (0..4).any(|c| channel(gpu, i, c).abs_diff(channel(cpu, i, c)) > tolerance);
    let reference_edge = |x: usize, y: usize| {
        let i = y * w + x;
        (-1i64..=1).any(|dy| {
            (-1i64..=1).any(|dx| {
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

fn on_cpu(scene: &mut Scene, camera: &Camera) -> Vec<u8> {
    let (image, _) = Renderer::new(WIDTH, HEIGHT)
        .samples(1)
        .render(scene, camera);
    image.pixels().to_vec()
}

fn sun(scene: &mut Scene) {
    let mut node = Node::new(
        "sun",
        Content::Light(Light::Directional {
            color: Rgb::WHITE,
            intensity: 1.0,
            direction: Vec3::new(-0.35, -0.8, -0.45),
            shadow: false,
        }),
    );
    node.position = Vec3::new(0.0, 4.0, 0.0);
    scene.add(node, None);
}

fn pane(
    scene: &mut Scene,
    name: &str,
    color: Color,
    opacity: f32,
    at: Vec3,
) -> vieww_3d::scene::NodeId {
    let mut material = Material::lambert(color);
    material.opacity = opacity;
    let mut node = Node::new(
        name,
        Content::mesh(geometry::plane(2.0, 2.0, 1, 1), material),
    );
    node.position = at;
    node.rotation = Quat::from_euler(-std::f32::consts::FRAC_PI_2, 0.0, 0.0);
    scene.add(node, None)
}

#[test]
#[ignore = "needs a Vulkan loader + ICD (lavapipe or a real GPU); run with `cargo test --features vulkan -- --ignored`"]
fn a_translucent_pane_blends_over_the_opaque_scene_like_the_cpu() {
    let Some((_lock, gpu)) = try_gpu() else {
        return;
    };
    let mut scene = Scene::new();
    scene.background = Rgb::new(0.05, 0.05, 0.08);
    sun(&mut scene);
    // The pane is added FIRST, in front of the box: the CPU draws the
    // opaque box before any transparent item regardless of scene order, so
    // the GPU's batch order must too.
    pane(
        &mut scene,
        "pane",
        Color::rgb(230, 60, 200),
        0.45,
        Vec3::new(0.0, 0.9, -0.2),
    );
    let mut node = Node::new(
        "box",
        Content::mesh(
            geometry::box_mesh(1.0, 1.0, 1.0),
            Material::lambert(Color::rgb(60, 200, 90)),
        ),
    );
    node.position = Vec3::new(0.0, 0.5, 0.6);
    scene.add(node, None);
    let camera = Camera::perspective(Vec3::new(0.0, 2.4, 3.4), Vec3::new(0.0, 0.4, 0.0), 0.9);

    // Anti-vacuity: the pane must actually blend — its translucent render
    // differs from an opaque one in many pixels, and the box must show
    // through the blend.
    let cpu = on_cpu(&mut scene, &camera);
    let mut opaque_pane = scene.clone();
    let pane_id = opaque_pane.find("pane").expect("the pane exists");
    if let Content::Mesh { material, .. } = &mut opaque_pane.node_mut(pane_id).content {
        let mut opaque = Material::lambert(Color::rgb(230, 60, 200));
        opaque.opacity = 1.0;
        *material = std::sync::Arc::new(opaque);
    }
    let (opaque_image, _) = Renderer::new(WIDTH, HEIGHT)
        .samples(1)
        .render(&mut opaque_pane, &camera);
    let blended = cpu
        .as_chunks::<4>()
        .0
        .iter()
        .zip(opaque_image.pixels().as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count();
    // The CPU oracle blends 1170 pixels of this 160x120 frame (about 6%).
    // That count is the CPU renderer's alone, so it is the same on every
    // machine; the floor sits below it with room for a rounding change and
    // far above the zero a pane that stopped blending would give. (It was
    // 1500, which this scene never reaches — unseen while the test could
    // only run where a Vulkan device exists.)
    assert!(
        blended > 1000,
        "the pane must actually blend ({blended} px differ from the opaque pane)"
    );

    let (image, _) = gpu.render(&mut scene, &camera);
    let mismatches = interior_mismatches(image.pixels(), &cpu, 2);
    assert_eq!(
        mismatches, 0,
        "translucent pane: {mismatches} interior pixels differ between GPU and CPU"
    );
}

#[test]
#[ignore = "needs a Vulkan loader + ICD (lavapipe or a real GPU); run with `cargo test --features vulkan -- --ignored`"]
fn stacked_translucent_panes_composite_in_depth_order_like_the_cpu() {
    let Some((_lock, gpu)) = try_gpu() else {
        return;
    };
    let mut scene = Scene::new();
    scene.background = Rgb::BLACK;
    sun(&mut scene);
    // Two overlapping panes at different depths, plus one nearer than the
    // box: three translucent surfaces in the same frame, so the far-to-near
    // order — and equal-depth ties — is the correctness point. A wrong
    // order blends the same colours into visibly different ones.
    pane(
        &mut scene,
        "far",
        Color::rgb(220, 40, 40),
        0.5,
        Vec3::new(0.0, 1.1, 0.5),
    );
    pane(
        &mut scene,
        "near",
        Color::rgb(40, 60, 230),
        0.6,
        Vec3::new(0.0, 1.2, 0.1),
    );
    let mut node = Node::new(
        "box",
        Content::mesh(
            geometry::box_mesh(1.4, 0.4, 1.0),
            Material::lambert(Color::rgb(240, 230, 80)),
        ),
    );
    node.position = Vec3::new(0.0, 0.2, -0.6);
    scene.add(node, None);
    let camera = Camera::perspective(Vec3::new(0.0, 2.6, 3.4), Vec3::new(0.0, 0.6, 0.0), 0.9);

    // Anti-vacuity: the stacking order must matter — swapping the two
    // panes' colours must change the picture (the two orders produce
    // different colours wherever both panes overlap).
    let cpu = on_cpu(&mut scene, &camera);
    let mut swapped = scene.clone();
    for (name, color) in [
        ("far", Color::rgb(40, 60, 230)),
        ("near", Color::rgb(220, 40, 40)),
    ] {
        let id = swapped.find(name).expect("the pane exists");
        if let Content::Mesh { material, .. } = &mut swapped.node_mut(id).content {
            let mut pane = Material::lambert(color);
            pane.opacity = material.opacity;
            *material = std::sync::Arc::new(pane);
        }
    }
    let (swapped_image, _) = Renderer::new(WIDTH, HEIGHT)
        .samples(1)
        .render(&mut swapped, &camera);
    let order_matters = cpu
        .as_chunks::<4>()
        .0
        .iter()
        .zip(swapped_image.pixels().as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        order_matters > 300,
        "the stacking order must be visible ({order_matters} px)"
    );

    let (image, _) = gpu.render(&mut scene, &camera);
    let mismatches = interior_mismatches(image.pixels(), &cpu, 2);
    assert_eq!(
        mismatches, 0,
        "stacked panes: {mismatches} interior pixels differ between GPU and CPU"
    );
}

#[test]
#[ignore = "needs a Vulkan loader + ICD (lavapipe or a real GPU); run with `cargo test --features vulkan -- --ignored`"]
fn a_translucent_surface_does_not_write_depth_but_its_opaque_parts_do() {
    let Some((_lock, gpu)) = try_gpu() else {
        return;
    };
    // An opaque box behind a translucent pane: if the pane's fragments
    // wrote depth (the bug the two-pass split exists to avoid), the box
    // would be culled by the pane's depth and disappear — the parity would
    // break across the whole box.
    let mut scene = Scene::new();
    scene.background = Rgb::BLACK;
    sun(&mut scene);
    pane(
        &mut scene,
        "pane",
        Color::rgb(120, 200, 255),
        0.35,
        Vec3::new(0.0, 0.7, 0.9),
    );
    let mut node = Node::new(
        "box",
        Content::mesh(
            geometry::box_mesh(1.0, 1.0, 1.0),
            Material::lambert(Color::rgb(255, 140, 30)),
        ),
    );
    node.position = Vec3::new(0.0, 0.5, 0.2);
    scene.add(node, None);
    let camera = Camera::perspective(Vec3::new(0.0, 1.8, 3.4), Vec3::new(0.0, 0.4, 0.0), 0.9);

    let (image, _) = gpu.render(&mut scene, &camera);
    let cpu = on_cpu(&mut scene, &camera);
    // The box must be there at all — visible through the pane, not culled
    // by a translucent depth write.
    let orange = cpu
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 100 && p[1] < 180 && p[2] < 120)
        .count();
    assert!(
        orange > 500,
        "the box must show through the pane ({orange} px)"
    );
    let mismatches = interior_mismatches(image.pixels(), &cpu, 2);
    assert_eq!(
        mismatches, 0,
        "translucent depth: {mismatches} interior pixels differ between GPU and CPU"
    );
}
