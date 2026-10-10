//! Ogg Vorbis decoding — Vorbis I, the whole specification.
//!
//! Headers (identification, comment, setup), codebooks with scalar and
//! vector-quantised lookups (types 1 and 2), floor 0 (LSP) and floor 1
//! (piecewise-linear, with the spec's integer line renderer and inverse-dB
//! table), residues 0, 1 and 2, mappings with submaps and square-polar
//! channel coupling, the power-sine windows with long/short transitions,
//! IMDCT (the crate's shared [`crate::fft::Imdct`]), overlap-add,
//! and granule-position trimming so the output has exactly the stream's
//! sample count. Page CRCs are verified by [`super::ogg`].
//!
//! The one fixed table, `floor1_inverse_dB_table`, is the specification's
//! (values via stb_vorbis, public domain).

use super::bits::LsbReader;
use super::ogg;
use super::{CodecError, Decoded};
use crate::fft::Imdct;

#[allow(
    clippy::excessive_precision,
    reason = "the specification's table, verbatim"
)]
const INVERSE_DB: [f32; 256] = [
    1.06498630e-7,
    1.13419510e-7,
    1.20790150e-7,
    1.28639780e-7,
    1.36999510e-7,
    1.45902510e-7,
    1.55384080e-7,
    1.65481810e-7,
    1.76235750e-7,
    1.87688550e-7,
    1.99885610e-7,
    2.12875300e-7,
    2.26709130e-7,
    2.41441970e-7,
    2.57132230e-7,
    2.73842130e-7,
    2.91637930e-7,
    3.10590210e-7,
    3.30774110e-7,
    3.52269680e-7,
    3.75162140e-7,
    3.99542290e-7,
    4.25506800e-7,
    4.53158630e-7,
    4.82607430e-7,
    5.13969980e-7,
    5.47370650e-7,
    5.82941870e-7,
    6.20824720e-7,
    6.61169410e-7,
    7.04135920e-7,
    7.49894640e-7,
    7.98627010e-7,
    8.50526300e-7,
    9.05798280e-7,
    9.64662160e-7,
    1.02735130e-6,
    1.09411440e-6,
    1.16521610e-6,
    1.24093840e-6,
    1.32158160e-6,
    1.40746540e-6,
    1.49893050e-6,
    1.59633940e-6,
    1.70007850e-6,
    1.81055920e-6,
    1.92821950e-6,
    2.05352610e-6,
    2.18697580e-6,
    2.32909780e-6,
    2.48045570e-6,
    2.64164970e-6,
    2.81331900e-6,
    2.99614430e-6,
    3.19085060e-6,
    3.39821010e-6,
    3.61904490e-6,
    3.85423080e-6,
    4.10470040e-6,
    4.37144700e-6,
    4.65552820e-6,
    4.95807070e-6,
    5.28027400e-6,
    5.62341600e-6,
    5.98885720e-6,
    6.37804690e-6,
    6.79252830e-6,
    7.23394510e-6,
    7.70404760e-6,
    8.20470000e-6,
    8.73788760e-6,
    9.30572480e-6,
    9.91046320e-6,
    1.05545010e-5,
    1.12403920e-5,
    1.19708560e-5,
    1.27487890e-5,
    1.35772780e-5,
    1.44596060e-5,
    1.53992720e-5,
    1.64000040e-5,
    1.74657680e-5,
    1.86007920e-5,
    1.98095760e-5,
    2.10969140e-5,
    2.24679110e-5,
    2.39280020e-5,
    2.54829780e-5,
    2.71390060e-5,
    2.89026510e-5,
    3.07809080e-5,
    3.27812250e-5,
    3.49115340e-5,
    3.71802820e-5,
    3.95964660e-5,
    4.21696670e-5,
    4.49100900e-5,
    4.78286010e-5,
    5.09367730e-5,
    5.42469310e-5,
    5.77722020e-5,
    6.15265650e-5,
    6.55249080e-5,
    6.97830850e-5,
    7.43179830e-5,
    7.91475850e-5,
    8.42910400e-5,
    8.97687470e-5,
    9.56024260e-5,
    1.01815210e-4,
    1.08431740e-4,
    1.15478240e-4,
    1.22982670e-4,
    1.30974770e-4,
    1.39486250e-4,
    1.48550850e-4,
    1.58204530e-4,
    1.68485550e-4,
    1.79434690e-4,
    1.91095360e-4,
    2.03513820e-4,
    2.16739290e-4,
    2.30824230e-4,
    2.45824490e-4,
    2.61799550e-4,
    2.78812760e-4,
    2.96931580e-4,
    3.16227870e-4,
    3.36778140e-4,
    3.58663880e-4,
    3.81971880e-4,
    4.06794560e-4,
    4.33230360e-4,
    4.61384110e-4,
    4.91367450e-4,
    5.23299270e-4,
    5.57306210e-4,
    5.93523110e-4,
    6.32093580e-4,
    6.73170580e-4,
    7.16917000e-4,
    7.63506300e-4,
    8.13123240e-4,
    8.65964570e-4,
    9.22239830e-4,
    9.82172160e-4,
    1.04599920e-3,
    1.11397420e-3,
    1.18636650e-3,
    1.26346330e-3,
    1.34557020e-3,
    1.43301290e-3,
    1.52613820e-3,
    1.62531530e-3,
    1.73093740e-3,
    1.84342350e-3,
    1.96321950e-3,
    2.09080060e-3,
    2.22667260e-3,
    2.37137430e-3,
    2.52547950e-3,
    2.68959940e-3,
    2.86438470e-3,
    3.05052860e-3,
    3.24876910e-3,
    3.45989250e-3,
    3.68473580e-3,
    3.92419060e-3,
    4.17920660e-3,
    4.45079500e-3,
    4.74003280e-3,
    5.04806680e-3,
    5.37611860e-3,
    5.72548910e-3,
    6.09756360e-3,
    6.49381760e-3,
    6.91582250e-3,
    7.36525160e-3,
    7.84388710e-3,
    8.35362710e-3,
    8.89649280e-3,
    9.47463700e-3,
    1.00903520e-2,
    1.07460800e-2,
    1.14444210e-2,
    1.21881440e-2,
    1.29801980e-2,
    1.38237250e-2,
    1.47220680e-2,
    1.56787910e-2,
    1.66976870e-2,
    1.77827970e-2,
    1.89384230e-2,
    2.01691490e-2,
    2.14798540e-2,
    2.28757350e-2,
    2.43623300e-2,
    2.59455310e-2,
    2.76316180e-2,
    2.94272760e-2,
    3.13396260e-2,
    3.33762520e-2,
    3.55452280e-2,
    3.78551570e-2,
    4.03151990e-2,
    4.29351080e-2,
    4.57252730e-2,
    4.86967580e-2,
    5.18613480e-2,
    5.52315910e-2,
    5.88208500e-2,
    6.26433610e-2,
    6.67142790e-2,
    7.10497490e-2,
    7.56669620e-2,
    8.05842270e-2,
    8.58210440e-2,
    9.13981790e-2,
    9.73377470e-2,
    1.03663300e-1,
    1.10399930e-1,
    1.17574340e-1,
    1.25214980e-1,
    1.33352150e-1,
    1.42018130e-1,
    1.51247270e-1,
    1.61076170e-1,
    1.71543800e-1,
    1.82691680e-1,
    1.94564020e-1,
    2.07207880e-1,
    2.20673420e-1,
    2.35014020e-1,
    2.50286560e-1,
    2.66551590e-1,
    2.83873610e-1,
    3.02321320e-1,
    3.21967860e-1,
    3.42891140e-1,
    3.65174140e-1,
    3.88905210e-1,
    4.14178470e-1,
    4.41094120e-1,
    4.69758900e-1,
    5.00286480e-1,
    5.32797910e-1,
    5.67422120e-1,
    6.04296400e-1,
    6.43566990e-1,
    6.85389590e-1,
    7.29930070e-1,
    7.77365040e-1,
    8.27882600e-1,
    8.81683070e-1,
    9.38979800e-1,
    1.00000000,
];

