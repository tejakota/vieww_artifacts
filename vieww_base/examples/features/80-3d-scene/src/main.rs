//! A lit, shadowed, textured 3D scene — `vieww-3d`, photographed.
//!
//! Five primitives, one per material model (Lambert, GGX metallic,
//! Blinn–Phong, Toon, a checker-textured Standard), a ground plane that
//! receives a directional light's shadow map, a hemisphere fill and a warm
//! point light, instanced pillars in the background and a LOD sphere —
//! orbited by the camera and spun on their own axes. The caption prints the
//! renderer's own counters for the frame: objects, culled, triangles, back
//! faces, fragments shaded.

use std::cell::RefCell;
use std::f32::consts::{FRAC_PI_2, TAU};
use std::rc::Rc;

use vieww_3d::{geometry, Content, Light, Mat4, Material, Node, OrbitControls, Quat, RenderStats, Renderer, Rgb, Scene, Texture, Vec3, Viewport3D};
use vieww_foundation::{Color, Size};
use vieww_widget::prelude::*;

fn build_scene() -> Scene {
    let mut s = Scene::new();
    s.background = Rgb::new(0.03, 0.035, 0.06);
    s.fog = Some((Rgb::new(0.03, 0.035, 0.06), 14.0, 30.0));
    s.add(
        Node::new(
            "ground",
            Content::mesh(geometry::plane(24.0, 24.0, 1, 1), Material::standard(Color::rgb(170, 175, 190), 0.0, 0.9)),
        )
        .rotated(Quat::from_axis_angle(Vec3::X, -FRAC_PI_2)),
        None,
    );
    let shapes: [(&str, vieww_mesh::Mesh, Material, Vec3); 5] = [
        ("lambert-box", geometry::box_mesh(1.2, 1.2, 1.2), Material::lambert(Color::rgb(230, 80, 70)), Vec3::new(-3.0, 0.6, 0.0)),
        ("gold-sphere", geometry::sphere(0.8, 48, 24), Material::standard(Color::rgb(255, 200, 90), 1.0, 0.3), Vec3::new(-1.0, 0.8, 1.0)),
        ("phong-torus", geometry::torus(0.6, 0.25, 24, 48), Material::phong(Color::rgb(70, 140, 255), 60.0), Vec3::new(1.2, 0.9, 0.6)),
        ("toon-cone", geometry::cone(0.7, 1.6, 40), Material::toon(Color::rgb(120, 220, 140), 3), Vec3::new(3.0, 0.8, -0.2)),
        (
            "checker-cube",
            geometry::box_mesh(1.0, 1.0, 1.0),
            Material::standard(Color::WHITE, 0.0, 0.6).map(Texture::checker(64, 8, Color::rgb(245, 245, 250), Color::rgb(40, 44, 60))),
            Vec3::new(0.2, 0.5, -2.2),
        ),
    ];
    for (name, mesh, mat, at) in shapes {
        s.add(Node::new(name, Content::mesh(mesh, mat)).at(at), None);
    }
    let pillars: Vec<Mat4> = (0..9)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let a = i as f32 / 9.0 * TAU;
            Mat4::translation(Vec3::new(a.cos() * 8.0, 1.0, a.sin() * 8.0))
        })
        .collect();
    s.add(
        Node::new("pillars", Content::instanced(geometry::cylinder(0.3, 0.3, 2.0, 16), Material::lambert(Color::rgb(120, 110, 150)), pillars)),
        None,
    );
    s.add(
        Node::new(
            "lod-sphere",
            Content::lod(
                vec![(9.0, geometry::sphere(0.5, 32, 16)), (1e9, geometry::sphere(0.5, 6, 4))],
                Material::lambert(Color::rgb(240, 240, 240)).flat(),
            ),
        )
        .at(Vec3::new(2.4, 0.5, 2.6)),
        None,
    );
    s.add(Node::new("hemi", Content::Light(Light::Hemisphere { sky: Rgb::new(0.55, 0.6, 0.8), ground: Rgb::new(0.2, 0.15, 0.1), intensity: 0.35 })), None);
    s.add(
        Node::new(
            "sun",
            Content::Light(Light::Directional { color: Rgb::new(1.0, 0.96, 0.9), intensity: 1.6, direction: Vec3::new(-0.5, -1.0, -0.35), shadow: true }),
        ),
        None,
    );
    s.add(
        Node::new("lamp", Content::Light(Light::Point { color: Rgb::new(1.0, 0.5, 0.2), intensity: 2.0, range: 6.0 })).at(Vec3::new(0.0, 1.2, 2.5)),
        None,
    );
    s
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("80 — 3d scene", Size::new(620.0, 470.0), |d| {
        let scene = Rc::new(RefCell::new(build_scene()));
        let stats = Rc::new(RefCell::new(RenderStats::default()));
        let view = feature_harness::clocked(d, 6.0, move |t| {
            {
                let mut s = scene.borrow_mut();
                for name in ["lambert-box", "phong-torus", "checker-cube", "toon-cone"] {
                    if let Some(id) = s.find(name) {
                        s.node_mut(id).rotation = Quat::from_euler(t * 0.7, t * 1.1, 0.0);
                    }
                }
            }
            let mut orbit = OrbitControls::new(Vec3::new(0.0, 0.6, 0.0), 8.5);
            orbit.yaw = 0.5 + t / 6.0 * TAU * 0.25;
            orbit.pitch = 0.42;
            let camera = orbit.camera(0.85);
            let viewport = Viewport3D::new(scene.clone(), camera, Renderer::new(580, 360).samples(2)).report_to(stats.clone());
            let st = *stats.borrow();
            Flex::column()
                .spacing(8.0)
                .children(children![
                    viewport,
                    feature_harness::caption(
                        "vieww-3d · software rasterizer",
                        &format!(
                            "last frame — objects {} · culled {} · triangles {} · back faces {} · fragments {} · shadow tris {}",
                            st.objects, st.culled_objects, st.triangles, st.backfaces, st.fragments, st.shadow_triangles
                        ),
                        true,
                    ),
                ])
                .into()
        });
        feature_harness::set_page(d, Container::new().color(Color::rgb(14, 16, 24)).padding(EdgeInsets::all(20.0)).child(view));
    })
}
