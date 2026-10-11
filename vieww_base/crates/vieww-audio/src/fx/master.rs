//! Spatial and modulation effects — reverb, convolution, chorus, stereo
//! width, saturation and a mastering limiter: Web Audio's `ConvolverNode`,
//! the reverb/chorus every DAW and TouchDesigner's audio CHOPs ship, and the
//! bus processing a score needs to sit in a room rather than in a wire.
//!
//! * [`Reverb`] — a Freeverb-topology algorithmic reverb (Jezar's eight
//!   damped comb filters in parallel, four all-passes in series, per
//!   channel with a stereo spread), with pre-delay, room size, damping,
//!   width and wet/dry.
//! * [`convolve`] — FFT overlap-add convolution of a signal with an impulse
//!   response (a real room, a cabinet, a plate), using the crate's own
//!   radix-2 FFT; [`impulse_response`] synthesises a decaying noise IR for
//!   a given RT60.
//! * [`Chorus`] — modulated delay lines (two voices, quadrature LFOs).
//! * [`stereo_width`] — mid/side width.
//! * [`saturate`] — tanh soft clipping with drive.
//! * [`Limiter`] — look-ahead peak limiter with a smooth release, so a
//!   master never clips.

use crate::analysis::fft;

/// In-place complex FFT on (re, im) pairs; `inverse` via the conjugate
/// trick, scaled by 1/n.
fn fft_c(x: &mut [(f32, f32)], inverse: bool) {
    let n = x.len();
    let mut re: Vec<f32> = x.iter().map(|v| v.0).collect();
    let mut im: Vec<f32> = x.iter().map(|v| if inverse { -v.1 } else { v.1 }).collect();
    fft(&mut re, &mut im);
    #[allow(clippy::cast_precision_loss)]
    let k = if inverse { 1.0 / n as f32 } else { 1.0 };
    for (i, v) in x.iter_mut().enumerate() {
        *v = (re[i] * k, if inverse { -im[i] * k } else { im[i] });
    }
}
use crate::Samples;

/// A feedback comb with one-pole damping in the loop (Freeverb's).
#[derive(Debug, Clone)]
struct Comb {
    buf: Vec<f32>,
    i: usize,
    store: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            i: 0,
            store: 0.0,
        }
    }
    fn tick(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let out = self.buf[self.i];
        self.store = out * (1.0 - damp) + self.store * damp;
        self.buf[self.i] = x + self.store * feedback;
        self.i = (self.i + 1) % self.buf.len();
        out
    }
}

#[derive(Debug, Clone)]
struct AllPass {
    buf: Vec<f32>,
    i: usize,
}

impl AllPass {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            i: 0,
        }
    }
    fn tick(&mut self, x: f32) -> f32 {
        let b = self.buf[self.i];
        let out = b - x;
        self.buf[self.i] = x + b * 0.5;
        self.i = (self.i + 1) % self.buf.len();
        out
    }
}

/// Freeverb. See the [module docs](self).
#[derive(Debug, Clone)]
pub struct Reverb {
    combs: [Vec<Comb>; 2],
    allpasses: [Vec<AllPass>; 2],
    pre: Vec<f32>,
    pre_i: usize,
    /// 0..1 — longer tails as it grows.
    pub room_size: f32,
    /// 0..1 — high frequencies die faster as it grows.
    pub damping: f32,
    pub wet: f32,
    pub dry: f32,
    /// 0..1 stereo spread of the tail.
    pub width: f32,
}

impl Reverb {
    /// A reverb at `rate` with `pre_delay` seconds before the tail.
    #[must_use]
    pub fn new(rate: u32, pre_delay: f32) -> Self {
        // Freeverb's tunings at 44.1 kHz, scaled to `rate`.
        const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
        const ALLPASS: [usize; 4] = [556, 441, 341, 225];
        const SPREAD: usize = 23;
        #[allow(clippy::cast_precision_loss)]
        let k = rate as f32 / 44_100.0;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let scale = |n: usize| ((n as f32) * k) as usize;
        let make = |off: usize| {
            (
                COMBS
                    .iter()
                    .map(|n| Comb::new(scale(n + off)))
                    .collect::<Vec<_>>(),
                ALLPASS
                    .iter()
                    .map(|n| AllPass::new(scale(n + off)))
                    .collect::<Vec<_>>(),
            )
        };
        let (cl, al) = make(0);
        let (cr, ar) = make(SPREAD);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let pre = ((pre_delay.max(0.0) * rate as f32) as usize).max(1);
        Self {
            combs: [cl, cr],
            allpasses: [al, ar],
            pre: vec![0.0; pre * 2],
            pre_i: 0,
            room_size: 0.8,
            damping: 0.4,
            wet: 0.3,
            dry: 0.9,
            width: 1.0,
        }
    }

