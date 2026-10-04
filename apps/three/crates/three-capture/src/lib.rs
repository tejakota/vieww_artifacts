//! Device-independent capture interfaces for depth video.
//!
//! # The one contract
//!
//! A [`DepthVideoSource`] delivers **synchronized color and depth**: every
//! frame carries RGBA pixels *and* the depth map the recording device
//! measured for those exact pixels. The depth conversion happens here, at
//! capture time, on the device that filmed — never on the viewer's phone,
//! never on a server. What crosses this trait boundary is finished footage.
//!
//! Platform implementations live behind it:
//!
//! * **Android** (`camera2`): the back camera's `YUV_420_888` stream plus
//!   `DEPTH16` where the hardware has it. On devices without a depth
//!   pipeline the source reports `None` depth and the capture is real video
//!   that plays flat — honest degradation, not a fabricated depth.
//! * **iOS** (AVFoundation): the back camera plus LiDAR `AVCaptureDepthData`,
//!   or TrueDepth on the front.
//! * **Host** (`SyntheticDepthCamera`): a deterministic generator for CI,
//!   demos and format tests.
//!
//! Nothing downstream — format, runtime, viewer — knows or cares which of
//! those produced a frame. That is the point of the boundary.

use std::fmt;
use std::time::Duration;

use three_core::{CameraIntrinsics, DepthSourceKind};

/// One synchronized color+depth observation from a recording device.
#[derive(Clone, Debug, PartialEq)]
pub struct DepthFrame {
    pub timestamp_ns: u64,
    pub width: u32,
    pub height: u32,
    /// RGBA8, row-major, top-left origin — `width * height * 4` bytes.
    pub color: Vec<u8>,
    /// Depth in millimeters, row-major — `width * height` samples,
    /// 0 = unknown. Native-depth APIs report their low-confidence samples
    /// as 0/unknown and this passes that through untouched.
    pub depth_mm: Vec<u16>,
}

impl DepthFrame {
    /// Whether this frame carries any known depth at all.
    pub fn has_depth(&self) -> bool {
        self.depth_mm.iter().any(|&mm| mm > 0)
    }
}

/// What a recording device can offer, reported before a capture starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaptureCapabilities {
    /// Whether the device produces a native depth map per frame.
    pub depth: bool,
    /// Which pipeline produces the depth, when it does.
    pub depth_kind: Option<DepthSourceKind>,
    /// Whether there is a front-facing camera wired up.
    pub front_camera: bool,
}

/// What a capture asks a device for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureRequest {
    pub duration: Duration,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
}

impl CaptureRequest {
    /// The product default: three seconds — the social moment this format
    /// was sized around — at 24 fps, 4:3.
    pub const DEFAULT_DURATION: Duration = Duration::from_secs(3);

    pub fn social_default() -> Self {
        Self {
            duration: Self::DEFAULT_DURATION,
            fps: 24,
            width: 640,
            height: 480,
        }
    }
}

impl Default for CaptureRequest {
    fn default() -> Self {
        Self::social_default()
    }
}

/// Errors common to capture backends.
#[derive(Debug)]
pub enum CaptureError {
    Unsupported(&'static str),
    Device(String),
    NotRunning,
    /// The platform camera stack denied permission. On Android this surfaces
    /// before the first frame rather than as a stream of empty ones.
    PermissionDenied,
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(s) => write!(f, "unsupported: {s}"),
            Self::Device(s) => write!(f, "device error: {s}"),
            Self::NotRunning => f.write_str("capture is not running"),
            Self::PermissionDenied => f.write_str("camera permission was denied"),
        }
    }
}
impl std::error::Error for CaptureError {}

