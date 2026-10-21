//! Whisper, the speech model: an encoder that listens and a decoder that
//! writes, run on the CPU.
//!
//! ```text
//!  log-mel [80, 3000] (30 s)
//!    -> conv(k=3) + GELU -> conv(k=3, stride 2) + GELU -> + positions   [1500, d]
//!    -> 6 x { x += attn(LN(x));  x += mlp(LN(x)) } -> LN               the encoder
//!  [<|startoftranscript|>, <|en|>, <|transcribe|>, <|notimestamps|>]
//!    -> 6 x { x += self-attn(LN(x)) (causal, cached)
//!             x += cross-attn(LN(x), encoder)
//!             x += mlp(LN(x)) } -> LN -> x . token embeddings          the decoder
//!    -> the most likely token, again, until <|endoftext|>
//! ```
//!
//! Reads whisper.cpp's `ggml-*.bin` files (the format they are published
//! in) and matches whisper.cpp's graph (`src/whisper.cpp`), including which
//! tokens may not be written (timestamps, task tokens, a blank start).
//! Half-float weights are quantized to 8-bit blocks at load: a quarter of
//! the memory, integer dot products, no audible difference in the text.

use std::path::Path;

use crate::audio::{self, WINDOW_FRAMES};
use crate::linear::{affine, ranges, with_pool, Matrix, Pool};
use crate::num::{dot_f32, f16_to_f32, softmax_term};

/// Languages in the model's token order (the token is `sot + 1 + index`).
pub const LANGUAGES: [&str; 100] = [
    "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar", "sv", "it", "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs", "ro", "da", "hu", "ta", "no", "th", "ur",
    "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk", "te", "fa", "lv", "bn", "sr", "az", "sl", "kn", "et", "mk", "br", "eu", "is", "hy", "ne", "mn", "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si",
    "km", "sn", "yo", "so", "af", "oc", "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo", "ht", "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt", "haw", "ln", "ha",
    "ba", "jw", "su", "yue",
];

#[derive(Debug, Clone)]
pub struct Hparams {
    pub n_vocab: usize,
    pub audio_ctx: usize,
    pub audio_state: usize,
    pub audio_head: usize,
    pub audio_layer: usize,
    pub text_ctx: usize,
    pub text_state: usize,
    pub text_head: usize,
    pub text_layer: usize,
    pub n_mels: usize,
}

/// The special tokens, which move with the vocabulary size.
#[derive(Debug, Clone, Copy)]
pub struct Specials {
    pub eot: u32,
    pub sot: u32,
    pub translate: u32,
    pub transcribe: u32,
    pub solm: u32,
    pub prev: u32,
    pub nosp: u32,
    pub not: u32,
    pub beg: u32,
    pub languages: usize,
    pub space: Option<u32>,
}

impl Specials {
    fn new(n_vocab: usize, space: Option<u32>) -> Specials {
        let multilingual = n_vocab >= 51865;
        let languages = if multilingual { n_vocab - 51765 - 1 } else { 0 };
        let shift = if multilingual { 1 + languages as u32 - 99 } else { 0 };
        let m = u32::from(multilingual);
        Specials {
            eot: 50256 + m,
            sot: 50257 + m,
            translate: 50357 + shift,
            transcribe: 50358 + shift,
            solm: 50359 + shift,
            prev: 50360 + shift,
            nosp: 50361 + shift,
            not: 50362 + shift,
            beg: 50363 + shift,
            languages,
            space,
        }
    }

    pub fn language_token(&self, code: &str) -> Option<u32> {
        LANGUAGES[..self.languages.min(LANGUAGES.len())].iter().position(|l| *l == code).map(|i| self.sot + 1 + i as u32)
    }
}

struct Attn {
    ln_w: Vec<f32>,
    ln_b: Vec<f32>,
    q: Matrix,
    q_b: Vec<f32>,
    k: Matrix,
    v: Matrix,
    v_b: Vec<f32>,
    o: Matrix,
    o_b: Vec<f32>,
}

struct Mlp {
    ln_w: Vec<f32>,
    ln_b: Vec<f32>,
    up: Matrix,
    up_b: Vec<f32>,
    down: Matrix,
    down_b: Vec<f32>,
}

struct DecoderLayer {
    attn: Attn,
    cross: Attn,
    mlp: Mlp,
}

