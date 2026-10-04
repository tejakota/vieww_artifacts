//! Core data model and invariants for the `.3` depth-video format.
//!
//! # What a `.3` is now
//!
//! A `.3` holds an **actual video** — real camera frames, as recorded — paired
//! with a **per-frame depth map**, converted to depth at capture time by the
//! device that filmed it. What a viewer receives is therefore not a
//! reconstruction or an artist's model: it is the footage itself, plus the
//! geometry the camera measured while filming, which is exactly what a viewer
//! needs to look at that footage *from a slightly different angle*.
//!
//! This is the "3D photo / depth video" model made temporal: pinch, pan and
//! hold on a feed card and the moment moves with dimensional depth because
//! every pixel carries its own distance from the lens.
//!
//! # Who produces the geometry
//!
//! The device, not the format. [`DepthSourceKind`] records which native
//! depth pipeline produced the maps — Android `camera2` `DEPTH16`, an iPhone's
//! LiDAR, its TrueDepth front camera — because that is a statement about the
//! capture's quality that viewers and moderation tools may want to read. The
//! `Test` variant exists for the host-side development pipeline: its maps are
//! generated (not measured) and every path that reads one is expected to say
//! so out loud. The one thing this crate refuses to do is pretend a synthesized
//! map is a measured one.
//!
//! # Determinism
//!
//! Everything here is plain data: no clocks, no RNG, no platform types. A
//! capture built from the same inputs compares equal, which is what lets the
//! format's round-trip tests assert on bytes rather than on screenshots.

use std::fmt;

/// A timestamp measured from the beginning of a capture.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct TimestampNs(pub u64);

impl TimestampNs {
    /// The timestamp as seconds, for display.
    pub fn as_secs_f32(self) -> f32 {
        self.0 as f32 / 1_000_000_000.0
    }
}

/// A three-dimensional point/vector in meters, camera-space by default.
///
/// Camera space follows the pinhole model the intrinsics define: **+X right,
/// +Y down, +Z forward into the scene** — the same axes as pixel coordinates,
/// extended by depth. A pixel `(x, y)` at depth `z` sits at
/// `((x - cx) * z / fx, (y - cy) * z / fy, z)`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    /// Construct a vector from Cartesian coordinates.
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

/// A normalized quaternion used for camera orientation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quaternion {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Quaternion {
    /// Identity orientation.
    pub const fn identity() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        }
    }
}

/// A rigid transform: the pose of one camera relative to another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quaternion,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: Vec3::default(),
            rotation: Quaternion::identity(),
        }
    }
}

/// The pinhole model of the camera that recorded the video.
///
/// This is the single most important number set in the file: the depth-warp
/// renderer re-projects pixels through *exactly* these values, so a viewer
/// sitting at the neutral pose sees the footage byte-for-byte as it was
/// filmed, and every gesture away from that pose is a true re-projection
/// rather than a stretch effect.
///
/// `fx`/`fy` are focal lengths in pixels; `cx`/`cy` the principal point
/// (where the optical axis pierces the image), in pixels from the top-left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraIntrinsics {
    pub fx: f32,
    pub fy: f32,
    pub cx: f32,
    pub cy: f32,
    /// Image width in pixels. Every frame's color and depth planes match.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
}

impl CameraIntrinsics {
    /// Intrinsics for an image of `width` x `height` assuming a ~60 degree
    /// horizontal field of view and a centered principal point — the honest
    /// assumption for a phone camera when the real calibration is unavailable,
    /// and the values the host-side importer uses.
    pub fn approximate(width: u32, height: u32) -> Self {
        let (w, h) = (width.max(1) as f32, height.max(1) as f32);
        let fx = 0.5 * w / (60.0_f32.to_radians() * 0.5).tan();
        Self {
            fx,
            fy: fx,
            cx: (w - 1.0) * 0.5,
            cy: (h - 1.0) * 0.5,
            width,
            height,
        }
    }

