//! Video export — the "Video Export / Offline Rendering" row: frames to
//! files, deterministic, with timecode.
//!
//! The document is blunt about it: "You cannot export video from GSAP,
//! Unity (without capture software), or Processing. After Effects,
//! Blender, Manim, Motion Canvas, and Cavalry all render to files — that's
//! their purpose." vieww's films were already rendered this way inside the
//! film lab, through ad-hoc ffmpeg pipes; this module makes it a framework
//! capability any app can use:
//!
//! | sink | what it writes |
//! |---|---|
//! | [`Y4mWriter`] | YUV4MPEG2 (4:2:0, BT.601 full range) — the lossless-ish raw stream every encoder reads (`ffmpeg -i out.y4m out.mp4`), written with no dependency |
//! | [`GifWriter`] | an animated GIF (NeuQuant-quantised, looping) — the social/preview format |
//! | [`PngSequence`] | numbered PNGs — Blender's and Manim's frame-dump output |
//! | [`FfmpegSink`] | H.264 MP4 (optionally muxing a WAV track) through a system `ffmpeg`, refused with a clear error when none is installed |
//!
//! [`read_y4m`] decodes Y4M back into a [`FrameSequence`], which closes the
//! loop for tests — and makes Y4M a real input format for
//! [`VideoPlayer`](crate::VideoPlayer). [`Timecode`] is SMPTE
//! `HH:MM:SS:FF`, including 29.97 drop-frame.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use vieww_foundation::Image;

use crate::{Frame, FrameSequence};

/// Something frames can be pushed into.
pub trait FrameSink {
    /// Append one frame (all frames must share one size).
    ///
    /// # Errors
    ///
    /// I/O failure or a size mismatch.
    fn push(&mut self, frame: &Image) -> io::Result<()>;
    /// Flush and close.
    ///
    /// # Errors
    ///
    /// I/O failure.
    fn finish(&mut self) -> io::Result<()>;
}

fn mismatch(w: u32, h: u32, f: &Image) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("frame is {}×{}, stream is {w}×{h}", f.width(), f.height()),
    )
}