pub struct Whisper {
    pub hp: Hparams,
    pub specials: Specials,
    filters: Vec<f32>,
    vocab: Vec<Vec<u8>>,
    conv1: Matrix,
    conv1_b: Vec<f32>,
    conv2: Matrix,
    conv2_b: Vec<f32>,
    enc_pos: Vec<f32>,
    enc: Vec<(Attn, Mlp)>,
    enc_ln_w: Vec<f32>,
    enc_ln_b: Vec<f32>,
    dec_pos: Vec<f32>,
    embed: Matrix,
    dec: Vec<DecoderLayer>,
    dec_ln_w: Vec<f32>,
    dec_ln_b: Vec<f32>,
    threads: usize,
}

/// What the encoder heard, ready for the decoder: per layer, the
/// cross-attention keys and values ([1500, d] each).
pub struct Heard {
    cross: Vec<(Vec<f32>, Vec<f32>)>,
}

/// The decoder's own keys and values so far, per layer.
pub struct DecodeState {
    k: Vec<Vec<f32>>,
    v: Vec<Vec<f32>>,
    pos: usize,
}

const EPS: f32 = 1e-5;

fn layer_norm(x: &mut [f32], w: &[f32], b: &[f32]) {
    let n = x.len() as f64;
    let mean = x.iter().map(|v| f64::from(*v)).sum::<f64>() / n;
    let var = x.iter().map(|v| (f64::from(*v) - mean).powi(2)).sum::<f64>() / n;
    let inv = 1.0 / (var as f32 + EPS).sqrt();
    let mean = mean as f32;
    for ((v, w), b) in x.iter_mut().zip(w).zip(b) {
        *v = (*v - mean) * inv * w + b;
    }
}

/// GELU, tanh form (as ggml computes it).
fn gelu(x: f32) -> f32 {
    const A: f32 = 0.044_715;
    const S: f32 = 0.797_884_6; // sqrt(2 / pi)
    0.5 * x * (1.0 + (S * x * (1.0 + A * x * x)).tanh())
}

/// One raw tensor from the file.
struct Raw {
    ttype: u32,
    dims: Vec<usize>,
    data: Vec<u8>,
}

impl Raw {
    fn floats(&self) -> Result<Vec<f32>, String> {
        match self.ttype {
            0 => Ok(self.data.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect()),
            1 => Ok(self.data.as_chunks::<2>().0.iter().map(|b| f16_to_f32(u16::from_le_bytes(*b))).collect()),
            t => Err(format!("tensor type {t} is not supported (use an F16 or F32 Whisper file)")),
        }
    }
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self.at.checked_add(n).filter(|e| *e <= self.b.len()).ok_or("the file ends early (truncated download?)")?;
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().map_err(|_| "i32")?))
    }
    fn count(&mut self) -> Result<usize, String> {
        usize::try_from(self.i32()?).ok().filter(|n| *n <= self.b.len()).ok_or_else(|| "implausible size in the model file".to_string())
    }
}

