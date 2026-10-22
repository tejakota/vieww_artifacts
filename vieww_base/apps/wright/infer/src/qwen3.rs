//! Qwen3, the text model: a decoder-only transformer, run on the CPU.
//!
//! ```text
//!  tokens -> embedding row each                              x: [T, d]
//!  28 x {
//!     h = rmsnorm(x)
//!     q, k, v = h Wq, h Wk, h Wv          (8 key/value heads shared by 16 query heads)
//!     q, k = rmsnorm per head, then RoPE (rotate half-pairs by position)
//!     a = softmax(q k^T / sqrt(128)) v    (causal: a token sees itself and before)
//!     x = x + a Wo
//!     h = rmsnorm(x)
//!     x = x + (silu(h Wgate) * h Wup) Wdown
//!  }
//!  logits = rmsnorm(x_last) Wout          (Wout is the embedding when not stored)
//! ```
//!
//! Matches llama.cpp's `qwen3` graph (`src/models/qwen3.cpp`). The weights
//! are 8-bit (Q8_0); each matrix product quantizes its input to the same
//! blocks and does integer dot products, as llama.cpp does on the CPU.
//!
//! Keys and values of every position seen are kept (in half floats) in a
//! [`Cache`], so a turn only computes its new tokens: an agent's long system
//! prompt and tool list are processed once, not on every step.

use std::path::Path;

use crate::gguf::{DType, Gguf};
use crate::linear::{matmul, ranges, with_pool, Matrix, Pool};
use crate::num::{dot_f32, f16_table, f32_to_f16, softmax_term, to_f32};
use crate::tokenizer::Tokenizer;

#[derive(Debug, Clone)]
pub struct Config {
    pub layers: usize,
    pub dim: usize,
    pub ff: usize,
    pub heads: usize,
    pub kv_heads: usize,
    pub head_dim: usize,
    pub vocab: usize,
    pub rope_base: f32,
    pub eps: f32,
    /// What the model was trained to (positions past this are refused).
    pub max_context: usize,
}

struct Layer {
    attn_norm: Vec<f32>,
    q: Matrix,
    k: Matrix,
    v: Matrix,
    o: Matrix,
    q_norm: Vec<f32>,
    k_norm: Vec<f32>,
    ffn_norm: Vec<f32>,
    gate: Matrix,
    up: Matrix,
    down: Matrix,
}

pub struct Model {
    pub cfg: Config,
    pub tokenizer: Tokenizer,
    file: Gguf,
    embed: Matrix,
    layers: Vec<Layer>,
    out_norm: Vec<f32>,
    out: Matrix,
    threads: usize,
}

/// Keys and values for the tokens seen so far, per layer, as f16.
pub struct Cache {
    pub tokens: Vec<u32>,
    k: Vec<Vec<u16>>,
    v: Vec<Vec<u16>>,
    limit: usize,
}

impl Cache {
    pub fn len(&self) -> usize {
        self.tokens.len()
    }
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }
    /// Forget everything from position `n` on.
    pub fn truncate(&mut self, n: usize, kv_dim: usize) {
        self.tokens.truncate(n);
        for l in self.k.iter_mut().chain(self.v.iter_mut()) {
            l.truncate(n * kv_dim);
        }
    }
}

fn floats(g: &Gguf, name: &str) -> Result<Vec<f32>, String> {
    let (t, raw) = g.tensor(name)?;
    Ok(to_f32(t.dtype, raw))
}

fn matrix(g: &Gguf, name: &str, rows: usize, cols: usize) -> Result<Matrix, String> {
    let (t, raw) = g.tensor(name)?;
    if t.dims.first() != Some(&cols) || t.elements() != rows * cols {
        return Err(format!("{name} is {:?}, expected [{cols}, {rows}]", t.dims));
    }
    Ok(match t.dtype {
        DType::Q8_0 if cols.is_multiple_of(32) => Matrix::q8_in_file(t.start, rows, cols),
        _ => Matrix::floats(to_f32(t.dtype, raw), rows, cols),
    })
}

