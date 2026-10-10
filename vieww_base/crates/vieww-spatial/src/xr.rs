//! Platform-neutral stereo and scene-picking primitives for XR integrations.
//!
//! This module does not pretend to implement ARKit, ARCore, visionOS or WebXR
//! runtime protocols; those need per-platform camera/pose/session adapters. It
//! supplies the shared, testable scene-side contract those adapters can feed.

use std::time::Duration;

use vieww_foundation::Image;

use crate::{render, Camera3, MeshInstance3, RasterError, Ray3, Scene3, Vec3};

/// Timestamped head/eye/controller pose emitted by a platform runtime.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct XrPose {
    pub position: Vec3,
    pub orientation: crate::Quat,
    pub sample_time: Duration,
}
impl XrPose {
    pub fn camera(self, projection: crate::Projection3) -> Result<Camera3, crate::CameraError> {
        crate::camera_from_pose(self.position, self.orientation, projection)
    }
}

/// Eye separation and optional eye-to-target convergence distance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StereoRig {
    pub interpupillary_distance: f32,
    pub convergence_distance: f32,
}
impl Default for StereoRig {
    fn default() -> Self {
        Self {
            interpupillary_distance: 0.064,
            convergence_distance: 10.0,
        }
    }
}
impl StereoRig {
    /// Configure eye separation. Invalid values are preserved and reported by
    /// [`Self::cameras`] rather than silently being coerced to another rig.
    #[must_use]
    pub fn new(interpupillary_distance: f32) -> Self {
        Self {
            interpupillary_distance,
            convergence_distance: 10.0,
        }
    }

    /// Derive left and right cameras from a center camera. A positive IPD moves
    /// each eye in opposite directions along the center camera's right axis.
    pub fn cameras(self, center: Camera3) -> Result<(Camera3, Camera3), XrError> {
        if !self.interpupillary_distance.is_finite()
            || self.interpupillary_distance < 0.0
            || !self.convergence_distance.is_finite()
            || self.convergence_distance <= 0.0
        {
            return Err(XrError::InvalidStereoRig);
        }
        center.validate().map_err(XrError::Camera)?;
        let offset = center.right() * (self.interpupillary_distance * 0.5);
        let convergence = self.convergence_distance;
        let left_target = center.eye + center.forward() * convergence;
        let right_target = left_target;
        let left = Camera3::look_at(
            center.eye - offset,
            left_target,
            center.up,
            center.projection,
        )?;
        let right = Camera3::look_at(
            center.eye + offset,
            right_target,
            center.up,
            center.projection,
        )?;
        Ok((left, right))
    }

    /// Render two independent eye buffers with ViewW's software rasterizer.
    pub fn render(
        self,
        scene: &Scene3,
        center: Camera3,
        width: u32,
        height: u32,
    ) -> Result<StereoImages, XrError> {
        let (left_camera, right_camera) = self.cameras(center)?;
        let left = render(scene, left_camera, width, height).map_err(XrError::Raster)?;
        let right = render(scene, right_camera, width, height).map_err(XrError::Raster)?;
        Ok(StereoImages { left, right })
    }
}

/// Render results before a host platform submits them to a headset/compositor.
#[derive(Debug, Clone)]
pub struct StereoImages {
    pub left: Image,
    pub right: Image,
}

#[derive(Debug)]
pub enum XrError {
    Camera(crate::CameraError),
    Raster(RasterError),
    DimensionMismatch,
    InvalidOpacity,
    InvalidStereoRig,
    InvalidPose,
}
impl std::fmt::Display for XrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { Self::Camera(e) => write!(f, "XR camera: {e}"), Self::Raster(e) => write!(f, "XR render: {e}"), Self::DimensionMismatch => f.write_str("camera feed and rendered overlay dimensions must match"), Self::InvalidOpacity => f.write_str("overlay opacity must be finite"), Self::InvalidStereoRig => f.write_str("stereo rig requires finite non-negative eye separation and positive finite convergence"), Self::InvalidPose => f.write_str("XR pose position and orientation must be finite and orientation non-degenerate") }
    }
}
impl std::error::Error for XrError {}
impl From<crate::CameraError> for XrError {
    fn from(error: crate::CameraError) -> Self {
        Self::Camera(error)
    }
}

/// Composite a transparent ViewW spatial overlay over a camera feed of the same
/// dimensions. The feed itself is supplied by a permission-aware host adapter.
pub fn composite_camera_feed(
    feed: &Image,
    overlay: &Image,
    opacity: f32,
) -> Result<Image, XrError> {
    if feed.width() != overlay.width() || feed.height() != overlay.height() {
        return Err(XrError::DimensionMismatch);
    }
    if !opacity.is_finite() {
        return Err(XrError::InvalidOpacity);
    }
    let opacity = opacity.clamp(0.0, 1.0);
    let mut output = feed.pixels().to_vec();
    for (destination, source) in output
        .chunks_exact_mut(4)
        .zip(overlay.pixels().chunks_exact(4))
    {
        let source_alpha = (f32::from(source[3]) / 255.0 * opacity).clamp(0.0, 1.0);
        let destination_alpha = f32::from(destination[3]) / 255.0;
        let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
        if output_alpha <= 1.0e-6 {
            destination.fill(0);
            continue;
        }
        for (channel, output) in destination.iter_mut().take(3).enumerate() {
            let source_linear = channel_to_linear(source[channel]);
            let destination_linear = channel_to_linear(*output);
            let output_linear = (source_linear * source_alpha
                + destination_linear * destination_alpha * (1.0 - source_alpha))
                / output_alpha;
            *output = linear_to_channel(output_linear);
        }
        destination[3] = (output_alpha * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8;
    }
    Ok(Image::from_rgba8(output, feed.width(), feed.height()))
}

fn channel_to_linear(channel: u8) -> f32 {
    let value = f32::from(channel) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}
fn linear_to_channel(value: f32) -> u8 {
    let value = if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let encoded = if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8
}

/// One hit returned by spatial pointer/controller picking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneHit {
    pub object_index: usize,
    pub distance: f32,
    pub position: Vec3,
    pub normal: Vec3,
}

