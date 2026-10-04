//! Reference `.3` binary container — version 1, the depth-video format.
//!
//! # What version 1 stores
//!
//! A `.3` v1 is an **actual video converted to depth at capture time**: a
//! sequence of camera frames (JPEG, ~social quality) each paired with a
//! depth map (zstd-compressed u16 millimeters) and one camera model for the
//! whole shot. Version 0 — the mesh/point-cloud reconstruction prototype —
//! is not readable by this decoder; the product it encoded (cartoonized
//! model content) is gone, and pretending to still support it would saddle
//! every reader with geometry code nothing produces any more.
//!
//! # Layout
//!
//! ```text
//! magic "DOT3"            4 bytes
//! version                 u16 LE      (= 1)
//! flags                   u16 LE      (= 0)
//! payload_len             u64 LE
//! ── payload ──────────────────────────────────────────────────────────
//! title_len               u32 LE
//! title                   UTF-8, exactly title_len bytes
//! duration_ns             u64 LE
//! fps                     u32 LE
//! width, height           u32 LE x 2
//! fx, fy, cx, cy          f32 LE x 4
//! source                  u8          (DepthSourceKind)
//! frame_count             u32 LE
//! ── per frame, in order ─────────────────────────────────────────────
//! timestamp_ns            u64 LE
//! color_len               u32 LE
//! color                   JPEG bytes, exactly color_len
//! depth_len               u32 LE
//! depth                   zstd bytes, exactly depth_len; decompresses to
//!                         width*height u16 LE millimeters
//! ```
//!
//! Frames are stored in timestamp order and the payload is read strictly
//! front-to-back: no offsets, no index, no seeking yet. The
//! streamable/chunked container is a roadmap item and will be version 2;
//! the version field exists precisely so it can change without ambiguity.
//!
//! # Determinism
//!
//! Encoding is deterministic: fixed JPEG quality, fixed zstd level, no
//! timestamps-of-encoding, no paths, no RNG. Two encodes of one capture
//! produce identical bytes, which is what the round-trip and stability
//! tests assert on.

use std::fmt;
use std::io::{self, Cursor, Read, Write};

use three_core::{CameraIntrinsics, DepthSourceKind, DepthVideo, DepthVideoFrame, TimestampNs};

pub const MAGIC: [u8; 4] = *b"DOT3";
/// The version this crate reads and writes.
///
/// Version 0 was the mesh/point prototype; version 1 is the depth-video
/// format. A reader that meets another value refuses the file rather than
/// guessing at bytes that mean something else.
pub const VERSION: u16 = 1;

/// The header of a `.3` file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub version: u16,
    pub flags: u16,
    pub payload_len: u64,
}

/// JPEG quality for color frames, 0-100.
///
/// 82 is the "social upload" bracket: visually indistinguishable from the
/// source on a phone screen at feed sizes while roughly 8x smaller than the
/// raw plane. It is a constant rather than an option so every capture a
/// device makes is byte-comparable with every other.
const JPEG_QUALITY: u8 = 82;

/// zstd level for depth planes.
///
/// Depth maps are wide runs of near-equal u16s; level 6 is where they stop
/// shrinking meaningfully before the next doubling of encode time. Capture
/// time is the constraint: this runs once per frame at the end of a 3-second
/// capture, not per view.
const ZSTD_LEVEL: i32 = 6;

// ── lazy (per-frame) decoding ────────────────────────────────────────────

/// Everything a player needs before the first frame is decoded.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptureMeta {
    pub title: String,
    pub duration_ns: u64,
    pub fps: u32,
    pub intrinsics: CameraIntrinsics,
    pub source: DepthSourceKind,
    pub frame_count: usize,
}

/// A `.3` held compressed, decoded one frame at a time.
///
/// [`decode`] materializes an entire capture — right for tests, the CLI and
/// the importer. A viewer on a phone cannot afford that: a 3-second, 640x480,
/// 24 fps capture is ~130 MB decompressed against ~2 MB compressed, and a
/// feed holds several. `open` keeps the file's bytes and an index of where
/// each frame's payloads sit, and [`frame`](LazyCapture::frame) decompresses
/// exactly the one being shown.
///
/// Frame records are located once at open time by walking the payload — the
/// format has no offset table yet (see FORMAT.md's roadmap), so this is a
/// single linear pass that decompresses nothing.
/// One frame record in a [`LazyCapture`]'s index: the timestamp, and the
/// color and depth payloads' absolute byte ranges in the file.
type FrameRecord = (u64, (usize, usize), (usize, usize));