/// Full-range BT.601 RGB → YCbCr.
fn ycbcr(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (f32::from(r), f32::from(g), f32::from(b));
    (
        0.299 * r + 0.587 * g + 0.114 * b,
        128.0 - 0.168_736 * r - 0.331_264 * g + 0.5 * b,
        128.0 + 0.5 * r - 0.418_688 * g - 0.081_312 * b,
    )
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn byte(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// Writes YUV4MPEG2.
#[derive(Debug)]
pub struct Y4mWriter<W: Write> {
    out: W,
    width: u32,
    height: u32,
    fps: (u32, u32),
    started: bool,
    /// Frames written so far.
    pub frames: usize,
}

impl<W: Write> Y4mWriter<W> {
    /// A stream of `width × height` (both even) at `fps` = num/den.
    #[must_use]
    pub const fn new(out: W, width: u32, height: u32, fps: (u32, u32)) -> Self {
        Self { out, width, height, fps, started: false, frames: 0 }
    }

    /// Give back the underlying writer.
    pub fn into_inner(self) -> W {
        self.out
    }
}

impl<W: Write> FrameSink for Y4mWriter<W> {
    fn push(&mut self, f: &Image) -> io::Result<()> {
        if f.width() != self.width || f.height() != self.height {
            return Err(mismatch(self.width, self.height, f));
        }
        if !self.started {
            writeln!(self.out, "YUV4MPEG2 W{} H{} F{}:{} Ip A1:1 C420jpeg", self.width, self.height, self.fps.0, self.fps.1)?;
            self.started = true;
        }
        self.out.write_all(b"FRAME\n")?;
        let (w, h) = (self.width as usize, self.height as usize);
        let px = f.pixels();
        let mut y = Vec::with_capacity(w * h);
        for i in 0..w * h {
            y.push(byte(ycbcr(px[i * 4], px[i * 4 + 1], px[i * 4 + 2]).0));
        }
        let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
        let mut u = Vec::with_capacity(cw * ch);
        let mut v = Vec::with_capacity(cw * ch);
        for cy in 0..ch {
            for cx in 0..cw {
                let (mut su, mut sv, mut n) = (0.0, 0.0, 0.0);
                for dy in 0..2 {
                    for dx in 0..2 {
                        let (x, yy) = (cx * 2 + dx, cy * 2 + dy);
                        if x < w && yy < h {
                            let i = (yy * w + x) * 4;
                            let (_, a, b) = ycbcr(px[i], px[i + 1], px[i + 2]);
                            su += a;
                            sv += b;
                            n += 1.0;
                        }
                    }
                }
                u.push(byte(su / n));
                v.push(byte(sv / n));
            }
        }
        self.out.write_all(&y)?;
        self.out.write_all(&u)?;
        self.out.write_all(&v)?;
        self.frames += 1;
        Ok(())
    }

    fn finish(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

/// Decode a YUV4MPEG2 stream (4:2:0 or 4:4:4) into frames; returns the
/// frames and the frame rate (rounded).
///
/// # Errors
///
/// A malformed header or a truncated frame.
pub fn read_y4m(bytes: &[u8]) -> Result<FrameSequence, String> {
    let nl = bytes.iter().position(|&b| b == b'\n').ok_or("no header")?;
    let header = std::str::from_utf8(&bytes[..nl]).map_err(|_| "header not UTF-8")?;
    let mut parts = header.split_whitespace();
    if parts.next() != Some("YUV4MPEG2") {
        return Err("not a Y4M stream".into());
    }
    let (mut w, mut h, mut fps, mut c444) = (0usize, 0usize, 25u32, false);
    for p in parts {
        match p.as_bytes()[0] {
            b'W' => w = p[1..].parse().map_err(|_| "bad W")?,
            b'H' => h = p[1..].parse().map_err(|_| "bad H")?,
            b'F' => {
                let (a, b) = p[1..].split_once(':').ok_or("bad F")?;
                let (a, b): (f64, f64) = (a.parse().map_err(|_| "bad F")?, b.parse().map_err(|_| "bad F")?);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    fps = (a / b).round().max(1.0) as u32;
                }
            }
            b'C' => c444 = p.starts_with("C444"),
            _ => {}
        }
    }
    if w == 0 || h == 0 {
        return Err("missing size".into());
    }
    let (cw, ch) = if c444 { (w, h) } else { (w.div_ceil(2), h.div_ceil(2)) };
    let frame_len = w * h + 2 * cw * ch;
    let mut at = nl + 1;
    let mut frames: Vec<Frame> = Vec::new();
    while at < bytes.len() {
        let fnl = bytes[at..].iter().position(|&b| b == b'\n').ok_or("truncated frame header")?;
        if !bytes[at..].starts_with(b"FRAME") {
            return Err("expected FRAME".into());
        }
        at += fnl + 1;
        let data = bytes.get(at..at + frame_len).ok_or("truncated frame")?;
        let (yp, rest) = data.split_at(w * h);
        let (up, vp) = rest.split_at(cw * ch);
        let mut rgba = Vec::with_capacity(w * h * 4);
        for yy in 0..h {
            for x in 0..w {
                let ci = if c444 { yy * w + x } else { (yy / 2) * cw + x / 2 };
                let (y, u, v) = (f32::from(yp[yy * w + x]), f32::from(up[ci]) - 128.0, f32::from(vp[ci]) - 128.0);
                rgba.extend_from_slice(&[byte(y + 1.402 * v), byte(y - 0.344_136 * u - 0.714_136 * v), byte(y + 1.772 * u), 255]);
            }
        }
        #[allow(clippy::cast_possible_truncation)]
        frames.push(Image::from_rgba8(rgba, w as u32, h as u32));
        at += frame_len;
    }
    if frames.is_empty() {
        return Err("no frames".into());
    }
    Ok(FrameSequence::new(frames, fps))
}

/// Writes an animated GIF.
pub struct GifWriter<W: Write> {
    encoder: Option<image::codecs::gif::GifEncoder<W>>,
    delay_ms: u32,
    width: u32,
    height: u32,
    pub frames: usize,
}

impl<W: Write> std::fmt::Debug for GifWriter<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GifWriter").field("frames", &self.frames).field("delay_ms", &self.delay_ms).finish_non_exhaustive()
    }
}

impl<W: Write> GifWriter<W> {
    /// A looping GIF at `fps` (GIF delays are in centiseconds, so the rate
    /// is rounded to the nearest representable one).
    ///
    /// # Errors
    ///
    /// The encoder refusing its settings.
    pub fn new(out: W, width: u32, height: u32, fps: u32) -> io::Result<Self> {
        let mut enc = image::codecs::gif::GifEncoder::new_with_speed(out, 10);
        enc.set_repeat(image::codecs::gif::Repeat::Infinite).map_err(io::Error::other)?;
        Ok(Self {
            encoder: Some(enc),
            delay_ms: 1000 / fps.max(1),
            width,
            height,
            frames: 0,
        })
    }
}

impl<W: Write> FrameSink for GifWriter<W> {
    fn push(&mut self, f: &Image) -> io::Result<()> {
        if f.width() != self.width || f.height() != self.height {
            return Err(mismatch(self.width, self.height, f));
        }
        let buf = image::RgbaImage::from_raw(f.width(), f.height(), f.pixels().to_vec())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "pixel buffer size"))?;
        let frame = image::Frame::from_parts(buf, 0, 0, image::Delay::from_numer_denom_ms(self.delay_ms, 1));
        self.encoder
            .as_mut()
            .ok_or_else(|| io::Error::other("finished"))?
            .encode_frame(frame)
            .map_err(io::Error::other)?;
        self.frames += 1;
        Ok(())
    }

    fn finish(&mut self) -> io::Result<()> {
        // Dropping the encoder writes the trailer.
        self.encoder.take();
        Ok(())
    }
}

