//! The lit pass of `GpuRenderer` against the CPU oracle: Lambert shading
//! under ambient, hemisphere and directional light, shadows, double-sided
//! and single-sided back faces, flat shading, and the fall-back for what the
//! pass does not cover.
//!
//! Same claim and same helper as `gpu_unlit.rs` — **interiors match**, and
//! disagreements may only sit where the reference image itself has an edge.
//! A shadow's boundary is such an edge (the PCF steps across it), so the
//! assertion covers the lit and the shadowed interiors on both sides of it.
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

/// How many pixels differ at all (the anti-vacuity check: a scene whose GPU
/// image is the CPU's must actually draw something lit).
fn lit_pixels(cpu: &[u8], background: [u8; 3]) -> usize {
    cpu.as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[..3] != background)
        .count()
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
    if let Some(dir) = std::env::var_os("VIEWW_GPU_LIT_DUMP") {
        let dir = std::path::PathBuf::from(dir);
        let name: String = what
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect();
        dump(&dir.join(format!("{name}-gpu.ppm")), image.pixels());
        dump(&dir.join(format!("{name}-cpu.ppm")), &cpu);
    }
    let mismatches = interior_mismatches(image.pixels(), &cpu, 2);
    assert_eq!(
        mismatches, 0,
        "{what}: {mismatches} interior pixels differ between GPU and CPU"
    );
}

/// Binary PPM, for looking at a failure (`VIEWW_GPU_LIT_DUMP=<dir>`).
fn dump(path: &std::path::Path, rgba: &[u8]) {
    let mut out = format!("P6 {WIDTH} {HEIGHT} 255\n").into_bytes();
    for p in rgba.as_chunks::<4>().0 {
        out.extend_from_slice(&p[..3]);
    }
    std::fs::write(path, out).expect("writing a dump");
}

fn light(scene: &mut Scene, name: &str, l: Light) {
    scene.add(Node::new(name, Content::Light(l)), None);
}

fn mesh(
    scene: &mut Scene,
    name: &str,
    m: vieww_mesh::Mesh,
    mat: Material,
    at: Vec3,
) -> vieww_3d::scene::NodeId {
    let mut node = Node::new(name, Content::mesh(m, mat));
    node.position = at;
    scene.add(node, None)
}

/// A sphere (smooth normals — the interpolation and per-fragment normalize
/// are what is under test), a rotated box, and a flat-shaded box, under an
/// ambient, a hemisphere and two directional lights, one of them coloured.
fn lit_scene() -> (Scene, Camera) {
    let mut scene = Scene::new();
    scene.background = Rgb::new(0.02, 0.03, 0.05);
    light(
        &mut scene,
        "amb",
        Light::Ambient {
            color: Rgb::WHITE,
            intensity: 0.08,
        },
    );
    light(
        &mut scene,
        "hemi",
        Light::Hemisphere {
            sky: Rgb::new(0.6, 0.7, 1.0),
            ground: Rgb::new(0.3, 0.2, 0.1),
            intensity: 0.25,
        },
    );
    light(
        &mut scene,
        "sun",
        Light::Directional {
            color: Rgb::WHITE,
            intensity: 0.9,
            direction: Vec3::new(-0.4, -1.0, -0.6),
            shadow: false,
        },
    );
    light(
        &mut scene,
        "rim",
        Light::Directional {
            color: Rgb::new(1.0, 0.5, 0.2),
            intensity: 0.5,
            direction: Vec3::new(0.8, 0.1, 0.5),
            shadow: false,
        },
    );
    mesh(
        &mut scene,
        "sphere",
        geometry::sphere(0.8, 48, 32),
        Material::lambert(Color::rgb(200, 200, 210)),
        Vec3::new(-0.9, 0.0, 0.0),
    );
    let id = mesh(
        &mut scene,
        "box",
        geometry::box_mesh(0.9, 0.9, 0.9),
        Material::lambert(Color::rgb(60, 160, 90)).emissive(Rgb::new(0.02, 0.0, 0.0)),
        Vec3::new(1.0, 0.1, 0.3),
    );
    scene.node_mut(id).rotation = Quat::from_euler(0.5, 0.7, 0.1);
    mesh(
        &mut scene,
        "flat",
        geometry::sphere(0.4, 10, 8),
        Material::lambert(Color::rgb(220, 120, 60)).flat(),
        Vec3::new(0.1, 0.9, -0.6),
    );
    // One Basic material in the same frame: the lit pass's unlit branch.
    mesh(
        &mut scene,
        "basic",
        geometry::box_mesh(0.3, 0.3, 0.3),
        Material::basic(Color::rgb(255, 255, 0)),
        Vec3::new(0.2, -0.8, 0.8),
    );
    let camera = Camera::perspective(Vec3::new(0.0, 1.2, 4.0), Vec3::ZERO, 0.9);
    (scene, camera)
}

#[test]
#[ignore = "needs a Vulkan loader + ICD (lavapipe or a real GPU); run with `cargo test --features vulkan -- --ignored`"]
fn lambert_under_ambient_hemisphere_and_directional_matches_the_cpu() {
    let Some((_lock, gpu)) = try_gpu() else {
        return;
    };
    let (mut scene, camera) = lit_scene();
    let cpu = on_cpu(&mut scene, &camera);
    assert!(
        lit_pixels(&cpu, [cpu[0], cpu[1], cpu[2]]) > 3000,
        "the scene must draw"
    );
    assert_parity(&gpu, &mut scene, &camera, "lit scene");
}

