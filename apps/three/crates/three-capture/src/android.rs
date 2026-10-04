//! The Android backend: camera2 through the Java shim, polled from Rust.
//!
//! # The shape
//!
//! `ThreeCameraShim.java` — compiled and dexed by this crate's `build.rs`,
//! loaded through `InMemoryDexClassLoader` — owns every camera2 object: the
//! session, the `ImageReader`s, the handler thread. Rust polls it: one JNI
//! call per frame, no callbacks across the FFI, no `RegisterNatives`. This is
//! the vavlt picker pattern, and for the same reasons: android-activity
//! never surfaces Java callbacks to Rust, and the poll cost (one static call
//! per frame) is nothing next to the pixels it returns.
//!
//! # What arrives per frame
//!
//! Color is the shim's repacked `[Y][U][V]` planes; the YUV→RGBA conversion
//! below is the standard BT.601 matrix. Depth is raw DEPTH16 shorts —
//! depth in millimeters in the low 13 bits, confidence in the top 3 — and
//! samples below the confidence threshold become unknown (0), the same
//! value the file format and the warp renderer already treat as "nothing
//! measured here". A device with no DEPTH16 output reports that honestly
//! through capabilities, and its captures are real video that plays flat.
//!
//! # Time
//!
//! Frame timestamps come from arrival order, not the camera clock (whose
//! epoch is a device secret): the capture session paces itself against its
//! own frame count, exactly like the synthetic source, so a capture's
//! timeline is always capture-relative nanoseconds.

use jni::objects::{GlobalRef, JObject, JValue};
use jni::{AttachGuard, JNIEnv, JavaVM};

use crate::{CaptureCapabilities, CaptureError, CaptureRequest, DepthFrame, DepthVideoSource};

/// Compiled from `java/ThreeCameraShim.java` by `build.rs`.
const DEX: &[u8] = include_bytes!(env!("THREE_DEX"));

const CLASS_NAME: &str = "dev/three/app/ThreeCameraShim";

/// Mirrors the shim's constants. An int, so the values cross as ints.
mod state {
    pub const PERMISSION_DENIED: i32 = 4;
    pub const ERROR: i32 = 5;
    pub const STOPPED: i32 = 3;
}

/// Below this DEPTH16 confidence (0-7, in the sample's top three bits), a
/// depth sample is treated as unknown rather than believed.
///
/// 3 of 7: the point where the measurement is right more often than it is
/// wrong. The alternative — keeping every sample — makes the warp jitter
/// on devices whose stereo depth is uncertain everywhere, and a jittering
/// parallax reads as broken, not as honest.
const DEPTH_CONFIDENCE_MIN: i32 = 3;

/// The camera2 backend. Construct one per activity; a capture screen owns it
/// on the main thread, where the shim wants to be driven from anyway.
pub struct Camera2Source {
    vm: JavaVM,
    activity: GlobalRef,
    class: GlobalRef,
    request: Option<CaptureRequest>,
    /// The frame pacing clock: capture-relative nanoseconds, one frame
    /// period per delivered frame.
    next_ns: u64,
    frame_ns: u64,
    intrinsics: Option<three_core::CameraIntrinsics>,
    /// The most recent depth map, converted. Hardware depth cadences run
    /// slower than video, so between depth deliveries the last one sticks —
    /// the honest nearest-neighbour match.
    last_depth: Vec<u16>,
}

impl Camera2Source {
    /// Load the shim and resolve the class. Call once at startup, with the
    /// activity's `AndroidApp` handle.
    pub fn new(app: &vieww_platform_winit::AndroidApp) -> Result<Self, CaptureError> {
        let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }
            .map_err(|e| CaptureError::Device(format!("JavaVM from android-activity: {e}")))?;