/// Writes `prefix-00000.png`, `prefix-00001.png`, … into a directory.
#[derive(Debug)]
pub struct PngSequence {
    dir: PathBuf,
    prefix: String,
    pub frames: usize,
}

impl PngSequence {
    /// # Errors
    ///
    /// The directory cannot be created.
    pub fn new(dir: &Path, prefix: &str) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        Ok(Self { dir: dir.to_owned(), prefix: prefix.to_owned(), frames: 0 })
    }

    /// Path of frame `i`.
    #[must_use]
    pub fn path(&self, i: usize) -> PathBuf {
        self.dir.join(format!("{}-{i:05}.png", self.prefix))
    }
}

impl FrameSink for PngSequence {
    fn push(&mut self, f: &Image) -> io::Result<()> {
        let buf = image::RgbaImage::from_raw(f.width(), f.height(), f.pixels().to_vec())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "pixel buffer size"))?;
        buf.save(self.path(self.frames)).map_err(io::Error::other)?;
        self.frames += 1;
        Ok(())
    }

    fn finish(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Pipes raw frames into a system `ffmpeg` for H.264 MP4 output.
#[derive(Debug)]
pub struct FfmpegSink {
    child: Option<std::process::Child>,
    width: u32,
    height: u32,
    pub frames: usize,
}

impl FfmpegSink {
    /// Start `ffmpeg` writing `out` at `fps`, muxing `audio` (a WAV) if
    /// given.
    ///
    /// # Errors
    ///
    /// No `ffmpeg` on the `PATH`, or it failed to start.
    pub fn new(out: &Path, width: u32, height: u32, fps: u32, audio: Option<&Path>) -> io::Result<Self> {
        let mut cmd = std::process::Command::new("ffmpeg");
        cmd.args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s"])
            .arg(format!("{width}x{height}"))
            .args(["-r", &fps.to_string(), "-i", "-"]);
        if let Some(a) = audio {
            cmd.arg("-i").arg(a).args(["-c:a", "aac", "-shortest"]);
        }
        cmd.args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-movflags", "+faststart"])
            .arg(out)
            .stdin(std::process::Stdio::piped());
        let child = cmd.spawn().map_err(|e| io::Error::new(e.kind(), format!("ffmpeg is not available: {e}")))?;
        Ok(Self { child: Some(child), width, height, frames: 0 })
    }
}

impl FrameSink for FfmpegSink {
    fn push(&mut self, f: &Image) -> io::Result<()> {
        if f.width() != self.width || f.height() != self.height {
            return Err(mismatch(self.width, self.height, f));
        }
        let child = self.child.as_mut().ok_or_else(|| io::Error::other("finished"))?;
        child.stdin.as_mut().ok_or_else(|| io::Error::other("no stdin"))?.write_all(f.pixels())?;
        self.frames += 1;
        Ok(())
    }

    fn finish(&mut self) -> io::Result<()> {
        if let Some(mut child) = self.child.take() {
            drop(child.stdin.take());
            let status = child.wait()?;
            if !status.success() {
                return Err(io::Error::other(format!("ffmpeg exited with {status}")));
            }
        }
        Ok(())
    }
}

/// SMPTE timecode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timecode {
    pub hours: u32,
    pub minutes: u32,
    pub seconds: u32,
    pub frames: u32,
    /// 29.97/59.94 drop-frame counting (frames 0 and 1 of each minute
    /// skipped, except every tenth minute).
    pub drop_frame: bool,
}

impl Timecode {
    /// The timecode of frame number `frame` at a nominal `fps` (30 for
    /// 29.97).
    #[must_use]
    pub const fn from_frame(frame: u64, fps: u32, drop_frame: bool) -> Self {
        let mut f = frame;
        if drop_frame {
            let drop = (fps / 15) as u64; // 2 at 30, 4 at 60
            let per_10min = (fps as u64) * 600 - drop * 9;
            let per_min = (fps as u64) * 60 - drop;
            let d = f / per_10min;
            let m = f % per_10min;
            f += drop * 9 * d + if m > drop { drop * ((m - drop) / per_min) } else { 0 };
        }
        let fps64 = fps as u64;
        #[allow(clippy::cast_possible_truncation)]
        Self {
            hours: (f / (fps64 * 3600)) as u32,
            minutes: ((f / (fps64 * 60)) % 60) as u32,
            seconds: ((f / fps64) % 60) as u32,
            frames: (f % fps64) as u32,
            drop_frame,
        }
    }