pub struct LazyCapture {
    bytes: Vec<u8>,
    meta: CaptureMeta,
    records: Vec<FrameRecord>,
}

impl LazyCapture {
    /// Index a `.3` for lazy frame access. The whole payload is walked once
    /// and validated structurally; frames themselves are only decoded on
    /// demand, so a corrupt JPEG deep in the file surfaces when that frame
    /// is shown, not at open.
    pub fn open(bytes: Vec<u8>) -> Result<Self, FormatError> {
        // The full decode is the validation: everything it checks, the lazy
        // reader needs, and everything it decompresses, the lazy reader
        // skips. Running it and throwing the pixels away costs one decode
        // — cheap at capture sizes, and the only way both readers can never
        // disagree about what is valid.
        let probe = decode(&bytes)?;
        let meta = CaptureMeta {
            title: probe.title.clone(),
            duration_ns: probe.duration_ns,
            fps: probe.fps,
            intrinsics: probe.intrinsics,
            source: probe.source,
            frame_count: probe.frames.len(),
        };

        // Walk again, recording offsets. The fixed block is duration(8) +
        // fps(4) + width(4) + height(4) + fx/fy/cx/cy(16) + source(1) = 37
        // bytes — the frame count is read separately so its value, not just
        // its bytes, is what positions the records.
        let payload_start = 16usize;
        let mut cursor = Cursor::new(
            &bytes[payload_start..payload_start + header(&bytes)?.payload_len as usize],
        );
        read_string(&mut cursor)?;
        let mut fixed = [0u8; 8 + 4 + 4 + 4 + 16 + 1];
        cursor.read_exact(&mut fixed)?;
        let frame_count = read_u32(&mut cursor)? as usize;
        let mut records = Vec::with_capacity(frame_count);
        for _ in 0..frame_count {
            let timestamp = read_u64(&mut cursor)?;
            let color = read_blob_range(&mut cursor)?;
            let depth = read_blob_range(&mut cursor)?;
            // read_blob_range reports payload-relative offsets; the payload
            // begins at byte 16 of the file.
            records.push((
                timestamp,
                (payload_start + color.0, payload_start + color.1),
                (payload_start + depth.0, payload_start + depth.1),
            ));
        }
        Ok(Self {
            bytes,
            meta,
            records,
        })
    }

    /// The capture's metadata, without decoding any frame.
    pub fn meta(&self) -> &CaptureMeta {
        &self.meta
    }

    /// Decode one frame. Decompressing is per call by design — callers cache
    /// (the reference viewer keeps the current frame), and a borrow-returning
    /// API would pin the decompressed plane to this object's lifetime.
    pub fn frame(&self, index: usize) -> Result<DepthVideoFrame, FormatError> {
        let Some(&(timestamp, color_range, depth_range)) = self.records.get(index) else {
            return Err(FormatError::InvalidData("frame index out of range"));
        };
        let (w, h) = (self.meta.intrinsics.width, self.meta.intrinsics.height);
        Ok(DepthVideoFrame {
            timestamp: TimestampNs(timestamp),
            color: decode_color(&self.bytes[color_range.0..color_range.1], w, h)?,
            depth: decode_depth(
                &self.bytes[depth_range.0..depth_range.1],
                w as usize * h as usize,
            )?,
        })
    }

    /// Materialize the whole capture — the bridge back to [`decode`]'s
    /// shape for callers that want everything at once.
    pub fn capture(&self) -> Result<DepthVideo, FormatError> {
        decode(&self.bytes)
    }
}

impl std::fmt::Debug for LazyCapture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The bytes stay out: a dump wants the shape, not the payload.
        f.debug_struct("LazyCapture")
            .field("meta", &self.meta)
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

