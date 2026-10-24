//! The textured reach of `GpuRenderer`'s lit pass against the CPU oracle:
//! bilinear and nearest sampling, a map on a `Basic` material (which routes
//! through the lit pass now, because sampling lives there), and an
//! alpha-cutout map — the case where the transparent split's `a >= 1.0`
//! half writes depth and the `a < 1.0` half blends, per fragment, exactly
//! as the oracle's single pass does.
//!
//! Same claim and same helper as `gpu_lit.rs` — **interiors match**, and
//! disagreements may only sit where the reference image itself has an edge.
//! A texel boundary the two samplers filter differently is such an edge, so
//! the checker's interiors — whole texels deep — are what the assertion
//! pins.
//!
//! Run with `cargo test -p vieww-3d --features vulkan -- --ignored --test-threads=1`.

#![cfg(feature = "vulkan")]

use vieww_3d::geometry;
use vieww_3d::gpu::GpuRenderer;
use vieww_3d::math::{Quat, Vec3};
use vieww_3d::render::Renderer;
use vieww_3d::scene::{Camera, Content, Light, Material, Node, Rgb, Scene, Texture};
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

fn assert_parity(gpu: &GpuRenderer, scene: &mut Scene, camera: &Camera, what: &str) {
    let (image, _) = gpu.render(scene, camera);
    let cpu = on_cpu(scene, camera);
    let mismatches = interior_mismatches(image.pixels(), &cpu, 2);
    assert_eq!(
        mismatches, 0,
        "{what}: {mismatches} interior pixels differ between GPU and CPU"
    );
}

fn plane_with(scene: &mut Scene, name: &str, mat: Material, at: Vec3) -> vieww_3d::scene::NodeId {
    let mut node = Node::new(name, Content::mesh(geometry::plane(3.0, 3.0, 1, 1), mat));
    node.position = at;
    node.rotation = Quat::from_euler(-std::f32::consts::FRAC_PI_2, 0.0, 0.0);
    scene.add(node, None)
}

/// A textured scene: a checkerboard plane, lit by a directional light, seen
/// at an angle so the sampling is minified and magnified across the frame.
fn textured_scene(filter: vieww_3d::scene::Filter, map: Texture) -> (Scene, Camera) {
    let mut scene = Scene::new();
    scene.background = Rgb::BLACK;
    let mut sun = Node::new(
        "sun",
        Content::Light(Light::Directional {
            color: Rgb::WHITE,
            intensity: 1.0,
            direction: Vec3::new(-0.4, -0.8, -0.4),
            shadow: false,
        }),
    );
    sun.position = Vec3::new(0.0, 4.0, 0.0);
    scene.add(sun, None);
    let mut material = Material::lambert(Color::WHITE);
    material.map = Some(map);
    material.map.as_mut().expect("just set").filter = filter;
    plane_with(&mut scene, "ground", material, Vec3::ZERO);
    let camera = Camera::perspective(Vec3::new(0.0, 2.2, 3.2), Vec3::new(0.0, 0.0, 0.0), 0.9);
    (scene, camera)
}

#[test]
#[ignore = "needs a Vulkan loader + ICD (lavapipe or a real GPU); run with `cargo test --features vulkan -- --ignored`"]
fn a_bilinear_textured_lambert_plane_matches_the_cpu() {
    let Some((_lock, gpu)) = try_gpu() else {
        return;
    };
    let (mut scene, camera) = textured_scene(
        vieww_3d::scene::Filter::Bilinear,
        Texture::checker(64, 8, Color::WHITE, Color::rgb(120, 40, 30)),
    );
    // Anti-vacuity: the texture must actually show — the textured render
    // differs from the untextured one in many pixels.
    let cpu = on_cpu(&mut scene, &camera);
    let mut plain = scene.clone();
    let node = plain.find("ground").expect("the ground plane exists");
    if let Content::Mesh { material, .. } = &mut plain.node_mut(node).content {
        *material = std::sync::Arc::new(Material::lambert(Color::WHITE));
    }
    let (plain_image, _) = Renderer::new(WIDTH, HEIGHT)
        .samples(1)
        .render(&mut plain, &camera);
    let textured_pixels = cpu
        .as_chunks::<4>()
        .0
        .iter()
        .zip(plain_image.pixels().as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        textured_pixels > 2000,
        "the texture must actually show ({textured_pixels} px)"
    );
    assert_parity(&gpu, &mut scene, &camera, "bilinear checker");
}

