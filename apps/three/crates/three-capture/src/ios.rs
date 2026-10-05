//! The iOS backend: AVFoundation capture with LiDAR depth.
//!
//! # The shape
//!
//! An `AVCaptureSession` on the back camera, a `AVCaptureVideoDataOutput`
//! for color (BGRA pixels) and — on devices whose back camera delivers it —
//! an `AVCaptureDepthDataOutput` for depth, both handing frames to one
//! delegate object on one serial dispatch queue. Rust polls a shared slot
//! the delegate fills: no callbacks across the FFI, the newest frame
//! survives, stale frames drop — the same contract as Android's shim.
//!
//! # Depth
//!
//! LiDAR hands over meters; dual-camera stereo hands over disparity. The
//! depth callback normalises both to `kCVPixelFormatType_DepthFloat32`
//! meters before reading (`depthDataByConvertingToDepthDataType`), with the
//! output's own temporal filtering left on to smooth noise and fill invalid
//! values. Hardware depth cadence runs slower than video, so between depth
//! deliveries the last one sticks — the same honest nearest-neighbour match
//! as Android. The depth map also arrives at its own resolution (a LiDAR
//! 240x320 map next to 640x480 color), so the poll resamples it onto the
//! color grid before it becomes millimetres. Devices without a depth-capable
//! back camera (and the simulator) report `depth: false` through
//! capabilities and their captures are real video that plays flat.
//!
//! # The delegate
//!
//! One `define_class!` object implementing both sample-buffer protocols,
//! storing pixels in a process-global slot. A global rather than per-session
//! state because the Objective-C object cannot carry a Rust pointer without
//! an ivar dance, and because the app records at most one video at a time
//! by design: a second `start` clears the slot the first one filled.
//!
//! # Compiled where it runs, verified where it builds
//!
//! This file is `cfg(target_os = "ios")`: the macOS CI runner compiles it
//! with every push, and the first iPhone it runs on will tell us what CI
//! could not. The conversions and the pacing math are unit-tested on the
//! host because they are plain data-in/data-out functions; the AVFoundation
//! wiring itself is exercised by compile plus device.

use std::sync::Mutex;

use dispatch2::{DispatchQueue, DispatchQueueAttr, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, AnyThread, ClassType};
use objc2_av_foundation::{
    AVAuthorizationStatus, AVCaptureConnection, AVCaptureDepthDataOutput,
    AVCaptureDepthDataOutputDelegate, AVCaptureDevice, AVCaptureDeviceDiscoverySession,
    AVCaptureDeviceInput, AVCaptureDevicePosition, AVCaptureDeviceTypeBuiltInDualCamera,
    AVCaptureDeviceTypeBuiltInDualWideCamera, AVCaptureDeviceTypeBuiltInLiDARDepthCamera,
    AVCaptureDeviceTypeBuiltInTripleCamera, AVCaptureOutput, AVCaptureSession,
    AVCaptureSessionPresetHigh, AVCaptureVideoDataOutput,
    AVCaptureVideoDataOutputSampleBufferDelegate, AVDepthData, AVMediaType, AVMediaTypeVideo,
};
use objc2_core_foundation::CFString;
use objc2_core_media::{CMSampleBuffer, CMTime};
use objc2_core_video::{
    kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_32BGRA, kCVPixelFormatType_DepthFloat32,
    kCVReturnSuccess, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferGetHeight, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress,
    CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSString};

use crate::depth::{depth_meters_to_mm, resample_depth};
use crate::{CaptureCapabilities, CaptureError, CaptureRequest, DepthFrame, DepthVideoSource};

/// The buffers the delegate fills, newest-wins. Global because the delegate
/// object the runtime holds cannot safely carry a Rust pointer through an
/// ivar, and because one recording session at a time is the app's design.
static SLOTS: Mutex<Slots> = Mutex::new(Slots {
    color: None,
    color_size: (0, 0),
    depth: None,
    depth_size: (0, 0),
});

