//! Vieww integration boundary.
//!
//! This crate intentionally does not duplicate Vieww's widget or renderer
//! architecture. It exposes the small set of media operations a Vieww-side
//! widget needs, split into two halves:
//!
//! * **Media state** ([`ThreeView`]) — playback position, looping, and which
//!   mesh/point frame is current at a timestamp. Owns the decoded
//!   [`Capture3D`](three_core::Capture3D) behind an [`Rc`](std::rc::Rc) so a
//!   painter can share it without cloning geometry every rebuild.
//! * **Presentation preparation** ([`Projector`], [`ShadedTriangle`]) — the
//!   pure geometry that turns a 3D frame into depth-sorted, flat-shaded 2D
//!   triangles under an [`OrbitCamera`]. This is deliberately *not* a
//!   renderer: it produces numbers a renderer can draw. The reference Vieww
//!   widget records them into a `Sketchbook` (see
//!   `examples/vieww-integration`); a GPU backend could feed the same numbers
//!   to a triangle rasterizer.
//!
//! The crate stays free of any UI-framework dependency on purpose. The `.3`
//! stack (core, format, capture, reconstruction, runtime) must not know how a
//! feed is rendered, and the concrete widget belongs to the Vieww side of the
//! boundary — see `docs/VIEWW-INTEGRATION.md`.
//!
//! # Coordinate conventions
//!
//! The presentation half of this crate fixes the conventions the viewer uses,
//! because projection is meaningless without them:
//!
//! * World space is right-handed, meters, **+Y up**.
//! * [`OrbitCamera`] positions the eye around a target and looks *at* it; view
//!   space is right-handed with the camera looking down **−Z**.
//! * [`Perspective`] is a vertical-field-of-view frustum.
//! * [`ScreenPoint`] is in **logical pixels with a top-left origin**, the
//!   convention 2D UI frameworks (Vieww included) paint in, so a widget can
//!   hand projected coordinates straight to its canvas without a flip.
//!
//! # Determinism
//!
//! Everything here is pure float math with no allocation hidden behind
//! traits, so a given `(frame, camera, viewport)` always produces the same
//! projected triangles — which is what lets the viewer's tests assert on
//! recorded drawings rather than on screenshots.

use three_core::{MeshFrame, Vec3};

mod camera;
mod playback;
mod presentation;
mod projection;

pub use camera::OrbitCamera;
pub use playback::ThreeView;
pub use presentation::{
    projected_points, shaded_triangles, wireframe_edges, MeshShading, ShadedTriangle, WireframeEdge,
};
pub use projection::{Perspective, Projector, ScreenPoint, Viewport};

/// A renderer-owned handle for a `.3` asset.
///
/// This is the *future* boundary for a real GPU renderer: a Vieww-side GPU
/// backend would upload mesh frames and draw the currently selected frame.
/// The reference viewer shipped in `examples/vieww-integration` does not use
/// it — it draws through the CPU presentation pipeline above, because that
/// path works on every backend Vieww has today. The trait stays so a GPU
/// adapter can slot in without touching the media stack.
pub trait ThreeRenderer {
    /// Upload or otherwise prepare one mesh frame for rendering.
    fn upload_mesh(&mut self, mesh: &MeshFrame);
    /// Draw the currently selected frame.
    fn draw(&mut self);
}

/// A unit vector pointing from surfaces toward the default key light.
///
/// Chosen so a ground-facing capture (the common phone-scan case: camera above
/// the subject) reads as lit from slightly up, slightly left, slightly toward
/// the viewer — a portrait-lighting default rather than a flat overhead one.
pub const DEFAULT_LIGHT: Vec3 = Vec3::new(-0.35, 0.8, 0.5);

#[cfg(test)]
mod tests {
    use super::*;
    use three_core::{Capture3D, CaptureSource, PointFrame, TimestampNs};

    pub fn wave_capture() -> Capture3D {
        Capture3D {
            duration_ns: 1_000_000_000,
            source: CaptureSource::Synthetic,
            cameras: vec![],
            depth: vec![],
            meshes: vec![
                MeshFrame {
                    timestamp: TimestampNs(0),
                    vertices: vec![
                        Vec3::new(-1.0, 0.0, 0.0),
                        Vec3::new(1.0, 0.0, 0.0),
                        Vec3::new(0.0, 0.5, 0.0),
                    ],
                    indices: vec![0, 1, 2],
                },
                MeshFrame {
                    timestamp: TimestampNs(1_000_000_000),
                    vertices: vec![
                        Vec3::new(-1.0, 0.0, 0.0),
                        Vec3::new(1.0, 0.0, 0.0),
                        Vec3::new(0.0, 1.5, 0.0),
                    ],
                    indices: vec![0, 1, 2],
                },
            ],
            points: vec![PointFrame { timestamp: TimestampNs(0), points: vec![Vec3::new(0.0, 2.0, 0.0)] }],
            title: "test".into(),
        }
    }

    #[test]
    fn crate_reexports_match_the_documented_surface() {
        // Compile-time smoke: every name the integration guide names resolves.
        let _view = ThreeView::new(wave_capture());
        let _camera = OrbitCamera::default();
        let _perspective = Perspective::default();
        let _viewport = Viewport::new(320.0, 240.0);
        let _: Option<&MeshFrame> = _view.current_mesh();
    }
}
