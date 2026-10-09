//! PNG (ISO/IEC 15948), encoder and decoder, on [`deflate`](super::deflate).
//!
//! Decoding covers the whole core format: colour types 0 (grey), 2 (RGB),
//! 3 (palette, with `tRNS`), 4 (grey + alpha) and 6 (RGBA); bit depths
//! 1/2/4/8/16; all five row filters; Adam7 interlacing; `tRNS` colour keys
//! for grey and RGB. Every chunk's CRC is checked. Output is straight-alpha
//! RGBA8 (16-bit samples keep their high byte).
//!
//! Encoding writes RGBA8 (or RGB8 when every pixel is opaque) with the
//! per-row adaptive filter choice libpng uses — the filter minimising the
//! sum of absolute residuals — then zlib.

use super::deflate::{crc32_update, zlib_compress, zlib_decompress};
use super::{CodecError, Decoded};

const SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    #[allow(clippy::cast_possible_truncation)]
    out.extend((data.len() as u32).to_be_bytes());
    out.extend(kind);
    out.extend(data);
    let c = crc32_update(crc32_update(0xFFFF_FFFF, kind), data) ^ 0xFFFF_FFFF;
    out.extend(c.to_be_bytes());
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (a, b, c) = (i16::from(a), i16::from(b), i16::from(c));
    let p = a + b - c;
    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let r = if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    } as u8;
    r
}

