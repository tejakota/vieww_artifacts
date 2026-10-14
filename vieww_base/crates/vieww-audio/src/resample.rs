//! Sample-rate conversion: linear, cubic and windowed-sinc, one-shot or
//! streaming.
//!
//! The crate refuses to mix buffers at different rates *by accident*
//! ([`MixError::RateMismatch`](crate::MixError::RateMismatch)) because a
//! silent conversion is a silent quality decision. This module is the
//! conversion asked for *on purpose*: [`Samples::resampled`] for a buffer,
//! [`Resampler`] for a stream arriving in blocks of any size.
//!
//! # The sinc converter
//!
//! An ideal converter reconstructs the continuous signal with `sinc` and
//! samples it again; when going *down*, it first low-passes to the new
//! Nyquist so nothing folds. [`Quality::Sinc`] does exactly that with a
//! Kaiser-windowed sinc of `±taps` zero-crossings, stored as a polyphase
//! table (`PHASES` sub-sample positions, linearly interpolated between), so
//! the cost per output sample is `2·taps` multiply-adds whatever the ratio —
//! the scheme libsamplerate and SoX popularised. At 32 taps the residual
//! against an ideal sine is below −100 dB; the tests measure it.

use crate::fx::fir::{bessel_i0, kaiser_beta};
use crate::Samples;

/// Converter quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    /// Two-point linear interpolation. Fast; audible aliasing and droop.
    Linear,
    /// Four-point Hermite. Cheap and smooth; still aliases on the way down.
    Cubic,
    /// Kaiser-windowed sinc with this many zero-crossings per side
    /// (8 = fast, 16 = good, 32 = mastering).
    Sinc(usize),
}

const PHASES: usize = 1024;

/// A streaming converter for interleaved audio.
#[derive(Debug, Clone)]
pub struct Resampler {
    channels: usize,
    step: f64,
    quality: Quality,
    /// Taps either side of the interpolation point, in *input* frames.
    half: usize,
    table: Vec<f32>,
    /// Outputs produced so far; the position of output `n` is exactly
    /// `n · step` input frames, computed fresh each time so the result does
    /// not depend on how the input was chunked.
    produced: u64,
    /// Input frames discarded from the front of `buf`.
    dropped: u64,
    /// Interleaved input not yet consumed. Seeded with `half − 1` frames of
    /// silence so output 0 interpolates exactly at input frame 0: the
    /// converter adds no signal delay, only a look-ahead of `half` frames.
    buf: Vec<f32>,
}

impl Resampler {
    /// A converter from `from` Hz to `to` Hz for `channels` interleaved
    /// channels.
    ///
    /// # Panics
    /// If either rate is zero.
    #[must_use]
    pub fn new(from: u32, to: u32, channels: usize, quality: Quality) -> Self {
        assert!(from > 0 && to > 0, "rates must be positive");
        let step = f64::from(from) / f64::from(to);
        let (half, table) = match quality {
            Quality::Linear => (1, Vec::new()),
            Quality::Cubic => (2, Vec::new()),
            Quality::Sinc(t) => {
                // Cut at the lower Nyquist, with a little guard band. When
                // going down, the sinc stretches by 1/cutoff, and so must its
                // support, or a 3:1 reduction gets a third of its taps.
                let cutoff = (1.0 / step).min(1.0) * 0.96;
                #[expect(
                    clippy::cast_precision_loss,
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "tap counts"
                )]
                let half = ((t.clamp(2, 128) as f64) / cutoff).ceil() as usize;
                (half, build_table(half, cutoff))
            }
        };
        Self {
            channels: channels.max(1),
            step,
            quality,
            half,
            table,
            produced: 0,
            dropped: 0,
            buf: vec![0.0; (half - 1) * channels.max(1)],
        }
    }

    /// Input frames that must arrive before an output can be produced —
    /// the look-ahead a real-time caller budgets for. The signal itself is
    /// not delayed.
    #[must_use]
    pub const fn lookahead(&self) -> usize {
        self.half
    }

    /// Convert `input` (interleaved), appending to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let ch = self.channels;
        self.buf.extend_from_slice(input);
        let frames = self.buf.len() / ch;
        let need = 2 * self.half; // frames of support
        loop {
            #[expect(
                clippy::cast_precision_loss,
                reason = "2^53 outputs is a century of audio"
            )]
            let pos = self.produced as f64 * self.step;
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "non-negative"
            )]
            let abs = pos.floor() as u64;
            #[expect(clippy::cast_possible_truncation, reason = "relative index fits")]
            let i = (abs - self.dropped) as usize;
            if i + need > frames {
                break;
            }
            #[expect(clippy::cast_possible_truncation, reason = "fraction")]
            let frac = (pos - pos.floor()) as f32;
            for c in 0..ch {
                out.push(self.tap(i, frac, c));
            }
            self.produced += 1;
        }
        // Drop frames no future output can reach.
        #[expect(clippy::cast_precision_loss, reason = "see above")]
        let next = (self.produced as f64 * self.step).floor();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "non-negative"
        )]
        let reach = next as u64;
        #[expect(clippy::cast_possible_truncation, reason = "relative index fits")]
        let drop = ((reach.saturating_sub(self.dropped)) as usize).min(frames);
        if drop > 0 {
            self.buf.drain(..drop * ch);
            self.dropped += drop as u64;
        }
    }

    /// Flush the tail with silence so every input frame contributes.
    pub fn finish(&mut self, out: &mut Vec<f32>) {
        let pad = vec![0.0; 2 * self.half * self.channels];
        self.process(&pad, out);
    }

    /// The output sample for support starting at frame `i` (the
    /// interpolation point is frame `i + half − 1 + frac`).
    fn tap(&self, i: usize, frac: f32, c: usize) -> f32 {
        let ch = self.channels;
        let x = |k: usize| self.buf[(i + k) * ch + c];
        match self.quality {
            Quality::Linear => {
                let a = x(0);
                let b = x(1);
                a + (b - a) * frac
            }
            Quality::Cubic => {
                let (y0, y1, y2, y3) = (x(0), x(1), x(2), x(3));
                let c1 = 0.5 * (y2 - y0);
                let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
                let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
                ((c3 * frac + c2) * frac + c1) * frac + y1
            }
            Quality::Sinc(_) => {
                let n = 2 * self.half;
                #[expect(clippy::cast_precision_loss, reason = "phase count")]
                let p = f64::from(frac) * PHASES as f64;
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "0..PHASES"
                )]
                let p0 = (p.floor() as usize).min(PHASES - 1);
                let w = p - p.floor();
                let r0 = &self.table[p0 * n..(p0 + 1) * n];
                let r1 = &self.table[(p0 + 1) * n..(p0 + 2) * n];
                let mut acc = 0.0f64;
                for k in 0..n {
                    let h = f64::from(r0[k]) + (f64::from(r1[k]) - f64::from(r0[k])) * w;
                    acc += f64::from(x(k)) * h;
                }
                #[expect(clippy::cast_possible_truncation, reason = "f32 audio")]
                let y = acc as f32;
                y
            }
        }
    }
}

