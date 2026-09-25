//! Runtime orchestration for capture, reconstruction and `.3` playback.

use std::fmt;
use three_capture::{CameraSource, CaptureConfig};
use three_core::{
    CameraSample, Capture3D, CaptureSource, MeshFrame, PointFrame, Quaternion, TimestampNs,
    Transform, Vec3,
};
use three_format::{decode, encode, FormatError};
use three_reconstruction::{Observation, Reconstructor, ReconstructionError};

/// Runtime-level error.
#[derive(Debug)]
pub enum RuntimeError { Capture(three_capture::CaptureError), Reconstruction(ReconstructionError), Format(FormatError) }
impl fmt::Display for RuntimeError { fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{match self{Self::Capture(e)=>write!(f,"capture: {e}"),Self::Reconstruction(e)=>write!(f,"reconstruction: {e}"),Self::Format(e)=>write!(f,"format: {e}")}} }
impl std::error::Error for RuntimeError {}
impl From<three_capture::CaptureError> for RuntimeError{fn from(e:three_capture::CaptureError)->Self{Self::Capture(e)}}
impl From<ReconstructionError> for RuntimeError{fn from(e:ReconstructionError)->Self{Self::Reconstruction(e)}}
impl From<FormatError> for RuntimeError{fn from(e:FormatError)->Self{Self::Format(e)}}

/// Coordinates a short capture without owning any UI or platform code.
pub struct CaptureSession<C, R> { camera: C, reconstructor: R, config: CaptureConfig }
impl<C:CameraSource,R:Reconstructor> CaptureSession<C,R> {
    pub fn new(camera:C,reconstructor:R,config:CaptureConfig)->Self{Self{camera,reconstructor,config}}

    /// Capture until the configured duration is reached, then reconstruct.
    ///
    /// The production implementation should use an async/native frame queue;
    /// this synchronous version is deliberately easy to exercise in tests.
    pub fn run(&mut self)->Result<Capture3D,RuntimeError>{
        self.camera.start(self.config.target_fps)?;
        self.reconstructor.begin()?;
        let end_ns=self.config.duration.as_nanos() as u64;
        while let Some(frame)=self.camera.poll_frame()? {
            let done=frame.timestamp_ns>=end_ns;
            self.reconstructor.push(Observation{frame,tracking:None})?;
            if done{break;}
        }
        self.camera.stop()?;
        Ok(self.reconstructor.finish()?.capture)
    }
}

/// Serialize a capture to `.3` bytes.
pub fn save(capture:&Capture3D)->Result<Vec<u8>,RuntimeError>{Ok(encode(capture)?)}

/// Open `.3` bytes for playback.
pub fn open(bytes:&[u8])->Result<Capture3D,RuntimeError>{Ok(decode(bytes)?)}

/// Return the nearest frame at or after a requested timestamp.
pub fn seek_mesh(capture:&Capture3D,timestamp_ns:u64)->Option<&three_core::MeshFrame>{capture.meshes.iter().find(|m|m.timestamp.0>=timestamp_ns).or_else(||capture.meshes.last())}

