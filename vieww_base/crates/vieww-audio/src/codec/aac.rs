//! AAC-LC decoding (ISO/IEC 14496-3, General Audio, Low Complexity).
//!
//! Containers: ADTS frames ([`decode_adts`]) and MP4/M4A
//! ([`decode_mp4`], through [`mp4`](super::mp4), honouring the edit list's
//! priming and duration so the output is sample-exact).
//!
//! Syntax: single-channel, channel-pair (with common window and M/S mask),
//! LFE, data-stream, fill and program-config elements; section data,
//! scalefactors (with intensity positions and PNS energies), pulse data,
//! TNS, and all eleven spectral codebooks with escape sequences.
//! Tools: requantisation (`|q|^{4/3}·2^{(sf−100)/4}`), M/S stereo,
//! intensity stereo, perceptual noise substitution, TNS synthesis
//! filtering, short-window grouping, and the filterbank — 2048- and
//! 256-point IMDCTs with sine and Kaiser–Bessel-derived windows and the
//! four window sequences. Channel layouts 1–7 are mapped to WAV order.
//!
//! Refused by name: HE-AAC's SBR/PS (object types 5 and 29 are decoded at
//! the core rate only when signalled implicitly), Main-profile prediction,
//! LTP, and coupling channels.

use std::sync::OnceLock;

use super::aac_tables::{BOOKS, SCF, SWB_LONG, SWB_SHORT};
use super::bits::MsbReader;
use super::{CodecError, Decoded};
use crate::fft::Imdct;
use crate::rng::Rng;

const RATES: [u32; 12] = [
    96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025, 8_000,
];

type Rd<'a> = MsbReader<'a>;

fn rd(r: &mut Rd<'_>, n: u32) -> Result<u32, CodecError> {
    r.read(n).ok_or(CodecError::Truncated)
}

/// Huffman tree for a codebook: leaf = !index.
struct Tree(Vec<[i32; 2]>);

fn build(codes: impl Iterator<Item = (u32, u8)>) -> Tree {
    let mut nodes = vec![[0i32; 2]];
    for (idx, (code, len)) in codes.enumerate() {
        let mut n = 0usize;
        for i in (0..len).rev() {
            let bit = ((code >> i) & 1) as usize;
            if i == 0 {
                nodes[n][bit] = !i32::try_from(idx).unwrap_or(0);
            } else {
                if nodes[n][bit] <= 0 {
                    nodes.push([0, 0]);
                    nodes[n][bit] = i32::try_from(nodes.len() - 1).unwrap_or(0);
                }
                n = usize::try_from(nodes[n][bit]).unwrap_or(0);
            }
        }
    }
    Tree(nodes)
}

fn trees() -> &'static (Vec<Tree>, Tree) {
    static T: OnceLock<(Vec<Tree>, Tree)> = OnceLock::new();
    T.get_or_init(|| {
        let books = BOOKS
            .iter()
            .map(|b| build(b.iter().map(|e| (e.0, e.1))))
            .collect();
        let scf = build(SCF.iter().copied());
        (books, scf)
    })
}

