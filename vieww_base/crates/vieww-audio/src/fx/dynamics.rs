//! Dynamics: compression with sidechain and lookahead, brick-wall limiting,
//! gating, expansion, transient shaping, and multiband compression on
//! Linkwitz–Riley crossovers.
//!
//! The crate's original [`Compressor`](crate::dsp::Compressor) is a
//! feed-forward compressor keyed by its own input. Production needs it keyed
//! by *another* signal (the kick ducking the bass — "sidechain pumping"),
//! able to see peaks before they arrive (lookahead), and linked across
//! stereo so the image does not wander. [`Dynamics`] is that compressor, and
//! the gate and expander share its detector and smoothing.
//!
//! [`Limiter`]'s ceiling is a *guarantee*, not a tendency: the gain applied
//! to each delayed sample is the average of `L` sliding minima that each
//! include that sample's required gain, so it can never exceed it — the
//! argument is in the type's doc and the test drives full-scale noise
//! through it to check.

use super::filter::{Svf, SvfMode};
use crate::dsp::Processor;

fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

fn lin_to_db(x: f32) -> f32 {
    20.0 * x.abs().max(1e-9).log10()
}

fn coeff(seconds: f32, rate: f32) -> f32 {
    if seconds <= 0.0 {
        0.0
    } else {
        (-1.0 / (seconds * rate)).exp()
    }
}

/// Level detection mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Detect {
    /// Instantaneous peak.
    #[default]
    Peak,
    /// RMS over ~10 ms.
    Rms,
}

/// What a [`Dynamics`] processor does above/below its threshold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Law {
    /// Downward compression above threshold by `ratio`, with a soft knee.
    Compress {
        /// e.g. 4.0 for 4:1.
        ratio: f32,
        /// Knee width in dB.
        knee: f32,
    },
    /// Downward expansion below threshold by `ratio` (2.0 = 1:2).
    Expand {
        /// Expansion ratio.
        ratio: f32,
        /// Maximum attenuation in dB (positive number).
        range: f32,
    },
}

/// A compressor/expander with sidechain, lookahead and stereo link.
#[derive(Debug, Clone)]
pub struct Dynamics {
    /// Threshold in dBFS.
    pub threshold: f32,
    /// The gain law.
    pub law: Law,
    /// Make-up gain in dB.
    pub makeup: f32,
    /// Detection mode.
    pub detect: Detect,
    attack: f32,
    release: f32,
    rms: f32,
    rms_c: f32,
    gr_db: f32,
    look_l: super::filter::DelayLine,
    look_r: super::filter::DelayLine,
    lookahead: usize,
    sc_hp: Option<(Svf, Svf)>,
    last_gr: f32,
}

impl Dynamics {
    /// A compressor: `threshold` dB, `ratio`:1, attack/release seconds.
    #[must_use]
    pub fn compressor(threshold: f32, ratio: f32, attack: f32, release: f32, rate: f32) -> Self {
        Self::new(
            threshold,
            Law::Compress { ratio, knee: 6.0 },
            attack,
            release,
            rate,
        )
    }

    /// An expander.
    #[must_use]
    pub fn expander(threshold: f32, ratio: f32, range: f32, rate: f32) -> Self {
        Self::new(threshold, Law::Expand { ratio, range }, 0.001, 0.1, rate)
    }

    /// A processor with any law.
    #[must_use]
    pub fn new(threshold: f32, law: Law, attack: f32, release: f32, rate: f32) -> Self {
        Self {
            threshold,
            law,
            makeup: 0.0,
            detect: Detect::Peak,
            attack: coeff(attack, rate),
            release: coeff(release, rate),
            rms: 0.0,
            rms_c: coeff(0.01, rate),
            gr_db: 0.0,
            look_l: super::filter::DelayLine::new(rate as usize / 50 + 8),
            look_r: super::filter::DelayLine::new(rate as usize / 50 + 8),
            lookahead: 0,
            sc_hp: None,
            last_gr: 0.0,
        }
    }

    /// Delay the audio by `samples` so gain reduction lands before the peak
    /// (up to 20 ms). The latency is reported by [`latency`](Self::latency).
    #[must_use]
    pub fn lookahead(mut self, samples: usize) -> Self {
        self.lookahead = samples.min(self.look_l.capacity() - 1);
        self
    }

