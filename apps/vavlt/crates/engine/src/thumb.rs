//! Decode a picture down to a tile.
//!
//! Both hosts need the same thing — RGBA8 at a known edge, small enough that a
//! grid of them is not a hundred megabytes of decoded camera JPEG — so it is
//! written once here rather than twice in the hosts.
//!
//! The result is deliberately *bigger* than the tile it renders at. A 96pt tile
//! on a 3x phone panel is 288 device pixels, and a 96px thumbnail upscaled into
//! it is visibly soft; the edge below is the compromise that keeps a grid sharp
//! without decoding a full-resolution image per tile.

use anyhow::{Context, Result};

/// Decoded edge, in pixels. Large enough for a phone tile while avoiding the
/// 33% per-axis / 56% total pixel cost of 288px thumbnails on large selections.
pub const THUMB_EDGE: u32 = 224;

/// A square RGBA8 thumbnail, centre-cropped, ready for `vieww`'s `Image`.
///
/// Cropped to a square rather than letterboxed because the grid's tiles are
/// square: a fitted image leaves two grey bars per tile, and a grid of those
/// reads as a broken layout rather than as photographs.
///
/// Returns `(pixels, edge)` — the edge comes back because a very small source
/// image is not upscaled, and the caller needs the real dimensions.
pub fn thumbnail(bytes: &[u8], edge: u32) -> Result<(Vec<u8>, u32)> {
    let decoded = image::load_from_memory(bytes).context("decoding for a thumbnail")?;

    // Square-crop about the centre, then scale once. Cropping first means the
    // scale never has to touch pixels that are about to be thrown away.
    let (w, h) = (decoded.width(), decoded.height());
    let side = w.min(h);
    let cropped = image::imageops::crop_imm(&decoded, (w - side) / 2, (h - side) / 2, side, side)
        .to_image();

    let edge = edge.min(side).max(1);
    // Triangle rather than Lanczos: at these sizes the difference is invisible
    // and Lanczos is several times the cost, which shows up as a stalled grid
    // when eighty tiles arrive at once.
    let scaled = image::imageops::resize(&cropped, edge, edge, image::imageops::FilterType::Triangle);

    Ok((scaled.into_raw(), edge))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wide source must come back square, at the requested edge — the
    /// property the grid depends on.
    #[test]
    fn a_wide_photo_comes_back_square() {
        let mut png = Vec::new();
        let source = image::RgbaImage::from_pixel(200, 100, image::Rgba([9, 9, 9, 255]));
        image::DynamicImage::ImageRgba8(source)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("encode");

        let (pixels, edge) = thumbnail(&png, 64).expect("thumbnail");
        assert_eq!(edge, 64);
        assert_eq!(pixels.len(), 64 * 64 * 4, "RGBA8 at the requested edge");
    }

    /// Bytes that are not an image are an error, not a panic: they arrive from
    /// a picker grant, and a mislabelled file must cost one tile.
    #[test]
    fn rubbish_bytes_are_an_error() {
        assert!(thumbnail(b"not a picture at all", 64).is_err());
    }
}