fn ilog(v: u32) -> u32 {
    32 - v.leading_zeros()
}

fn float32(v: u32) -> f32 {
    let mantissa = f64::from(v & 0x1F_FFFF);
    let sign = if v & 0x8000_0000 != 0 { -1.0 } else { 1.0 };
    let exp = i32::try_from((v & 0x7FE0_0000) >> 21).unwrap_or(0);
    #[expect(clippy::cast_possible_truncation, reason = "the spec's float32")]
    let out = (sign * mantissa * 2f64.powi(exp - 788)) as f32;
    out
}

type R<'a> = LsbReader<'a>;

fn rd(r: &mut R<'_>, n: u32) -> Result<u32, CodecError> {
    r.read(n).ok_or(CodecError::Truncated)
}

#[derive(Debug, Clone)]
struct Codebook {
    dims: usize,
    /// Huffman tree: node pairs; leaf = !entry.
    tree: Vec<[i32; 2]>,
    /// VQ values, `entries × dims`, if the book has a lookup.
    vq: Option<Vec<f32>>,
    single: Option<i32>,
}

impl Codebook {
    fn parse(r: &mut R<'_>) -> Result<Self, CodecError> {
        if rd(r, 24)? != 0x56_4342 {
            return Err(CodecError::Malformed("codebook sync"));
        }
        let dims = rd(r, 16)? as usize;
        let entries = rd(r, 24)? as usize;
        let ordered = rd(r, 1)? == 1;
        let mut lens = vec![0u8; entries];
        if ordered {
            let mut cur = rd(r, 5)? + 1;
            let mut i = 0;
            while i < entries {
                let n = rd(r, ilog(u32::try_from(entries - i).unwrap_or(0)))? as usize;
                if i + n > entries {
                    return Err(CodecError::Malformed("ordered lengths"));
                }
                for l in &mut lens[i..i + n] {
                    *l = u8::try_from(cur).unwrap_or(0);
                }
                i += n;
                cur += 1;
            }
        } else {
            let sparse = rd(r, 1)? == 1;
            for l in &mut lens {
                if !sparse || rd(r, 1)? == 1 {
                    *l = u8::try_from(rd(r, 5)? + 1).unwrap_or(0);
                }
            }
        }
        let lookup = rd(r, 4)?;
        let vq = match lookup {
            0 => None,
            1 | 2 => {
                let min = float32(rd(r, 32)?);
                let delta = float32(rd(r, 32)?);
                let bits = rd(r, 4)? + 1;
                let seq = rd(r, 1)? == 1;
                let nvals = if lookup == 1 {
                    // The largest n with n^dims ≤ entries.
                    let mut n = 0usize;
                    while (n + 1)
                        .checked_pow(u32::try_from(dims).unwrap_or(1))
                        .is_some_and(|p| p <= entries)
                    {
                        n += 1;
                    }
                    n
                } else {
                    entries * dims
                };
                let mults: Vec<u32> = (0..nvals).map(|_| rd(r, bits)).collect::<Result<_, _>>()?;
                let mut out = vec![0f32; entries * dims];
                for e in 0..entries {
                    let mut last = 0.0f32;
                    let mut div = 1usize;
                    for d in 0..dims {
                        let idx = if lookup == 1 {
                            let i = (e / div) % nvals.max(1);
                            div = div.saturating_mul(nvals.max(1));
                            i
                        } else {
                            e * dims + d
                        };
                        #[expect(clippy::cast_precision_loss, reason = "multiplicands are small")]
                        let v = mults.get(idx).copied().unwrap_or(0) as f32 * delta + min + last;
                        out[e * dims + d] = v;
                        if seq {
                            last = v;
                        }
                    }
                }
                Some(out)
            }
            _ => return Err(CodecError::Malformed("codebook lookup type")),
        };
        let (tree, single) = build_tree(&lens)?;
        Ok(Self {
            dims,
            tree,
            vq,
            single,
        })
    }