    /// High-pass the sidechain at `hz` so bass does not dominate detection.
    #[must_use]
    pub fn sidechain_highpass(mut self, hz: f32, rate: f32) -> Self {
        self.sc_hp = Some((
            Svf::new(SvfMode::HighPass, hz, 0.707, rate),
            Svf::new(SvfMode::HighPass, hz, 0.707, rate),
        ));
        self
    }

    /// Latency in samples.
    #[must_use]
    pub const fn latency(&self) -> usize {
        self.lookahead
    }

    /// The last gain reduction in dB (positive = reducing) — for a meter.
    #[must_use]
    pub const fn gain_reduction(&self) -> f32 {
        self.last_gr
    }

    /// The static curve: output level for input level, both dB.
    #[must_use]
    pub fn curve(&self, x_db: f32) -> f32 {
        let t = self.threshold;
        match self.law {
            Law::Compress { ratio, knee } => {
                let r = ratio.max(1.0);
                let over = x_db - t;
                if 2.0 * over < -knee {
                    x_db
                } else if 2.0 * over.abs() <= knee && knee > 0.0 {
                    x_db + (1.0 / r - 1.0) * (over + knee / 2.0).powi(2) / (2.0 * knee)
                } else {
                    t + over / r
                }
            }
            Law::Expand { ratio, range } => {
                if x_db >= t {
                    x_db
                } else {
                    (t + (x_db - t) * ratio.max(1.0)).max(x_db - range.abs())
                }
            }
        }
    }

    fn gain_for_key(&mut self, key: f32) -> f32 {
        let key = match &mut self.sc_hp {
            Some((a, b)) => b.process(a.process(key)),
            None => key,
        };
        let level = match self.detect {
            Detect::Peak => key.abs(),
            Detect::Rms => {
                self.rms = key * key + self.rms_c * (self.rms - key * key);
                self.rms.sqrt()
            }
        };
        let x_db = lin_to_db(level);
        let target = x_db - self.curve(x_db); // ≥ 0 when reducing
        let c = if target > self.gr_db {
            self.attack
        } else {
            self.release
        };
        self.gr_db = target + c * (self.gr_db - target);
        self.last_gr = self.gr_db;
        db_to_lin(self.makeup - self.gr_db)
    }

    /// Process one sample keyed by an external `key` (sidechain).
    pub fn process_keyed(&mut self, x: f32, key: f32) -> f32 {
        let g = self.gain_for_key(key);
        self.look_l.push(x);
        let delayed = if self.lookahead == 0 {
            x
        } else {
            self.look_l.tap(self.lookahead + 1)
        };
        delayed * g
    }

    /// Process a stereo frame with linked detection (max of channels, or
    /// an external stereo key).
    pub fn process_stereo_keyed(&mut self, l: f32, r: f32, key: Option<(f32, f32)>) -> (f32, f32) {
        let (kl, kr) = key.unwrap_or((l, r));
        let k = if kl.abs() > kr.abs() { kl } else { kr };
        let g = self.gain_for_key(k);
        self.look_l.push(l);
        self.look_r.push(r);
        if self.lookahead == 0 {
            (l * g, r * g)
        } else {
            (
                self.look_l.tap(self.lookahead + 1) * g,
                self.look_r.tap(self.lookahead + 1) * g,
            )
        }
    }
}

impl Processor for Dynamics {
    fn process(&mut self, x: f32) -> f32 {
        self.process_keyed(x, x)
    }
}

impl super::Stereo for Dynamics {
    fn process_stereo(&mut self, l: f32, r: f32) -> (f32, f32) {
        self.process_stereo_keyed(l, r, None)
    }

    fn process_stereo_with_key(&mut self, l: f32, r: f32, key: (f32, f32)) -> (f32, f32) {
        self.process_stereo_keyed(l, r, Some(key))
    }

    fn latency(&self) -> usize {
        self.lookahead
    }
}