    /// Back to a frame number.
    #[must_use]
    pub const fn to_frame(&self, fps: u32) -> u64 {
        let fps64 = fps as u64;
        let total_minutes = (self.hours as u64) * 60 + self.minutes as u64;
        let mut f = ((self.hours as u64 * 3600) + (self.minutes as u64 * 60) + self.seconds as u64) * fps64 + self.frames as u64;
        if self.drop_frame {
            let drop = (fps / 15) as u64;
            f -= drop * (total_minutes - total_minutes / 10);
        }
        f
    }
}

impl std::fmt::Display for Timecode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sep = if self.drop_frame { ';' } else { ':' };
        write!(f, "{:02}:{:02}:{:02}{sep}{:02}", self.hours, self.minutes, self.seconds, self.frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(r: u8, g: u8, b: u8) -> Image {
        let mut px = Vec::new();
        for _ in 0..(8 * 6) {
            px.extend_from_slice(&[r, g, b, 255]);
        }
        Image::from_rgba8(px, 8, 6)
    }

    #[test]
    fn y4m_round_trips_colours_closely() {
        let mut w = Y4mWriter::new(Vec::new(), 8, 6, (30, 1));
        for c in [(255, 0, 0), (0, 200, 40), (20, 30, 240), (128, 128, 128)] {
            w.push(&frame(c.0, c.1, c.2)).unwrap();
        }
        w.finish().unwrap();
        assert_eq!(w.frames, 4);
        let bytes = w.into_inner();
        assert!(bytes.starts_with(b"YUV4MPEG2 W8 H6 F30:1"));
        let seq = read_y4m(&bytes).unwrap();
        use crate::VideoSource;
        assert_eq!(seq.frame_count(), 4);
        let f0 = seq.frame_at(0).unwrap();
        let p = &f0.pixels()[..3];
        assert!(p[0] > 245 && p[1] < 10 && p[2] < 10, "{p:?}");
        let f3 = seq.frame_at(3).unwrap();
        assert!(f3.pixels()[..3].iter().all(|&c| (i32::from(c) - 128).abs() <= 1));
    }

    #[test]
    fn size_mismatch_is_an_error() {
        let mut w = Y4mWriter::new(Vec::new(), 4, 4, (30, 1));
        assert!(w.push(&frame(0, 0, 0)).is_err());
    }

    #[test]
    fn gif_frames_decode_back() {
        let mut out = Vec::new();
        {
            let mut g = GifWriter::new(&mut out, 8, 6, 10).unwrap();
            g.push(&frame(255, 0, 0)).unwrap();
            g.push(&frame(0, 0, 255)).unwrap();
            g.finish().unwrap();
        }
        assert!(out.starts_with(b"GIF89a"));
        use image::AnimationDecoder;
        let dec = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(out)).unwrap();
        let frames: Vec<_> = dec.into_frames().collect::<Result<_, _>>().unwrap();
        assert_eq!(frames.len(), 2);
        let px = frames[1].buffer().get_pixel(2, 2);
        assert!(px[2] > 240 && px[0] < 15);
        let (n, d) = frames[0].delay().numer_denom_ms();
        assert_eq!(n / d.max(1), 100);
    }

    #[test]
    fn png_sequences_number_their_files() {
        let dir = std::env::temp_dir().join(format!("vieww-pngseq-{}", std::process::id()));
        let mut s = PngSequence::new(&dir, "shot").unwrap();
        s.push(&frame(1, 2, 3)).unwrap();
        s.push(&frame(4, 5, 6)).unwrap();
        assert!(s.path(1).exists());
        assert!(s.path(1).to_string_lossy().ends_with("shot-00001.png"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn timecode_counts_and_drops_frames() {
        assert_eq!(Timecode::from_frame(0, 24, false).to_string(), "00:00:00:00");
        assert_eq!(Timecode::from_frame(24 * 3661 + 5, 24, false).to_string(), "01:01:01:05");
        // 29.97 DF: frame 1800 is 00:01:00;02 (frames ;00 and ;01 skipped).
        let tc = Timecode::from_frame(1800, 30, true);
        assert_eq!(tc.to_string(), "00:01:00;02");
        assert_eq!(tc.to_frame(30), 1800);
        // Every tenth minute keeps its frames.
        assert_eq!(Timecode::from_frame(17982, 30, true).to_string(), "00:10:00;00");
        for f in [0u64, 1799, 1800, 17981, 17982, 100_000] {
            assert_eq!(Timecode::from_frame(f, 30, true).to_frame(30), f, "frame {f}");
        }
    }
}