/// Polyphase table: `PHASES + 1` rows of `2·half` taps. Row `p` holds
/// `h(k − (half − 1) − p/PHASES)` for `k = 0..2·half`.
fn build_table(half: usize, cutoff: f64) -> Vec<f32> {
    let n = 2 * half;
    let beta = kaiser_beta(120.0);
    let i0b = bessel_i0(beta);
    #[expect(clippy::cast_precision_loss, reason = "small")]
    let hw = half as f64;
    let mut t = Vec::with_capacity((PHASES + 1) * n);
    for p in 0..=PHASES {
        #[expect(clippy::cast_precision_loss, reason = "small")]
        let frac = p as f64 / PHASES as f64;
        for k in 0..n {
            #[expect(clippy::cast_precision_loss, reason = "small")]
            let x = k as f64 - (hw - 1.0) - frac;
            let s = if x.abs() < 1e-12 {
                1.0
            } else {
                let a = std::f64::consts::PI * cutoff * x;
                a.sin() / a
            };
            let r = x / hw;
            let win = if r.abs() >= 1.0 {
                0.0
            } else {
                bessel_i0(beta * (1.0 - r * r).sqrt()) / i0b
            };
            #[expect(clippy::cast_possible_truncation, reason = "f32 table")]
            t.push((cutoff * s * win) as f32);
        }
    }
    t
}