fn walk(r: &mut Rd<'_>, t: &Tree) -> Result<usize, CodecError> {
    let mut n = 0usize;
    loop {
        let b = usize::from(r.bit().ok_or(CodecError::Truncated)?);
        let v = t.0.get(n).ok_or(CodecError::Malformed("huffman"))?[b];
        if v < 0 {
            return usize::try_from(!v).map_err(|_| CodecError::Malformed("huffman"));
        }
        if v == 0 {
            return Err(CodecError::Malformed("invalid huffman code"));
        }
        n = usize::try_from(v).map_err(|_| CodecError::Malformed("huffman"))?;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Seq {
    #[default]
    OnlyLong,
    LongStart,
    EightShort,
    LongStop,
}

#[derive(Debug, Clone, Default)]
struct IcsInfo {
    seq: Seq,
    kbd: bool,
    max_sfb: usize,
    /// Group lengths in windows (sum 8 for short, `[1]` for long).
    groups: Vec<usize>,
}

impl IcsInfo {
    fn short(&self) -> bool {
        self.seq == Seq::EightShort
    }
}

fn ics_info(r: &mut Rd<'_>) -> Result<IcsInfo, CodecError> {
    rd(r, 1)?;
    let seq = match rd(r, 2)? {
        0 => Seq::OnlyLong,
        1 => Seq::LongStart,
        2 => Seq::EightShort,
        _ => Seq::LongStop,
    };
    let kbd = rd(r, 1)? == 1;
    if seq == Seq::EightShort {
        let max_sfb = rd(r, 4)? as usize;
        let grouping = rd(r, 7)?;
        let mut groups = vec![1usize];
        for i in (0..7).rev() {
            if grouping & (1 << i) != 0 {
                if let Some(last) = groups.last_mut() {
                    *last += 1;
                }
            } else {
                groups.push(1);
            }
        }
        Ok(IcsInfo {
            seq,
            kbd,
            max_sfb,
            groups,
        })
    } else {
        let max_sfb = rd(r, 6)? as usize;
        if rd(r, 1)? == 1 {
            return Err(CodecError::Unsupported(
                "AAC prediction (Main/LTP profiles)",
            ));
        }
        Ok(IcsInfo {
            seq,
            kbd,
            max_sfb,
            groups: vec![1],
        })
    }
}

/// One TNS filter: (start band, end band, runs upward, LPC coefficients).
type TnsFilter = (usize, usize, bool, Vec<f32>);

#[derive(Debug, Clone, Default)]
struct Tns {
    /// Per window: its filters.
    filters: Vec<Vec<TnsFilter>>,
}

/// One decoded channel's spectrum (1024 values; short windows as 8×128).
#[derive(Debug, Clone)]
struct Channel {
    info: IcsInfo,
    spec: Vec<f32>,
    /// Codebook per (group, band).
    cb: Vec<Vec<u8>>,
    /// Scalefactor / intensity position / noise energy per (group, band).
    sf: Vec<Vec<i32>>,
    tns: Tns,
}

const ZERO_HCB: u8 = 0;
const NOISE_HCB: u8 = 13;
const INTENSITY_HCB2: u8 = 14;
const INTENSITY_HCB: u8 = 15;

fn swb(rate_idx: usize, short: bool) -> &'static [u16] {
    if short {
        SWB_SHORT[rate_idx]
    } else {
        SWB_LONG[rate_idx]
    }
}

#[expect(clippy::too_many_lines, reason = "the ICS syntax, top to bottom")]
fn ics(
    r: &mut Rd<'_>,
    rate_idx: usize,
    common: Option<&IcsInfo>,
    rng: &mut Rng,
) -> Result<Channel, CodecError> {
    let global_gain = i32::try_from(rd(r, 8)?).unwrap_or(0);
    let info = match common {
        Some(i) => i.clone(),
        None => ics_info(r)?,
    };
    let short = info.short();
    let bands = swb(rate_idx, short);
    let nbands = bands.len() - 1;
    if info.max_sfb > nbands {
        return Err(CodecError::Malformed("max_sfb"));
    }
    let ngroups = info.groups.len();
    // Section data.
    let (sect_bits, esc) = if short { (3, 7) } else { (5, 31) };
    let mut cb = vec![vec![0u8; info.max_sfb]; ngroups];
    for g in cb.iter_mut() {
        let mut k = 0;
        while k < info.max_sfb {
            let book = u8::try_from(rd(r, 4)?).unwrap_or(0);
            if book == 12 {
                return Err(CodecError::Malformed("reserved codebook 12"));
            }
            let mut len = 0usize;
            loop {
                let inc = rd(r, sect_bits)? as usize;
                len += inc;
                if inc != esc {
                    break;
                }
            }
            if k + len > info.max_sfb {
                return Err(CodecError::Malformed("section length"));
            }
            for b in &mut g[k..k + len] {
                *b = book;
            }
            k += len;
        }
    }
    // Scalefactors.
    let (_, scf_tree) = trees();
    let mut sf = vec![vec![0i32; info.max_sfb]; ngroups];
    let mut gain = global_gain;
    let mut is_pos = 0i32;
    let mut noise = global_gain - 90;
    let mut first_noise = true;
    for (g, row) in sf.iter_mut().enumerate() {
        for (b, v) in row.iter_mut().enumerate() {
            match cb[g][b] {
                ZERO_HCB => *v = 0,
                INTENSITY_HCB | INTENSITY_HCB2 => {
                    is_pos += i32::try_from(walk(r, scf_tree)?).unwrap_or(60) - 60;
                    *v = is_pos;
                }
                NOISE_HCB => {
                    if first_noise {
                        first_noise = false;
                        noise += i32::try_from(rd(r, 9)?).unwrap_or(256) - 256;
                    } else {
                        noise += i32::try_from(walk(r, scf_tree)?).unwrap_or(60) - 60;
                    }
                    *v = noise;
                }
                _ => {
                    gain += i32::try_from(walk(r, scf_tree)?).unwrap_or(60) - 60;
                    *v = gain;
                }
            }
        }
    }
    // Pulse data.
    let mut pulses: Vec<(usize, i32)> = Vec::new();
    if rd(r, 1)? == 1 {
        if short {
            return Err(CodecError::Malformed("pulse data in a short window"));
        }
        let n = rd(r, 2)? as usize + 1;
        let start = rd(r, 6)? as usize;
        let mut k = usize::from(
            *bands
                .get(start)
                .ok_or(CodecError::Malformed("pulse start"))?,
        );
        for _ in 0..n {
            k += rd(r, 5)? as usize;
            let amp = i32::try_from(rd(r, 4)?).unwrap_or(0);
            pulses.push((k, amp));
        }
    }
    // TNS.
    let mut tns = Tns::default();
    if rd(r, 1)? == 1 {
        let windows = if short { 8 } else { 1 };
        let (nf_bits, len_bits, ord_bits) = if short { (1, 4, 3) } else { (2, 6, 5) };
        for _ in 0..windows {
            let nf = rd(r, nf_bits)?;
            let mut fl = Vec::new();
            if nf > 0 {
                let res = rd(r, 1)?;
                let mut top = nbands.min(info.max_sfb.max(nbands));
                top = top.min(nbands);
                for _ in 0..nf {
                    let len = rd(r, len_bits)? as usize;
                    let order = rd(r, ord_bits)? as usize;
                    let bottom = top.saturating_sub(len);
                    if order > 0 {
                        let up = rd(r, 1)? == 0;
                        let compress = rd(r, 1)?;
                        let bits = res + 3 - compress;
                        let mut coefs = Vec::with_capacity(order);
                        for _ in 0..order {
                            let raw = rd(r, bits)?;
                            // Sign-extend, then inverse-quantise (sin domain).
                            let shift = 32 - bits;
                            let q = (raw.cast_signed() << shift) >> shift;
                            let iqfac = if q >= 0 {
                                ((1 << (res + 3 - 1)) as f64 - 0.5) / (std::f64::consts::PI / 2.0)
                            } else {
                                ((1 << (res + 3 - 1)) as f64 + 0.5) / (std::f64::consts::PI / 2.0)
                            };
                            #[expect(
                                clippy::cast_possible_truncation,
                                reason = "f32 coefficients"
                            )]
                            coefs.push((f64::from(q) / iqfac).sin() as f32);
                        }
                        fl.push((bottom, top, up, parcor_to_lpc(&coefs)));
                    }
                    top = bottom;
                }
            }
            tns.filters.push(fl);
        }
    }
    if rd(r, 1)? == 1 {
        return Err(CodecError::Unsupported("AAC gain control (SSR profile)"));
    }
    // Spectral data.
    let (books, _) = trees();
    let mut q = vec![0i32; 1024];
    let mut win0 = 0usize; // first window of the current group
    for (g, &glen) in info.groups.iter().enumerate() {
        for b in 0..info.max_sfb {
            let book = cb[g][b];
            let (lo, hi) = (usize::from(bands[b]), usize::from(bands[b + 1]));
            if book == ZERO_HCB || book >= NOISE_HCB {
                continue;
            }
            let bi = usize::from(book);
            let entries = BOOKS[bi];
            let (dims, unsigned) = match book {
                1 | 2 => (4, false),
                3 | 4 => (4, true),
                5 | 6 => (2, false),
                _ => (2, true),
            };
            for w in 0..glen {
                let base = (win0 + w) * 128 * usize::from(short);
                let mut k = lo;
                while k < hi {
                    let idx = walk(r, &books[bi])?;
                    let e = entries[idx];
                    let mut vals = [
                        i32::from(e.2),
                        i32::from(e.3),
                        i32::from(e.4),
                        i32::from(e.5),
                    ];
                    if unsigned {
                        for v in vals.iter_mut().take(dims) {
                            if *v != 0 && rd(r, 1)? == 1 {
                                *v = -*v;
                            }
                        }
                    }
                    if book == 11 {
                        for v in vals.iter_mut().take(2) {
                            if v.abs() == 16 {
                                let mut n = 0;
                                while rd(r, 1)? == 1 {
                                    n += 1;
                                    if n > 8 {
                                        return Err(CodecError::Malformed("escape"));
                                    }
                                }
                                let word = i32::try_from(rd(r, n + 4)?).unwrap_or(0);
                                let mag = (1 << (n + 4)) + word;
                                *v = if *v < 0 { -mag } else { mag };
                            }
                        }
                    }
                    for (j, v) in vals.iter().take(dims).enumerate() {
                        if let Some(slot) = q.get_mut(base + k + j) {
                            *slot = *v;
                        }
                    }
                    k += dims;
                }
            }
        }
        win0 += glen;
    }
    for (k, amp) in pulses {
        if let Some(v) = q.get_mut(k) {
            *v += if *v < 0 { -amp } else { amp };
        }
    }
    // Requantise, and noise substitution.
    let mut spec = vec![0f32; 1024];
    let mut win0 = 0usize;
    for (g, &glen) in info.groups.iter().enumerate() {
        for b in 0..info.max_sfb {
            let (lo, hi) = (usize::from(bands[b]), usize::from(bands[b + 1]));
            let book = cb[g][b];
            for w in 0..glen {
                let base = (win0 + w) * 128 * usize::from(short);
                if book == NOISE_HCB {
                    let mut energy = 0.0f64;
                    let mut tmp = vec![0f64; hi - lo];
                    for t in &mut tmp {
                        *t = rng.range(-1.0, 1.0);
                        energy += *t * *t;
                    }
                    let scale = 2f64.powf(0.25 * f64::from(sf[g][b])) / energy.max(1e-30).sqrt();
                    for (j, t) in tmp.iter().enumerate() {
                        #[expect(clippy::cast_possible_truncation, reason = "f32 audio")]
                        {
                            spec[base + lo + j] = (t * scale) as f32;
                        }
                    }
                } else if book != ZERO_HCB && book < NOISE_HCB {
                    let scale = 2f64.powf(0.25 * f64::from(sf[g][b] - 100));
                    for k in lo..hi {
                        let v = q[base + k];
                        if v != 0 {
                            let m = f64::from(v.unsigned_abs()).powf(4.0 / 3.0) * scale;
                            #[expect(clippy::cast_possible_truncation, reason = "f32 audio")]
                            {
                                spec[base + k] = if v < 0 { -m as f32 } else { m as f32 };
                            }
                        }
                    }
                }
            }
        }
        win0 += glen;
    }
    Ok(Channel {
        info,
        spec,
        cb,
        sf,
        tns,
    })
}