#[test]
#[ignore = "needs a Vulkan loader + ICD (lavapipe or a real GPU); run with `cargo test --features vulkan -- --ignored`"]
fn a_nearest_filtered_texture_matches_the_cpu() {
    let Some((_lock, gpu)) = try_gpu() else {
        return;
    };
    let (mut scene, camera) = textured_scene(
        vieww_3d::scene::Filter::Nearest,
        Texture::checker(64, 4, Color::rgb(30, 160, 90), Color::rgb(20, 30, 150)),
    );
    assert_parity(&gpu, &mut scene, &camera, "nearest checker");
}

#[test]
#[ignore = "needs a Vulkan loader + ICD (lavapipe or a real GPU); run with `cargo test --features vulkan -- --ignored`"]
fn a_textured_basic_material_matches_the_cpu() {
    let Some((_lock, gpu)) = try_gpu() else {
        return;
    };
    // `Basic` + a map: unlit shading, but sampling — so the lit pass's
    // unlit branch with `albedo * texel.rgb`, `alpha = texel.a`.
    let (mut scene, camera) = textured_scene(
        vieww_3d::scene::Filter::Bilinear,
        Texture::checker(64, 8, Color::WHITE, Color::rgb(160, 160, 20)),
    );
    let node = scene.find("ground").expect("the ground plane exists");
    if let Content::Mesh { material, .. } = &mut scene.node_mut(node).content {
        let mut basic = Material::basic(Color::WHITE);
        basic.map = material.map.clone();
        *material = std::sync::Arc::new(basic);
    }
    assert_parity(&gpu, &mut scene, &camera, "textured basic");
}

#[test]
#[ignore = "needs a Vulkan loader + ICD (lavapipe or a real GPU); run with `cargo test --features vulkan -- --ignored`"]
fn an_alpha_cutout_map_writes_depth_where_the_cpu_does() {
    let Some((_lock, gpu)) = try_gpu() else {
        return;
    };
    // Binary alpha: whole texels at a = 1 (the cutout writes colour and
    // depth, the split's Write half) and whole texels at a = 0 (the Blend
    // half with `lerp(bg, c, 0)` — no visible change, and crucially no
    // depth write, so the box behind the holes shows through).
    let mut px = Vec::new();
    for y in 0..32u32 {
        for x in 0..32u32 {
            let hole = (x / 8 + y / 8) % 2 == 0;
            if hole {
                px.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                px.extend_from_slice(&[220, 60, 40, 255]);
            }
        }
    }
    let (mut scene, _camera) = textured_scene(
        vieww_3d::scene::Filter::Nearest,
        Texture::from_rgba8(32, 32, &px),
    );
    // The box behind the cutout, showing through the holes: if the GPU's
    // Blend half wrote depth, the box would be hidden behind a transparent
    // texel and the parity would break right there.
    let mut node = Node::new(
        "behind",
        Content::mesh(
            geometry::box_mesh(1.0, 1.0, 1.0),
            Material::lambert(Color::rgb(30, 90, 220)),
        ),
    );
    node.position = Vec3::new(0.0, 0.5, 0.4);
    scene.add(node, None);
    let camera = Camera::perspective(Vec3::new(0.0, 2.6, 3.6), Vec3::new(0.0, 0.2, 0.0), 0.9);

    let cpu = on_cpu(&mut scene, &camera);
    // Anti-vacuity: the holes must actually let the box through — pixels
    // of the box's blue differ from a render without the box.
    let mut no_box = scene.clone();
    let behind = no_box.find("behind").expect("the box exists");
    no_box.remove(behind);
    let (no_box_image, _) = Renderer::new(WIDTH, HEIGHT)
        .samples(1)
        .render(&mut no_box, &camera);
    let seen = cpu
        .as_chunks::<4>()
        .0
        .iter()
        .zip(no_box_image.pixels().as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        seen > 500,
        "the box must be visible through the cutout's holes ({seen} px)"
    );
    let (image, _) = gpu.render(&mut scene, &camera);
    let mismatches = interior_mismatches(image.pixels(), &cpu, 2);
    assert_eq!(
        mismatches, 0,
        "alpha cutout: {mismatches} interior pixels differ between GPU and CPU"
    );
}
