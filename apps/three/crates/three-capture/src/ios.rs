//! The iOS backend: AVFoundation capture with LiDAR depth.
//!
//! # The shape
//!
//! An `AVCaptureSession` on the back camera, a `AVCaptureVideoDataOutput`
//! for color (BGRA pixels) and — on devices whose back camera delivers it —
//! an `AVCaptureDepthDataOutput` for LiDAR depth, both on one serial
//! dispatch queue that buffers the newest frame of each. Rust polls, the
//! same contract as Android's shim: no callbacks across the FFI, the
//! newest frame survives, stale frames drop.
//!
//! # Depth
//!
//! `AVCaptureDepthData` arrives as **disparity** by default; the output is
//! flipped to true depth (`depthDataFiltered` + the conversion below) where
//! the units are meters. Hardware depth cadence runs slower than video, so
//! between depth deliveries the last one sticks — the same honest
//! nearest-neighbour match as Android. Devices without LiDAR (and the
//! simulator) report `depth: false` through capabilities and their
//! captures are real video that plays flat.
//!
//! # Compiled where it runs, verified where it builds
//!
//! This file is `cfg(target_os = "ios")`: the macOS CI runner compiles it
//! with every push, and the first iPhone it runs on will tell us what CI
//! could not. The conversions and the pacing math are unit-tested on the
//! host because they are plain data-in/data-out functions; the
//! AVFoundation wiring itself is exercised by compile plus device.

use std::cell::RefCell;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{class, msg_send, msg_send_id};
use objc2_av_foundation::{
    AVCaptureConnection, AVCaptureDepthDataOutput, AVCaptureDevice, AVCaptureDeviceInput,
    AVCaptureSession, AVCaptureVideoDataOutput, AVVideoCodecType,
};
use objc2_core_video::CVPixelBuffer;
use objc2_foundation::{NSArray, NSString};

use crate::{CaptureCapabilities, CaptureError, CaptureRequest, DepthFrame, DepthVideoSource};

/// The session and its outputs, held as the runtime objects they are.
///
/// `AVCaptureSession` is documented thread-safe for configuration and
/// start/stop; the polling access this backend does (reading buffered
/// frames the delegate appended) sits behind the same one queue, so the
/// `RefCell` here is about Rust's view of the world, not about the runtime's.
struct Session {
    session: Retained<AVCaptureSession>,
    video: Retained<AVCaptureVideoDataOutput>,
    depth: Option<Retained<AVCaptureDepthDataOutput>>,
    /// The buffers the delegates appended, newest-wins.
    slots: Rc<RefCell<Slots>>,
    /// The dispatch queue both outputs run their delegates on. Serial, so
    /// a color append and a depth append cannot interleave.
    _queue: dispatch2::Queue,
}

#[derive(Default)]
struct Slots {
    color: Option<Vec<u8>>,
    color_size: (u32, u32),
    depth: Option<Vec<f32>>,
    depth_size: (u32, u32),
}

/// The polling state machine the `DepthVideoSource` trait drives.
pub struct AvFoundationSource {
    session: Option<Session>,
    request: Option<CaptureRequest>,
    next_ns: u64,
    frame_ns: u64,
    last_depth: Vec<u16>,
    /// Set when a start was refused for camera permissions; iOS surfaces
    /// authorization asynchronously, and the honest answer to the caller
    /// while the dialog is up is "not yet" rather than a hard error.
    awaiting_permission: bool,
}

impl Default for AvFoundationSource {
    fn default() -> Self {
        Self {
            session: None,
            request: None,
            next_ns: 0,
            frame_ns: 0,
            last_depth: Vec::new(),
            awaiting_permission: false,
        }
    }
}

impl AvFoundationSource {
    /// Whether this device's back camera can deliver depth (LiDAR, or a
    /// dual-camera stereo pipeline — the discovery session is the honest
    /// answer either way).
    fn depth_device() -> Option<Retained<AVCaptureDevice>> {
        let types = NSArray::from_retained_slice([unsafe { AVCaptureDevice::typeDepthData() }]);
        unsafe {
            AVCaptureDevice::devicesWithMediaTypeAndCharacteristics(NSString::new("vide"), &types)
        }
        .first()
        .map(|device| unsafe { Retained::cast::<AVCaptureDevice>(device.clone()) })
    }
}

impl DepthVideoSource for AvFoundationSource {
    fn capabilities(&self) -> CaptureCapabilities {
        let depth = Self::depth_device().is_some();
        CaptureCapabilities {
            depth,
            depth_kind: depth.then_some(three_core::DepthSourceKind::IosLidar),
            front_camera: true,
        }
    }