/// Step-up recursion: reflection coefficients to direct-form LPC.
fn parcor_to_lpc(k: &[f32]) -> Vec<f32> {
    let mut a: Vec<f32> = Vec::with_capacity(k.len());
    for (m, &km) in k.iter().enumerate() {
        let prev = a.clone();
        a.push(km);
        for i in 0..m {
            a[i] = prev[i] + km * prev[m - 1 - i];
        }
    }
    a
}

/// TNS_MAX_BANDS for AAC-LC (long, short) per sampling-frequency index.
const TNS_MAX_BANDS: [(usize, usize); 12] = [
    (31, 9),
    (31, 9),
    (34, 10),
    (40, 14),
    (42, 14),
    (51, 14),
    (46, 14),
    (46, 14),
    (42, 14),
    (42, 14),
    (42, 14),
    (39, 14),
];

fn apply_tns(ch: &mut Channel, rate_idx: usize) {
    let short = ch.info.short();
    let bands = swb(rate_idx, short);
    let nbands = bands.len() - 1;
    let limit = if short {
        TNS_MAX_BANDS[rate_idx].1
    } else {
        TNS_MAX_BANDS[rate_idx].0
    };
    let max = ch.info.max_sfb.min(nbands).min(limit);
    for (w, filters) in ch.tns.filters.iter().enumerate() {
        let base = if short { w * 128 } else { 0 };
        for (bottom, top, up, lpc) in filters {
            let start = usize::from(bands[(*bottom).min(max)]);
            let end = usize::from(bands[(*top).min(max)]);
            if end <= start || lpc.is_empty() {
                continue;
            }
            let order = lpc.len();
            let mut hist = vec![0f32; order];
            let n = end - start;
            for i in 0..n {
                let k = if *up { start + i } else { end - 1 - i };
                let x = ch.spec[base + k];
                let mut y = x;
                for j in 0..order {
                    y -= lpc[j] * hist[j];
                }
                hist.rotate_right(1);
                hist[0] = y;
                ch.spec[base + k] = y;
            }
        }
    }
}

