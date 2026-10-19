//! vieww's own image codecs — written from the specifications, no external
//! library underneath.
//!
//! | format | decode | encode | module |
//! |---|---|---|---|
//! | DEFLATE / zlib | stored, fixed, dynamic | LZ77 + dynamic Huffman | [`deflate`] |
//! | PNG | all colour types, 1–16 bit, Adam7, `tRNS` | RGBA8 / RGB8, adaptive filters | [`png`] |
//! | GIF | animated, disposal, interlace, transparency | animated, median-cut palettes | [`gif`] |
//! | JPEG | baseline + extended sequential, any sampling, restarts | baseline 4:2:0 | [`jpeg`] |
//! | BMP | 24/32-bit uncompressed, both row orders | 32-bit BGRA | [`bmp`] |
//!
//! [`decode`] sniffs the format from the leading bytes and dispatches.

pub mod bmp;
pub mod deflate;
pub mod gif;
pub mod jpeg;
pub mod png;

use std::fmt;

use vieww_foundation::Image;

/// Why a file did not decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodecError(pub String);

impl CodecError {
    #[must_use]
    pub fn new(msg: &str) -> Self {
        Self(msg.to_owned())
    }
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CodecError {}

/// Decoded straight-alpha RGBA8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Decoded {
    /// As a foundation image.
    #[must_use]
    pub fn into_image(self) -> Image {
        Image::from_rgba8(self.rgba, self.width, self.height)
    }
}

/// A recognised container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Gif,
    Jpeg,
    Bmp,
}

/// Identify a format by its signature.
#[must_use]
pub fn sniff(bytes: &[u8]) -> Option<Format> {
    if bytes.starts_with(&[137, 80, 78, 71]) {
        Some(Format::Png)
    } else if bytes.starts_with(b"GIF8") {
        Some(Format::Gif)
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        Some(Format::Jpeg)
    } else if bytes.starts_with(b"BM") {
        Some(Format::Bmp)
    } else {
        None
    }
}

/// Decode any supported still image (a GIF's first frame).
///
/// # Errors
/// An unknown signature or a decoder error.
pub fn decode(bytes: &[u8]) -> Result<Decoded, CodecError> {
    match sniff(bytes) {
        Some(Format::Png) => png::decode(bytes),
        Some(Format::Jpeg) => jpeg::decode(bytes),
        Some(Format::Bmp) => bmp::decode(bytes),
        Some(Format::Gif) => {
            let g = gif::decode(bytes)?;
            let f = g
                .frames
                .into_iter()
                .next()
                .ok_or_else(|| CodecError::new("gif: no frames"))?;
            Ok(Decoded {
                width: g.width,
                height: g.height,
                rgba: f.rgba,
            })
        }
        None => Err(CodecError::new("unrecognised image signature")),
    }
}

/// [`decode`], on a worker: the UI thread gets a [`Task`](vieww_foundation::task::Task)
/// it polls each frame and never waits on a decode.
///
/// # Why this exists (advisory Fix 13, 2026-10-19)
///
/// The framework itself never decodes inside a frame: images reach the
/// renderer as decoded [`Image`]s, and the one in-tree decoder call
/// (`vieww-lottie`'s embedded assets) runs at parse time. The stall risk is
/// in *applications*: the obvious way to show a PNG is to call [`decode`] in
/// the code that builds the screen, and a multi-megabyte photo decoded there
/// costs a frame its whole budget on a mid-range phone. This is the one-line
/// alternative, in the shape `AsyncBuilder` already consumes:
///
/// ```
/// use std::sync::Arc;
/// use vieww_foundation::task::{AsyncValue, NoWaker, Pool};
///
/// let png = vieww_image::codec::png::encode(1, 1, &[255, 0, 0, 255]);
/// let pool = Pool::with_threads(1);
/// let mut task = vieww_image::codec::decode_async(&pool, Arc::new(NoWaker), png);
/// while !task.poll() {
///     std::thread::yield_now();
/// }
/// assert!(matches!(task.value(), AsyncValue::Ready(image) if image.width() == 1));
/// ```
///
/// **Same bytes:** the worker runs [`decode`] itself on the bytes it was
/// given, not a second decoder, so the image a frame eventually draws is
/// bit-for-bit the one a synchronous call would have produced. Only *when*
/// it appears moves; what the application shows until then is its own
/// `AsyncValue::Pending` arm, an explicit choice rather than a placeholder
/// the framework invents.
pub fn decode_async(
    spawner: &dyn vieww_foundation::task::Spawn,
    waker: std::sync::Arc<dyn vieww_foundation::task::FrameWaker>,
    bytes: Vec<u8>,
) -> vieww_foundation::task::Task<Image, CodecError> {
    vieww_foundation::task::Task::spawn(spawner, waker, move || {
        decode(&bytes).map(Decoded::into_image)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffing_dispatches_every_encoder_output() {
        let px: Vec<u8> = (0..16 * 16)
            .flat_map(|i: u32| [(i % 16 * 16) as u8, 90, 200, 255])
            .collect();
        for (bytes, f) in [
            (png::encode(16, 16, &px), Format::Png),
            (gif::encode(16, 16, &[(px.clone(), 1)], 0), Format::Gif),
            (jpeg::encode(16, 16, &px, 90), Format::Jpeg),
            (bmp::encode(16, 16, &px), Format::Bmp),
        ] {
            assert_eq!(sniff(&bytes), Some(f));
            let d = decode(&bytes).unwrap();
            assert_eq!((d.width, d.height), (16, 16));
        }
        assert!(decode(b"RIFF....").is_err());
    }

    #[test]
    fn the_async_decode_is_the_sync_decode_bit_for_bit() {
        use std::sync::Arc;
        use vieww_foundation::task::{AsyncValue, NoWaker, Pool};

        let px: Vec<u8> = (0..32 * 24)
            .flat_map(|i: u32| {
                [
                    (i * 7 % 256) as u8,
                    (i % 256) as u8,
                    200,
                    (i * 3 % 256) as u8,
                ]
            })
            .collect();
        let pool = Pool::with_threads(2);
        for bytes in [
            png::encode(32, 24, &px),
            jpeg::encode(32, 24, &px, 85),
            bmp::encode(32, 24, &px),
        ] {
            let sync = decode(&bytes).unwrap().into_image();
            let mut task = decode_async(&pool, Arc::new(NoWaker), bytes);
            while !task.poll() {
                std::thread::yield_now();
            }
            match task.value() {
                // By content: `Image`'s own `==` is allocation identity
                // (damage tracking's choice), and two decodes never share one.
                AsyncValue::Ready(image) => {
                    assert_eq!(
                        (image.width(), image.height()),
                        (sync.width(), sync.height())
                    );
                    assert!(
                        image.pixels() == sync.pixels(),
                        "async decode differs from sync"
                    );
                }
                other => panic!(
                    "expected Ready, got {}",
                    match other {
                        AsyncValue::Pending => "Pending",
                        AsyncValue::Failed(_) => "Failed",
                        AsyncValue::Lost => "Lost",
                        AsyncValue::Ready(_) => unreachable!(),
                    }
                ),
            }
        }
    }

    #[test]
    fn an_undecodable_file_fails_the_task_rather_than_losing_it() {
        use std::sync::Arc;
        use vieww_foundation::task::{AsyncValue, Inline, NoWaker};

        let mut task = decode_async(&Inline, Arc::new(NoWaker), b"RIFF....".to_vec());
        assert!(task.poll());
        assert!(matches!(task.value(), AsyncValue::Failed(_)));
    }
}