    /// One stereo frame.
    pub fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        let n = self.pre.len() / 2;
        let (pl, pr) = (self.pre[self.pre_i * 2], self.pre[self.pre_i * 2 + 1]);
        self.pre[self.pre_i * 2] = l;
        self.pre[self.pre_i * 2 + 1] = r;
        self.pre_i = (self.pre_i + 1) % n;
        let input = (pl + pr) * 0.015;
        let feedback = 0.7 + 0.28 * self.room_size.clamp(0.0, 1.0);
        let damp = self.damping.clamp(0.0, 1.0) * 0.4;
        let mut out = [0.0f32; 2];
        for (ch, out) in out.iter_mut().enumerate() {
            let mut s = 0.0;
            for c in &mut self.combs[ch] {
                s += c.tick(input, feedback, damp);
            }
            for a in &mut self.allpasses[ch] {
                s = a.tick(s);
            }
            *out = s;
        }
        let w1 = self.wet * (self.width / 2.0 + 0.5);
        let w2 = self.wet * ((1.0 - self.width) / 2.0);
        (
            out[0] * w1 + out[1] * w2 + l * self.dry,
            out[1] * w1 + out[0] * w2 + r * self.dry,
        )
    }

    /// Process a whole buffer (mono input is spread to stereo), appending
    /// `tail` seconds so the decay is not cut.
    #[must_use]
    pub fn process(&mut self, s: &Samples, tail: f32) -> Samples {
        let frames = s.frames();
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let extra = (tail.max(0.0) * s.rate as f32) as usize;
        let mut out = Vec::with_capacity((frames + extra) * 2);
        for i in 0..frames + extra {
            let (l, r) = if i < frames {
                if s.channels == 2 {
                    (s.data[i * 2], s.data[i * 2 + 1])
                } else {
                    (s.data[i], s.data[i])
                }
            } else {
                (0.0, 0.0)
            };
            let (a, b) = self.tick(l, r);
            out.push(a);
            out.push(b);
        }
        Samples::stereo(out, s.rate)
    }
}

/// FFT overlap-add convolution of mono `signal` with `ir`; the output is
/// `signal.len() + ir.len() − 1` long.
#[must_use]
pub fn convolve(signal: &[f32], ir: &[f32]) -> Vec<f32> {
    if signal.is_empty() || ir.is_empty() {
        return Vec::new();
    }
    let block = ir.len().next_power_of_two().max(256);
    let n = (block * 2).next_power_of_two();
    // IR spectrum, once.
    let mut h: Vec<(f32, f32)> = ir.iter().map(|v| (*v, 0.0)).collect();
    h.resize(n, (0.0, 0.0));
    fft_c(&mut h, false);
    let mut out = vec![0.0f32; signal.len() + ir.len() - 1];
    let mut start = 0;
    while start < signal.len() {
        let end = (start + block).min(signal.len());
        let mut x: Vec<(f32, f32)> = signal[start..end].iter().map(|v| (*v, 0.0)).collect();
        x.resize(n, (0.0, 0.0));
        fft_c(&mut x, false);
        for (a, b) in x.iter_mut().zip(&h) {
            *a = (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0);
        }
        fft_c(&mut x, true);
        for (i, v) in x.iter().enumerate() {
            if start + i < out.len() {
                out[start + i] += v.0;
            }
        }
        start = end;
    }
    out
}

/// A synthetic room: exponentially decaying noise reaching −60 dB at
/// `rt60` seconds, with early reflections and a low-passed late tail.
#[must_use]
pub fn impulse_response(rate: u32, rt60: f32, seed: u32) -> Vec<f32> {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let n = ((rt60.max(0.05) * rate as f32) as usize).max(16);
    let mut s = seed.wrapping_mul(2_654_435_761).max(1);
    let mut rnd = || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        #[allow(clippy::cast_precision_loss)]
        let v = (s >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
        v
    };
    #[allow(clippy::cast_precision_loss)]
    let k = (1000.0f32).ln() / (rt60.max(0.05) * rate as f32);
    let mut ir = Vec::with_capacity(n);
    let mut lp = 0.0f32;
    for i in 0..n {
        #[allow(clippy::cast_precision_loss)]
        let env = (-k * i as f32).exp();
        lp = lp * 0.6 + rnd() * 0.4;
        ir.push(lp * env);
    }
    // Early reflections.
    for (t, g) in [(0.007, 0.6), (0.013, 0.45), (0.021, 0.35), (0.034, 0.25)] {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let i = (t * rate as f32) as usize;
        if i < n {
            ir[i] += g;
        }
    }
    ir[0] = 1.0;
    let norm = ir.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
    ir.iter().map(|v| v / norm).collect()
}

