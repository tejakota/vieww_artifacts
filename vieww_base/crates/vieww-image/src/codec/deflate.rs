//! DEFLATE (RFC 1951) and the zlib wrapper (RFC 1950), from the spec.
//!
//! * [`inflate`] decodes all three block types — stored, fixed Huffman and
//!   dynamic Huffman — with canonical-code tables, length/distance
//!   extra bits, and overlapping back-references.
//! * [`deflate`] encodes with LZ77 over 32 KiB windows (hash chains, lazy
//!   matching one step deep) and **dynamic** Huffman codes built per block
//!   with length-limited package-merge-free heuristics (a heap-built tree,
//!   lengths clamped to 15 and re-balanced), falling back to stored blocks
//!   when that is smaller.
//! * [`zlib_compress`]/[`zlib_decompress`] add the two-byte header and the
//!   Adler-32 trailer; [`crc32`] is the IEEE CRC PNG chunks need.

use std::fmt;

/// Why a stream did not decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InflateError(pub &'static str);

impl fmt::Display for InflateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "inflate: {}", self.0)
    }
}

impl std::error::Error for InflateError {}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
const CL_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    bit: u32,
    nbits: u32,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bit: 0,
            nbits: 0,
        }
    }
    fn need(&mut self, n: u32) -> Result<(), InflateError> {
        while self.nbits < n {
            let b = *self
                .data
                .get(self.pos)
                .ok_or(InflateError("unexpected end of data"))?;
            self.pos += 1;
            self.bit |= u32::from(b) << self.nbits;
            self.nbits += 8;
        }
        Ok(())
    }
    fn get(&mut self, n: u32) -> Result<u32, InflateError> {
        if n == 0 {
            return Ok(0);
        }
        self.need(n)?;
        let v = self.bit & ((1u32 << n) - 1);
        self.bit >>= n;
        self.nbits -= n;
        Ok(v)
    }
    fn align(&mut self) {
        self.bit = 0;
        self.nbits = 0;
    }
}

/// A canonical Huffman decoding table: counts per length and symbols in order.
struct Huff {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huff {
    fn new(lengths: &[u8]) -> Result<Self, InflateError> {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[usize::from(l)] += 1;
        }
        counts[0] = 0;
        let mut left: i32 = 1;
        for &c in &counts[1..] {
            left = (left << 1) - i32::from(c);
            if left < 0 {
                return Err(InflateError("over-subscribed code"));
            }
        }
        let mut offs = [0u16; 16];
        for i in 1..15 {
            offs[i + 1] = offs[i] + counts[i];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (s, &l) in lengths.iter().enumerate() {
            if l != 0 {
                #[allow(clippy::cast_possible_truncation)]
                {
                    symbols[usize::from(offs[usize::from(l)])] = s as u16;
                }
                offs[usize::from(l)] += 1;
            }
        }
        Ok(Self { counts, symbols })
    }