/// `x * w / rms(x)`, in place.
fn rmsnorm(x: &mut [f32], w: &[f32], eps: f32) {
    let ss: f64 = x.iter().map(|v| f64::from(*v) * f64::from(*v)).sum();
    let scale = 1.0 / ((ss / x.len() as f64) as f32 + eps).sqrt();
    for (v, w) in x.iter_mut().zip(w) {
        *v = *v * scale * w;
    }
}

/// NEOX-style rotary embedding on one head: pairs `(i, i + half)`.
fn rope(x: &mut [f32], pos: usize, base: f32) {
    let half = x.len() / 2;
    let theta_scale = base.powf(-2.0 / x.len() as f32);
    let mut theta = pos as f32;
    for i in 0..half {
        let (s, c) = theta.sin_cos();
        let (a, b) = (x[i], x[i + half]);
        x[i] = a * c - b * s;
        x[i + half] = a * s + b * c;
        theta *= theta_scale;
    }
}

fn silu(x: f32) -> f32 {
    x / (1.0 + (-x).exp())
}

impl Model {
    pub fn load(path: &Path) -> Result<Model, String> {
        Model::from_gguf(Gguf::open(path)?)
    }

    pub fn from_gguf(g: Gguf) -> Result<Model, String> {
        let arch = g
            .get("general.architecture")
            .and_then(|m| m.as_str())
            .unwrap_or("");
        if arch != "qwen3" {
            return Err(format!(
                "this is a '{arch}' model; the built-in engine runs Qwen3 only"
            ));
        }
        let int = |k: &str| g.int(&format!("qwen3.{k}")).map(|v| v as usize);
        let dim = int("embedding_length")?;
        let heads = int("attention.head_count")?;
        let tokenizer = Tokenizer::from_gguf(&g)?;
        let cfg = Config {
            layers: int("block_count")?,
            dim,
            ff: int("feed_forward_length")?,
            heads,
            kv_heads: int("attention.head_count_kv").unwrap_or(heads),
            head_dim: int("attention.key_length").unwrap_or(dim / heads),
            vocab: tokenizer.vocab_size(),
            rope_base: g.float("qwen3.rope.freq_base").unwrap_or(1_000_000.0) as f32,
            eps: g
                .float("qwen3.attention.layer_norm_rms_epsilon")
                .unwrap_or(1e-6) as f32,
            max_context: int("context_length").unwrap_or(32_768),
        };
        if !cfg.heads.is_multiple_of(cfg.kv_heads) || !cfg.head_dim.is_multiple_of(2) {
            return Err("inconsistent attention shape in the model file".into());
        }
        let (qd, kvd) = (cfg.heads * cfg.head_dim, cfg.kv_heads * cfg.head_dim);
        let mut layers = Vec::with_capacity(cfg.layers);
        for i in 0..cfg.layers {
            let n = |s: &str| format!("blk.{i}.{s}.weight");
            layers.push(Layer {
                attn_norm: floats(&g, &n("attn_norm"))?,
                q: matrix(&g, &n("attn_q"), qd, dim)?,
                k: matrix(&g, &n("attn_k"), kvd, dim)?,
                v: matrix(&g, &n("attn_v"), kvd, dim)?,
                o: matrix(&g, &n("attn_output"), dim, qd)?,
                q_norm: floats(&g, &n("attn_q_norm"))?,
                k_norm: floats(&g, &n("attn_k_norm"))?,
                ffn_norm: floats(&g, &n("ffn_norm"))?,
                gate: matrix(&g, &n("ffn_gate"), cfg.ff, dim)?,
                up: matrix(&g, &n("ffn_up"), cfg.ff, dim)?,
                down: matrix(&g, &n("ffn_down"), dim, cfg.ff)?,
            });
        }
        let embed = matrix(&g, "token_embd.weight", cfg.vocab, dim)?;
        let out = if g.tensors.contains_key("output.weight") {
            matrix(&g, "output.weight", cfg.vocab, dim)?
        } else {
            matrix(&g, "token_embd.weight", cfg.vocab, dim)?
        };
        let out_norm = floats(&g, "output_norm.weight")?;
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .min(16);
        Ok(Model {
            cfg,
            tokenizer,
            file: g,
            embed,
            layers,
            out_norm,
            out,
            threads,
        })
    }