impl Whisper {
    pub fn load(path: &Path) -> Result<Whisper, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Whisper::parse(&bytes, true).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// `quantize`: half-float matrices to 8-bit blocks (the default). Float
    /// files are always kept as floats.
    pub fn parse(bytes: &[u8], quantize: bool) -> Result<Whisper, String> {
        let mut r = Reader { b: bytes, at: 0 };
        if r.take(4)? != b"lmgg" {
            return Err("not a whisper.cpp model file (ggml-*.bin)".into());
        }
        let mut h = [0usize; 11];
        for v in &mut h {
            *v = r.count()?;
        }
        let hp = Hparams { n_vocab: h[0], audio_ctx: h[1], audio_state: h[2], audio_head: h[3], audio_layer: h[4], text_ctx: h[5], text_state: h[6], text_head: h[7], text_layer: h[8], n_mels: h[9] };
        let (n_mel, n_fft) = (r.count()?, r.count()?);
        if n_mel != hp.n_mels || n_fft != audio::N_FFT / 2 + 1 {
            return Err(format!("mel filters are {n_mel} x {n_fft}; expected {} x {}", hp.n_mels, audio::N_FFT / 2 + 1));
        }
        let filters: Vec<f32> = r.take(4 * n_mel * n_fft)?.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect();
        let n_words = r.count()?;
        let mut vocab = Vec::with_capacity(hp.n_vocab.max(n_words));
        for _ in 0..n_words {
            let n = r.count()?;
            vocab.push(r.take(n)?.to_vec());
        }
        vocab.resize(hp.n_vocab.max(n_words), Vec::new()); // special tokens past the list
        let mut tensors = std::collections::HashMap::new();
        while r.at < bytes.len() {
            let n_dims = r.count()?;
            let name_len = r.count()?;
            let ttype = r.count()? as u32;
            if n_dims > 4 {
                return Err(format!("a tensor with {n_dims} dimensions"));
            }
            let dims: Vec<usize> = (0..n_dims).map(|_| r.count()).collect::<Result<_, _>>()?;
            let name = String::from_utf8_lossy(r.take(name_len)?).into_owned();
            let n: usize = dims.iter().product();
            let size = match ttype {
                0 => n * 4,
                1 => n * 2,
                t => return Err(format!("{name}: tensor type {t} is not supported (use an F16 or F32 Whisper file)")),
            };
            let data = r.take(size)?.to_vec();
            tensors.insert(name, Raw { ttype, dims, data });
        }
        let get = |name: &str| tensors.get(name).ok_or_else(|| format!("the model file has no tensor '{name}'"));
        let vec = |name: &str| get(name)?.floats();
        let mat = |name: &str, rows: usize, cols: usize| -> Result<Matrix, String> {
            let t = get(name)?;
            if t.dims.iter().product::<usize>() != rows * cols {
                return Err(format!("{name} is {:?}, expected {rows} x {cols}", t.dims));
            }
            let w = t.floats()?;
            Ok(if quantize && t.ttype == 1 && cols.is_multiple_of(32) { Matrix::quantized(&w, rows, cols) } else { Matrix::floats(w, rows, cols) })
        };
        let attn = |p: &str, d: usize| -> Result<Attn, String> {
            Ok(Attn {
                ln_w: vec(&format!("{p}_ln.weight"))?,
                ln_b: vec(&format!("{p}_ln.bias"))?,
                q: mat(&format!("{p}.query.weight"), d, d)?,
                q_b: vec(&format!("{p}.query.bias"))?,
                k: mat(&format!("{p}.key.weight"), d, d)?,
                v: mat(&format!("{p}.value.weight"), d, d)?,
                v_b: vec(&format!("{p}.value.bias"))?,
                o: mat(&format!("{p}.out.weight"), d, d)?,
                o_b: vec(&format!("{p}.out.bias"))?,
            })
        };
        let mlp = |p: &str, d: usize| -> Result<Mlp, String> {
            Ok(Mlp {
                ln_w: vec(&format!("{p}.mlp_ln.weight"))?,
                ln_b: vec(&format!("{p}.mlp_ln.bias"))?,
                up: mat(&format!("{p}.mlp.0.weight"), 4 * d, d)?,
                up_b: vec(&format!("{p}.mlp.0.bias"))?,
                down: mat(&format!("{p}.mlp.2.weight"), d, 4 * d)?,
                down_b: vec(&format!("{p}.mlp.2.bias"))?,
            })
        };
        let (da, dt) = (hp.audio_state, hp.text_state);
        let enc = (0..hp.audio_layer).map(|i| Ok((attn(&format!("encoder.blocks.{i}.attn"), da)?, mlp(&format!("encoder.blocks.{i}"), da)?))).collect::<Result<Vec<_>, String>>()?;
        let dec = (0..hp.text_layer)
            .map(|i| {
                Ok(DecoderLayer { attn: attn(&format!("decoder.blocks.{i}.attn"), dt)?, cross: attn(&format!("decoder.blocks.{i}.cross_attn"), dt)?, mlp: mlp(&format!("decoder.blocks.{i}"), dt)? })
            })
            .collect::<Result<Vec<_>, String>>()?;
        // Convolutions as matrices over [channel][tap] windows (3 taps).
        let conv1 = Matrix::floats(vec("encoder.conv1.weight")?, da, hp.n_mels * 3);
        let conv2 = Matrix::floats(vec("encoder.conv2.weight")?, da, da * 3);
        let space = vocab.iter().position(|t| t == b" ").map(|i| i as u32);
        let specials = Specials::new(hp.n_vocab, space);
        if (specials.beg as usize) >= hp.n_vocab {
            return Err(format!("a vocabulary of {} is too small for Whisper's special tokens", hp.n_vocab));
        }
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(16);
        Ok(Whisper {
            specials,
            filters,
            vocab,
            conv1,
            conv1_b: vec("encoder.conv1.bias")?,
            conv2,
            conv2_b: vec("encoder.conv2.bias")?,
            enc_pos: vec("encoder.positional_embedding")?,
            enc,
            enc_ln_w: vec("encoder.ln_post.weight")?,
            enc_ln_b: vec("encoder.ln_post.bias")?,
            dec_pos: vec("decoder.positional_embedding")?,
            embed: mat("decoder.token_embedding.weight", hp.n_vocab, dt)?,
            dec,
            dec_ln_w: vec("decoder.ln.weight")?,
            dec_ln_b: vec("decoder.ln.bias")?,
            hp,
            threads,
        })
    }