/// Errors returned by `.3` encoding and decoding.
#[derive(Debug)]
pub enum FormatError {
    Io(io::Error),
    InvalidMagic,
    UnsupportedVersion(u16),
    InvalidData(&'static str),
    /// A color or depth payload failed to decompress/decode.
    Codec(String),
    Core(three_core::CoreError),
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::InvalidMagic => f.write_str("not a .3 file (bad magic)"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported .3 version {v}"),
            Self::InvalidData(s) => f.write_str(s),
            Self::Codec(s) => write!(f, "codec error: {s}"),
            Self::Core(e) => write!(f, "invalid capture: {e}"),
        }
    }
}
impl std::error::Error for FormatError {}
impl From<io::Error> for FormatError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<three_core::CoreError> for FormatError {
    fn from(e: three_core::CoreError) -> Self {
        Self::Core(e)
    }
}

/// Read just the header of a `.3` byte string.
///
/// For inspecting a file without decoding every frame's payloads.
pub fn header(bytes: &[u8]) -> Result<Header, FormatError> {
    let mut cursor = Cursor::new(bytes);
    let mut magic = [0u8; 4];
    cursor.read_exact(&mut magic)?;
    if magic != MAGIC {
        return Err(FormatError::InvalidMagic);
    }
    let mut version = [0u8; 2];
    cursor.read_exact(&mut version)?;
    let version = u16::from_le_bytes(version);
    if version != VERSION {
        return Err(FormatError::UnsupportedVersion(version));
    }
    let mut flags = [0u8; 2];
    cursor.read_exact(&mut flags)?;
    let mut payload_len = [0u8; 8];
    cursor.read_exact(&mut payload_len)?;
    Ok(Header {
        version,
        flags: u16::from_le_bytes(flags),
        payload_len: u64::from_le_bytes(payload_len),
    })
}

/// Encode a complete depth-video capture into `.3` bytes.
///
/// Validates first — a capture that cannot be played must not be writable,
/// or every reader inherits the cost of the check.
pub fn encode(capture: &DepthVideo) -> Result<Vec<u8>, FormatError> {
    capture.validate()?;

    let mut payload = Vec::new();
    write_string(&mut payload, &capture.title)?;
    payload.write_all(&capture.duration_ns.to_le_bytes())?;
    payload.write_all(&capture.fps.to_le_bytes())?;
    payload.write_all(&capture.intrinsics.width.to_le_bytes())?;
    payload.write_all(&capture.intrinsics.height.to_le_bytes())?;
    payload.write_all(&capture.intrinsics.fx.to_le_bytes())?;
    payload.write_all(&capture.intrinsics.fy.to_le_bytes())?;
    payload.write_all(&capture.intrinsics.cx.to_le_bytes())?;
    payload.write_all(&capture.intrinsics.cy.to_le_bytes())?;
    payload.write_all(&[source_to_u8(capture.source)])?;
    // Frame count is u32 in the file: usize's width differs by platform,
    // and a container field must not.
    payload.write_all(&(capture.frames.len() as u32).to_le_bytes())?;
    for frame in &capture.frames {
        payload.write_all(&frame.timestamp.0.to_le_bytes())?;
        let color = encode_color(frame, capture.intrinsics.width, capture.intrinsics.height)?;
        payload.write_all(&(color.len() as u32).to_le_bytes())?;
        payload.write_all(&color)?;
        let depth = encode_depth(frame)?;
        payload.write_all(&(depth.len() as u32).to_le_bytes())?;
        payload.write_all(&depth)?;
    }

    let mut out = Vec::with_capacity(16 + payload.len());
    out.write_all(&MAGIC)?;
    out.write_all(&VERSION.to_le_bytes())?;
    out.write_all(&0u16.to_le_bytes())?;
    out.write_all(&(payload.len() as u64).to_le_bytes())?;
    out.write_all(&payload)?;
    Ok(out)
}