fn ms_and_intensity(l: &mut Channel, r: &mut Channel, ms: &[Vec<bool>], rate_idx: usize) {
    let short = l.info.short();
    let bands = swb(rate_idx, short);
    let mut win0 = 0usize;
    for (g, &glen) in l.info.groups.iter().enumerate() {
        for b in 0..l.info.max_sfb.min(r.info.max_sfb) {
            let (lo, hi) = (usize::from(bands[b]), usize::from(bands[b + 1]));
            let rb = r.cb[g][b];
            let msb = ms.get(g).and_then(|m| m.get(b)).copied().unwrap_or(false);
            for w in 0..glen {
                let base = (win0 + w) * 128 * usize::from(short);
                if rb == INTENSITY_HCB || rb == INTENSITY_HCB2 {
                    let mut sign = if rb == INTENSITY_HCB { 1.0 } else { -1.0 };
                    if msb {
                        sign = -sign;
                    }
                    #[expect(clippy::cast_possible_truncation, reason = "f32 audio")]
                    let scale = (sign * 0.5f64.powf(0.25 * f64::from(r.sf[g][b]))) as f32;
                    for k in lo..hi {
                        r.spec[base + k] = l.spec[base + k] * scale;
                    }
                } else if msb && l.cb[g][b] < NOISE_HCB && rb < NOISE_HCB {
                    for k in lo..hi {
                        let (m, s) = (l.spec[base + k], r.spec[base + k]);
                        l.spec[base + k] = m + s;
                        r.spec[base + k] = m - s;
                    }
                }
            }
        }
        win0 += glen;
    }
}

