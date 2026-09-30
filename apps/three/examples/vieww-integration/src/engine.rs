//! The engine path: a `.3` capture rendered through `vieww-3d`.
//!
//! The viewer's first pass drew the capture itself — `MeshPainter`'s
//! projection, painter's-algorithm sorting, per-triangle brightness. That
//! code stays: it is the reference the engine is held against, and the only
//! path that draws a wireframe. But the framework now owns a real 3D stack —
//! [`vieww-3d`](vieww_3d): a scene graph, materials, lights, a depth-buffered
//! software rasteriser with a shadow pass, and a [`Viewport3D`] widget that
//! lands the whole thing in the tree as an ordinary image (the TouchDesigner
//! "render the 3D to a texture, composite the texture" model).
//!
//! This module is the bridge: a capture's current `MeshFrame` becomes a
//! [`Mesh`](vieww_mesh::Mesh) with computed normals, the viewer's
//! [`OrbitCamera`](three_vieww::OrbitCamera) becomes the engine camera, and
//! the chamber gains a light rig. The point motes are drawn as one
//! instanced-mesh node rather than a depth-ignoring afterthought — in the
//! engine they are occluded by the surface, which is the honest answer for
//! points that orbit *around* a subject.
//!
//! What the engine buys, concretely, over the painter:
//!
//! * **A depth buffer** instead of mean-depth triangle sorting — where the
//!   wave surface folds over itself, the painter can draw a farther
//!   triangle over a nearer one; the engine cannot.
//! * **Shadows** — the directional light casts the surface onto itself, so
//!   the wave's relief reads as relief rather than as brightness variation.
//! * **Per-vertex normals** — `Mesh::compute_normals`'s area-weighted
//!   average, so lighting is smooth where the surface is smooth instead of
//!   flat per triangle.
//!
//! What it costs: the engine renders the scene on every rebuild of the
//! viewport, where the painter's geometry projection is cheaper. Both are
//! software paths, both deterministic, and the demo capture is 128
//! triangles — the difference is not measurable here.

use std::cell::RefCell;
use std::rc::Rc;

use three_core::{Capture3D, MeshFrame};
use three_vieww::OrbitCamera;
use vieww::foundation::Constraints;
use vieww::prelude::*;
use vieww_3d::math::Mat4;
use vieww_3d::{
    geometry, Camera as EngineCamera, Content, Light, Material, Node, Renderer, Rgb, Scene,
    Vec3, Viewport3D,
};
use vieww_mesh::Mesh;

use crate::painter::ViewPalette;

/// One mesh frame as the engine sees it: positions, indices, and the
/// per-vertex normals the lighting needs.
///
/// The capture's frames carry no normals (reconstruction output does not
/// guarantee them), so they are computed here — the average of the adjacent
/// faces' normals, area-weighted by construction. Winding order is
/// untrusted, which is exactly why the viewer's painter culls nothing: the
/// engine does not cull either (its default is to draw both sides), so an
/// inconsistently wound surface still shows up complete.
#[must_use]
pub fn frame_mesh(frame: &MeshFrame) -> Mesh {
    let mut mesh = Mesh {
        positions: frame
            .vertices
            .iter()
            .map(|v| [v.x, v.y, v.z])
            .collect(),
        normals: Vec::new(),
        uvs: Vec::new(),
        indices: frame.indices.clone(),
    };
    mesh.compute_normals();
    mesh
}