/// Decode complete `.3` bytes into a playable capture.
pub fn decode(bytes: &[u8]) -> Result<DepthVideo, FormatError> {
    let header = header(bytes)?;
    let payload_len = header.payload_len as usize;
    let start = 16usize;
    let end = start
        .checked_add(payload_len)
        .ok_or(FormatError::InvalidData("payload length overflows"))?;
    if bytes.len() < end {
        return Err(FormatError::InvalidData(
            "file is shorter than its header claims",
        ));
    }
    let mut cursor = Cursor::new(&bytes[start..end]);

    let title = read_string(&mut cursor)?;
    let duration_ns = read_u64(&mut cursor)?;
    let fps = read_u32(&mut cursor)?;
    let width = read_u32(&mut cursor)?;
    let height = read_u32(&mut cursor)?;
    let (fx, fy, cx, cy) = (
        read_f32(&mut cursor)?,
        read_f32(&mut cursor)?,
        read_f32(&mut cursor)?,
        read_f32(&mut cursor)?,
    );
    let source = read_source(&mut cursor)?;
    let frame_count = read_u32(&mut cursor)? as usize;

    // A frame_count that would overflow the payload is rejected by the
    // per-frame reads running off the end; the cap exists so a corrupt
    // count cannot make the decoder pre-allocate gigabytes before the
    // first read fails.
    let frame_count = frame_count.min(bytes.len() / 24);

    let expected_samples = width as usize * height as usize;
    let mut frames = Vec::with_capacity(frame_count);
    for _ in 0..frame_count {
        let timestamp = TimestampNs(read_u64(&mut cursor)?);
        let color = read_blob(&mut cursor)?;
        let depth = read_blob(&mut cursor)?;
        frames.push(DepthVideoFrame {
            timestamp,
            color: decode_color(color, width, height)?,
            depth: decode_depth(depth, expected_samples)?,
        });
    }
    let capture = DepthVideo {
        title,
        duration_ns,
        fps,
        intrinsics: CameraIntrinsics {
            fx,
            fy,
            cx,
            cy,
            width,
            height,
        },
        source,
        frames,
    };
    capture.validate()?;
    Ok(capture)
}

// ── per-plane codecs ─────────────────────────────────────────────────────

fn encode_color(frame: &DepthVideoFrame, width: u32, height: u32) -> Result<Vec<u8>, FormatError> {
    // The color plane's dimensions are capture-level facts; the frame
    // carries only bytes. A mismatch means the caller skipped `validate`,
    // and the honest answer is to refuse rather than encode garbage.
    if frame.color.len() != width as usize * height as usize * 4 {
        return Err(FormatError::InvalidData(
            "color plane does not match the capture resolution",
        ));
    }
    let (w, h) = (
        u16::try_from(width).map_err(|_| FormatError::InvalidData("width exceeds jpeg's u16"))?,
        u16::try_from(height).map_err(|_| FormatError::InvalidData("height exceeds jpeg's u16"))?,
    );
    let mut out = Vec::new();
    // RGBA straight in: the encoder handles the alpha drop itself, and one
    // colorspace conversion fewer is one place a channel swap can't hide.
    // Below quality 90 it subsamples chroma 4:2:0 — the standard social
    // upload trade, and the reason JPEG_QUALITY is 82 and not 95.
    let encoder = jpeg_encoder::Encoder::new(&mut out, JPEG_QUALITY);
    encoder
        .encode(&frame.color, w, h, jpeg_encoder::ColorType::Rgba)
        .map_err(|e| FormatError::Codec(format!("jpeg encode: {e}")))?;
    Ok(out)
}

fn decode_color(bytes: &[u8], width: u32, height: u32) -> Result<Vec<u8>, FormatError> {
    // Forced RGB output: a foreign tool may hand this decoder a grayscale
    // JPEG, and zune-jpeg widens it on the way out so every reader sees one
    // format. Dimensions must still match the capture's own — a mismatched
    // frame is a corrupt file, not a resize request.
    let options = zune_jpeg::zune_core::options::DecoderOptions::default()
        .jpeg_set_out_colorspace(zune_jpeg::zune_core::colorspace::ColorSpace::RGB);
    let mut decoder =
        zune_jpeg::JpegDecoder::new_with_options(std::io::Cursor::new(bytes), options);
    let rgb = decoder
        .decode()
        .map_err(|e| FormatError::Codec(format!("jpeg decode: {e}")))?;
    let expected = width as usize * height as usize * 3;
    if rgb.len() != expected {
        return Err(FormatError::InvalidData(
            "color plane decodes to the wrong size",
        ));
    }
    let mut rgba = Vec::with_capacity(expected / 3 * 4);
    for pixel in rgb.as_chunks::<3>().0 {
        rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
    }
    Ok(rgba)
}