struct Windows {
    long: [Vec<f32>; 2],  // [sine, kbd], 1024 rising halves
    short: [Vec<f32>; 2], // 128 rising halves
}

fn kbd(n: usize, alpha: f64) -> Vec<f32> {
    let half = n / 2;
    let beta = std::f64::consts::PI * alpha;
    let i0b = crate::fx::fir::bessel_i0(beta);
    #[expect(clippy::cast_precision_loss, reason = "window sizes")]
    let k: Vec<f64> = (0..=half)
        .map(|j| {
            let r = 2.0 * j as f64 / half as f64 - 1.0;
            crate::fx::fir::bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / i0b
        })
        .collect();
    let total: f64 = k.iter().sum();
    let mut acc = 0.0;
    (0..half)
        .map(|j| {
            acc += k[j];
            #[expect(clippy::cast_possible_truncation, reason = "f32 window")]
            let v = (acc / total).sqrt() as f32;
            v
        })
        .collect()
}

fn sine(n: usize) -> Vec<f32> {
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "f32 window"
    )]
    (0..n / 2)
        .map(|i| (std::f64::consts::PI / n as f64 * (i as f64 + 0.5)).sin() as f32)
        .collect()
}

fn windows() -> &'static Windows {
    static W: OnceLock<Windows> = OnceLock::new();
    W.get_or_init(|| Windows {
        long: [sine(2048), kbd(2048, 4.0)],
        short: [sine(256), kbd(256, 6.0)],
    })
}

/// Filterbank state for one channel.
#[derive(Debug, Clone)]
struct Fb {
    overlap: Vec<f32>,
    prev_kbd: bool,
}

impl Fb {
    fn new() -> Self {
        Self {
            overlap: vec![0.0; 1024],
            prev_kbd: false,
        }
    }

    fn run(&mut self, ch: &Channel, long: &Imdct, short: &Imdct) -> Vec<f32> {
        let w = windows();
        let cur = usize::from(ch.info.kbd);
        let prev = usize::from(self.prev_kbd);
        let mut buf = vec![0f32; 2048];
        match ch.info.seq {
            Seq::EightShort => {
                let mut y = vec![0f32; 256];
                for win in 0..8 {
                    short.run(&ch.spec[win * 128..win * 128 + 128], &mut y);
                    let left = if win == 0 {
                        &w.short[prev]
                    } else {
                        &w.short[cur]
                    };
                    let right = &w.short[cur];
                    for i in 0..256 {
                        let wv = if i < 128 { left[i] } else { right[255 - i] };
                        buf[448 + win * 128 + i] += y[i] * wv * (2.0 / 256.0);
                    }
                }
            }
            seq => {
                long.run(&ch.spec, &mut buf);
                for (i, v) in buf.iter_mut().enumerate() {
                    let left = match seq {
                        Seq::LongStop => {
                            if i < 448 {
                                0.0
                            } else if i < 576 {
                                w.short[prev][i - 448]
                            } else if i < 1024 {
                                1.0
                            } else {
                                w.long[cur][2047 - i]
                            }
                        }
                        _ => {
                            if i < 1024 {
                                w.long[prev][i]
                            } else {
                                w.long[cur][2047 - i]
                            }
                        }
                    };
                    let wv = if seq == Seq::LongStart && i >= 1024 {
                        if i < 1472 {
                            1.0
                        } else if i < 1600 {
                            w.short[cur][1599 - i]
                        } else {
                            0.0
                        }
                    } else {
                        left
                    };
                    *v *= wv * (2.0 / 2048.0);
                }
            }
        }
        let out: Vec<f32> = (0..1024).map(|i| buf[i] + self.overlap[i]).collect();
        self.overlap.copy_from_slice(&buf[1024..]);
        self.prev_kbd = ch.info.kbd;
        out
    }
}

/// The decoder for a stream of raw data blocks.
struct Decoder {
    rate_idx: usize,
    channels: usize,
    fbs: Vec<Fb>,
    long: Imdct,
    short: Imdct,
    rng: Rng,
}

