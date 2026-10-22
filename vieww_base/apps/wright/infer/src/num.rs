//! Numbers: half floats, and the 8-bit blocks the weights are stored in.
//!
//! ```text
//!  Q8_0 block (34 bytes):  scale:f16  q[32]:i8        value_i = scale * q_i
//! ```
//!
//! A matrix-vector product with Q8_0 weights quantizes the *vector* into
//! the same blocks too, so each block's dot product is 32 integer
//! multiplies (which the compiler vectorizes) and one float multiply:
//!
//! ```text
//!  w . x  =  sum over blocks  ( w.scale * x.scale * sum_i w.q_i * x.q_i )
//! ```
//!
//! This is exactly what llama.cpp's CPU path does for Q8_0 x Q8_0, so the
//! two produce the same numbers up to float rounding.

/// IEEE 754 half -> single.
pub fn f16_to_f32(h: u16) -> f32 {
    let sign = u32::from(h >> 15) << 31;
    let exp = u32::from((h >> 10) & 0x1f);
    let frac = u32::from(h & 0x3ff);
    let bits = match (exp, frac) {
        (0, 0) => sign,
        (0, _) => {
            // Subnormal: renormalize.
            let mut e = 127 - 15 + 1;
            let mut f = frac;
            while f & 0x400 == 0 {
                f <<= 1;
                e -= 1;
            }
            sign | ((e as u32) << 23) | ((f & 0x3ff) << 13)
        }
        (0x1f, 0) => sign | 0x7f80_0000,
        (0x1f, _) => sign | 0x7fc0_0000,
        _ => sign | ((exp + 127 - 15) << 23) | (frac << 13),
    };
    f32::from_bits(bits)
}

/// IEEE 754 single -> half, round to nearest even.
pub fn f32_to_f16(x: f32) -> u16 {
    let bits = x.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32;
    let frac = bits & 0x7f_ffff;
    if exp == 0xff {
        return sign | 0x7c00 | if frac != 0 { 0x200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = frac | 0x80_0000;
        let shift = (14 - e) as u32;
        let half = 1u32 << (shift - 1);
        let rem = m & ((1 << shift) - 1);
        let mut out = m >> shift;
        if rem > half || (rem == half && out & 1 == 1) {
            out += 1;
        }
        return sign | out as u16;
    }
    let mut out = ((e as u32) << 10) | (frac >> 13);
    let rem = frac & 0x1fff;
    if rem > 0x1000 || (rem == 0x1000 && out & 1 == 1) {
        out += 1;
    }
    sign | out as u16
}

/// Every half float as a single, looked up: the attention loop converts
/// one per cached value, so a branchy conversion there would dominate.
pub fn f16_table() -> &'static [f32] {
    static TABLE: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| (0..=u16::MAX).map(f16_to_f32).collect())
}

pub fn bf16_to_f32(h: u16) -> f32 {
    f32::from_bits(u32::from(h) << 16)
}

/// Read `n` floats of `dtype` from raw bytes into f32.
pub fn to_f32(dtype: crate::gguf::DType, raw: &[u8]) -> Vec<f32> {
    use crate::gguf::DType;
    match dtype {
        DType::F32 => raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect(),
        DType::F16 => raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| f16_to_f32(u16::from_le_bytes(*b)))
            .collect(),
        DType::BF16 => raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| bf16_to_f32(u16::from_le_bytes(*b)))
            .collect(),
        DType::Q8_0 => raw
            .as_chunks::<34>()
            .0
            .iter()
            .flat_map(|blk| {
                let d = f16_to_f32(u16::from_le_bytes([blk[0], blk[1]]));
                blk[2..].iter().map(move |q| d * f32::from(*q as i8))
            })
            .collect(),
    }
}

/// A vector quantized to Q8_0-style blocks (f32 scales, for speed).
pub struct QVec {
    pub scales: Vec<f32>,
    pub q: Vec<[i8; 32]>,
}

