//! Playback, capture orchestration and the development import pipeline for
//! `.3` depth-video media.
//!
//! # The player is a pure function of accumulated time
//!
//! [`DepthVideoPlayer`] never reads a clock. A frame loop hands it deltas via
//! [`advance`](DepthVideoPlayer::advance) and reads
//! [`frame_index`](DepthVideoPlayer::frame_index) — the same contract the
//! framework's own animation and video crates use, for the same reasons: a
//! dropped frame skips ahead instead of stretching, a test hands it a
//! `Duration` instead of sleeping, and the same input always produces the
//! same frame. Social feed playback, the viewer's scrub bar and every test
//! share one definition of "where are we".
//!
//! # The session is a poll loop, not a callback graph
//!
//! [`CaptureSession::run`] drives any [`DepthVideoSource`] — the synthetic
//! host camera, Android camera2, or iOS AVFoundation — through one loop:
//! start, drain, stop, and a `.3` at the end. The depth conversion already
//! happened on the device by the time frames arrive here.
//!
//! # The import pipeline is honest about its depth
//!
//! [`import_mp4`] shells out to **ffmpeg** to turn any real video into a
//! `.3` on a development machine. Its depth is *generated*, labeled
//! [`DepthSourceKind::Test`], and exists so the format, the warp renderer and
//! the viewer can be exercised with real footage before any phone is
//! involved. It is a development tool, not a product feature; nothing in this
//! crate ever presents its depth as measured.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use three_capture::{CaptureRequest, DepthVideoSource};
use three_core::{CameraIntrinsics, DepthSourceKind, DepthVideo, DepthVideoFrame, TimestampNs};
use three_format::{decode, encode, FormatError};

/// Serialize a depth-video capture to `.3` bytes.
pub fn save(capture: &DepthVideo) -> Result<Vec<u8>, FormatError> {
    encode(capture)
}

/// Open `.3` bytes for playback.
pub fn open(bytes: &[u8]) -> Result<DepthVideo, FormatError> {
    decode(bytes)
}

/// Errors from this runtime.
#[derive(Debug)]
pub enum RuntimeError {
    Format(FormatError),
    /// The source failed mid-capture.
    Capture(three_capture::CaptureError),
    /// The source ended before the requested duration.
    TooShort {
        got_frames: usize,
    },
    /// The import pipeline failed — ffmpeg missing, frames unreadable, etc.
    Import(String),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Format(e) => write!(f, "{e}"),
            Self::Capture(e) => write!(f, "{e}"),
            Self::TooShort { got_frames } => {
                write!(f, "capture produced only {got_frames} frames")
            }
            Self::Import(s) => write!(f, "import: {s}"),
        }
    }
}

impl std::error::Error for RuntimeError {}
impl From<FormatError> for RuntimeError {
    fn from(value: FormatError) -> Self {
        Self::Format(value)
    }
}
impl From<three_capture::CaptureError> for RuntimeError {
    fn from(value: three_capture::CaptureError) -> Self {
        Self::Capture(value)
    }
}
impl From<three_core::CoreError> for RuntimeError {
    fn from(value: three_core::CoreError) -> Self {
        Self::Format(FormatError::Core(value))
    }
}

// ── playback ─────────────────────────────────────────────────────────────

/// Where in a capture the playback clock sits: play/pause, loop, rate, and
/// the frame those imply.
///
/// Owned by whatever drives the app's frames (the Vieww side), advanced by
/// deltas, read for the current frame index. It never allocates, never reads
/// a wall clock, and never touches the capture's pixels — pairing it with a
/// decoded capture is the Vieww widget's job.
pub struct DepthVideoPlayer {
    duration_ns: u64,
    fps: u32,
    frame_count: usize,
    time_ns: u64,
    playing: bool,
    looping: bool,
    rate: f32,
}