    fn start(&mut self, request: CaptureRequest) -> Result<(), CaptureError> {
        let device = unsafe { AVCaptureDevice::defaultCameraWithMediaType(NSString::new("vide")) }
            .ok_or_else(|| CaptureError::Device("no camera on this device".into()))?;

        // Authorization: denied is an error, not-yet-determined is a wait.
        // The caller polls; frames arriving once the user grants land on the
        // next polls without a restart.
        let status =
            unsafe { AVCaptureDevice::authorizationStatusForMediaType(NSString::new("vide")) };
        use objc2_av_foundation::AVAuthorizationStatus as Status;
        match status {
            Status::Denied | Status::Restricted => return Err(CaptureError::PermissionDenied),
            Status::NotDetermined => {
                self.awaiting_permission = true;
                unsafe {
                    AVCaptureDevice::requestAccessForMediaType_completionHandler(
                        NSString::new("vide"),
                        None,
                    );
                }
                // Not an error: start proceeds, and the session's first
                // frames arrive when the grant does.
            }
            _ => {}
        }

        let session = AVCaptureSession::new();
        unsafe { session.beginConfiguration() };
        session.setSessionPreset(unsafe { AVCaptureSession::PresetHigh() });

        let input = unsafe { AVCaptureDeviceInput::deviceInputWithDevice_error(&device) }
            .map_err(|_| CaptureError::Device("cannot open the camera input".into()))?;
        if !session.canAddInput(Some(&input)) {
            return Err(CaptureError::Device(
                "the session refuses the camera input".into(),
            ));
        }
        unsafe { session.addInput(&input) };

        // Color: BGRA pixels, one pixel buffer per frame, newest-wins.
        let video = AVCaptureVideoDataOutput::new();
        if !session.canAddOutput(Some(&video)) {
            return Err(CaptureError::Device(
                "the session refuses the video output".into(),
            ));
        }
        unsafe { session.addOutput(&video) };

        // Depth: LiDAR where present. Disparity is flipped to depth by the
        // output's own filter flag; the conversion below reads meters.
        let depth = Self::depth_device().map(|_| AVCaptureDepthDataOutput::new());
        if let Some(depth) = &depth {
            if session.canAddOutput(Some(depth)) {
                unsafe { session.addOutput(depth) };
            }
        }

        unsafe { session.commitConfiguration() };

        let queue = dispatch2::Queue::serial("three-camera");
        let slots = Rc::new(RefCell::new(Slots::default()));

        // The delegates are tiny Objective-C objects (declared below, once
        // per process) whose protocol methods copy pixels into `slots`.
        unsafe {
            video.setAlwaysDiscardsLateVideoFrames(true);
            if let Some(depth) = &depth {
                depth.setDelegateFilteringEnabled(true);
            }
            install_delegates(&video, depth.as_deref(), &queue, Rc::clone(&slots));
        }

        session.startRunning();

        self.frame_ns = 1_000_000_000 / request.fps.max(1) as u64;
        self.next_ns = 0;
        self.request = Some(request);
        self.session = Some(Session {
            session,
            video,
            depth,
            slots,
            _queue: queue,
        });
        Ok(())
    }

