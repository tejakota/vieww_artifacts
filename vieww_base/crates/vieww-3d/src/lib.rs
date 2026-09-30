//! The vieww 3D layer — the Three.js / React Three Fiber / engine-renderer
//! row of the comparison document, answered on vieww's own terms.
//!
//! # Why this crate exists
//!
//! The document's 3D capabilities — a **scene graph** of `Object3D`s with
//! TRS transforms (§2.2 L2–L3), **primitive and loaded geometry** (L4),
//! **materials** from unlit to physically based (L5), **perspective and
//! orthographic cameras** (L7), **lights and shadows** (Unity, Unreal,
//! Godot, Blender EEVEE), **frustum culling**, **instancing** and **LOD**,
//! **ray picking** for pointer events (R3F L5), a **declarative reconciler**
//! over the graph (R3F L2), orbit controls (drei), and **path tracing**
//! (Blender Cycles) — had one foothold here: `Transform3::project_rect`
//! plus hand-written pipelines inside film-lab experiments. Nothing a
//! framework user could `use`.
//!
//! # Why a software rasterizer, again
//!
//! For the same reason the 2D renderer is one: vieww's rendering promise is
//! that the CPU implementation is the reference, deterministic to the byte,
//! headless, and auditable per frame. A 3D render here is a pure function of
//! (scene, camera, settings) → RGBA8, testable without a GPU, and it lands in
//! the widget tree as an ordinary image ([`Viewport3D`]) — the "Render TOP"
//! model of TouchDesigner (§2.8 L6): render the 3D to a texture, composite
//! the texture. A GPU backend can later be held to its pixels.
//!
//! # The map
//!
//! | layer | module |
//! |---|---|
//! | math (`Vector3`, `Quaternion`, `Matrix4`) | [`math`] |
//! | geometry (`BoxGeometry` …) | [`geometry`] (+ `vieww-mesh` for OBJ/STL/glTF) |
//! | glTF import and animation (`useGLTF`, `AnimationMixer`) | [`import`] |
//! | scene graph, materials, textures, lights, cameras, reconciler | [`scene`] |
//! | rasterizer, shadows, culling, stats | [`render`] |
//! | picking, projection | [`raycast`] |
//! | path tracing | [`pathtrace`] |
//! | the widget | [`widget`] |
//!
//! ```
//! use vieww_3d::{geometry, Camera, Content, Light, Material, Node, Renderer, Rgb, Scene, Vec3};
//! use vieww_foundation::Color;
//!
//! let mut scene = Scene::new();
//! scene.add(Node::new("cube", Content::mesh(geometry::box_mesh(1.0, 1.0, 1.0), Material::lambert(Color::RED))), None);
//! scene.add(Node::new("sun", Content::Light(Light::Directional {
//!     color: Rgb::WHITE, intensity: 1.0, direction: Vec3::new(-1.0, -1.0, -1.0), shadow: false,
//! })), None);
//! let camera = Camera::perspective(Vec3::new(2.0, 2.0, 3.0), Vec3::ZERO, 0.8);
//! let (image, stats) = Renderer::new(64, 48).render(&mut scene, &camera);
//! assert_eq!((image.width(), image.height()), (64, 48));
//! assert!(stats.fragments > 0);
//! ```

pub mod geometry;
pub mod import;
pub mod math;
pub mod pathtrace;
pub mod raycast;
pub mod render;
pub mod scene;
pub mod widget;

pub use import::{apply_animation, import_gltf};
pub use math::{Mat4, Quat, Vec3};
pub use pathtrace::{Accumulator, PathTracer, TraceScene};
pub use raycast::{project, raycast, Hit, Ray};
pub use render::{RenderStats, Renderer};
pub use scene::{
    Camera, Content, Filter, Light, Material, Node, NodeDesc, NodeId, OrbitControls, Projection, ReconcileStats, Rgb,
    Scene, Shading, Texture,
};
pub use widget::Viewport3D;
