//! End-to-end deterministic demonstration of the planned device pipeline.
//!
//! Replace `SyntheticCamera` with an Android/iOS implementation once the native
//! camera bridge is available. Nothing downstream needs to change.

use three_capture::{CaptureConfig, SyntheticCamera};
use three_reconstruction::DemoReconstructor;
use three_runtime::{save, CaptureSession};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let camera=SyntheticCamera::default();
    let reconstructor=DemoReconstructor::default();
    let mut session=CaptureSession::new(camera,reconstructor,CaptureConfig::default());
    let capture=session.run()?;
    let bytes=save(&capture)?;
    std::fs::write("demo.3",&bytes)?;
    println!("captured {} mesh frames into demo.3 ({} bytes)",capture.meshes.len(),bytes.len());
    Ok(())
}
