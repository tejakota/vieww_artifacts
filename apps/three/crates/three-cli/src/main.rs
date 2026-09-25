//! Small developer CLI for inspecting and creating `.3` files.
//!
//! Usage:
//!
//! ```text
//! three create-demo <file.3>   write a deterministic sample capture
//! three inspect <file.3>       print a summary of a .3 file
//! ```

use std::{env, fs};
use three_format::{decode, encode, header};
use three_runtime::demo_capture;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("inspect") => {
            let path = args.next().ok_or("missing file path")?;
            let bytes = fs::read(&path)?;
            let h = header(&bytes)?;
            let c = decode(&bytes)?;
            let bounds = c
                .bounds()
                .map(|b| {
                    let (min, max) = (b.min, b.max);
                    format!(
                        "[{:.2} {:.2} {:.2}] .. [{:.2} {:.2} {:.2}]",
                        min.x, min.y, min.z, max.x, max.y, max.z
                    )
                })
                .unwrap_or_else(|| "no spatial data".into());
            println!(".3 version: {}", h.version);
            println!("payload: {} bytes", h.payload_len);
            println!("title: {}", c.title);
            println!("duration: {} ms", c.duration_ns / 1_000_000);
            println!("source: {}", c.source);
            println!("camera samples: {}", c.cameras.len());
            println!("mesh frames: {}", c.meshes.len());
            println!("point frames: {}", c.points.len());
            println!("bounds: {bounds}");
            println!(
                "mesh vertices (first frame): {}",
                c.meshes.first().map(|m| m.vertices.len()).unwrap_or(0)
            );
        }
        Some("create-demo") => {
            let path = args.next().ok_or("missing output path")?;
            let capture = demo_capture();
            fs::write(&path, encode(&capture)?)?;
            println!(
                "created {path} ({} mesh frames, {} point frames, {} ms)",
                capture.meshes.len(),
                capture.points.len(),
                capture.duration_ns / 1_000_000
            );
        }
        _ => {
            eprintln!("Usage:\n  three create-demo <file.3>\n  three inspect <file.3>");
            std::process::exit(2);
        }
    }
    Ok(())
}