impl QVec {
    /// `x.len()` must be a multiple of 32.
    pub fn quantize(x: &[f32]) -> QVec {
        let mut scales = Vec::with_capacity(x.len() / 32);
        let mut q = Vec::with_capacity(x.len() / 32);
        for blk in x.chunks(32) {
            let amax = blk.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            let d = amax / 127.0;
            // As llama.cpp's quantize_row_q8_0: quantize with the exact
            // scale, store the scale rounded to f16.
            let inv = if d > 0.0 { 1.0 / d } else { 0.0 };
            scales.push(f16_to_f32(f32_to_f16(d)));
            let mut b = [0i8; 32];
            for (o, v) in b.iter_mut().zip(blk) {
                *o = (v * inv).round().clamp(-127.0, 127.0) as i8;
            }
            q.push(b);
        }
        QVec { scales, q }
    }
}

/// 32 signed bytes times 32 signed bytes, summed. Fixed-size arrays and
/// no indexing: the compiler turns this into a few SIMD instructions.
#[inline(always)]
fn block_dot(w: &[u8; 32], x: &[i8; 32]) -> i32 {
    w.iter()
        .zip(x)
        .map(|(a, b)| i32::from(*a as i8) * i32::from(*b))
        .sum()
}

/// Dot product of one Q8_0 weight row (raw blocks) with a quantized vector.
#[inline]
pub fn dot_q8(row: &[u8], x: &QVec) -> f32 {
    let lut = f16_table();
    let mut sum = 0.0f32;
    for ((blk, xq), xd) in row.as_chunks::<34>().0.iter().zip(&x.q).zip(&x.scales) {
        let (d, w) = blk.split_first_chunk::<2>().expect("34 bytes");
        let w: &[u8; 32] = w.try_into().expect("32 bytes");
        sum += lut[usize::from(u16::from_le_bytes(*d))] * xd * block_dot(w, xq) as f32;
    }
    sum
}

/// `exp(x)` for a softmax term (`x <= 0`), but exactly 0 where it would
/// be subnormal (below ~1e-38): those weigh nothing, and arithmetic on
/// subnormal floats is up to a hundred times slower on most CPUs (it made
/// Whisper's encoder three times slower before this).
#[inline]
pub fn softmax_term(x: f32) -> f32 {
    if x < -80.0 {
        0.0
    } else {
        x.exp()
    }
}

pub fn dot_f32(a: &[f32], b: &[f32]) -> f32 {
    // Four accumulators: lets the compiler vectorize without reordering
    // one long dependency chain.
    let mut acc = [0.0f32; 8];
    let (ca, ra) = a.as_chunks::<8>();
    let (cb, rb) = b.as_chunks::<8>();
    for (x, y) in ca.iter().zip(cb) {
        for i in 0..8 {
            acc[i] += x[i] * y[i];
        }
    }
    let mut s: f32 = acc.iter().sum();
    for (x, y) in ra.iter().zip(rb) {
        s += x * y;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_floats_round_trip() {
        for x in [0.0f32, 1.0, -2.5, 0.333_333_34, 65504.0, 6.1e-5, 1e-7, -0.0] {
            let back = f16_to_f32(f32_to_f16(x));
            assert!((back - x).abs() <= x.abs() * 1e-3 + 1e-7, "{x} -> {back}");
        }
        assert_eq!(f32_to_f16(1.0), 0x3c00);
        assert_eq!(f32_to_f16(-2.0), 0xc000);
        assert_eq!(f16_to_f32(0x7bff), 65504.0);
        assert_eq!(f16_to_f32(0x0001), 5.960_464_5e-8, "smallest subnormal");
        assert!(f16_to_f32(f32_to_f16(1e6)).is_infinite());
    }

    #[test]
    fn q8_dot_matches_float_dot() {
        let w: Vec<f32> = (0..64)
            .map(|i| ((i * 37 % 19) as f32 - 9.0) / 7.0)
            .collect();
        let x: Vec<f32> = (0..64)
            .map(|i| ((i * 11 % 23) as f32 - 11.0) / 5.0)
            .collect();
        // Weights as Q8_0 blocks.
        let qw = QVec::quantize(&w);
        let mut raw = Vec::new();
        for (d, q) in qw.scales.iter().zip(&qw.q) {
            raw.extend_from_slice(&f32_to_f16(*d).to_le_bytes());
            raw.extend(q.iter().map(|v| *v as u8));
        }
        let exact = dot_f32(&w, &x);
        let approx = dot_q8(&raw, &QVec::quantize(&x));
        assert!(
            (exact - approx).abs() < 0.02 * exact.abs().max(1.0),
            "{exact} vs {approx}"
        );
    }
}