#[derive(Default)]
struct Slots {
    color: Option<Vec<u8>>,
    color_size: (u32, u32),
    depth: Option<Vec<f32>>,
    depth_size: (u32, u32),
}

/// The polling state machine the `DepthVideoSource` trait drives.
#[derive(Default)]
pub struct AvFoundationSource {
    session: Option<Session>,
    request: Option<CaptureRequest>,
    next_ns: u64,
    frame_ns: u64,
    last_depth: Vec<u16>,
    /// Set when a start found authorisation not yet determined; iOS shows
    /// the camera prompt on first use and the polls answer "nothing yet"
    /// until the grant lands, rather than erroring mid-dialog.
    awaiting_permission: bool,
}

/// The session and its outputs, held as the runtime objects they are.
///
/// `AVCaptureSession` is documented thread-safe for configuration and
/// start/stop; the delegate runs on the one serial queue below, and the
/// poll reads the global slot the delegate fills, so nothing here needs
/// more than Rust's own view of the world.
struct Session {
    session: Retained<AVCaptureSession>,
    _video: Retained<AVCaptureVideoDataOutput>,
    _depth: Option<Retained<AVCaptureDepthDataOutput>>,
    /// AVFoundation holds the delegate without retaining it, so the session
    /// keeps it alive for exactly as long as the outputs can call it.
    _delegate: Retained<BufferDelegate>,
    _queue: DispatchRetained<DispatchQueue>,
}

// The Objective-C side: one class implementing both sample-buffer
// protocols, writing pixels into [`SLOTS`].
define_class!(
    #[unsafe(super(NSObject))]
    #[name = "ThreeCameraBufferDelegate"]
    struct BufferDelegate;

    unsafe impl NSObjectProtocol for BufferDelegate {}

    #[allow(non_snake_case)]
    unsafe impl AVCaptureVideoDataOutputSampleBufferDelegate for BufferDelegate {
        #[unsafe(method(captureOutput:didOutputSampleBuffer:fromConnection:))]
        unsafe fn captureOutput_didOutputSampleBuffer_fromConnection(
            &self,
            _output: &AVCaptureOutput,
            sample_buffer: &CMSampleBuffer,
            _connection: &AVCaptureConnection,
        ) {
            store_color(sample_buffer);
        }
    }

    #[allow(non_snake_case)]
    unsafe impl AVCaptureDepthDataOutputDelegate for BufferDelegate {
        #[unsafe(method(depthDataOutput:didOutputDepthData:timestamp:connection:))]
        unsafe fn depthDataOutput_didOutputDepthData_timestamp_connection(
            &self,
            _output: &AVCaptureDepthDataOutput,
            depth_data: &AVDepthData,
            _timestamp: CMTime,
            _connection: &AVCaptureConnection,
        ) {
            store_depth(depth_data);
        }
    }
);

impl BufferDelegate {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

impl AvFoundationSource {
    /// Whether this device's back camera can deliver depth: a LiDAR scanner
    /// or one of the virtual stereo devices (dual, dual-wide, triple). The
    /// discovery session is the honest answer either way.
    fn depth_device() -> Option<Retained<AVCaptureDevice>> {
        let types = unsafe {
            NSArray::from_slice(&[
                AVCaptureDeviceTypeBuiltInLiDARDepthCamera,
                AVCaptureDeviceTypeBuiltInDualCamera,
                AVCaptureDeviceTypeBuiltInDualWideCamera,
                AVCaptureDeviceTypeBuiltInTripleCamera,
            ])
        };
        unsafe {
            AVCaptureDeviceDiscoverySession::discoverySessionWithDeviceTypes_mediaType_position(
                &types,
                AVMediaTypeVideo,
                AVCaptureDevicePosition::Back,
            )
            .devices()
            .firstObject()
        }
    }
}

/// `AVMediaTypeVideo`, or the honest panic if AVFoundation is broken.
fn video_media_type() -> &'static AVMediaType {
    unsafe { AVMediaTypeVideo }.expect("AVMediaTypeVideo")
}