/// A recording device producing synchronized color+depth frames.
///
/// Poll, don't callback: the same rule the vavlt picker shim follows. A
/// capture session runs its own loop and drains frames at its own cadence,
/// and a platform's callback threads (camera2's `ImageReader` handlers,
/// AVFoundation's delegate queues) stay on their side of the FFI where they
/// cannot re-enter Rust state mid-frame.
pub trait DepthVideoSource {
    /// What this device can do. Call before `start`.
    fn capabilities(&self) -> CaptureCapabilities;
    /// Begin recording at the requested cadence. Implementations may adjust
    /// to what the hardware offers (the actual fps/dimensions of every frame
    /// are reported per frame, and the capture session records what it got).
    fn start(&mut self, request: CaptureRequest) -> Result<(), CaptureError>;
    /// Drain the next available frame, if one is ready. Non-blocking.
    fn poll_frame(&mut self) -> Result<Option<DepthFrame>, CaptureError>;
    /// Stop recording and release the camera.
    fn stop(&mut self) -> Result<(), CaptureError>;
    /// The camera model this source records under. Sources with a real
    /// calibration override this; the default is the centered ~60°
    /// approximation, which the warp degrades gracefully under (the
    /// parallax is slightly wrong, never inverted).
    fn intrinsics(&self, request: &CaptureRequest) -> CameraIntrinsics {
        CameraIntrinsics::approximate(request.width, request.height)
    }
}

/// A boxed source is a source — the object-safe half of this trait, which a
/// UI needs because the concrete source is decided by the platform it runs
/// on.
impl DepthVideoSource for Box<dyn DepthVideoSource> {
    fn capabilities(&self) -> CaptureCapabilities {
        (**self).capabilities()
    }
    fn start(&mut self, request: CaptureRequest) -> Result<(), CaptureError> {
        (**self).start(request)
    }
    fn poll_frame(&mut self) -> Result<Option<DepthFrame>, CaptureError> {
        (**self).poll_frame()
    }
    fn stop(&mut self) -> Result<(), CaptureError> {
        (**self).stop()
    }
    fn intrinsics(&self, request: &CaptureRequest) -> CameraIntrinsics {
        (**self).intrinsics(request)
    }
}

// ── platform backends ─────────────────────────────────────────────────────

#[cfg(target_os = "android")]
pub mod android;
#[cfg(target_os = "android")]
pub use android::Camera2Source;

#[cfg(target_os = "ios")]
pub mod ios;
#[cfg(target_os = "ios")]
pub use ios::AvFoundationSource;

// ── the deterministic host source ────────────────────────────────────────

/// A synthetic depth camera for CI, demos and format tests.
///
/// Every frame is a pure function of its index: a moving diagonal color
/// band over a gradient, and a depth field that is a slanted plane with a
/// bump — near at the bottom, far at the top, a soft mound in the middle.
/// No clocks, no RNG: two runs produce identical captures, which is what
/// the format's tests assert on. Its depth is *labeled* `Test` so nothing
/// downstream can present it as a measurement.
pub struct SyntheticDepthCamera {
    running: bool,
    next: u64,
    request: CaptureRequest,
}

impl Default for SyntheticDepthCamera {
    // Hand-written rather than derived on purpose: the zero values are the
    // contract (a camera that has not started), not an accident of types.
    #[allow(clippy::derivable_impls)]
    fn default() -> Self {
        Self {
            running: false,
            next: 0,
            request: CaptureRequest::default(),
        }
    }
}

impl SyntheticDepthCamera {
    /// The color of frame `index` at pixel `(x, y)` — the deterministic
    /// pattern, exposed for tests that want to assert on geometry.
    pub fn pattern_color(index: u64, x: u32, y: u32, width: u32, height: u32) -> [u8; 4] {
        let phase = index * 8;
        let band = ((x + phase as u32) % 48 < 24) as u8;
        let r = (x * 255 / width.max(1)) as u8;
        let g = (y * 255 / height.max(1)) as u8;
        let b = band * 180 + 30;
        [r, g, b, 255]
    }

    /// The depth of frame `index` at pixel `(x, y)`, in millimeters — the
    /// deterministic field. A plane slanting from 3.4 m at the top to 1.6 m
    /// at the bottom, plus a mound centered on the optical axis.
    pub fn pattern_depth(index: u64, x: u32, y: u32, width: u32, height: u32) -> u16 {
        let (w, h) = (width.max(1) as i32, height.max(1) as i32);
        let (cx, cy) = ((x as i32 - w / 2), (y as i32 - h / 2));
        // The mound breathes with the frame index, so the temporal parallax
        // is visible too, not just the spatial one.
        let breathe = ((index as i32 - 3) * 90).clamp(-270, 270);
        let mound = (cx * cx + cy * cy) / (w * h / 3).max(1);
        let plane = 3400 - 1800 * y as i32 / h;
        (plane - mound * 5 + breathe).clamp(300, 8000) as u16
    }
}