impl Decoder {
    fn new(rate_idx: usize, channels: usize) -> Self {
        Self {
            rate_idx,
            channels,
            fbs: (0..channels.max(1)).map(|_| Fb::new()).collect(),
            long: Imdct::new(2048),
            short: Imdct::new(256),
            rng: Rng::new(0x5EED),
        }
    }

    /// Decode one raw_data_block into interleaved PCM (1024 frames).
    fn block(&mut self, data: &[u8]) -> Result<Vec<f32>, CodecError> {
        let mut r = Rd::new(data);
        let mut chans: Vec<Channel> = Vec::new();
        loop {
            let id = rd(&mut r, 3)?;
            match id {
                0 | 3 => {
                    rd(&mut r, 4)?;
                    let mut c = ics(&mut r, self.rate_idx, None, &mut self.rng)?;
                    apply_tns(&mut c, self.rate_idx);
                    chans.push(c);
                }
                1 => {
                    rd(&mut r, 4)?;
                    let common = rd(&mut r, 1)? == 1;
                    let mut info = None;
                    let mut ms = Vec::new();
                    if common {
                        let i = ics_info(&mut r)?;
                        let mask = rd(&mut r, 2)?;
                        ms = match mask {
                            1 => {
                                let mut m = Vec::new();
                                for _ in 0..i.groups.len() {
                                    let row: Vec<bool> = (0..i.max_sfb)
                                        .map(|_| rd(&mut r, 1).map(|v| v == 1))
                                        .collect::<Result<_, _>>()?;
                                    m.push(row);
                                }
                                m
                            }
                            2 => vec![vec![true; i.max_sfb]; i.groups.len()],
                            _ => Vec::new(),
                        };
                        info = Some(i);
                    }
                    let mut l = ics(&mut r, self.rate_idx, info.as_ref(), &mut self.rng)?;
                    let mut rr = ics(&mut r, self.rate_idx, info.as_ref(), &mut self.rng)?;
                    if common {
                        ms_and_intensity(&mut l, &mut rr, &ms, self.rate_idx);
                    } else {
                        ms_and_intensity(&mut l, &mut rr, &[], self.rate_idx);
                    }
                    apply_tns(&mut l, self.rate_idx);
                    apply_tns(&mut rr, self.rate_idx);
                    chans.push(l);
                    chans.push(rr);
                }
                2 => return Err(CodecError::Unsupported("AAC coupling channel elements")),
                4 => {
                    rd(&mut r, 4)?;
                    let align = rd(&mut r, 1)? == 1;
                    let mut n = rd(&mut r, 8)? as usize;
                    if n == 255 {
                        n += rd(&mut r, 8)? as usize;
                    }
                    if align {
                        r.align();
                    }
                    r.skip(n * 8).ok_or(CodecError::Truncated)?;
                }
                5 => skip_pce(&mut r)?,
                6 => {
                    let mut n = rd(&mut r, 4)? as usize;
                    if n == 15 {
                        n += rd(&mut r, 8)? as usize - 1;
                    }
                    r.skip(n * 8).ok_or(CodecError::Truncated)?;
                }
                _ => break,
            }
        }
        let nch = self.channels.max(chans.len()).max(1);
        while self.fbs.len() < chans.len() {
            self.fbs.push(Fb::new());
        }
        let mut pcm = vec![0f32; 1024 * nch];
        for (c, ch) in chans.iter().enumerate().take(nch) {
            let out = self.fbs[c].run(ch, &self.long, &self.short);
            for (i, v) in out.iter().enumerate() {
                pcm[i * nch + c] = v / 32_768.0;
            }
        }
        Ok(pcm)
    }
}

fn skip_pce(r: &mut Rd<'_>) -> Result<(), CodecError> {
    rd(r, 4)?;
    rd(r, 2)?;
    rd(r, 4)?;
    let front = rd(r, 4)?;
    let side = rd(r, 4)?;
    let back = rd(r, 4)?;
    let lfe = rd(r, 2)?;
    let assoc = rd(r, 3)?;
    let cc = rd(r, 4)?;
    for _ in 0..3 {
        if rd(r, 1)? == 1 {
            rd(r, 4)?;
        }
    }
    if rd(r, 1)? == 1 {
        rd(r, 3)?;
    }
    for _ in 0..(front + side + back) {
        rd(r, 5)?;
    }
    for _ in 0..lfe {
        rd(r, 4)?;
    }
    for _ in 0..assoc {
        rd(r, 4)?;
    }
    for _ in 0..cc {
        rd(r, 5)?;
    }
    r.align();
    let n = rd(r, 8)? as usize;
    r.skip(n * 8).ok_or(CodecError::Truncated)
}

