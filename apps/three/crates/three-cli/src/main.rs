//! Small developer CLI for inspecting, creating and importing `.3` files.
//!
//! Usage:
//!
//! ```text
//! three inspect <file.3>           print a summary of a .3 file
//! three create-sample <file.3>    write the deterministic sample capture
//! three import <in.mp4> <out.3>   turn a real video into a .3 (ffmpeg on PATH)
//! ```

use std::{env, fs, path::Path, process::ExitCode};

use three_format::{decode, header};
use three_runtime::{import_mp4_titled, sample_capture, ImportOptions};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("inspect") => match inspect(args.next().as_deref()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("three: {error}");
                ExitCode::FAILURE
            }
        },
        Some("create-sample") => match create_sample(args.next().as_deref()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("three: {error}");
                ExitCode::FAILURE
            }
        },
        Some("import") => match import(args.next(), args.next(), args.next()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("three: {error}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!(
                "Usage:\n  three inspect <file.3>\n  three create-sample <file.3>\n  three import <in.mp4> <out.3> [title]"
            );
            ExitCode::from(2)
        }
    }
}

fn inspect(path: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.ok_or("missing file path")?;
    let bytes = fs::read(path)?;
    let h = header(&bytes)?;
    let c = decode(&bytes)?;
    let k = c.intrinsics;
    println!(".3 version: {}", h.version);
    println!("payload: {} bytes", h.payload_len);
    println!("title: {}", c.title);
    println!("duration: {} ms", c.duration_ns / 1_000_000);
    println!("frames: {} at {} fps", c.frames.len(), c.fps);
    println!("resolution: {}x{}", k.width, k.height);
    println!(
        "intrinsics: fx {:.1} fy {:.1} cx {:.1} cy {:.1}",
        k.fx, k.fy, k.cx, k.cy
    );
    println!(
        "depth source: {} ({})",
        c.source,
        if c.source.is_measured() {
            "measured"
        } else {
            "not measured"
        }
    );
    let known = c
        .frames
        .iter()
        .map(|f| f.depth.iter().filter(|&&mm| mm > 0).count())
        .sum::<usize>();
    let total: usize = c.frames.iter().map(|f| f.depth.len()).sum();
    if total > 0 {
        println!(
            "depth coverage: {:.0}%",
            100.0 * known as f32 / total as f32
        );
    }
    println!("subject distance: {:.2} m", c.subject_distance());
    Ok(())
}

fn create_sample(path: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.ok_or("missing output path")?;
    let capture = sample_capture();
    fs::write(path, three_runtime::save(&capture)?)?;
    println!(
        "created {path} ({} frames, {} bytes)",
        capture.frames.len(),
        fs::metadata(path)?.len()
    );
    Ok(())
}

fn import(
    input: Option<String>,
    output: Option<String>,
    title: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let input = input.ok_or("missing input video path")?;
    let input = Path::new(&input);
    let output = output.ok_or("missing output .3 path")?;
    let title = title.as_deref().unwrap_or_else(|| {
        // The file stem is the honest default title.
        input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("imported moment")
    });
    let capture = import_mp4_titled(input, title, ImportOptions::default())?;
    let bytes = three_runtime::save(&capture)?;
    fs::write(&output, &bytes)?;
    println!(
        "imported {} -> {} ({} frames at {} fps, {:.1} s, {} bytes; depth is Test-generated)",
        input.display(),
        output,
        capture.frames.len(),
        capture.fps,
        capture.duration_ns as f32 / 1_000_000_000.0,
        bytes.len()
    );
    Ok(())
}
