//! Matrix products, shared out across threads. Both models use these.
//!
//! ```text
//!  with_pool(threads, |pool| ...)      worker threads for one pass of a model
//!  matmul(pool, file, W, x, t)         t input rows -> t output rows
//!                                      W's rows split among the threads; each
//!                                      weight row is read once for all t
//! ```
//!
//! A weight matrix is 8-bit blocks (Q8_0) — either a range of the model
//! file or quantized at load — or plain floats. With Q8_0 weights the input
//! is quantized to the same blocks, so the inner loop is integer dot
//! products (see `num.rs`).

use std::ops::Range;
use std::sync::{mpsc, Arc, Mutex};

use crate::gguf::DType;
use crate::num::{dot_f32, dot_q8, f32_to_f16, to_f32, QVec};

enum Mat {
    /// Q8_0 blocks at `start` in the model file.
    Q8File {
        start: usize,
    },
    /// Q8_0 blocks made at load time.
    Q8(Vec<u8>),
    F32(Vec<f32>),
}

/// A weight matrix, `rows` outputs by `cols` inputs.
pub struct Matrix {
    mat: Mat,
    pub rows: usize,
    pub cols: usize,
}

impl Matrix {
    /// Q8_0 blocks already in the file (`rows * cols / 32` blocks at `start`).
    pub fn q8_in_file(start: usize, rows: usize, cols: usize) -> Matrix {
        Matrix { mat: Mat::Q8File { start }, rows, cols }
    }

    pub fn floats(w: Vec<f32>, rows: usize, cols: usize) -> Matrix {
        Matrix { mat: Mat::F32(w), rows, cols }
    }

    /// Floats quantized to Q8_0 rows (`cols` must be a multiple of 32).
    pub fn quantized(w: &[f32], rows: usize, cols: usize) -> Matrix {
        let mut raw = Vec::with_capacity(rows * cols / 32 * 34);
        for row in w.chunks(cols) {
            let q = QVec::quantize(row);
            for (d, b) in q.scales.iter().zip(&q.q) {
                raw.extend_from_slice(&f32_to_f16(*d).to_le_bytes());
                raw.extend(b.iter().map(|v| *v as u8));
            }
        }
        Matrix { mat: Mat::Q8(raw), rows, cols }
    }

    fn q8_row<'a>(&'a self, file: &'a [u8], r: usize) -> &'a [u8] {
        let bytes = self.cols / 32 * 34;
        match &self.mat {
            Mat::Q8File { start } => &file[start + r * bytes..start + (r + 1) * bytes],
            Mat::Q8(raw) => &raw[r * bytes..(r + 1) * bytes],
            Mat::F32(_) => unreachable!("not a Q8_0 matrix"),
        }
    }

    /// One row as floats (an embedding lookup).
    pub fn row(&self, file: &[u8], r: usize, out: &mut [f32]) {
        match &self.mat {
            Mat::F32(w) => out.copy_from_slice(&w[r * self.cols..(r + 1) * self.cols]),
            _ => out.copy_from_slice(&to_f32(DType::Q8_0, self.q8_row(file, r))),
        }
    }

    /// Bytes this matrix holds in memory (not counting the file).
    pub fn own_bytes(&self) -> usize {
        match &self.mat {
            Mat::Q8File { .. } => 0,
            Mat::Q8(raw) => raw.len(),
            Mat::F32(w) => w.len() * 4,
        }
    }
}

/// A matrix product's input, ready for that matrix's kind.
enum Input {
    Quantized(Vec<QVec>),
    Floats(Vec<f32>),
}

/// Work handed to the pool's threads.
type Task<'a> = Box<dyn FnOnce() + Send + 'a>;

pub struct Pool<'a> {
    tx: mpsc::Sender<Task<'a>>,
    workers: usize,
}

/// Threads for one pass of a model. They start once and take every matrix
/// product's share from a channel — a model pass does a few hundred
/// products, and starting threads for each costs more than some of them.
pub fn with_pool<'a, R>(threads: usize, f: impl FnOnce(&Pool<'a>) -> R) -> R {
    std::thread::scope(|s| {
        let (tx, rx) = mpsc::channel::<Task<'a>>();
        let rx = Arc::new(Mutex::new(rx));
        for _ in 1..threads.max(1) {
            let rx = rx.clone();
            s.spawn(move || loop {
                let task = rx.lock().unwrap_or_else(|p| p.into_inner()).recv();
                match task {
                    Ok(task) => task(),
                    Err(_) => break, // the pass is over
                }
            });
        }
        f(&Pool { tx, workers: threads.max(1) - 1 })
    })
}

impl Pool<'_> {
    pub fn threads(&self) -> usize {
        self.workers + 1
    }
}