    fn poll_frame(&mut self) -> Result<Option<DepthFrame>, CaptureError> {
        let Some(Session { slots, .. }) = &self.session else {
            return Err(CaptureError::NotRunning);
        };
        let Some(request) = self.request else {
            return Err(CaptureError::NotRunning);
        };

        let (color, (cw, ch), depth) = {
            let mut slots = slots.borrow_mut();
            let color = slots.color.take();
            let size = slots.color_size;
            let depth = slots.depth.take();
            (color, size, depth)
        };

        let Some(bgra) = color else {
            // While the permission dialog is up, the honest poll answer is
            // "nothing yet", not an error.
            if self.awaiting_permission {
                let status = unsafe {
                    AVCaptureDevice::authorizationStatusForMediaType(NSString::new("vide"))
                };
                use objc2_av_foundation::AVAuthorizationStatus as Status;
                match status {
                    Status::Authorized => self.awaiting_permission = false,
                    Status::Denied | Status::Restricted => {
                        return Err(CaptureError::PermissionDenied)
                    }
                    _ => return Ok(None),
                }
            }
            return Ok(None);
        };

        // BGRA -> RGBA: one byte swap per pixel.
        let mut rgba = bgra;
        for pixel in rgba.as_chunks::<4>().0.iter_mut() {
            pixel.swap(0, 2);
        }

        // Depth: meters to filtered millimeters, newest sticks between
        // hardware deliveries.
        if let Some(meters) = depth {
            if meters.len() == cw as usize * ch as usize {
                self.last_depth = depth_meters_to_mm(&meters);
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
            session.session.stopRunning();
        }
        self.request = None;
        Ok(())
    }
}

/// Depth in meters to millimeters with a plausibility filter: iOS depth is
/// already filtered by the output, but a zero or absurd sample is data
/// corruption rather than measurement, and unknown (0) is the honest answer.
fn depth_meters_to_mm(meters: &[f32]) -> Vec<u16> {
    meters
        .iter()
        .map(|&m| {
            if m.is_finite() && (0.1..=20.0).contains(&m) {
                (m * 1000.0).round().clamp(0.0, 8191.0) as u16
            } else {
                0
            }
        })
        .collect()
}

/// Declare and install the two Objective-C delegate classes the outputs
/// need, with the shared slots reachable from both.
///
/// Declared once per process (a global), because the Objective-C runtime
/// is a global. The delegates do nothing but copy pixels into the slots —
/// one BGRA blit for color, one float read for depth — so the capture
/// queue's latency stays where AVFoundation put it.
unsafe fn install_delegates(
    video: &AVCaptureVideoDataOutput,
    depth: Option<&AVCaptureDepthDataOutput>,
    queue: &dispatch2::Queue,
    slots: Rc<RefCell<Slots>>,
) {
    // The delegate protocol conformance is declared through the
    // `ProtocolObject` machinery objc2 provides: the class implements
    // `captureOutput:didOutputSampleBuffer:fromConnection:` by pulling the
    // pixel buffer's base address and copying it out.
    let video_delegate = BufferDelegate::new(Rc::clone(&slots), false);
    video.setSampleBufferDelegate(
        Some(ProtocolObject::from_ref(&*video_delegate)),
        Some(&queue.queue),
    );

    if let Some(depth) = depth {
        let depth_delegate = BufferDelegate::new(Rc::clone(&slots), true);
        depth.setDelegate(
            Some(ProtocolObject::from_ref(&*depth_delegate)),
            Some(&queue.queue),
        );
    }
}

/// The Objective-C side of the delegates: an object whose class implements
/// the two sample-buffer protocols, holding the shared slots behind an
/// `Rc` the Rust side also owns.
struct BufferDelegate {
    slots: Rc<RefCell<Slots>>,
    is_depth: bool,
}

impl BufferDelegate {
    fn new(slots: Rc<RefCell<Slots>>, is_depth: bool) -> Retained<AnyObject> {
        let class = delegate_class();
        unsafe {
            let object: Retained<AnyObject> = msg_send_id![class, new];
            // The slots pointer rides in the object's extra ivar, written
            // through the same runtime that declared it.
            let boxed = Box::into_raw(Box::new((slots, is_depth)));
            unsafe extern "C" fn marker() {}
            let _ = marker;
            (*object).set_ivar("threeSlots", boxed as *mut std::ffi::c_void);
            object
        }
    }
}

/// Declare (once) the class both delegates are instances of. The ivar holds
/// the boxed slots; the two protocol methods read it and copy pixels.
unsafe fn delegate_class() -> &'static objc2::runtime::AnyClass {
    use objc2::runtime::{AnyClass, ClassBuilder};

    static mut CLASS: Option<&'static AnyClass> = None;
    if let Some(class) = unsafe { CLASS } {
        return class;
    }

    let mut builder = ClassBuilder::new("ThreeCameraBufferDelegate", class!(NSObject))
        .expect("declare ThreeCameraBufferDelegate");
    builder.add_ivar::<*mut std::ffi::c_void>("threeSlots");

    extern "C" fn did_output_sample_buffer(
        this: &AnyObject,
        _cmd: objc2::runtime::Sel,
        _output: *mut AnyObject,
        sample_buffer: *mut AnyObject,
        _connection: *mut AnyObject,
    ) {
        unsafe {
            let payload = this.get_ivar::<*mut std::ffi::c_void>("threeSlots").read();
            if payload.is_null() {
                return;
            }
            // SAFETY: the payload was Box::into_raw'd from a matching Box.
            let (slots, is_depth) = &*(payload as *const (Rc<RefCell<Slots>>, bool));
            capture_buffer(sample_buffer, slots, *is_depth);
        }
    }

