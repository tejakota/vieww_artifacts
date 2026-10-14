//! FLAC — lossless audio, decoded and encoded.
//!
//! # Decoder
//!
//! Every part of the format a file can use: `STREAMINFO` (other metadata
//! blocks are skipped by length), fixed and variable block sizes, the
//! header's coded sample rates and sizes, UTF-8 frame numbers, all four
//! subframe types (constant, verbatim, fixed predictors of order 0–4, LPC of
//! order 1–32), wasted bits, Rice and Rice2 residual partitions with escape
//! codes, and the four channel assignments (independent, left/side,
//! side/right, mid/side). Each frame's CRC-8 header and CRC-16 body are
//! checked, and the decoded audio's MD5 is compared with the one STREAMINFO
//! carries — so a decode that returns `Ok` is bit-exact by the file's own
//! account.
//!
//! # Encoder
//!
//! [`encode`] writes a standard stream any decoder reads: fixed 4096-sample
//! blocks; per channel the cheapest of constant, verbatim, fixed orders 0–4
//! and LPC (Tukey-windowed autocorrelation, Levinson–Durbin, orders up to
//! 12, coefficients quantised to 15 bits); residuals Rice-coded with the
//! partition order and parameters searched exhaustively; and for stereo the
//! cheapest of the four channel assignments. STREAMINFO carries the true
//! MD5, total samples and frame-size bounds.

use super::bits::{crc16, crc8, MsbReader, MsbWriter};
use super::md5::Md5;
use super::{CodecError, Decoded};

/// What `STREAMINFO` says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamInfo {
    /// Minimum block size.
    pub min_block: u16,
    /// Maximum block size.
    pub max_block: u16,
    /// Sample rate.
    pub rate: u32,
    /// Channels.
    pub channels: u8,
    /// Bits per sample.
    pub bits: u8,
    /// Total inter-channel samples (0 = unknown).
    pub total: u64,
    /// MD5 of the decoded audio.
    pub md5: [u8; 16],
}

fn read_info(b: &[u8]) -> Result<StreamInfo, CodecError> {
    if b.len() < 34 {
        return Err(CodecError::Truncated);
    }
    let mut r = MsbReader::new(b);
    let t = |v: Option<u32>| v.ok_or(CodecError::Truncated);
    let min_block = u16::try_from(t(r.read(16))?).unwrap_or(0);
    let max_block = u16::try_from(t(r.read(16))?).unwrap_or(0);
    t(r.read(24))?;
    t(r.read(24))?;
    let rate = t(r.read(20))?;
    let channels = u8::try_from(t(r.read(3))? + 1).unwrap_or(1);
    let bits = u8::try_from(t(r.read(5))? + 1).unwrap_or(16);
    let hi = u64::from(t(r.read(4))?);
    let lo = u64::from(t(r.read(32))?);
    let mut md5 = [0u8; 16];
    md5.copy_from_slice(&b[18..34]);
    Ok(StreamInfo {
        min_block,
        max_block,
        rate,
        channels,
        bits,
        total: (hi << 32) | lo,
        md5,
    })
}

/// Decode a FLAC stream, verifying CRCs and the MD5.
///
/// # Errors
/// Any structural problem, CRC failure or MD5 mismatch.
pub fn decode(bytes: &[u8]) -> Result<Decoded, CodecError> {
    let (info, ints) = decode_ints(bytes)?;
    let scale = 1.0 / f64::from(1u32 << (info.bits - 1));
    #[expect(clippy::cast_possible_truncation, reason = "f32 output")]
    let data = ints
        .iter()
        .map(|v| (f64::from(*v) * scale) as f32)
        .collect();
    Ok(Decoded {
        data,
        channels: info.channels,
        rate: info.rate,
    })
}

