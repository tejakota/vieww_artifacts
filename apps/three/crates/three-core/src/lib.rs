//! Core types shared by the `.3` format, capture pipeline, reconstruction
//! pipeline and playback runtime.
//!
//! The crate intentionally has no camera, GPU, filesystem or UI dependency.
//! Keeping these types boring is a feature: every platform should be able to
//! produce and consume the same temporal 3D model.

use std::fmt;

/// A timestamp measured from the beginning of a capture.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct TimestampNs(pub u64);

/// A three-dimensional point/vector in meters.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    /// Construct a vector from Cartesian coordinates.
    pub const fn new(x: f32, y: f32, z: f32) -> Self { Self { x, y, z } }
}

/// A normalized quaternion used for device/camera/object orientation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quaternion {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Quaternion {
    /// Identity orientation.
    pub const fn identity() -> Self { Self { x: 0.0, y: 0.0, z: 0.0, w: 1.0 } }
}

/// A rigid transform in the capture coordinate system.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quaternion,
}

impl Default for Transform {
    fn default() -> Self { Self { translation: Vec3::default(), rotation: Quaternion::identity() } }
}

/// A single tracked camera observation.
#[derive(Clone, Debug, PartialEq)]
pub struct CameraSample {
    pub timestamp: TimestampNs,
    pub pose: Transform,
    pub focal_length_px: (f32, f32),
    pub principal_point_px: (f32, f32),
    pub image_size_px: (u32, u32),
}

/// Optional depth observation associated with a camera frame.
#[derive(Clone, Debug, PartialEq)]
pub struct DepthSample {
    pub timestamp: TimestampNs,
    pub width: u32,
    pub height: u32,
    /// Depth values in millimeters. Zero means "unknown".
    pub millimeters: Vec<u16>,
}

/// One reconstructed mesh snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshFrame {
    pub timestamp: TimestampNs,
    pub vertices: Vec<Vec3>,
    /// Triangle indices. Every group of three indices forms one triangle.
    pub indices: Vec<u32>,
}

/// One point-cloud snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct PointFrame {
    pub timestamp: TimestampNs,
    pub points: Vec<Vec3>,
}

/// How the capture obtained its spatial information.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureSource {
    Rgb,
    RgbWithDepth,
    RgbWithLidar,
    MultiCamera,
    Synthetic,
}

impl fmt::Display for CaptureSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Rgb => "rgb",
            Self::RgbWithDepth => "rgb+depth",
            Self::RgbWithLidar => "rgb+lidar",
            Self::MultiCamera => "multi-camera",
            Self::Synthetic => "synthetic",
        };
        f.write_str(name)
    }
}

/// A complete temporal 3D capture before serialization.
#[derive(Clone, Debug, PartialEq)]
pub struct Capture3D {
    pub duration_ns: u64,
    pub source: CaptureSource,
    pub cameras: Vec<CameraSample>,
    pub depth: Vec<DepthSample>,
    pub meshes: Vec<MeshFrame>,
    pub points: Vec<PointFrame>,
    pub title: String,
}

/// The axis-aligned bounds of a capture, as `(min, max)` corners.
///
/// `None` when the capture carries no spatial data at all (no meshes, no point
/// clouds) — a capture with only camera samples can still be valid, but it has
/// nothing to frame in a viewer.
///
/// This is a presentation concern living in the core types because every
/// consumer that wants to frame, cull or auto-fit needs the same definition:
/// the union of all mesh vertices and point-cloud points across the whole
/// timeline. Camera poses are deliberately excluded — the camera moves around
/// the subject, and including it would make the subject appear to shrink every
/// time the device orbited further away.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl Bounds {
    /// The center of the box.
    pub fn center(&self) -> Vec3 {
        Vec3::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
            (self.min.z + self.max.z) * 0.5,
        )
    }

    /// Half the diagonal of the box.
    ///
    /// A sphere of this radius around [`center`](Self::center) encloses the
    /// whole capture, which is what an auto-fitting camera wants: whatever
    /// direction it looks from, the subject stays inside the frustum.
    pub fn radius(&self) -> f32 {
        let dx = self.max.x - self.min.x;
        let dy = self.max.y - self.min.y;
        let dz = self.max.z - self.min.z;
        0.5 * (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

impl Capture3D {
    /// The union bounds of every mesh vertex and point across the whole
    /// timeline. See [`Bounds`] for what is included and why.
    pub fn bounds(&self) -> Option<Bounds> {
        let mut bounds: Option<Bounds> = None;
        let mut include = |p: &Vec3| {
            let b = bounds.get_or_insert(Bounds { min: *p, max: *p });
            b.min.x = b.min.x.min(p.x);
            b.min.y = b.min.y.min(p.y);
            b.min.z = b.min.z.min(p.z);
            b.max.x = b.max.x.max(p.x);
            b.max.y = b.max.y.max(p.y);
            b.max.z = b.max.z.max(p.z);
        };
        for mesh in &self.meshes {
            for vertex in &mesh.vertices {
                include(vertex);
            }
        }
        for frame in &self.points {
            for point in &frame.points {
                include(point);
            }
        }
        bounds
    }

    /// Validate invariants that must hold regardless of reconstruction method.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.duration_ns == 0 {
            return Err(CoreError::Invalid("capture duration must be non-zero"));
        }
        for mesh in &self.meshes {
            if mesh.indices.len() % 3 != 0 {
                return Err(CoreError::Invalid("mesh indices must be a multiple of three"));
            }
            if mesh.indices.iter().any(|&i| i as usize >= mesh.vertices.len()) {
                return Err(CoreError::Invalid("mesh index points outside vertex array"));
            }
        }
        for depth in &self.depth {
            let expected = depth.width as usize * depth.height as usize;
            if depth.millimeters.len() != expected {
                return Err(CoreError::Invalid("depth sample has the wrong pixel count"));
            }
        }
        Ok(())
    }
}