/// A lookahead brick-wall limiter.
///
/// For every incoming sample the gain it *requires* is `min(1, c/|x|)`.
/// The audio is delayed by `L` samples; the gain applied to the delayed
/// sample is the mean of the last `L` values of a sliding minimum (window
/// `L+1`) of required gains. Each of those minima covers the delayed sample,
/// so each is ≤ its requirement, so their mean is too: the ceiling holds
/// exactly. The mean is also what makes the gain ramp down smoothly over `L`
/// samples rather than step. Release is a one-pole recovery that only ever
/// moves the gain *up toward* that bound, never past it.
#[derive(Debug, Clone)]
pub struct Limiter {
    ceiling: f32,
    len: usize,
    audio: std::collections::VecDeque<(f32, f32)>,
    req: std::collections::VecDeque<f32>,
    mins: std::collections::VecDeque<f32>,
    sum: f64,
    release: f32,
    env: f32,
    /// True-peak detection, when asked for ([`Limiter::true_peak`]).
    tp: Option<Box<TruePeakStage>>,
    /// The last applied gain (for a meter).
    pub gain: f32,
}

/// The true-peak front end: a BS.1770 4× interpolating detector per channel,
/// and the audio held back by the detector's group delay so the estimate
/// arrives with the frame it describes.
#[derive(Debug, Clone)]
struct TruePeakStage {
    det: [crate::meter::TruePeak; 2],
    hold: std::collections::VecDeque<(f32, f32)>,
    prev: f32,
}

impl Limiter {
    /// A limiter at `ceiling_db` with `lookahead` seconds and `release`
    /// seconds.
    #[must_use]
    pub fn new(ceiling_db: f32, lookahead: f32, release: f32, rate: f32) -> Self {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "short"
        )]
        let len = ((lookahead * rate).round() as usize).max(1);
        let mut audio = std::collections::VecDeque::with_capacity(len + 1);
        let mut req = std::collections::VecDeque::with_capacity(len + 2);
        let mut mins = std::collections::VecDeque::with_capacity(len + 1);
        for _ in 0..len {
            audio.push_back((0.0, 0.0));
            req.push_back(1.0);
            mins.push_back(1.0);
        }
        req.push_back(1.0);
        #[expect(clippy::cast_precision_loss, reason = "short windows")]
        let sum = len as f64;
        Self {
            ceiling: db_to_lin(ceiling_db),
            len,
            audio,
            req,
            mins,
            sum,
            release: coeff(release, rate),
            env: 1.0,
            tp: None,
            gain: 1.0,
        }
    }

    /// A limiter whose ceiling is a **true-peak** ceiling (dBTP, ITU-R
    /// BS.1770 Annex 2) rather than a sample-peak one.
    ///
    /// A waveform can peak *between* samples — by up to ~3 dB at high
    /// frequencies — and a DAC's reconstruction, or the next lossy encode,
    /// plays that peak. Broadcast and streaming delivery specs (EBU R128,
    /// −1 dBTP; most platforms, −1 or −2 dBTP) set their ceilings in true
    /// peak for exactly this reason. The requirement here is taken from a
    /// 4× interpolating detector per channel (`meter::TruePeak`), with the
    /// audio delayed by the detector's group delay so the estimate meets the
    /// frame it belongs to; the extra delay is part of
    /// [`latency`](Self::latency), so the mixer's delay compensation keeps
    /// the bus aligned.
    ///
    /// The ceiling holds as the BS.1770 meter measures it — the same 4×
    /// detector a loudness meter or a delivery check runs. That detector can
    /// read a few tenths of a dB under the continuous waveform near Nyquist
    /// (0.17 dB on a quarter-rate sine at its worst phase), so, as delivery
    /// specs themselves do, leave that much headroom: a −1 dBTP limiter for a
    /// −1 dBTP spec is at the edge; −1.2 is not.
    #[must_use]
    pub fn true_peak(ceiling_dbtp: f32, lookahead: f32, release: f32, rate: f32) -> Self {
        let mut lim = Self::new(ceiling_dbtp, lookahead, release, rate);
        let mut hold = std::collections::VecDeque::with_capacity(crate::meter::TRUE_PEAK_DELAY + 1);
        for _ in 0..crate::meter::TRUE_PEAK_DELAY {
            hold.push_back((0.0, 0.0));
        }
        lim.tp = Some(Box::new(TruePeakStage {
            det: [crate::meter::TruePeak::new(), crate::meter::TruePeak::new()],
            hold,
            prev: 0.0,
        }));
        lim
    }

    /// Latency in samples: the lookahead, plus the true-peak detector's
    /// group delay when [`true_peak`](Self::true_peak) built it.
    #[must_use]
    pub const fn latency(&self) -> usize {
        if self.tp.is_some() {
            self.len + crate::meter::TRUE_PEAK_DELAY
        } else {
            self.len
        }
    }

    /// Limit one stereo frame (linked).
    pub fn process_frame(&mut self, l: f32, r: f32) -> (f32, f32) {
        // True-peak mode: the frame entering the limiter is the one the
        // detector's latest estimate describes, and its requirement is that
        // estimate (over this and the previous interval, so the peak's
        // neighbours on both sides are covered) — never less than the
        // frame's own sample peak.
        let (l, r, inter) = match self.tp.as_deref_mut() {
            Some(tp) => {
                let t = tp.det[0].tick(l).max(tp.det[1].tick(r));
                let both = t.max(tp.prev);
                tp.prev = t;
                tp.hold.push_back((l, r));
                let (dl, dr) = tp.hold.pop_front().unwrap_or((0.0, 0.0));
                (dl, dr, both)
            }
            None => (l, r, 0.0),
        };
        let peak = l.abs().max(r.abs()).max(inter);
        let need = if peak > self.ceiling {
            self.ceiling / peak
        } else {
            1.0
        };
        self.req.push_back(need);
        self.req.pop_front();
        // Sliding min over the L+1 most recent requirements. L is small
        // (a few ms), so the linear scan is cheaper than a deque's
        // bookkeeping and allocation-free.
        let m = self.req.iter().copied().fold(1.0f32, f32::min);
        self.mins.push_back(m);
        self.sum += f64::from(m);
        if let Some(old) = self.mins.pop_front() {
            self.sum -= f64::from(old);
        }
        #[expect(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            reason = "gain"
        )]
        let bound = (self.sum / self.len as f64) as f32;
        // Guard float drift in the running sum.
        let bound = bound.min(
            *self
                .mins
                .iter()
                .max_by(|a, b| a.total_cmp(b))
                .unwrap_or(&1.0),
        );
        self.env = if bound < self.env {
            bound
        } else {
            bound + self.release * (self.env - bound)
        };
        self.audio.push_back((l, r));
        let (dl, dr) = self.audio.pop_front().unwrap_or((0.0, 0.0));
        self.gain = self.env;
        (dl * self.env, dr * self.env)
    }
}