    fn scalar(&self, r: &mut R<'_>) -> Result<usize, CodecError> {
        if let Some(e) = self.single {
            return usize::try_from(e).map_err(|_| CodecError::Malformed("entry"));
        }
        let mut n = 0usize;
        loop {
            let b = usize::from(r.bit().ok_or(CodecError::Truncated)?);
            let v = self.tree.get(n).ok_or(CodecError::Malformed("tree"))?[b];
            if v < 0 {
                return usize::try_from(!v).map_err(|_| CodecError::Malformed("entry"));
            }
            if v == 0 {
                return Err(CodecError::Malformed("undefined codeword"));
            }
            n = usize::try_from(v).map_err(|_| CodecError::Malformed("tree"))?;
        }
    }

    fn vector(&self, r: &mut R<'_>) -> Result<&[f32], CodecError> {
        let e = self.scalar(r)?;
        let vq = self
            .vq
            .as_ref()
            .ok_or(CodecError::Malformed("scalar book used as VQ"))?;
        vq.get(e * self.dims..(e + 1) * self.dims)
            .ok_or(CodecError::Malformed("VQ entry"))
    }
}

/// The spec's codeword assignment (§3.2.1), as a tree.
fn build_tree(lens: &[u8]) -> Result<(Vec<[i32; 2]>, Option<i32>), CodecError> {
    let used: Vec<usize> = (0..lens.len()).filter(|i| lens[*i] > 0).collect();
    if used.len() == 1 {
        return Ok((vec![[0, 0]], Some(i32::try_from(used[0]).unwrap_or(0))));
    }
    let mut nodes = vec![[0i32; 2]];
    let mut available = [0u32; 33];
    let mut first = true;
    for &e in &used {
        let len = usize::from(lens[e]);
        let code: u32;
        if first {
            first = false;
            code = 0;
            for (i, a) in available.iter_mut().enumerate().take(len + 1).skip(1) {
                *a = 1u32.wrapping_shl(32 - u32::try_from(i).unwrap_or(32));
            }
        } else {
            let mut z = len;
            while z > 0 && available[z] == 0 {
                z -= 1;
            }
            if z == 0 {
                return Err(CodecError::Malformed("overspecified codebook"));
            }
            let res = available[z];
            available[z] = 0;
            for i in (z + 1..=len).rev() {
                available[i] =
                    res.wrapping_add(1u32.wrapping_shl(32 - u32::try_from(i).unwrap_or(32)));
            }
            code = res;
        }
        // `code` is left-aligned; insert its top `len` bits, MSB first.
        let mut n = 0usize;
        for i in 0..len {
            let bit = ((code >> (31 - i)) & 1) as usize;
            if i + 1 == len {
                nodes[n][bit] = !i32::try_from(e).unwrap_or(0);
            } else {
                if nodes[n][bit] <= 0 {
                    nodes.push([0, 0]);
                    nodes[n][bit] = i32::try_from(nodes.len() - 1).unwrap_or(0);
                }
                n = usize::try_from(nodes[n][bit]).unwrap_or(0);
            }
        }
    }
    Ok((nodes, None))
}

#[derive(Debug, Clone)]
enum Floor {
    Zero {
        order: usize,
        rate: u32,
        bark_size: u32,
        amp_bits: u32,
        amp_offset: u32,
        books: Vec<usize>,
    },
    One {
        partition_class: Vec<usize>,
        class_dims: Vec<usize>,
        class_sub: Vec<u32>,
        class_master: Vec<usize>,
        sub_books: Vec<Vec<i32>>,
        multiplier: u32,
        xs: Vec<u32>,
    },
}

#[derive(Debug, Clone)]
struct Residue {
    kind: u32,
    begin: usize,
    end: usize,
    part_size: usize,
    classes: usize,
    classbook: usize,
    books: Vec<[i32; 8]>,
}

#[derive(Debug, Clone)]
struct Mapping {
    coupling: Vec<(usize, usize)>,
    mux: Vec<usize>,
    submaps: Vec<(usize, usize)>, // (floor, residue)
}

#[derive(Debug, Clone, Copy)]
struct Mode {
    long: bool,
    mapping: usize,
}

struct Setup {
    channels: usize,
    rate: u32,
    bs: [usize; 2],
    books: Vec<Codebook>,
    floors: Vec<Floor>,
    residues: Vec<Residue>,
    maps: Vec<Mapping>,
    modes: Vec<Mode>,
}

