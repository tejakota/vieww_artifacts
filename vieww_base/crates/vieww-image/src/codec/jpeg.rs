//! Baseline JPEG (ITU-T T.81), decoder and encoder, from the spec.
//!
//! **Decoder:** baseline sequential DCT (SOF0) and extended sequential with
//! 8-bit samples (SOF1); Huffman entropy coding with any number of tables;
//! 8- and 16-bit quantisation tables; greyscale and YCbCr with any sampling
//! factors (4:4:4, 4:2:2, 4:2:0, 4:1:1 …), upsampled bilinearly; restart
//! intervals (`DRI`/`RSTn`); APP/COM segments skipped; Adobe-style CMYK is
//! refused by name, as are progressive (SOF2) and arithmetic-coded files —
//! the decoder says which feature it met rather than producing garbage.
//!
//! **Encoder:** baseline 4:2:0 YCbCr with the Annex K tables scaled by a
//! 1–100 quality, standard Huffman tables, separable float DCT. Enough for
//! MJPEG video and round-trip tests.

use super::{CodecError, Decoded};

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27,
    20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58,
    59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

#[derive(Clone, Default)]
struct Huff {
    /// (length, code) → symbol, as a lookup by length.
    maxcode: [i32; 18],
    valptr: [i32; 17],
    mincode: [i32; 17],
    values: Vec<u8>,
}

impl Huff {
    fn new(counts: &[u8; 16], values: Vec<u8>) -> Self {
        let mut h = Self {
            values,
            ..Self::default()
        };
        let (mut code, mut k) = (0i32, 0i32);
        for l in 0..16 {
            let n = i32::from(counts[l]);
            if n == 0 {
                h.maxcode[l + 1] = -1;
            } else {
                h.valptr[l + 1] = k;
                h.mincode[l + 1] = code;
                code += n;
                k += n;
                h.maxcode[l + 1] = code - 1;
            }
            code <<= 1;
        }
        h.maxcode[17] = i32::MAX;
        h
    }
}

struct Reader<'a> {
    d: &'a [u8],
    pos: usize,
    bits: u32,
    n: u32,
}

impl Reader<'_> {
    fn bit(&mut self) -> Result<u32, CodecError> {
        if self.n == 0 {
            let b = *self.d.get(self.pos).ok_or_else(|| CodecError::new("jpeg: scan past end"))?;
            self.pos += 1;
            if b == 0xFF {
                let nx = self.d.get(self.pos).copied().unwrap_or(0);
                if nx == 0 {
                    self.pos += 1; // stuffed byte
                } else {
                    // A marker inside the scan: feed zeros (spec F.2.2.5).
                    self.pos -= 1;
                    self.bits = 0;
                    self.n = 8;
                    let r = (self.bits >> 7) & 1;
                    self.n -= 1;
                    return Ok(r);
                }
            }
            self.bits = u32::from(b);
            self.n = 8;
        }
        self.n -= 1;
        Ok((self.bits >> self.n) & 1)
    }
    fn receive(&mut self, s: u32) -> Result<i32, CodecError> {
        let mut v = 0i32;
        for _ in 0..s {
            #[allow(clippy::cast_possible_wrap)]
            {
                v = (v << 1) | self.bit()? as i32;
            }
        }
        Ok(v)
    }
    fn decode(&mut self, h: &Huff) -> Result<u8, CodecError> {
        #[allow(clippy::cast_possible_wrap)]
        let mut code = self.bit()? as i32;
        for l in 1..=16 {
            if code <= h.maxcode[l] {
                #[allow(clippy::cast_sign_loss)]
                let i = (h.valptr[l] + code - h.mincode[l]) as usize;
                return h.values.get(i).copied().ok_or_else(|| CodecError::new("jpeg: bad code"));
            }
            #[allow(clippy::cast_possible_wrap)]
            {
                code = (code << 1) | self.bit()? as i32;
            }
        }
        Err(CodecError::new("jpeg: bad Huffman code"))
    }
    fn reset(&mut self) {
        self.n = 0;
    }
}

fn extend(v: i32, s: u32) -> i32 {
    if s == 0 {
        0
    } else if v < (1 << (s - 1)) {
        v - (1 << s) + 1
    } else {
        v
    }
}