    /// Back-project pixel `(x, y)` at depth `z` meters into camera space.
    pub fn unproject(&self, x: f32, y: f32, z: f32) -> Vec3 {
        Vec3::new((x - self.cx) * z / self.fx, (y - self.cy) * z / self.fy, z)
    }

    /// Project a camera-space point to pixels. Returns `None` behind the
    /// camera plane (`z <= 0`), where a projection has no honest meaning.
    pub fn project(&self, point: Vec3) -> Option<(f32, f32)> {
        if point.z <= f32::EPSILON {
            return None;
        }
        Some((
            self.fx * point.x / point.z + self.cx,
            self.fy * point.y / point.z + self.cy,
        ))
    }
}

/// Which native depth pipeline measured a capture's depth maps.
///
/// Stored in the file so a viewer knows what the depth is *worth*: LiDAR is
/// measured hardware depth, `DEPTH16` is whatever the device's stereo/ToF
/// pipeline reported, and `Test` is generated by the development pipeline and
/// must never be presented as measured.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DepthSourceKind {
    /// Android `camera2` `DEPTH16` output — hardware depth where the device
    /// has it. The 3-bit confidence field is filtered at capture time:
    /// samples below the threshold are stored as unknown (0).
    AndroidCamera2,
    /// An iPhone's LiDAR scanner, delivered as `AVCaptureDepthData`.
    IosLidar,
    /// An iPhone's TrueDepth front camera (the face-unlock array).
    IosTrueDepth,
    /// The recording device had no native depth to offer — an ordinary
    /// camera on a phone without a depth pipeline. The video is real; the
    /// depth planes are all unknown, and a viewer shows it flat rather
    /// than pretending.
    None,
    /// Generated by the host-side development pipeline (see
    /// `three_runtime::import`). Real video, synthesized depth — labeled so
    /// nothing downstream can mistake it for a measurement.
    Test,
}

impl DepthSourceKind {
    /// Whether this kind is a hardware measurement rather than a development
    /// fixture or an absent sensor.
    pub fn is_measured(self) -> bool {
        matches!(
            self,
            Self::AndroidCamera2 | Self::IosLidar | Self::IosTrueDepth
        )
    }
}

impl fmt::Display for DepthSourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::AndroidCamera2 => "camera2-depth16",
            Self::IosLidar => "lidar",
            Self::IosTrueDepth => "truedepth",
            Self::None => "no-depth",
            Self::Test => "test-depth",
        };
        f.write_str(name)
    }
}

/// One frame of a depth video: the color image and its depth map, together.
///
/// Color is 8-bit RGBA, row-major, `width * height * 4` bytes. Depth is
/// millimeters, row-major, `width * height` samples; **0 means unknown** —
/// the value both Android `DEPTH16` and iOS depth APIs use for "no
/// measurement here", kept verbatim so the warp renderer can skip rather
/// than guess.
#[derive(Clone, Debug, PartialEq)]
pub struct DepthVideoFrame {
    pub timestamp: TimestampNs,
    /// RGBA8 pixels, row-major, top-left origin.
    pub color: Vec<u8>,
    /// Depth in millimeters; 0 = unknown. Parallel to `color`'s pixel grid.
    pub depth: Vec<u16>,
}

impl DepthVideoFrame {
    /// A frame of `width` x `height` where every pixel is `color` with
    /// unknown depth — the smallest honest frame, used by tests and as the
    /// "no data yet" placeholder in a capture screen.
    pub fn solid(width: u32, height: u32, color: [u8; 4]) -> Self {
        Self {
            timestamp: TimestampNs(0),
            color: [color[0], color[1], color[2], color[3]]
                .repeat(width as usize * height as usize),
            depth: vec![0; width as usize * height as usize],
        }
    }
}

/// A complete depth-video capture: the unit a `.3` file stores and a feed
/// card plays.
///
/// Frames share one resolution and one [`CameraIntrinsics`] — a social
/// moment is a single continuous shot from one camera, and a cut or a
/// re-target would be a different moment. Frame timestamps are strictly
/// increasing; `duration_ns` is the whole capture, at or after the last
/// frame.
#[derive(Clone, Debug, PartialEq)]
pub struct DepthVideo {
    pub title: String,
    pub duration_ns: u64,
    /// The nominal frame rate, used by players to pace playback.
    pub fps: u32,
    pub intrinsics: CameraIntrinsics,
    pub source: DepthSourceKind,
    pub frames: Vec<DepthVideoFrame>,
}