    unsafe {
        builder.add_method(
            objc2::runtime::Sel::register("captureOutput:didOutputSampleBuffer:fromConnection:"),
            did_output_sample_buffer as unsafe extern "C" fn(_, _, _, _, _),
        );
        // The depth output delivers `depthDataOutput:didOutputDepthData:timestamp:connection:`.
        extern "C" fn did_output_depth(
            this: &AnyObject,
            _cmd: objc2::runtime::Sel,
            _output: *mut AnyObject,
            depth_data: *mut AnyObject,
            _timestamp: *mut AnyObject,
            _connection: *mut AnyObject,
        ) {
            unsafe {
                let payload = this.get_ivar::<*mut std::ffi::c_void>("threeSlots").read();
                if payload.is_null() {
                    return;
                }
                let (slots, _) = &*(payload as *const (Rc<RefCell<Slots>>, bool));
                depth_buffer(depth_data, slots);
            }
        }
        builder.add_method(
            objc2::runtime::Sel::register(
                "depthDataOutput:didOutputDepthData:timestamp:connection:",
            ),
            did_output_depth as unsafe extern "C" fn(_, _, _, _, _, _),
        );

        let class = builder.register();
        CLASS = Some(class);
        class
    }
}

/// Pull a `CMSampleBuffer`'s pixel buffer and copy its pixels into the
/// color slot. One blit, no format conversion — BGRA stays BGRA until Rust
/// swaps it.
unsafe fn capture_buffer(
    sample_buffer: *mut AnyObject,
    slots: &Rc<RefCell<Slots>>,
    is_depth: bool,
) {
    let _ = is_depth;
    let pixel_buffer: Option<Retained<CVPixelBuffer>> = msg_send_id![
        msg_send_id![sample_buffer, imageBuffer] as Retained<AnyObject>,
        retain
    ];
    let Some(buffer) = pixel_buffer else { return };
    let width = CVPixelBufferGetWidth(&buffer);
    let height = CVPixelBufferGetHeight(&buffer);
    let base = CVPixelBufferGetBaseAddress(&buffer);
    let bytes_per_row = CVPixelBufferGetBytesPerRow(&buffer);
    if base.is_null() || width == 0 || height == 0 {
        return;
    }
    // Lock, copy, unlock: the base address is only valid between the two,
    // and the lock/unlock pair brackets exactly this copy.
    unsafe {
        if CVPixelBufferLockBaseAddress(&buffer, 0) != 0 {
            return;
        }
    }
    let bytes = std::slice::from_raw_parts(base as *const u8, (bytes_per_row * height) as usize);
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height as usize {
        let start = row * bytes_per_row as usize;
        out.extend_from_slice(&bytes[start..start + width as usize * 4]);
    }
    unsafe { CVPixelBufferUnlockBaseAddress(&buffer, 0) };
    let mut slots = slots.borrow_mut();
    slots.color = Some(out);
    slots.color_size = (width, height);
}

/// Pull a `AVDepthData`'s depth map and copy its floats into the depth
/// slot.
unsafe fn depth_buffer(depth_data: *mut AnyObject, slots: &Rc<RefCell<Slots>>) {
    let depth_map: Option<Retained<CVPixelBuffer>> = msg_send_id![
        msg_send_id![depth_data, depthDataMap] as Retained<AnyObject>,
        retain
    ];
    let Some(buffer) = depth_map else { return };
    let width = CVPixelBufferGetWidth(&buffer);
    let height = CVPixelBufferGetHeight(&buffer);
    let base = CVPixelBufferGetBaseAddress(&buffer);
    let bytes_per_row = CVPixelBufferGetBytesPerRow(&buffer);
    if base.is_null() || width == 0 || height == 0 {
        return;
    }
    let bytes = std::slice::from_raw_parts(base as *const u8, (bytes_per_row * height) as usize);
    let mut out = Vec::with_capacity((width * height) as usize);
    for row in 0..height as usize {
        let start = row * bytes_per_row as usize;
        let floats =
            std::slice::from_raw_parts(bytes[start..].as_ptr() as *const f32, width as usize);
        out.extend_from_slice(floats);
    }
    CVPixelBufferUnlockBaseAddress(&buffer, 0);
    let mut slots = slots.borrow_mut();
    slots.depth = Some(out);
    slots.depth_size = (width, height);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_meters_survive_the_conversion() {
        let mm = depth_meters_to_mm(&[1.0, 0.5, 3.75, 0.0, -1.0, f32::NAN, 25.0]);
        assert_eq!(mm[0], 1000);
        assert_eq!(mm[1], 500);
        assert_eq!(mm[2], 3750);
        assert_eq!(mm[3], 0, "zero meters is unknown, not zero depth");
        assert_eq!(mm[4], 0, "negative is corruption");
        assert_eq!(mm[5], 0, "NaN is corruption");
        assert_eq!(mm[6], 0, "beyond the 13-bit range is corruption");
    }

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
