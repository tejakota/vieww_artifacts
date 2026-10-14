//! Reverb: algorithmic (Freeverb's Schroeder–Moorer network, and a feedback
//! delay network) and convolution.
//!
//! * [`Freeverb`] — Jezar's public-domain design: eight damped feedback combs
//!   in parallel into four series all-passes per channel, the right channel's
//!   delays offset by 23 samples for width. Bright, dense, the sound of a
//!   thousand plug-ins.
//! * [`Fdn`] — an eight-line feedback delay network with a Hadamard mixing
//!   matrix (lossless and maximally diffusing), slowly modulated delays to
//!   break up metallic ringing, and per-line gains computed from the
//!   requested RT60 so the decay time is a *specification*, not a knob:
//!   `g = 10^(−3·d / (RT60·fs))`. A one-pole in each loop makes highs decay
//!   faster, as air does. The test measures RT60 by Schroeder backward
//!   integration and checks it.
//! * [`ConvolutionReverb`] — an impulse response through the partitioned
//!   [`super::fir::Convolver`]; [`synthetic_ir`] builds a
//!   plausible room from shaped, decaying stereo noise when no recording is
//!   at hand.

use super::filter::{Comb, DelayLine, OnePole, SchroederAllpass};
use super::fir::Convolver;
use super::Stereo;
use crate::dsp::Processor;
use crate::rng::Rng;

const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
const SPREAD: usize = 23;

/// Freeverb.
#[derive(Debug, Clone)]
pub struct Freeverb {
    combs: [Vec<Comb>; 2],
    alls: [Vec<SchroederAllpass>; 2],
    /// Wet level.
    pub wet: f32,
    /// Dry level.
    pub dry: f32,
    /// Stereo width 0..=1.
    pub width: f32,
}

impl Freeverb {
    /// A reverb with `room` (0..=1, decay) and `damp` (0..=1, high-cut in
    /// the tail), scaled from Freeverb's 44.1 kHz tunings to `rate`.
    #[must_use]
    pub fn new(room: f32, damp: f32, rate: f32) -> Self {
        let scale = |n: usize| {
            #[expect(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "delay lengths"
            )]
            let v = (n as f32 * rate / 44_100.0).round() as usize;
            v.max(1)
        };
        let feedback = room.clamp(0.0, 1.0) * 0.28 + 0.7;
        let damp = damp.clamp(0.0, 1.0) * 0.4;
        let combs = [0, SPREAD].map(|off| {
            COMBS
                .iter()
                .map(|&n| Comb::new(scale(n + off), feedback, damp))
                .collect()
        });
        let alls = [0, SPREAD].map(|off| {
            ALLPASSES
                .iter()
                .map(|&n| SchroederAllpass::new(scale(n + off), 0.5))
                .collect()
        });
        Self {
            combs,
            alls,
            wet: 0.33,
            dry: 0.7,
            width: 1.0,
        }
    }
}

impl Stereo for Freeverb {
    fn process_stereo(&mut self, l: f32, r: f32) -> (f32, f32) {
        let input = (l + r) * 0.015;
        let mut out = [0.0f32; 2];
        for (ch, o) in out.iter_mut().enumerate() {
            let mut acc = 0.0;
            for c in &mut self.combs[ch] {
                acc += c.process(input);
            }
            for a in &mut self.alls[ch] {
                acc = a.process(acc);
            }
            *o = acc;
        }
        let w1 = self.wet * (self.width / 2.0 + 0.5);
        let w2 = self.wet * ((1.0 - self.width) / 2.0);
        (
            out[0] * w1 + out[1] * w2 + l * self.dry,
            out[1] * w1 + out[0] * w2 + r * self.dry,
        )
    }
}

const FDN_LINES: usize = 8;
const FDN_DELAYS_MS: [f32; FDN_LINES] = [29.7, 37.1, 41.1, 43.7, 53.0, 59.9, 67.3, 79.1];

