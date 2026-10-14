//! AVI (RIFF) with Motion-JPEG — the container and codec webcams, capture
//! cards and dash cams emit, decoded and encoded by vieww itself.
//!
//! * [`MjpegAvi::parse`] walks the RIFF tree: `hdrl` → `avih` (frame period)
//!   → `strl`/`strh`+`strf` (the video stream's `MJPG` handler and size),
//!   then the `movi` list's `00dc`/`00db` chunks, recording each frame's
//!   byte range (with `LIST rec ` groups flattened). Frames are decoded
//!   lazily through [`vieww_image::codec::jpeg`], so the source implements
//!   [`VideoSource`] without holding every decoded frame.
//! * [`MjpegWriter`] is a [`FrameSink`] that writes a standards-shaped AVI
//!   1.0 file with an `idx1` index — playable by every media player.
//!
//! Inter-frame codecs (H.264, VP9, AV1) are out of scope by name: each is a
//! book of its own. MJPEG is intra-only, which is what makes a from-spec
//! implementation honest at this size.

use std::io::{self, Write};

use vieww_foundation::Image;
use vieww_image::codec::jpeg;

use crate::export::FrameSink;
use crate::{Frame, VideoSource};

fn u32le(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// A parsed MJPEG AVI: frame byte ranges plus the stream header.
#[derive(Debug, Clone)]
pub struct MjpegAvi {
    data: Vec<u8>,
    frames: Vec<(usize, usize)>,
    width: u32,
    height: u32,
    fps: u32,
}

impl MjpegAvi {
    /// Parse the container.
    ///
    /// # Errors
    /// Not RIFF/AVI, a non-MJPEG video stream, or no frames.
    pub fn parse(data: Vec<u8>) -> Result<Self, String> {
        if data.len() < 12 || &data[..4] != b"RIFF" || &data[8..12] != b"AVI " {
            return Err("not an AVI file".into());
        }
        let mut s = Self {
            data,
            frames: Vec::new(),
            width: 0,
            height: 0,
            fps: 0,
        };
        let (mut handler_ok, mut period_us) = (false, 0u32);
        // Iterative walk of nested LISTs.
        let mut stack = vec![(12usize, s.data.len())];
        while let Some((mut at, end)) = stack.pop() {
            while at + 8 <= end {
                let id = &s.data[at..at + 4];
                let size = u32le(&s.data, at + 4).ok_or("truncated chunk")? as usize;
                let body = at + 8;
                let body_end = (body + size).min(end);
                match id {
                    b"LIST" => {
                        if body + 4 <= body_end {
                            stack.push((body + 4, body_end));
                        }
                    }
                    b"avih" => period_us = u32le(&s.data, body).unwrap_or(0),
                    b"strh" => {
                        let kind = s.data.get(body..body + 4).unwrap_or(&[]);
                        let handler = s.data.get(body + 4..body + 8).unwrap_or(&[]);
                        if kind == b"vids" {
                            handler_ok = handler.eq_ignore_ascii_case(b"MJPG")
                                || handler.eq_ignore_ascii_case(b"mjpg")
                                || handler == b"\0\0\0\0";
                            if let (Some(scale), Some(rate)) =
                                (u32le(&s.data, body + 20), u32le(&s.data, body + 24))
                            {
                                if scale > 0 && rate > 0 {
                                    s.fps = (rate + scale / 2) / scale;
                                }
                            }
                        }
                    }
                    b"strf" if s.width == 0 => {
                        s.width = u32le(&s.data, body + 4).unwrap_or(0);
                        let h = u32le(&s.data, body + 8).unwrap_or(0).cast_signed();
                        s.height = h.unsigned_abs();
                        let comp = s.data.get(body + 16..body + 20).unwrap_or(&[]);
                        if !(comp.eq_ignore_ascii_case(b"MJPG") || comp == b"\0\0\0\0") {
                            handler_ok = false;
                        }
                    }
                    [b'0', b'0', b'd', b'c' | b'b'] if size > 0 => {
                        s.frames.push((body, body_end));
                    }
                    _ => {}
                }
                at = body + size + (size & 1);
            }
        }
        if !handler_ok {
            return Err("the video stream is not Motion-JPEG".into());
        }
        // The walk visits LISTs last-in-first-out; restore file order.
        s.frames.sort_unstable();
        if s.fps == 0 && period_us > 0 {
            s.fps = (1_000_000 + period_us / 2) / period_us;
        }
        if s.fps == 0 {
            s.fps = 30;
        }
        if s.frames.is_empty() {
            return Err("no frames".into());
        }
        Ok(s)
    }

    /// The compressed bytes of frame `i`.
    #[must_use]
    pub fn jpeg_bytes(&self, i: usize) -> Option<&[u8]> {
        self.frames.get(i).map(|&(a, b)| &self.data[a..b])
    }
}

impl VideoSource for MjpegAvi {
    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    fn frame_rate(&self) -> u32 {
        self.fps
    }
    fn frame_count(&self) -> usize {
        self.frames.len()
    }
    fn frame_at(&self, index: usize) -> Option<Frame> {
        jpeg::decode(self.jpeg_bytes(index)?)
            .ok()
            .map(|d| d.into_image())
    }
}

/// Writes MJPEG AVI 1.0 with an `idx1` index.
#[derive(Debug)]
pub struct MjpegWriter<W: Write> {
    out: Option<W>,
    width: u32,
    height: u32,
    fps: u32,
    quality: u8,
    chunks: Vec<Vec<u8>>,
    pub frames: usize,
}

impl<W: Write> MjpegWriter<W> {
    #[must_use]
    pub fn new(out: W, width: u32, height: u32, fps: u32, quality: u8) -> Self {
        Self {
            out: Some(out),
            width,
            height,
            fps: fps.max(1),
            quality,
            chunks: Vec::new(),
            frames: 0,
        }
    }
}

fn list(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = b"LIST".to_vec();
    #[allow(clippy::cast_possible_truncation)]
    v.extend(((body.len() + 4) as u32).to_le_bytes());
    v.extend(kind);
    v.extend(body);
    v
}

fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = id.to_vec();
    #[allow(clippy::cast_possible_truncation)]
    v.extend((body.len() as u32).to_le_bytes());
    v.extend(body);
    if body.len() % 2 == 1 {
        v.push(0);
    }
    v
}