/// A box over a ground plane, the sun casting a shadow map: the shadowed and
/// lit regions of the plane are both interiors, and must both match.
#[test]
#[ignore = "needs a Vulkan loader + ICD (lavapipe or a real GPU); run with `cargo test --features vulkan -- --ignored`"]
fn a_shadow_falls_where_the_cpu_says() {
    let Some((_lock, gpu)) = try_gpu() else {
        return;
    };
    let mut scene = Scene::new();
    scene.background = Rgb::BLACK;
    light(
        &mut scene,
        "amb",
        Light::Ambient {
            color: Rgb::WHITE,
            intensity: 0.15,
        },
    );
    light(
        &mut scene,
        "sun",
        Light::Directional {
            color: Rgb::WHITE,
            intensity: 1.0,
            // Low and toward the camera, so the shadow is long and in view.
            direction: Vec3::new(0.6, -0.55, 0.7),
            shadow: true,
        },
    );
    let ground = mesh(
        &mut scene,
        "ground",
        geometry::plane(6.0, 6.0, 1, 1),
        Material::lambert(Color::rgb(200, 200, 200)),
        Vec3::ZERO,
    );
    scene.node_mut(ground).rotation = Quat::from_euler(-std::f32::consts::FRAC_PI_2, 0.0, 0.0);
    mesh(
        &mut scene,
        "caster",
        geometry::box_mesh(0.8, 1.2, 0.8),
        Material::lambert(Color::rgb(180, 80, 60)),
        Vec3::new(0.0, 0.8, 0.0),
    );
    let camera = Camera::perspective(Vec3::new(1.5, 3.0, 4.0), Vec3::new(0.0, 0.3, 0.0), 0.9);

    // The shadow must actually be in the picture: the CPU with shadows off
    // differs from the CPU with them on in many interior pixels.
    let with = on_cpu(&mut scene, &camera);
    let mut no_shadows = Renderer::new(WIDTH, HEIGHT).samples(1);
    no_shadows.shadow_size = 0;
    let (without, _) = no_shadows.render(&mut scene, &camera);
    let shadowed = with
        .as_chunks::<4>()
        .0
        .iter()
        .zip(without.pixels().as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        shadowed > 500,
        "the test scene must cast a visible shadow ({shadowed} px)"
    );

    assert_parity(&gpu, &mut scene, &camera, "shadowed ground");
}

/// A plane seen from behind: single-sided it is culled (the background shows
/// through, as on the CPU); double-sided it is lit with its normal flipped.
#[test]
#[ignore = "needs a Vulkan loader + ICD (lavapipe or a real GPU); run with `cargo test --features vulkan -- --ignored`"]
fn back_faces_are_culled_or_flipped_as_on_the_cpu() {
    let Some((_lock, gpu)) = try_gpu() else {
        return;
    };
    for double in [false, true] {
        let mut scene = Scene::new();
        scene.background = Rgb::new(0.1, 0.1, 0.1);
        light(
            &mut scene,
            "sun",
            Light::Directional {
                color: Rgb::WHITE,
                intensity: 1.0,
                // Travelling toward +z: lights the plane's back (-z) side.
                direction: Vec3::new(0.2, -0.3, 1.0),
                shadow: false,
            },
        );
        let mat = Material::lambert(Color::rgb(90, 140, 230));
        let mat = if double { mat.double_sided() } else { mat };
        mesh(
            &mut scene,
            "plane",
            geometry::plane(2.0, 1.5, 4, 3),
            mat,
            Vec3::ZERO,
        );
        // Behind the plane (it faces +z), looking at its back.
        let camera = Camera::perspective(Vec3::new(0.4, 0.3, -3.0), Vec3::ZERO, 0.9);
        let cpu = on_cpu(&mut scene, &camera);
        let drawn = lit_pixels(&cpu, [cpu[0], cpu[1], cpu[2]]);
        if double {
            assert!(
                drawn > 2000,
                "double-sided: the back face is drawn ({drawn})"
            );
        } else {
            assert_eq!(drawn, 0, "single-sided: the back face is culled");
        }
        assert_parity(
            &gpu,
            &mut scene,
            &camera,
            if double {
                "double-sided"
            } else {
                "single-sided"
            },
        );
    }
}

/// What the lit pass does not cover falls back to the CPU renderer — at its
/// own default settings — for the whole frame, so the image is the CPU's
/// exactly.
#[test]
#[ignore = "needs a Vulkan loader + ICD (lavapipe or a real GPU); run with `cargo test --features vulkan -- --ignored`"]
fn a_point_light_falls_back_to_the_cpu_exactly() {
    let Some((_lock, gpu)) = try_gpu() else {
        return;
    };
    let (mut scene, camera) = lit_scene();
    let mut lamp = Node::new(
        "lamp",
        Content::Light(Light::Point {
            color: Rgb::WHITE,
            intensity: 2.0,
            range: 10.0,
        }),
    );
    lamp.position = Vec3::new(0.0, 2.0, 2.0);
    scene.add(lamp, None);
    let (image, _) = gpu.render(&mut scene, &camera);
    let (cpu, _) = Renderer::new(WIDTH, HEIGHT).render(&mut scene, &camera);
    assert_eq!(image.pixels(), cpu.pixels());
}