/// An eight-line feedback delay network.
#[derive(Debug, Clone)]
pub struct Fdn {
    lines: Vec<DelayLine>,
    lengths: [f32; FDN_LINES],
    gains: [f32; FDN_LINES],
    damping: Vec<OnePole>,
    lfo_phase: [f64; FDN_LINES],
    lfo_inc: f64,
    /// Modulation depth in samples.
    pub mod_depth: f32,
    pre: DelayLine,
    pre_len: usize,
    /// Wet level.
    pub wet: f32,
    /// Dry level.
    pub dry: f32,
}

impl Fdn {
    /// A network with decay `rt60` seconds, `damping_hz` (the loop low-pass
    /// corner; lower = darker tail), and `predelay` seconds.
    #[must_use]
    pub fn new(rt60: f32, damping_hz: f32, predelay: f32, rate: f32) -> Self {
        let mut lengths = [0.0; FDN_LINES];
        let mut gains = [0.0; FDN_LINES];
        for i in 0..FDN_LINES {
            lengths[i] = FDN_DELAYS_MS[i] * 0.001 * rate;
            gains[i] = 10f32.powf(-3.0 * lengths[i] / (rt60.max(0.01) * rate));
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "lengths"
        )]
        let lines = lengths
            .iter()
            .map(|l| DelayLine::new(*l as usize + 64))
            .collect();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "lengths"
        )]
        let pre_len = (predelay.max(0.0) * rate) as usize;
        let mut lfo_phase = [0.0; FDN_LINES];
        for (i, p) in lfo_phase.iter_mut().enumerate() {
            *p = f64::from(u8::try_from(i).unwrap_or(0)) / 8.0;
        }
        Self {
            lines,
            lengths,
            gains,
            damping: (0..FDN_LINES)
                .map(|_| OnePole::new(damping_hz, rate))
                .collect(),
            lfo_phase,
            lfo_inc: 0.5 / f64::from(rate),
            mod_depth: 6.0,
            pre: DelayLine::new(pre_len + 4),
            pre_len,
            wet: 0.3,
            dry: 1.0,
        }
    }

    /// The per-line loop gains (for inspection).
    #[must_use]
    pub const fn gains(&self) -> &[f32; FDN_LINES] {
        &self.gains
    }

    fn hadamard(v: &mut [f32; FDN_LINES]) {
        let mut h = 1;
        while h < FDN_LINES {
            for i in (0..FDN_LINES).step_by(h * 2) {
                for j in i..i + h {
                    let a = v[j];
                    let b = v[j + h];
                    v[j] = a + b;
                    v[j + h] = a - b;
                }
            }
            h *= 2;
        }
        let k = 1.0 / (FDN_LINES as f32).sqrt();
        for x in v.iter_mut() {
            *x *= k;
        }
    }
}

impl Stereo for Fdn {
    fn process_stereo(&mut self, l: f32, r: f32) -> (f32, f32) {
        let mono = (l + r) * 0.5;
        self.pre.push(mono);
        let input = if self.pre_len == 0 {
            mono
        } else {
            self.pre.tap(self.pre_len)
        };
        let mut outs = [0.0f32; FDN_LINES];
        // Each line's LFO runs slightly faster than the last, so the
        // modulation never lines up across lines.
        let mut speed = 1.0;
        for (i, out) in outs.iter_mut().enumerate() {
            self.lfo_phase[i] = (self.lfo_phase[i] + self.lfo_inc * speed).rem_euclid(1.0);
            speed += 0.13;
            #[expect(clippy::cast_possible_truncation, reason = "modulation")]
            let m = (std::f64::consts::TAU * self.lfo_phase[i]).sin() as f32 * self.mod_depth;
            let o = self.lines[i].tap_frac(self.lengths[i] + m);
            *out = self.damping[i].process(o) * self.gains[i];
        }
        let wet_l = outs.iter().step_by(2).sum::<f32>();
        let wet_r = outs.iter().skip(1).step_by(2).sum::<f32>();
        let mut fb = outs;
        Self::hadamard(&mut fb);
        for (i, (line, f)) in self.lines.iter_mut().zip(fb).enumerate() {
            let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
            line.push(f + input * sign * 0.35);
        }
        (
            l * self.dry + wet_l * self.wet * 0.5,
            r * self.dry + wet_r * self.wet * 0.5,
        )
    }
}