impl<W: Write> FrameSink for MjpegWriter<W> {
    fn push(&mut self, f: &Image) -> io::Result<()> {
        if f.width() != self.width || f.height() != self.height {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "frame size changed",
            ));
        }
        self.chunks.push(jpeg::encode(
            f.width(),
            f.height(),
            f.pixels(),
            self.quality,
        ));
        self.frames += 1;
        Ok(())
    }

    #[allow(clippy::cast_possible_truncation)]
    fn finish(&mut self) -> io::Result<()> {
        let Some(mut out) = self.out.take() else {
            return Ok(());
        };
        let n = self.chunks.len() as u32;
        let max = self.chunks.iter().map(Vec::len).max().unwrap_or(0) as u32;
        let mut avih = Vec::new();
        for v in [
            1_000_000 / self.fps,
            max * self.fps,
            0,
            0x10, // AVIF_HASINDEX
            n,
            0,
            1,
            max,
            self.width,
            self.height,
            0,
            0,
            0,
            0,
        ] {
            avih.extend(v.to_le_bytes());
        }
        let mut strh = Vec::new();
        strh.extend(b"vidsMJPG");
        for v in [0u32, 0, 0, 1, self.fps, 0, n, max, u32::MAX, 0] {
            strh.extend(v.to_le_bytes());
        }
        strh.extend([0u8; 8]); // rcFrame
        let mut strf = Vec::new();
        for v in [40u32, self.width, self.height] {
            strf.extend(v.to_le_bytes());
        }
        strf.extend(1u16.to_le_bytes());
        strf.extend(24u16.to_le_bytes());
        strf.extend(b"MJPG");
        strf.extend((self.width * self.height * 3).to_le_bytes());
        strf.extend([0u8; 16]);
        let strl = list(
            b"strl",
            &[chunk(b"strh", &strh), chunk(b"strf", &strf)].concat(),
        );
        let hdrl = list(b"hdrl", &[chunk(b"avih", &avih), strl].concat());
        let mut movi_body = Vec::new();
        let mut idx = Vec::new();
        for c in &self.chunks {
            let offset = movi_body.len() as u32 + 4; // relative to 'movi'
            idx.extend(b"00dc");
            idx.extend(0x10u32.to_le_bytes()); // AVIIF_KEYFRAME
            idx.extend(offset.to_le_bytes());
            idx.extend((c.len() as u32).to_le_bytes());
            movi_body.extend(chunk(b"00dc", c));
        }
        let movi = list(b"movi", &movi_body);
        let idx1 = chunk(b"idx1", &idx);
        let body = [b"AVI ".to_vec(), hdrl, movi, idx1].concat();
        out.write_all(b"RIFF")?;
        out.write_all(&(body.len() as u32).to_le_bytes())?;
        out.write_all(&body)?;
        out.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(k: u8) -> Image {
        let px: Vec<u8> = (0..32 * 24u32)
            .flat_map(|i| {
                #[allow(clippy::cast_possible_truncation)]
                [
                    ((i % 32) * 8) as u8,
                    k.wrapping_mul(40),
                    ((i / 32) * 10) as u8,
                    255,
                ]
            })
            .collect();
        Image::from_rgba8(px, 32, 24)
    }

    #[test]
    fn write_then_parse_then_decode() {
        let mut bytes = Vec::new();
        {
            let mut w = MjpegWriter::new(&mut bytes, 32, 24, 25, 90);
            for k in 0..5 {
                w.push(&frame(k)).unwrap();
            }
            w.finish().unwrap();
        }
        assert_eq!(&bytes[..4], b"RIFF");
        let avi = MjpegAvi::parse(bytes).unwrap();
        assert_eq!(avi.dimensions(), (32, 24));
        assert_eq!(avi.frame_rate(), 25);
        assert_eq!(avi.frame_count(), 5);
        let f3 = avi.frame_at(3).unwrap();
        let p = &f3.pixels()[(12 * 32 + 16) * 4..(12 * 32 + 16) * 4 + 3];
        assert!((i32::from(p[1]) - 120).abs() < 12, "{p:?}");
        assert!(avi.frame_at(5).is_none());
    }

    #[test]
    fn non_avi_and_non_mjpeg_are_refused() {
        assert!(MjpegAvi::parse(b"RIFF\0\0\0\0WAVE".to_vec()).is_err());
        let mut bytes = Vec::new();
        {
            let mut w = MjpegWriter::new(&mut bytes, 32, 24, 25, 90);
            w.push(&frame(0)).unwrap();
            w.finish().unwrap();
        }
        let i = bytes.windows(8).position(|w| w == b"vidsMJPG").unwrap();
        bytes[i + 4..i + 8].copy_from_slice(b"H264");
        let j = bytes.windows(4).rposition(|w| w == b"MJPG").unwrap();
        bytes[j..j + 4].copy_from_slice(b"H264");
        assert!(MjpegAvi::parse(bytes).unwrap_err().contains("Motion-JPEG"));
    }
}