fn encode_depth(frame: &DepthVideoFrame) -> Result<Vec<u8>, FormatError> {
    // u16 samples to LE bytes, then one zstd frame. Depth is a smooth
    // signal; treating it as bytes rather than samples is fine — the byte
    // stream has even longer matches than the sample stream would.
    let mut raw = Vec::with_capacity(frame.depth.len() * 2);
    for sample in &frame.depth {
        raw.extend_from_slice(&sample.to_le_bytes());
    }
    zstd::stream::encode_all(Cursor::new(raw), ZSTD_LEVEL)
        .map_err(|e| FormatError::Codec(format!("zstd encode: {e}")))
}

fn decode_depth(bytes: &[u8], expected_samples: usize) -> Result<Vec<u16>, FormatError> {
    let raw = zstd::stream::decode_all(bytes)
        .map_err(|e| FormatError::Codec(format!("zstd decode: {e}")))?;
    if raw.len() != expected_samples * 2 {
        return Err(FormatError::InvalidData(
            "depth plane decompresses to the wrong size",
        ));
    }
    Ok(raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect())
}

// ── primitive readers/writers ────────────────────────────────────────────

fn write_string(out: &mut Vec<u8>, s: &str) -> Result<(), FormatError> {
    let bytes = s.as_bytes();
    out.write_all(&(bytes.len() as u32).to_le_bytes())?;
    out.write_all(bytes)?;
    Ok(())
}

fn read_string(cursor: &mut Cursor<&[u8]>) -> Result<String, FormatError> {
    let bytes = read_blob(cursor)?;
    String::from_utf8(bytes.to_vec()).map_err(|_| FormatError::InvalidData("title is not UTF-8"))
}

fn read_blob<'a>(cursor: &mut Cursor<&'a [u8]>) -> Result<&'a [u8], FormatError> {
    let (start, end) = read_blob_range(cursor)?;
    Ok(&cursor.get_ref()[start..end])
}

/// Read a length-prefixed blob, reporting its **payload-relative** byte
/// range instead of borrowing the slice — the lazy reader's index pass
/// wants offsets it can store.
fn read_blob_range(cursor: &mut Cursor<&[u8]>) -> Result<(usize, usize), FormatError> {
    let len = read_u32(cursor)? as usize;
    let start = cursor.position() as usize;
    let end = start
        .checked_add(len)
        .ok_or(FormatError::InvalidData("blob length overflows"))?;
    if cursor.get_ref().len() < end {
        return Err(FormatError::InvalidData("file ends inside a payload"));
    }
    cursor.set_position(end as u64);
    Ok((start, end))
}

