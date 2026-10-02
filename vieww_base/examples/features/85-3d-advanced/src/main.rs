//! 3D beyond the rasterizer: picking, a path tracer, glTF round-trip and
//! a Blender-style modifier stack — photographed.
//!
//! 1. Raycast picking: a cursor sweeps the view, `Ray::from_screen` +
//!    `raycast` report the nearest hit; the hit point and surface normal
//!    are projected back and drawn, the object's name is in the caption.
//! 2. A Cornell box through `PathTracer` (BVH, GGX, emissive area light),
//!    accumulating more samples per pixel as the clip plays.
//! 3. glTF: a modified mesh written with `write_gltf`, parsed back with
//!    `parse_gltf` and instantiated with `import_gltf`, then rendered.
//! 4. `ModifierStack`: subdivide → twist → bend → taper on a column, array +
//!    mirror on a cube, displace + smooth on a sphere — animated.

use std::cell::RefCell;
use std::f32::consts::TAU;
use std::rc::Rc;

use feature_harness::draw::{circle, grid, page, paint, panel, HUES, INK};
use vieww_3d::{
    geometry, import_gltf, project, raycast, Accumulator, Camera, Content, Light, Material, Node,
    OrbitControls, PathTracer, Quat, Ray, Renderer, Rgb, Scene, TraceScene, Vec3,
};
use vieww_foundation::{Color, Offset, Size};
use vieww_mesh::gltf::{parse_gltf, write_gltf};
use vieww_mesh::modifiers::{Axis, Modifier, ModifierStack};
use vieww_widget::prelude::*;

const W: u32 = 260;
const H: u32 = 190;
const SPAN: f32 = 4.0;

fn lights(s: &mut Scene) {
    s.add(
        Node::new(
            "hemi",
            Content::Light(Light::Hemisphere {
                sky: Rgb::new(0.6, 0.65, 0.8),
                ground: Rgb::new(0.15, 0.12, 0.1),
                intensity: 0.5,
            }),
        ),
        None,
    );
    s.add(
        Node::new(
            "sun",
            Content::Light(Light::Directional {
                color: Rgb::new(1.0, 0.95, 0.9),
                intensity: 1.5,
                direction: Vec3::new(-0.4, -1.0, -0.5),
                shadow: true,
            }),
        ),
        None,
    );
}

fn pick_scene() -> Scene {
    let mut s = Scene::new();
    s.background = Rgb::new(0.05, 0.06, 0.09);
    s.add(
        Node::new(
            "floor",
            Content::mesh(
                geometry::plane(10.0, 10.0, 1, 1),
                Material::lambert(Color::rgb(90, 95, 110)),
            ),
        )
        .rotated(Quat::from_axis_angle(Vec3::X, -TAU / 4.0)),
        None,
    );
    s.add(
        Node::new(
            "red box",
            Content::mesh(
                geometry::box_mesh(1.0, 1.0, 1.0),
                Material::lambert(Color::rgb(230, 90, 80)),
            ),
        )
        .at(Vec3::new(-1.6, 0.5, 0.0)),
        None,
    );
    s.add(
        Node::new(
            "gold sphere",
            Content::mesh(
                geometry::sphere(0.7, 32, 16),
                Material::standard(Color::rgb(250, 200, 90), 1.0, 0.3),
            ),
        )
        .at(Vec3::new(0.0, 0.7, 0.3)),
        None,
    );
    s.add(
        Node::new(
            "blue torus",
            Content::mesh(
                geometry::torus(0.5, 0.2, 16, 32),
                Material::phong(Color::rgb(80, 140, 250), 40.0),
            ),
        )
        .at(Vec3::new(1.6, 0.6, -0.2)),
        None,
    );
    lights(&mut s);
    s
}