/// Errors produced by the core model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    Invalid(&'static str),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self { Self::Invalid(message) => f.write_str(message) }
    }
}

impl std::error::Error for CoreError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture(meshes: Vec<MeshFrame>, points: Vec<PointFrame>) -> Capture3D {
        Capture3D {
            duration_ns: 1,
            source: CaptureSource::Synthetic,
            cameras: vec![],
            depth: vec![],
            meshes,
            points,
            title: String::new(),
        }
    }

    fn mesh(vertices: Vec<Vec3>) -> MeshFrame {
        MeshFrame { timestamp: TimestampNs(0), vertices, indices: vec![0, 1, 2] }
    }

    #[test]
    fn bounds_union_mesh_and_point_data_across_time() {
        let capture = capture(
            vec![
                mesh(vec![
                    Vec3::new(-1.0, 0.0, -1.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                ]),
                // A later frame that extends the box on +y and +z.
                mesh(vec![
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(0.5, 2.0, 0.5),
                    Vec3::new(0.5, 0.0, 0.5),
                ]),
            ],
            vec![PointFrame { timestamp: TimestampNs(0), points: vec![Vec3::new(0.0, -3.0, 0.0)] }],
        );

        let bounds = capture.bounds().expect("spatial data present");
        assert_eq!(bounds.min, Vec3::new(-1.0, -3.0, -1.0));
        assert_eq!(bounds.max, Vec3::new(1.0, 2.0, 0.5));
    }

    #[test]
    fn bounds_center_and_radius_of_a_cube() {
        let capture = capture(
            vec![mesh(vec![
                Vec3::new(-1.0, -1.0, -1.0),
                Vec3::new(1.0, 1.0, 1.0),
                Vec3::new(1.0, 1.0, 1.0),
            ])],
            vec![],
        );

        let bounds = capture.bounds().unwrap();
        assert_eq!(bounds.center(), Vec3::new(0.0, 0.0, 0.0));
        // Half the diagonal of a cube with edge 2 is sqrt(3).
        assert!((bounds.radius() - 3.0_f32.sqrt()).abs() < 1e-6);
    }

    #[test]
    fn bounds_is_none_without_spatial_data() {
        let capture = capture(vec![], vec![]);
        assert!(capture.bounds().is_none());
    }

    #[test]
    fn bounds_ignores_camera_poses() {
        let mut capture = capture(vec![mesh(vec![
            Vec3::new(-0.1, -0.1, -0.1),
            Vec3::new(0.1, 0.1, 0.1),
            Vec3::new(0.0, 0.0, 0.0),
        ])], vec![]);
        capture.cameras.push(CameraSample {
            timestamp: TimestampNs(0),
            pose: Transform {
                translation: Vec3::new(100.0, 100.0, 100.0),
                rotation: Quaternion::identity(),
            },
            focal_length_px: (500.0, 500.0),
            principal_point_px: (320.0, 240.0),
            image_size_px: (640, 480),
        });

        let bounds = capture.bounds().unwrap();
        assert_eq!(bounds.max, Vec3::new(0.1, 0.1, 0.1));
    }
}