/// Decode to interleaved integers at the stream's own bit depth.
///
/// # Errors
/// As [`decode`].
pub fn decode_ints(bytes: &[u8]) -> Result<(StreamInfo, Vec<i32>), CodecError> {
    if !bytes.starts_with(b"fLaC") {
        return Err(CodecError::NotThisFormat);
    }
    let mut at = 4;
    let mut info = None;
    loop {
        if at + 4 > bytes.len() {
            return Err(CodecError::Truncated);
        }
        let last = bytes[at] & 0x80 != 0;
        let kind = bytes[at] & 0x7F;
        let len = (usize::from(bytes[at + 1]) << 16)
            | (usize::from(bytes[at + 2]) << 8)
            | usize::from(bytes[at + 3]);
        let body = bytes
            .get(at + 4..at + 4 + len)
            .ok_or(CodecError::Truncated)?;
        if kind == 0 {
            info = Some(read_info(body)?);
        }
        at += 4 + len;
        if last {
            break;
        }
    }
    let info = info.ok_or(CodecError::Malformed("no STREAMINFO"))?;
    let ch = usize::from(info.channels);
    let mut out = Vec::with_capacity(usize::try_from(info.total).unwrap_or(0) * ch);
    let mut md5 = Md5::default();
    let bps_bytes = usize::from(info.bits).div_ceil(8);
    while at + 2 <= bytes.len() {
        if !(bytes[at] == 0xFF && bytes[at + 1] & 0xFE == 0xF8) {
            at += 1; // resync (garbage between frames)
            continue;
        }
        let (block, used) = decode_frame(&bytes[at..], &info)?;
        let n = block.first().map_or(0, Vec::len);
        for i in 0..n {
            for c in &block {
                let v = c[i];
                out.push(v);
                md5.update(&v.to_le_bytes()[..bps_bytes]);
            }
        }
        at += used;
    }
    if info.total > 0 {
        out.truncate(usize::try_from(info.total).unwrap_or(usize::MAX) * ch);
    }
    if info.md5 != [0; 16] && md5.finish() != info.md5 {
        return Err(CodecError::Checksum("FLAC MD5 of decoded audio"));
    }
    Ok((info, out))
}

fn utf8_number(r: &mut MsbReader<'_>) -> Option<u64> {
    let first = r.read(8)?;
    let extra = u8::try_from(first).ok()?.leading_ones();
    if extra == 1 {
        return None;
    }
    if extra == 0 {
        return Some(u64::from(first));
    }
    let mut v = u64::from(first & (0x7F >> extra));
    for _ in 1..extra {
        let b = r.read(8)?;
        if b & 0xC0 != 0x80 {
            return None;
        }
        v = (v << 6) | u64::from(b & 0x3F);
    }
    Some(v)
}

fn decode_frame(b: &[u8], info: &StreamInfo) -> Result<(Vec<Vec<i32>>, usize), CodecError> {
    let mut r = MsbReader::new(b);
    let t = |v: Option<u32>| v.ok_or(CodecError::Truncated);
    t(r.read(15))?; // sync + reserved
    t(r.read(1))?; // blocking strategy
    let bs_code = t(r.read(4))?;
    let sr_code = t(r.read(4))?;
    let ch_code = t(r.read(4))?;
    let sz_code = t(r.read(3))?;
    t(r.read(1))?;
    utf8_number(&mut r).ok_or(CodecError::Malformed("frame number"))?;
    let block = match bs_code {
        1 => 192,
        2..=5 => 576 << (bs_code - 2),
        6 => t(r.read(8))? as usize + 1,
        7 => t(r.read(16))? as usize + 1,
        8..=15 => 256 << (bs_code - 8),
        _ => return Err(CodecError::Malformed("block size code")),
    };
    match sr_code {
        12 => {
            t(r.read(8))?;
        }
        13 | 14 => {
            t(r.read(16))?;
        }
        15 => return Err(CodecError::Malformed("sample rate code")),
        _ => {}
    }
    let bits = match sz_code {
        0 => u32::from(info.bits),
        1 => 8,
        2 => 12,
        4 => 16,
        5 => 20,
        6 => 24,
        7 => 32,
        _ => return Err(CodecError::Malformed("sample size code")),
    };
    let header_end = r.position() / 8;
    let crc = t(r.read(8))?;
    if crc8(&b[..header_end]) != u8::try_from(crc).unwrap_or(0) {
        return Err(CodecError::Checksum("FLAC frame header CRC-8"));
    }
    let (channels, mode) = match ch_code {
        0..=7 => (ch_code as usize + 1, 0),
        8..=10 => (2, ch_code),
        _ => return Err(CodecError::Malformed("channel assignment")),
    };
    let mut chans = Vec::with_capacity(channels);
    for c in 0..channels {
        let extra = u32::from(matches!((mode, c), (8 | 10, 1) | (9, 0)));
        chans.push(subframe(&mut r, block, bits + extra)?);
    }
    r.align();
    let body_end = r.position() / 8;
    let crc = t(r.read(16))?;
    if crc16(&b[..body_end]) != u16::try_from(crc).unwrap_or(0) {
        return Err(CodecError::Checksum("FLAC frame CRC-16"));
    }
    match mode {
        8 => {
            let (a, b) = chans.split_at_mut(1);
            for (l, s) in a[0].iter().zip(b[0].iter_mut()) {
                *s = l - *s;
            }
        }
        9 => {
            let (a, b) = chans.split_at_mut(1);
            for (s, r) in a[0].iter_mut().zip(&b[0]) {
                *s += r;
            }
        }
        10 => {
            let (a, b) = chans.split_at_mut(1);
            for (m, s) in a[0].iter_mut().zip(b[0].iter_mut()) {
                let side = *s;
                let mid = (*m << 1) | (side & 1);
                *m = (mid + side) >> 1;
                *s = (mid - side) >> 1;
            }
        }
        _ => {}
    }
    Ok((chans, r.position() / 8))
}