impl DepthVideoPlayer {
    /// A player at time zero, paused.
    pub fn new(capture: &DepthVideo) -> Self {
        Self::new_from_meta(&three_format::CaptureMeta {
            title: capture.title.clone(),
            duration_ns: capture.duration_ns,
            fps: capture.fps,
            intrinsics: capture.intrinsics,
            source: capture.source,
            frame_count: capture.frames.len(),
        })
    }

    /// A player from a capture's metadata alone — the lazy path, where the
    /// frames stay compressed and the clock is all that exists up front.
    pub fn new_from_meta(meta: &three_format::CaptureMeta) -> Self {
        Self {
            duration_ns: meta.duration_ns.max(1),
            fps: meta.fps.max(1),
            frame_count: meta.frame_count,
            time_ns: 0,
            playing: false,
            looping: true,
            rate: 1.0,
        }
    }

    pub fn play(&mut self) {
        self.playing = true;
    }
    pub fn pause(&mut self) {
        self.playing = false;
    }
    pub fn toggle(&mut self) {
        self.playing = !self.playing;
    }
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }
    pub fn is_looping(&self) -> bool {
        self.looping
    }

    /// Playback rate, 1.0 = recorded speed. Negative rates play backward
    /// when looping, which is a legitimate thing to do to a three-second
    /// moment.
    pub fn set_rate(&mut self, rate: f32) {
        if rate.is_finite() && rate != 0.0 {
            self.rate = rate;
        }
    }
    pub fn rate(&self) -> f32 {
        self.rate
    }

    /// Where the clock sits, nanoseconds since the capture started.
    pub fn time_ns(&self) -> u64 {
        self.time_ns
    }

    /// How far through, `0.0 ..= 1.0`.
    pub fn progress(&self) -> f32 {
        self.time_ns as f32 / self.duration_ns as f32
    }

    /// Jump the clock. Clamped into the capture, because a scrub bar can
    /// hand over anything.
    pub fn seek(&mut self, progress: f32) {
        let clamped = progress.clamp(0.0, 1.0);
        self.time_ns = (clamped * self.duration_ns as f32) as u64;
    }

    /// Move time forward (or backward, with a negative rate) by `delta`
    /// scaled by the playback rate, wrapping when looping and parking at the
    /// ends otherwise.
    pub fn advance(&mut self, delta: Duration) {
        if !self.playing {
            return;
        }
        let scaled = delta.as_nanos() as f64 * self.rate as f64;
        if scaled >= 0.0 {
            self.time_ns = self.time_ns.saturating_add(scaled as u64);
        } else {
            self.time_ns = self.time_ns.saturating_sub((-scaled) as u64);
        }
        if self.time_ns >= self.duration_ns {
            if self.looping {
                self.time_ns %= self.duration_ns;
            } else {
                self.time_ns = self.duration_ns - 1;
                self.playing = false;
            }
        }
    }

    /// The frame the current time shows. `None` only for a capture with no
    /// frames, which `validate` refuses anyway — the player still returns
    /// `None` rather than indexing, because "no frames yet" is a state a
    /// capture screen honestly passes through.
    pub fn frame_index(&self) -> Option<usize> {
        if self.frame_count == 0 {
            return None;
        }
        let frame_ns = 1_000_000_000u64 / self.fps as u64;
        let index = (self.time_ns / frame_ns.max(1)) as usize;
        Some(index.min(self.frame_count - 1))
    }
}

// ── capture ──────────────────────────────────────────────────────────────

/// An in-flight recording a UI drives one frame at a time.
///
/// [`CaptureSession`] runs a whole capture synchronously — right for tests
/// and the importer, wrong for a UI whose main thread also paints. This is
/// the same state machine cut into steps: [`start`](Recording::start) on
/// the button press, [`poll`](Recording::poll) once per frame tick until it
/// returns `false`, [`finish`](Recording::finish) to get the capture. The
/// source's own pacing decides when the requested duration is covered,
/// exactly as in the session.
pub struct Recording<S: DepthVideoSource> {
    source: S,
    request: CaptureRequest,
    frames: Vec<DepthVideoFrame>,
    title: String,
}