/// The capture's current frame, as an engine scene.
///
/// Built per rebuild rather than cached: a scene is three nodes and a
/// 128-triangle mesh, and rebuilding it is cheaper than the bookkeeping a
/// cache would add. When a future capture is heavy enough to want one, the
/// right cache is on the playback state, keyed by frame index — not here.
#[must_use]
pub fn capture_scene(
    capture: &Capture3D,
    mesh_index: Option<usize>,
    points_index: Option<usize>,
    show_points: bool,
    palette: &ViewPalette,
) -> Scene {
    let mut scene = Scene::new();
    // The chamber, as the engine sees it: the painter's gradient ends in
    // this colour at the bottom, so the engine's flat background and the
    // painter's chamber agree at the seam.
    scene.background = Rgb::from_color(palette.background_bottom);

    if let Some(index) = mesh_index {
        if let Some(frame) = capture.meshes.get(index) {
            scene.add(
                Node::new(
                    "capture",
                    Content::mesh(frame_mesh(frame), Material::lambert(palette.mesh)),
                ),
                None,
            );
        }
    }

    // The motes: one instanced node, not per-point nodes — and drawn by the
    // same depth buffer as the surface, so a mote behind the wave is hidden
    // by it. The painter draws them on top (deliberately; see its docs);
    // here occlusion is the honest default.
    if show_points {
        if let Some(index) = points_index {
            if let Some(frame) = capture.points.get(index) {
                let instances = frame
                    .points
                    .iter()
                    .map(|p| Mat4::translation(Vec3::new(p.x, p.y, p.z)))
                    .collect();
                scene.add(
                    Node::new(
                        "motes",
                        Content::instanced(
                            geometry::sphere(0.025, 8, 6),
                            Material::basic(palette.points),
                            instances,
                        ),
                    ),
                    None,
                );
            }
        }
    }

    // The light rig: ambient fill plus one shadow-casting key light, the
    // same balance the painter's MeshShading defaults use (0.35 ambient +
    // 0.65 diffuse), so the two renders of the same frame agree about
    // *where* the light comes from even though they disagree about how to
    // draw it.
    scene.add(
        Node::new(
            "fill",
            Content::Light(Light::Ambient {
                color: Rgb::WHITE,
                intensity: 0.35,
            }),
        ),
        None,
    );
    scene.add(
        Node::new(
            "key",
            Content::Light(Light::Directional {
                color: Rgb::WHITE,
                intensity: 0.65,
                direction: Vec3::new(-0.4, -1.0, -0.55),
                shadow: true,
            }),
        ),
        None,
    );

    scene
}

/// The viewer's orbit camera, as the engine camera.
///
/// Same eye, same target, same vertical field of view as the painter's
/// lens — the two renderers must agree about the viewpoint, or comparing
/// them is meaningless.
#[must_use]
pub fn engine_camera(camera: &OrbitCamera, fov_y: f32) -> EngineCamera {
    let eye = camera.eye();
    let target = camera.target();
    EngineCamera::perspective(
        Vec3::new(eye.x, eye.y, eye.z),
        Vec3::new(target.x, target.y, target.z),
        fov_y,
    )
}

/// The engine's viewport widget: the capture through `vieww-3d`, at the
/// chamber's own size.
///
/// `Viewport3D` renders to a fixed-size image, so the chamber's laid-out
/// size — which only a `LayoutBuilder` knows — is what sizes the renderer.
/// Every rebuild re-renders; on a playing capture that is every frame,
/// which is the same cadence the painter's projection runs at.
pub struct EngineViewport {
    pub capture: Rc<Capture3D>,
    pub mesh_index: Option<usize>,
    pub points_index: Option<usize>,
    pub camera: OrbitCamera,
    pub show_points: bool,
    pub palette: ViewPalette,
    /// The vertical field of view, radians — the painter's lens value.
    pub fov_y: f32,
}

impl std::fmt::Debug for EngineViewport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineViewport")
            .field("capture", &self.capture.title)
            .field("mesh_index", &self.mesh_index)
            .field("points_index", &self.points_index)
            .field("camera", &self.camera)
            .finish()
    }
}