    pub fn set_threads(&mut self, n: usize) {
        self.threads = n.max(1);
    }

    /// The spectrogram of 16 kHz samples.
    pub fn mel(&self, samples: &[f32]) -> (Vec<f32>, usize) {
        audio::log_mel(samples, &self.filters, self.hp.n_mels)
    }

    /// Listen to 30 s of spectrogram starting at frame `offset`.
    pub fn encode(&self, mel: &[f32], n_len: usize, offset: usize) -> Heard {
        let hp = &self.hp;
        let d = hp.audio_state;
        let frames = 2 * hp.audio_ctx; // 3000 in, 1500 out
                                       // The window, [mel][frame], zeros past the end.
        let mut inp = vec![0.0f32; hp.n_mels * frames];
        for m in 0..hp.n_mels {
            for f in 0..frames {
                if offset + f < n_len {
                    inp[m * frames + f] = mel[m * n_len + offset + f];
                }
            }
        }
        with_pool(self.threads, |pool| {
            // conv1: stride 1, pad 1 -> [frames][d]
            let cols = im2col(&inp, hp.n_mels, frames, 1);
            let mut x = affine(pool, &[], &self.conv1, &self.conv1_b, &cols, frames);
            x.iter_mut().for_each(|v| *v = gelu(*v));
            // conv2: stride 2 -> [ctx][d]; im2col wants [channel][frame].
            let chw = transpose(&x, frames, d);
            let cols = im2col(&chw, d, frames, 2);
            let mut x = affine(pool, &[], &self.conv2, &self.conv2_b, &cols, hp.audio_ctx);
            for (i, v) in x.iter_mut().enumerate() {
                *v = gelu(*v) + self.enc_pos[i];
            }
            let t = hp.audio_ctx;
            for (a, m) in &self.enc {
                let mut h = x.clone();
                h.chunks_mut(d).for_each(|r| layer_norm(r, &a.ln_w, &a.ln_b));
                let q = affine(pool, &[], &a.q, &a.q_b, &h, t);
                let k = affine(pool, &[], &a.k, &[], &h, t);
                let v = affine(pool, &[], &a.v, &a.v_b, &h, t);
                let att = attention(&q, &k, &v, t, t, d, hp.audio_head, pool.threads());
                add(&mut x, &affine(pool, &[], &a.o, &a.o_b, &att, t));
                self.mlp(pool, m, &mut x, t, d);
            }
            x.chunks_mut(d).for_each(|r| layer_norm(r, &self.enc_ln_w, &self.enc_ln_b));
            let cross = self.dec.iter().map(|l| (affine(pool, &[], &l.cross.k, &[], &x, t), affine(pool, &[], &l.cross.v, &l.cross.v_b, &x, t))).collect();
            Heard { cross }
        })
    }