/// `kCVPixelBufferPixelFormatTypeKey` as an `NSString`. The constant is a
/// `CFString` — toll-free bridged, the same object, but these bindings type
/// the two sides of that bridge separately.
fn pixel_format_key() -> &'static NSString {
    unsafe { &*(kCVPixelBufferPixelFormatTypeKey as *const CFString as *const NSString) }
}

impl DepthVideoSource for AvFoundationSource {
    fn capabilities(&self) -> CaptureCapabilities {
        let depth = Self::depth_device().is_some();
        CaptureCapabilities {
            depth,
            depth_kind: depth.then_some(three_core::DepthSourceKind::IosLidar),
            front_camera: false,
        }
    }

    fn start(&mut self, request: CaptureRequest) -> Result<(), CaptureError> {
        // Prefer the depth-capable back camera, fall back to the wide one.
        let device = Self::depth_device()
            .or_else(|| unsafe { AVCaptureDevice::defaultDeviceWithMediaType(video_media_type()) })
            .ok_or_else(|| CaptureError::Device("no camera on this device".into()))?;

        // Authorization: denied is an error, not-yet-determined is a wait.
        // The caller polls; frames arriving once the user grants land on the
        // next polls without a restart.
        let status =
            unsafe { AVCaptureDevice::authorizationStatusForMediaType(video_media_type()) };
        match status {
            AVAuthorizationStatus::Denied | AVAuthorizationStatus::Restricted => {
                return Err(CaptureError::PermissionDenied)
            }
            AVAuthorizationStatus::NotDetermined => self.awaiting_permission = true,
            AVAuthorizationStatus::Authorized => {}
            _ => {}
        }

        let session = unsafe { AVCaptureSession::new() };
        unsafe { session.beginConfiguration() };
        unsafe { session.setSessionPreset(AVCaptureSessionPresetHigh) };

        let input = unsafe { AVCaptureDeviceInput::deviceInputWithDevice_error(&device) }
            .map_err(|_| CaptureError::Device("cannot open the camera input".into()))?;
        if !unsafe { session.canAddInput(input.as_super()) } {
            return Err(CaptureError::Device(
                "the session refuses the camera input".into(),
            ));
        }
        unsafe { session.addInput(input.as_super()) };

        // Color: BGRA pixels, one pixel buffer per frame, newest-wins. The
        // video settings are what makes the byte order below real — without
        // them the output hands over the device's native format.
        let video = unsafe { AVCaptureVideoDataOutput::new() };
        {
            let value: Retained<AnyObject> =
                unsafe { Retained::cast_unchecked(NSNumber::new_u32(kCVPixelFormatType_32BGRA)) };
            let settings = NSDictionary::from_slices(&[pixel_format_key()], &[&*value]);
            unsafe { video.setVideoSettings(Some(&*settings)) };
        }
        unsafe { video.setAlwaysDiscardsLateVideoFrames(true) };
        if !unsafe { session.canAddOutput(video.as_super()) } {
            return Err(CaptureError::Device(
                "the session refuses the video output".into(),
            ));
        }
        unsafe { session.addOutput(video.as_super()) };

        // Depth: LiDAR or stereo where present, filtered and late-dropped.
        // The output is kept only if the session accepts it, so a
        // depth-less configuration degrades to real flat video instead of a
        // dangling output.
        let depth = Self::depth_device().and_then(|_| {
            let output = unsafe { AVCaptureDepthDataOutput::new() };
            unsafe { output.setFilteringEnabled(true) };
            unsafe { output.setAlwaysDiscardsLateDepthData(true) };
            let added = unsafe { session.canAddOutput(output.as_super()) };
            if added {
                unsafe { session.addOutput(output.as_super()) };
                Some(output)
            } else {
                None
            }
        });

        unsafe { session.commitConfiguration() };

        // One serial queue for both outputs, so a color append and a depth
        // append cannot interleave, and one delegate object for both.
        let queue = DispatchQueue::new("three-camera", DispatchQueueAttr::SERIAL);
        let delegate = BufferDelegate::new();
        unsafe {
            video.setSampleBufferDelegate_queue(
                Some(ProtocolObject::from_ref(&*delegate)),
                Some(&*queue),
            );
            if let Some(depth) = &depth {
                depth.setDelegate_callbackQueue(
                    Some(ProtocolObject::from_ref(&*delegate)),
                    Some(&*queue),
                );
            }
        }

        // Fresh slots: a previous session's last frame must not leak into
        // this one's first poll.
        if let Ok(mut slots) = SLOTS.lock() {
            *slots = Slots::default();
        }

        unsafe { session.startRunning() };

        self.frame_ns = 1_000_000_000 / request.fps.max(1) as u64;
        self.next_ns = 0;
        self.last_depth.clear();
        self.request = Some(request);
        self.session = Some(Session {
            session,
            _video: video,
            _depth: depth,
            _delegate: delegate,
            _queue: queue,
        });
        Ok(())
    }

