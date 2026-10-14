//! MP3 — MPEG-1, MPEG-2 and MPEG-2.5 Audio Layer III decoding.
//!
//! The whole of ISO/IEC 11172-3 and 13818-3's Layer III, written from the
//! standards: frame headers at every bitrate and sample rate (free format is
//! refused by name), CRC skipping, the bit reservoir, MPEG-1 scalefactors
//! with `scfsi` reuse and MPEG-2 LSF scalefactor partitions, Huffman
//! big-value regions (with linbits) and count1 quadruples, requantisation
//! with pre-emphasis and sub-block gain, mid/side and intensity stereo
//! (both the MPEG-1 tangent law and the LSF power law), short-block
//! reordering, alias reduction, the 36- and 12-point IMDCTs with all four
//! window shapes and mixed blocks, frequency inversion, and the 32-band
//! polyphase synthesis filterbank with the standard's window.
//!
//! ID3v2 tags are skipped; a Xing/Info header's frame is not played; its
//! LAME extension's encoder delay and padding are honoured so the output
//! is *gapless* — sample-aligned with the original, as ffmpeg and every
//! modern player trim it.

use std::sync::OnceLock;

use super::bits::MsbReader;
use super::mp3_tables::{ENWINDOW, HUFF, LINBITS, SFB_LONG, SFB_MIXED, SFB_SHORT};
use super::{CodecError, Decoded};

#[derive(Debug, Clone, Copy)]
struct Header {
    mpeg1: bool,
    crc: bool,
    rate: u32,
    sr_idx: usize,
    mode: u8,
    mode_ext: u8,
    len: usize,
    channels: usize,
}

const BITRATE_V1: [u32; 15] = [
    0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
];
const BITRATE_V2: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];

fn header(b: &[u8]) -> Option<Header> {
    if b.len() < 4 || b[0] != 0xFF || b[1] & 0xE0 != 0xE0 {
        return None;
    }
    let version = (b[1] >> 3) & 3; // 0: 2.5, 2: 2, 3: 1
    let layer = (b[1] >> 1) & 3;
    if version == 1 || layer != 1 {
        return None; // reserved version, or not Layer III
    }
    let br_idx = usize::from(b[2] >> 4);
    let rate_idx = usize::from((b[2] >> 2) & 3);
    if br_idx == 0 || br_idx == 15 || rate_idx == 3 {
        return None;
    }
    let mpeg1 = version == 3;
    let base = [44_100, 48_000, 32_000][rate_idx];
    let rate = match version {
        3 => base,
        2 => base / 2,
        _ => base / 4,
    };
    let my = rate_idx
        + match version {
            3 => 6,
            2 => 3,
            _ => 0,
        };
    let sr_idx = my - usize::from(my != 0);
    let kbps = if mpeg1 {
        BITRATE_V1[br_idx]
    } else {
        BITRATE_V2[br_idx]
    };
    let padding = usize::from((b[2] >> 1) & 1);
    let len = (if mpeg1 { 144_000 } else { 72_000 } * kbps / rate) as usize + padding;
    let mode = b[3] >> 6;
    Some(Header {
        mpeg1,
        crc: b[1] & 1 == 0,
        rate,
        sr_idx,
        mode,
        mode_ext: (b[3] >> 4) & 3,
        len,
        channels: if mode == 3 { 1 } else { 2 },
    })
}

impl Header {
    fn side_len(&self) -> usize {
        match (self.mpeg1, self.channels) {
            (true, 1) => 17,
            (true, _) => 32,
            (false, 1) => 9,
            (false, _) => 17,
        }
    }