/// Separable float IDCT of one 8×8 block (natural order), +128, clamped.
fn idct(block: &[f32; 64], out: &mut [u8; 64]) {
    static C: std::sync::OnceLock<[[f32; 8]; 8]> = std::sync::OnceLock::new();
    let c = C.get_or_init(|| {
        let mut c = [[0f32; 8]; 8];
        for (x, row) in c.iter_mut().enumerate() {
            for (u, v) in row.iter_mut().enumerate() {
                let cu = if u == 0 { std::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };
                #[allow(clippy::cast_precision_loss)]
                {
                    *v = cu * ((2 * x + 1) as f32 * u as f32 * std::f32::consts::PI / 16.0).cos();
                }
            }
        }
        c
    });
    let mut tmp = [0f32; 64];
    for y in 0..8 {
        for x in 0..8 {
            let mut s = 0.0;
            for u in 0..8 {
                s += c[x][u] * block[y * 8 + u];
            }
            tmp[y * 8 + x] = s / 2.0;
        }
    }
    for x in 0..8 {
        for y in 0..8 {
            let mut s = 0.0;
            for v in 0..8 {
                s += c[y][v] * tmp[v * 8 + x];
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                out[y * 8 + x] = (s / 2.0 + 128.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

fn fdct(px: &[f32; 64]) -> [f32; 64] {
    let mut out = [0f32; 64];
    for v in 0..8 {
        for u in 0..8 {
            let mut s = 0.0;
            for y in 0..8 {
                for x in 0..8 {
                    #[allow(clippy::cast_precision_loss)]
                    {
                        s += px[y * 8 + x]
                            * ((2 * x + 1) as f32 * u as f32 * std::f32::consts::PI / 16.0).cos()
                            * ((2 * y + 1) as f32 * v as f32 * std::f32::consts::PI / 16.0).cos();
                    }
                }
            }
            let cu = if u == 0 { std::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };
            let cv = if v == 0 { std::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };
            out[v * 8 + u] = 0.25 * cu * cv * s;
        }
    }
    out
}

struct Comp {
    id: u8,
    h: usize,
    v: usize,
    tq: usize,
    td: usize,
    ta: usize,
    /// Decoded samples for the whole padded component plane.
    plane: Vec<u8>,
    pw: usize,
    pred: i32,
}

/// Decode a baseline JPEG to RGBA8.
///
/// # Errors
/// On malformed data or an unsupported coding process (named in the error).
#[allow(clippy::too_many_lines)]
pub fn decode(d: &[u8]) -> Result<Decoded, CodecError> {
    if d.len() < 4 || d[0] != 0xFF || d[1] != 0xD8 {
        return Err(CodecError::new("jpeg: no SOI marker"));
    }
    let mut q = [[0u16; 64]; 4];
    let mut dc: [Huff; 4] = Default::default();
    let mut ac: [Huff; 4] = Default::default();
    let mut comps: Vec<Comp> = Vec::new();
    let (mut w, mut h) = (0usize, 0usize);
    let mut restart = 0usize;
    let mut adobe_transform: Option<u8> = None;
    let mut at = 2;
    loop {
        while at < d.len() && d[at] != 0xFF {
            at += 1;
        }
        while at < d.len() && d[at] == 0xFF {
            at += 1;
        }
        let Some(&m) = d.get(at) else {
            return Err(CodecError::new("jpeg: no EOI"));
        };
        at += 1;
        if m == 0xD9 {
            break;
        }
        if (0xD0..=0xD7).contains(&m) || m == 0x01 {
            continue;
        }
        let len = usize::from(u16::from_be_bytes([
            *d.get(at).ok_or_else(|| CodecError::new("jpeg: truncated"))?,
            *d.get(at + 1).ok_or_else(|| CodecError::new("jpeg: truncated"))?,
        ]));
        let seg = d
            .get(at + 2..at + len)
            .ok_or_else(|| CodecError::new("jpeg: segment past end"))?;
        match m {
            0xDB => {
                let mut i = 0;
                while i < seg.len() {
                    let (pq, tq) = (seg[i] >> 4, usize::from(seg[i] & 15));
                    i += 1;
                    for k in 0..64 {
                        let v = if pq == 0 {
                            u16::from(seg[i + k])
                        } else {
                            u16::from_be_bytes([seg[i + 2 * k], seg[i + 2 * k + 1]])
                        };
                        q[tq & 3][ZIGZAG[k]] = v;
                    }
                    i += if pq == 0 { 64 } else { 128 };
                }
            }
            0xC4 => {
                let mut i = 0;
                while i + 17 <= seg.len() {
                    let (tc, th) = (seg[i] >> 4, usize::from(seg[i] & 15) & 3);
                    let mut counts = [0u8; 16];
                    counts.copy_from_slice(&seg[i + 1..i + 17]);
                    let n: usize = counts.iter().map(|&c| usize::from(c)).sum();
                    let vals = seg
                        .get(i + 17..i + 17 + n)
                        .ok_or_else(|| CodecError::new("jpeg: DHT past end"))?
                        .to_vec();
                    let t = Huff::new(&counts, vals);
                    if tc == 0 {
                        dc[th] = t;
                    } else {
                        ac[th] = t;
                    }
                    i += 17 + n;
                }
            }
            0xC0 | 0xC1 => {
                if seg[0] != 8 {
                    return Err(CodecError::new("jpeg: only 8-bit samples are supported"));
                }
                h = usize::from(u16::from_be_bytes([seg[1], seg[2]]));
                w = usize::from(u16::from_be_bytes([seg[3], seg[4]]));
                let n = usize::from(seg[5]);
                for k in 0..n {
                    let c = &seg[6 + 3 * k..9 + 3 * k];
                    comps.push(Comp {
                        id: c[0],
                        h: usize::from(c[1] >> 4).max(1),
                        v: usize::from(c[1] & 15).max(1),
                        tq: usize::from(c[2]) & 3,
                        td: 0,
                        ta: 0,
                        plane: Vec::new(),
                        pw: 0,
                        pred: 0,
                    });
                }
                if w == 0 || h == 0 || comps.is_empty() {
                    return Err(CodecError::new("jpeg: bad frame header"));
                }
            }
            0xC2 | 0xC6 | 0xCA | 0xCE => {
                return Err(CodecError::new("jpeg: progressive coding is not supported"))
            }
            0xC3 | 0xC5 | 0xC7 | 0xCB | 0xCD | 0xCF => {
                return Err(CodecError::new("jpeg: lossless/hierarchical coding is not supported"))
            }
            0xC9 => return Err(CodecError::new("jpeg: arithmetic coding is not supported")),
            0xDD => restart = usize::from(u16::from_be_bytes([seg[0], seg[1]])),
            0xEE if seg.starts_with(b"Adobe") && seg.len() >= 12 => adobe_transform = Some(seg[11]),
            0xDA => {
                let ns = usize::from(seg[0]);
                let mut order = Vec::with_capacity(ns);
                for k in 0..ns {
                    let (cid, t) = (seg[1 + 2 * k], seg[2 + 2 * k]);
                    let ci = comps
                        .iter()
                        .position(|c| c.id == cid)
                        .ok_or_else(|| CodecError::new("jpeg: scan names an unknown component"))?;
                    comps[ci].td = usize::from(t >> 4) & 3;
                    comps[ci].ta = usize::from(t & 15) & 3;
                    order.push(ci);
                }
                let hmax = comps.iter().map(|c| c.h).max().unwrap_or(1);
                let vmax = comps.iter().map(|c| c.v).max().unwrap_or(1);
                let (mcux, mcuy) = (w.div_ceil(8 * hmax), h.div_ceil(8 * vmax));
                for c in &mut comps {
                    c.pw = mcux * c.h * 8;
                    c.plane = vec![0; c.pw * mcuy * c.v * 8];
                    c.pred = 0;
                }
                let mut r = Reader {
                    d,
                    pos: at + len,
                    bits: 0,
                    n: 0,
                };
                let single = ns == 1;
                let (bx, by) = if single {
                    let c = &comps[order[0]];
                    ((w * c.h).div_ceil(8 * hmax), (h * c.v).div_ceil(8 * vmax))
                } else {
                    (mcux, mcuy)
                };
                let total = bx * by;
                let mut blk = [0f32; 64];
                let mut pix = [0u8; 64];
                let mut decode_block = |c: &mut Comp, r: &mut Reader<'_>, ox: usize, oy: usize| -> Result<(), CodecError> {
                    blk.fill(0.0);
                    let t = r.decode(&dc[c.td])?;
                    let diff = extend(r.receive(u32::from(t))?, u32::from(t));
                    c.pred += diff;
                    blk[0] = c.pred as f32 * f32::from(q[c.tq][0]);
                    let mut k = 1;
                    while k < 64 {
                        let rs = r.decode(&ac[c.ta])?;
                        let (run, s) = (usize::from(rs >> 4), u32::from(rs & 15));
                        if s == 0 {
                            if run == 15 {
                                k += 16;
                                continue;
                            }
                            break;
                        }
                        k += run;
                        if k > 63 {
                            return Err(CodecError::new("jpeg: coefficient index past 63"));
                        }
                        let z = ZIGZAG[k];
                        #[allow(clippy::cast_precision_loss)]
                        {
                            blk[z] = extend(r.receive(s)?, s) as f32 * f32::from(q[c.tq][z]);
                        }
                        k += 1;
                    }
                    idct(&blk, &mut pix);
                    for y in 0..8 {
                        let row = (oy + y) * c.pw + ox;
                        if row + 8 <= c.plane.len() {
                            c.plane[row..row + 8].copy_from_slice(&pix[y * 8..y * 8 + 8]);
                        }
                    }
                    Ok(())
                };
                for n in 0..total {
                    if restart > 0 && n > 0 && n % restart == 0 {
                        // Expect RSTn: realign and reset predictors.
                        r.reset();
                        while r.pos + 1 < d.len() && !(d[r.pos] == 0xFF && (0xD0..=0xD7).contains(&d[r.pos + 1])) {
                            r.pos += 1;
                        }
                        r.pos += 2;
                        for c in &mut comps {
                            c.pred = 0;
                        }
                    }
                    if single {
                        let c = &mut comps[order[0]];
                        let (x, y) = (n % bx, n / bx);
                        decode_block(c, &mut r, x * 8, y * 8)?;
                    } else {
                        let (mx, my) = (n % mcux, n / mcux);
                        for &ci in &order {
                            let c = &mut comps[ci];
                            for v in 0..c.v {
                                for hh in 0..c.h {
                                    let (ox, oy) = ((mx * c.h + hh) * 8, (my * c.v + v) * 8);
                                    decode_block(c, &mut r, ox, oy)?;
                                }
                            }
                        }
                    }
                }
                at = r.pos;
                continue;
            }
            _ => {}
        }
        at += len;
    }
    if comps.is_empty() {
        return Err(CodecError::new("jpeg: no frame"));
    }
    if comps.len() == 4 {
        return Err(CodecError::new("jpeg: CMYK/YCCK is not supported"));
    }
    let hmax = comps.iter().map(|c| c.h).max().unwrap_or(1);
    let vmax = comps.iter().map(|c| c.v).max().unwrap_or(1);
    // Sample a component at full-resolution (x, y), bilinear.
    let sample = |c: &Comp, x: usize, y: usize| -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let (sx, sy) = (
            ((x as f32 + 0.5) * c.h as f32 / hmax as f32 - 0.5).max(0.0),
            ((y as f32 + 0.5) * c.v as f32 / vmax as f32 - 0.5).max(0.0),
        );
        let ph = c.plane.len() / c.pw.max(1);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (x0, y0) = (sx as usize, sy as usize);
        let (x1, y1) = ((x0 + 1).min(c.pw - 1), (y0 + 1).min(ph - 1));
        #[allow(clippy::cast_precision_loss)]
        let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
        let g = |x: usize, y: usize| f32::from(c.plane[y * c.pw + x]);
        let a = g(x0, y0) + (g(x1, y0) - g(x0, y0)) * fx;
        let b = g(x0, y1) + (g(x1, y1) - g(x0, y1)) * fx;
        a + (b - a) * fy
    };
    let mut rgba = Vec::with_capacity(w * h * 4);
    let to8 = |v: f32| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let b = v.round().clamp(0.0, 255.0) as u8;
        b
    };
    let rgb_direct = comps.len() == 3 && adobe_transform == Some(0);
    for y in 0..h {
        for x in 0..w {
            if comps.len() == 1 {
                let g = to8(sample(&comps[0], x, y));
                rgba.extend([g, g, g, 255]);
            } else {
                let (a, b, c) = (sample(&comps[0], x, y), sample(&comps[1], x, y), sample(&comps[2], x, y));
                if rgb_direct {
                    rgba.extend([to8(a), to8(b), to8(c), 255]);
                } else {
                    let (cb, cr) = (b - 128.0, c - 128.0);
                    rgba.extend([
                        to8(a + 1.402 * cr),
                        to8(a - 0.344_136 * cb - 0.714_136 * cr),
                        to8(a + 1.772 * cb),
                        255,
                    ]);
                }
            }
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    Ok(Decoded {
        width: w as u32,
        height: h as u32,
        rgba,
    })
}

// ───────────────────────────── encoder ─────────────────────────────

const LUMA_Q: [u8; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56,
    14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113,
    92, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99,
];
const CHROMA_Q: [u8; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99, 99,
    47, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
];
const DC_L_BITS: [u8; 16] = [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0];
const DC_C_BITS: [u8; 16] = [0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0];
const DC_VALS: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
const AC_L_BITS: [u8; 16] = [0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 0x7d];
const AC_C_BITS: [u8; 16] = [0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 0x77];
const AC_L_VALS: [u8; 162] = [
    0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07,
    0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xa1, 0x08, 0x23, 0x42, 0xb1, 0xc1, 0x15, 0x52, 0xd1, 0xf0,
    0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0a, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x25, 0x26, 0x27, 0x28,
    0x29, 0x2a, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49,
    0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
    0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
    0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7,
    0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5,
    0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe1, 0xe2,
    0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8,
    0xf9, 0xfa,
];
const AC_C_VALS: [u8; 162] = [
    0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71,
    0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91, 0xa1, 0xb1, 0xc1, 0x09, 0x23, 0x33, 0x52, 0xf0,
    0x15, 0x62, 0x72, 0xd1, 0x0a, 0x16, 0x24, 0x34, 0xe1, 0x25, 0xf1, 0x17, 0x18, 0x19, 0x1a, 0x26,
    0x27, 0x28, 0x29, 0x2a, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48,
    0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68,
    0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
    0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5,
    0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3,
    0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda,
    0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8,
    0xf9, 0xfa,
];

/// `(code, length)` per symbol.
fn enc_table(bits: &[u8; 16], vals: &[u8]) -> [(u16, u8); 256] {
    let mut t = [(0u16, 0u8); 256];
    let (mut code, mut k) = (0u16, 0usize);
    for (l, &n) in bits.iter().enumerate() {
        for _ in 0..n {
            #[allow(clippy::cast_possible_truncation)]
            {
                t[usize::from(vals[k])] = (code, (l + 1) as u8);
            }
            code += 1;
            k += 1;
        }
        code <<= 1;
    }
    t
}

struct Writer {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}

impl Writer {
    fn put(&mut self, code: u16, len: u8) {
        for i in (0..len).rev() {
            self.acc = (self.acc << 1) | u32::from((code >> i) & 1);
            self.n += 1;
            if self.n == 8 {
                #[allow(clippy::cast_possible_truncation)]
                let b = self.acc as u8;
                self.out.push(b);
                if b == 0xFF {
                    self.out.push(0);
                }
                self.acc = 0;
                self.n = 0;
            }
        }
    }
    fn flush(&mut self) {
        while self.n != 0 {
            self.put(1, 1);
        }
    }
}

fn category(v: i32) -> (u8, u16) {
    let a = v.unsigned_abs();
    #[allow(clippy::cast_possible_truncation)]
    let s = (32 - a.leading_zeros()) as u8;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let bits = if v < 0 { (v - 1 + (1 << s)) as u16 } else { v as u16 };
    (s, bits)
}

/// Encode RGBA8 (alpha ignored) as baseline 4:2:0 JPEG at `quality` 1..=100.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn encode(width: u32, height: u32, rgba: &[u8], quality: u8) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let quality = u32::from(quality.clamp(1, 100));
    let scale = if quality < 50 { 5000 / quality } else { 200 - quality * 2 };
    let qt = |base: &[u8; 64]| -> [u8; 64] {
        base.map(|v| {
            #[allow(clippy::cast_possible_truncation)]
            let q = ((u32::from(v) * scale + 50) / 100).clamp(1, 255) as u8;
            q
        })
    };
    let (ql, qc) = (qt(&LUMA_Q), qt(&CHROMA_Q));
    let mut out = vec![0xFF, 0xD8];
    // JFIF APP0.
    out.extend([0xFF, 0xE0, 0, 16]);
    out.extend(b"JFIF\0");
    out.extend([1, 1, 0, 0, 1, 0, 1, 0, 0]);
    for (id, t) in [(0u8, &ql), (1, &qc)] {
        out.extend([0xFF, 0xDB, 0, 67, id]);
        for k in 0..64 {
            out.push(t[ZIGZAG[k]]);
        }
    }
    out.extend([0xFF, 0xC0, 0, 17, 8]);
    #[allow(clippy::cast_possible_truncation)]
    {
        out.extend((h as u16).to_be_bytes());
        out.extend((w as u16).to_be_bytes());
    }
    out.extend([3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
    for (class_id, bits, vals) in [
        (0x00u8, &DC_L_BITS, &DC_VALS[..]),
        (0x10, &AC_L_BITS, &AC_L_VALS[..]),
        (0x01, &DC_C_BITS, &DC_VALS[..]),
        (0x11, &AC_C_BITS, &AC_C_VALS[..]),
    ] {
        let n: usize = bits.iter().map(|&b| usize::from(b)).sum();
        #[allow(clippy::cast_possible_truncation)]
        out.extend([0xFF, 0xC4]);
        #[allow(clippy::cast_possible_truncation)]
        out.extend(((19 + n) as u16).to_be_bytes());
        out.push(class_id);
        out.extend(bits);
        out.extend(&vals[..n]);
    }
    out.extend([0xFF, 0xDA, 0, 12, 3, 1, 0x00, 2, 0x11, 3, 0x11, 0, 63, 0]);
    let (dcl, acl) = (enc_table(&DC_L_BITS, &DC_VALS), enc_table(&AC_L_BITS, &AC_L_VALS));
    let (dcc, acc) = (enc_table(&DC_C_BITS, &DC_VALS), enc_table(&AC_C_BITS, &AC_C_VALS));
    let px = |x: usize, y: usize| -> [f32; 3] {
        let (x, y) = (x.min(w - 1), y.min(h - 1));
        let p = &rgba[(y * w + x) * 4..(y * w + x) * 4 + 3];
        let (r, g, b) = (f32::from(p[0]), f32::from(p[1]), f32::from(p[2]));
        [
            0.299 * r + 0.587 * g + 0.114 * b,
            -0.168_736 * r - 0.331_264 * g + 0.5 * b + 128.0,
            0.5 * r - 0.418_688 * g - 0.081_312 * b + 128.0,
        ]
    };
    let mut wr = Writer {
        out: Vec::new(),
        acc: 0,
        n: 0,
    };
    let mut pred = [0i32; 3];
    let block = |samples: &[f32; 64], q: &[u8; 64], dc: &[(u16, u8); 256], ac: &[(u16, u8); 256], p: &mut i32, wr: &mut Writer| {
        let shifted = samples.map(|v| v - 128.0);
        let coef = fdct(&shifted);
        let mut zz = [0i32; 64];
        for k in 0..64 {
            #[allow(clippy::cast_possible_truncation)]
            {
                zz[k] = (coef[ZIGZAG[k]] / f32::from(q[ZIGZAG[k]])).round() as i32;
            }
        }
        let diff = zz[0] - *p;
        *p = zz[0];
        let (s, bits) = category(diff);
        let (c, l) = dc[usize::from(s)];
        wr.put(c, l);
        wr.put(bits, s);
        let mut run = 0;
        for &v in &zz[1..] {
            if v == 0 {
                run += 1;
                continue;
            }
            while run > 15 {
                let (c, l) = ac[0xF0];
                wr.put(c, l);
                run -= 16;
            }
            let (s, bits) = category(v);
            let (c, l) = ac[(run << 4) | usize::from(s)];
            wr.put(c, l);
            wr.put(bits, s);
            run = 0;
        }
        if run > 0 {
            let (c, l) = ac[0];
            wr.put(c, l);
        }
    };
    for my in 0..h.div_ceil(16) {
        for mx in 0..w.div_ceil(16) {
            for (by, bx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let mut s = [0f32; 64];
                for y in 0..8 {
                    for x in 0..8 {
                        s[y * 8 + x] = px(mx * 16 + bx * 8 + x, my * 16 + by * 8 + y)[0];
                    }
                }
                block(&s, &ql, &dcl, &acl, &mut pred[0], &mut wr);
            }
            for ch in 1..3 {
                let mut s = [0f32; 64];
                for y in 0..8 {
                    for x in 0..8 {
                        let (x0, y0) = (mx * 16 + x * 2, my * 16 + y * 2);
                        s[y * 8 + x] = (px(x0, y0)[ch] + px(x0 + 1, y0)[ch] + px(x0, y0 + 1)[ch] + px(x0 + 1, y0 + 1)[ch]) / 4.0;
                    }
                }
                block(&s, &qc, &dcc, &acc, &mut pred[ch], &mut wr);
            }
        }
    }
    wr.flush();
    out.extend(wr.out);
    out.extend([0xFF, 0xD9]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(w: usize, h: usize) -> Vec<u8> {
        let mut v = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    let fx = x as f32 / w as f32;
                    let fy = y as f32 / h as f32;
                    v.extend([
                        (255.0 * fx) as u8,
                        (255.0 * (0.5 + 0.5 * (fx * 6.0 + fy * 3.0).sin())) as u8,
                        (255.0 * fy) as u8,
                        255,
                    ]);
                }
            }
        }
        v
    }

    fn psnr(a: &[u8], b: &[u8]) -> f64 {
        let mse: f64 = a
            .iter()
            .zip(b)
            .enumerate()
            .filter(|(i, _)| i % 4 != 3)
            .map(|(_, (x, y))| (f64::from(*x) - f64::from(*y)).powi(2))
            .sum::<f64>()
            / (a.len() as f64 * 0.75);
        10.0 * (255.0f64.powi(2) / mse.max(1e-9)).log10()
    }

    #[test]
    fn encode_decode_round_trip_quality() {
        for (w, h) in [(64, 48), (37, 23), (33, 17)] {
            let src = photo(w, h);
            let d = decode(&encode(w as u32, h as u32, &src, 90)).unwrap();
            assert_eq!((d.width as usize, d.height as usize), (w, h));
            let p = psnr(&src, &d.rgba);
            assert!(p > 30.0, "{w}x{h}: PSNR {p:.1}");
        }
        // Tiny and odd sizes: a grey ramp has no chroma for 4:2:0 to lose,
        // so any error here would be an edge-padding or upsampling bug.
        for (w, h) in [(8, 8), (100, 3), (1, 1), (17, 9)] {
            let src: Vec<u8> = (0..w * h)
                .flat_map(|i| {
                    #[allow(clippy::cast_possible_truncation)]
                    let g = ((i % w) * 255 / w.max(1)) as u8;
                    [g, g, g, 255]
                })
                .collect();
            let d = decode(&encode(w as u32, h as u32, &src, 95)).unwrap();
            let p = psnr(&src, &d.rgba);
            assert!(p > 35.0, "grey {w}x{h}: PSNR {p:.1}");
        }
    }

    #[test]
    fn quality_trades_size_for_fidelity() {
        let src = photo(96, 64);
        let lo = encode(96, 64, &src, 20);
        let hi = encode(96, 64, &src, 95);
        assert!(lo.len() < hi.len());
        let (dl, dh) = (decode(&lo).unwrap(), decode(&hi).unwrap());
        assert!(psnr(&src, &dh.rgba) > psnr(&src, &dl.rgba));
    }

    #[test]
    fn unsupported_processes_are_named() {
        let mut j = encode(8, 8, &photo(8, 8), 80);
        let i = j.windows(2).position(|w| w == [0xFF, 0xC0]).unwrap();
        j[i + 1] = 0xC2;
        let e = decode(&j).unwrap_err();
        assert!(e.to_string().contains("progressive"));
        assert!(decode(b"\xFF\xD8\xFF\xD9").is_err());
    }

    #[test]
    fn idct_of_dc_only_is_flat() {
        let mut b = [0f32; 64];
        b[0] = 80.0; // DC: mean +10 after the /8 scaling
        let mut o = [0u8; 64];
        idct(&b, &mut o);
        assert!(o.iter().all(|&v| v == 138));
    }
}