fn cornell() -> Scene {
    let mut s = Scene::new();
    s.background = Rgb::BLACK;
    let wall = |name: &str, c: Color, at: Vec3, size: (f32, f32, f32)| {
        Node::new(
            name,
            Content::mesh(
                geometry::box_mesh(size.0, size.1, size.2),
                Material::lambert(c),
            ),
        )
        .at(at)
    };
    let white = Color::rgb(200, 200, 200);
    s.add(
        wall("floor", white, Vec3::new(0.0, -0.05, 0.0), (2.2, 0.1, 2.2)),
        None,
    );
    s.add(
        wall("ceiling", white, Vec3::new(0.0, 2.05, 0.0), (2.2, 0.1, 2.2)),
        None,
    );
    s.add(
        wall("back", white, Vec3::new(0.0, 1.0, -1.05), (2.2, 2.2, 0.1)),
        None,
    );
    s.add(
        wall(
            "left",
            Color::rgb(200, 40, 40),
            Vec3::new(-1.05, 1.0, 0.0),
            (0.1, 2.2, 2.2),
        ),
        None,
    );
    s.add(
        wall(
            "right",
            Color::rgb(40, 190, 60),
            Vec3::new(1.05, 1.0, 0.0),
            (0.1, 2.2, 2.2),
        ),
        None,
    );
    s.add(
        Node::new(
            "lamp",
            Content::mesh(
                geometry::box_mesh(0.9, 0.02, 0.9),
                Material::lambert(Color::WHITE).emissive(Rgb::new(3.0, 2.8, 2.4)),
            ),
        )
        .at(Vec3::new(0.0, 1.99, 0.0)),
        None,
    );
    // A point light just under the panel: sampled directly (next-event
    // estimation), so the direct term converges fast; the emissive panel
    // supplies the visible light source and bounce light.
    s.add(
        Node::new(
            "bulb",
            Content::Light(Light::Point {
                color: Rgb::new(1.0, 0.92, 0.8),
                intensity: 2.2,
                range: 6.0,
            }),
        )
        .at(Vec3::new(0.0, 1.85, 0.0)),
        None,
    );
    s.add(
        Node::new(
            "tall",
            Content::mesh(
                geometry::box_mesh(0.55, 1.2, 0.55),
                Material::lambert(white),
            ),
        )
        .at(Vec3::new(-0.35, 0.6, -0.3))
        .rotated(Quat::from_axis_angle(Vec3::Y, 0.35)),
        None,
    );
    s.add(
        Node::new(
            "ball",
            Content::mesh(
                geometry::sphere(0.3, 32, 16),
                Material::standard(Color::rgb(240, 240, 240), 1.0, 0.08),
            ),
        )
        .at(Vec3::new(0.4, 0.3, 0.3)),
        None,
    );
    s
}

fn column_stack(t: f32) -> ModifierStack {
    ModifierStack::new()
        .with(Modifier::Subdivide {
            levels: 3,
            smooth: false,
        })
        .with(Modifier::Twist {
            angle: (t * 1.3).sin() * 2.0,
        })
        .with(Modifier::Bend {
            angle: (t * 0.9).sin() * 1.2,
        })
        .with(Modifier::Taper { factor: 0.4 })
}