/// AAC's channel order for configuration `c` mapped to WAV order.
fn reorder(pcm: &mut [f32], channels: usize, config: u8) {
    // AAC: C, L, R, Ls, Rs, LFE (config 6). WAV: L, R, C, LFE, Ls, Rs.
    let map: &[usize] = match config {
        3 => &[1, 2, 0],
        4 => &[1, 2, 0, 3],
        5 => &[1, 2, 0, 3, 4],
        6 => &[1, 2, 0, 5, 3, 4],
        7 => &[3, 4, 0, 7, 5, 6, 1, 2],
        _ => return,
    };
    if map.len() != channels {
        return;
    }
    for f in pcm.chunks_exact_mut(channels) {
        let src = f.to_vec();
        for (dst, &s) in f.iter_mut().zip(map) {
            *dst = src[s];
        }
    }
}

/// Decode an ADTS AAC stream.
///
/// # Errors
/// Malformed frames or unsupported profiles.
pub fn decode_adts(bytes: &[u8]) -> Result<Decoded, CodecError> {
    let mut at = 0;
    let mut dec: Option<Decoder> = None;
    let mut cfg = 0u8;
    let mut out = Vec::new();
    let mut rate = 0;
    while at + 7 <= bytes.len() {
        if !(bytes[at] == 0xFF && bytes[at + 1] & 0xF6 == 0xF0) {
            at += 1;
            continue;
        }
        let mut r = Rd::new(&bytes[at..]);
        rd(&mut r, 12)?;
        rd(&mut r, 1)?;
        rd(&mut r, 2)?;
        let no_crc = rd(&mut r, 1)? == 1;
        let profile = rd(&mut r, 2)?;
        let rate_idx = rd(&mut r, 4)? as usize;
        rd(&mut r, 1)?;
        let channel_cfg = u8::try_from(rd(&mut r, 3)?).unwrap_or(0);
        rd(&mut r, 4)?;
        let len = rd(&mut r, 13)? as usize;
        rd(&mut r, 11)?;
        let blocks = rd(&mut r, 2)? as usize + 1;
        if profile != 1 {
            return Err(CodecError::Unsupported("AAC profiles other than LC"));
        }
        if rate_idx >= 12 || len < 7 || at + len > bytes.len() {
            return Err(CodecError::Malformed("ADTS header"));
        }
        let hdr = if no_crc { 7 } else { 9 };
        if blocks > 1 {
            return Err(CodecError::Unsupported(
                "multiple raw blocks per ADTS frame",
            ));
        }
        let d = dec.get_or_insert_with(|| Decoder::new(rate_idx, usize::from(channel_cfg.max(1))));
        cfg = channel_cfg;
        rate = RATES[rate_idx];
        out.extend(d.block(&bytes[at + hdr..at + len])?);
        at += len;
    }
    let d = dec.ok_or(CodecError::NotThisFormat)?;
    let ch = d.channels.max(1);
    reorder(&mut out, ch, cfg);
    Ok(Decoded {
        data: out,
        channels: u8::try_from(ch).unwrap_or(2),
        rate,
    })
}