fn parse_setup(id: &[u8], setup: &[u8]) -> Result<Setup, CodecError> {
    if id.len() < 30 || id[0] != 1 || &id[1..7] != b"vorbis" {
        return Err(CodecError::NotThisFormat);
    }
    let channels = usize::from(id[11]);
    let rate = u32::from_le_bytes([id[12], id[13], id[14], id[15]]);
    let bs = [1usize << (id[28] & 0xF), 1usize << (id[28] >> 4)];
    if channels == 0 || rate == 0 || bs[0] < 64 || bs[1] < bs[0] || bs[1] > 8192 {
        return Err(CodecError::Malformed("identification header"));
    }
    if setup.len() < 7 || setup[0] != 5 || &setup[1..7] != b"vorbis" {
        return Err(CodecError::Malformed("setup header"));
    }
    let mut r = R::new(&setup[7..]);
    let nbooks = rd(&mut r, 8)? as usize + 1;
    let books = (0..nbooks)
        .map(|_| Codebook::parse(&mut r))
        .collect::<Result<Vec<_>, _>>()?;
    let ntime = rd(&mut r, 6)? + 1;
    for _ in 0..ntime {
        if rd(&mut r, 16)? != 0 {
            return Err(CodecError::Malformed("time domain transform"));
        }
    }
    let nfloors = rd(&mut r, 6)? as usize + 1;
    let mut floors = Vec::with_capacity(nfloors);
    for _ in 0..nfloors {
        match rd(&mut r, 16)? {
            0 => {
                let order = rd(&mut r, 8)? as usize;
                let rate = rd(&mut r, 16)?;
                let bark_size = rd(&mut r, 16)?;
                let amp_bits = rd(&mut r, 6)?;
                let amp_offset = rd(&mut r, 8)?;
                let n = rd(&mut r, 4)? as usize + 1;
                let books = (0..n)
                    .map(|_| rd(&mut r, 8).map(|v| v as usize))
                    .collect::<Result<_, _>>()?;
                floors.push(Floor::Zero {
                    order,
                    rate,
                    bark_size,
                    amp_bits,
                    amp_offset,
                    books,
                });
            }
            1 => {
                let parts = rd(&mut r, 5)? as usize;
                let partition_class: Vec<usize> = (0..parts)
                    .map(|_| rd(&mut r, 4).map(|v| v as usize))
                    .collect::<Result<_, _>>()?;
                let nclass = partition_class.iter().copied().max().map_or(0, |m| m + 1);
                let mut class_dims = Vec::new();
                let mut class_sub = Vec::new();
                let mut class_master = Vec::new();
                let mut sub_books = Vec::new();
                for _ in 0..nclass {
                    class_dims.push(rd(&mut r, 3)? as usize + 1);
                    let sub = rd(&mut r, 2)?;
                    class_sub.push(sub);
                    class_master.push(if sub > 0 { rd(&mut r, 8)? as usize } else { 0 });
                    let sb = (0..1usize << sub)
                        .map(|_| rd(&mut r, 8).map(|v| i32::try_from(v).unwrap_or(0) - 1))
                        .collect::<Result<_, _>>()?;
                    sub_books.push(sb);
                }
                let multiplier = rd(&mut r, 2)? + 1;
                let rangebits = rd(&mut r, 4)?;
                let mut xs = vec![0, 1 << rangebits];
                for &c in &partition_class {
                    for _ in 0..class_dims[c] {
                        xs.push(rd(&mut r, rangebits)?);
                    }
                }
                floors.push(Floor::One {
                    partition_class,
                    class_dims,
                    class_sub,
                    class_master,
                    sub_books,
                    multiplier,
                    xs,
                });
            }
            _ => return Err(CodecError::Malformed("floor type")),
        }
    }
    let nres = rd(&mut r, 6)? as usize + 1;
    let mut residues = Vec::with_capacity(nres);
    for _ in 0..nres {
        let kind = rd(&mut r, 16)?;
        if kind > 2 {
            return Err(CodecError::Malformed("residue type"));
        }
        let begin = rd(&mut r, 24)? as usize;
        let end = rd(&mut r, 24)? as usize;
        let part_size = rd(&mut r, 24)? as usize + 1;
        let classes = rd(&mut r, 6)? as usize + 1;
        let classbook = rd(&mut r, 8)? as usize;
        let mut cascade = Vec::with_capacity(classes);
        for _ in 0..classes {
            let low = rd(&mut r, 3)?;
            let high = if rd(&mut r, 1)? == 1 {
                rd(&mut r, 5)?
            } else {
                0
            };
            cascade.push(high * 8 + low);
        }
        let mut books = Vec::with_capacity(classes);
        for c in cascade {
            let mut b = [-1i32; 8];
            for (j, slot) in b.iter_mut().enumerate() {
                if c & (1 << j) != 0 {
                    *slot = i32::try_from(rd(&mut r, 8)?).unwrap_or(-1);
                }
            }
            books.push(b);
        }
        residues.push(Residue {
            kind,
            begin,
            end,
            part_size,
            classes,
            classbook,
            books,
        });
    }
    let nmaps = rd(&mut r, 6)? as usize + 1;
    let mut maps = Vec::with_capacity(nmaps);
    for _ in 0..nmaps {
        if rd(&mut r, 16)? != 0 {
            return Err(CodecError::Malformed("mapping type"));
        }
        let nsub = if rd(&mut r, 1)? == 1 {
            rd(&mut r, 4)? as usize + 1
        } else {
            1
        };
        let mut coupling = Vec::new();
        if rd(&mut r, 1)? == 1 {
            let steps = rd(&mut r, 8)? as usize + 1;
            let bits = ilog(u32::try_from(channels - 1).unwrap_or(0));
            for _ in 0..steps {
                let m = rd(&mut r, bits)? as usize;
                let a = rd(&mut r, bits)? as usize;
                if m == a || m >= channels || a >= channels {
                    return Err(CodecError::Malformed("coupling"));
                }
                coupling.push((m, a));
            }
        }
        if rd(&mut r, 2)? != 0 {
            return Err(CodecError::Malformed("mapping reserved"));
        }
        let mux = if nsub > 1 {
            (0..channels)
                .map(|_| rd(&mut r, 4).map(|v| v as usize))
                .collect::<Result<_, _>>()?
        } else {
            vec![0; channels]
        };
        let mut submaps = Vec::with_capacity(nsub);
        for _ in 0..nsub {
            rd(&mut r, 8)?;
            let f = rd(&mut r, 8)? as usize;
            let res = rd(&mut r, 8)? as usize;
            submaps.push((f, res));
        }
        maps.push(Mapping {
            coupling,
            mux,
            submaps,
        });
    }
    let nmodes = rd(&mut r, 6)? as usize + 1;
    let mut modes = Vec::with_capacity(nmodes);
    for _ in 0..nmodes {
        let long = rd(&mut r, 1)? == 1;
        rd(&mut r, 16)?;
        rd(&mut r, 16)?;
        let mapping = rd(&mut r, 8)? as usize;
        modes.push(Mode { long, mapping });
    }
    if rd(&mut r, 1)? != 1 {
        return Err(CodecError::Malformed("setup framing"));
    }
    Ok(Setup {
        channels,
        rate,
        bs,
        books,
        floors,
        residues,
        maps,
        modes,
    })
}