fn subframe(r: &mut MsbReader<'_>, block: usize, bits: u32) -> Result<Vec<i32>, CodecError> {
    let t = |v: Option<u32>| v.ok_or(CodecError::Truncated);
    t(r.read(1))?;
    let kind = t(r.read(6))?;
    let wasted = if t(r.read(1))? == 1 {
        r.unary().ok_or(CodecError::Truncated)? + 1
    } else {
        0
    };
    let bits = bits - wasted;
    let mut s = match kind {
        0 => {
            let v = r.read_signed(bits).ok_or(CodecError::Truncated)?;
            vec![v; block]
        }
        1 => (0..block)
            .map(|_| r.read_signed(bits).ok_or(CodecError::Truncated))
            .collect::<Result<_, _>>()?,
        8..=12 => {
            let order = (kind - 8) as usize;
            let mut s: Vec<i32> = (0..order)
                .map(|_| r.read_signed(bits).ok_or(CodecError::Truncated))
                .collect::<Result<_, _>>()?;
            residual(r, block, order, &mut s)?;
            restore_fixed(&mut s, order);
            s
        }
        32..=63 => {
            let order = (kind - 31) as usize;
            let mut s: Vec<i32> = (0..order)
                .map(|_| r.read_signed(bits).ok_or(CodecError::Truncated))
                .collect::<Result<_, _>>()?;
            let precision = t(r.read(4))? + 1;
            if precision == 16 {
                return Err(CodecError::Malformed("LPC precision"));
            }
            let shift = r.read_signed(5).ok_or(CodecError::Truncated)?;
            let coefs: Vec<i64> = (0..order)
                .map(|_| {
                    r.read_signed(precision)
                        .map(i64::from)
                        .ok_or(CodecError::Truncated)
                })
                .collect::<Result<_, _>>()?;
            residual(r, block, order, &mut s)?;
            let shift = u32::try_from(shift.max(0)).unwrap_or(0);
            for i in order..block {
                let mut acc: i64 = 0;
                for (j, c) in coefs.iter().enumerate() {
                    acc += c * i64::from(s[i - 1 - j]);
                }
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "the format bounds sample width"
                )]
                {
                    s[i] = s[i].wrapping_add((acc >> shift) as i32);
                }
            }
            s
        }
        _ => return Err(CodecError::Malformed("subframe type")),
    };
    if wasted > 0 {
        for v in &mut s {
            *v <<= wasted;
        }
    }
    Ok(s)
}

/// Read Rice-coded residuals, appending after the warm-up samples in `s`.
fn residual(
    r: &mut MsbReader<'_>,
    block: usize,
    order: usize,
    s: &mut Vec<i32>,
) -> Result<(), CodecError> {
    let t = |v: Option<u32>| v.ok_or(CodecError::Truncated);
    let method = t(r.read(2))?;
    let (pbits, escape) = match method {
        0 => (4, 15),
        1 => (5, 31),
        _ => return Err(CodecError::Malformed("residual method")),
    };
    let porder = t(r.read(4))?;
    let parts = 1usize << porder;
    let per = block >> porder;
    for p in 0..parts {
        let count = if p == 0 {
            per.checked_sub(order)
                .ok_or(CodecError::Malformed("partition"))?
        } else {
            per
        };
        let k = t(r.read(pbits))?;
        if k == escape {
            let raw = t(r.read(5))?;
            for _ in 0..count {
                s.push(r.read_signed(raw).ok_or(CodecError::Truncated)?);
            }
        } else {
            for _ in 0..count {
                let q = r.unary().ok_or(CodecError::Truncated)?;
                let low = t(r.read(k))?;
                let u = (q << k) | low;
                s.push(((u >> 1) as i32) ^ -((u & 1) as i32));
            }
        }
    }
    if s.len() != block {
        return Err(CodecError::Malformed("residual count"));
    }
    Ok(())
}