/// Convolution reverb with a stereo impulse response.
#[derive(Debug, Clone)]
pub struct ConvolutionReverb {
    left: Convolver,
    right: Convolver,
    /// Wet level.
    pub wet: f32,
    /// Dry level.
    pub dry: f32,
    dry_l: DelayLine,
    dry_r: DelayLine,
}

impl ConvolutionReverb {
    /// From a left and right impulse response, processed in `block`-sample
    /// partitions (the latency; the dry path is delayed to match).
    #[must_use]
    pub fn new(ir_left: &[f32], ir_right: &[f32], block: usize) -> Self {
        let left = Convolver::new(ir_left, block);
        let right = Convolver::new(ir_right, block);
        Self {
            dry_l: DelayLine::new(block + 4),
            dry_r: DelayLine::new(block + 4),
            left,
            right,
            wet: 0.3,
            dry: 1.0,
        }
    }

    /// Latency in samples.
    #[must_use]
    pub const fn latency(&self) -> usize {
        self.left.latency()
    }
}

impl Stereo for ConvolutionReverb {
    fn process_stereo(&mut self, l: f32, r: f32) -> (f32, f32) {
        let wl = self.left.process(l);
        let wr = self.right.process(r);
        self.dry_l.push(l);
        self.dry_r.push(r);
        let n = self.latency();
        (
            self.dry_l.tap(n + 1) * self.dry + wl * self.wet,
            self.dry_r.tap(n + 1) * self.dry + wr * self.wet,
        )
    }

    fn latency(&self) -> usize {
        self.left.latency()
    }
}

/// A synthetic stereo room impulse response: exponentially decaying noise
/// reaching −60 dB at `rt60`, darkening over time (a one-pole whose corner
/// falls), with a few discrete early reflections. Seeded.
#[must_use]
pub fn synthetic_ir(rt60: f32, rate: f32, seed: u64) -> (Vec<f32>, Vec<f32>) {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "lengths"
    )]
    let n = (rt60 * rate * 1.1) as usize;
    let mut rng = Rng::new(seed);
    let mut out = [vec![0.0; n], vec![0.0; n]];
    for ch in &mut out {
        let mut lp = OnePole::new(12_000.0, rate);
        for (i, v) in ch.iter_mut().enumerate() {
            #[expect(clippy::cast_precision_loss, reason = "sample index")]
            let t = i as f32 / rate;
            let env = 10f32.powf(-3.0 * t / rt60);
            let corner = 12_000.0 * (-t / rt60 * 2.5).exp() + 500.0;
            lp.set_cutoff(corner, rate);
            *v = lp.process(rng.bipolar()) * env;
        }
        for k in 0u8..6 {
            let d = rng.range(0.003, 0.03) * f64::from(rate);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "index"
            )]
            let idx = (d as usize).min(n.saturating_sub(1));
            let g = 0.6 / (1.0 + f32::from(k));
            ch[idx] += g * if rng.next_u64() & 1 == 0 { 1.0 } else { -1.0 };
        }
    }
    let [l, r] = out;
    (l, r)
}