    fn samples(&self) -> usize {
        if self.mpeg1 {
            1152
        } else {
            576
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Granule {
    part2_3: usize,
    big_values: usize,
    global_gain: i32,
    scalefac_compress: u32,
    block_type: u8,
    mixed: bool,
    table: [usize; 3],
    subblock_gain: [i32; 3],
    region0: usize,
    region1: usize,
    preflag: bool,
    scale: bool,
    count1: usize,
}

impl Granule {
    fn short(&self) -> bool {
        self.block_type == 2
    }

    /// The band-width table this granule uses.
    fn sfbtab(&self, sr: usize) -> &'static [u8] {
        if !self.short() {
            &SFB_LONG[sr]
        } else if self.mixed {
            &SFB_MIXED[sr]
        } else {
            &SFB_SHORT[sr]
        }
    }

    /// Long bands at the start of this granule's table.
    fn n_long(&self, mpeg1: bool) -> usize {
        if !self.short() {
            22
        } else if self.mixed {
            if mpeg1 {
                8
            } else {
                6
            }
        } else {
            0
        }
    }
}

struct Side {
    main_data_begin: usize,
    scfsi: [[bool; 4]; 2],
    gr: [[Granule; 2]; 2],
}

fn side_info(r: &mut MsbReader<'_>, h: &Header) -> Option<Side> {
    let nch = h.channels;
    let ngr = if h.mpeg1 { 2 } else { 1 };
    let main_data_begin = r.read(if h.mpeg1 { 9 } else { 8 })? as usize;
    r.read(match (h.mpeg1, nch) {
        (true, 1) => 5,
        (true, _) => 3,
        (false, 1) => 1,
        (false, _) => 2,
    })?;
    let mut scfsi = [[false; 4]; 2];
    if h.mpeg1 {
        for s in scfsi.iter_mut().take(nch) {
            for b in s.iter_mut() {
                *b = r.bit()?;
            }
        }
    }
    let mut gr = [[Granule::default(); 2]; 2];
    for g in gr.iter_mut().take(ngr) {
        for c in g.iter_mut().take(nch) {
            c.part2_3 = r.read(12)? as usize;
            c.big_values = (r.read(9)? as usize).min(288);
            c.global_gain = i32::try_from(r.read(8)?).ok()?;
            c.scalefac_compress = r.read(if h.mpeg1 { 4 } else { 9 })?;
            if r.bit()? {
                c.block_type = u8::try_from(r.read(2)?).ok()?;
                if c.block_type == 0 {
                    return None; // forbidden with window switching
                }
                c.mixed = r.bit()?;
                for t in c.table.iter_mut().take(2) {
                    *t = r.read(5)? as usize;
                }
                for s in &mut c.subblock_gain {
                    *s = i32::try_from(r.read(3)?).ok()?;
                }
                c.region0 = if c.short() && !c.mixed { 8 } else { 7 };
                c.region1 = 255;
            } else {
                for t in &mut c.table {
                    *t = r.read(5)? as usize;
                }
                c.region0 = r.read(4)? as usize;
                c.region1 = r.read(3)? as usize;
            }
            c.preflag = if h.mpeg1 { r.bit()? } else { false };
            c.scale = r.bit()?;
            c.count1 = r.read(1)? as usize;
        }
    }
    Some(Side {
        main_data_begin,
        scfsi,
        gr,
    })
}

/// A Huffman tree: nodes are `[left, right]`, a negative entry is a leaf
/// holding `!(x << 8 | y)`.
struct Tree(Vec<[i32; 2]>);

fn trees() -> &'static Vec<Tree> {
    static TREES: OnceLock<Vec<Tree>> = OnceLock::new();
    TREES.get_or_init(|| {
        HUFF.iter()
            .map(|codes| {
                let mut nodes = vec![[0i32; 2]];
                for &(code, len, x, y) in *codes {
                    let mut n = 0usize;
                    for i in (0..len).rev() {
                        let bit = ((code >> i) & 1) as usize;
                        if i == 0 {
                            nodes[n][bit] = !((i32::from(x) << 8) | i32::from(y));
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
            })
            .collect()
    })
}

fn huff_pair(r: &mut MsbReader<'_>, t: &Tree) -> Option<(i32, i32)> {
    let mut n = 0usize;
    loop {
        let v = t.0.get(n)?[usize::from(r.bit()?)];
        if v < 0 {
            let leaf = !v;
            return Some((leaf >> 8, leaf & 0xFF));
        }
        if v == 0 {
            return None;
        }
        n = usize::try_from(v).ok()?;
    }
}

/// Scalefactors for one granule/channel: long[22], short[13][3].
#[derive(Debug, Clone, Copy, Default)]
struct Scf {
    long: [i32; 22],
    short: [[i32; 3]; 13],
}

const SLEN: [[u32; 16]; 2] = [
    [0, 0, 0, 0, 3, 1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4],
    [0, 1, 2, 3, 0, 1, 2, 3, 1, 2, 3, 1, 2, 3, 2, 3],
];

const NR_OF_SFB: [[[usize; 4]; 3]; 6] = [
    [[6, 5, 5, 5], [9, 9, 9, 9], [6, 9, 9, 9]],
    [[6, 5, 7, 3], [9, 9, 12, 6], [6, 9, 12, 6]],
    [[11, 10, 0, 0], [18, 18, 0, 0], [15, 18, 0, 0]],
    [[7, 7, 7, 0], [12, 12, 12, 0], [6, 15, 12, 0]],
    [[6, 6, 6, 3], [12, 9, 9, 6], [6, 12, 9, 6]],
    [[8, 8, 5, 0], [15, 12, 9, 0], [6, 18, 9, 0]],
];

const PRETAB: [i32; 22] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 3, 3, 3, 2, 0,
];

fn read_scf_mpeg1(
    r: &mut MsbReader<'_>,
    g: &Granule,
    scfsi: &[bool; 4],
    gi: usize,
    prev: &Scf,
) -> Option<Scf> {
    let s1 = SLEN[0][g.scalefac_compress as usize];
    let s2 = SLEN[1][g.scalefac_compress as usize];
    let mut sf = Scf::default();
    let rd = |r: &mut MsbReader<'_>, n: u32| r.read(n).and_then(|v| i32::try_from(v).ok());
    if g.short() {
        let long_end = if g.mixed { 8 } else { 0 };
        for l in 0..long_end {
            sf.long[l] = rd(r, s1)?;
        }
        let start = if g.mixed { 3 } else { 0 };
        for s in start..12 {
            let n = if s < 6 { s1 } else { s2 };
            for w in 0..3 {
                sf.short[s][w] = rd(r, n)?;
            }
        }
    } else {
        let bands = [(0, 6, s1), (6, 11, s1), (11, 16, s2), (16, 21, s2)];
        for (band, &(a, b, n)) in bands.iter().enumerate() {
            for l in a..b {
                sf.long[l] = if gi == 1 && scfsi[band] {
                    prev.long[l]
                } else {
                    rd(r, n)?
                };
            }
        }
    }
    Some(sf)
}

/// LSF scalefactors; returns (scalefactors, illegal intensity positions per
/// long band / short band, preflag).
fn read_scf_lsf(
    r: &mut MsbReader<'_>,
    g: &Granule,
    intensity_right: bool,
) -> Option<(Scf, [i32; 22], [i32; 13], bool)> {
    let mut sfc = g.scalefac_compress;
    let (slen, table, preflag) = if intensity_right {
        sfc >>= 1;
        if sfc < 180 {
            ([sfc / 36, (sfc % 36) / 6, (sfc % 36) % 6, 0], 3, false)
        } else if sfc < 244 {
            let s = sfc - 180;
            ([(s % 64) >> 4, (s % 16) >> 2, s % 4, 0], 4, false)
        } else {
            let s = sfc - 244;
            ([s / 3, s % 3, 0, 0], 5, false)
        }
    } else if sfc < 400 {
        (
            [(sfc >> 4) / 5, (sfc >> 4) % 5, (sfc % 16) >> 2, sfc % 4],
            0,
            false,
        )
    } else if sfc < 500 {
        let s = sfc - 400;
        ([(s >> 2) / 5, (s >> 2) % 5, s % 4, 0], 1, false)
    } else {
        let s = sfc - 500;
        ([s / 3, s % 3, 0, 0], 2, true)
    };
    let block = if !g.short() {
        0
    } else if g.mixed {
        2
    } else {
        1
    };
    let counts = NR_OF_SFB[table][block];
    let mut vals = Vec::with_capacity(39);
    let mut maxes = Vec::with_capacity(39);
    for p in 0..4 {
        for _ in 0..counts[p] {
            let v = r.read(slen[p])?;
            vals.push(i32::try_from(v).ok()?);
            maxes.push((1i32 << slen[p]) - 1);
        }
    }
    let mut sf = Scf::default();
    let mut ill_l = [-1i32; 22];
    let mut ill_s = [-1i32; 13];
    let mut k = 0;
    let n_long = g.n_long(false);
    for (sv, ill) in sf
        .long
        .iter_mut()
        .zip(ill_l.iter_mut())
        .take(n_long.min(21))
    {
        *sv = *vals.get(k).unwrap_or(&0);
        *ill = *maxes.get(k).unwrap_or(&0);
        k += 1;
    }
    if g.short() {
        let start = if g.mixed { 3 } else { 0 };
        for (row, ill) in sf
            .short
            .iter_mut()
            .zip(ill_s.iter_mut())
            .take(12)
            .skip(start)
        {
            for slot in row.iter_mut() {
                *slot = *vals.get(k).unwrap_or(&0);
                *ill = *maxes.get(k).unwrap_or(&0);
                k += 1;
            }
        }
    }
    Some((sf, ill_l, ill_s, preflag))
}

/// Huffman-decode one granule/channel's spectrum into integers.
fn huffman(r: &mut MsbReader<'_>, g: &Granule, sr: usize, end: usize) -> Option<[i32; 576]> {
    let mut is = [0i32; 576];
    let tab = g.sfbtab(sr);
    let edge = |n: usize| tab.iter().take(n).map(|v| usize::from(*v)).sum::<usize>();
    let big = (g.big_values * 2).min(576);
    let r1 = edge(g.region0 + 1).min(big);
    let r2 = if g.region1 >= 255 {
        576
    } else {
        edge(g.region0 + g.region1 + 2)
    }
    .min(big);
    let t = trees();
    let mut i = 0;
    while i < big {
        let ti = if i < r1 {
            g.table[0]
        } else if i < r2 {
            g.table[1]
        } else {
            g.table[2]
        };
        if ti == 0 {
            i += 2;
            continue;
        }
        let lin = u32::from(LINBITS[ti]);
        let (mut x, mut y) = huff_pair(r, &t[ti])?;
        if lin > 0 && x == 15 {
            x += i32::try_from(r.read(lin)?).ok()?;
        }
        if x != 0 && r.bit()? {
            x = -x;
        }
        if lin > 0 && y == 15 {
            y += i32::try_from(r.read(lin)?).ok()?;
        }
        if y != 0 && r.bit()? {
            y = -y;
        }
        is[i] = x;
        is[i + 1] = y;
        i += 2;
    }
    let q = &t[32 + g.count1];
    while i + 4 <= 576 && r.position() < end {
        let (v, _) = huff_pair(r, q)?;
        let mut vals = [(v >> 3) & 1, (v >> 2) & 1, (v >> 1) & 1, v & 1];
        for s in &mut vals {
            if *s != 0 && r.bit()? {
                *s = -1;
            }
        }
        if r.position() > end {
            break; // the last quadruple overran part2_3: discard it
        }
        is[i..i + 4].copy_from_slice(&vals);
        i += 4;
    }
    Some(is)
}

fn pow43(x: i32) -> f32 {
    #[expect(clippy::cast_possible_truncation, reason = "f32 audio")]
    let v = f64::from(x.unsigned_abs()).powf(4.0 / 3.0) as f32;
    if x < 0 {
        -v
    } else {
        v
    }
}

/// Requantise into `xr` following the band table.
fn requantise(is: &[i32; 576], g: &Granule, sf: &Scf, sr: usize, mpeg1: bool, xr: &mut [f32; 576]) {
    let tab = g.sfbtab(sr);
    let n_long = g.n_long(mpeg1);
    let mult = if g.scale { 1.0 } else { 0.5 };
    let gg = f64::from(g.global_gain - 210) * 0.25;
    let mut pos = 0usize;
    for (e, &w) in tab.iter().enumerate() {
        let w = usize::from(w);
        if w == 0 || pos >= 576 {
            break;
        }
        let exp = if e < n_long {
            let pre = if g.preflag { PRETAB[e] } else { 0 };
            gg - mult * f64::from(sf.long[e] + pre)
        } else {
            let k = e - n_long;
            let s = (if g.mixed { 3 } else { 0 }) + k / 3;
            let win = k % 3;
            gg - 2.0 * f64::from(g.subblock_gain[win]) - mult * f64::from(sf.short[s.min(12)][win])
        };
        #[expect(clippy::cast_possible_truncation, reason = "f32 audio")]
        let scale = 2f64.powf(exp) as f32;
        for j in pos..(pos + w).min(576) {
            xr[j] = if is[j] == 0 {
                0.0
            } else {
                pow43(is[j]) * scale
            };
        }
        pos += w;
    }
}

/// Intensity and mid/side stereo on a granule pair (in place).
#[expect(
    clippy::too_many_arguments,
    reason = "the stereo stage reads both channels' whole state"
)]
fn stereo(
    h: &Header,
    g: &Granule,
    l: &mut [f32; 576],
    r: &mut [f32; 576],
    rsf: &Scf,
    ill_l: &[i32; 22],
    ill_s: &[i32; 13],
    r_is: &[i32; 576],
) {
    let ms = h.mode == 1 && h.mode_ext & 2 != 0;
    let intensity = h.mode == 1 && h.mode_ext & 1 != 0;
    let tab = g.sfbtab(h.sr_idx);
    let n_long = g.n_long(h.mpeg1);
    // Where the right channel's last non-zero line is, per window for
    // short bands: intensity applies above it.
    let mut use_is = [false; 576];
    let mut is_pos = [0i32; 576];
    if intensity {
        let mut pos = 0usize;
        let mut ranges = Vec::new();
        for (e, &w) in tab.iter().enumerate() {
            let w = usize::from(w);
            if w == 0 {
                break;
            }
            ranges.push((e, pos, w));
            pos += w;
        }
        // Long part: bands above the highest non-zero long band.
        let mut zero_from_long = 0;
        for &(e, p, w) in &ranges {
            if e < n_long && r_is[p..p + w].iter().any(|v| *v != 0) {
                zero_from_long = e + 1;
            }
        }
        let mut zero_from_short = [0usize; 3];
        for &(e, p, w) in &ranges {
            if e >= n_long && r_is[p..p + w].iter().any(|v| *v != 0) {
                let k = e - n_long;
                zero_from_short[k % 3] = k / 3 + 1;
            }
        }
        let short_any = zero_from_short.iter().any(|v| *v > 0);
        for &(e, p, w) in &ranges {
            let (applies, pos_val, illegal) = if e < n_long {
                let ok = e >= zero_from_long && !(n_long < 22 && short_any);
                (
                    ok,
                    rsf.long[e.min(21)],
                    if h.mpeg1 { 7 } else { ill_l[e.min(21)] },
                )
            } else {
                let k = e - n_long;
                let s = (if g.mixed { 3 } else { 0 }) + k / 3;
                let ok = k / 3 >= zero_from_short[k % 3];
                // The last short band takes the previous band's position.
                let s_eff = if s >= 12 { 11 } else { s };
                (
                    ok,
                    rsf.short[s_eff][k % 3],
                    if h.mpeg1 { 7 } else { ill_s[s_eff] },
                )
            };
            if applies && pos_val != illegal {
                for j in p..(p + w).min(576) {
                    use_is[j] = true;
                    is_pos[j] = pos_val;
                }
            }
        }
    }
    let io = if g.scalefac_compress & 1 == 1 {
        std::f32::consts::FRAC_1_SQRT_2
    } else {
        0.840_896_4
    };
    for i in 0..576 {
        if use_is[i] {
            let x = l[i];
            if h.mpeg1 {
                let p = is_pos[i];
                if p == 6 {
                    l[i] = x;
                    r[i] = 0.0;
                } else {
                    let ratio = (f64::from(p) * std::f64::consts::PI / 12.0).tan();
                    #[expect(clippy::cast_possible_truncation, reason = "f32 audio")]
                    let (kl, kr) = ((ratio / (1.0 + ratio)) as f32, (1.0 / (1.0 + ratio)) as f32);
                    l[i] = x * kl;
                    r[i] = x * kr;
                }
            } else {
                let p = is_pos[i];
                if p == 0 {
                    r[i] = x;
                } else if p % 2 == 1 {
                    l[i] = x * io.powi((p + 1) / 2);
                    r[i] = x;
                } else {
                    r[i] = x * io.powi(p / 2);
                }
            }
        } else if ms {
            let (m, s) = (l[i], r[i]);
            l[i] = (m + s) * std::f32::consts::FRAC_1_SQRT_2;
            r[i] = (m - s) * std::f32::consts::FRAC_1_SQRT_2;
        }
    }
}

/// Reorder short-block lines into subband order (3 windows interleaved).
fn reorder(g: &Granule, sr: usize, mpeg1: bool, xr: &mut [f32; 576]) {
    if !g.short() {
        return;
    }
    let tab = g.sfbtab(sr);
    let n_long = g.n_long(mpeg1);
    let mut pos: usize = tab.iter().take(n_long).map(|v| usize::from(*v)).sum();
    let mut out = *xr;
    let mut e = n_long;
    while e + 2 < tab.len() && tab[e] != 0 {
        let w = usize::from(tab[e]);
        for j in 0..w {
            for win in 0..3 {
                let src = pos + win * w + j;
                let dst = pos + 3 * j + win;
                if src < 576 && dst < 576 {
                    out[dst] = xr[src];
                }
            }
        }
        pos += 3 * w;
        e += 3;
    }
    *xr = out;
}

fn antialias(g: &Granule, xr: &mut [f32; 576]) {
    static CS: OnceLock<([f32; 8], [f32; 8])> = OnceLock::new();
    let (cs, ca) = CS.get_or_init(|| {
        let ci = [
            -0.6, -0.535, -0.33, -0.185, -0.095, -0.041, -0.0142, -0.0037f64,
        ];
        let mut cs = [0f32; 8];
        let mut ca = [0f32; 8];
        for i in 0..8 {
            let d = (1.0 + ci[i] * ci[i]).sqrt();
            #[expect(clippy::cast_possible_truncation, reason = "f32 audio")]
            {
                cs[i] = (1.0 / d) as f32;
                ca[i] = (ci[i] / d) as f32;
            }
        }
        (cs, ca)
    });
    let bounds = if g.short() {
        if g.mixed {
            1
        } else {
            0
        }
    } else {
        31
    };
    for sb in 0..bounds {
        for i in 0..8 {
            let a = 18 * sb + 17 - i;
            let b = 18 * (sb + 1) + i;
            let (bu, bd) = (xr[a], xr[b]);
            xr[a] = bu * cs[i] - bd * ca[i];
            xr[b] = bd * cs[i] + bu * ca[i];
        }
    }
}

struct Imdct {
    cos36: Vec<f32>,
    cos12: Vec<f32>,
    win: [[f32; 36]; 4],
}

fn imdct_tables() -> &'static Imdct {
    static T: OnceLock<Imdct> = OnceLock::new();
    T.get_or_init(|| {
        use std::f64::consts::PI;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss,
            reason = "f32 tables"
        )]
        let cos36 = (0..36 * 18)
            .map(|n| {
                let (i, k) = (n / 18, n % 18);
                (PI / 72.0 * (2.0 * i as f64 + 1.0 + 18.0) * (2.0 * k as f64 + 1.0)).cos() as f32
            })
            .collect();
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss,
            reason = "f32 tables"
        )]
        let cos12 = (0..12 * 6)
            .map(|n| {
                let (i, k) = (n / 6, n % 6);
                (PI / 24.0 * (2.0 * i as f64 + 1.0 + 6.0) * (2.0 * k as f64 + 1.0)).cos() as f32
            })
            .collect();
        let mut win = [[0f32; 36]; 4];
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss,
            clippy::needless_range_loop,
            reason = "f32 tables, four windows by index"
        )]
        for i in 0..36 {
            let s36 = (PI / 36.0 * (i as f64 + 0.5)).sin() as f32;
            win[0][i] = s36;
            win[1][i] = if i < 18 {
                s36
            } else if i < 24 {
                1.0
            } else if i < 30 {
                (PI / 12.0 * ((i - 18) as f64 + 0.5)).sin() as f32
            } else {
                0.0
            };
            win[3][i] = if i < 6 {
                0.0
            } else if i < 12 {
                (PI / 12.0 * ((i - 6) as f64 + 0.5)).sin() as f32
            } else if i < 18 {
                1.0
            } else {
                s36
            };
            if i < 12 {
                win[2][i] = (PI / 12.0 * (i as f64 + 0.5)).sin() as f32;
            }
        }
        Imdct { cos36, cos12, win }
    })
}