/// Encode straight-alpha RGBA8 pixels.
#[must_use]
pub fn encode(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let opaque = rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255);
    let bpp = if opaque { 3 } else { 4 };
    let (w, h) = (width as usize, height as usize);
    let stride = w * bpp;
    let rows: Vec<Vec<u8>> = (0..h)
        .map(|y| {
            let src = &rgba[y * w * 4..(y + 1) * w * 4];
            if opaque {
                src.as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|p| [p[0], p[1], p[2]])
                    .collect()
            } else {
                src.to_vec()
            }
        })
        .collect();
    let mut raw = Vec::with_capacity((stride + 1) * h);
    let zero = vec![0u8; stride];
    for y in 0..h {
        let cur = &rows[y];
        let up = if y > 0 { &rows[y - 1] } else { &zero };
        let mut best: (u64, u8, Vec<u8>) = (u64::MAX, 0, Vec::new());
        for f in 0..5u8 {
            let mut line = Vec::with_capacity(stride);
            for i in 0..stride {
                let a = if i >= bpp { cur[i - bpp] } else { 0 };
                let c = if i >= bpp { up[i - bpp] } else { 0 };
                let b = up[i];
                let pred = match f {
                    0 => 0,
                    1 => a,
                    2 => b,
                    #[allow(clippy::cast_possible_truncation)]
                    3 => ((u16::from(a) + u16::from(b)) / 2) as u8,
                    _ => paeth(a, b, c),
                };
                line.push(cur[i].wrapping_sub(pred));
            }
            let score: u64 = line
                .iter()
                .map(|&v| u64::from((v as i8).unsigned_abs()))
                .sum();
            if score < best.0 {
                best = (score, f, line);
            }
        }
        raw.push(best.1);
        raw.extend(best.2);
    }
    let mut out = SIG.to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend(width.to_be_bytes());
    ihdr.extend(height.to_be_bytes());
    ihdr.extend([8, if opaque { 2 } else { 6 }, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib_compress(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

struct Header {
    w: usize,
    h: usize,
    depth: u8,
    color: u8,
    interlace: bool,
}

impl Header {
    fn channels(&self) -> usize {
        match self.color {
            0 | 3 => 1,
            2 => 3,
            4 => 2,
            _ => 4,
        }
    }
    /// Bytes per pixel for filtering (at least 1).
    fn bpp(&self) -> usize {
        ((self.channels() * usize::from(self.depth)).div_ceil(8)).max(1)
    }
    fn row_bytes(&self, w: usize) -> usize {
        (w * self.channels() * usize::from(self.depth)).div_ceil(8)
    }
}

fn unfilter(data: &[u8], w: usize, h: usize, hd: &Header) -> Result<Vec<u8>, CodecError> {
    let stride = hd.row_bytes(w);
    let bpp = hd.bpp();
    if data.len() < (stride + 1) * h {
        return Err(CodecError::new("png: image data too short"));
    }
    let mut out = vec![0u8; stride * h];
    for y in 0..h {
        let f = data[y * (stride + 1)];
        let src = &data[y * (stride + 1) + 1..(y + 1) * (stride + 1)];
        for i in 0..stride {
            let a = if i >= bpp {
                out[y * stride + i - bpp]
            } else {
                0
            };
            let b = if y > 0 { out[(y - 1) * stride + i] } else { 0 };
            let c = if y > 0 && i >= bpp {
                out[(y - 1) * stride + i - bpp]
            } else {
                0
            };
            let pred = match f {
                0 => 0,
                1 => a,
                2 => b,
                #[allow(clippy::cast_possible_truncation)]
                3 => ((u16::from(a) + u16::from(b)) / 2) as u8,
                4 => paeth(a, b, c),
                _ => return Err(CodecError::new("png: bad filter type")),
            };
            out[y * stride + i] = src[i].wrapping_add(pred);
        }
    }
    Ok(out)
}

/// Expand unfiltered rows to RGBA8.
fn expand(
    rows: &[u8],
    w: usize,
    h: usize,
    hd: &Header,
    palette: &[[u8; 3]],
    trns: &[u8],
) -> Result<Vec<u8>, CodecError> {
    let stride = hd.row_bytes(w);
    let d = usize::from(hd.depth);
    let mut out = Vec::with_capacity(w * h * 4);
    let sample = |row: &[u8], idx: usize| -> u16 {
        match d {
            16 => u16::from_be_bytes([row[idx * 2], row[idx * 2 + 1]]),
            8 => u16::from(row[idx]),
            _ => {
                let bit = idx * d;
                let byte = row[bit / 8];
                let shift = 8 - d - (bit % 8);
                u16::from((byte >> shift) & ((1u8 << d) - 1))
            }
        }
    };
    let to8 = |v: u16| -> u8 {
        #[allow(clippy::cast_possible_truncation)]
        match d {
            16 => (v >> 8) as u8,
            8 => v as u8,
            _ => (u32::from(v) * 255 / ((1u32 << d) - 1)) as u8,
        }
    };
    let key16 = |i: usize| -> Option<u16> {
        trns.get(i * 2..i * 2 + 2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
    };
    for y in 0..h {
        let row = &rows[y * stride..(y + 1) * stride];
        for x in 0..w {
            match hd.color {
                0 => {
                    let v = sample(row, x);
                    let a = if key16(0) == Some(v) { 0 } else { 255 };
                    let g = to8(v);
                    out.extend([g, g, g, a]);
                }
                2 => {
                    let (r, g, b) = (
                        sample(row, 3 * x),
                        sample(row, 3 * x + 1),
                        sample(row, 3 * x + 2),
                    );
                    let a = if key16(0) == Some(r) && key16(1) == Some(g) && key16(2) == Some(b) {
                        0
                    } else {
                        255
                    };
                    out.extend([to8(r), to8(g), to8(b), a]);
                }
                3 => {
                    let i = usize::from(sample(row, x));
                    let p = palette
                        .get(i)
                        .ok_or_else(|| CodecError::new("png: palette index out of range"))?;
                    out.extend([p[0], p[1], p[2], trns.get(i).copied().unwrap_or(255)]);
                }
                4 => {
                    let (g, a) = (to8(sample(row, 2 * x)), to8(sample(row, 2 * x + 1)));
                    out.extend([g, g, g, a]);
                }
                _ => {
                    out.extend([
                        to8(sample(row, 4 * x)),
                        to8(sample(row, 4 * x + 1)),
                        to8(sample(row, 4 * x + 2)),
                        to8(sample(row, 4 * x + 3)),
                    ]);
                }
            }
        }
    }
    Ok(out)
}

/// Decode a PNG to RGBA8.
///
/// # Errors
/// On a bad signature, a CRC mismatch, an unsupported or inconsistent header,
/// or malformed image data.
pub fn decode(bytes: &[u8]) -> Result<Decoded, CodecError> {
    if bytes.len() < 8 || bytes[..8] != SIG {
        return Err(CodecError::new("png: bad signature"));
    }
    let mut at = 8;
    let mut hd: Option<Header> = None;
    let mut idat = Vec::new();
    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut trns: Vec<u8> = Vec::new();
    while at + 12 <= bytes.len() {
        let len =
            u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize;
        let kind = &bytes[at + 4..at + 8];
        let data = bytes
            .get(at + 8..at + 8 + len)
            .ok_or_else(|| CodecError::new("png: chunk past end"))?;
        let crc_at = at + 8 + len;
        let want = bytes
            .get(crc_at..crc_at + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or_else(|| CodecError::new("png: missing CRC"))?;
        if crc_update_kind(kind, data) != want {
            return Err(CodecError::new("png: CRC mismatch"));
        }
        match kind {
            b"IHDR" => {
                if data.len() != 13 {
                    return Err(CodecError::new("png: bad IHDR"));
                }
                let h = Header {
                    w: u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize,
                    h: u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize,
                    depth: data[8],
                    color: data[9],
                    interlace: data[12] == 1,
                };
                let ok = matches!(
                    (h.color, h.depth),
                    (0, 1 | 2 | 4 | 8 | 16) | (3, 1 | 2 | 4 | 8) | (2 | 4 | 6, 8 | 16)
                );
                if !ok || h.w == 0 || h.h == 0 || data[10] != 0 || data[11] != 0 {
                    return Err(CodecError::new("png: unsupported IHDR"));
                }
                hd = Some(h);
            }
            b"PLTE" => {
                palette = data.as_chunks::<3>().0.to_vec();
            }
            b"tRNS" => trns = data.to_vec(),
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => break,
            _ => {}
        }
        at = crc_at + 4;
    }
    let hd = hd.ok_or_else(|| CodecError::new("png: no IHDR"))?;
    let raw = zlib_decompress(&idat).map_err(|e| CodecError::new(&format!("png: {e}")))?;
    let pixels = if hd.interlace {
        // Adam7: seven passes, each a small filtered image.
        const P: [(usize, usize, usize, usize); 7] = [
            (0, 0, 8, 8),
            (4, 0, 8, 8),
            (0, 4, 4, 8),
            (2, 0, 4, 4),
            (0, 2, 2, 4),
            (1, 0, 2, 2),
            (0, 1, 1, 2),
        ];
        let mut out = vec![0u8; hd.w * hd.h * 4];
        let mut off = 0;
        for (x0, y0, dx, dy) in P {
            let pw = (hd.w + dx - 1 - x0) / dx;
            let ph = (hd.h + dy - 1 - y0) / dy;
            if pw == 0 || ph == 0 {
                continue;
            }
            let size = (hd.row_bytes(pw) + 1) * ph;
            let part = raw
                .get(off..off + size)
                .ok_or_else(|| CodecError::new("png: interlaced data too short"))?;
            off += size;
            let rows = unfilter(part, pw, ph, &hd)?;
            let px = expand(&rows, pw, ph, &hd, &palette, &trns)?;
            for py in 0..ph {
                for pxx in 0..pw {
                    let (x, y) = (x0 + pxx * dx, y0 + py * dy);
                    let s = (py * pw + pxx) * 4;
                    let d = (y * hd.w + x) * 4;
                    out[d..d + 4].copy_from_slice(&px[s..s + 4]);
                }
            }
        }
        out
    } else {
        let rows = unfilter(&raw, hd.w, hd.h, &hd)?;
        expand(&rows, hd.w, hd.h, &hd, &palette, &trns)?
    };
    #[allow(clippy::cast_possible_truncation)]
    Ok(Decoded {
        width: hd.w as u32,
        height: hd.h as u32,
        rgba: pixels,
    })
}

fn crc_update_kind(kind: &[u8], data: &[u8]) -> u32 {
    crc32_update(crc32_update(0xFFFF_FFFF, kind), data) ^ 0xFFFF_FFFF
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32, alpha: bool) -> Vec<u8> {
        let mut v = Vec::new();
        for y in 0..h {
            for x in 0..w {
                #[allow(clippy::cast_possible_truncation)]
                v.extend([
                    (x * 255 / w.max(1)) as u8,
                    (y * 255 / h.max(1)) as u8,
                    ((x ^ y) & 0xff) as u8,
                    if alpha { ((x + y) % 256) as u8 } else { 255 },
                ]);
            }
        }
        v
    }

    #[test]
    fn round_trip_rgba_and_rgb() {
        for (w, h, a) in [
            (1, 1, false),
            (37, 19, true),
            (64, 48, false),
            (3, 200, true),
        ] {
            let px = gradient(w, h, a);
            let d = decode(&encode(w, h, &px)).unwrap();
            assert_eq!((d.width, d.height), (w, h));
            assert_eq!(d.rgba, px);
        }
    }

    #[test]
    fn a_flat_image_compresses_well() {
        let px = vec![200u8; 256 * 256 * 4];
        assert!(encode(256, 256, &px).len() < 1500);
    }

    /// Build a PNG by hand with arbitrary IHDR fields and raw rows.
    fn handmade(
        w: u32,
        h: u32,
        depth: u8,
        color: u8,
        rows: &[u8],
        extra: &[(&[u8; 4], Vec<u8>)],
    ) -> Vec<u8> {
        let mut out = SIG.to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend(w.to_be_bytes());
        ihdr.extend(h.to_be_bytes());
        ihdr.extend([depth, color, 0, 0, 0]);
        chunk(&mut out, b"IHDR", &ihdr);
        for (k, d) in extra {
            chunk(&mut out, k, d);
        }
        chunk(&mut out, b"IDAT", &zlib_compress(rows));
        chunk(&mut out, b"IEND", &[]);
        out
    }

    #[test]
    fn palette_with_transparency_and_low_bit_depths() {
        // 2-bit palette, 4 pixels in one byte: indices 0,1,2,3.
        let png = handmade(
            4,
            1,
            2,
            3,
            &[0, 0b0001_1011],
            &[
                (b"PLTE", vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 9, 9, 9]),
                (b"tRNS", vec![255, 128]),
            ],
        );
        let d = decode(&png).unwrap();
        assert_eq!(&d.rgba[..8], &[255, 0, 0, 255, 0, 255, 0, 128]);
        assert_eq!(&d.rgba[12..], &[9, 9, 9, 255]);
        // 1-bit grey: 8 pixels.
        let g = decode(&handmade(8, 1, 1, 0, &[0, 0b1010_0000], &[])).unwrap();
        assert_eq!(g.rgba[0], 255);
        assert_eq!(g.rgba[4], 0);
    }

    #[test]
    fn sixteen_bit_and_grey_alpha() {
        let png = handmade(
            1,
            1,
            16,
            6,
            &[0, 0x12, 0x34, 0xAB, 0xCD, 0xFF, 0x00, 0x80, 0x00],
            &[],
        );
        assert_eq!(decode(&png).unwrap().rgba, vec![0x12, 0xAB, 0xFF, 0x80]);
        let ga = handmade(2, 1, 8, 4, &[0, 50, 200, 60, 100], &[]);
        assert_eq!(
            decode(&ga).unwrap().rgba,
            vec![50, 50, 50, 200, 60, 60, 60, 100]
        );
    }

    #[test]
    fn crc_damage_is_caught() {
        let mut p = encode(4, 4, &gradient(4, 4, true));
        p[20] ^= 0xff;
        assert!(decode(&p).is_err());
        assert!(decode(b"not a png").is_err());
    }

    #[test]
    fn adam7_interlace_decodes() {
        // 3×3 RGB8 interlaced, hand-built from the pass layout.
        let px: Vec<[u8; 3]> = (0..9u8).map(|i| [i * 20, i, 255 - i]).collect();
        let at = |x: usize, y: usize| px[y * 3 + x];
        let passes: [(usize, usize, usize, usize); 7] = [
            (0, 0, 8, 8),
            (4, 0, 8, 8),
            (0, 4, 4, 8),
            (2, 0, 4, 4),
            (0, 2, 2, 4),
            (1, 0, 2, 2),
            (0, 1, 1, 2),
        ];
        let mut raw = Vec::new();
        for (x0, y0, dx, dy) in passes {
            let mut y = y0;
            while y < 3 {
                let mut row = vec![0u8];
                let mut x = x0;
                while x < 3 {
                    row.extend(at(x, y));
                    x += dx;
                }
                if row.len() > 1 {
                    raw.extend(row);
                }
                y += dy;
            }
        }
        let mut out = SIG.to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend(3u32.to_be_bytes());
        ihdr.extend(3u32.to_be_bytes());
        ihdr.extend([8, 2, 0, 0, 1]);
        chunk(&mut out, b"IHDR", &ihdr);
        chunk(&mut out, b"IDAT", &zlib_compress(&raw));
        chunk(&mut out, b"IEND", &[]);
        let d = decode(&out).unwrap();
        for (i, expected) in px.iter().enumerate() {
            assert_eq!(&d.rgba[i * 4..i * 4 + 3], expected);
        }
    }
}