        // Scoped: the attach guard borrows the VM, and the VM moves into
        // `Self` at the end. Both refs the guard produces are global.
        let (activity, class) = {
            let mut env = vm
                .attach_current_thread()
                .map_err(|e| CaptureError::Device(format!("attach main thread: {e}")))?;

            // SAFETY: android-activity owns this reference for the lifetime
            // of the process; we immediately promote it to a global ref.
            let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
            let activity = env
                .new_global_ref(&activity)
                .map_err(|e| CaptureError::Device(format!("global activity ref: {e}")))?;

            // The dex is 'static, so the direct buffer it backs stays valid
            // for as long as the class loader might touch it.
            let buf = unsafe {
                env.new_direct_byte_buffer(DEX.as_ptr() as *mut u8, DEX.len())
                    .map_err(|e| CaptureError::Device(format!("direct buffer over the dex: {e}")))?
            };
            let parent = env
                .call_method(
                    activity.as_obj(),
                    "getClassLoader",
                    "()Ljava/lang/ClassLoader;",
                    &[],
                )
                .map_err(|e| CaptureError::Device(format!("getClassLoader: {e}")))?
                .l()
                .map_err(|e| CaptureError::Device(format!("class loader object: {e}")))?;
            let loader = env
                .new_object(
                    "dalvik/system/InMemoryDexClassLoader",
                    "(Ljava/nio/ByteBuffer;Ljava/lang/ClassLoader;)V",
                    &[JValue::Object(&buf), JValue::Object(&parent)],
                )
                .map_err(|e| CaptureError::Device(format!("InMemoryDexClassLoader: {e}")))?;
            let name = env
                .new_string(CLASS_NAME)
                .map_err(|e| CaptureError::Device(format!("class name string: {e}")))?;
            let class = env
                .call_method(
                    &loader,
                    "loadClass",
                    "(Ljava/lang/String;)Ljava/lang/Class;",
                    &[JValue::Object(&name)],
                )
                .map_err(|e| CaptureError::Device(format!("loadClass ThreeCameraShim: {e}")))?
                .l()
                .map_err(|e| CaptureError::Device(format!("class object: {e}")))?;
            let class = env
                .new_global_ref(&class)
                .map_err(|e| CaptureError::Device(format!("global class ref: {e}")))?;

            (activity, class)
        };

        Ok(Self {
            vm,
            activity,
            class,
            request: None,
            next_ns: 0,
            frame_ns: 0,
            intrinsics: None,
            last_depth: Vec::new(),
        })
    }

    /// Attach this thread and run one closure against the JVM. Every JNI
    /// call the backend makes goes through here, which is what keeps the
    /// exception discipline in one place: a failed call clears its pending
    /// exception instead of poisoning the next one.
    fn with_env<T>(
        &self,
        f: impl FnOnce(&mut JNIEnv) -> Result<T, CaptureError>,
    ) -> Result<T, CaptureError> {
        let mut env = self
            .vm
            .attach_current_thread()
            .map_err(|e| CaptureError::Device(format!("attach: {e}")))?;
        let result = f(&mut env);
        if result.is_err() {
            // A pending Java exception poisons every later JNI call on this
            // thread, so it is cleared here rather than left for the next one.
            let _ = env.exception_describe();
            let _ = env.exception_clear();
        }
        result
    }

    fn shim_state(&self) -> Result<i32, CaptureError> {
        self.with_env(|env| {
            Ok(env
                .call_static_method(&self.class, "state", "()I", &[])
                .map_err(|e| CaptureError::Device(format!("state(): {e}")))?
                .i()
                .map_err(|e| CaptureError::Device(format!("state() returns {e}")))?)
        })
    }

    /// Read `LENS_INTRINSIC_CALIBRATION` through the shim, scaled to the
    /// requested resolution — `None` when the device publishes nothing,
    /// which is common and falls back to the ~60-degree approximation.
    fn read_intrinsics(
        &self,
        env: &mut JNIEnv,
        width: u32,
        height: u32,
    ) -> Option<three_core::CameraIntrinsics> {
        let values = env
            .call_static_method(
                &self.class,
                "intrinsics",
                "(Landroid/content/Context;)[F",
                &[JValue::Object(self.activity.as_obj())],
            )
            .ok()?
            .l()
            .ok()?;
        let array: jni::objects::JFloatArray = values.into();
        let len = env.get_array_length(&array).ok()?;
        let mut raw = vec![0.0f32; len as usize];
        env.get_float_array_region(&array, 0, &mut raw).ok()?;
        if raw.len() < 4 || raw.iter().any(|f| !f.is_finite() || *f <= 0.0) {
            return None;
        }
        Some(three_core::CameraIntrinsics {
            fx: raw[0],
            fy: raw[1],
            cx: raw[2],
            cy: raw[3],
            width,
            height,
        })
    }
}

impl DepthVideoSource for Camera2Source {
    fn capabilities(&self) -> CaptureCapabilities {
        let depth = self
            .with_env(|env| {
                Ok(env
                    .call_static_method(
                        &self.class,
                        "depthSupported",
                        "(Landroid/content/Context;)Z",
                        &[JValue::Object(self.activity.as_obj())],
                    )
                    .map_err(|e| CaptureError::Device(format!("depthSupported(): {e}")))?
                    .z()
                    .map_err(|e| CaptureError::Device(format!("depthSupported() returns {e}")))?)
            })
            .unwrap_or(false);
        CaptureCapabilities {
            depth,
            depth_kind: depth.then_some(three_core::DepthSourceKind::AndroidCamera2),
            front_camera: false,
        }
    }