impl DepthVideoSource for SyntheticDepthCamera {
    fn capabilities(&self) -> CaptureCapabilities {
        CaptureCapabilities {
            depth: true,
            depth_kind: Some(DepthSourceKind::Test),
            front_camera: false,
        }
    }

    fn start(&mut self, request: CaptureRequest) -> Result<(), CaptureError> {
        if request.fps == 0
            || request.width == 0
            || request.height == 0
            || request.duration.is_zero()
        {
            return Err(CaptureError::Device(
                "request must have non-zero fps, size and duration".into(),
            ));
        }
        self.request = request;
        self.running = true;
        self.next = 0;
        Ok(())
    }

    fn poll_frame(&mut self) -> Result<Option<DepthFrame>, CaptureError> {
        if !self.running {
            return Err(CaptureError::NotRunning);
        }
        // The frame count is computed from the exact product, not from a
        // truncated frame period: 1s at 12fps must be 12 frames, and
        // `floor(1e9/12)`-style periods let a thirteenth frame sneak under
        // the duration boundary.
        let count =
            self.request.duration.as_nanos() as u64 * self.request.fps as u64 / 1_000_000_000;
        if self.next >= count {
            return Ok(None); // the requested duration is covered
        }
        let timestamp = self.next * 1_000_000_000 / self.request.fps as u64;
        let (w, h) = (self.request.width, self.request.height);
        let index = self.next;
        self.next += 1;
        let mut color = Vec::with_capacity((w * h * 4) as usize);
        let mut depth = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                color.extend_from_slice(&Self::pattern_color(index, x, y, w, h));
                depth.push(Self::pattern_depth(index, x, y, w, h));
            }
        }
        Ok(Some(DepthFrame {
            timestamp_ns: timestamp,
            width: w,
            height: h,
            color,
            depth_mm: depth,
        }))
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        self.running = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_deterministic_and_paced() {
        let request = CaptureRequest {
            duration: Duration::from_millis(250),
            fps: 10,
            width: 32,
            height: 24,
        };
        let mut camera = SyntheticDepthCamera::default();
        assert!(camera.capabilities().depth);
        camera.start(request).unwrap();

        let mut frames = Vec::new();
        while let Some(frame) = camera.poll_frame().unwrap() {
            assert_eq!(frame.color.len(), 32 * 24 * 4);
            assert_eq!(frame.depth_mm.len(), 32 * 24);
            frames.push(frame);
        }
        assert_eq!(
            frames.len(),
            2,
            "250ms at 10fps is exactly two frame periods, floored"
        );
        assert!(frames
            .windows(2)
            .all(|pair| pair[0].timestamp_ns < pair[1].timestamp_ns));

        camera.stop().unwrap();
        assert!(matches!(camera.poll_frame(), Err(CaptureError::NotRunning)));
    }

    #[test]
    fn two_cameras_produce_identical_footage() {
        let request = CaptureRequest {
            duration: Duration::from_millis(100),
            fps: 8,
            width: 16,
            height: 12,
        };
        let mut a = SyntheticDepthCamera::default();
        let mut b = SyntheticDepthCamera::default();
        a.start(request).unwrap();
        b.start(request).unwrap();
        loop {
            let (fa, fb) = (a.poll_frame().unwrap(), b.poll_frame().unwrap());
            assert_eq!(fa, fb, "determinism: same index, same frame");
            if fa.is_none() {
                break;
            }
        }
    }

    #[test]
    fn zero_axes_are_rejected_up_front() {
        let mut camera = SyntheticDepthCamera::default();
        assert!(camera
            .start(CaptureRequest {
                duration: Duration::from_secs(1),
                fps: 0,
                width: 32,
                height: 24
            })
            .is_err());
    }
}