    fn decode(&self, b: &mut Bits<'_>) -> Result<u16, InflateError> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= b.get(1)? as i32;
            let count = i32::from(self.counts[len]);
            if code - count < first {
                #[allow(clippy::cast_sign_loss)]
                return Ok(self.symbols[(index + (code - first)) as usize]);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err(InflateError("invalid Huffman code"))
    }
}

fn fixed() -> (Huff, Huff) {
    let mut l = [0u8; 288];
    l[..144].fill(8);
    l[144..256].fill(9);
    l[256..280].fill(7);
    l[280..].fill(8);
    (
        Huff::new(&l).expect("fixed literal code"),
        Huff::new(&[5u8; 30]).expect("fixed distance code"),
    )
}

fn codes(b: &mut Bits<'_>, out: &mut Vec<u8>, lit: &Huff, dist: &Huff) -> Result<(), InflateError> {
    loop {
        let sym = lit.decode(b)?;
        match sym {
            0..=255 => out.push(sym as u8),
            256 => return Ok(()),
            257..=285 => {
                let i = usize::from(sym - 257);
                let len = usize::from(LEN_BASE[i]) + b.get(u32::from(LEN_EXTRA[i]))? as usize;
                let d = usize::from(dist.decode(b)?);
                if d >= 30 {
                    return Err(InflateError("bad distance symbol"));
                }
                let dist = usize::from(DIST_BASE[d]) + b.get(u32::from(DIST_EXTRA[d]))? as usize;
                if dist > out.len() {
                    return Err(InflateError("distance too far back"));
                }
                let start = out.len() - dist;
                for k in 0..len {
                    let v = out[start + k];
                    out.push(v);
                }
            }
            _ => return Err(InflateError("bad literal/length symbol")),
        }
    }
}

/// Decode a raw DEFLATE stream.
///
/// # Errors
/// On any malformed block.
pub fn inflate(data: &[u8]) -> Result<Vec<u8>, InflateError> {
    let mut b = Bits::new(data);
    let mut out = Vec::with_capacity(data.len() * 3);
    loop {
        let last = b.get(1)?;
        match b.get(2)? {
            0 => {
                b.align();
                let p = b.pos;
                if p + 4 > data.len() {
                    return Err(InflateError("stored header past end"));
                }
                let len = usize::from(u16::from_le_bytes([data[p], data[p + 1]]));
                let nlen = u16::from_le_bytes([data[p + 2], data[p + 3]]);
                #[allow(clippy::cast_possible_truncation)]
                if nlen != !(len as u16) {
                    return Err(InflateError("stored length check failed"));
                }
                let s = p + 4;
                out.extend_from_slice(
                    data.get(s..s + len)
                        .ok_or(InflateError("stored past end"))?,
                );
                b.pos = s + len;
            }
            1 => {
                let (l, d) = fixed();
                codes(&mut b, &mut out, &l, &d)?;
            }
            2 => {
                let hlit = b.get(5)? as usize + 257;
                let hdist = b.get(5)? as usize + 1;
                let hclen = b.get(4)? as usize + 4;
                let mut cl = [0u8; 19];
                for &o in CL_ORDER.iter().take(hclen) {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        cl[o] = b.get(3)? as u8;
                    }
                }
                let clh = Huff::new(&cl)?;
                let mut lens = Vec::with_capacity(hlit + hdist);
                while lens.len() < hlit + hdist {
                    let sym = clh.decode(&mut b)?;
                    match sym {
                        0..=15 => lens.push(sym as u8),
                        16 => {
                            let prev =
                                *lens.last().ok_or(InflateError("repeat with no previous"))?;
                            for _ in 0..3 + b.get(2)? {
                                lens.push(prev);
                            }
                        }
                        17 => lens.extend(std::iter::repeat_n(0, 3 + b.get(3)? as usize)),
                        _ => lens.extend(std::iter::repeat_n(0, 11 + b.get(7)? as usize)),
                    }
                }
                if lens.len() > hlit + hdist {
                    return Err(InflateError("code lengths overrun"));
                }
                let l = Huff::new(&lens[..hlit])?;
                let d = Huff::new(&lens[hlit..])?;
                codes(&mut b, &mut out, &l, &d)?;
            }
            _ => return Err(InflateError("reserved block type")),
        }
        if last == 1 {
            return Ok(out);
        }
    }
}

// ───────────────────────────── encoder ─────────────────────────────

struct BitWriter {
    out: Vec<u8>,
    bit: u64,
    n: u32,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            out: Vec::new(),
            bit: 0,
            n: 0,
        }
    }
    fn put(&mut self, v: u32, n: u32) {
        self.bit |= u64::from(v) << self.n;
        self.n += n;
        while self.n >= 8 {
            #[allow(clippy::cast_possible_truncation)]
            self.out.push(self.bit as u8);
            self.bit >>= 8;
            self.n -= 8;
        }
    }
    /// Huffman codes are sent MSB-first: reverse before `put`.
    fn code(&mut self, code: u16, len: u8) {
        let mut r = 0u32;
        for i in 0..len {
            r |= u32::from((code >> i) & 1) << (len - 1 - i);
        }
        self.put(r, u32::from(len));
    }
    fn flush(mut self) -> Vec<u8> {
        if self.n > 0 {
            #[allow(clippy::cast_possible_truncation)]
            self.out.push(self.bit as u8);
        }
        self.out
    }
}

#[derive(Clone, Copy)]
enum Tok {
    Lit(u8),
    Match(u16, u16),
}