/// A deterministic sample capture: a rippling surface with a point cloud
/// drifting above it, plus the synthetic camera track that "filmed" it.
///
/// This is the content the CLI's `create-demo` writes and the reference Vieww
/// viewer falls back to, so both demonstrate a *temporal* 3D capture —
/// something that visibly changes as you play it — rather than one static
/// triangle. It is a pure function of nothing at all: no RNG state, no
/// clock, so two calls produce byte-identical captures and a test can assert
/// on the exact geometry.
///
/// Shape of the demo:
///
/// * 30 mesh frames at 10 fps over 3 seconds — a 9x9 vertex grid whose height
///   is `sin` of position and phase, wound counter-clockwise viewed from
///   above (the front-face convention of `three-vieww`'s culling).
/// * One point frame per mesh frame: 32 deterministic points floating above
///   the surface, bobbing with the same phase.
/// * One camera sample per mesh frame, orbiting the subject on +Y —
///   reconstructing what a phone circling a table would have reported.
pub fn demo_capture() -> Capture3D {
    const GRID: usize = 9;       // vertices per side
    const SPAN: f32 = 2.4;       // meters, edge to edge
    const FRAMES: usize = 30;    // over the whole duration
    const DURATION_NS: u64 = 3_000_000_000;
    const POINTS: usize = 32;

    let cell = SPAN / (GRID - 1) as f32;
    let frame_period_ns = DURATION_NS / FRAMES.max(1) as u64;

    let mut meshes = Vec::with_capacity(FRAMES);
    let mut points = Vec::with_capacity(FRAMES);
    let mut cameras = Vec::with_capacity(FRAMES);

    for frame in 0..FRAMES {
        let phase = std::f32::consts::TAU * frame as f32 / FRAMES as f32;
        let t_ns = frame as u64 * frame_period_ns;

        // -- the surface --------------------------------------------------
        let height = |x: f32, z: f32| {
            0.22 * (std::f32::consts::TAU * (x + z) / SPAN + phase).sin()
        };
        let mut vertices = Vec::with_capacity(GRID * GRID);
        for row in 0..GRID {
            for column in 0..GRID {
                let x = -SPAN / 2.0 + column as f32 * cell;
                let z = -SPAN / 2.0 + row as f32 * cell;
                vertices.push(Vec3::new(x, height(x, z), z));
            }
        }
        // Two triangles per cell, wound CCW seen from +Y (above): row-major
        // indexing means (row, col) -> row * GRID + col.
        let mut indices = Vec::with_capacity((GRID - 1) * (GRID - 1) * 6);
        for row in 0..GRID - 1 {
            for column in 0..GRID - 1 {
                let tl = (row * GRID + column) as u32;
                let tr = tl + 1;
                let bl = tl + GRID as u32;
                let br = bl + 1;
                // CCW from above: tl -> br -> tr, then tl -> bl -> br.
                indices.extend_from_slice(&[tl, br, tr, tl, bl, br]);
            }
        }
        meshes.push(MeshFrame { timestamp: TimestampNs(t_ns), vertices, indices });

        // -- the drifting motes --------------------------------------------
        // A tiny LCG with a fixed seed: deterministic, dependency-free, and
        // good enough for decorative points that must not look like a grid.
        let mut seed: u32 = 0x3D5C_1A7F;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            seed
        };
        let mut motes = Vec::with_capacity(POINTS);
        for i in 0..POINTS {
            let x = -SPAN / 2.0 + SPAN * (next() as f32 / u32::MAX as f32);
            let z = -SPAN / 2.0 + SPAN * (next() as f32 / u32::MAX as f32);
            let bob = 0.15 * (phase + i as f32 * 0.7).sin();
            let y = height(x, z) + 0.35 + 0.25 * (next() as f32 / u32::MAX as f32) + bob;
            motes.push(Vec3::new(x, y, z));
        }
        points.push(PointFrame { timestamp: TimestampNs(t_ns), points: motes });

        // -- the camera track ----------------------------------------------
        let yaw = std::f32::consts::TAU * frame as f32 / FRAMES as f32;
        let radius = 2.6;
        let (sin, cos) = yaw.sin_cos();
        cameras.push(CameraSample {
            timestamp: TimestampNs(t_ns),
            pose: Transform {
                translation: Vec3::new(radius * cos, 1.4, radius * sin),
                rotation: Quaternion { x: 0.0, y: sin * 0.5, z: 0.0, w: cos * 0.5 },
            },
            focal_length_px: (600.0, 600.0),
            principal_point_px: (320.0, 240.0),
            image_size_px: (640, 480),
        });
    }

    Capture3D {
        duration_ns: DURATION_NS,
        source: CaptureSource::Synthetic,
        cameras,
        depth: vec![],
        meshes,
        points,
        title: "3 demo capture".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_demo_is_a_valid_capture() {
        demo_capture().validate().expect("the demo must satisfy the format invariants");
    }

    #[test]
    fn the_demo_is_deterministic() {
        assert_eq!(demo_capture(), demo_capture());
    }

    #[test]
    fn the_demo_actually_moves() {
        let capture = demo_capture();
        assert!(capture.meshes.len() > 1, "a temporal demo needs many frames");
        let first = &capture.meshes[0].vertices;
        let middle = &capture.meshes[capture.meshes.len() / 2].vertices;
        assert_ne!(first, middle, "half a duration in, the surface has rippled");
    }

    #[test]
    fn timestamps_are_increasing_and_inside_the_duration() {
        let capture = demo_capture();
        for window in capture.meshes.windows(2) {
            assert!(window[0].timestamp.0 < window[1].timestamp.0, "mesh frames are ordered");
        }
        for mesh in &capture.meshes {
            assert!(mesh.timestamp.0 < capture.duration_ns, "frame inside the timeline");
        }
        assert_eq!(capture.points.len(), capture.meshes.len(), "motes every frame");
        assert_eq!(capture.cameras.len(), capture.meshes.len(), "camera track every frame");
    }

    #[test]
    fn the_demo_round_trips_through_the_format() {
        let capture = demo_capture();
        let bytes = save(&capture).expect("encode");
        let decoded = open(&bytes).expect("decode");
        assert_eq!(decoded, capture);
    }
}
