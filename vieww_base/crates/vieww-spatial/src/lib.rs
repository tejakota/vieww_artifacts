//! ViewW's native spatial capability layer.
//!
//! This crate adds 3D scene description, deterministic CPU rasterization,
//! fixed-step collision simulation, particle emission, keyed timelines, stereo
//! camera helpers and a typed behavior-graph runtime. It depends only on other
//! ViewW crates and the Rust standard library; it does not embed a UI toolkit,
//! renderer framework, game engine or external physics engine.
//!
//! The CPU rasterizer currently serves as the reference/portable path and can
//! produce an ordinary [`vieww_foundation::Image`]. Native GPU 3D execution and
//! platform XR runtime adapters remain separate backend/integration work.
//!
//! # Render a cube
//!
//! ```
//! use vieww_foundation::Color;
//! use vieww_spatial::{render, Camera3, Material3, Mesh3, MeshInstance3, Projection3, Scene3, Vec3};
//!
//! let camera = Camera3::perspective(
//!     Vec3::new(0.0, 0.0, 3.0), Vec3::ZERO, Vec3::Y,
//!     1.0, 0.1, 100.0,
//! ).expect("valid camera");
//! let mut scene = Scene3::new();
//! scene.add(MeshInstance3::new(
//!     Mesh3::cube(), Material3::new(Color::rgba(40, 170, 255, 255)),
//! ));
//! let image = render(&scene, camera, 320, 240).expect("render scene");
//! assert_eq!((image.width(), image.height()), (320, 240));
//! ```

mod behavior;
mod math;
mod particles;
mod physics;
mod raster;
mod scene;
mod timeline;
mod xr;

pub use behavior::{BehaviorGraph, BehaviorNode, GraphError, GraphValue, NodeId, NodeKind};
pub use math::{Aabb3, Quat, Ray3, Transform3, Vec2, Vec3};
pub use particles::{
    BillboardParticle, Particle, ParticleEmitter, MAX_PARTICLES_PER_EMITTER,
    MAX_PARTICLE_SPAWNS_PER_UPDATE,
};
pub use physics::{
    ColliderShape, PhysicsError, PhysicsWorld, RigidBody, MAX_FIXED_STEP_SECONDS, MAX_SUBSTEPS,
};
pub use raster::{
    render, render_into, render_into_with_config, render_with_config, RasterConfig, RasterError,
    DEFAULT_MAX_PIXELS,
};
pub use scene::{
    camera_from_pose, Camera3, CameraError, Light3, Material3, Mesh3, MeshError, MeshInstance3,
    Projection3, Scene3, ShadingModel, TextureAddressMode, TextureFilter, Vertex3,
};
pub use timeline::{AnimationTrack, Keyframe, PlaybackMode};
pub use xr::{
    composite_camera_feed, controller_ray, pick_object, pick_scene, SceneHit, StereoImages,
    StereoRig, XrError, XrPose,
};

pub use vieww_animation as animation;
pub use vieww_foundation as foundation;
