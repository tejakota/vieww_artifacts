//! GIF89a — animated decode and encode, LZW written from the spec.
//!
//! Decoding composes every frame onto a full-size canvas the way a browser
//! does: global and local colour tables, the Graphic Control Extension's
//! delay, transparent index and **disposal** (none / keep, restore to
//! background, restore to previous), interlaced rows, and the NETSCAPE2.0
//! loop count. Each output frame is the whole canvas, RGBA8.
//!
//! Encoding quantises each frame with **median cut** (to 255 colours plus a
//! reserved transparent index when any pixel is under half alpha), maps
//! pixels to the nearest palette entry, and LZW-compresses with code-size
//! growth and table resets.

use super::CodecError;

/// One composed frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GifFrame {
    pub rgba: Vec<u8>,
    /// Hundredths of a second.
    pub delay_cs: u16,
}

/// A decoded animation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gif {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<GifFrame>,
    /// 0 = loop forever; `None` = play once (no NETSCAPE block).
    pub loops: Option<u16>,
}

fn lzw_decode(data: &[u8], min_size: u8, out_len: usize) -> Result<Vec<u8>, CodecError> {
    if !(2..=11).contains(&min_size) {
        return Err(CodecError::new("gif: bad LZW minimum code size"));
    }
    let clear = 1usize << min_size;
    let end = clear + 1;
    let mut prefix: Vec<u16> = vec![0; 4096];
    let mut suffix: Vec<u8> = vec![0; 4096];
    let mut first: Vec<u8> = vec![0; 4096];
    for i in 0..clear {
        #[allow(clippy::cast_possible_truncation)]
        {
            suffix[i] = i as u8;
            first[i] = i as u8;
        }
    }
    let mut size = u32::from(min_size) + 1;
    let mut next = end + 1;
    let mut prev: Option<usize> = None;
    let mut out = Vec::with_capacity(out_len);
    let (mut bits, mut nbits, mut pos) = (0u32, 0u32, 0usize);
    let mut stack = Vec::with_capacity(4096);
    loop {
        while nbits < size {
            let Some(&b) = data.get(pos) else {
                return Ok(out);
            };
            pos += 1;
            bits |= u32::from(b) << nbits;
            nbits += 8;
        }
        let code = (bits & ((1 << size) - 1)) as usize;
        bits >>= size;
        nbits -= size;
        if code == clear {
            size = u32::from(min_size) + 1;
            next = end + 1;
            prev = None;
            continue;
        }
        if code == end {
            return Ok(out);
        }
        let Some(p) = prev else {
            if code >= clear {
                return Err(CodecError::new("gif: first code is not a literal"));
            }
            #[allow(clippy::cast_possible_truncation)]
            out.push(code as u8);
            prev = Some(code);
            continue;
        };
        let (walk, extra) = if code < next {
            (code, None)
        } else if code == next {
            (p, Some(first[p]))
        } else {
            return Err(CodecError::new("gif: LZW code out of range"));
        };
        stack.clear();
        let mut c = walk;
        while c >= clear {
            stack.push(suffix[c]);
            c = usize::from(prefix[c]);
        }
        stack.push(suffix[c]);
        let head = *stack.last().expect("non-empty");
        out.extend(stack.iter().rev());
        if let Some(e) = extra {
            out.push(e);
        }
        if next < 4096 {
            #[allow(clippy::cast_possible_truncation)]
            {
                prefix[next] = p as u16;
            }
            suffix[next] = if extra.is_some() { first[p] } else { head };
            first[next] = first[p];
            next += 1;
            if next == (1 << size) && size < 12 {
                size += 1;
            }
        }
        prev = Some(code);
        if out.len() >= out_len {
            return Ok(out);
        }
    }
}