fn restore_fixed(s: &mut [i32], order: usize) {
    for i in order..s.len() {
        let p = match order {
            0 => 0,
            1 => s[i - 1],
            2 => 2 * s[i - 1] - s[i - 2],
            3 => 3 * s[i - 1] - 3 * s[i - 2] + s[i - 3],
            _ => 4 * s[i - 1] - 6 * s[i - 2] + 4 * s[i - 3] - s[i - 4],
        };
        s[i] = s[i].wrapping_add(p);
    }
}

// ───────────────────────────── encoder ─────────────────────────────

const BLOCK: usize = 4096;
const MAX_LPC: usize = 12;
const QLP_PRECISION: u32 = 15;

fn rice_bits(res: &[i64], k: u32) -> u64 {
    res.iter()
        .map(|v| {
            let u = ((v << 1) ^ (v >> 63)).cast_unsigned();
            u64::from(k) + 1 + (u >> k)
        })
        .sum()
}

/// Best (bits, partition order, params) for a residual of a block whose
/// first `order` samples are warm-up. Parameters run to 30; any above 14
/// switches the residual to the 5-bit-parameter method (Rice2).
fn best_partition(res: &[i64], block: usize, order: usize) -> (u64, u32, Vec<u32>) {
    let mut best = (u64::MAX, 0, Vec::new());
    for porder in 0..=8u32 {
        let parts = 1usize << porder;
        if !block.is_multiple_of(parts) || (block >> porder) <= order {
            break;
        }
        let per = block >> porder;
        let mut total = 6u64;
        let mut params = Vec::with_capacity(parts);
        let mut start = 0;
        for p in 0..parts {
            let n = if p == 0 { per - order } else { per };
            let seg = &res[start..start + n];
            start += n;
            let (mut kb, mut kbits) = (0, u64::MAX);
            for k in 0..=30 {
                let b = rice_bits(seg, k);
                if b < kbits {
                    kbits = b;
                    kb = k;
                }
            }
            total += kbits;
            params.push(kb);
        }
        let pbits = if params.iter().any(|k| *k > 14) { 5 } else { 4 };
        total += pbits * parts as u64;
        if total < best.0 {
            best = (total, porder, params);
        }
    }
    best
}

fn fixed_residual(x: &[i64], order: usize) -> Vec<i64> {
    (order..x.len())
        .map(|i| {
            let p = match order {
                0 => 0,
                1 => x[i - 1],
                2 => 2 * x[i - 1] - x[i - 2],
                3 => 3 * x[i - 1] - 3 * x[i - 2] + x[i - 3],
                _ => 4 * x[i - 1] - 6 * x[i - 2] + 4 * x[i - 3] - x[i - 4],
            };
            x[i] - p
        })
        .collect()
}

