//! Renders a `.3` at several camera poses to PNGs — the visual receipt of
//! the whole pipeline: real footage in, warped re-photographs out.
//!
//! ```text
//! cargo run -p three-cli --example render-poses -- ./imported.3 [outdir]
//! ```

use three_format::LazyCapture;
use three_vieww::{warp_frame, ViewCamera};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .expect("usage: render-poses <file.3> [outdir]");
    let out_dir = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "target/poses".into());
    std::fs::create_dir_all(&out_dir)?;

    let lazy = LazyCapture::open(std::fs::read(&path)?)?;
    let meta = lazy.meta();
    println!(
        "capture: {} — {} frames, {}x{}, subject {:.2} m",
        meta.title,
        meta.frame_count,
        meta.intrinsics.width,
        meta.intrinsics.height,
        {
            let frame = lazy.frame(0)?;
            let mut known: Vec<u16> = frame.depth.iter().copied().filter(|&mm| mm > 0).collect();
            known.sort_unstable();
            if known.is_empty() {
                2.0
            } else {
                known[known.len() / 2] as f32 / 1000.0
            }
        }
    );

    let frame = lazy.frame(0)?;
    let subject = {
        let mut known: Vec<u16> = frame.depth.iter().copied().filter(|&mm| mm > 0).collect();
        known.sort_unstable();
        if known.is_empty() {
            2.0
        } else {
            known[known.len() / 2] as f32 / 1000.0
        }
    };

    let poses: [(&str, ViewCamera); 5] = [
        ("1-neutral", ViewCamera::default()),
        ("2-orbit-left", camera(0.35, 0.0, 1.0)),
        ("3-orbit-up", camera(-0.15, 0.28, 1.0)),
        ("4-dolly-in", camera(0.0, 0.0, 0.6)),
        ("5-dolly-out-yaw", camera(-0.3, -0.15, 1.8)),
    ];

    for (name, camera) in poses {
        let out = warp_frame(
            &frame.color,
            &frame.depth,
            &meta.intrinsics,
            &camera,
            subject,
        );
        let path = format!("{out_dir}/{name}.png");
        write_png(&out.rgba, out.width, out.height, &path)?;
        println!("wrote {path}");
    }
    Ok(())
}

fn camera(yaw: f32, pitch: f32, dolly: f32) -> ViewCamera {
    let mut camera = ViewCamera::default();
    camera.yaw = yaw;
    camera.pitch = pitch;
    camera.dolly = dolly;
    camera
}

/// Minimal PNG writer (truecolor, no dependencies): the IHDR/IDAT/IEND
/// handshake with zlib stored blocks — RGBA data is already incompressible
/// enough for a receipt.
fn write_png(rgba: &[u8], width: u32, height: u32, path: &str) -> std::io::Result<()> {
    use std::io::Write;

    let mut raw = Vec::with_capacity(rgba.len() + height as usize * 3);
    for row in 0..height as usize {
        raw.push(0); // filter: none
        let start = row * width as usize * 4;
        raw.extend_from_slice(&rgba[start..start + width as usize * 4]);
    }

    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    out.write_all(b"\x89PNG\r\n\x1a\n")?;
    write_chunk(&mut out, b"IHDR", &{
        let mut b = Vec::with_capacity(13);
        b.extend_from_slice(&width.to_be_bytes());
        b.extend_from_slice(&height.to_be_bytes());
        b.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
        b
    })?;
    write_chunk(&mut out, b"IDAT", &zlib_store(&raw))?;
    write_chunk(&mut out, b"IEND", &[])?;
    Ok(())
}

fn write_chunk(mut out: impl std::io::Write, kind: &[u8; 4], data: &[u8]) -> std::io::Result<()> {
    out.write_all(&(data.len() as u32).to_be_bytes())?;
    out.write_all(kind)?;
    out.write_all(data)?;
    let mut crc = Crc32::new();
    crc.update(kind);
    crc.update(data);
    out.write_all(&crc.finish().to_be_bytes())
}

/// zlib stream with stored (uncompressed) blocks — the smallest honest
/// compressor, fine for receipts.
fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 65535 * 5 + 16);
    out.extend_from_slice(&[0x78, 0x01]); // zlib header, stored
    let mut offset = 0;
    while offset < data.len() {
        let len = (data.len() - offset).min(65535);
        let last = offset + len >= data.len();
        out.push(last as u8);
        out.extend_from_slice(&(len as u16).to_le_bytes());
        out.extend_from_slice(&(!(len as u16)).to_le_bytes());
        out.extend_from_slice(&data[offset..offset + len]);
        offset += len;
    }
    let mut crc = Crc32::new();
    crc.update(data);
    out.extend_from_slice(&crc.finish().to_be_bytes());
    out
}

struct Crc32(u32);
impl Crc32 {
    fn new() -> Self {
        Self(0xFFFF_FFFF)
    }
    fn update(&mut self, data: &[u8]) {
        for &byte in data {
            let mut c = self.0 ^ byte as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            self.0 = c;
        }
    }
    fn finish(&self) -> u32 {
        !self.0
    }
}