fn image_panel(
    title: &str,
    detail: &str,
    img: vieww_foundation::Image,
    overlay: Option<WidgetNode>,
) -> WidgetNode {
    let body: WidgetNode = match overlay {
        Some(o) => Stack::new()
            .children(children![
                Image::new(img),
                Positioned::new().left(0.0).top(0.0).child(o)
            ])
            .into(),
        None => Image::new(img).into(),
    };
    panel(
        title,
        detail,
        SizedBox::from_size(Size::new(W as f32, H as f32)).child(body),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // glTF round trip, done once: modify → write → parse → import.
    let knot = ModifierStack::new()
        .with(Modifier::Subdivide {
            levels: 1,
            smooth: true,
        })
        .with(Modifier::Twist { angle: 1.2 })
        .apply(&geometry::torus(0.7, 0.28, 12, 36));
    let text = write_gltf(&knot, "twisted-torus", [0.35, 0.8, 0.6, 1.0]);
    let gltf = parse_gltf(&text, &|_| None).map_err(|e| e.0)?;
    let gltf_bytes = text.len();

    feature_harness::launch("85 — 3d advanced", Size::new(1210.0, 330.0), move |d| {
        let pick = Rc::new(RefCell::new(pick_scene()));
        let cornell_scene = Rc::new(TraceScene::build(&mut cornell()));
        let gltf = gltf.clone();
        // Each view is a full software render: rebuild it only when the
        // quantised time moves (8 steps per loop), not every display frame.
        let cache: Rc<RefCell<Option<(i32, WidgetNode)>>> = Rc::default();
        let view = feature_harness::clocked(d, SPAN, move |t| {
            let step = (t * 2.0).floor() as i32;
            if let Some((k, node)) = cache.borrow().as_ref() {
                if *k == step {
                    return node.clone();
                }
            }
            let t = step as f32 / 2.0;
            // 1 — picking.
            let cam = OrbitControls {
                yaw: 0.3,
                pitch: 0.35,
                ..OrbitControls::new(Vec3::new(0.0, 0.5, 0.0), 6.0)
            }
            .camera(0.8);
            let (img, _) = Renderer::new(W, H)
                .samples(2)
                .render(&mut pick.borrow_mut(), &cam);
            let cursor = Offset::new(
                W as f32 * (0.15 + 0.7 * (t / SPAN)),
                H as f32 * (0.55 + 0.12 * (t * 3.0).sin()),
            );
            let ray = Ray::from_screen(&cam, W as f32, H as f32, cursor.dx, cursor.dy);
            let hits = raycast(&mut pick.borrow_mut(), &ray);
            let (name, marker) = hits.first().map_or(("nothing".to_owned(), None), |h| {
                let p =
                    project(&cam, W as f32, H as f32, h.point).map(|(x, y, _)| Offset::new(x, y));
                let n = project(&cam, W as f32, H as f32, h.point + h.normal * 0.5)
                    .map(|(x, y, _)| Offset::new(x, y));
                (
                    format!("{} @ {:.2}m", pick.borrow().node(h.node).name, h.distance),
                    p.zip(n),
                )
            });
            let overlay = paint(Size::new(W as f32, H as f32), move |g, _| {
                g.line(
                    cursor + Offset::new(-8.0, 0.0),
                    cursor + Offset::new(8.0, 0.0),
                    INK,
                    1.5,
                );
                g.line(
                    cursor + Offset::new(0.0, -8.0),
                    cursor + Offset::new(0.0, 8.0),
                    INK,
                    1.5,
                );
                if let Some((p, n)) = marker {
                    g.stroke(circle(p, 6.0), HUES[3], 2.0);
                    g.line(p, n, HUES[3], 2.0);
                }
            });
            let p1 = image_panel(
                "raycast picking",
                &format!("hit: {name} · {} hits along the ray", hits.len()),
                img,
                Some(overlay),
            );

            // 2 — path tracer, progressive.
            let spp = 2 + (t / SPAN * 40.0) as u32;
            let mut acc = Accumulator::new(W, H);
            let tracer = PathTracer {
                max_bounces: 4,
                seed: 9,
                ..PathTracer::default()
            };
            let camera =
                Camera::perspective(Vec3::new(0.0, 1.0, 3.4), Vec3::new(0.0, 1.0, 0.0), 0.75);
            let rays = tracer.accumulate(&cornell_scene, &camera, &mut acc, spp);
            let p2 = image_panel(
                "path tracer · Cornell box",
                &format!(
                    "{spp} spp · {rays} rays · {} tris in BVH",
                    cornell_scene.triangles()
                ),
                acc.image(1.0),
                None,
            );

            // 3 — glTF import.
            let mut s = Scene::new();
            s.background = Rgb::new(0.05, 0.06, 0.09);
            let ids = import_gltf(&mut s, &gltf, None);
            if let Some(Some(id)) = ids.first() {
                s.node_mut(*id).rotation = Quat::from_euler(0.9, t * 0.8, 0.2);
            }
            lights(&mut s);
            let cam3 = OrbitControls::new(Vec3::ZERO, 3.4).camera(0.8);
            let (img3, st) = Renderer::new(W, H).samples(2).render(&mut s, &cam3);
            let p3 = image_panel(
                "glTF write → parse → import",
                &format!(
                    "{gltf_bytes} bytes of .gltf · {} triangles drawn",
                    st.triangles
                ),
                img3,
                None,
            );

            // 4 — modifiers.
            let mut m = Scene::new();
            m.background = Rgb::new(0.05, 0.06, 0.09);
            let col = column_stack(t).apply(&geometry::box_mesh(0.5, 2.0, 0.5));
            m.add(
                Node::new(
                    "column",
                    Content::mesh(col, Material::standard(Color::rgb(240, 160, 90), 0.0, 0.5)),
                )
                .at(Vec3::new(-1.3, 1.0, 0.0)),
                None,
            );
            let arr = ModifierStack::new()
                .with(Modifier::Array {
                    count: 3,
                    offset: [0.45, 0.25, 0.0],
                })
                .with(Modifier::Mirror {
                    axis: Axis::X,
                    merge: 0.01,
                })
                .apply(&geometry::box_mesh(0.3, 0.3, 0.3));
            m.add(
                Node::new(
                    "array",
                    Content::mesh(arr, Material::lambert(Color::rgb(110, 170, 250))),
                )
                .at(Vec3::new(0.2, 0.3, 0.6)),
                None,
            );
            let blob = ModifierStack::new()
                .with(Modifier::Displace {
                    strength: 0.25 + 0.15 * (t * 2.0).sin(),
                    scale: 2.5,
                    seed: 4,
                })
                .with(Modifier::Smooth {
                    factor: 0.5,
                    iterations: 2,
                })
                .apply(&geometry::sphere(0.55, 32, 16));
            m.add(
                Node::new(
                    "blob",
                    Content::mesh(blob, Material::phong(Color::rgb(180, 120, 240), 30.0)),
                )
                .at(Vec3::new(1.3, 0.8, -0.3)),
                None,
            );
            lights(&mut m);
            let cam4 = OrbitControls {
                pitch: 0.25,
                ..OrbitControls::new(Vec3::new(0.0, 0.8, 0.0), 4.6)
            }
            .camera(0.8);
            let (img4, _) = Renderer::new(W, H).samples(2).render(&mut m, &cam4);
            let p4 = image_panel(
                "modifier stack",
                "subdivide·twist·bend·taper | array·mirror | displace·smooth",
                img4,
                None,
            );

            let node = page(
                "85 · 3d: picking, path tracing, glTF, modifiers",
                "vieww-3d + vieww-mesh, all CPU",
                grid(4, vec![p1, p2, p3, p4]),
            );
            *cache.borrow_mut() = Some((step, node.clone()));
            node
        });
        feature_harness::set_page(d, view);
    })
}