    pub fn set_threads(&mut self, n: usize) {
        self.threads = n.max(1);
    }

    pub fn cache(&self, limit: usize) -> Cache {
        let n = self.cfg.layers;
        Cache {
            tokens: Vec::new(),
            k: vec![Vec::new(); n],
            v: vec![Vec::new(); n],
            limit: limit.min(self.cfg.max_context),
        }
    }

    fn kv_dim(&self) -> usize {
        self.cfg.kv_heads * self.cfg.head_dim
    }

    /// Make `cache` hold exactly `tokens`, computing only what it lacks, and
    /// return the logits after the last one. Whatever prefix the cache
    /// already shares with `tokens` is kept.
    pub fn prefill(&self, cache: &mut Cache, tokens: &[u32]) -> Result<Vec<f32>, String> {
        let mut keep = cache
            .tokens
            .iter()
            .zip(tokens)
            .take_while(|(a, b)| a == b)
            .count();
        // Something new must run to produce logits.
        if keep == tokens.len() {
            keep = keep.saturating_sub(1);
        }
        cache.truncate(keep, self.kv_dim());
        self.forward(cache, &tokens[keep..])
    }

    /// Run `tokens` after what the cache holds; logits after the last one.
    pub fn forward(&self, cache: &mut Cache, tokens: &[u32]) -> Result<Vec<f32>, String> {
        with_pool(self.threads, |pool| self.forward_in(pool, cache, tokens))
    }