impl<S: DepthVideoSource> Recording<S> {
    /// Begin recording: starts the source and returns the driver.
    pub fn start(
        mut source: S,
        request: CaptureRequest,
        title: impl Into<String>,
    ) -> Result<Self, RuntimeError> {
        source.start(request)?;
        Ok(Self {
            source,
            request,
            frames: Vec::new(),
            title: title.into(),
        })
    }

    /// Drain whatever the source has ready. Returns whether recording is
    /// still in flight.
    ///
    /// Completion is the **frame count**, not the source's silence: a real
    /// camera returns "nothing ready" between deliveries mid-recording, and
    /// that must not end the capture. The target is the requested duration
    /// at the requested cadence; a source that over-delivers stops at it, a
    /// source that under-delivers keeps the recording open until it does.
    pub fn poll(&mut self) -> bool {
        while let Ok(Some(frame)) = self.source.poll_frame() {
            self.frames.push(DepthVideoFrame {
                timestamp: TimestampNs(frame.timestamp_ns),
                color: frame.color,
                depth: frame.depth_mm,
            });
            if self.frames.len() >= self.target_frames() {
                return false;
            }
        }
        self.frames.len() < self.target_frames()
    }

    /// How many frames the request asks for, rounded down — the completion
    /// target, and the same count the synthetic source produces.
    fn target_frames(&self) -> usize {
        (self.request.duration.as_nanos() as u64 * self.request.fps as u64 / 1_000_000_000) as usize
    }

    /// Frames collected so far — the progress a UI shows.
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// The fraction of the requested duration already collected.
    pub fn progress(&self) -> f32 {
        let wanted = self.request.duration.as_nanos() as f32;
        let have = self.frames.len() as f32 / self.request.fps.max(1) as f32 * 1_000_000_000.0;
        (have / wanted.max(1.0)).clamp(0.0, 1.0)
    }

    /// Stop the source and assemble the capture.
    pub fn finish(mut self) -> Result<DepthVideo, RuntimeError> {
        self.source.stop()?;
        if self.frames.is_empty() {
            return Err(RuntimeError::TooShort { got_frames: 0 });
        }
        let capabilities = self.source.capabilities();
        let kind =
            if capabilities.depth && self.frames.iter().any(|f| f.depth.iter().any(|&mm| mm > 0)) {
                capabilities.depth_kind.unwrap_or(DepthSourceKind::None)
            } else {
                DepthSourceKind::None
            };
        let intrinsics = self.source.intrinsics(&self.request);
        let capture = DepthVideo {
            title: self.title,
            duration_ns: self.request.duration.as_nanos() as u64,
            fps: self.request.fps,
            intrinsics,
            source: kind,
            frames: self.frames,
        };
        capture.validate()?;
        Ok(capture)
    }
}

/// A recording over a type-erased source — the shape a UI holds, since the
/// concrete source is decided by the platform the app happens to run on.
pub type DynRecording = Recording<Box<dyn DepthVideoSource>>;

impl DynRecording {
    /// Begin recording on whatever source the platform provided.
    pub fn start_boxed(
        source: Box<dyn DepthVideoSource>,
        request: CaptureRequest,
        title: impl Into<String>,
    ) -> Result<Self, RuntimeError> {
        Recording::start(source, request, title)
    }
}

/// Drives a [`DepthVideoSource`] through one complete capture and returns
/// the finished [`DepthVideo`].
///
/// The loop is deliberately dumb: start, drain every frame the source
/// offers, stop. Pacing belongs to the source (a real camera produces frames
/// on its own schedule; the synthetic one produces them as fast as asked),
/// and *not* sleeping in here is what lets the tests run a full capture in
/// microseconds.
pub struct CaptureSession<S: DepthVideoSource> {
    source: S,
    request: CaptureRequest,
    title: String,
}