    fn start(&mut self, request: CaptureRequest) -> Result<(), CaptureError> {
        // The shim's start is (Activity, int, int, int).
        self.with_env(|env| {
            env.call_static_method(
                &self.class,
                "start",
                "(Landroid/app/Activity;III)I",
                &[
                    JValue::Object(self.activity.as_obj()),
                    JValue::Int(request.width as i32),
                    JValue::Int(request.height as i32),
                    JValue::Int(request.fps as i32),
                ],
            )
            .map_err(|e| CaptureError::Device(format!("start(): {e}")))?
            .i()
            .map_err(|e| CaptureError::Device(format!("start() returns {e}")))?;
            Ok(())
        })?;

        // A permission denial is the one start failure with an honest
        // dedicated answer; everything else either starts or errors on the
        // first poll.
        match self.shim_state()? {
            state::PERMISSION_DENIED => Err(CaptureError::PermissionDenied),
            state::ERROR => Err(CaptureError::Device("camera2 failed to start".into())),
            _ => {
                self.request = Some(request);
                self.next_ns = 0;
                self.frame_ns = 1_000_000_000 / request.fps.max(1) as u64;
                let intrinsics = self
                    .with_env(|env| Ok(self.read_intrinsics(env, request.width, request.height)))?;
                self.intrinsics = intrinsics;
                Ok(())
            }
        }
    }

    fn poll_frame(&mut self) -> Result<Option<DepthFrame>, CaptureError> {
        let Some(request) = self.request else {
            return Err(CaptureError::NotRunning);
        };
        let (w, h) = (request.width as usize, request.height as usize);

        // Everything JNI happens in one scoped closure that only reads
        // `self`; the mutations (pacing clock, sticky depth) happen after
        // the guard is gone, which is the whole trick of this structure.
        let outcome: Result<Option<DepthFrame>, CaptureError> = self.with_env(|env| {
            let color: Option<Vec<u8>> = env
                .call_static_method(&self.class, "pollColor", "()[B", &[])
                .map_err(|e| CaptureError::Device(format!("pollColor(): {e}")))?
                .l()
                .ok()
                .and_then(|object| {
                    let array: jni::objects::JByteArray = object.into();
                    env.convert_byte_array(&array).ok()
                });

            let Some(yuv) = color else {
                // No new frame. An error state surfaces as an error, not
                // silence — read through the env already held rather than
                // re-attaching (a nested attach is a detach-count footgun
                // this close to the FFI).
                let current = env
                    .call_static_method(&self.class, "state", "()I", &[])
                    .map_err(|e| CaptureError::Device(format!("state(): {e}")))?
                    .i()
                    .map_err(|e| CaptureError::Device(format!("state() returns {e}")))?;
                if current == state::ERROR {
                    return Err(CaptureError::Device(
                        "camera2 entered an error state".into(),
                    ));
                }
                if current == state::PERMISSION_DENIED {
                    return Err(CaptureError::PermissionDenied);
                }
                if current == state::STOPPED {
                    return Err(CaptureError::NotRunning);
                }
                return Ok(None);
            };

            // Depth is optional: absent on most devices, stale between
            // polls when present. The last depth sticks (see the field).
            let fresh_depth: Option<Vec<u8>> = env
                .call_static_method(&self.class, "pollDepth", "()[B", &[])
                .map_err(|e| CaptureError::Device(format!("pollDepth(): {e}")))?
                .l()
                .ok()
                .and_then(|object| {
                    let array: jni::objects::JByteArray = object.into();
                    env.convert_byte_array(&array).ok()
                });
            let fresh_depth = match fresh_depth {
                Some(bytes) if bytes.len() == w * h * 2 => Some(depth16_to_mm(&bytes)),
                _ => None,
            };

            let color = yuv_to_rgba(&yuv, w, h).ok_or_else(|| {
                CaptureError::Device("the color plane has the wrong size for its resolution".into())
            })?;

            Ok(Some(DepthFrame {
                timestamp_ns: 0, // assigned by the caller, post-guard
                width: request.width,
                height: request.height,
                color,
                depth_mm: fresh_depth.unwrap_or_default(),
            }))
        });

        let Some(mut frame) = outcome? else {
            return Ok(None);
        };
        if !frame.depth_mm.is_empty() {
            self.last_depth = std::mem::take(&mut frame.depth_mm);
        }
        frame.timestamp_ns = self.next_ns;
        self.next_ns += self.frame_ns;
        frame.depth_mm = self.last_depth.clone();
        Ok(Some(frame))
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        self.with_env(|env| {
            env.call_static_method(&self.class, "stop", "()I", &[])
                .map_err(|e| CaptureError::Device(format!("stop(): {e}")))?;
            Ok(())
        })?;
        self.request = None;
        Ok(())
    }