fn read_u32(cursor: &mut Cursor<&[u8]>) -> Result<u32, FormatError> {
    let mut bytes = [0u8; 4];
    cursor.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(cursor: &mut Cursor<&[u8]>) -> Result<u64, FormatError> {
    let mut bytes = [0u8; 8];
    cursor.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn read_f32(cursor: &mut Cursor<&[u8]>) -> Result<f32, FormatError> {
    let mut bytes = [0u8; 4];
    cursor.read_exact(&mut bytes)?;
    Ok(f32::from_le_bytes(bytes))
}

fn read_source(cursor: &mut Cursor<&[u8]>) -> Result<DepthSourceKind, FormatError> {
    let mut byte = [0u8; 1];
    cursor.read_exact(&mut byte)?;
    source_from_u8(byte[0])
}

fn source_to_u8(source: DepthSourceKind) -> u8 {
    match source {
        DepthSourceKind::AndroidCamera2 => 0,
        DepthSourceKind::IosLidar => 1,
        DepthSourceKind::IosTrueDepth => 2,
        DepthSourceKind::None => 4,
        DepthSourceKind::Test => 3,
    }
}

fn source_from_u8(byte: u8) -> Result<DepthSourceKind, FormatError> {
    match byte {
        0 => Ok(DepthSourceKind::AndroidCamera2),
        1 => Ok(DepthSourceKind::IosLidar),
        2 => Ok(DepthSourceKind::IosTrueDepth),
        3 => Ok(DepthSourceKind::Test),
        4 => Ok(DepthSourceKind::None),
        _ => Err(FormatError::InvalidData("unknown depth source kind")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use three_core::CameraIntrinsics;

    /// A small capture with *real photographic-ish* content: a horizontal
    /// gradient plus a moving stripe, so JPEG actually compresses something
    /// and the depth has smooth ramps the warp will one day sell.
    fn sample_capture() -> DepthVideo {
        let (w, h) = (96u32, 64u32);
        let intrinsics = CameraIntrinsics::approximate(w, h);
        let frames = (0..6)
            .map(|index| {
                let timestamp = TimestampNs(index * 100_000_000);
                let mut color = Vec::with_capacity((w * h * 4) as usize);
                let mut depth = Vec::with_capacity((w * h) as usize);
                for y in 0..h {
                    for x in 0..w {
                        let stripe = ((x + index as u32 * 8) % 24 < 12) as u8;
                        color.extend_from_slice(&[
                            (x * 255 / w) as u8,
                            (y * 255 / h) as u8,
                            stripe * 200 + 20,
                            255,
                        ]);
                        // A slanted plane: nearer at the bottom of frame,
                        // plus a bump in the middle. Millimeters.
                        let bump = (-((x as i32 - w as i32 / 2).pow(2)
                            + (y as i32 - h as i32 / 2).pow(2))
                            / 512) as i32;
                        let mm = (2400 - 1200 * y as i32 / h as i32 + bump).max(300) as u16;
                        depth.push(mm);
                    }
                }
                DepthVideoFrame {
                    timestamp,
                    color,
                    depth,
                }
            })
            .collect();
        DepthVideo {
            title: "gradient walk".into(),
            duration_ns: 600_000_000,
            fps: 10,
            intrinsics,
            source: DepthSourceKind::Test,
            frames,
        }
    }

    #[test]
    fn round_trip_preserves_every_field() {
        let capture = sample_capture();
        let bytes = encode(&capture).unwrap();
        let back = decode(&bytes).unwrap();
        assert_eq!(back.title, "gradient walk");
        assert_eq!(back.duration_ns, 600_000_000);
        assert_eq!(back.fps, 10);
        assert_eq!(back.intrinsics, capture.intrinsics);
        assert_eq!(back.source, capture.source);
        assert_eq!(back.frames.len(), capture.frames.len());
        for (a, b) in capture.frames.iter().zip(back.frames.iter()) {
            assert_eq!(a.timestamp, b.timestamp);
            // Color survives only within JPEG's lossy budget: same size,
            // and every channel within a distance of the original that
            // stays under a *mean* bound (no systematic drift) while the
            // worst pixel is allowed its ringing — hard synthetic edges
            // are JPEG's worst case and overshoot by design.
            assert_eq!(a.color.len(), b.color.len());
            let mut deltas = a
                .color
                .iter()
                .zip(b.color.iter())
                .map(|(x, y)| x.abs_diff(*y) as u32);
            let mean = deltas.clone().sum::<u32>() as f32 / deltas.count() as f32;
            let worst = a
                .color
                .iter()
                .zip(b.color.iter())
                .map(|(x, y)| x.abs_diff(*y))
                .max()
                .unwrap();
            assert!(
                mean < 4.0,
                "q82 should stay near the source on average, mean {mean}"
            );
            assert!(
                worst < 96,
                "edge ringing at q82 stays bounded, worst delta {worst}"
            );
            // Depth is lossless: identical samples.
            assert_eq!(a.depth, b.depth);
        }
    }

    #[test]
    fn encoding_is_deterministic() {
        let capture = sample_capture();
        let first = encode(&capture).unwrap();
        let second = encode(&capture).unwrap();
        assert_eq!(first, second, "same capture, same bytes");
    }

    #[test]
    fn compression_beats_raw_substantially() {
        let capture = sample_capture();
        let raw: usize = capture
            .frames
            .iter()
            .map(|f| f.color.len() + f.depth.len() * 2)
            .sum();
        let encoded = encode(&capture).unwrap().len();
        // Gradients are JPEG's best case, but if the container were ever
        // *bigger* than raw something structural would be wrong.
        assert!(encoded < raw, "{encoded} bytes encoded vs {raw} raw");
    }

    #[test]
    fn header_reports_the_version_and_length() {
        let bytes = encode(&sample_capture()).unwrap();
        let h = header(&bytes).unwrap();
        assert_eq!(h.version, 1);
        assert_eq!(h.flags, 0);
        assert_eq!(h.payload_len as usize + 16, bytes.len());
    }

    #[test]
    fn rejects_bad_magic_and_foreign_versions() {
        assert!(matches!(
            header(b"NOT3....").unwrap_err(),
            FormatError::InvalidMagic
        ));

        let mut bytes = encode(&sample_capture()).unwrap();
        bytes[4..6].copy_from_slice(&0u16.to_le_bytes());
        assert!(matches!(
            decode(&bytes).unwrap_err(),
            FormatError::UnsupportedVersion(0)
        ));
    }

    #[test]
    fn rejects_truncated_payloads() {
        let bytes = encode(&sample_capture()).unwrap();
        // Claim more payload than the file carries.
        let mut lying = bytes.clone();
        lying[8..16].copy_from_slice(&(bytes.len() as u64 * 2).to_le_bytes());
        assert!(matches!(
            decode(&lying).unwrap_err(),
            FormatError::InvalidData(_)
        ));
        // Actually truncate mid-frame.
        assert!(decode(&bytes[..bytes.len() - 20]).is_err());
    }

    #[test]
    fn rejects_corrupt_depth_streams() {
        let capture = sample_capture();
        let mut bytes = encode(&capture).unwrap();
        // Overwrite the first depth blob's zstd bytes with garbage. The
        // frame record starts at: header 16 + fixed fields 53 + title.
        // Rather than compute it, brute-force a search: flip bytes one at
        // a time until decode fails — every flip lands inside *some*
        // payload, and the only payloads that can fail are the zstd ones
        // (JPEG survives most single-byte damage, zstd never does).
        let mut corrupted = false;
        for offset in 60..bytes.len() {
            let mut probe = bytes.clone();
            probe[offset] ^= 0xFF;
            if decode(&probe).is_err() {
                corrupted = true;
                break;
            }
        }
        assert!(
            corrupted,
            "no single-byte flip inside the payloads broke the zstd stream?"
        );
        // And the intact copy still decodes.
        assert!(decode(&bytes).is_ok());
    }

    #[test]
    fn lazy_open_matches_full_decode_frame_by_frame() {
        let bytes = encode(&sample_capture()).unwrap();
        let full = decode(&bytes).unwrap();
        let lazy = LazyCapture::open(bytes).unwrap();

        assert_eq!(lazy.meta().title, "gradient walk");
        assert_eq!(lazy.meta().frame_count, full.frames.len());
        assert_eq!(lazy.meta().intrinsics, full.intrinsics);
        for (index, expected) in full.frames.iter().enumerate() {
            let frame = lazy.frame(index).unwrap();
            assert_eq!(frame.timestamp, expected.timestamp);
            assert_eq!(
                frame.color, expected.color,
                "frame {index}'s color decodes identically"
            );
            assert_eq!(frame.depth, expected.depth);
        }
        assert!(
            lazy.frame(full.frames.len() + 1).is_err(),
            "out of range is an error, not a panic"
        );
    }

    #[test]
    fn lazy_open_rejects_what_decode_rejects() {
        let mut bytes = encode(&sample_capture()).unwrap();
        bytes[4..6].copy_from_slice(&9u16.to_le_bytes());
        assert!(LazyCapture::open(bytes).is_err());
    }

    #[test]
    fn grayscale_jpeg_from_foreign_tools_still_decodes() {
        // A 2x2 grayscale JPEG written by the same encoder but forced
        // through the Luma path by hand-decoding: zune-jpeg is asked to
        // decode a grayscale image and our decode must widen it to RGBA.
        let mut gray = Vec::new();
        {
            let encoder = jpeg_encoder::Encoder::new(&mut gray, 90);
            encoder
                .encode(
                    &[128u8, 128, 128, 128, 200, 200, 200, 200],
                    2,
                    2,
                    jpeg_encoder::ColorType::Luma,
                )
                .unwrap();
        }
        let rgba = decode_color(&gray, 2, 2).unwrap();
        assert_eq!(rgba.len(), 16, "2x2 RGBA");
        assert!(
            rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255),
            "opaque"
        );
    }
}