#[widget]
impl EngineViewport {
    fn build(&self, _ctx: &BuildContext) -> impl Into<WidgetNode> {
        let capture = Rc::clone(&self.capture);
        let mesh_index = self.mesh_index;
        let points_index = self.points_index;
        let camera = self.camera;
        let show_points = self.show_points;
        let palette = self.palette;
        let fov_y = self.fov_y;

        LayoutBuilder::new(move |constraints: Constraints| {
            let width = if constraints.has_bounded_width() {
                constraints.max_width
            } else {
                480.0
            }
            .max(2.0);
            let height = if constraints.has_bounded_height() {
                constraints.max_height
            } else {
                360.0
            }
            .max(2.0);

            let mut renderer = Renderer::new(width as u32, height as u32);
            // The receipt-quality shadow at a size a debug build can afford;
            // the release build could take the 1024 default without noticing.
            renderer.shadow_size = 512;

            let scene = Rc::new(RefCell::new(capture_scene(
                &capture,
                mesh_index,
                points_index,
                show_points,
                &palette,
            )));

            Viewport3D::new(scene, engine_camera(&camera, fov_y), renderer).into()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use three_core::{PointFrame, TimestampNs, Vec3 as ThreeVec3};

    fn frame() -> MeshFrame {
        MeshFrame {
            timestamp: TimestampNs(0),
            vertices: vec![
                ThreeVec3::new(-1.0, 0.0, 0.0),
                ThreeVec3::new(1.0, 0.0, 0.0),
                ThreeVec3::new(0.0, 1.0, 0.0),
                ThreeVec3::new(0.0, 0.0, 1.0),
            ],
            indices: vec![0, 1, 2, 0, 1, 3, 0, 2, 3, 1, 2, 3],
        }
    }

    #[test]
    fn a_frame_converts_with_normals_for_every_vertex() {
        let mesh = frame_mesh(&frame());
        assert_eq!(mesh.positions.len(), 4);
        assert_eq!(mesh.indices.len(), 12);
        assert!(mesh.has_normals(), "the engine lights per-vertex normals");
    }

    #[test]
    fn the_scene_holds_the_mesh_the_motes_and_the_lights() {
        let capture = Capture3D {
            duration_ns: 1_000_000_000,
            source: three_core::CaptureSource::Synthetic,
            cameras: vec![],
            depth: vec![],
            meshes: vec![frame()],
            points: vec![PointFrame {
                timestamp: TimestampNs(0),
                points: vec![
                    ThreeVec3::new(0.0, 1.5, 0.0),
                    ThreeVec3::new(0.5, 1.2, 0.0),
                ],
            }],
            title: "test".into(),
        };
        let palette = ViewPalette::from_theme(&ThemeData::dark());
        let scene = capture_scene(&capture, Some(0), Some(0), true, &palette);
        // mesh + motes + fill + key
        assert_eq!(scene.len(), 4);

        // Without motes: three nodes.
        let scene = capture_scene(&capture, Some(0), Some(0), false, &palette);
        assert_eq!(scene.len(), 3);

        // Without spatial data: just the lights.
        let empty = Capture3D {
            meshes: vec![],
            points: vec![],
            ..capture
        };
        let scene = capture_scene(&empty, None, None, true, &palette);
        assert_eq!(scene.len(), 2);
    }

    #[test]
    fn the_engine_camera_sits_where_the_orbit_camera_sits() {
        let orbit = OrbitCamera {
            yaw: 0.0,
            pitch: 0.0,
            distance: 2.0,
            target: ThreeVec3::new(1.0, 2.0, 3.0),
        };
        let engine = engine_camera(&orbit, 0.8);
        let eye = orbit.eye();
        assert!((engine.position.x - eye.x).abs() < 1e-6);
        assert!((engine.position.y - eye.y).abs() < 1e-6);
        assert!((engine.position.z - eye.z).abs() < 1e-6);
        assert_eq!(engine.target, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn the_engine_renders_the_demo_capture_with_fragments() {
        let capture = three_runtime::demo_capture();
        let palette = ViewPalette::from_theme(&ThemeData::dark());
        let camera = OrbitCamera {
            yaw: 0.6,
            pitch: 0.35,
            distance: 3.0,
            target: ThreeVec3::new(0.0, 0.0, 0.0),
        };
        let mut scene = capture_scene(&capture, Some(0), Some(0), true, &palette);
        let mut renderer = Renderer::new(96, 72);
        renderer.shadow_size = 128;
        let (_image, stats) = renderer.render(&mut scene, &engine_camera(&camera, 0.9));
        assert!(
            stats.fragments > 0,
            "the engine must produce pixels for the demo capture"
        );
    }
}