/// Levinson–Durbin on a Tukey(0.5)-windowed autocorrelation; returns LPC
/// coefficient sets for every order 1..=max.
fn lpc_sets(x: &[i64], max: usize) -> Vec<Vec<f64>> {
    let n = x.len();
    if n <= max {
        return Vec::new();
    }
    #[expect(clippy::cast_precision_loss, reason = "block sizes")]
    let w: Vec<f64> = (0..n)
        .map(|i| {
            let a = 0.5;
            let r = i as f64 / (n - 1) as f64;
            if r < a / 2.0 {
                0.5 * (1.0 - (std::f64::consts::TAU * r / a).cos())
            } else if r > 1.0 - a / 2.0 {
                0.5 * (1.0 - (std::f64::consts::TAU * (1.0 - r) / a).cos())
            } else {
                1.0
            }
        })
        .collect();
    #[expect(clippy::cast_precision_loss, reason = "sample values")]
    let xw: Vec<f64> = x.iter().zip(&w).map(|(v, w)| *v as f64 * w).collect();
    let ac: Vec<f64> = (0..=max)
        .map(|lag| (lag..n).map(|i| xw[i] * xw[i - lag]).sum())
        .collect();
    if ac[0] <= 0.0 {
        return Vec::new();
    }
    let mut sets = Vec::new();
    let mut a = vec![0.0f64; max + 1];
    let mut err = ac[0];
    for i in 1..=max {
        let mut acc = ac[i];
        for j in 1..i {
            acc -= a[j] * ac[i - j];
        }
        let k = acc / err;
        let prev = a.clone();
        a[i] = k;
        for j in 1..i {
            a[j] = prev[j] - k * prev[i - j];
        }
        err *= 1.0 - k * k;
        if err <= 0.0 {
            break;
        }
        sets.push(a[1..=i].to_vec());
    }
    sets
}

/// Quantise LPC coefficients: (q coefs, shift).
fn quantise(c: &[f64]) -> (Vec<i64>, i32) {
    let cmax = c.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    if cmax == 0.0 {
        return (vec![0; c.len()], 0);
    }
    #[expect(clippy::cast_possible_truncation, reason = "a small exponent")]
    let log2 = cmax.log2().floor() as i32 + 1;
    let shift = (i32::try_from(QLP_PRECISION).unwrap_or(15) - 1 - log2).clamp(0, 15);
    let lim = (1i64 << (QLP_PRECISION - 1)) - 1;
    let mut err = 0.0;
    let q = c
        .iter()
        .map(|v| {
            let s = v * f64::from(1u32 << shift) + err;
            #[expect(clippy::cast_possible_truncation, reason = "clamped to precision")]
            let r = (s.round() as i64).clamp(-lim - 1, lim);
            #[expect(clippy::cast_precision_loss, reason = "small")]
            {
                err = s - r as f64;
            }
            r
        })
        .collect();
    (q, shift)
}

fn lpc_residual(x: &[i64], q: &[i64], shift: i32) -> Option<Vec<i64>> {
    let order = q.len();
    let mut out = Vec::with_capacity(x.len() - order);
    for i in order..x.len() {
        let mut acc: i64 = 0;
        for (j, c) in q.iter().enumerate() {
            acc = acc.checked_add(c.checked_mul(x[i - 1 - j])?)?;
        }
        out.push(x[i] - (acc >> shift));
    }
    Some(out)
}

/// A candidate channel assignment: (code, the channels and their widths).
type Assignment = (u64, Vec<(Vec<i64>, u32)>);
/// The cheapest plan found: (bits, code, planned subframes).
type Planned = (u64, u64, Vec<(Vec<i64>, u32, Sub)>);

enum Sub {
    Constant(i64),
    Verbatim,
    Fixed(usize, Vec<i64>, u32, Vec<u32>),
    Lpc(Vec<i64>, i32, Vec<i64>, u32, Vec<u32>),
}

/// Choose the cheapest subframe; returns (bits, plan).
fn plan_subframe(x: &[i64], bits: u32) -> (u64, Sub) {
    let n = x.len();
    if x.iter().all(|v| *v == x[0]) {
        return (8 + u64::from(bits), Sub::Constant(x[0]));
    }
    let mut best = (8 + u64::from(bits) * n as u64, Sub::Verbatim);
    for order in 0..=4.min(n - 1) {
        let res = fixed_residual(x, order);
        let (rb, po, params) = best_partition(&res, n, order);
        let total = 8 + u64::from(bits) * order as u64 + 2 + 4 + rb;
        if total < best.0 {
            best = (total, Sub::Fixed(order, res, po, params));
        }
    }
    for c in lpc_sets(x, MAX_LPC.min(n.saturating_sub(1))) {
        let (q, shift) = quantise(&c);
        let Some(res) = lpc_residual(x, &q, shift) else {
            continue;
        };
        if res.iter().any(|v| v.unsigned_abs() > (1 << 30)) {
            continue;
        }
        let order = q.len();
        let (rb, po, params) = best_partition(&res, n, order);
        let total = 8
            + u64::from(bits) * order as u64
            + 4
            + 5
            + u64::from(QLP_PRECISION) * order as u64
            + 2
            + 4
            + rb;
        if total < best.0 {
            best = (total, Sub::Lpc(q, shift, res, po, params));
        }
    }
    best
}