/// Pick the closest triangle hit among scene meshes. This is also useful for
/// mouse/touch ray interactions in ordinary 3D scenes, not just XR controllers.
#[must_use]
pub fn pick_scene(scene: &Scene3, ray: Ray3, max_distance: f32) -> Option<SceneHit> {
    if !ray.is_valid() || !max_distance.is_finite() || max_distance < 0.0 {
        return None;
    }
    let mut closest: Option<SceneHit> = None;
    for (object_index, object) in scene.objects.iter().enumerate() {
        if object.mesh.validate().is_err() || !object.transform.is_valid() {
            continue;
        }
        for triangle in object.mesh.indices.chunks_exact(3) {
            let a = object
                .transform
                .transform_point(object.mesh.vertices[triangle[0] as usize].position);
            let b = object
                .transform
                .transform_point(object.mesh.vertices[triangle[1] as usize].position);
            let c = object
                .transform
                .transform_point(object.mesh.vertices[triangle[2] as usize].position);
            let Some(distance) = ray.intersect_triangle(a, b, c) else {
                continue;
            };
            if distance > max_distance || closest.is_some_and(|hit| distance >= hit.distance) {
                continue;
            }
            let normal = (b - a).cross(c - a).normalized();
            closest = Some(SceneHit {
                object_index,
                distance,
                position: ray.at(distance),
                normal,
            });
        }
    }
    closest
}

/// A convenience helper to build a ray from a controller's world pose.
pub fn controller_ray(position: Vec3, orientation: crate::Quat) -> Result<Ray3, XrError> {
    let orientation_length_squared = orientation.length_squared();
    if !position.is_finite()
        || !orientation.is_finite()
        || !orientation_length_squared.is_finite()
        || orientation_length_squared <= 1.0e-12
    {
        return Err(XrError::InvalidPose);
    }
    let orientation = orientation.normalized();
    let ray = Ray3::new(position, orientation.rotate(Vec3::new(0.0, 0.0, -1.0)));
    if ray.is_valid() {
        Ok(ray)
    } else {
        Err(XrError::InvalidPose)
    }
}

/// Ray-pick one object. Useful when a UI hit-test already selected an object.
#[must_use]
pub fn pick_object(
    object: &MeshInstance3,
    object_index: usize,
    ray: Ray3,
    max_distance: f32,
) -> Option<SceneHit> {
    if !ray.is_valid() || !max_distance.is_finite() || max_distance < 0.0 {
        return None;
    }
    let mut closest: Option<SceneHit> = None;
    if object.mesh.validate().is_err() || !object.transform.is_valid() {
        return None;
    }
    for triangle in object.mesh.indices.chunks_exact(3) {
        let a = object
            .transform
            .transform_point(object.mesh.vertices[triangle[0] as usize].position);
        let b = object
            .transform
            .transform_point(object.mesh.vertices[triangle[1] as usize].position);
        let c = object
            .transform
            .transform_point(object.mesh.vertices[triangle[2] as usize].position);
        let Some(distance) = ray.intersect_triangle(a, b, c) else {
            continue;
        };
        if distance > max_distance || closest.is_some_and(|hit| distance >= hit.distance) {
            continue;
        }
        closest = Some(SceneHit {
            object_index,
            distance,
            position: ray.at(distance),
            normal: (b - a).cross(c - a).normalized(),
        });
    }
    closest
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Material3, Mesh3, MeshInstance3, Projection3, Vec3};
    use vieww_foundation::Color;

    #[test]
    fn stereo_views_move_in_opposite_directions() {
        let center = Camera3::look_at(
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::Y,
            Projection3::perspective(1.0, 0.1, 50.0).unwrap(),
        )
        .unwrap();
        let (left, right) = StereoRig::default().cameras(center).unwrap();
        assert!((left.eye - right.eye).length() > 0.06);
    }

    #[test]
    fn controller_ray_rejects_invalid_pose_and_normalizes_valid_pose() {
        assert!(matches!(
            controller_ray(Vec3::ZERO, crate::Quat::new(0.0, 0.0, 0.0, 0.0)),
            Err(XrError::InvalidPose)
        ));
        assert!(matches!(
            controller_ray(Vec3::new(f32::NAN, 0.0, 0.0), crate::Quat::IDENTITY),
            Err(XrError::InvalidPose)
        ));
        let ray = controller_ray(Vec3::ZERO, crate::Quat::new(0.0, 0.0, 0.0, 2.0)).unwrap();
        assert!(ray.is_valid());
        assert_eq!(ray.direction, Vec3::new(0.0, 0.0, -1.0));
    }

    #[test]
    fn scene_picker_returns_nearest_triangle() {
        let mut scene = Scene3::new();
        scene.add(
            MeshInstance3::new(Mesh3::cube(), Material3::new(Color::WHITE)).with_transform(
                crate::Transform3::new(
                    Vec3::new(0.0, 0.0, -3.0),
                    crate::Quat::IDENTITY,
                    Vec3::new(1.0, 1.0, 1.0),
                ),
            ),
        );
        let hit = pick_scene(
            &scene,
            Ray3::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0)),
            10.0,
        )
        .unwrap();
        assert_eq!(hit.object_index, 0);
        assert!(hit.distance > 2.0 && hit.distance < 3.0);
    }
}