impl Processor for Limiter {
    fn process(&mut self, x: f32) -> f32 {
        self.process_frame(x, x).0
    }
}

impl super::Stereo for Limiter {
    fn process_stereo(&mut self, l: f32, r: f32) -> (f32, f32) {
        self.process_frame(l, r)
    }

    fn latency(&self) -> usize {
        Limiter::latency(self)
    }
}

/// A noise gate with hysteresis, hold and range.
#[derive(Debug, Clone)]
pub struct Gate {
    /// Opens above this (dBFS).
    pub open_db: f32,
    /// Closes below this (dBFS; ≤ `open_db`).
    pub close_db: f32,
    /// Attenuation when closed (dB, positive).
    pub range: f32,
    hold: usize,
    held: usize,
    open: bool,
    attack: f32,
    release: f32,
    g: f32,
}

impl Gate {
    /// A gate.
    #[must_use]
    pub fn new(open_db: f32, close_db: f32, hold: f32, rate: f32) -> Self {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "short"
        )]
        let hold = (hold.max(0.0) * rate) as usize;
        Self {
            open_db,
            close_db: close_db.min(open_db),
            range: 80.0,
            hold,
            held: 0,
            open: false,
            attack: coeff(0.0005, rate),
            release: coeff(0.05, rate),
            g: db_to_lin(-80.0),
        }
    }

    /// Whether the gate is open.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// Process keyed by `key`.
    pub fn process_keyed(&mut self, x: f32, key: f32) -> f32 {
        let lvl = lin_to_db(key);
        if lvl >= self.open_db {
            self.open = true;
            self.held = self.hold;
        } else if lvl < self.close_db {
            if self.held > 0 {
                self.held -= 1;
            } else {
                self.open = false;
            }
        }
        let target = if self.open {
            1.0
        } else {
            db_to_lin(-self.range.abs())
        };
        let c = if target > self.g {
            self.attack
        } else {
            self.release
        };
        self.g = target + c * (self.g - target);
        x * self.g
    }
}

impl Processor for Gate {
    fn process(&mut self, x: f32) -> f32 {
        self.process_keyed(x, x)
    }
}