/// Split `0..n` into `parts` nearly equal ranges.
pub fn ranges(n: usize, parts: usize) -> Vec<Range<usize>> {
    let parts = parts.clamp(1, n.max(1));
    (0..parts).map(|p| n * p / parts..n * (p + 1) / parts).collect()
}

/// `x` is `t` rows of `m.cols`; returns `t` rows of `m.rows`. `file` is
/// the model file's bytes (for matrices that live there).
pub fn matmul<'a>(pool: &Pool<'a>, file: &'a [u8], m: &'a Matrix, x: &[f32], t: usize) -> Vec<f32> {
    debug_assert_eq!(x.len(), t * m.cols);
    let input = Arc::new(match m.mat {
        Mat::F32(_) => Input::Floats(x.to_vec()),
        _ => Input::Quantized(x.chunks(m.cols).map(QVec::quantize).collect()),
    });
    let parts = ranges(m.rows, pool.workers + 1);
    let (done_tx, done_rx) = mpsc::channel();
    for (p, range) in parts.iter().enumerate().skip(1) {
        let (input, done, range) = (input.clone(), done_tx.clone(), range.clone());
        let task: Task<'a> = Box::new(move || {
            let _ = done.send((p, rows(file, m, &input, range, t)));
        });
        if pool.tx.send(task).is_err() {
            unreachable!("the workers live as long as the pool");
        }
    }
    drop(done_tx);
    let mut by_part = vec![Vec::new(); parts.len()];
    by_part[0] = rows(file, m, &input, parts[0].clone(), t);
    for (p, v) in done_rx {
        by_part[p] = v;
    }
    // Computed as [row][token]; returned as [token][row].
    let by_row = by_part.concat();
    assert_eq!(by_row.len(), m.rows * t, "a worker thread failed");
    if t == 1 {
        return by_row;
    }
    let mut out = vec![0.0f32; t * m.rows];
    for r in 0..m.rows {
        for i in 0..t {
            out[i * m.rows + r] = by_row[r * t + i];
        }
    }
    out
}

/// Rows `range` of `m` times every input row: [row][token].
fn rows(file: &[u8], m: &Matrix, input: &Input, range: Range<usize>, t: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(range.len() * t);
    match (&m.mat, input) {
        (Mat::F32(w), Input::Floats(x)) => {
            for r in range {
                let w = &w[r * m.cols..(r + 1) * m.cols];
                out.extend(x.chunks(m.cols).map(|xi| dot_f32(w, xi)));
            }
        }
        (_, Input::Quantized(xs)) => {
            for r in range {
                let w = m.q8_row(file, r);
                out.extend(xs.iter().map(|xq| dot_q8(w, xq)));
            }
        }
        _ => unreachable!("the input is prepared for the matrix's kind"),
    }
    out
}

/// `matmul`, then add `bias` to every output row.
pub fn affine<'a>(pool: &Pool<'a>, file: &'a [u8], m: &'a Matrix, bias: &[f32], x: &[f32], t: usize) -> Vec<f32> {
    let mut y = matmul(pool, file, m, x, t);
    if !bias.is_empty() {
        for row in y.chunks_mut(m.rows) {
            for (v, b) in row.iter_mut().zip(bias) {
                *v += b;
            }
        }
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_cover_everything_once() {
        assert_eq!(ranges(10, 3), vec![0..3, 3..6, 6..10]);
        assert_eq!(ranges(2, 8).len(), 2);
    }

    #[test]
    fn threaded_products_match_the_obvious_loop() {
        let (rows, cols, t) = (37, 64, 5);
        let w: Vec<f32> = (0..rows * cols).map(|i| ((i * 31 % 17) as f32 - 8.0) / 9.0).collect();
        let x: Vec<f32> = (0..t * cols).map(|i| ((i * 7 % 13) as f32 - 6.0) / 5.0).collect();
        let mut want = Vec::new();
        for i in 0..t {
            for r in 0..rows {
                want.push((0..cols).map(|c| w[r * cols + c] * x[i * cols + c]).sum::<f32>());
            }
        }
        for threads in [1, 3] {
            let f = Matrix::floats(w.clone(), rows, cols);
            let q = Matrix::quantized(&w, rows, cols);
            let (got_f, got_q) = with_pool(threads, |p| (matmul(p, &[], &f, &x, t), matmul(p, &[], &q, &x, t)));
            for ((a, b), c) in want.iter().zip(&got_f).zip(&got_q) {
                assert!((a - b).abs() < 1e-4, "{a} vs {b}");
                assert!((a - c).abs() < 0.05 * a.abs().max(1.0), "{a} vs quantized {c}");
            }
        }
    }
}