/// Floor 1 packet data: `None` = unused this packet.
fn floor1_decode(
    r: &mut R<'_>,
    f: &Floor,
    books: &[Codebook],
) -> Result<Option<Vec<i32>>, CodecError> {
    let Floor::One {
        partition_class,
        class_dims,
        class_sub,
        class_master,
        sub_books,
        multiplier,
        xs,
    } = f
    else {
        return Ok(None);
    };
    if !r.bit().ok_or(CodecError::Truncated)? {
        return Ok(None);
    }
    let range = [256u32, 128, 86, 64][(*multiplier - 1) as usize];
    let bits = ilog(range - 1);
    let mut y = Vec::with_capacity(xs.len());
    y.push(i32::try_from(rd(r, bits)?).unwrap_or(0));
    y.push(i32::try_from(rd(r, bits)?).unwrap_or(0));
    for &c in partition_class {
        let cdim = class_dims[c];
        let cbits = class_sub[c];
        let csub = (1u32 << cbits) - 1;
        let mut cval = if cbits > 0 {
            u32::try_from(
                books
                    .get(class_master[c])
                    .ok_or(CodecError::Malformed("book"))?
                    .scalar(r)?,
            )
            .unwrap_or(0)
        } else {
            0
        };
        for _ in 0..cdim {
            let book = sub_books[c][(cval & csub) as usize];
            cval >>= cbits;
            y.push(if book >= 0 {
                let b = books
                    .get(usize::try_from(book).unwrap_or(0))
                    .ok_or(CodecError::Malformed("book"))?;
                i32::try_from(b.scalar(r)?).unwrap_or(0)
            } else {
                0
            });
        }
    }
    Ok(Some(y))
}

fn render_point(x0: i32, y0: i32, x1: i32, y1: i32, x: i32) -> i32 {
    let dy = y1 - y0;
    let adx = x1 - x0;
    let ady = dy.abs();
    let err = ady * (x - x0);
    let off = err / adx;
    if dy < 0 {
        y0 - off
    } else {
        y0 + off
    }
}

fn render_line(x0: i32, y0: i32, x1: i32, y1: i32, out: &mut [f32]) {
    let dy = y1 - y0;
    let adx = x1 - x0;
    let mut ady = dy.abs();
    let base = dy / adx;
    let sy = if dy < 0 { base - 1 } else { base + 1 };
    let mut x = x0;
    let mut y = y0;
    let mut err = 0;
    ady -= base.abs() * adx;
    let n = i32::try_from(out.len()).unwrap_or(0);
    if x < n {
        out[usize::try_from(x).unwrap_or(0)] *=
            INVERSE_DB[usize::try_from(y.clamp(0, 255)).unwrap_or(0)];
    }
    for _ in (x0 + 1)..x1 {
        x += 1;
        err += ady;
        if err >= adx {
            err -= adx;
            y += sy;
        } else {
            y += base;
        }
        if x < n {
            out[usize::try_from(x).unwrap_or(0)] *=
                INVERSE_DB[usize::try_from(y.clamp(0, 255)).unwrap_or(0)];
        }
    }
}

/// Apply floor 1's curve (multiplying `out`, the residue, in place).
fn floor1_apply(f: &Floor, y: &[i32], out: &mut [f32]) {
    let Floor::One { multiplier, xs, .. } = f else {
        return;
    };
    let n = xs.len();
    let range = [256i32, 128, 86, 64][(*multiplier - 1) as usize];
    let xi: Vec<i32> = xs.iter().map(|v| i32::try_from(*v).unwrap_or(0)).collect();
    let mut step2 = vec![false; n];
    let mut fy = vec![0i32; n];
    step2[0] = true;
    step2[1] = true;
    fy[0] = y[0];
    fy[1] = y[1];
    for i in 2..n {
        // Low and high neighbours among earlier points.
        let (mut lo, mut hi) = (0usize, 1usize);
        for j in 0..i {
            if xi[j] < xi[i] && xi[j] > xi[lo] {
                lo = j;
            }
            if xi[j] > xi[i] && xi[j] < xi[hi] {
                hi = j;
            }
        }
        let predicted = render_point(xi[lo], fy[lo], xi[hi], fy[hi], xi[i]);
        let val = y[i];
        let high_room = range - predicted;
        let low_room = predicted;
        let room = if high_room < low_room {
            high_room * 2
        } else {
            low_room * 2
        };
        if val != 0 {
            step2[lo] = true;
            step2[hi] = true;
            step2[i] = true;
            fy[i] = if val >= room {
                if high_room > low_room {
                    val - low_room + predicted
                } else {
                    predicted - val + high_room - 1
                }
            } else if val % 2 == 1 {
                predicted - (val + 1) / 2
            } else {
                predicted + val / 2
            };
        } else {
            step2[i] = false;
            fy[i] = predicted;
        }
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| xi[i]);
    let mult = i32::try_from(*multiplier).unwrap_or(1);
    let mut lx = 0;
    let mut ly = fy[order[0]] * mult;
    let len = i32::try_from(out.len()).unwrap_or(0);
    for &i in &order[1..] {
        if step2[i] {
            let hy = fy[i] * mult;
            let hx = xi[i];
            render_line(lx, ly, hx, hy, out);
            lx = hx;
            ly = hy;
        }
    }
    if lx < len {
        let v = INVERSE_DB[usize::try_from(ly.clamp(0, 255)).unwrap_or(0)];
        for o in &mut out[usize::try_from(lx).unwrap_or(0)..] {
            *o *= v;
        }
    }
}