impl<S: DepthVideoSource> CaptureSession<S> {
    pub fn new(source: S, request: CaptureRequest) -> Self {
        Self {
            source,
            request,
            title: "untitled moment".into(),
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Run the whole capture.
    pub fn run(mut self) -> Result<DepthVideo, RuntimeError> {
        let request = self.request;
        self.source.start(request)?;
        let mut frames: Vec<DepthVideoFrame> = Vec::new();
        while let Some(frame) = self.source.poll_frame()? {
            // The source is authoritative about its own dimensions: a
            // device may deliver a different size than requested, and the
            // file must describe what was actually recorded.
            frames.push(DepthVideoFrame {
                timestamp: TimestampNs(frame.timestamp_ns),
                color: frame.color,
                depth: frame.depth_mm,
            });
        }
        self.source.stop()?;
        if frames.is_empty() {
            return Err(RuntimeError::TooShort { got_frames: 0 });
        }
        // What the source says it measured — downgraded to `None` when the
        // frames carry no known depth at all, so a device without a depth
        // pipeline can never mislabel its flat captures.
        let capabilities = self.source.capabilities();
        let kind = if capabilities.depth && frames.iter().any(|f| f.depth.iter().any(|&mm| mm > 0))
        {
            capabilities.depth_kind.unwrap_or(DepthSourceKind::None)
        } else {
            DepthSourceKind::None
        };
        // Intrinsics come from the source when it has a real calibration
        // (LENS_INTRINSIC_CALIBRATION, ARKit) and from the centered ~60°
        // model otherwise — decided by the trait's default, not here.
        let intrinsics = self.source.intrinsics(&request);
        let capture = DepthVideo {
            title: self.title,
            duration_ns: request.duration.as_nanos() as u64,
            fps: request.fps,
            intrinsics,
            source: kind,
            frames,
        };
        capture.validate()?;
        Ok(capture)
    }
}

// ── the deterministic sample ─────────────────────────────────────────────

/// A small deterministic sample capture: what the tests, the CLI's
/// `create-sample` and the feed's development content use.
///
/// It is the synthetic host camera's output — a moving band over a gradient
/// with a breathing depth mound — through the real capture session, so what
/// it exercises is the actual pipeline, not a hand-built struct. Its depth
/// is `Test`-labeled by construction.
pub fn sample_capture() -> DepthVideo {
    sample_with_size(96, 64, 10, Duration::from_secs(1))
}

/// The sample at non-default dimensions, for tests that need small frames.
pub fn sample_with_size(width: u32, height: u32, fps: u32, duration: Duration) -> DepthVideo {
    let request = CaptureRequest {
        duration,
        fps,
        width,
        height,
    };
    CaptureSession::new(three_capture::SyntheticDepthCamera::default(), request)
        .with_title("sample moment")
        .run()
        .expect("the synthetic camera cannot fail")
}

// ── the import pipeline ──────────────────────────────────────────────────

/// Options for [`import_mp4`].
#[derive(Clone, Copy, Debug)]
pub struct ImportOptions {
    /// Frames per second to sample the source at.
    pub fps: u32,
    /// Longest edge of the output frames; the other edge keeps aspect.
    pub max_edge: u32,
    /// Generated-depth "character": 0 = flat far plane, 1 = full
    /// luminance-modulated relief. The depth is `Test`-labeled regardless.
    pub relief: f32,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            fps: 24,
            max_edge: 480,
            relief: 0.7,
        }
    }
}

/// Turn a real video file into a `.3` on a development machine.
///
/// **This is a development tool.** It shells out to `ffmpeg` (which must be
/// on `PATH`) to extract and scale frames, then pairs them with *generated*
/// depth — a soft radial scene with luminance-guided relief — clearly
/// labeled [`DepthSourceKind::Test`]. It exists so the format, the warp
/// renderer and the viewer can be exercised with actual footage before any
/// phone is involved, and it is never a substitute for a device's measured
/// depth.
pub fn import_mp4(input: &Path, options: ImportOptions) -> Result<DepthVideo, RuntimeError> {
    import_mp4_titled(input, "imported moment", options)
}