/// Envelope-difference transient shaper: boost or cut the attack and the
/// sustain independently of level.
///
/// Two peak followers share a release and differ only in attack (0.5 ms vs
/// 20 ms). On a steady tone both sit on the peaks, so their difference is
/// zero and nothing changes; at an onset the fast one leads, and the lead
/// (in dB) is the transient. Giving the followers *different* releases
/// would make one settle on the waveform's average and the other on its
/// peak, and every steady tone would read as a permanent transient.
#[derive(Debug, Clone)]
pub struct TransientDesigner {
    /// Attack gain in dB per unit of detected transient (±).
    pub attack: f32,
    /// Sustain gain in dB per unit of detected sustain (±).
    pub sustain: f32,
    fast: f32,
    slow: f32,
    fa: f32,
    fr: f32,
    sa: f32,
    sr: f32,
}

impl TransientDesigner {
    /// A shaper with `attack` and `sustain` amounts in dB.
    #[must_use]
    pub fn new(attack: f32, sustain: f32, rate: f32) -> Self {
        Self {
            attack,
            sustain,
            fast: 0.0,
            slow: 0.0,
            fa: coeff(0.0005, rate),
            fr: coeff(0.1, rate),
            sa: coeff(0.02, rate),
            sr: coeff(0.1, rate),
        }
    }
}

impl Processor for TransientDesigner {
    fn process(&mut self, x: f32) -> f32 {
        let a = x.abs();
        let c = if a > self.fast { self.fa } else { self.fr };
        self.fast = a + c * (self.fast - a);
        let c = if a > self.slow { self.sa } else { self.sr };
        self.slow = a + c * (self.slow - a);
        // Difference in dB between the envelopes, clamped to ±12 dB.
        let d = (lin_to_db(self.fast) - lin_to_db(self.slow)).clamp(-12.0, 12.0) / 12.0;
        let gain_db = if d > 0.0 {
            self.attack * d
        } else {
            self.sustain * (-d)
        };
        x * db_to_lin(gain_db)
    }
}

/// A fourth-order Linkwitz–Riley crossover: low and high outputs that sum
/// to an all-pass (flat magnitude).
#[derive(Debug, Clone)]
pub struct Crossover {
    lp: [Svf; 2],
    hp: [Svf; 2],
}

impl Crossover {
    /// A split at `hz`.
    #[must_use]
    pub fn new(hz: f32, rate: f32) -> Self {
        let q = std::f32::consts::FRAC_1_SQRT_2;
        Self {
            lp: [
                Svf::new(SvfMode::LowPass, hz, q, rate),
                Svf::new(SvfMode::LowPass, hz, q, rate),
            ],
            hp: [
                Svf::new(SvfMode::HighPass, hz, q, rate),
                Svf::new(SvfMode::HighPass, hz, q, rate),
            ],
        }
    }

    /// Split into (low, high).
    pub fn split(&mut self, x: f32) -> (f32, f32) {
        let lo1 = self.lp[0].process(x);
        let lo = self.lp[1].process(lo1);
        let hi1 = self.hp[0].process(x);
        let hi = self.hp[1].process(hi1);
        (lo, hi)
    }
}

/// A three-band compressor on LR4 crossovers, with the low band phase-
/// compensated for the upper split so neutral settings sum flat.
#[derive(Debug, Clone)]
pub struct Multiband {
    x1: Crossover,
    x2: Crossover,
    comp_low: Crossover,
    /// Per-band processors: low, mid, high.
    pub bands: [Dynamics; 3],
    /// Per-band output gain in dB.
    pub band_gain: [f32; 3],
}

impl Multiband {
    /// Bands split at `low_hz` and `high_hz`, each with a 4:1 compressor at
    /// −18 dB.
    #[must_use]
    pub fn new(low_hz: f32, high_hz: f32, rate: f32) -> Self {
        let c = || Dynamics::compressor(-18.0, 4.0, 0.005, 0.1, rate);
        Self {
            x1: Crossover::new(low_hz, rate),
            x2: Crossover::new(high_hz, rate),
            comp_low: Crossover::new(high_hz, rate),
            bands: [c(), c(), c()],
            band_gain: [0.0; 3],
        }
    }

