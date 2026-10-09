//! Windows bitmap — uncompressed 24- and 32-bit (`BI_RGB`, `BI_BITFIELDS`
//! with the standard masks), bottom-up or top-down rows.

use super::{CodecError, Decoded};

fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Decode to RGBA8.
///
/// # Errors
/// On a bad header, an unsupported bit depth or compression, or short data.
pub fn decode(b: &[u8]) -> Result<Decoded, CodecError> {
    if b.len() < 54 || &b[..2] != b"BM" {
        return Err(CodecError::new("bmp: bad header"));
    }
    let off = u32le(b, 10) as usize;
    let w = u32le(b, 18);
    let raw_h = u32le(b, 22).cast_signed();
    let bpp = u16::from_le_bytes([b[28], b[29]]);
    let comp = u32le(b, 30);
    if !matches!(bpp, 24 | 32) || !(comp == 0 || (comp == 3 && bpp == 32)) {
        return Err(CodecError::new(
            "bmp: only uncompressed 24/32-bit is supported",
        ));
    }
    let (h, top_down) = (raw_h.unsigned_abs(), raw_h < 0);
    let bytes_pp = usize::from(bpp / 8);
    let stride = (w as usize * bytes_pp).div_ceil(4) * 4;
    if b.len() < off + stride * h as usize {
        return Err(CodecError::new("bmp: pixel data too short"));
    }
    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h as usize {
        let row = if top_down { y } else { h as usize - 1 - y };
        let r = &b[off + row * stride..];
        for x in 0..w as usize {
            let p = &r[x * bytes_pp..];
            let a = if bpp == 32 && comp == 3 { p[3] } else { 255 };
            rgba.extend([p[2], p[1], p[0], a]);
        }
    }
    Ok(Decoded {
        width: w,
        height: h,
        rgba,
    })
}

/// Encode RGBA8 as a top-down 32-bit BGRA bitmap (`BI_BITFIELDS`).
#[must_use]
pub fn encode(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let data_len = width * height * 4;
    let off = 14 + 56;
    let mut o = b"BM".to_vec();
    o.extend((off + data_len).to_le_bytes());
    o.extend([0; 4]);
    o.extend(off.to_le_bytes());
    o.extend(56u32.to_le_bytes()); // BITMAPV3INFOHEADER
    o.extend(width.to_le_bytes());
    o.extend((-(height.cast_signed())).to_le_bytes());
    o.extend(1u16.to_le_bytes());
    o.extend(32u16.to_le_bytes());
    o.extend(3u32.to_le_bytes());
    o.extend(data_len.to_le_bytes());
    o.extend([0x13, 0x0B, 0, 0, 0x13, 0x0B, 0, 0]); // 72 dpi
    o.extend([0; 8]);
    for m in [0x00FF_0000u32, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000] {
        o.extend(m.to_le_bytes());
    }
    for p in rgba.as_chunks::<4>().0 {
        o.extend([p[2], p[1], p[0], p[3]]);
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let px: Vec<u8> = (0..5 * 3)
            .flat_map(|i: u8| [i, i * 2, 255 - i, 100 + i])
            .collect();
        let d = decode(&encode(5, 3, &px)).unwrap();
        assert_eq!((d.width, d.height), (5, 3));
        assert_eq!(d.rgba, px);
    }

    #[test]
    fn bottom_up_24_bit_with_padding() {
        // 3×2, 24-bit: rows padded to 12 bytes, stored bottom row first.
        let mut b = b"BM".to_vec();
        b.extend((54u32 + 24).to_le_bytes());
        b.extend([0; 4]);
        b.extend(54u32.to_le_bytes());
        b.extend(40u32.to_le_bytes());
        b.extend(3u32.to_le_bytes());
        b.extend(2i32.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(24u16.to_le_bytes());
        b.extend([0; 24]);
        b.extend([1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 0, 0]); // bottom row (y = 1)
        b.extend([10, 11, 12, 13, 14, 15, 16, 17, 18, 0, 0, 0]); // top row (y = 0)
        let d = decode(&b).unwrap();
        assert_eq!(&d.rgba[..4], &[12, 11, 10, 255]);
        assert_eq!(&d.rgba[12..16], &[3, 2, 1, 255]);
    }
}