/// Two-voice chorus.
#[derive(Debug, Clone)]
pub struct Chorus {
    buf: Vec<[f32; 2]>,
    i: usize,
    phase: f32,
    rate_hz: f32,
    sr: f32,
    /// Modulation depth, seconds.
    pub depth: f32,
    /// Centre delay, seconds.
    pub delay: f32,
    pub mix: f32,
}

impl Chorus {
    #[must_use]
    pub fn new(rate: u32, rate_hz: f32) -> Self {
        #[allow(clippy::cast_precision_loss)]
        let sr = rate as f32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let len = (sr * 0.06) as usize;
        Self {
            buf: vec![[0.0; 2]; len.max(4)],
            i: 0,
            phase: 0.0,
            rate_hz,
            sr,
            depth: 0.003,
            delay: 0.015,
            mix: 0.35,
        }
    }

    fn read(&self, ch: usize, delay_s: f32) -> f32 {
        let n = self.buf.len();
        #[allow(clippy::cast_precision_loss)]
        let d = (delay_s * self.sr).clamp(1.0, (n - 2) as f32);
        #[allow(clippy::cast_precision_loss)]
        let pos = self.i as f32 + n as f32 - d;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let a = pos.floor() as usize % n;
        let t = pos - pos.floor();
        self.buf[a][ch] * (1.0 - t) + self.buf[(a + 1) % n][ch] * t
    }

    pub fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        self.buf[self.i] = [l, r];
        let tau = std::f32::consts::TAU;
        let dl = self.delay + self.depth * (self.phase * tau).sin();
        let dr = self.delay + self.depth * (self.phase * tau + tau / 4.0).sin();
        let wl = self.read(0, dl);
        let wr = self.read(1, dr);
        self.i = (self.i + 1) % self.buf.len();
        self.phase = (self.phase + self.rate_hz / self.sr).fract();
        (
            l * (1.0 - self.mix) + wl * self.mix,
            r * (1.0 - self.mix) + wr * self.mix,
        )
    }
}

/// Mid/side width on an interleaved stereo buffer: 0 = mono, 1 = as is,
/// above 1 = wider.
pub fn stereo_width(stereo: &mut [f32], width: f32) {
    for f in stereo.as_chunks_mut::<2>().0 {
        let m = (f[0] + f[1]) * 0.5;
        let s = (f[0] - f[1]) * 0.5 * width;
        f[0] = m + s;
        f[1] = m - s;
    }
}

/// tanh soft clipping with `drive` ≥ 1: ±1 maps to ±1, everything is
/// bounded by ±1/tanh(drive)·1 = ±1 at the rails, and harmonics grow with the
/// drive — tape/valve warmth on a bus.
pub fn saturate(buf: &mut [f32], drive: f32) {
    let d = drive.max(1.0);
    let norm = 1.0 / d.tanh();
    for s in buf {
        *s = (*s * d).tanh() * norm;
    }
}

/// Look-ahead peak limiter: output never exceeds `ceiling`.
#[derive(Debug, Clone)]
pub struct Limiter {
    pub ceiling: f32,
    pub lookahead: usize,
    pub release: f32,
}