/// Decode AAC in an MP4/M4A container.
///
/// # Errors
/// No audio track, unsupported object types, malformed frames.
pub fn decode_mp4(bytes: &[u8]) -> Result<Decoded, CodecError> {
    let t = super::mp4::audio_track(bytes)?;
    let mut r = Rd::new(&t.config);
    let mut aot = rd(&mut r, 5)?;
    if aot == 31 {
        aot = 32 + rd(&mut r, 6)?;
    }
    let rate_idx = rd(&mut r, 4)? as usize;
    let rate = if rate_idx == 15 {
        rd(&mut r, 24)?
    } else {
        *RATES
            .get(rate_idx)
            .ok_or(CodecError::Malformed("rate index"))?
    };
    let channel_cfg = u8::try_from(rd(&mut r, 4)?).unwrap_or(0);
    match aot {
        2 => {}
        5 | 29 => return Err(CodecError::Unsupported("HE-AAC (SBR/PS)")),
        _ => return Err(CodecError::Unsupported("AAC object types other than LC")),
    }
    if rd(&mut r, 1)? == 1 {
        return Err(CodecError::Unsupported("960-sample AAC frames"));
    }
    let ri = RATES.iter().position(|v| *v == rate).unwrap_or(4);
    let mut d = Decoder::new(ri, usize::from(channel_cfg.max(1)));
    let mut out = Vec::new();
    for (off, len) in &t.samples {
        out.extend(d.block(&bytes[*off..off + len])?);
    }
    let ch = d.channels.max(1);
    reorder(&mut out, ch, channel_cfg);
    let skip = usize::try_from(t.skip).unwrap_or(0) * ch;
    let mut data = out.get(skip.min(out.len())..).unwrap_or(&[]).to_vec();
    if let Some(p) = t.play {
        data.truncate(usize::try_from(p).unwrap_or(usize::MAX) * ch);
    }
    Ok(Decoded {
        data,
        channels: u8::try_from(ch).unwrap_or(2),
        rate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{s16le, snr_db};

    #[test]
    fn parcor_conversion_matches_a_hand_case() {
        // Order 2: a = [k1 + k2 k1, k2].
        let a = parcor_to_lpc(&[0.5, -0.25]);
        assert!((a[0] - (0.5 + -0.25 * 0.5)).abs() < 1e-6 && (a[1] + 0.25).abs() < 1e-6);
    }

    #[test]
    fn kbd_window_is_power_complementary() {
        let w = kbd(2048, 4.0);
        for i in 0..1024 {
            let a = w[i];
            let b = w[1023 - i];
            assert!((a * a + b * b - 1.0).abs() < 1e-5, "{i}");
        }
    }

    #[test]
    fn decodes_ffmpegs_adts_mono_like_ffmpeg() {
        let d = decode_adts(include_bytes!("../../tests/fixtures/src_mono.aac")).unwrap();
        let reference = s16le(include_bytes!("../../tests/fixtures/src_mono.aac.ref"));
        assert_eq!((d.channels, d.rate), (1, 44_100));
        assert_eq!(d.data.len(), reference.len());
        let (snr, lag) = snr_db(&reference, &d.data, 1, 2_048);
        eprintln!("aac adts mono: {snr:.1} dB at lag {lag}");
        assert_eq!(lag, 0);
        assert!(snr > 60.0, "SNR {snr} dB");
    }

    /// Energy in dB of `x` between `lo` and `hi` Hz, averaged over frames.
    fn band_db(x: &[f32], lo: f64, hi: f64) -> f64 {
        use crate::fft::{Complex, Plan};
        let n = 2048;
        let plan = Plan::new(n);
        let mut e = 0.0;
        let mut frames = 0;
        for start in (0..x.len().saturating_sub(n)).step_by(n / 2) {
            let mut buf: Vec<Complex> = (0..n)
                .map(|i| Complex::new(f64::from(x[start + i]), 0.0))
                .collect();
            plan.forward(&mut buf);
            for (k, z) in buf.iter().enumerate().take(n / 2) {
                let hz = k as f64 * 44_100.0 / n as f64;
                if hz >= lo && hz < hi {
                    e += z.norm().powi(2);
                }
            }
            frames += 1;
        }
        10.0 * (e / f64::from(frames.max(1))).log10()
    }

    #[test]
    fn noise_substitution_matches_the_reference_by_band_energy() {
        // PNS bands are decoder-generated noise: samples cannot match, the
        // energy per band must.
        let d = decode_adts(include_bytes!("../../tests/fixtures/src_mono_pns.aac")).unwrap();
        let reference = s16le(include_bytes!("../../tests/fixtures/src_mono_pns.aac.ref"));
        for (lo, hi) in [
            (4_000.0, 8_000.0),
            (8_000.0, 12_000.0),
            (12_000.0, 16_000.0),
        ] {
            let (a, b) = (band_db(&d.data, lo, hi), band_db(&reference, lo, hi));
            eprintln!("pns {lo}-{hi} Hz: ours {a:.2} dB, reference {b:.2} dB");
            assert!((a - b).abs() < 1.5, "{lo}-{hi}: {a} vs {b}");
        }
        // Below the PNS bands the decode is still sample-exact.
        let (lo_ours, lo_ref) = (
            band_db(&d.data, 50.0, 2_000.0),
            band_db(&reference, 50.0, 2_000.0),
        );
        assert!((lo_ours - lo_ref).abs() < 0.05);
    }

    #[test]
    fn decodes_ffmpegs_m4a_stereo_with_its_edit_list() {
        let d = decode_mp4(include_bytes!("../../tests/fixtures/src.m4a")).unwrap();
        let reference = s16le(include_bytes!("../../tests/fixtures/src.m4a.ref"));
        assert_eq!((d.channels, d.rate), (2, 44_100));
        assert_eq!(d.frames(), 17_640, "priming skipped, padding trimmed");
        let (snr, lag) = snr_db(&reference, &d.data, 2, 2_048);
        eprintln!("aac m4a stereo: {snr:.1} dB at lag {lag}");
        assert_eq!(lag, 0);
        assert!(snr > 60.0, "SNR {snr} dB");
    }
}