/// IMDCT + overlap-add + frequency inversion for one granule/channel.
fn hybrid(g: &Granule, xr: &mut [f32; 576], overlap: &mut [f32; 576]) {
    let t = imdct_tables();
    for sb in 0..32 {
        let x = &xr[sb * 18..sb * 18 + 18];
        let mut y = [0f32; 36];
        let long_here = !g.short() || (g.mixed && sb < 2);
        if long_here {
            let bt = if g.short() {
                0
            } else {
                usize::from(g.block_type)
            };
            for (i, yi) in y.iter_mut().enumerate() {
                let row = &t.cos36[i * 18..i * 18 + 18];
                let s: f32 = row.iter().zip(x).map(|(c, v)| c * v).sum();
                *yi = s * t.win[bt][i];
            }
        } else {
            for w in 0..3 {
                for i in 0..12 {
                    let row = &t.cos12[i * 6..i * 6 + 6];
                    let mut s = 0.0;
                    for (k, c) in row.iter().enumerate() {
                        s += c * x[3 * k + w];
                    }
                    y[6 + 6 * w + i] += s * t.win[2][i];
                }
            }
        }
        let ov = &mut overlap[sb * 18..sb * 18 + 18];
        let out = &mut xr[sb * 18..sb * 18 + 18];
        for i in 0..18 {
            out[i] = y[i] + ov[i];
            ov[i] = y[18 + i];
        }
        if sb % 2 == 1 {
            for i in (1..18).step_by(2) {
                out[i] = -out[i];
            }
        }
    }
}