impl DepthVideo {
    /// Validate the invariants that must hold regardless of which device
    /// produced the capture. A file that fails this never reaches a viewer;
    /// a frame that fails this never enters a file.
    pub fn validate(&self) -> Result<(), CoreError> {
        let expected = self.intrinsics.width as usize * self.intrinsics.height as usize;
        if expected == 0 {
            return Err(CoreError::Invalid("capture resolution must be non-zero"));
        }
        if self.frames.is_empty() {
            return Err(CoreError::Invalid("a capture needs at least one frame"));
        }
        if self.fps == 0 {
            return Err(CoreError::Invalid("frame rate must be non-zero"));
        }
        if self.duration_ns == 0 {
            return Err(CoreError::Invalid("capture duration must be non-zero"));
        }
        if !self.intrinsics.fx.is_finite()
            || self.intrinsics.fx <= 0.0
            || !self.intrinsics.fy.is_finite()
            || self.intrinsics.fy <= 0.0
        {
            return Err(CoreError::Invalid(
                "focal lengths must be positive and finite",
            ));
        }
        let mut previous = TimestampNs(0);
        let mut first = true;
        for frame in &self.frames {
            if frame.color.len() != expected * 4 {
                return Err(CoreError::Invalid("color plane has the wrong byte count"));
            }
            if frame.depth.len() != expected {
                return Err(CoreError::Invalid("depth plane has the wrong sample count"));
            }
            if !first && frame.timestamp <= previous {
                return Err(CoreError::Invalid(
                    "frame timestamps must be strictly increasing",
                ));
            }
            previous = frame.timestamp;
            first = false;
        }
        if self.frames.last().expect("checked non-empty").timestamp.0 >= self.duration_ns {
            return Err(CoreError::Invalid("duration must outlast the last frame"));
        }
        Ok(())
    }

    /// The distance from the camera to the middle of the subject, in meters.
    ///
    /// The **median of the first frame's known depths** — a statistic robust
    /// to the sky, the foreground finger and every other outlier a phone
    /// points at. This is where a viewer's virtual camera starts: orbit
    /// gestures swing around this point, the default dolly distance is a
    /// small multiple of it, and the near/far clamp for pinching derives
    /// from it.
    pub fn subject_distance(&self) -> f32 {
        let first = self
            .frames
            .first()
            .expect("validated captures have a frame");
        let mut known: Vec<u16> = first.depth.iter().copied().filter(|&mm| mm > 0).collect();
        if known.is_empty() {
            // A capture with no depth at all still needs a viewer default;
            // 2 meters is the "arm's length subject" the format's examples
            // use. It is a fallback for broken depth, not a measurement.
            return 2.0;
        }
        known.sort_unstable();
        let median = known[known.len() / 2];
        median as f32 / 1000.0
    }
}

