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
}