/// One channel's polyphase synthesis: the standard's 1024-value FIFO `V`,
/// stored circularly (`V[n]` is `v[(off + n) % 1024]`).
struct Synth {
    v: [f32; 1024],
    off: usize,
}

struct SynthTables {
    n: Vec<f32>,
    d: [f32; 512],
}

fn synth_tables() -> &'static SynthTables {
    static T: OnceLock<SynthTables> = OnceLock::new();
    T.get_or_init(|| {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss,
            reason = "f32 tables"
        )]
        let n = (0..64 * 32)
            .map(|x| {
                let (i, k) = (x / 32, x % 32);
                ((16.0 + i as f64) * (2.0 * k as f64 + 1.0) * std::f64::consts::PI / 64.0).cos()
                    as f32
            })
            .collect();
        let mut d = [0f32; 512];
        for i in 0..257 {
            #[expect(clippy::cast_precision_loss, reason = "window values")]
            let v = ENWINDOW[i] as f32 / 65_536.0;
            d[i] = v;
            if i != 0 {
                d[512 - i] = if i % 64 != 0 { -v } else { v };
            }
        }
        SynthTables { n, d }
    })
}

impl Synth {
    const fn new() -> Self {
        Self {
            v: [0.0; 1024],
            off: 0,
        }
    }