impl Samples {
    /// This buffer converted to `rate`, latency-compensated so the result
    /// lines up with the original in time.
    #[must_use]
    pub fn resampled(&self, rate: u32, quality: Quality) -> Self {
        if rate == self.rate || self.data.is_empty() {
            return Self {
                data: self.data.clone(),
                channels: self.channels,
                rate,
            };
        }
        let ch = usize::from(self.channels.max(1));
        let mut r = Resampler::new(self.rate, rate, ch, quality);
        let mut out = Vec::new();
        r.process(&self.data, &mut out);
        r.finish(&mut out);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "frame counts"
        )]
        let want =
            ((self.frames() as f64) * f64::from(rate) / f64::from(self.rate)).round() as usize;
        out.resize(want * ch, 0.0);
        Self {
            data: out,
            channels: self.channels,
            rate,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    fn sine(hz: f64, rate: u32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_precision_loss,
                    reason = "test"
                )]
                let v = (std::f64::consts::TAU * hz * i as f64 / f64::from(rate)).sin() as f32;
                v
            })
            .collect()
    }

    /// Residual (dB re. signal) after least-squares fitting a sine at `hz`.
    fn residual_db(x: &[f32], hz: f64, rate: u32) -> f64 {
        let (mut ss, mut sc, mut cc, mut xs, mut xc) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (i, v) in x.iter().enumerate() {
            #[expect(clippy::cast_precision_loss, reason = "test")]
            let th = std::f64::consts::TAU * hz * i as f64 / f64::from(rate);
            let (s, c) = th.sin_cos();
            ss += s * s;
            sc += s * c;
            cc += c * c;
            xs += f64::from(*v) * s;
            xc += f64::from(*v) * c;
        }
        let det = ss * cc - sc * sc;
        let a = (xs * cc - xc * sc) / det;
        let b = (xc * ss - xs * sc) / det;
        let mut e_sig = 0.0;
        let mut e_res = 0.0;
        for (i, v) in x.iter().enumerate() {
            #[expect(clippy::cast_precision_loss, reason = "test")]
            let th = std::f64::consts::TAU * hz * i as f64 / f64::from(rate);
            let fit = a * th.sin() + b * th.cos();
            e_sig += fit * fit;
            e_res += (f64::from(*v) - fit).powi(2);
        }
        10.0 * (e_res / e_sig).log10()
    }

    #[test]
    fn sinc_44k1_to_48k_is_clean() {
        let x = Samples::mono(sine(997.0, 44_100, 44_100), 44_100);
        let y = x.resampled(48_000, Quality::Sinc(32));
        assert_eq!(y.frames(), 48_000);
        let core = &y.data[2_000..46_000];
        let r = residual_db(core, 997.0, 48_000);
        assert!(r < -100.0, "residual {r} dB");
    }

    #[test]
    fn quality_orders_as_named() {
        let x = Samples::mono(sine(6_000.0, 44_100, 22_050), 44_100);
        let res = |q| {
            let y = x.resampled(48_000, q);
            residual_db(&y.data[1_000..23_000], 6_000.0, 48_000)
        };
        let (l, c, s) = (
            res(Quality::Linear),
            res(Quality::Cubic),
            res(Quality::Sinc(16)),
        );
        assert!(l > c && c > s, "{l} {c} {s}");
        assert!(s < -80.0);
    }

    #[test]
    fn downsampling_removes_what_cannot_be_represented() {
        // 10 kHz cannot exist at 16 kHz; it must vanish, not fold to 6 kHz.
        let x = Samples::mono(sine(10_000.0, 48_000, 48_000), 48_000);
        let y = x.resampled(16_000, Quality::Sinc(32));
        let core = &y.data[500..15_500];
        let rms = (core.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / 15_000.0).sqrt();
        assert!(20.0 * rms.log10() < -80.0, "{}", 20.0 * rms.log10());
        // And a 3 kHz tone survives at full level.
        let x = Samples::mono(sine(3_000.0, 48_000, 48_000), 48_000);
        let y = x.resampled(16_000, Quality::Sinc(32));
        let peak = y.data[500..15_500]
            .iter()
            .fold(0.0f32, |m, v| m.max(v.abs()));
        assert!((peak - 1.0).abs() < 0.01, "{peak}");
    }

    #[test]
    fn resampled_buffers_line_up_in_time() {
        // An impulse at 1000 frames lands at 1000·(48/44.1) frames.
        let mut d = vec![0.0; 4_410];
        d[1_000] = 1.0;
        let y = Samples::mono(d, 44_100).resampled(48_000, Quality::Sinc(16));
        let peak = y
            .data
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .unwrap()
            .0;
        assert!((peak as i64 - 1088).abs() <= 1, "{peak}");
    }

    #[test]
    fn streaming_in_odd_chunks_equals_one_shot() {
        let mut rng = Rng::new(2);
        let x: Vec<f32> = (0..20_000).map(|_| rng.bipolar()).collect();
        let mut a = Resampler::new(48_000, 44_100, 2, Quality::Sinc(16));
        let mut one = Vec::new();
        a.process(&x, &mut one);
        let mut b = Resampler::new(48_000, 44_100, 2, Quality::Sinc(16));
        let mut many = Vec::new();
        let mut i = 0;
        let mut k = 1;
        while i < x.len() {
            let n = (k * 2 * 37 % 1000 + 2).min(x.len() - i);
            let n = n - n % 2;
            let n = n.max(2).min(x.len() - i);
            b.process(&x[i..i + n], &mut many);
            i += n;
            k += 1;
        }
        assert_eq!(one.len(), many.len());
        for (p, q) in one.iter().zip(&many) {
            assert!((p - q).abs() < 1e-6);
        }
    }

    #[test]
    fn stereo_channels_stay_separate() {
        let l = sine(500.0, 44_100, 4_410);
        let mut inter = Vec::new();
        for v in &l {
            inter.push(*v);
            inter.push(0.0);
        }
        let y = Samples::stereo(inter, 44_100).resampled(48_000, Quality::Sinc(16));
        let right_peak = y
            .data
            .iter()
            .skip(1)
            .step_by(2)
            .fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(right_peak < 1e-6);
    }
}