    fn forward_in<'a>(
        &'a self,
        pool: &Pool<'a>,
        cache: &mut Cache,
        tokens: &[u32],
    ) -> Result<Vec<f32>, String> {
        let c = &self.cfg;
        let t = tokens.len();
        if t == 0 {
            return Err("nothing to run".into());
        }
        if cache.len() + t > cache.limit {
            return Err(format!(
                "the conversation is {} tokens; the built-in model holds {}",
                cache.len() + t,
                cache.limit
            ));
        }
        let start = cache.len();
        let (qd, kvd) = (c.heads * c.head_dim, self.kv_dim());
        // Embeddings.
        let mut x = vec![0.0f32; t * c.dim];
        for (i, tok) in tokens.iter().enumerate() {
            if *tok as usize >= c.vocab {
                return Err(format!("token {tok} is outside the vocabulary"));
            }
            self.embed.row(
                &self.file.data,
                *tok as usize,
                &mut x[i * c.dim..(i + 1) * c.dim],
            );
        }
        let mut h = vec![0.0f32; t * c.dim];
        for (li, l) in self.layers.iter().enumerate() {
            // ── attention ──
            h.copy_from_slice(&x);
            for row in h.chunks_mut(c.dim) {
                rmsnorm(row, &l.attn_norm, c.eps);
            }
            let mut q = matmul(pool, &self.file.data, &l.q, &h, t);
            let mut k = matmul(pool, &self.file.data, &l.k, &h, t);
            let v = matmul(pool, &self.file.data, &l.v, &h, t);
            for (i, (qr, kr)) in q.chunks_mut(qd).zip(k.chunks_mut(kvd)).enumerate() {
                for head in qr.chunks_mut(c.head_dim) {
                    rmsnorm(head, &l.q_norm, c.eps);
                    rope(head, start + i, c.rope_base);
                }
                for head in kr.chunks_mut(c.head_dim) {
                    rmsnorm(head, &l.k_norm, c.eps);
                    rope(head, start + i, c.rope_base);
                }
            }
            cache.k[li].extend(k.iter().map(|v| f32_to_f16(*v)));
            cache.v[li].extend(v.iter().map(|v| f32_to_f16(*v)));
            let att = self.attend(&q, &cache.k[li], &cache.v[li], start, t);
            let o = matmul(pool, &self.file.data, &l.o, &att, t);
            for (xv, ov) in x.iter_mut().zip(&o) {
                *xv += ov;
            }
            // ── feed-forward ──
            h.copy_from_slice(&x);
            for row in h.chunks_mut(c.dim) {
                rmsnorm(row, &l.ffn_norm, c.eps);
            }
            let mut g = matmul(pool, &self.file.data, &l.gate, &h, t);
            let u = matmul(pool, &self.file.data, &l.up, &h, t);
            for (gv, uv) in g.iter_mut().zip(&u) {
                *gv = silu(*gv) * uv;
            }
            let d = matmul(pool, &self.file.data, &l.down, &g, t);
            for (xv, dv) in x.iter_mut().zip(&d) {
                *xv += dv;
            }
        }
        cache.tokens.extend_from_slice(tokens);
        let mut last = x[(t - 1) * c.dim..].to_vec();
        rmsnorm(&mut last, &self.out_norm, c.eps);
        Ok(matmul(pool, &self.file.data, &self.out, &last, 1))
    }

    /// Causal attention for `t` new queries at positions `start..start+t`.
    /// For a prompt, heads run in parallel; one new token is cheap enough
    /// to do here.
    fn attend(&self, q: &[f32], k: &[u16], v: &[u16], start: usize, t: usize) -> Vec<f32> {
        let c = &self.cfg;
        let (hd, qd, kvd) = (c.head_dim, c.heads * c.head_dim, self.kv_dim());
        let group = c.heads / c.kv_heads;
        let scale = 1.0 / (hd as f32).sqrt();
        let parts = if t == 1 { 1 } else { self.threads };
        let per_head: Vec<Vec<f32>> = std::thread::scope(|s| {
            let work = move |heads: std::ops::Range<usize>| {
                let lut = f16_table();
                let mut out = Vec::new();
                let mut kf = vec![0.0f32; hd];
                for head in heads {
                    let kvh = head / group;
                    let mut o = vec![0.0f32; t * hd];
                    for i in 0..t {
                        // The query as the cache stores keys: f16.
                        let qi: Vec<f32> = q[i * qd + head * hd..][..hd]
                            .iter()
                            .map(|x| lut[usize::from(f32_to_f16(*x))])
                            .collect();
                        let n = start + i + 1;
                        let mut scores = Vec::with_capacity(n);
                        for p in 0..n {
                            for (d, kv) in kf.iter_mut().zip(&k[p * kvd + kvh * hd..][..hd]) {
                                *d = lut[usize::from(*kv)];
                            }
                            scores.push(dot_f32(&qi, &kf) * scale);
                        }
                        let max = scores.iter().fold(f32::NEG_INFINITY, |m, s| m.max(*s));
                        let mut sum = 0.0f32;
                        for s in &mut scores {
                            *s = softmax_term(*s - max);
                            sum += *s;
                        }
                        let oi = &mut o[i * hd..(i + 1) * hd];
                        for (p, s) in scores.iter().enumerate() {
                            let w = s / sum;
                            for (d, vv) in oi.iter_mut().zip(&v[p * kvd + kvh * hd..][..hd]) {
                                *d += w * lut[usize::from(*vv)];
                            }
                        }
                    }
                    out.push((head, o));
                }
                out
            };
            let mut ranges = ranges(c.heads, parts).into_iter();
            let first = ranges.next().unwrap_or(0..0);
            let jobs: Vec<_> = ranges.map(|heads| s.spawn(move || work(heads))).collect();
            let mut all = vec![Vec::new(); c.heads];
            for (head, o) in work(first).into_iter().chain(
                jobs.into_iter()
                    .flat_map(|j| j.join().expect("attention thread")),
            ) {
                all[head] = o;
            }
            all
        });
        let mut out = vec![0.0f32; t * qd];
        for (head, o) in per_head.iter().enumerate() {
            for i in 0..t {
                out[i * qd + head * hd..][..hd].copy_from_slice(&o[i * hd..(i + 1) * hd]);
            }
        }
        out
    }

    /// Bytes the weights take in memory.
    pub fn weight_bytes(&self) -> usize {
        self.file.data.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rope_at_zero_is_identity_and_preserves_length() {
        let mut x: Vec<f32> = (0..8).map(|i| i as f32 - 3.0).collect();
        let before = x.clone();
        rope(&mut x, 0, 10_000.0);
        assert_eq!(x, before);
        rope(&mut x, 17, 10_000.0);
        let n = |v: &[f32]| v.iter().map(|a| a * a).sum::<f32>();
        assert!((n(&x) - n(&before)).abs() < 1e-3);
    }
}