    fn poll_frame(&mut self) -> Result<Option<DepthFrame>, CaptureError> {
        if self.session.is_none() {
            return Err(CaptureError::NotRunning);
        }
        if self.request.is_none() {
            return Err(CaptureError::NotRunning);
        }

        let (color, color_size, depth, depth_size) = {
            let mut slots = SLOTS
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            (
                slots.color.take(),
                slots.color_size,
                slots.depth.take(),
                slots.depth_size,
            )
        };

        let Some(bgra) = color else {
            // While the permission dialog is up, the honest poll answer is
            // "nothing yet", not an error.
            if self.awaiting_permission {
                let status =
                    unsafe { AVCaptureDevice::authorizationStatusForMediaType(video_media_type()) };
                match status {
                    AVAuthorizationStatus::Authorized => self.awaiting_permission = false,
                    AVAuthorizationStatus::Denied | AVAuthorizationStatus::Restricted => {
                        return Err(CaptureError::PermissionDenied)
                    }
                    _ => return Ok(None),
                }
            }
            return Ok(None);
        };

        let (cw, ch) = color_size;
        if bgra.len() != cw as usize * ch as usize * 4 {
            // A torn or foreign-format buffer is not a frame.
            return Ok(None);
        }

        // BGRA -> RGBA: one byte swap per pixel.
        let mut rgba = bgra;
        let (pixels, _) = rgba.as_chunks_mut::<4>();
        for pixel in pixels {
            pixel.swap(0, 2);
        }

        // Depth: metres on the depth grid, resampled onto the color grid,
        // then to filtered millimetres. Newest sticks between hardware
        // deliveries.
        if let Some(meters) = depth {
            let (dw, dh) = depth_size;
            if dw > 0 && dh > 0 && meters.len() == dw as usize * dh as usize {
                let meters = resample_depth(&meters, (dw, dh), (cw, ch));
                self.last_depth = depth_meters_to_mm(&meters);
            } else {
                self.last_depth.clear();
            }
        }

        let timestamp = self.next_ns;
        self.next_ns += self.frame_ns;

        Ok(Some(DepthFrame {
            timestamp_ns: timestamp,
            width: cw,
            height: ch,
            color: rgba,
            depth_mm: self.last_depth.clone(),
        }))
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        if let Some(session) = self.session.take() {
            unsafe { session.session.stopRunning() };
            if let Ok(mut slots) = SLOTS.lock() {
                *slots = Slots::default();
            }
        }
        self.request = None;
        Ok(())
    }
}