/// RT60 measured from an impulse response by Schroeder backward
/// integration: the time for the energy decay curve to fall from −5 dB to
/// −35 dB, doubled (T30).
#[must_use]
pub fn measure_rt60(ir: &[f32], rate: f32) -> f32 {
    let mut edc = vec![0.0f64; ir.len()];
    let mut acc = 0.0;
    for i in (0..ir.len()).rev() {
        acc += f64::from(ir[i]).powi(2);
        edc[i] = acc;
    }
    let total = edc.first().copied().unwrap_or(1.0).max(1e-30);
    let db = |i: usize| 10.0 * (edc[i] / total).log10();
    let t5 = (0..ir.len()).find(|&i| db(i) <= -5.0).unwrap_or(0);
    let t35 = (0..ir.len()).find(|&i| db(i) <= -35.0).unwrap_or(ir.len());
    #[expect(clippy::cast_precision_loss, reason = "sample counts")]
    let secs = (t35 - t5) as f32 / rate;
    secs * 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::testing::rms;

    const RATE: f32 = 48_000.0;

    fn impulse_response(s: &mut impl Stereo, seconds: f32) -> (Vec<f32>, Vec<f32>) {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "test"
        )]
        let n = (seconds * RATE) as usize;
        let mut l = Vec::with_capacity(n);
        let mut r = Vec::with_capacity(n);
        for i in 0..n {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let (a, b) = s.process_stereo(x, x);
            l.push(a);
            r.push(b);
        }
        (l, r)
    }

    #[test]
    fn fdn_decays_at_its_specified_rt60() {
        for rt in [0.8, 2.0] {
            let mut f = Fdn::new(rt, 20_000.0, 0.0, RATE);
            f.dry = 0.0;
            f.wet = 1.0;
            f.mod_depth = 0.0;
            let (l, _) = impulse_response(&mut f, rt * 1.5);
            let m = measure_rt60(&l, RATE);
            assert!((m - rt).abs() / rt < 0.15, "asked {rt}, measured {m}");
        }
    }

    #[test]
    fn fdn_mixing_matrix_is_lossless() {
        let mut v = [1.0, -2.0, 0.5, 3.0, 0.0, 1.0, -1.0, 2.0];
        let e0: f32 = v.iter().map(|x| x * x).sum();
        Fdn::hadamard(&mut v);
        let e1: f32 = v.iter().map(|x| x * x).sum();
        assert!((e0 - e1).abs() < 1e-4);
    }

    #[test]
    fn freeverb_has_a_dense_decorrelated_decaying_tail() {
        let mut f = Freeverb::new(0.8, 0.3, RATE);
        f.dry = 0.0;
        f.wet = 1.0;
        let (l, r) = impulse_response(&mut f, 3.0);
        let early = rms(&l[4_800..14_400]);
        let late = rms(&l[100_000..120_000]);
        assert!(early > 1e-3 && late < early * 0.3, "{early} {late}");
        let dot: f64 = l[4_800..48_000]
            .iter()
            .zip(&r[4_800..48_000])
            .map(|(a, b)| f64::from(*a) * f64::from(*b))
            .sum();
        let corr = dot / (rms(&l[4_800..48_000]) * rms(&r[4_800..48_000]) * 43_200.0);
        assert!(corr.abs() < 0.5, "stereo tail decorrelated: {corr}");
    }

    #[test]
    fn convolution_reverb_reproduces_its_ir() {
        let (il, ir) = synthetic_ir(0.5, RATE, 3);
        let mut c = ConvolutionReverb::new(&il, &ir, 256);
        c.dry = 0.0;
        c.wet = 1.0;
        let (l, r) = impulse_response(&mut c, 0.6);
        let lat = c.latency();
        for i in (0..il.len()).step_by(97) {
            assert!((l[i + lat] - il[i]).abs() < 1e-4);
            assert!((r[i + lat] - ir[i]).abs() < 1e-4);
        }
    }

    #[test]
    fn synthetic_ir_has_the_rt60_it_was_asked_for() {
        let (l, _) = synthetic_ir(1.2, RATE, 1);
        let m = measure_rt60(&l, RATE);
        assert!((m - 1.2).abs() < 0.2, "{m}");
    }
}