fn lz77(data: &[u8]) -> Vec<Tok> {
    const WIN: usize = 32_768;
    const HBITS: u32 = 15;
    const CHAIN: usize = 48;
    let mut head = vec![usize::MAX; 1 << HBITS];
    let mut prev = vec![usize::MAX; data.len()];
    let hash = |i: usize| -> usize {
        let v = u32::from(data[i]) << 16 | u32::from(data[i + 1]) << 8 | u32::from(data[i + 2]);
        (v.wrapping_mul(0x9E37_79B1) >> (32 - HBITS)) as usize
    };
    let insert = |i: usize, head: &mut Vec<usize>, prev: &mut Vec<usize>| {
        if i + 2 < data.len() {
            let h = hash(i);
            prev[i] = head[h];
            head[h] = i;
        }
    };
    let best = |i: usize, head: &Vec<usize>, prev: &Vec<usize>| -> (usize, usize) {
        if i + 2 >= data.len() {
            return (0, 0);
        }
        let mut cand = head[hash(i)];
        let (mut bl, mut bd) = (0, 0);
        let max = (data.len() - i).min(258);
        let mut steps = 0;
        while cand != usize::MAX && i - cand <= WIN && steps < CHAIN {
            let mut l = 0;
            while l < max && data[cand + l] == data[i + l] {
                l += 1;
            }
            if l > bl {
                bl = l;
                bd = i - cand;
                if l == max {
                    break;
                }
            }
            cand = prev[cand];
            steps += 1;
        }
        (bl, bd)
    };
    let mut toks = Vec::with_capacity(data.len() / 2);
    let mut i = 0;
    while i < data.len() {
        let (l, d) = best(i, &head, &prev);
        if l >= 3 {
            // Lazy: does starting one later find something better?
            insert(i, &mut head, &mut prev);
            let (l2, _) = best(i + 1, &head, &prev);
            if l2 > l + 1 {
                toks.push(Tok::Lit(data[i]));
                i += 1;
                continue;
            }
            #[allow(clippy::cast_possible_truncation)]
            toks.push(Tok::Match(l as u16, d as u16));
            for k in 1..l {
                insert(i + k, &mut head, &mut prev);
            }
            i += l;
        } else {
            insert(i, &mut head, &mut prev);
            toks.push(Tok::Lit(data[i]));
            i += 1;
        }
    }
    toks
}

fn len_sym(l: u16) -> (usize, u32, u32) {
    let i = LEN_BASE.iter().rposition(|&b| b <= l).unwrap_or(0);
    (257 + i, u32::from(LEN_EXTRA[i]), u32::from(l - LEN_BASE[i]))
}
fn dist_sym(d: u16) -> (usize, u32, u32) {
    let i = DIST_BASE.iter().rposition(|&b| b <= d).unwrap_or(0);
    (i, u32::from(DIST_EXTRA[i]), u32::from(d - DIST_BASE[i]))
}

/// Code lengths for `freq`, limited to `limit` bits.
fn lengths(freq: &[u32], limit: u8) -> Vec<u8> {
    let n = freq.len();
    let mut lens = vec![0u8; n];
    let used: Vec<usize> = (0..n).filter(|&i| freq[i] > 0).collect();
    match used.len() {
        0 => return lens,
        1 => {
            lens[used[0]] = 1;
            return lens;
        }
        _ => {}
    }
    // Huffman by repeatedly merging the two lightest nodes; ties broken by
    // node index so the result is deterministic.
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    let mut parent: Vec<usize> = vec![usize::MAX; used.len()];
    let mut heap: BinaryHeap<Reverse<(u64, usize)>> = used
        .iter()
        .enumerate()
        .map(|(k, &i)| Reverse((u64::from(freq[i]), k)))
        .collect();
    while heap.len() > 1 {
        let Reverse((wa, a)) = heap.pop().expect("two nodes");
        let Reverse((wb, b)) = heap.pop().expect("two nodes");
        let node = parent.len();
        parent.push(usize::MAX);
        parent[a] = node;
        parent[b] = node;
        heap.push(Reverse((wa + wb, node)));
    }
    for (k, &sym) in used.iter().enumerate() {
        let (mut d, mut x) = (0u8, k);
        while parent[x] != usize::MAX {
            d = d.saturating_add(1);
            x = parent[x];
        }
        lens[sym] = d.max(1);
    }
    // Length-limit: clamp, then repair the Kraft sum by lengthening the
    // shortest codes of the rarest symbols.
    if lens.iter().any(|&l| l > limit) {
        for l in &mut lens {
            if *l > limit {
                *l = limit;
            }
        }
        let kraft = |lens: &[u8]| -> u64 {
            lens.iter()
                .filter(|&&l| l > 0)
                .map(|&l| 1u64 << (limit - l))
                .sum()
        };
        let cap = 1u64 << limit;
        let mut order: Vec<usize> = (0..n).filter(|&i| lens[i] > 0).collect();
        order.sort_by_key(|&i| (freq[i], std::cmp::Reverse(lens[i])));
        while kraft(&lens) > cap {
            for &i in &order {
                if lens[i] < limit {
                    lens[i] += 1;
                    break;
                }
            }
        }
    }
    lens
}