/// Floor 0: decode coefficients and amplitude.
fn floor0_decode(
    r: &mut R<'_>,
    f: &Floor,
    books: &[Codebook],
) -> Result<Option<(f32, Vec<f32>)>, CodecError> {
    let Floor::Zero {
        order,
        amp_bits,
        books: fb,
        ..
    } = f
    else {
        return Ok(None);
    };
    let amp = rd(r, *amp_bits)?;
    if amp == 0 {
        return Ok(None);
    }
    let bn = rd(r, ilog(u32::try_from(fb.len()).unwrap_or(1)))? as usize;
    let book = books
        .get(*fb.get(bn).ok_or(CodecError::Malformed("floor0 book"))?)
        .ok_or(CodecError::Malformed("book"))?;
    let mut coefs = Vec::with_capacity(*order);
    let mut last = 0.0;
    while coefs.len() < *order {
        let v = book.vector(r)?.to_vec();
        for x in &v {
            coefs.push(x + last);
        }
        last = *coefs.last().unwrap_or(&0.0);
    }
    coefs.truncate(*order);
    #[expect(clippy::cast_precision_loss, reason = "amplitude")]
    Ok(Some((amp as f32, coefs)))
}

fn floor0_apply(f: &Floor, amp: f32, coefs: &[f32], out: &mut [f32]) {
    let Floor::Zero {
        rate,
        bark_size,
        amp_bits,
        amp_offset,
        ..
    } = f
    else {
        return;
    };
    let n = out.len();
    let bark =
        |x: f64| 13.1 * (0.00074 * x).atan() + 2.24 * (0.000_000_018_5 * x * x).atan() + 0.0001 * x;
    let rate = f64::from(*rate);
    let bs = f64::from(*bark_size);
    #[expect(clippy::cast_precision_loss, reason = "sizes")]
    let nf = n as f64;
    let max_amp = f64::from((1u32 << amp_bits) - 1);
    let mut i = 0;
    while i < n {
        #[expect(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the spec's map"
        )]
        let map = ((bark(rate * i as f64 / (2.0 * nf)) * bs / bark(0.5 * rate)).floor() as usize)
            .min(*bark_size as usize - 1);
        #[expect(clippy::cast_precision_loss, reason = "map index")]
        let omega = std::f64::consts::PI * map as f64 / bs;
        let cw = omega.cos();
        let order = coefs.len();
        let (mut p, mut q) = (1.0f64, 1.0f64);
        if order % 2 == 1 {
            for j in (0..order).step_by(2) {
                q *= 2.0 * (cw - f64::from(coefs[j]).cos()).powi(1);
            }
            for j in (1..order).step_by(2) {
                p *= 2.0 * (cw - f64::from(coefs[j]).cos());
            }
            p = p * p * (1.0 - cw * cw);
            q = q * q / 4.0;
        } else {
            for j in (0..order).step_by(2) {
                q *= 2.0 * (cw - f64::from(coefs[j]).cos());
            }
            for j in (1..order).step_by(2) {
                p *= 2.0 * (cw - f64::from(coefs[j]).cos());
            }
            p = p * p * (1.0 - cw) / 2.0;
            q = q * q * (1.0 + cw) / 2.0;
        }
        let linear = (0.115_129_25
            * (f64::from(amp) * f64::from(*amp_offset) / (max_amp * (p + q).sqrt())
                - f64::from(*amp_offset)))
        .exp();
        #[expect(clippy::cast_possible_truncation, reason = "f32 audio")]
        let lv = linear as f32;
        while i < n {
            #[expect(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "the spec's map"
            )]
            let m2 = ((bark(rate * i as f64 / (2.0 * nf)) * bs / bark(0.5 * rate)).floor()
                as usize)
                .min(*bark_size as usize - 1);
            if m2 != map {
                break;
            }
            out[i] *= lv;
            i += 1;
        }
    }
}

fn residue_decode(
    r: &mut R<'_>,
    res: &Residue,
    books: &[Codebook],
    n: usize,
    vecs: &mut [Vec<f32>],
    skip: &[bool],
) -> Result<(), CodecError> {
    let ch = vecs.len();
    if res.kind == 2 {
        // Interleave into one vector, decode as type 1, then split.
        if skip.iter().all(|s| *s) {
            return Ok(());
        }
        let mut one = vec![vec![0f32; n * ch]];
        residue_core(r, res, books, n * ch, &mut one, &[false], 1)?;
        for (i, v) in one[0].iter().enumerate() {
            vecs[i % ch][i / ch] += v;
        }
        return Ok(());
    }
    residue_core(r, res, books, n, vecs, skip, res.kind)
}