    /// Split into the three bands (before processing).
    pub fn split(&mut self, x: f32) -> [f32; 3] {
        let (lo, rest) = self.x1.split(x);
        let (mid, hi) = self.x2.split(rest);
        // The rest went through x2's all-pass response; give the low band
        // the same so the bands stay in phase at the crossover.
        let (a, b) = self.comp_low.split(lo);
        [a + b, mid, hi]
    }
}

impl Processor for Multiband {
    fn process(&mut self, x: f32) -> f32 {
        let b = self.split(x);
        self.bands
            .iter_mut()
            .zip(b)
            .zip(self.band_gain)
            .map(|((band, x), g)| band.process(x) * db_to_lin(g))
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    const RATE: f32 = 48_000.0;

    /// Steady-state gain by RMS ratio (a sampled peak under-reads by up to
    /// `1 − cos(π·f/fs)`, ~2% at 3 kHz).
    fn sine_gain(p: &mut impl FnMut(f32) -> f32, hz: f32, amp: f32) -> f32 {
        let mut e_in = 0.0f64;
        let mut e_out = 0.0f64;
        for i in 0..48_000 {
            #[expect(clippy::cast_precision_loss, reason = "test")]
            let x = amp * (2.0 * std::f32::consts::PI * hz * i as f32 / RATE).sin();
            let y = p(x);
            if i >= 24_000 {
                e_in += f64::from(x).powi(2);
                e_out += f64::from(y).powi(2);
            }
        }
        #[expect(clippy::cast_possible_truncation, reason = "test")]
        let g = (e_out / e_in).sqrt() as f32;
        g
    }

    #[test]
    fn compressor_static_curve() {
        let c = Dynamics::new(
            -20.0,
            Law::Compress {
                ratio: 4.0,
                knee: 0.0,
            },
            0.0,
            0.0,
            RATE,
        );
        assert!((c.curve(-30.0) + 30.0).abs() < 1e-5);
        assert!(
            (c.curve(0.0) + 15.0).abs() < 1e-5,
            "20 dB over at 4:1 is 5 over"
        );
        let soft = Dynamics::compressor(-20.0, 4.0, 0.0, 0.0, RATE);
        // Continuous through the knee.
        let mut prev = soft.curve(-30.0);
        for i in 1..200 {
            #[expect(clippy::cast_precision_loss, reason = "test")]
            let x = -30.0 + i as f32 * 0.1;
            let y = soft.curve(x);
            assert!((y - prev).abs() < 0.11);
            prev = y;
        }
    }

    #[test]
    fn compressor_reduces_loud_and_passes_quiet() {
        let mut c = Dynamics::compressor(-20.0, 4.0, 0.001, 0.05, RATE);
        let loud = sine_gain(&mut |x| c.process(x), 1000.0, 1.0);
        let mut c = Dynamics::compressor(-20.0, 4.0, 0.001, 0.05, RATE);
        let quiet = sine_gain(&mut |x| c.process(x), 1000.0, 0.01);
        assert!(lin_to_db(loud) < -10.0, "{}", lin_to_db(loud));
        assert!(lin_to_db(quiet).abs() < 0.5);
    }

    #[test]
    fn sidechain_ducks_on_the_key_not_the_input() {
        let mut c = Dynamics::compressor(-30.0, 10.0, 0.001, 0.05, RATE);
        let mut out_with_key = 0.0f32;
        let mut out_without = 0.0f32;
        for i in 0..24_000 {
            #[expect(clippy::cast_precision_loss, reason = "test")]
            let x = 0.1 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / RATE).sin();
            let key = if i < 12_000 { 1.0 } else { 0.0 };
            let y = c.process_keyed(x, key);
            if (6_000..12_000).contains(&i) {
                out_with_key = out_with_key.max(y.abs());
            }
            if i > 20_000 {
                out_without = out_without.max(y.abs());
            }
        }
        assert!(out_with_key < 0.02, "ducked: {out_with_key}");
        assert!(out_without > 0.09, "recovered: {out_without}");
    }

    #[test]
    fn lookahead_catches_the_first_peak() {
        let mut c = Dynamics::compressor(-20.0, 100.0, 0.0005, 0.1, RATE).lookahead(48);
        let mut out = Vec::new();
        for i in 0..2_000 {
            let x = if i >= 1_000 { 1.0 } else { 0.0 };
            out.push(c.process(x));
        }
        // The step arrives at 1000 + 48; gain is already down by then.
        assert!(out[1_048].abs() < 0.5, "{}", out[1_048]);
        assert_eq!(c.latency(), 48);
    }

