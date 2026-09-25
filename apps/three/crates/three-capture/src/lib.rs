//! Platform-neutral capture interfaces.
//!
//! Android and iOS implementations belong behind these traits. This prevents
//! camera APIs from leaking into the `.3` format or Vieww UI code.

use std::{fmt, time::Duration};
use three_core::{CameraSample, DepthSample};

pub mod platform;

/// Raw image delivered by a camera backend.
#[derive(Clone, Debug)]
pub struct CameraFrame {
    pub timestamp_ns: u64,
    pub width: u32,
    pub height: u32,
    /// Packed image bytes owned by the backend until reconstruction consumes it.
    pub pixels: Vec<u8>,
}

/// Optional inertial measurement sample.
#[derive(Clone, Copy, Debug, Default)]
pub struct ImuSample {
    pub timestamp_ns: u64,
    pub acceleration: [f32; 3],
    pub gyroscope: [f32; 3],
}

/// Capabilities advertised by a device.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaptureCapabilities {
    pub depth: bool,
    pub lidar: bool,
    pub multi_camera: bool,
    pub imu: bool,
}

/// Errors common to capture backends.
#[derive(Debug)]
pub enum CaptureError {
    Unsupported(&'static str),
    Device(String),
    NotRunning,
}
impl fmt::Display for CaptureError { fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result { match self {Self::Unsupported(s)=>write!(f,"unsupported: {s}"),Self::Device(s)=>write!(f,"device error: {s}"),Self::NotRunning=>f.write_str("capture is not running")}} }
impl std::error::Error for CaptureError {}

/// Abstract camera source.
pub trait CameraSource {
    /// Return hardware capabilities before starting capture.
    fn capabilities(&self) -> CaptureCapabilities;
    /// Start the source at the requested target cadence.
    fn start(&mut self, target_fps: u32) -> Result<(), CaptureError>;
    /// Poll one frame. Real implementations should avoid blocking the UI thread.
    fn poll_frame(&mut self) -> Result<Option<CameraFrame>, CaptureError>;
    /// Stop capture and release camera resources.
    fn stop(&mut self) -> Result<(), CaptureError>;
}

/// Optional depth source paired with a camera source.
pub trait DepthSource {
    fn poll_depth(&mut self) -> Result<Option<DepthSample>, CaptureError>;
}

/// Optional motion source used to improve camera pose estimation.
pub trait ImuSource {
    fn poll_imu(&mut self) -> Result<Option<ImuSample>, CaptureError>;
}

/// Output of a tracking stage. Reconstruction can consume this without caring
/// whether the pose came from ARKit, ARCore, SLAM, or another implementation.
#[derive(Clone, Debug)]
pub struct TrackingSample {
    pub camera: CameraSample,
    pub confidence: f32,
}

/// Configuration for a short social-media capture.
#[derive(Clone, Copy, Debug)]
pub struct CaptureConfig {
    pub duration: Duration,
    pub target_fps: u32,
    pub require_depth: bool,
}

impl Default for CaptureConfig {
    fn default() -> Self { Self { duration: Duration::from_secs(3), target_fps: 30, require_depth: false } }
}

/// A deterministic camera used for CI, demos and format tests.
#[derive(Debug, Default)]
pub struct SyntheticCamera {
    running: bool,
    next: u64,
    fps: u32,
}
impl CameraSource for SyntheticCamera {
    fn capabilities(&self) -> CaptureCapabilities { CaptureCapabilities { imu: true, ..CaptureCapabilities::default() } }
    fn start(&mut self, target_fps:u32)->Result<(),CaptureError>{ if target_fps==0{return Err(CaptureError::Device("target fps cannot be zero".into()))}; self.running=true; self.next=0; self.fps=target_fps; Ok(()) }
    fn poll_frame(&mut self)->Result<Option<CameraFrame>,CaptureError>{ if !self.running{return Err(CaptureError::NotRunning)}; let timestamp=self.next * 1_000_000_000 / self.fps as u64; self.next+=1; Some(CameraFrame{timestamp_ns:timestamp,width:16,height:16,pixels:vec![0;16*16*3]}).into_ok() }
    fn stop(&mut self)->Result<(),CaptureError>{self.running=false;Ok(())}
}

trait IntoOk<T> { fn into_ok(self) -> Result<Option<T>, CaptureError>; }
impl<T> IntoOk<T> for Option<T> { fn into_ok(self)->Result<Option<T>,CaptureError>{Ok(self)} }