fn residue_core(
    r: &mut R<'_>,
    res: &Residue,
    books: &[Codebook],
    n: usize,
    vecs: &mut [Vec<f32>],
    skip: &[bool],
    kind: u32,
) -> Result<(), CodecError> {
    let begin = res.begin.min(n);
    let end = res.end.min(n);
    if end <= begin {
        return Ok(());
    }
    let ps = res.part_size;
    let parts = (end - begin) / ps;
    if parts == 0 {
        return Ok(());
    }
    let cb = books
        .get(res.classbook)
        .ok_or(CodecError::Malformed("classbook"))?;
    let per_word = cb.dims.max(1);
    let ch = vecs.len();
    let mut classes = vec![vec![0usize; parts + per_word]; ch];
    for pass in 0..8 {
        let mut p = 0;
        while p < parts {
            if pass == 0 {
                for c in 0..ch {
                    if skip[c] {
                        continue;
                    }
                    let mut word = cb.scalar(r)?;
                    for i in (0..per_word).rev() {
                        if p + i < classes[c].len() {
                            classes[c][p + i] = word % res.classes;
                        }
                        word /= res.classes;
                    }
                }
            }
            for _ in 0..per_word {
                if p >= parts {
                    break;
                }
                for c in 0..ch {
                    if skip[c] {
                        continue;
                    }
                    let cls = classes[c][p];
                    let bk = res.books.get(cls).map_or(-1, |b| b[pass]);
                    if bk < 0 {
                        continue;
                    }
                    let book = books
                        .get(usize::try_from(bk).unwrap_or(0))
                        .ok_or(CodecError::Malformed("residue book"))?;
                    let off = begin + p * ps;
                    let d = book.dims.max(1);
                    if kind == 0 {
                        let step = ps / d;
                        for i in 0..step {
                            let v = book.vector(r)?;
                            for (j, x) in v.iter().enumerate() {
                                if let Some(o) = vecs[c].get_mut(off + i + j * step) {
                                    *o += x;
                                }
                            }
                        }
                    } else {
                        let mut i = 0;
                        while i < ps {
                            let v = book.vector(r)?;
                            for (j, x) in v.iter().enumerate() {
                                if let Some(o) = vecs[c].get_mut(off + i + j) {
                                    *o += x;
                                }
                            }
                            i += d;
                        }
                    }
                }
                p += 1;
            }
        }
    }
    Ok(())
}

fn window(n: usize, left_n: usize, right_n: usize) -> Vec<f32> {
    let mut w = vec![0f32; n];
    let lc = n / 4;
    let rc = 3 * n / 4;
    let ls = lc - left_n / 2;
    let le = lc + left_n / 2;
    let rs = rc - right_n / 2;
    let re = rc + right_n / 2;
    let f = |x: f64| {
        let s = (x * std::f64::consts::FRAC_PI_2).sin();
        #[expect(clippy::cast_possible_truncation, reason = "f32 window")]
        let v = (std::f64::consts::FRAC_PI_2 * s * s).sin() as f32;
        v
    };
    for (i, wi) in w.iter_mut().enumerate() {
        #[expect(clippy::cast_precision_loss, reason = "window sizes")]
        let fi = i as f64;
        *wi = if i < ls {
            0.0
        } else if i < le {
            #[expect(clippy::cast_precision_loss, reason = "window sizes")]
            f((fi - ls as f64 + 0.5) / left_n as f64)
        } else if i < rs {
            1.0
        } else if i < re {
            #[expect(clippy::cast_precision_loss, reason = "window sizes")]
            f((fi - rs as f64 + 0.5) / right_n as f64 + 1.0)
        } else {
            0.0
        };
    }
    w
}