/// [`import_mp4`] with an explicit title.
pub fn import_mp4_titled(
    input: &Path,
    title: &str,
    options: ImportOptions,
) -> Result<DepthVideo, RuntimeError> {
    let input = input
        .canonicalize()
        .map_err(|e| RuntimeError::Import(format!("cannot read {}: {e}", input.display())))?;
    let frames_dir = std::env::temp_dir().join(format!(
        "three-import-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&frames_dir).map_err(|e| {
        RuntimeError::Import(format!("cannot create {}: {e}", frames_dir.display()))
    })?;

    // One ffmpeg call: probe-scale to max_edge, fps-sample, JPEG out.
    // `-vf` computes the even-dimension scale (JPEG and every downstream
    // consumer wants mod-2 edges); `%05d` names frames in order.
    let pattern = frames_dir.join("frame_%05d.jpg");
    let status = Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(&input)
        .arg("-vf")
        .arg(format!(
            "scale='if(gt(iw,ih),min({max_edge},iw),-2)':'if(gt(iw,ih),-2,min({max_edge},ih))',fps={fps}",
            max_edge = options.max_edge,
            fps = options.fps,
        ))
        .arg("-q:v")
        .arg("3")
        .arg(pattern.to_str().ok_or_else(|| RuntimeError::Import("non-UTF-8 temp path".into()))?)
        .status()
        .map_err(|e| RuntimeError::Import(format!("ffmpeg not found on PATH: {e}")))?;
    if !status.success() {
        return Err(RuntimeError::Import(format!(
            "ffmpeg exited {} — is {} a readable video?",
            status.code().unwrap_or(-1),
            input.display()
        )));
    }

    let mut frames: Vec<DepthVideoFrame> = Vec::new();
    // ffmpeg's %05d starts at 1 — frame_00001.jpg, never frame_00000.jpg —
    // so the read loop starts there too.
    let mut index: u64 = 1;
    let mut frame_ns = 1_000_000_000u64 / options.fps.max(1) as u64;
    let mut dimensions: Option<(u32, u32)> = None;
    loop {
        let path: PathBuf = frames_dir.join(format!("frame_{index:05}.jpg"));
        if !path.exists() {
            break;
        }
        let bytes = std::fs::read(&path)
            .map_err(|e| RuntimeError::Import(format!("reading {}: {e}", path.display())))?;
        let (color, width, height) = decode_jpeg_rgba(&bytes)?;
        if let Some((first_width, first_height)) = dimensions {
            if (width, height) != (first_width, first_height) {
                return Err(RuntimeError::Import(format!(
                    "frame {index} is {width}x{height} but the capture began at \
                     {first_width}x{first_height} — ffmpeg should not have varied mid-stream"
                )));
            }
        } else {
            dimensions = Some((width, height));
            if frame_ns == 0 {
                frame_ns = 1;
            }
        }
        // The file number starts at 1; the timestamp starts at 0.
        let sequence = index - 1;
        let depth = test_depth_map(&color, width, height, options.relief, sequence);
        frames.push(DepthVideoFrame {
            timestamp: TimestampNs(sequence * frame_ns),
            color,
            depth,
        });
        index += 1;
    }
    let _ = std::fs::remove_dir_all(&frames_dir);

    if frames.is_empty() {
        return Err(RuntimeError::Import("ffmpeg produced no frames".into()));
    }
    let (width, height) = dimensions.expect("frames is non-empty, so dimensions were set");
    let duration_ns = frames.len() as u64 * frame_ns;
    let capture = DepthVideo {
        title: title.into(),
        duration_ns,
        fps: options.fps,
        intrinsics: CameraIntrinsics::approximate(width, height),
        source: DepthSourceKind::Test,
        frames,
    };
    capture.validate()?;
    Ok(capture)
}