impl Limiter {
    #[must_use]
    pub fn new(rate: u32, ceiling: f32) -> Self {
        Self {
            ceiling,
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss
            )]
            lookahead: (rate as f32 * 0.005) as usize,
            #[allow(clippy::cast_precision_loss)]
            release: (-1.0 / (rate as f32 * 0.08)).exp(),
        }
    }

    /// Limit an interleaved buffer of `channels` in place.
    pub fn process(&self, data: &mut [f32], channels: usize) {
        let ch = channels.max(1);
        let frames = data.len() / ch;
        // Required gain per frame, then a running minimum over the window
        // ahead (so the gain is already down when the peak arrives), then a
        // smooth release.
        let need: Vec<f32> = (0..frames)
            .map(|f| {
                let p = (0..ch).map(|c| data[f * ch + c].abs()).fold(0.0, f32::max);
                if p > self.ceiling {
                    self.ceiling / p
                } else {
                    1.0
                }
            })
            .collect();
        let la = self.lookahead.max(1);
        let mut gain = 1.0f32;
        let mut window = std::collections::VecDeque::new();
        for f in 0..frames {
            // Monotone deque minimum over need[f ..= f + la].
            let ahead = f + la;
            if f == 0 {
                for k in 0..=la.min(frames - 1) {
                    while window.back().is_some_and(|&j: &usize| need[j] >= need[k]) {
                        window.pop_back();
                    }
                    window.push_back(k);
                }
            } else if ahead < frames {
                while window
                    .back()
                    .is_some_and(|&j: &usize| need[j] >= need[ahead])
                {
                    window.pop_back();
                }
                window.push_back(ahead);
            }
            while window.front().is_some_and(|&j| j < f) {
                window.pop_front();
            }
            let target = window.front().map_or(1.0, |&j| need[j]);
            gain = if target < gain {
                target
            } else {
                target + (gain - target) * self.release
            };
            for c in 0..ch {
                let v = data[f * ch + c] * gain;
                data[f * ch + c] = v.clamp(-self.ceiling, self.ceiling);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reverb_tail_rings_after_the_input_stops_and_decays() {
        let mut v = Reverb::new(48_000, 0.01);
        let mut imp = vec![0.0f32; 48_000];
        imp[0] = 1.0;
        let out = v.process(&Samples::mono(imp, 48_000), 2.0);
        let energy = |a: usize, b: usize| out.data[a * 2..b * 2].iter().map(|x| x * x).sum::<f32>();
        let early = energy(2_000, 12_000);
        let late = energy(60_000, 70_000);
        assert!(early > 1e-4, "{early}");
        assert!(late < early, "{late} {early}");
        assert!(out.data.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn convolution_with_a_unit_impulse_is_identity_and_matches_direct_form() {
        let sig: Vec<f32> = (0..1000).map(|i| ((i as f32) * 0.05).sin()).collect();
        let out = convolve(&sig, &[1.0]);
        for (a, b) in out.iter().zip(&sig) {
            assert!((a - b).abs() < 1e-4);
        }
        let ir = [0.5, 0.25, -0.125, 0.0625];
        let fast = convolve(&sig[..300], &ir);
        for n in 0..303 {
            let mut d = 0.0;
            for (k, h) in ir.iter().enumerate() {
                if n >= k && n - k < 300 {
                    d += sig[n - k] * h;
                }
            }
            assert!((fast[n] - d).abs() < 1e-4, "{n}");
        }
    }

    #[test]
    fn a_synthetic_room_decays_by_sixty_db_at_its_rt60() {
        let ir = impulse_response(8_000, 0.5, 3);
        assert_eq!(ir.len(), 4_000);
        let rms = |a: usize, b: usize| {
            (ir[a..b].iter().map(|v| v * v).sum::<f32>() / (b - a) as f32).sqrt()
        };
        let head = rms(400, 800);
        let tail = rms(3_600, 4_000);
        let db = 20.0 * (tail / head).log10();
        assert!(db < -40.0, "{db}");
    }

    #[test]
    fn chorus_and_width_keep_mono_compatible_energy() {
        let mut c = Chorus::new(48_000, 0.8);
        let mut peak = 0.0f32;
        for i in 0..48_000 {
            let x = ((i as f32) * 0.03).sin() * 0.5;
            let (l, r) = c.tick(x, x);
            peak = peak.max(l.abs()).max(r.abs());
        }
        assert!(peak <= 0.55);
        let mut s = vec![1.0, 0.0, 0.0, 1.0];
        stereo_width(&mut s, 0.0);
        assert_eq!(s, vec![0.5, 0.5, 0.5, 0.5]);
    }

    #[test]
    fn the_limiter_never_lets_a_peak_through() {
        let mut d: Vec<f32> = (0..20_000)
            .map(|i| ((i as f32) * 0.01).sin() * if i > 10_000 { 3.0 } else { 0.5 })
            .collect();
        Limiter::new(48_000, 0.9).process(&mut d, 1);
        assert!(d.iter().all(|v| v.abs() <= 0.9 + 1e-6));
        // Quiet part untouched (before the look-ahead window of the loud).
        assert!((d[1_000] - ((1_000f32) * 0.01).sin() * 0.5).abs() < 1e-4);
    }

    #[test]
    fn saturation_is_odd_and_bounded() {
        let mut b = vec![-10.0, -0.01, 0.0, 0.01, 10.0];
        saturate(&mut b, 3.0);
        assert!(b[0] < 0.0 && b[4] > 0.0 && (b[0] + b[4]).abs() < 1e-6);
        assert!(b.iter().all(|v| v.abs() <= 1.5));
    }
}