fn write_residual(w: &mut MsbWriter, res: &[i64], n: usize, order: usize, po: u32, params: &[u32]) {
    let rice2 = params.iter().any(|k| *k > 14);
    w.write(u64::from(rice2), 2);
    w.write(u64::from(po), 4);
    let pbits = if rice2 { 5 } else { 4 };
    let per = n >> po;
    let mut start = 0;
    for (p, k) in params.iter().enumerate() {
        let cnt = if p == 0 { per - order } else { per };
        w.write(u64::from(*k), pbits);
        for v in &res[start..start + cnt] {
            let u = ((v << 1) ^ (v >> 63)).cast_unsigned();
            let q = u >> k;
            for _ in 0..q {
                w.write(0, 1);
            }
            w.write(1, 1);
            w.write(u & ((1u64 << k) - 1), *k);
        }
        start += cnt;
    }
}

fn write_subframe(w: &mut MsbWriter, x: &[i64], bits: u32, plan: &Sub) {
    w.write(0, 1);
    match plan {
        Sub::Constant(v) => {
            w.write(0, 6);
            w.write(0, 1);
            w.write_signed(*v, bits);
        }
        Sub::Verbatim => {
            w.write(1, 6);
            w.write(0, 1);
            for v in x {
                w.write_signed(*v, bits);
            }
        }
        Sub::Fixed(order, res, po, params) => {
            w.write(8 + *order as u64, 6);
            w.write(0, 1);
            for v in &x[..*order] {
                w.write_signed(*v, bits);
            }
            write_residual(w, res, x.len(), *order, *po, params);
        }
        Sub::Lpc(q, shift, res, po, params) => {
            let order = q.len();
            w.write(31 + order as u64, 6);
            w.write(0, 1);
            for v in &x[..order] {
                w.write_signed(*v, bits);
            }
            w.write(u64::from(QLP_PRECISION - 1), 4);
            w.write_signed(i64::from(*shift), 5);
            for c in q {
                w.write_signed(*c, QLP_PRECISION);
            }
            write_residual(w, res, x.len(), order, *po, params);
        }
    }
}

fn write_utf8(w: &mut MsbWriter, v: u64) {
    if v < 0x80 {
        w.write(v, 8);
        return;
    }
    let mut bytes = 2;
    while v >= 1u64 << (5 * bytes + 1) {
        bytes += 1;
    }
    let lead_bits = 7 - bytes;
    let first = (0xFFu64 << (8 - bytes)) & 0xFF;
    w.write(first | (v >> (6 * (bytes - 1))) & ((1 << lead_bits) - 1), 8);
    for i in (0..bytes - 1).rev() {
        w.write(0x80 | ((v >> (6 * i)) & 0x3F), 8);
    }
}