    fn mlp<'a>(&'a self, pool: &Pool<'a>, m: &'a Mlp, x: &mut [f32], t: usize, d: usize) {
        let mut h = x.to_vec();
        h.chunks_mut(d).for_each(|r| layer_norm(r, &m.ln_w, &m.ln_b));
        let mut u = affine(pool, &[], &m.up, &m.up_b, &h, t);
        u.iter_mut().for_each(|v| *v = gelu(*v));
        add(x, &affine(pool, &[], &m.down, &m.down_b, &u, t));
    }

    pub fn start(&self) -> DecodeState {
        DecodeState { k: vec![Vec::new(); self.dec.len()], v: vec![Vec::new(); self.dec.len()], pos: 0 }
    }

    /// Feed one token; the logits for the next.
    pub fn step(&self, heard: &Heard, st: &mut DecodeState, token: u32) -> Result<Vec<f32>, String> {
        let hp = &self.hp;
        let d = hp.text_state;
        if st.pos >= hp.text_ctx {
            return Err("the transcript window is full".into());
        }
        if token as usize >= hp.n_vocab {
            return Err(format!("token {token} is outside the vocabulary"));
        }
        let mut x = vec![0.0f32; d];
        self.embed.row(&[], token as usize, &mut x);
        add(&mut x, &self.dec_pos[st.pos * d..(st.pos + 1) * d]);
        let logits = with_pool(self.threads, |pool| {
            for (li, l) in self.dec.iter().enumerate() {
                let a = &l.attn;
                let mut h = x.clone();
                layer_norm(&mut h, &a.ln_w, &a.ln_b);
                let q = affine(pool, &[], &a.q, &a.q_b, &h, 1);
                st.k[li].extend(affine(pool, &[], &a.k, &[], &h, 1));
                st.v[li].extend(affine(pool, &[], &a.v, &a.v_b, &h, 1));
                let n = st.pos + 1;
                let att = attention(&q, &st.k[li], &st.v[li], 1, n, d, hp.text_head, 1);
                add(&mut x, &affine(pool, &[], &a.o, &a.o_b, &att, 1));
                let c = &l.cross;
                let mut h = x.clone();
                layer_norm(&mut h, &c.ln_w, &c.ln_b);
                let q = affine(pool, &[], &c.q, &c.q_b, &h, 1);
                let (ck, cv) = &heard.cross[li];
                let att = attention(&q, ck, cv, 1, hp.audio_ctx, d, hp.text_head, 1);
                add(&mut x, &affine(pool, &[], &c.o, &c.o_b, &att, 1));
                self.mlp(pool, &l.mlp, &mut x, 1, d);
            }
            layer_norm(&mut x, &self.dec_ln_w, &self.dec_ln_b);
            crate::linear::matmul(pool, &[], &self.embed, &x, 1)
        });
        st.pos += 1;
        Ok(logits)
    }

    /// Which language is spoken: (code, probability), most likely first.
    pub fn detect_language(&self, heard: &Heard) -> Result<Vec<(&'static str, f32)>, String> {
        let s = &self.specials;
        let logits = self.step(heard, &mut self.start(), s.sot)?;
        let n = s.languages.min(LANGUAGES.len());
        let lang: Vec<f32> = (0..n).map(|i| logits[(s.sot + 1) as usize + i]).collect();
        let max = lang.iter().fold(f32::MIN, |m, v| m.max(*v));
        let sum: f32 = lang.iter().map(|v| (v - max).exp()).sum();
        let mut out: Vec<(&'static str, f32)> = lang.iter().enumerate().map(|(i, v)| (LANGUAGES[i], (v - max).exp() / sum)).collect();
        out.sort_by(|a, b| b.1.total_cmp(&a.1));
        Ok(out)
    }

    /// Hide what may not be written next (whisper.cpp's rules, no
    /// timestamps).
    fn suppress(&self, logits: &mut [f32], first: bool) {
        let s = &self.specials;
        if first {
            logits[s.eot as usize] = f32::NEG_INFINITY;
            if let Some(sp) = s.space {
                logits[sp as usize] = f32::NEG_INFINITY;
            }
        }
        for t in [s.not, s.sot, s.nosp, s.solm, s.translate, s.transcribe, s.prev] {
            logits[t as usize] = f32::NEG_INFINITY;
        }
        for v in &mut logits[s.beg as usize..] {
            *v = f32::NEG_INFINITY;
        }
        for i in 0..s.languages {
            logits[s.sot as usize + 1 + i] = f32::NEG_INFINITY;
        }
    }

    /// Speech to text. `language`: a code (`"en"`), or `None` to detect it
    /// from the first 30 seconds.
    pub fn transcribe(&self, samples: &[f32], language: Option<&str>) -> Result<Transcript, String> {
        let s = self.specials;
        let (mel, n_len) = self.mel(samples);
        let real_frames = samples.len() / audio::HOP;
        let mut text = Vec::new();
        let mut detected = language.map(String::from);
        let mut offset = 0;
        while offset < real_frames.max(1) {
            let heard = self.encode(&mel, n_len, offset);
            let lang = match &detected {
                Some(l) => l.clone(),
                None if s.languages > 0 => {
                    let l = self.detect_language(&heard)?[0].0.to_string();
                    detected = Some(l.clone());
                    l
                }
                None => "en".into(),
            };
            let mut prompt = vec![s.sot];
            if s.languages > 0 {
                prompt.push(s.language_token(&lang).ok_or_else(|| format!("'{lang}' is not a language this model knows"))?);
                prompt.push(s.transcribe);
            }
            prompt.push(s.not);
            let mut st = self.start();
            let mut logits = Vec::new();
            for t in &prompt {
                logits = self.step(&heard, &mut st, *t)?;
            }
            let limit = self.hp.text_ctx / 2;
            for n in 0..limit {
                self.suppress(&mut logits, n == 0);
                let next = logits.iter().enumerate().fold((0, f32::NEG_INFINITY), |b, (i, v)| if *v > b.1 { (i, *v) } else { b }).0 as u32;
                if next == s.eot {
                    break;
                }
                text.push(next);
                if st.pos >= self.hp.text_ctx {
                    break;
                }
                logits = self.step(&heard, &mut st, next)?;
            }
            offset += WINDOW_FRAMES;
        }
        let bytes: Vec<u8> = text.iter().filter(|t| **t < s.eot).flat_map(|t| self.vocab[*t as usize].clone()).collect();
        Ok(Transcript { text: String::from_utf8_lossy(&bytes).trim().to_string(), language: detected.unwrap_or_else(|| "en".into()), tokens: text })
    }

    pub fn token_bytes(&self, t: u32) -> &[u8] {
        self.vocab.get(t as usize).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

#[derive(Debug, Clone)]
pub struct Transcript {
    pub text: String,
    pub language: String,
    pub tokens: Vec<u32>,
}

fn add(x: &mut [f32], y: &[f32]) {
    for (a, b) in x.iter_mut().zip(y) {
        *a += b;
    }
}

fn transpose(x: &[f32], rows: usize, cols: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; rows * cols];
    for r in 0..rows {
        for c in 0..cols {
            out[c * rows + r] = x[r * cols + c];
        }
    }
    out
}

/// Windows for a 3-tap convolution with padding 1: for each output frame,
/// `[channel][tap]` (the order the weights are stored in).
fn im2col(x: &[f32], channels: usize, frames: usize, stride: usize) -> Vec<f32> {
    let out_frames = (frames + 2 - 3) / stride + 1;
    let mut cols = vec![0.0f32; out_frames * channels * 3];
    for o in 0..out_frames {
        for c in 0..channels {
            for k in 0..3 {
                let f = (o * stride + k) as isize - 1;
                if f >= 0 && (f as usize) < frames {
                    cols[(o * channels + c) * 3 + k] = x[c * frames + f as usize];
                }
            }
        }
    }
    cols
}

/// Multi-head attention of `tq` queries over `tk` keys (all visible),
/// heads in parallel. Each head works on its own contiguous copies of Q,
/// K and V (rows of `d` hold every head side by side), four queries at a
/// time against each key row.
#[allow(clippy::too_many_arguments)]
fn attention(q: &[f32], k: &[f32], v: &[f32], tq: usize, tk: usize, d: usize, heads: usize, threads: usize) -> Vec<f32> {
    let hd = d / heads;
    let scale = 1.0 / (hd as f32).sqrt();
    let gather = |x: &[f32], n: usize, h: usize| -> Vec<f32> { (0..n).flat_map(|i| x[i * d + h * hd..][..hd].iter().copied()).collect() };
    let work = |hs: std::ops::Range<usize>| {
        let mut out = Vec::new();
        for h in hs {
            let (qh, kh) = (gather(q, tq, h), gather(k, tk, h));
            // V transposed: [hd][tk], so each output is one long dot.
            let mut vt = vec![0.0f32; hd * tk];
            for p in 0..tk {
                for c in 0..hd {
                    vt[c * tk + p] = v[p * d + h * hd + c];
                }
            }
            let mut o = vec![0.0f32; tq * hd];
            let mut scores = vec![0.0f32; 4 * tk];
            for i0 in (0..tq).step_by(4) {
                let n = (tq - i0).min(4);
                for p in 0..tk {
                    let kp = &kh[p * hd..(p + 1) * hd];
                    for j in 0..n {
                        scores[j * tk + p] = dot_f32(&qh[(i0 + j) * hd..(i0 + j + 1) * hd], kp) * scale;
                    }
                }
                for j in 0..n {
                    let row = &mut scores[j * tk..(j + 1) * tk];
                    let max = row.iter().fold(f32::NEG_INFINITY, |m, s| m.max(*s));
                    let mut sum = 0.0f32;
                    for sc in row.iter_mut() {
                        *sc = softmax_term(*sc - max);
                        sum += *sc;
                    }
                    let inv = 1.0 / sum;
                    row.iter_mut().for_each(|sc| *sc *= inv);
                    for c in 0..hd {
                        o[(i0 + j) * hd + c] = dot_f32(row, &vt[c * tk..(c + 1) * tk]);
                    }
                }
            }
            out.push((h, o));
        }
        out
    };
    if tq == 1 {
        return attend_one(q, k, v, tk, d, heads);
    }
    let per_head: Vec<(usize, Vec<f32>)> = if threads <= 1 {
        work(0..heads)
    } else {
        std::thread::scope(|s| {
            let jobs: Vec<_> = ranges(heads, threads).into_iter().map(|hs| s.spawn(move || work(hs))).collect();
            jobs.into_iter().flat_map(|j| j.join().expect("attention thread")).collect()
        })
    };
    let mut out = vec![0.0f32; tq * d];
    for (h, o) in per_head {
        for i in 0..tq {
            out[i * d + h * hd..][..hd].copy_from_slice(&o[i * hd..(i + 1) * hd]);
        }
    }
    out
}

/// One query (a decoding step): straight over the rows, no copies.
fn attend_one(q: &[f32], k: &[f32], v: &[f32], tk: usize, d: usize, heads: usize) -> Vec<f32> {
    let hd = d / heads;
    let scale = 1.0 / (hd as f32).sqrt();
    let mut out = vec![0.0f32; d];
    let mut scores = vec![0.0f32; tk];
    for h in 0..heads {
        let qh = &q[h * hd..(h + 1) * hd];
        for (p, sc) in scores.iter_mut().enumerate() {
            *sc = dot_f32(qh, &k[p * d + h * hd..][..hd]) * scale;
        }
        let max = scores.iter().fold(f32::NEG_INFINITY, |m, s| m.max(*s));
        let mut sum = 0.0f32;
        for sc in &mut scores {
            *sc = softmax_term(*sc - max);
            sum += *sc;
        }
        let oh = &mut out[h * hd..(h + 1) * hd];
        for (p, sc) in scores.iter().enumerate() {
            let w = sc / sum;
            if w != 0.0 {
                for (a, b) in oh.iter_mut().zip(&v[p * d + h * hd..][..hd]) {
                    *a += w * b;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_query_matches_the_batched_path() {
        let (tk, d, heads) = (37, 16, 4);
        let q: Vec<f32> = (0..2 * d).map(|i| ((i * 5 % 11) as f32 - 5.0) / 4.0).collect();
        let k: Vec<f32> = (0..tk * d).map(|i| ((i * 3 % 7) as f32 - 3.0) / 3.0).collect();
        let v: Vec<f32> = (0..tk * d).map(|i| ((i * 13 % 17) as f32 - 8.0) / 8.0).collect();
        let batched = attention(&q, &k, &v, 2, tk, d, heads, 2);
        let one = attend_one(&q[d..], &k, &v, tk, d, heads);
        for (a, b) in batched[d..].iter().zip(&one) {
            assert!((a - b).abs() < 1e-5, "{a} vs {b}");
        }
    }

    #[test]
    fn special_tokens_follow_the_vocabulary_size() {
        let s = Specials::new(51865, Some(220));
        assert_eq!((s.eot, s.sot, s.transcribe, s.not, s.beg, s.languages), (50257, 50258, 50359, 50363, 50364, 99));
        assert_eq!(s.language_token("en"), Some(50259));
        // large-v3: one more language, everything after it moves by one.
        let s = Specials::new(51866, None);
        assert_eq!((s.transcribe, s.beg, s.language_token("yue")), (50360, 50365, Some(50358)));
        // English-only models have no language tokens.
        let s = Specials::new(51864, None);
        assert_eq!((s.eot, s.sot, s.languages), (50256, 50257, 0));
    }

    #[test]
    fn im2col_pads_and_strides() {
        // 1 channel, frames 1 2 3 4, stride 2 -> windows (0 1 2), (2 3 4)
        let c = im2col(&[1.0, 2.0, 3.0, 4.0], 1, 4, 2);
        assert_eq!(c, vec![0.0, 1.0, 2.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn gelu_is_gelu() {
        assert_eq!(gelu(0.0), 0.0);
        assert!((gelu(1.0) - 0.841_192).abs() < 1e-4);
        assert!(gelu(-6.0).abs() < 1e-6);
    }
}