/// Errors produced by the core model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    Invalid(&'static str),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for CoreError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn intrinsics() -> CameraIntrinsics {
        CameraIntrinsics {
            fx: 300.0,
            fy: 300.0,
            cx: 160.0,
            cy: 120.0,
            width: 320,
            height: 240,
        }
    }

    fn frame(timestamp: u64) -> DepthVideoFrame {
        DepthVideoFrame::solid(320, 240, [12, 34, 56, 255]).tap(timestamp)
    }

    /// Test helper: set the timestamp after construction.
    trait Tap {
        fn tap(self, timestamp: u64) -> Self;
    }
    impl Tap for DepthVideoFrame {
        fn tap(mut self, timestamp: u64) -> Self {
            self.timestamp = TimestampNs(timestamp);
            self
        }
    }

    fn video(frames: Vec<DepthVideoFrame>) -> DepthVideo {
        DepthVideo {
            title: "test".into(),
            duration_ns: 1_000_000_000,
            fps: 10,
            intrinsics: intrinsics(),
            source: DepthSourceKind::Test,
            frames,
        }
    }

    #[test]
    fn unproject_and_project_round_trip_through_the_principal_point() {
        let k = intrinsics();
        // A pixel on the principal point is on the optical axis: it
        // back-projects to (0, 0, z) and comes back to itself.
        let p = k.unproject(160.0, 120.0, 4.0);
        assert_eq!(p, Vec3::new(0.0, 0.0, 4.0));
        assert_eq!(k.project(p), Some((160.0, 120.0)));

        // A corner pixel round-trips too.
        let q = k.unproject(10.5, 20.25, 2.5);
        let (x, y) = k.project(q).expect("in front of the camera");
        assert!((x - 10.5).abs() < 1e-3 && (y - 20.25).abs() < 1e-3);
    }

    #[test]
    fn projection_rejects_points_behind_the_lens() {
        assert_eq!(intrinsics().project(Vec3::new(0.0, 0.0, -1.0)), None);
    }

    #[test]
    fn validate_rejects_the_wrong_plane_sizes() {
        let mut bad = video(vec![frame(0)]);
        bad.frames[0].color.truncate(7);
        assert!(bad.validate().is_err());

        let mut bad = video(vec![frame(0)]);
        bad.frames[0].depth.truncate(3);
        assert!(bad.validate().is_err());
    }

    #[test]
    fn validate_rejects_flat_or_backwards_time() {
        assert!(video(vec![frame(0), frame(0)]).validate().is_err());
        assert!(video(vec![frame(50), frame(10)]).validate().is_err());
        // Last frame at exactly the duration is out of bounds: the duration
        // must outlast it, or a player has no time to show the final frame.
        assert!(video(vec![frame(0), frame(1_000_000_000)])
            .validate()
            .is_err());
        assert!(video(vec![frame(0), frame(999_999_999)]).validate().is_ok());
    }

    #[test]
    fn validate_rejects_zero_axes() {
        let mut bad = video(vec![frame(0)]);
        bad.fps = 0;
        assert!(bad.validate().is_err());
        let mut bad = video(vec![frame(0)]);
        bad.duration_ns = 0;
        assert!(bad.validate().is_err());
        let mut bad = video(vec![]);
        bad.duration_ns = 1;
        assert!(bad.validate().is_err());
        let mut bad = video(vec![frame(0)]);
        bad.intrinsics.fx = 0.0;
        assert!(bad.validate().is_err());
    }

    #[test]
    fn subject_distance_is_the_median_of_known_depth() {
        let mut v = video(vec![frame(0)]);
        // Fill with a background of 3000mm, one known near pixel of 1000mm.
        v.frames[0].depth.iter_mut().for_each(|d| *d = 3000);
        v.frames[0].depth[0] = 1000;
        v.frames[0].depth[1] = 5000;
        assert!(
            (v.subject_distance() - 3.0).abs() < 1e-6,
            "median, not mean or min"
        );
    }

    #[test]
    fn subject_distance_falls_back_when_nothing_is_known() {
        let v = video(vec![frame(0)]);
        assert_eq!(v.subject_distance(), 2.0);
    }

    #[test]
    fn test_depth_is_not_marked_measured() {
        assert!(DepthSourceKind::IosLidar.is_measured());
        assert!(DepthSourceKind::AndroidCamera2.is_measured());
        assert!(!DepthSourceKind::Test.is_measured());
        assert!(
            !DepthSourceKind::None.is_measured(),
            "an absent sensor is not a measurement"
        );
    }

    #[test]
    fn approximate_intrinsics_are_centered_and_square_pixel() {
        let k = CameraIntrinsics::approximate(320, 240);
        assert_eq!(k.cx, 159.5);
        assert_eq!(k.cy, 119.5);
        assert_eq!(k.fx, k.fy);
        assert!(
            k.fx > 200.0 && k.fx < 350.0,
            "a ~60 deg hfov on 320px is ~277px"
        );
    }
}