/// Canonical codes from lengths.
fn canonical(lens: &[u8]) -> Vec<u16> {
    let mut bl = [0u16; 16];
    for &l in lens {
        bl[usize::from(l)] += 1;
    }
    bl[0] = 0;
    let mut next = [0u16; 16];
    let mut code = 0u16;
    for bits in 1..16 {
        code = (code + bl[bits - 1]) << 1;
        next[bits] = code;
    }
    lens.iter()
        .map(|&l| {
            if l == 0 {
                0
            } else {
                let c = next[usize::from(l)];
                next[usize::from(l)] += 1;
                c
            }
        })
        .collect()
}

fn write_block(w: &mut BitWriter, toks: &[Tok], last: bool) {
    let mut lf = vec![0u32; 286];
    let mut df = vec![0u32; 30];
    for t in toks {
        match *t {
            Tok::Lit(b) => lf[usize::from(b)] += 1,
            Tok::Match(l, d) => {
                lf[len_sym(l).0] += 1;
                df[dist_sym(d).0] += 1;
            }
        }
    }
    lf[256] = 1;
    if df.iter().all(|&f| f == 0) {
        df[0] = 1;
    }
    let ll = lengths(&lf, 15);
    let dl = lengths(&df, 15);
    let hlit = (257..=286)
        .rev()
        .find(|&n| ll[n - 1] != 0)
        .unwrap_or(257)
        .max(257);
    let hdist = (1..=30).rev().find(|&n| dl[n - 1] != 0).unwrap_or(1);
    // Run-length encode the code lengths.
    let all: Vec<u8> = ll[..hlit].iter().chain(&dl[..hdist]).copied().collect();
    let mut rle: Vec<(u8, u8)> = Vec::new(); // (symbol, extra)
    let mut i = 0;
    while i < all.len() {
        let v = all[i];
        let mut run = 1;
        while i + run < all.len() && all[i + run] == v {
            run += 1;
        }
        let mut r = run;
        if v == 0 {
            while r >= 11 {
                let k = r.min(138);
                #[allow(clippy::cast_possible_truncation)]
                rle.push((18, (k - 11) as u8));
                r -= k;
            }
            if r >= 3 {
                #[allow(clippy::cast_possible_truncation)]
                rle.push((17, (r - 3) as u8));
                r = 0;
            }
        } else {
            rle.push((v, 0));
            r -= 1;
            while r >= 3 {
                let k = r.min(6);
                #[allow(clippy::cast_possible_truncation)]
                rle.push((16, (k - 3) as u8));
                r -= k;
            }
        }
        for _ in 0..r {
            rle.push((v, 0));
        }
        i += run;
    }
    let mut cf = vec![0u32; 19];
    for (s, _) in &rle {
        cf[usize::from(*s)] += 1;
    }
    let cl = lengths(&cf, 7);
    let hclen = (4..=19)
        .rev()
        .find(|&n| cl[CL_ORDER[n - 1]] != 0)
        .unwrap_or(4);
    let (lc, dc, cc) = (canonical(&ll), canonical(&dl), canonical(&cl));

    w.put(u32::from(last), 1);
    w.put(2, 2);
    #[allow(clippy::cast_possible_truncation)]
    {
        w.put((hlit - 257) as u32, 5);
        w.put((hdist - 1) as u32, 5);
        w.put((hclen - 4) as u32, 4);
    }
    for &o in CL_ORDER.iter().take(hclen) {
        w.put(u32::from(cl[o]), 3);
    }
    for (s, e) in &rle {
        let s = usize::from(*s);
        w.code(cc[s], cl[s]);
        match s {
            16 => w.put(u32::from(*e), 2),
            17 => w.put(u32::from(*e), 3),
            18 => w.put(u32::from(*e), 7),
            _ => {}
        }
    }
    for t in toks {
        match *t {
            Tok::Lit(b) => w.code(lc[usize::from(b)], ll[usize::from(b)]),
            Tok::Match(l, d) => {
                let (s, n, v) = len_sym(l);
                w.code(lc[s], ll[s]);
                w.put(v, n);
                let (s, n, v) = dist_sym(d);
                w.code(dc[s], dl[s]);
                w.put(v, n);
            }
        }
    }
    w.code(lc[256], ll[256]);
}

/// Encode `data` as a raw DEFLATE stream.
#[must_use]
pub fn deflate(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        // One empty fixed block: BFINAL=1, BTYPE=01, end-of-block (7 zero bits).
        return vec![0x03, 0x00];
    }
    let toks = lz77(data);
    let mut w = BitWriter::new();
    let chunk = 16_384;
    let blocks: Vec<&[Tok]> = toks.chunks(chunk).collect();
    for (i, b) in blocks.iter().enumerate() {
        write_block(&mut w, b, i + 1 == blocks.len());
    }
    let compressed = w.flush();
    // Stored fallback for incompressible input.
    let stored_len = data.len() + data.len().div_ceil(65_535) * 5;
    if compressed.len() <= stored_len {
        return compressed;
    }
    let mut out = Vec::with_capacity(stored_len);
    let parts: Vec<&[u8]> = data.chunks(65_535).collect();
    for (i, p) in parts.iter().enumerate() {
        out.push(u8::from(i + 1 == parts.len()));
        #[allow(clippy::cast_possible_truncation)]
        let l = p.len() as u16;
        out.extend(l.to_le_bytes());
        out.extend((!l).to_le_bytes());
        out.extend_from_slice(p);
    }
    out
}