/// Decode a JPEG to RGBA using the same decoder the format crate uses.
fn decode_jpeg_rgba(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), RuntimeError> {
    let options = zune_jpeg::zune_core::options::DecoderOptions::default()
        .jpeg_set_out_colorspace(zune_jpeg::zune_core::colorspace::ColorSpace::RGBA);
    let mut decoder =
        zune_jpeg::JpegDecoder::new_with_options(std::io::Cursor::new(bytes), options);
    let rgba = decoder
        .decode()
        .map_err(|e| RuntimeError::Import(format!("frame decode: {e}")))?;
    let (width, height) = decoder
        .dimensions()
        .ok_or_else(|| RuntimeError::Import("frame carries no dimensions".into()))?;
    Ok((rgba, width as u32, height as u32))
}

/// The generated depth for imported frames — a smooth dome-plus-relief
/// field in millimeters.
///
/// The shape is deliberate: a center-near dome reads as "subject" to the
/// warp renderer the way a person at arm's length does, and the luminance
/// term gives edges and surfaces some variation to move against. It is
/// fabricated geometry and the capture's `Test` label says so everywhere
/// a viewer can read.
fn test_depth_map(rgba: &[u8], width: u32, height: u32, relief: f32, index: u64) -> Vec<u16> {
    let (w, h) = (width.max(1) as f32, height.max(1) as f32);
    let relief = relief.clamp(0.0, 1.0);
    let mut depth = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let (nx, ny) = (x as f32 / w - 0.5, y as f32 / h - 0.5);
            // Dome: 1 at center, 0 at the frame's edge (approximately).
            let dome = (1.0 - 2.0 * (nx * nx + ny * ny)).clamp(0.0, 1.0);
            // Luminance relief: brighter pixels sit nearer, scaled by the
            // requested amount.
            let pixel = rgba.as_chunks::<4>().0[y as usize * width as usize + x as usize];
            let luminance =
                (0.299 * pixel[0] as f32 + 0.587 * pixel[1] as f32 + 0.114 * pixel[2] as f32)
                    / 255.0;
            let sway = 0.05 * (index as f32 * 0.35).sin();
            let meters = 2.8 - dome * 1.1 - relief * luminance * 0.8 + sway;
            depth.push((meters.clamp(0.3, 8.0) * 1000.0) as u16);
        }
    }
    depth
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_is_a_pure_function_of_accumulated_time() {
        let capture = sample_with_size(32, 24, 10, Duration::from_secs(1));
        let mut player = DepthVideoPlayer::new(&capture);
        assert!(!player.is_playing());
        player.play();
        player.advance(Duration::from_millis(500));
        assert_eq!(player.frame_index(), Some(5));
        assert!((player.progress() - 0.5).abs() < 1e-3);
        // Same ticks, same place.
        let mut other = DepthVideoPlayer::new(&capture);
        other.play();
        for _ in 0..5 {
            other.advance(Duration::from_millis(100));
        }
        assert_eq!(other.frame_index(), Some(5));
    }

    #[test]
    fn player_wraps_when_looping_and_parks_when_not() {
        let capture = sample_with_size(32, 24, 10, Duration::from_secs(1));
        let mut player = DepthVideoPlayer::new(&capture);
        player.play();
        player.advance(Duration::from_secs(3));
        assert!(
            player.is_playing(),
            "looping by default, so 3s through a 1s capture still plays"
        );
        assert_eq!(player.time_ns(), 0, "3s modulo 1s");

        player.set_looping(false);
        player.advance(Duration::from_secs(3));
        assert!(
            !player.is_playing(),
            "a non-looping capture ends, and says so"
        );
        assert_eq!(
            player.frame_index(),
            Some(capture.frames.len() - 1),
            "parks on the last frame"
        );
    }

    #[test]
    fn player_seek_and_rate() {
        let capture = sample_with_size(32, 24, 10, Duration::from_secs(1));
        let mut player = DepthVideoPlayer::new(&capture);
        player.seek(0.75);
        assert_eq!(player.frame_index(), Some(7));
        player.play();
        player.set_rate(2.0);
        player.advance(Duration::from_millis(100));
        assert_eq!(player.time_ns(), 950_000_000, "100ms at 2x from 750ms");
        player.set_rate(-1.0);
        player.advance(Duration::from_millis(150));
        assert_eq!(
            player.time_ns(),
            800_000_000,
            "backward through the same timeline"
        );
    }

    #[test]
    fn a_ui_driven_recording_collects_the_same_capture_as_a_session() {
        // The same request, driven one poll at a time the way a frame loop
        // would, must produce the same capture the synchronous session does.
        use three_capture::SyntheticDepthCamera;
        let request = three_capture::CaptureRequest {
            duration: Duration::from_millis(500),
            fps: 12,
            width: 48,
            height: 32,
        };
        let session_capture = CaptureSession::new(SyntheticDepthCamera::default(), request)
            .with_title("same moment")
            .run()
            .unwrap();

        let mut recording =
            Recording::start(SyntheticDepthCamera::default(), request, "same moment").unwrap();
        // A synchronous source can complete in a single poll — the frame
        // count is the only completion signal, not the source's rhythm.
        assert!(
            !recording.poll(),
            "the frame count is the completion target"
        );
        let (frames, progress) = (recording.frame_count(), recording.progress());
        assert_eq!(frames, 6, "500ms at 12fps");
        assert!(
            (progress - 1.0).abs() < 1e-6,
            "fully collected, not {progress}"
        );
        let polled_capture = recording.finish().unwrap();

        assert_eq!(polled_capture.title, session_capture.title);
        assert_eq!(polled_capture.fps, session_capture.fps);
        assert_eq!(polled_capture.source, session_capture.source);
        assert_eq!(polled_capture.frames.len(), session_capture.frames.len());
        assert_eq!(
            polled_capture.frames[0].timestamp,
            session_capture.frames[0].timestamp
        );
    }

    #[test]
    fn session_produces_a_valid_capture_from_the_synthetic_camera() {
        let capture = sample_with_size(64, 48, 12, Duration::from_secs(1));
        assert_eq!(capture.title, "sample moment");
        assert_eq!(capture.fps, 12);
        assert_eq!(capture.frames.len(), 12);
        assert_eq!(capture.source, DepthSourceKind::Test);
        assert!(capture
            .frames
            .iter()
            .all(|f| f.depth.iter().any(|&mm| mm > 0)));
        capture.validate().unwrap();
        // And it round-trips through the real format.
        let bytes = save(&capture).unwrap();
        let back = open(&bytes).unwrap();
        assert_eq!(back.title, "sample moment");
        assert_eq!(back.frames.len(), 12);
    }

    #[test]
    fn test_depth_relief_is_bounded_and_positive() {
        let rgba: Vec<u8> = vec![255, 0, 0, 255].repeat(16);
        let d1 = test_depth_map(&rgba, 4, 4, 0.0, 0);
        let d2 = test_depth_map(&rgba, 4, 4, 1.0, 0);
        assert!(d1
            .iter()
            .chain(d2.iter())
            .all(|&mm| mm >= 300 && mm <= 8000));
        // Relief pulls the (bright) center nearer than the flat version.
        assert!(d2[1 + 1 * 4] < d1[1 + 1 * 4]);
    }

    #[test]
    fn import_reports_missing_ffmpeg_clearly() {
        // A real file — canonicalize must succeed so the failure names the
        // missing tool, not a missing input.
        let dummy = std::env::temp_dir().join("three-import-missing-ffmpeg.mp4");
        std::fs::write(&dummy, b"not really a video").unwrap();
        // PATH without ffmpeg: the error names the tool, not a panic.
        let saved = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", "/nonexistent");
        let error = import_mp4(&dummy, ImportOptions::default()).unwrap_err();
        std::env::set_var("PATH", saved);
        let _ = std::fs::remove_file(&dummy);
        assert!(error.to_string().contains("ffmpeg"), "error was: {error}");
    }
}