/// Encode interleaved integers (`bits` per sample, 4..=24) to FLAC.
///
/// # Errors
/// Unsupported shapes (0 or >8 channels, out-of-range depth or rate).
pub fn encode_ints(data: &[i32], channels: u8, rate: u32, bits: u8) -> Result<Vec<u8>, CodecError> {
    if channels == 0 || channels > 8 || !(4..=24).contains(&bits) || rate == 0 || rate >= 1 << 20 {
        return Err(CodecError::Unsupported("FLAC stream shape"));
    }
    let ch = usize::from(channels);
    let frames = data.len() / ch;
    let mut md5 = Md5::default();
    let bps_bytes = usize::from(bits).div_ceil(8);
    for v in &data[..frames * ch] {
        md5.update(&v.to_le_bytes()[..bps_bytes]);
    }
    let mut out = b"fLaC".to_vec();
    let info_at = out.len();
    out.extend([0x80u8, 0, 0, 34]);
    out.extend([0u8; 34]);
    let (mut min_f, mut max_f) = (u32::MAX, 0u32);
    let b = u32::from(bits);
    let size_code: u64 = match bits {
        8 => 1,
        12 => 2,
        16 => 4,
        20 => 5,
        24 => 6,
        _ => 0,
    };
    for (fi, start) in (0..frames).step_by(BLOCK).enumerate() {
        let n = BLOCK.min(frames - start);
        let chans: Vec<Vec<i64>> = (0..ch)
            .map(|c| {
                (0..n)
                    .map(|i| i64::from(data[(start + i) * ch + c]))
                    .collect()
            })
            .collect();
        // Choose the channel assignment.
        let mut options: Vec<Assignment> = Vec::new();
        let indep: Vec<(Vec<i64>, u32)> = chans.iter().map(|c| (c.clone(), b)).collect();
        options.push((ch as u64 - 1, indep));
        if ch == 2 {
            let (l, r) = (&chans[0], &chans[1]);
            let side: Vec<i64> = l.iter().zip(r).map(|(a, b)| a - b).collect();
            let mid: Vec<i64> = l.iter().zip(r).map(|(a, b)| (a + b) >> 1).collect();
            options.push((8, vec![(l.clone(), b), (side.clone(), b + 1)]));
            options.push((9, vec![(side.clone(), b + 1), (r.clone(), b)]));
            options.push((10, vec![(mid, b), (side, b + 1)]));
        }
        let mut best: Option<Planned> = None;
        for (code, subs) in options {
            let mut total = 0;
            let mut planned = Vec::new();
            for (x, bb) in subs {
                let (bits_needed, plan) = plan_subframe(&x, bb);
                total += bits_needed;
                planned.push((x, bb, plan));
            }
            if best.as_ref().is_none_or(|b| total < b.0) {
                best = Some((total, code, planned));
            }
        }
        let Some((_, code, planned)) = best else {
            continue;
        };
        let mut w = MsbWriter::default();
        w.write(0x3FFE, 14); // the 14-bit frame sync
        w.write(0, 1);
        w.write(0, 1); // fixed blocksize
        let (bs_code, bs_extra) = if n == BLOCK {
            (12u64, None)
        } else {
            (7, Some(n - 1))
        };
        w.write(bs_code, 4);
        w.write(0, 4); // rate from STREAMINFO
        w.write(code, 4);
        w.write(size_code, 3);
        w.write(0, 1);
        write_utf8(&mut w, fi as u64);
        if let Some(e) = bs_extra {
            w.write(e as u64, 16);
        }
        let hdr_crc = crc8(&w.bytes);
        w.write(u64::from(hdr_crc), 8);
        for (x, bb, plan) in &planned {
            write_subframe(&mut w, x, *bb, plan);
        }
        w.align();
        let c = crc16(&w.bytes);
        w.write(u64::from(c), 16);
        let len = u32::try_from(w.bytes.len()).unwrap_or(u32::MAX);
        min_f = min_f.min(len);
        max_f = max_f.max(len);
        out.extend(&w.bytes);
    }
    let mut si = MsbWriter::default();
    let blk = u64::try_from(BLOCK.min(frames.max(16))).unwrap_or(4096);
    si.write(blk, 16);
    si.write(blk, 16);
    si.write(u64::from(if min_f == u32::MAX { 0 } else { min_f }), 24);
    si.write(u64::from(max_f), 24);
    si.write(u64::from(rate), 20);
    si.write(u64::from(channels - 1), 3);
    si.write(u64::from(bits - 1), 5);
    si.write(frames as u64, 36);
    for byte in md5.finish() {
        si.write(u64::from(byte), 8);
    }
    out[info_at + 4..info_at + 38].copy_from_slice(&si.bytes);
    Ok(out)
}