    /// 32 subband samples in → 32 PCM samples out.
    fn run(&mut self, s: &[f32; 32], out: &mut [f32; 32]) {
        let t = synth_tables();
        // Shift V by 64: the newest 64 values become V[0..64].
        self.off = (self.off + 1024 - 64) % 1024;
        let base = self.off;
        for i in 0..64 {
            let row = &t.n[i * 32..i * 32 + 32];
            self.v[(base + i) % 1024] = row.iter().zip(s).map(|(a, b)| a * b).sum();
        }
        for (j, o) in out.iter_mut().enumerate() {
            let mut sum = 0.0;
            for i in 0..8 {
                // U[64i + j] = V[128i + j]; U[64i + 32 + j] = V[128i + 96 + j].
                let a = self.v[(base + 128 * i + j) % 1024];
                let b = self.v[(base + 128 * i + 96 + j) % 1024];
                sum += a * t.d[64 * i + j] + b * t.d[64 * i + 32 + j];
            }
            *o = sum;
        }
    }
}

/// LAME/Xing gapless information.
#[derive(Debug, Clone, Copy, Default)]
struct Gapless {
    frames: Option<u32>,
    delay: usize,
    padding: usize,
}

fn xing(frame: &[u8], h: &Header) -> Option<Gapless> {
    let start = 4 + h.side_len() + if h.crc { 2 } else { 0 };
    let b = frame.get(start..)?;
    if !(b.starts_with(b"Xing") || b.starts_with(b"Info")) {
        return None;
    }
    let flags = u32::from_be_bytes([b[4], b[5], b[6], b[7]]);
    let mut at = 8;
    let mut g = Gapless::default();
    if flags & 1 != 0 {
        g.frames = Some(u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]));
        at += 4;
    }
    if flags & 2 != 0 {
        at += 4;
    }
    if flags & 4 != 0 {
        at += 100;
    }
    if flags & 8 != 0 {
        at += 4;
    }
    if let Some(l) = b.get(at..at + 24) {
        if l.starts_with(b"LAME")
            || l.starts_with(b"Lavc")
            || l.starts_with(b"Lavf")
            || l.starts_with(b"L3.9")
        {
            g.delay = (usize::from(l[21]) << 4) | usize::from(l[22] >> 4);
            g.padding = (usize::from(l[22] & 0x0F) << 8) | usize::from(l[23]);
        }
    }
    Some(g)
}