    #[test]
    fn limiter_never_exceeds_its_ceiling() {
        let mut lim = Limiter::new(-1.0, 0.002, 0.05, RATE);
        let ceil = db_to_lin(-1.0);
        let mut r = Rng::new(9);
        let mut max_out = 0.0f32;
        for i in 0..96_000 {
            let burst = if (i / 4000) % 2 == 0 { 4.0 } else { 0.3 };
            let x = r.bipolar() * burst;
            let (l, rr) = lim.process_frame(x, -x * 0.5);
            max_out = max_out.max(l.abs()).max(rr.abs());
        }
        assert!(max_out <= ceil + 1e-5, "{max_out} > {ceil}");
        assert!(max_out > ceil * 0.9, "actually limiting near the ceiling");
    }

    /// The peak a 4× meter sees on `x` from frame `from` on, in dBTP. The
    /// meter runs over the whole signal: started cold mid-signal, its
    /// interpolator would ring on the step from its empty history and
    /// over-read.
    fn true_peak_dbtp(x: &[f32], from: usize) -> f32 {
        let mut m = crate::meter::TruePeak::new();
        let mut p = 0.0f32;
        for (i, v) in x.iter().enumerate() {
            let t = m.tick(*v);
            if i >= from {
                p = p.max(t);
            }
        }
        20.0 * p.log10()
    }

    /// fs/4 at 45°: every sample is ±0.707, the waveform peaks at 1.0 —
    /// 3 dB of over that a sample-peak limiter cannot see.
    fn inter_sample_overs(frames: usize) -> Vec<f32> {
        let s = std::f32::consts::FRAC_1_SQRT_2;
        (0..frames).map(|i| [s, s, -s, -s][i % 4]).collect()
    }

    #[test]
    fn a_sample_peak_limiter_lets_inter_sample_overs_through() {
        // The problem, pinned: samples at 0.707 sit under a −1 dB ceiling,
        // so nothing is limited, and the reconstructed peak is 0 dBTP.
        let x = inter_sample_overs(9_600);
        let mut lim = Limiter::new(-1.0, 0.002, 0.05, RATE);
        let y: Vec<f32> = x.iter().map(|v| lim.process(*v)).collect();
        assert!(true_peak_dbtp(&y, 2_000) > -0.2);
    }

    #[test]
    fn a_true_peak_limiter_holds_its_ceiling_between_samples() {
        let x = inter_sample_overs(9_600);
        let mut lim = Limiter::true_peak(-1.0, 0.002, 0.05, RATE);
        let y: Vec<f32> = x.iter().map(|v| lim.process(*v)).collect();
        let tp = true_peak_dbtp(&y, 2_000);
        assert!(
            tp <= -1.0 + 0.05,
            "true peak {tp} dBTP over a −1 dBTP ceiling"
        );
        assert!(tp > -2.0, "limiting, not muting: {tp}");
        // Music-like input: bursts of noise at +12 dB, both channels.
        let mut lim = Limiter::true_peak(-1.0, 0.002, 0.05, RATE);
        let mut r = Rng::new(3);
        let (mut l_out, mut r_out) = (Vec::new(), Vec::new());
        for i in 0..96_000 {
            let burst = if (i / 4000) % 2 == 0 { 4.0 } else { 0.3 };
            let v = r.bipolar() * burst;
            let (l, rr) = lim.process_frame(v, -v * 0.7);
            l_out.push(l);
            r_out.push(rr);
        }
        for (name, ch) in [("left", &l_out), ("right", &r_out)] {
            let tp = true_peak_dbtp(ch, 1_000);
            assert!(tp <= -1.0 + 0.05, "{name}: {tp} dBTP");
        }
    }

    #[test]
    fn a_true_peak_limiter_is_transparent_below_its_ceiling_and_reports_its_delay() {
        let mut lim = Limiter::true_peak(0.0, 0.001, 0.05, RATE);
        let lat = lim.latency();
        assert_eq!(
            lat,
            Limiter::new(0.0, 0.001, 0.05, RATE).latency() + crate::meter::TRUE_PEAK_DELAY
        );
        let x: Vec<f32> = (0..2000).map(|i| ((i as f32) * 0.01).sin() * 0.5).collect();
        let y: Vec<f32> = x.iter().map(|v| lim.process(*v)).collect();
        for i in 0..1800 {
            assert!((y[i + lat] - x[i]).abs() < 1e-6, "frame {i}");
        }
    }