    fn intrinsics(&self, request: &CaptureRequest) -> three_core::CameraIntrinsics {
        self.intrinsics.unwrap_or_else(|| {
            three_core::CameraIntrinsics::approximate(request.width, request.height)
        })
    }
}

/// BT.601 YUV (planar, 4:2:0) to RGBA — the standard matrix, the standard
/// clamping, one pixel per iteration.
///
/// Returns `None` when the planes are the wrong size for the claimed
/// resolution, which is the only way this can fail and the only input it
/// refuses to guess about.
fn yuv_to_rgba(yuv: &[u8], width: usize, height: usize) -> Option<Vec<u8>> {
    let chroma_w = width.div_ceil(2);
    let chroma_h = height.div_ceil(2);
    let y_len = width * height;
    let expected = y_len + chroma_w * chroma_h * 2;
    if yuv.len() < expected {
        return None;
    }
    let (y_plane, rest) = yuv.split_at(y_len);
    let (u_plane, v_plane) = rest.split_at(chroma_w * chroma_h);
    let mut rgba = Vec::with_capacity(y_len * 4);
    for row in 0..height {
        for column in 0..width {
            let y = y_plane[row * width + column] as i32;
            let u = u_plane[(row / 2) * chroma_w + column / 2] as i32 - 128;
            let v = v_plane[(row / 2) * chroma_w + column / 2] as i32 - 128;
            // BT.601 full-range:
            //   r = y            + 1.402 v
            //   g = y - 0.344 u - 0.714 v
            //   b = y + 1.772 u
            let r = (y + (91881 * v) / 65536).clamp(0, 255) as u8;
            let g = (y - (22554 * u + 46802 * v) / 65536).clamp(0, 255) as u8;
            let b = (y + (116130 * u) / 65536).clamp(0, 255) as u8;
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    Some(rgba)
}

/// DEPTH16 shorts to filtered millimeters: depth in the low 13 bits,
/// confidence in the top 3, and anything under the threshold is unknown.
fn depth16_to_mm(bytes: &[u8]) -> Vec<u16> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let sample = u16::from_le_bytes([pair[0], pair[1]]);
            let confidence = (sample >> 13) as i32;
            if confidence >= DEPTH_CONFIDENCE_MIN {
                sample & 0x1FFF
            } else {
                0
            }
        })
        .collect()
}

// The attach guard is used only inside `with_env`; this alias exists for
// the signature's readability and future callers.
#[allow(dead_code)]
type AttachAlias<'a> = AttachGuard<'a>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yuv_black_is_rgba_black() {
        // Y=16 is studio-swing black; with full-range interpretation the
        // output must be dark, not negative-clamped-to-zero weirdness.
        let (w, h) = (4usize, 4usize);
        let mut yuv = vec![16u8; w * h];
        yuv.extend(vec![128u8; (w / 2) * (h / 2)]); // u
        yuv.extend(vec![128u8; (w / 2) * (h / 2)]); // v
        let rgba = yuv_to_rgba(&yuv, w, h).expect("sizes agree");
        assert_eq!(rgba.len(), w * h * 4);
        for pixel in rgba.as_chunks::<4>().0 {
            assert_eq!(pixel[3], 255);
            assert!(
                pixel[0] <= 20 && pixel[1] <= 20 && pixel[2] <= 20,
                "{pixel:?}"
            );
        }
    }

    #[test]
    fn yuv_white_is_white() {
        let (w, h) = (2usize, 2usize);
        let mut yuv = vec![235u8; w * h];
        yuv.extend(vec![128u8; 1]);
        yuv.extend(vec![128u8; 1]);
        let rgba = yuv_to_rgba(&yuv, w, h).expect("sizes agree");
        let pixel = &rgba[0..4];
        assert!(
            pixel[0] > 200 && pixel[1] > 200 && pixel[2] > 200,
            "{pixel:?}"
        );
    }

    #[test]
    fn yuv_rejects_short_planes() {
        assert!(yuv_to_rgba(&[0; 10], 8, 8).is_none());
    }

    #[test]
    fn depth16_confidence_and_range_unpack() {
        // 3000 mm, confidence 7 -> kept.
        let kept = 3000u16 | (7 << 13);
        // 2500 mm, confidence 1 -> dropped to unknown.
        let dropped = 2500u16 | (1 << 13);
        let bytes: Vec<u8> = [kept, dropped]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let mm = depth16_to_mm(&bytes);
        assert_eq!(mm, vec![3000, 0]);
    }
}