/// Decode a GIF into composed RGBA8 frames.
///
/// # Errors
/// On a bad header or malformed blocks.
#[allow(clippy::too_many_lines)]
pub fn decode(bytes: &[u8]) -> Result<Gif, CodecError> {
    if bytes.len() < 13 || !(bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        return Err(CodecError::new("gif: bad signature"));
    }
    let w = usize::from(u16::from_le_bytes([bytes[6], bytes[7]]));
    let h = usize::from(u16::from_le_bytes([bytes[8], bytes[9]]));
    let packed = bytes[10];
    let bg = usize::from(bytes[11]);
    let mut at = 13;
    let read_table = |at: &mut usize, n: usize| -> Result<Vec<[u8; 3]>, CodecError> {
        let t = bytes
            .get(*at..*at + 3 * n)
            .ok_or_else(|| CodecError::new("gif: colour table past end"))?;
        *at += 3 * n;
        Ok(t.as_chunks::<3>().0.to_vec())
    };
    let global = if packed & 0x80 != 0 {
        read_table(&mut at, 2 << (packed & 7))?
    } else {
        Vec::new()
    };
    let mut canvas = vec![0u8; w * h * 4];
    let mut frames = Vec::new();
    let mut loops = None;
    let (mut delay, mut transparent, mut disposal) = (0u16, None::<usize>, 0u8);
    let sub_blocks = |at: &mut usize| -> Result<Vec<u8>, CodecError> {
        let mut out = Vec::new();
        loop {
            let n = usize::from(*bytes.get(*at).ok_or_else(|| CodecError::new("gif: truncated"))?);
            *at += 1;
            if n == 0 {
                return Ok(out);
            }
            out.extend_from_slice(
                bytes
                    .get(*at..*at + n)
                    .ok_or_else(|| CodecError::new("gif: sub-block past end"))?,
            );
            *at += n;
        }
    };
    while at < bytes.len() {
        match bytes[at] {
            0x21 => {
                let label = *bytes.get(at + 1).ok_or_else(|| CodecError::new("gif: truncated"))?;
                at += 2;
                let data = sub_blocks(&mut at)?;
                if label == 0xF9 && data.len() >= 4 {
                    disposal = (data[0] >> 2) & 7;
                    delay = u16::from_le_bytes([data[1], data[2]]);
                    transparent = (data[0] & 1 == 1).then_some(usize::from(data[3]));
                } else if label == 0xFF && data.starts_with(b"NETSCAPE2.0") && data.len() >= 14 {
                    loops = Some(u16::from_le_bytes([data[12], data[13]]));
                }
            }
            0x2C => {
                let d = bytes
                    .get(at + 1..at + 10)
                    .ok_or_else(|| CodecError::new("gif: image descriptor past end"))?;
                let (fx, fy) = (
                    usize::from(u16::from_le_bytes([d[0], d[1]])),
                    usize::from(u16::from_le_bytes([d[2], d[3]])),
                );
                let (fw, fh) = (
                    usize::from(u16::from_le_bytes([d[4], d[5]])),
                    usize::from(u16::from_le_bytes([d[6], d[7]])),
                );
                let fp = d[8];
                at += 10;
                let local = if fp & 0x80 != 0 {
                    Some(read_table(&mut at, 2 << (fp & 7))?)
                } else {
                    None
                };
                let table = local.as_ref().unwrap_or(&global);
                let min = *bytes.get(at).ok_or_else(|| CodecError::new("gif: truncated"))?;
                at += 1;
                let lzw = sub_blocks(&mut at)?;
                let idx = lzw_decode(&lzw, min, fw * fh)?;
                let saved = (disposal == 3).then(|| canvas.clone());
                // Interlaced row order: 0,8,16…; 4,12…; 2,6,10…; 1,3,5…
                let order: Vec<usize> = if fp & 0x40 != 0 {
                    (0..fh)
                        .step_by(8)
                        .chain((4..fh).step_by(8))
                        .chain((2..fh).step_by(4))
                        .chain((1..fh).step_by(2))
                        .collect()
                } else {
                    (0..fh).collect()
                };
                for (src_row, &ry) in order.iter().enumerate() {
                    for x in 0..fw {
                        let Some(&ci) = idx.get(src_row * fw + x) else {
                            continue;
                        };
                        let ci = usize::from(ci);
                        if Some(ci) == transparent {
                            continue;
                        }
                        let (cx, cy) = (fx + x, fy + ry);
                        if cx >= w || cy >= h {
                            continue;
                        }
                        let c = table.get(ci).copied().unwrap_or([0, 0, 0]);
                        let o = (cy * w + cx) * 4;
                        canvas[o..o + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
                    }
                }
                frames.push(GifFrame {
                    rgba: canvas.clone(),
                    delay_cs: delay,
                });
                match disposal {
                    2 => {
                        let _ = bg;
                        for y in fy..(fy + fh).min(h) {
                            for x in fx..(fx + fw).min(w) {
                                let o = (y * w + x) * 4;
                                canvas[o..o + 4].copy_from_slice(&[0, 0, 0, 0]);
                            }
                        }
                    }
                    3 => {
                        if let Some(s) = saved {
                            canvas = s;
                        }
                    }
                    _ => {}
                }
                delay = 0;
                transparent = None;
                disposal = 0;
            }
            0x3B => break,
            _ => return Err(CodecError::new("gif: unknown block")),
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    Ok(Gif {
        width: w as u32,
        height: h as u32,
        frames,
        loops,
    })
}

/// Median-cut quantisation to at most `n` colours.
#[must_use]
pub fn median_cut(pixels: &[[u8; 3]], n: usize) -> Vec<[u8; 3]> {
    if pixels.is_empty() {
        return vec![[0, 0, 0]];
    }
    let mut boxes: Vec<Vec<[u8; 3]>> = vec![pixels.to_vec()];
    while boxes.len() < n {
        // Split the box with the widest channel range.
        let (bi, ch, range) = boxes
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let mut best = (0, 0u8);
                for c in 0..3 {
                    let lo = b.iter().map(|p| p[c]).min().unwrap_or(0);
                    let hi = b.iter().map(|p| p[c]).max().unwrap_or(0);
                    if hi - lo > best.1 {
                        best = (c, hi - lo);
                    }
                }
                (i, best.0, best.1)
            })
            .max_by_key(|t| (t.2, std::cmp::Reverse(t.0)))
            .expect("a box");
        if range == 0 {
            break;
        }
        let mut b = boxes.swap_remove(bi);
        b.sort_unstable_by_key(|p| p[ch]);
        let tail = b.split_off(b.len() / 2);
        boxes.push(b);
        boxes.push(tail);
    }
    boxes
        .iter()
        .map(|b| {
            let mut s = [0u64; 3];
            for p in b {
                for c in 0..3 {
                    s[c] += u64::from(p[c]);
                }
            }
            #[allow(clippy::cast_possible_truncation)]
            s.map(|v| (v / b.len().max(1) as u64) as u8)
        })
        .collect()
}

fn lzw_encode(idx: &[u8], min_size: u8) -> Vec<u8> {
    use std::collections::HashMap;
    let clear = 1u16 << min_size;
    let end = clear + 1;
    let mut out = Vec::new();
    let (mut bits, mut nbits) = (0u32, 0u32);
    let mut size = u32::from(min_size) + 1;
    let mut put = |code: u16, size: u32, out: &mut Vec<u8>| {
        bits |= u32::from(code) << nbits;
        nbits += size;
        while nbits >= 8 {
            #[allow(clippy::cast_possible_truncation)]
            out.push(bits as u8);
            bits >>= 8;
            nbits -= 8;
        }
    };
    let mut dict: HashMap<(u16, u8), u16> = HashMap::new();
    let mut next = end + 1;
    put(clear, size, &mut out);
    let mut cur: Option<u16> = None;
    for &b in idx {
        let Some(c) = cur else {
            cur = Some(u16::from(b));
            continue;
        };
        if let Some(&code) = dict.get(&(c, b)) {
            cur = Some(code);
            continue;
        }
        put(c, size, &mut out);
        if next < 4096 {
            dict.insert((c, b), next);
            next += 1;
            if u32::from(next) > (1 << size) && size < 12 {
                size += 1;
            }
        } else {
            put(clear, size, &mut out);
            dict.clear();
            next = end + 1;
            size = u32::from(min_size) + 1;
        }
        cur = Some(u16::from(b));
    }
    if let Some(c) = cur {
        put(c, size, &mut out);
    }
    put(end, size, &mut out);
    if nbits > 0 {
        #[allow(clippy::cast_possible_truncation)]
        out.push(bits as u8);
    }
    out
}

/// Encode RGBA8 frames as a looping animated GIF.
///
/// # Panics
/// If a frame's length is not `width × height × 4`.
#[must_use]
pub fn encode(width: u16, height: u16, frames: &[(Vec<u8>, u16)], loops: u16) -> Vec<u8> {
    let (w, h) = (usize::from(width), usize::from(height));
    let mut out = b"GIF89a".to_vec();
    out.extend(width.to_le_bytes());
    out.extend(height.to_le_bytes());
    out.extend([0x00, 0, 0]); // no global table
    out.extend([0x21, 0xFF, 11]);
    out.extend(b"NETSCAPE2.0");
    out.extend([3, 1]);
    out.extend(loops.to_le_bytes());
    out.push(0);
    for (rgba, delay) in frames {
        assert_eq!(rgba.len(), w * h * 4, "frame size");
        let px = rgba.as_chunks::<4>().0;
        let has_alpha = px.iter().any(|p| p[3] < 128);
        let opaque: Vec<[u8; 3]> = px.iter().filter(|p| p[3] >= 128).map(|p| [p[0], p[1], p[2]]).collect();
        let mut pal = median_cut(&opaque, if has_alpha { 255 } else { 256 });
        let tindex = if has_alpha {
            pal.push([0, 0, 0]);
            Some(pal.len() - 1)
        } else {
            None
        };
        let bits = (usize::BITS - (pal.len().max(2) - 1).leading_zeros()).max(1);
        let table_size = 1usize << bits;
        let mut cache = std::collections::HashMap::new();
        let idx: Vec<u8> = px
            .iter()
            .map(|p| {
                if p[3] < 128 {
                    #[allow(clippy::cast_possible_truncation)]
                    return tindex.unwrap_or(0) as u8;
                }
                let key = [p[0], p[1], p[2]];
                *cache.entry(key).or_insert_with(|| {
                    let n = pal[..pal.len() - usize::from(has_alpha)]
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, c)| {
                            (0..3)
                                .map(|k| (i32::from(c[k]) - i32::from(key[k])).pow(2))
                                .sum::<i32>()
                        })
                        .map_or(0, |(i, _)| i);
                    #[allow(clippy::cast_possible_truncation)]
                    let n = n as u8;
                    n
                })
            })
            .collect();
        // Graphic Control Extension.
        out.extend([0x21, 0xF9, 4]);
        #[allow(clippy::cast_possible_truncation)]
        out.push(if has_alpha { 0b0000_1001 } else { 0b0000_0100 });
        out.extend(delay.to_le_bytes());
        #[allow(clippy::cast_possible_truncation)]
        out.push(tindex.unwrap_or(0) as u8);
        out.push(0);
        // Image descriptor with a local table.
        out.push(0x2C);
        out.extend([0, 0, 0, 0]);
        out.extend(width.to_le_bytes());
        out.extend(height.to_le_bytes());
        #[allow(clippy::cast_possible_truncation)]
        out.push(0x80 | (bits - 1) as u8);
        for i in 0..table_size {
            out.extend(pal.get(i).copied().unwrap_or([0, 0, 0]));
        }
        #[allow(clippy::cast_possible_truncation)]
        let min = bits.max(2) as u8;
        out.push(min);
        for chunk in lzw_encode(&idx, min).chunks(255) {
            #[allow(clippy::cast_possible_truncation)]
            out.push(chunk.len() as u8);
            out.extend_from_slice(chunk);
        }
        out.push(0);
    }
    out.push(0x3B);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lzw_round_trips_across_table_resets() {
        let mut rng = 7u32;
        let data: Vec<u8> = (0..40_000)
            .map(|i| {
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                #[allow(clippy::cast_possible_truncation)]
                if i % 3 == 0 {
                    (rng % 16) as u8
                } else {
                    (i / 97 % 16) as u8
                }
            })
            .collect();
        let enc = lzw_encode(&data, 4);
        assert_eq!(lzw_decode(&enc, 4, data.len()).unwrap(), data);
    }

    #[test]
    fn frames_round_trip_with_few_colours_exactly() {
        let (w, h) = (20u16, 10u16);
        let mk = |shift: u8| -> Vec<u8> {
            (0..200)
                .flat_map(|i: u32| {
                    let c = ((i / 5) as u8 % 4).wrapping_add(shift) % 4;
                    [c * 60, 255 - c * 60, 30, 255]
                })
                .collect()
        };
        let frames = vec![(mk(0), 5), (mk(1), 7), (mk(2), 9)];
        let g = decode(&encode(w, h, &frames, 0)).unwrap();
        assert_eq!((g.width, g.height, g.loops), (20, 10, Some(0)));
        assert_eq!(g.frames.len(), 3);
        for (a, b) in g.frames.iter().zip(&frames) {
            assert_eq!(a.rgba, b.0);
            assert_eq!(a.delay_cs, b.1);
        }
    }

    #[test]
    fn transparency_is_preserved() {
        let px: Vec<u8> = (0..16).flat_map(|i| if i % 2 == 0 { [255, 0, 0, 255] } else { [0, 0, 0, 0] }).collect();
        let g = decode(&encode(4, 4, &[(px.clone(), 1)], 0)).unwrap();
        assert_eq!(g.frames[0].rgba, px);
    }

    #[test]
    fn many_colours_quantise_closely() {
        let px: Vec<u8> = (0..64 * 64u32)
            .flat_map(|i| {
                #[allow(clippy::cast_possible_truncation)]
                [(i % 64 * 4) as u8, (i / 64 * 4) as u8, 128, 255]
            })
            .collect();
        let g = decode(&encode(64, 64, &[(px.clone(), 1)], 0)).unwrap();
        let err: f64 = g.frames[0]
            .rgba
            .iter()
            .zip(&px)
            .map(|(a, b)| f64::from((i16::from(*a) - i16::from(*b)).unsigned_abs()))
            .sum::<f64>()
            / px.len() as f64;
        assert!(err < 4.0, "mean abs error {err}");
    }

    #[test]
    fn median_cut_keeps_distinct_colours() {
        let p = median_cut(&[[0, 0, 0], [255, 255, 255], [255, 0, 0]], 4);
        assert!(p.contains(&[255, 0, 0]) && p.contains(&[0, 0, 0]));
    }

    #[test]
    fn junk_is_refused() {
        assert!(decode(b"PNG...").is_err());
        assert!(lzw_decode(&[0xff], 1, 4).is_err());
    }
}