/// Adler-32 (zlib's checksum).
#[must_use]
pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += u32::from(x);
            b += a;
        }
        a %= 65_521;
        b %= 65_521;
    }
    (b << 16) | a
}

/// CRC-32 (IEEE 802.3, reflected), as PNG and gzip use.
#[must_use]
pub fn crc32(data: &[u8]) -> u32 {
    crc32_update(0xFFFF_FFFF, data) ^ 0xFFFF_FFFF
}

/// Continue a CRC-32 (pre-/post-inversion is the caller's).
#[must_use]
pub fn crc32_update(mut c: u32, data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let t = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (n, e) in t.iter_mut().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let mut c = n as u32;
            for _ in 0..8 {
                c = if c & 1 == 1 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            *e = c;
        }
        t
    });
    for &b in data {
        c = t[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8);
    }
    c
}

/// zlib-wrapped DEFLATE.
#[must_use]
pub fn zlib_compress(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x9C];
    out.extend(deflate(data));
    out.extend(adler32(data).to_be_bytes());
    out
}

/// Unwrap and inflate a zlib stream, checking the header and Adler-32.
///
/// # Errors
/// On a bad header, a preset dictionary, malformed data or a checksum mismatch.
pub fn zlib_decompress(data: &[u8]) -> Result<Vec<u8>, InflateError> {
    if data.len() < 6 {
        return Err(InflateError("zlib stream too short"));
    }
    let (cmf, flg) = (data[0], data[1]);
    if cmf & 0x0f != 8 || (u16::from(cmf) << 8 | u16::from(flg)) % 31 != 0 {
        return Err(InflateError("bad zlib header"));
    }
    if flg & 0x20 != 0 {
        return Err(InflateError("preset dictionary"));
    }
    let out = inflate(&data[2..])?;
    let n = data.len();
    let want = u32::from_be_bytes([data[n - 4], data[n - 3], data[n - 2], data[n - 1]]);
    if adler32(&out) != want {
        return Err(InflateError("Adler-32 mismatch"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples() -> Vec<Vec<u8>> {
        let mut rng = 12345u32;
        let noise: Vec<u8> = (0..20_000)
            .map(|_| {
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                #[allow(clippy::cast_possible_truncation)]
                let b = rng as u8;
                b
            })
            .collect();
        let text = "the quick brown fox jumps over the lazy dog. "
            .repeat(500)
            .into_bytes();
        let runs: Vec<u8> = (0..70_000u32).map(|i| ((i / 1000) % 7) as u8).collect();
        vec![Vec::new(), vec![7], b"abc".to_vec(), noise, text, runs]
    }

    #[test]
    fn round_trips() {
        for s in samples() {
            let c = deflate(&s);
            assert_eq!(inflate(&c).unwrap(), s, "len {}", s.len());
            assert_eq!(zlib_decompress(&zlib_compress(&s)).unwrap(), s);
        }
    }

    #[test]
    fn compresses_redundant_data_and_never_explodes_noise() {
        let s = samples();
        let text = &s[4];
        assert!(
            deflate(text).len() * 20 < text.len(),
            "{}",
            deflate(text).len()
        );
        let noise = &s[3];
        assert!(deflate(noise).len() <= noise.len() + 10);
    }

    #[test]
    fn decodes_a_known_zlib_stream() {
        // `zlib.compress(b"hello hello hello")` from CPython.
        let z = [
            0x78, 0x9c, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0xc8, 0x40, 0x90, 0x00, 0x3a, 0x2e,
            0x06, 0x7d,
        ];
        assert_eq!(zlib_decompress(&z).unwrap(), b"hello hello hello");
    }

    #[test]
    fn checksums_match_reference_values() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn corrupt_streams_are_refused() {
        let mut z = zlib_compress(b"some data some data");
        let n = z.len();
        z[n - 1] ^= 1;
        assert!(zlib_decompress(&z).is_err());
        assert!(inflate(&[0x07]).is_err(), "reserved block type");
        assert!(inflate(&[]).is_err());
    }
}