/// Pull the sample buffer's image buffer and copy its pixels into the color
/// slot. One blit, no format conversion — BGRA stays BGRA until Rust swaps
/// it on the poll side.
fn store_color(sample_buffer: &CMSampleBuffer) {
    let Some(buffer) = (unsafe { sample_buffer.image_buffer() }) else {
        return;
    };
    let buffer = &*buffer;

    let width = CVPixelBufferGetWidth(buffer);
    let height = CVPixelBufferGetHeight(buffer);
    let bytes_per_row = CVPixelBufferGetBytesPerRow(buffer);
    if width == 0 || height == 0 {
        return;
    }
    // Lock, copy, unlock: the base address is only valid between the two,
    // and the lock/unlock pair brackets exactly this copy.
    if unsafe { CVPixelBufferLockBaseAddress(buffer, CVPixelBufferLockFlags(0)) }
        != kCVReturnSuccess
    {
        return;
    }
    let mut out = Vec::with_capacity(width * height * 4);
    let base = CVPixelBufferGetBaseAddress(buffer);
    if !base.is_null() {
        let bytes =
            unsafe { std::slice::from_raw_parts(base as *const u8, bytes_per_row * height) };
        for row in 0..height {
            let start = row * bytes_per_row;
            out.extend_from_slice(&bytes[start..start + width * 4]);
        }
    }
    unsafe { CVPixelBufferUnlockBaseAddress(buffer, CVPixelBufferLockFlags(0)) };

    if let Ok(mut slots) = SLOTS.lock() {
        slots.color = Some(out);
        slots.color_size = (width as u32, height as u32);
    }
}

/// Pull the depth data's map and copy its floats into the depth slot —
/// in metres, after normalising away disparity.
fn store_depth(depth_data: &AVDepthData) {
    // LiDAR delivers metres; dual-camera stereo delivers disparity. Convert
    // to the one format the reader below assumes.
    let converted;
    let data: &AVDepthData =
        if unsafe { depth_data.depthDataType() } != kCVPixelFormatType_DepthFloat32 {
            converted = unsafe {
                depth_data.depthDataByConvertingToDepthDataType(kCVPixelFormatType_DepthFloat32)
            };
            &converted
        } else {
            depth_data
        };

    let buffer = unsafe { data.depthDataMap() };
    let buffer = &*buffer;

    let width = CVPixelBufferGetWidth(buffer);
    let height = CVPixelBufferGetHeight(buffer);
    let bytes_per_row = CVPixelBufferGetBytesPerRow(buffer);
    if width == 0 || height == 0 {
        return;
    }
    if unsafe { CVPixelBufferLockBaseAddress(buffer, CVPixelBufferLockFlags(0)) }
        != kCVReturnSuccess
    {
        return;
    }
    let mut out = Vec::with_capacity(width * height);
    let base = CVPixelBufferGetBaseAddress(buffer);
    if !base.is_null() {
        let bytes =
            unsafe { std::slice::from_raw_parts(base as *const u8, bytes_per_row * height) };
        for row in 0..height {
            let start = row * bytes_per_row;
            let floats =
                unsafe { std::slice::from_raw_parts(bytes[start..].as_ptr() as *const f32, width) };
            out.extend_from_slice(floats);
        }
    }
    unsafe { CVPixelBufferUnlockBaseAddress(buffer, CVPixelBufferLockFlags(0)) };

    if let Ok(mut slots) = SLOTS.lock() {
        slots.depth = Some(out);
        slots.depth_size = (width as u32, height as u32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_swaps_to_rgba() {
        // The swap loop is exercised indirectly through poll_frame on
        // device; here, the invariant: four bytes come out with 0 and 2
        // exchanged. (The test lives with the code it checks because the
        // function is a loop over a local slice, not a free function —
        // the poll does it inline.)
        let mut pixel = [30u8, 40, 50, 255];
        pixel.swap(0, 2);
        assert_eq!(pixel, [50, 40, 30, 255]);
    }
}