/// Encode float audio at `bits` (16 or 24).
///
/// # Errors
/// As [`encode_ints`].
pub fn encode(d: &Decoded, bits: u8) -> Result<Vec<u8>, CodecError> {
    let max = f64::from((1u32 << (bits.clamp(4, 24) - 1)) - 1);
    #[expect(clippy::cast_possible_truncation, reason = "clamped to the depth")]
    let ints: Vec<i32> = d
        .data
        .iter()
        .map(|v| (f64::from(v.clamp(-1.0, 1.0)) * max).round() as i32)
        .collect();
    encode_ints(&ints, d.channels, d.rate, bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav_ints(bytes: &[u8], bits: u16) -> Vec<i32> {
        // Read PCM straight from the data chunk (16- or 24-bit LE).
        let at = bytes.windows(4).position(|w| w == b"data").unwrap() + 8;
        let w = usize::from(bits / 8);
        bytes[at..]
            .chunks_exact(w)
            .map(|c| {
                if w == 2 {
                    i32::from(i16::from_le_bytes([c[0], c[1]]))
                } else {
                    i32::from_le_bytes([0, c[0], c[1], c[2]]) >> 8
                }
            })
            .collect()
    }

    #[test]
    fn decodes_ffmpegs_16_bit_stereo_bit_exactly() {
        let (info, got) = decode_ints(include_bytes!("../../tests/fixtures/src16.flac")).unwrap();
        assert_eq!((info.channels, info.bits, info.rate), (2, 16, 44_100));
        let want = wav_ints(include_bytes!("../../tests/fixtures/source16.wav"), 16);
        assert_eq!(got, want);
    }

    #[test]
    fn decodes_ffmpegs_24_bit_mono_bit_exactly() {
        let (info, got) = decode_ints(include_bytes!("../../tests/fixtures/src24m.flac")).unwrap();
        assert_eq!((info.channels, info.bits), (1, 24));
        let want = wav_ints(include_bytes!("../../tests/fixtures/source24m.wav"), 24);
        assert_eq!(got, want);
    }

    #[test]
    fn a_flipped_bit_fails_a_checksum() {
        let mut f = include_bytes!("../../tests/fixtures/src16.flac").to_vec();
        let n = f.len();
        f[n / 2] ^= 0x10;
        assert!(matches!(decode(&f), Err(CodecError::Checksum(_))));
    }

    #[test]
    fn encoder_round_trips_and_compresses() {
        let want = wav_ints(include_bytes!("../../tests/fixtures/source16.wav"), 16);
        let enc = encode_ints(&want, 2, 44_100, 16).unwrap();
        let (info, got) = decode_ints(&enc).unwrap();
        assert_eq!(got, want, "lossless");
        assert_eq!(info.total, 17_640);
        assert!(
            enc.len() < want.len() * 2 * 7 / 10,
            "compresses: {} of {}",
            enc.len(),
            want.len() * 2
        );
        assert!(
            enc.len() < include_bytes!("../../tests/fixtures/src16.flac").len() * 11 / 10,
            "close to ffmpeg -8"
        );
    }

    #[test]
    fn encoder_handles_edge_shapes() {
        // Silence (constant), a short block, 24-bit, mono, 6 channels.
        for (ch, bits, n) in [
            (1u8, 16u8, 10usize),
            (1, 24, 5000),
            (6, 16, 4097),
            (2, 8, 300),
        ] {
            let mut r = crate::rng::Rng::new(u64::from(ch) * 7 + u64::from(bits));
            let lim = (1i64 << (bits - 1)) - 1;
            let data: Vec<i32> = (0..n * usize::from(ch))
                .map(|i| {
                    if i < 50 {
                        0
                    } else {
                        i32::try_from((r.range(-1.0, 1.0) * lim as f64 * 0.3) as i64).unwrap()
                    }
                })
                .collect();
            let enc = encode_ints(&data, ch, 48_000, bits).unwrap();
            let (_, got) = decode_ints(&enc).unwrap();
            assert_eq!(got, data, "{ch}ch {bits}bit {n}");
        }
    }

    #[test]
    fn ffmpeg_reads_our_files_when_present() {
        // Cross-check with an independent decoder where one is installed.
        let Ok(out) = std::process::Command::new("ffmpeg")
            .arg("-version")
            .output()
        else {
            return;
        };
        if !out.status.success() {
            return;
        }
        let want = wav_ints(include_bytes!("../../tests/fixtures/source16.wav"), 16);
        let enc = encode_ints(&want, 2, 44_100, 16).unwrap();
        let dir = std::env::temp_dir().join(format!("vieww-flac-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ours.flac");
        std::fs::write(&path, &enc).unwrap();
        let dec = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-i"])
            .arg(&path)
            .args(["-f", "s16le", "-"])
            .output()
            .unwrap();
        assert!(
            dec.status.success(),
            "{}",
            String::from_utf8_lossy(&dec.stderr)
        );
        let got: Vec<i32> = dec
            .stdout
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| i32::from(i16::from_le_bytes([c[0], c[1]])))
            .collect();
        assert_eq!(got, want, "ffmpeg decodes our FLAC bit-exactly");
        let _ = std::fs::remove_dir_all(dir);
    }
}