/// Decode an Ogg Vorbis stream.
///
/// # Errors
/// Malformed headers or packets, or a page CRC failure.
pub fn decode(bytes: &[u8]) -> Result<Decoded, CodecError> {
    let packets = ogg::packets(bytes)?;
    if packets.len() < 3 {
        return Err(CodecError::Truncated);
    }
    let s = parse_setup(&packets[0].data, &packets[2].data)?;
    let ch = s.channels;
    let imdct = [Imdct::new(s.bs[0]), Imdct::new(s.bs[1])];
    let mode_bits = ilog(u32::try_from(s.modes.len() - 1).unwrap_or(0));
    let mut out: Vec<f32> = Vec::new();
    let mut prev: Option<(Vec<Vec<f32>>, usize)> = None;
    let mut final_granule: Option<i64> = None;
    for pk in &packets[3..] {
        if pk.last_page && pk.granule >= 0 {
            final_granule = Some(pk.granule);
        }
        let mut r = R::new(&pk.data);
        if r.bit() != Some(false) {
            continue; // not an audio packet
        }
        let Some(mi) = r.read(mode_bits) else {
            continue;
        };
        let Some(mode) = s.modes.get(mi as usize).copied() else {
            continue;
        };
        let n = s.bs[usize::from(mode.long)];
        let (prev_long, next_long) = if mode.long {
            (r.bit().unwrap_or(true), r.bit().unwrap_or(true))
        } else {
            (false, false)
        };
        let left_n = if mode.long && !prev_long {
            s.bs[0] / 2
        } else {
            n / 2
        };
        let right_n = if mode.long && !next_long {
            s.bs[0] / 2
        } else {
            n / 2
        };
        let map = s
            .maps
            .get(mode.mapping)
            .ok_or(CodecError::Malformed("mapping index"))?;
        let half = n / 2;
        // Floors.
        enum Fl {
            One(Vec<i32>),
            Zero(f32, Vec<f32>),
            Unused,
        }
        let mut floors = Vec::with_capacity(ch);
        let mut short = false;
        for c in 0..ch {
            let fi = map.submaps[map.mux[c].min(map.submaps.len() - 1)].0;
            let f = s
                .floors
                .get(fi)
                .ok_or(CodecError::Malformed("floor index"))?;
            let res = match f {
                Floor::One { .. } => {
                    floor1_decode(&mut r, f, &s.books).map(|o| o.map_or(Fl::Unused, Fl::One))
                }
                Floor::Zero { .. } => floor0_decode(&mut r, f, &s.books)
                    .map(|o| o.map_or(Fl::Unused, |(a, c)| Fl::Zero(a, c))),
            };
            match res {
                Ok(v) => floors.push(v),
                Err(CodecError::Truncated) => {
                    short = true;
                    floors.push(Fl::Unused);
                }
                Err(e) => return Err(e),
            }
        }
        let mut no_residue: Vec<bool> = floors.iter().map(|f| matches!(f, Fl::Unused)).collect();
        for &(m, a) in &map.coupling {
            if !no_residue[m] || !no_residue[a] {
                no_residue[m] = false;
                no_residue[a] = false;
            }
        }
        let mut spec = vec![vec![0f32; half]; ch];
        if !short {
            for (si, &(_, ri)) in map.submaps.iter().enumerate() {
                let chans: Vec<usize> = (0..ch).filter(|c| map.mux[*c] == si).collect();
                let mut vecs: Vec<Vec<f32>> = chans.iter().map(|_| vec![0f32; half]).collect();
                let skip: Vec<bool> = chans.iter().map(|c| no_residue[*c]).collect();
                let res = s
                    .residues
                    .get(ri)
                    .ok_or(CodecError::Malformed("residue index"))?;
                match residue_decode(&mut r, res, &s.books, half, &mut vecs, &skip) {
                    Ok(()) | Err(CodecError::Truncated) => {}
                    Err(e) => return Err(e),
                }
                for (k, c) in chans.iter().enumerate() {
                    spec[*c] = std::mem::take(&mut vecs[k]);
                }
            }
        }
        for &(m, a) in map.coupling.iter().rev() {
            #[expect(
                clippy::needless_range_loop,
                reason = "two channels of one array, by index"
            )]
            for i in 0..half {
                let (mv, av) = (spec[m][i], spec[a][i]);
                let (nm, na) = if mv > 0.0 {
                    if av > 0.0 {
                        (mv, mv - av)
                    } else {
                        (mv + av, mv)
                    }
                } else if av > 0.0 {
                    (mv, mv + av)
                } else {
                    (mv - av, mv)
                };
                spec[m][i] = nm;
                spec[a][i] = na;
            }
        }
        let w = window(n, left_n, right_n);
        let mut blocks = Vec::with_capacity(ch);
        for c in 0..ch {
            let fi = map.submaps[map.mux[c].min(map.submaps.len() - 1)].0;
            let f = &s.floors[fi];
            match &floors[c] {
                Fl::Unused => spec[c].fill(0.0),
                Fl::One(y) => floor1_apply(f, y, &mut spec[c]),
                Fl::Zero(a, coefs) => floor0_apply(f, *a, coefs, &mut spec[c]),
            }
            let mut y = vec![0f32; n];
            imdct[usize::from(mode.long)].run(&spec[c], &mut y);
            for (v, wv) in y.iter_mut().zip(&w) {
                *v *= wv;
            }
            blocks.push(y);
        }
        if let Some((pb, pn)) = &prev {
            let count = pn / 4 + n / 4;
            let shift = isize::try_from(n / 4).unwrap_or(0) - isize::try_from(pn / 4).unwrap_or(0);
            for j in 0..count {
                for c in 0..ch {
                    let a = pb[c].get(pn / 2 + j).copied().unwrap_or(0.0);
                    let bi = isize::try_from(j).unwrap_or(0) + shift;
                    let b = usize::try_from(bi)
                        .ok()
                        .and_then(|i| blocks[c].get(i))
                        .copied()
                        .unwrap_or(0.0);
                    out.push(a + b);
                }
            }
        }
        prev = Some((blocks, n));
    }
    if let Some(g) = final_granule {
        let want = usize::try_from(g).unwrap_or(0) * ch;
        if want < out.len() {
            out.truncate(want);
        }
    }
    Ok(Decoded {
        data: out,
        channels: u8::try_from(ch).unwrap_or(u8::MAX),
        rate: s.rate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{s16le, snr_db};

    #[test]
    fn float32_unpacks() {
        // 1.0 = mantissa 1<<20, exponent 768 (788 − 20).
        assert!((float32((768 << 21) | (1 << 20)) - 1.0).abs() < 1e-7);
        assert!((float32(0x8000_0000 | (768 << 21) | (1 << 20)) + 1.0).abs() < 1e-7);
    }

    #[test]
    fn codeword_assignment_follows_the_spec_example() {
        // The spec's §3.2.1 example: lengths 2,4,4,4,4,2,3,3 →
        // 00, 0100, 0101, 0110, 0111, 10, 110, 111.
        let (tree, _) = build_tree(&[2, 4, 4, 4, 4, 2, 3, 3]).unwrap();
        let walk = |bits: &[u8]| {
            let mut n = 0usize;
            for (i, b) in bits.iter().enumerate() {
                let v = tree[n][usize::from(*b)];
                if i + 1 == bits.len() {
                    return !v;
                }
                n = usize::try_from(v).unwrap();
            }
            -1
        };
        assert_eq!(walk(&[0, 0]), 0);
        assert_eq!(walk(&[0, 1, 0, 0]), 1);
        assert_eq!(walk(&[0, 1, 1, 1]), 4);
        assert_eq!(walk(&[1, 0]), 5);
        assert_eq!(walk(&[1, 1, 0]), 6);
        assert_eq!(walk(&[1, 1, 1]), 7);
    }

    #[test]
    fn decodes_libvorbis_stereo_like_ffmpeg() {
        let d = decode(include_bytes!("../../tests/fixtures/src.ogg")).unwrap();
        let reference = s16le(include_bytes!("../../tests/fixtures/src.ogg.ref"));
        assert_eq!((d.channels, d.rate), (2, 44_100));
        assert_eq!(d.frames(), 17_640, "trimmed to the stream's granule count");
        let (snr, lag) = snr_db(&reference, &d.data, 2, 2_048);
        eprintln!("vorbis: {snr:.1} dB at lag {lag}");
        assert_eq!(lag, 0);
        assert!(snr > 60.0, "SNR {snr} dB against ffmpeg's decode");
    }
}