/// Decode an MP3 stream.
///
/// # Errors
/// [`CodecError::NotThisFormat`] when no Layer III frame is found.
#[expect(clippy::too_many_lines, reason = "the frame loop, top to bottom")]
pub fn decode(bytes: &[u8]) -> Result<Decoded, CodecError> {
    let mut at = 0;
    if bytes.starts_with(b"ID3") && bytes.len() >= 10 {
        let size = (usize::from(bytes[6] & 0x7F) << 21)
            | (usize::from(bytes[7] & 0x7F) << 14)
            | (usize::from(bytes[8] & 0x7F) << 7)
            | usize::from(bytes[9] & 0x7F);
        at = 10 + size + if bytes[5] & 0x10 != 0 { 10 } else { 0 };
    }
    // First frame: a header whose successor also parses.
    let mut first = None;
    while at + 4 <= bytes.len() {
        if let Some(h) = header(&bytes[at..]) {
            let next = at + h.len;
            if next + 4 > bytes.len() || header(&bytes[next..]).is_some() {
                first = Some(h);
                break;
            }
        }
        at += 1;
    }
    let fh = first.ok_or(CodecError::NotThisFormat)?;
    let nch = fh.channels;
    let mut gap = Gapless::default();
    if let Some(g) = xing(&bytes[at..(at + fh.len).min(bytes.len())], &fh) {
        gap = g;
        at += fh.len;
    }
    let mut pcm: Vec<f32> = Vec::new();
    let mut reservoir: Vec<u8> = Vec::new();
    let mut overlap = [[0f32; 576]; 2];
    let mut synth = [Synth::new(), Synth::new()];
    let mut prev_scf = [Scf::default(); 2];
    let mut frames = 0u32;
    while at + 4 <= bytes.len() {
        let Some(h) = header(&bytes[at..]) else {
            at += 1;
            continue;
        };
        if h.channels != nch || h.rate != fh.rate {
            at += 1;
            continue;
        }
        let end = (at + h.len).min(bytes.len());
        let frame = &bytes[at..end];
        let hdr_len = 4 + if h.crc { 2 } else { 0 };
        if frame.len() < hdr_len + h.side_len() {
            break;
        }
        let mut sr = MsbReader::new(&frame[hdr_len..hdr_len + h.side_len()]);
        let Some(side) = side_info(&mut sr, &h) else {
            at = end;
            continue;
        };
        let main = &frame[hdr_len + h.side_len()..];
        let enough = reservoir.len() >= side.main_data_begin;
        let start = reservoir.len().saturating_sub(side.main_data_begin);
        let mut buf = reservoir[start..].to_vec();
        buf.extend_from_slice(main);
        reservoir.extend_from_slice(main);
        if reservoir.len() > 4096 {
            let cut = reservoir.len() - 2048;
            reservoir.drain(..cut);
        }
        at = end;
        frames += 1;
        let ngr = if h.mpeg1 { 2 } else { 1 };
        if !enough {
            // The first frames after a cut cannot be decoded: emit silence
            // so the timeline stays aligned.
            pcm.extend(std::iter::repeat_n(0.0, h.samples() * nch));
            continue;
        }
        let mut r = MsbReader::new(&buf);
        for gi in 0..ngr {
            let mut xr = [[0f32; 576]; 2];
            let mut raw = [[0i32; 576]; 2];
            let mut scfs = [Scf::default(); 2];
            let mut ill_l = [-1i32; 22];
            let mut ill_s = [-1i32; 13];
            for ch in 0..nch {
                let mut g = side.gr[gi][ch];
                let part_start = r.position();
                let part_end = part_start + g.part2_3;
                let sf = if h.mpeg1 {
                    read_scf_mpeg1(&mut r, &g, &side.scfsi[ch], gi, &prev_scf[ch])
                } else {
                    let ir = ch == 1 && h.mode == 1 && h.mode_ext & 1 != 0;
                    read_scf_lsf(&mut r, &g, ir).map(|(s, il, is, pre)| {
                        if ch == 1 {
                            ill_l = il;
                            ill_s = is;
                        }
                        g.preflag = pre;
                        s
                    })
                };
                let Some(sf) = sf else { break };
                let is = huffman(&mut r, &g, h.sr_idx, part_end).unwrap_or([0; 576]);
                r.seek(part_end);
                requantise(&is, &g, &sf, h.sr_idx, h.mpeg1, &mut xr[ch]);
                raw[ch] = is;
                scfs[ch] = sf;
                prev_scf[ch] = sf;
            }
            if nch == 2 {
                let g = side.gr[gi][1];
                let (a, b) = xr.split_at_mut(1);
                stereo(
                    &h, &g, &mut a[0], &mut b[0], &scfs[1], &ill_l, &ill_s, &raw[1],
                );
            }
            let mut granule_pcm = vec![0f32; 576 * nch];
            for ch in 0..nch {
                let g = side.gr[gi][ch];
                reorder(&g, h.sr_idx, h.mpeg1, &mut xr[ch]);
                antialias(&g, &mut xr[ch]);
                hybrid(&g, &mut xr[ch], &mut overlap[ch]);
                for t in 0..18 {
                    let mut s = [0f32; 32];
                    for (sb, v) in s.iter_mut().enumerate() {
                        *v = xr[ch][sb * 18 + t];
                    }
                    let mut o = [0f32; 32];
                    synth[ch].run(&s, &mut o);
                    for (j, v) in o.iter().enumerate() {
                        granule_pcm[(t * 32 + j) * nch + ch] = *v;
                    }
                }
            }
            pcm.extend_from_slice(&granule_pcm);
        }
    }
    // Gapless trimming.
    let spf = fh.samples();
    let decoder_delay = 529;
    let mut start = 0;
    let mut total = pcm.len() / nch;
    if gap.delay > 0 || gap.padding > 0 {
        start = gap.delay + decoder_delay;
        let frames_n = gap.frames.map_or(frames as usize, |f| f as usize);
        total = (frames_n * spf).saturating_sub(gap.delay + gap.padding);
    }
    let s = (start * nch).min(pcm.len());
    let e = (s + total * nch).min(pcm.len());
    Ok(Decoded {
        data: pcm[s..e].to_vec(),
        channels: u8::try_from(nch).unwrap_or(2),
        rate: fh.rate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{s16le, snr_db};

    #[test]
    fn huffman_tables_are_prefix_free_and_complete() {
        let t = trees();
        for (n, codes) in HUFF.iter().enumerate() {
            if codes.is_empty() {
                continue;
            }
            // Kraft equality: a complete prefix code sums to exactly 1.
            let k: f64 = codes.iter().map(|c| 0.5f64.powi(i32::from(c.1))).sum();
            assert!((k - 1.0).abs() < 1e-12, "table {n}: Kraft sum {k}");
            for &(code, len, x, y) in *codes {
                let bits: Vec<u8> = (0..len)
                    .rev()
                    .map(|i| u8::try_from((code >> i) & 1).unwrap())
                    .collect();
                let mut packed = Vec::new();
                for chunk in bits.chunks(8) {
                    let mut b = 0u8;
                    for (i, v) in chunk.iter().enumerate() {
                        b |= v << (7 - i);
                    }
                    packed.push(b);
                }
                packed.push(0);
                let mut r = MsbReader::new(&packed);
                assert_eq!(huff_pair(&mut r, &t[n]), Some((i32::from(x), i32::from(y))));
            }
        }
    }

    #[test]
    fn headers_parse_rates_and_lengths() {
        // 128 kb/s, 44.1 kHz, MPEG-1, no padding: 417 bytes.
        let h = header(&[0xFF, 0xFB, 0x90, 0x64]).unwrap();
        assert!(h.mpeg1);
        assert_eq!((h.rate, h.len, h.channels), (44_100, 417, 2));
        // MPEG-2 64 kb/s 22.05 kHz mono.
        let h = header(&[0xFF, 0xF3, 0x80, 0xC4]).unwrap();
        assert_eq!((h.rate, h.channels, h.samples()), (22_050, 1, 576));
        assert!(
            header(&[0xFF, 0xFD, 0x90, 0x64]).is_none(),
            "layer II refused"
        );
    }

    #[test]
    fn decodes_lame_joint_stereo_like_ffmpeg() {
        let d = decode(include_bytes!("../../tests/fixtures/src.mp3")).unwrap();
        let reference = s16le(include_bytes!("../../tests/fixtures/src.mp3.ref"));
        assert_eq!((d.channels, d.rate), (2, 44_100));
        assert_eq!(d.data.len(), reference.len(), "gapless length matches");
        let (snr, lag) = snr_db(&reference, &d.data, 2, 4);
        assert_eq!(lag, 0, "sample-aligned");
        eprintln!("mp3 joint stereo: {snr:.1} dB");
        assert!(snr > 60.0, "SNR {snr} dB against ffmpeg's decode");
    }

    #[test]
    fn decodes_mono_vbr_like_ffmpeg() {
        let d = decode(include_bytes!("../../tests/fixtures/src_mono_vbr.mp3")).unwrap();
        let reference = s16le(include_bytes!("../../tests/fixtures/src_mono_vbr.mp3.ref"));
        assert_eq!(d.channels, 1);
        assert_eq!(d.data.len(), reference.len());
        let (snr, lag) = snr_db(&reference, &d.data, 1, 4);
        assert_eq!(lag, 0);
        eprintln!("mp3 mono vbr: {snr:.1} dB");
        assert!(snr > 60.0, "SNR {snr} dB");
    }

    #[test]
    fn garbage_is_not_mp3() {
        assert_eq!(
            decode(b"definitely not audio").err(),
            Some(CodecError::NotThisFormat)
        );
    }
}