    #[test]
    fn limiter_is_transparent_below_ceiling_after_latency() {
        let mut lim = Limiter::new(0.0, 0.001, 0.05, RATE);
        let lat = lim.latency();
        let x: Vec<f32> = (0..1000).map(|i| ((i as f32) * 0.01).sin() * 0.5).collect();
        let y: Vec<f32> = x.iter().map(|v| lim.process(*v)).collect();
        for i in 0..900 {
            assert!((y[i + lat] - x[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn gate_closes_on_silence_with_hysteresis() {
        let mut g = Gate::new(-30.0, -40.0, 0.01, RATE);
        let mut last_open = 0;
        for i in 0..20_000 {
            let x = if i < 5_000 { 0.5 } else { 0.0001 };
            g.process(x);
            if g.is_open() {
                last_open = i;
            }
        }
        assert!(
            (5_479..5_520).contains(&last_open),
            "hold ~10 ms: {last_open}"
        );
        let mut y = 0.0;
        for _ in 0..48_000 {
            y = g.process(0.0001);
        }
        assert!(y.abs() < 2e-8, "closed to −80 dB: {y}");
        // Between thresholds: an open gate stays open.
        let mut g = Gate::new(-30.0, -40.0, 0.0, RATE);
        g.process(0.5);
        g.process(db_to_lin(-35.0));
        assert!(g.is_open());
    }

    #[test]
    fn expander_pushes_quiet_signals_down() {
        let e = Dynamics::expander(-40.0, 2.0, 30.0, RATE);
        assert!((e.curve(-50.0) + 60.0).abs() < 1e-5);
        assert!((e.curve(-100.0) + 130.0).abs() < 1e-5, "range caps the cut");
        assert!((e.curve(-10.0) + 10.0).abs() < 1e-5);
    }

    #[test]
    fn transient_designer_boosts_attacks() {
        let mut t = TransientDesigner::new(9.0, 0.0, RATE);
        let mut early = 0.0f32;
        let mut late = 0.0f32;
        for i in 0..9_600 {
            #[expect(clippy::cast_precision_loss, reason = "test")]
            let x = 0.3 * (2.0 * std::f32::consts::PI * 200.0 * i as f32 / RATE).sin();
            let y = t.process(x);
            if i < 480 {
                early = early.max(y.abs());
            } else if i > 7_000 {
                late = late.max(y.abs());
            }
        }
        assert!(early > late * 1.3, "{early} vs {late}");
        assert!((late - 0.3).abs() < 0.05);
    }

    #[test]
    fn crossover_sums_flat() {
        for hz in [50.0, 300.0, 1000.0, 3000.0, 12_000.0] {
            let mut x = Crossover::new(1000.0, RATE);
            let g = sine_gain(
                &mut |v| {
                    let (a, b) = x.split(v);
                    a + b
                },
                hz,
                1.0,
            );
            assert!((g - 1.0).abs() < 0.01, "{hz}: {g}");
        }
    }

    #[test]
    fn neutral_multiband_sums_flat() {
        for hz in [60.0, 400.0, 2000.0, 5000.0, 15_000.0] {
            let mut m = Multiband::new(250.0, 4000.0, RATE);
            let g = sine_gain(&mut |v| m.split(v).iter().sum(), hz, 1.0);
            assert!((g - 1.0).abs() < 0.015, "{hz}: {g}");
        }
    }

    #[test]
    fn multiband_compresses_only_the_loud_band() {
        let mut m = Multiband::new(250.0, 4000.0, RATE);
        m.bands[0].threshold = 100.0; // low: off
        m.bands[2].threshold = 100.0; // high: off
        let mid = sine_gain(&mut |v| m.process(v), 1000.0, 1.0);
        let mut m = Multiband::new(250.0, 4000.0, RATE);
        m.bands[0].threshold = 100.0;
        m.bands[2].threshold = 100.0;
        let low = sine_gain(&mut |v| m.process(v), 60.0, 1.0);
        assert!(mid < 0.5 && (low - 1.0).abs() < 0.05, "{mid} {low}");
    }
}
