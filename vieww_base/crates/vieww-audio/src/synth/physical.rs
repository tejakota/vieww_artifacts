//! Physical modelling — instruments as simulated objects.
//!
//! * [`KarplusStrong`] — a plucked string: a noise burst circulating in a
//!   delay line one period long, averaged each pass so highs die first.
//!   Tuned exactly with a first-order all-pass for the fractional part of
//!   the period (Jaffe & Smith), with pluck position, decay time and
//!   brightness.
//! * [`Clarinet`] and [`Bowed`] — digital waveguides after Perry Cook's and
//!   Gary Scavone's Synthesis ToolKit: a bore (or string) as delay lines,
//!   with the *nonlinear* excitation that makes a wind or a bow self-
//!   sustaining — a reed's pressure-dependent reflection, a bow's
//!   stick-slip friction curve. Breath and bow pressure are continuous
//!   controls, so these respond to expression the way the real thing does.
//! * [`Modal`] — a bank of decaying two-pole resonators, one per vibration
//!   mode, struck by a mallet whose hardness sets its contact time. Bars,
//!   bells, plates and wood with their measured mode ratios.
//! * [`MassSpring`] — a chain of masses and springs integrated
//!   symplectically; its modes are the textbook `2√(k/m)·sin(kπ/2(N+1))`
//!   and the test checks the first one.
//!
//! All are [`Voice`]s, so the polyphony, MPE and glide machinery applies.

use super::voice::{pitch_to_hz, Expression, Voice};
use crate::dsp::Processor;
use crate::fx::filter::{Allpass1, DelayLine};
use crate::rng::Rng;

/// A Karplus–Strong plucked string.
#[derive(Debug, Clone)]
pub struct KarplusStrong {
    rate: f32,
    line: DelayLine,
    len: usize,
    tune: Allpass1,
    prev: f32,
    loss: f32,
    rng: Rng,
    /// Decay to −60 dB, seconds (at the fundamental).
    pub decay: f32,
    /// Pluck brightness 0..=1 (low-passes the excitation).
    pub brightness: f32,
    /// Pluck position 0..1 along the string (0.5 removes even harmonics).
    pub position: f32,
    hz: f32,
    energy: f32,
    damped: bool,
}

impl KarplusStrong {
    /// A string at `rate`.
    #[must_use]
    pub fn new(rate: f32, seed: u64) -> Self {
        Self {
            rate,
            line: DelayLine::new((rate / 20.0) as usize + 8),
            len: 100,
            tune: Allpass1::default(),
            prev: 0.0,
            loss: 0.996,
            rng: Rng::new(seed),
            decay: 3.0,
            brightness: 0.8,
            position: 0.13,
            hz: 440.0,
            energy: 0.0,
            damped: false,
        }
    }

    fn retune(&mut self, hz: f32) {
        self.hz = hz.clamp(20.0, self.rate / 4.0);
        // Loop delay = N (line) + 0.5 (average) + d (all-pass).
        let period = self.rate / self.hz;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "period"
        )]
        let n = ((period - 0.5 - 0.1).floor() as usize).max(2);
        #[expect(clippy::cast_precision_loss, reason = "period")]
        let d = period - 0.5 - n as f32;
        self.len = n;
        self.tune.a = (1.0 - d) / (1.0 + d);
        self.loss = 0.001f32.powf(1.0 / (self.hz * self.decay.max(0.01)));
    }

    /// Pluck at `hz` with `velocity`.
    pub fn pluck(&mut self, hz: f32, velocity: f32) {
        // The old window's length — where the ringing string's continuation
        // sits in the line — before the retune moves it.
        let old_len = self.len;
        // Below ~−60 dB there is nothing worth continuing from, and fading
        // the excitation's head in from zero (a silent string's
        // "continuation") would only smear the pluck-position comb below.
        let was_ringing = self.energy > 1e-3;
        self.retune(hz);
        let n = self.len;
        let mut exc = vec![0.0f32; n];
        let mut lp = 0.0f32;
        let b = self.brightness.clamp(0.0, 1.0);
        for e in &mut exc {
            let w = self.rng.bipolar();
            lp += (w - lp) * (0.1 + 0.9 * b);
            *e = lp;
        }
        // Pluck position: a comb on the excitation (subtract a copy delayed
        // by βN) removes harmonics with a node at the plucking point. The
        // string repeats its contents every period, so the comb is circular.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "index"
        )]
        let off = ((self.position.clamp(0.01, 0.99) * n as f32).round() as usize).clamp(1, n - 1);
        let shaped: Vec<f32> = (0..n).map(|i| exc[i] - exc[(i + n - off) % n]).collect();
        let peak = shaped.iter().fold(1e-6f32, |m, v| m.max(v.abs()));
        // A retrigger replaces the string's whole read window at once, and
        // the switch from the old string to the new excitation is a step of
        // whatever the old string was still sounding — the click on every
        // steal. Instead, the excitation's head crossfades out of the old
        // string's own continuation — the samples it was about to output,
        // read straight out of the line before this loop's pushes overwrite
        // them — so a retriggered string starts exactly where the old one
        // was. The continuation is read per-sample (not held at its first
        // value) so the crossfade path follows the old waveform's slope.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "fade length in samples"
        )]
        let fade_n = (0.002 * self.rate) as usize;
        let fade_n = if was_ringing {
            fade_n.min(old_len / 2).max(1)
        } else {
            0
        };
        let old: Vec<f32> = (0..fade_n).map(|i| self.line.tap(old_len - i)).collect();
        for (i, v) in shaped.iter().enumerate() {
            let mut s = v / peak * velocity;
            if i < fade_n {
                #[expect(clippy::cast_precision_loss, reason = "index ratio")]
                let mix = i as f32 / fade_n as f32;
                let was = old[i];
                s = was + (s - was) * mix;
            }
            self.line.push(s);
        }
        self.energy = 1.0;
        self.damped = false;
    }

    /// The next sample.
    pub fn tick(&mut self) -> f32 {
        let out = self.line.tap(self.len);
        let avg = 0.5 * (out + self.prev);
        self.prev = out;
        let loss = if self.damped {
            self.loss.powi(40)
        } else {
            self.loss
        };
        let v = self.tune.process(avg) * loss;
        self.line.push(v);
        self.energy = self.energy * 0.9995 + out.abs() * 0.0005;
        out
    }
}

impl Voice for KarplusStrong {
    fn note_on(&mut self, pitch: f32, velocity: f32) {
        self.pluck(pitch_to_hz(pitch), velocity);
    }
    fn note_off(&mut self) {
        self.damped = true;
    }
    fn set_pitch(&mut self, _pitch: f32) {
        // A plucked string's length is fixed once plucked; bends would need
        // a modulated delay, and a fixed pitch is the honest model.
    }
    fn is_active(&self) -> bool {
        self.energy > 1e-4
    }
    fn next(&mut self) -> f32 {
        self.tick()
    }
    fn level(&self) -> f32 {
        self.energy
    }
    fn kill(&mut self) {
        // Palm-mute rather than flush: draining the delay line to zeros
        // ends the string at whatever amplitude it had — a step, which is
        // the click stealing exists to avoid. Damping instead runs the
        // loop down over a few tens of milliseconds (the muted loss is
        // the release loss to the fortieth power per pass), which is also
        // simply what a hand laid on a string does.
        self.damped = true;
    }
}

/// A clarinet: a quarter-wave bore with a nonlinear reed.
#[derive(Debug, Clone)]
pub struct Clarinet {
    rate: f32,
    bore: DelayLine,
    delay: f32,
    lowpass_z: f32,
    last: f32,
    rng: Rng,
    /// Breath pressure 0..=1 (sustains above ~0.55).
    pub breath: f32,
    /// Breath noise amount.
    pub noise: f32,
    env: f32,
    target: f32,
}

impl Clarinet {
    /// A clarinet at `rate`.
    #[must_use]
    pub fn new(rate: f32, seed: u64) -> Self {
        Self {
            rate,
            bore: DelayLine::new((rate / 20.0) as usize + 8),
            delay: 50.0,
            lowpass_z: 0.0,
            last: 0.0,
            rng: Rng::new(seed),
            breath: 0.8,
            noise: 0.2,
            env: 0.0,
            target: 0.0,
        }
    }

    /// Set the sounding frequency.
    pub fn set_frequency(&mut self, hz: f32) {
        self.delay = (self.rate / hz.max(20.0) * 0.5 - 1.5).max(2.0);
    }

    /// The next sample.
    pub fn tick(&mut self) -> f32 {
        // Breath envelope: ~10 ms rise, ~20 ms fall.
        let c = if self.target > self.env {
            0.005
        } else {
            0.0025
        };
        self.env += (self.target - self.env) * c;
        let breath = self.env * self.breath * (1.0 + self.noise * self.rng.bipolar() * 0.1);
        // Bell reflection: inverting, low-passed (one zero at −1).
        let lp = 0.5 * (self.last + self.lowpass_z);
        self.lowpass_z = self.last;
        let mut diff = -0.95 * lp;
        diff -= breath;
        // Reed table: offset 0.7, slope −0.3, clipped to ±1.
        let reed = (0.7 - 0.3 * diff).clamp(-1.0, 1.0);
        let into = breath + diff * reed;
        let out = self.bore.tap_frac(self.delay);
        self.bore.push(into);
        self.last = out;
        out
    }
}

impl Voice for Clarinet {
    fn note_on(&mut self, pitch: f32, velocity: f32) {
        self.set_frequency(pitch_to_hz(pitch));
        self.target = 0.6 + 0.4 * velocity;
    }
    fn note_off(&mut self) {
        self.target = 0.0;
    }
    fn set_pitch(&mut self, pitch: f32) {
        self.set_frequency(pitch_to_hz(pitch));
    }
    fn set_expression(&mut self, e: Expression) {
        if self.target > 0.0 {
            self.breath = 0.7 + 0.3 * e.pressure;
        }
    }
    fn is_active(&self) -> bool {
        self.target > 0.0 || self.env > 1e-3 || self.last.abs() > 1e-4
    }
    fn next(&mut self) -> f32 {
        self.tick()
    }
    fn level(&self) -> f32 {
        self.env
    }
}

/// A bowed string: two delay lines (nut side and bridge side of the bow)
/// with a stick-slip friction junction.
#[derive(Debug, Clone)]
pub struct Bowed {
    rate: f32,
    neck: DelayLine,
    bridge: DelayLine,
    neck_len: f32,
    bridge_len: f32,
    neck_out: f32,
    bridge_out: f32,
    string_lp: f32,
    body: crate::fx::filter::Svf,
    /// Bow pressure 0..=1 (shapes the friction curve).
    pub pressure: f32,
    /// Bow velocity 0..=1.
    pub velocity: f32,
    env: f32,
    target: f32,
}

impl Bowed {
    const BETA: f32 = 0.127_236;

    /// A string at `rate`.
    #[must_use]
    pub fn new(rate: f32) -> Self {
        Self {
            rate,
            neck: DelayLine::new((rate / 20.0) as usize + 8),
            bridge: DelayLine::new((rate / 20.0) as usize + 8),
            neck_len: 100.0,
            bridge_len: 10.0,
            neck_out: 0.0,
            bridge_out: 0.0,
            string_lp: 0.0,
            body: crate::fx::filter::Svf::new(
                crate::fx::filter::SvfMode::BandPass,
                500.0,
                0.8,
                rate,
            ),
            pressure: 0.6,
            velocity: 0.25,
            env: 0.0,
            target: 0.0,
        }
    }

    /// Set the sounding frequency.
    pub fn set_frequency(&mut self, hz: f32) {
        let base = (self.rate / hz.max(20.0) - 4.0).max(4.0);
        self.bridge_len = (base * Self::BETA).max(1.0);
        self.neck_len = (base * (1.0 - Self::BETA)).max(1.0);
    }

    fn bow_table(&self, dv: f32) -> f32 {
        // STK's BowTable: (|slope·(dv + offset)| + 0.75)^−4, clipped at 1.
        let slope = 5.0 - 4.0 * self.pressure.clamp(0.0, 1.0);
        let s = (dv * slope).abs() + 0.75;
        s.powi(-4).min(1.0)
    }

    /// The next sample.
    pub fn tick(&mut self) -> f32 {
        let c = if self.target > self.env {
            0.002
        } else {
            0.0008
        };
        self.env += (self.target - self.env) * c;
        let bow_v = self.velocity * self.env;
        // String loss at the bridge: a one-pole low-pass with gain 0.95.
        self.string_lp = 0.95 * (0.3 * self.bridge_out + 0.7 * self.string_lp);
        let bridge_reflection = -self.string_lp;
        let nut_reflection = -self.neck_out;
        let string_v = bridge_reflection + nut_reflection;
        let dv = bow_v - string_v;
        let new_v = if self.env > 1e-4 {
            dv * self.bow_table(dv)
        } else {
            0.0
        };
        self.neck_out = self.neck.tap_frac(self.neck_len);
        self.bridge_out = self.bridge.tap_frac(self.bridge_len);
        self.neck.push(bridge_reflection + new_v);
        self.bridge.push(nut_reflection + new_v);
        self.body.process(self.bridge_out) * 4.0
    }
}

impl Voice for Bowed {
    fn note_on(&mut self, pitch: f32, velocity: f32) {
        self.set_frequency(pitch_to_hz(pitch));
        self.target = 0.6 + 0.4 * velocity;
    }
    fn note_off(&mut self) {
        self.target = 0.0;
    }
    fn set_pitch(&mut self, pitch: f32) {
        self.set_frequency(pitch_to_hz(pitch));
    }
    fn set_expression(&mut self, e: Expression) {
        self.pressure = 0.4 + 0.6 * e.pressure;
    }
    fn is_active(&self) -> bool {
        self.target > 0.0 || self.env > 1e-3 || self.bridge_out.abs() > 1e-4
    }
    fn next(&mut self) -> f32 {
        self.tick()
    }
    fn level(&self) -> f32 {
        self.env
    }
}

/// One vibration mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mode {
    /// Frequency as a ratio of the fundamental.
    pub ratio: f32,
    /// Decay to −60 dB in seconds.
    pub t60: f32,
    /// Relative amplitude.
    pub gain: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct Resonator {
    a1: f32,
    a2: f32,
    g: f32,
    y1: f32,
    y2: f32,
}

/// A modal synthesiser: resonators struck by a mallet.
#[derive(Debug, Clone)]
pub struct Modal {
    modes: Vec<Mode>,
    res: Vec<Resonator>,
    rate: f32,
    /// Mallet hardness 0..=1 (harder = shorter contact = brighter).
    pub hardness: f32,
    strike: Vec<f32>,
    strike_pos: usize,
    hz: f32,
    level: f32,
    /// Output fade from [`Voice::kill`]: the per-sample multiplier while
    /// fading, `f32::MAX` when not.
    fade_k: f32,
    /// Samples of fade left before the state is cleared outright: twelve
    /// time constants, by which point the output is past −100 dB of
    /// whatever it was when killed.
    fade_left: u32,
}

impl Modal {
    /// Kill-fade time constant, seconds — the same few-millisecond fade
    /// the `Voice::kill` contract asks engines for, matching the
    /// subtractive voice's own steal fade.
    const KILL_FADE: f32 = 0.003;

    /// A modal object from its modes.
    #[must_use]
    pub fn new(modes: Vec<Mode>, rate: f32) -> Self {
        let n = modes.len();
        Self {
            modes,
            res: vec![Resonator::default(); n],
            rate,
            hardness: 0.6,
            strike: Vec::new(),
            strike_pos: 0,
            hz: 440.0,
            level: 0.0,
            fade_k: f32::MAX,
            fade_left: 0,
        }
    }

    /// A marimba bar (tuned rosewood: 1 : 3.99 : 10.65).
    #[must_use]
    pub fn marimba(rate: f32) -> Self {
        Self::new(
            vec![
                Mode {
                    ratio: 1.0,
                    t60: 1.2,
                    gain: 1.0,
                },
                Mode {
                    ratio: 3.99,
                    t60: 0.5,
                    gain: 0.5,
                },
                Mode {
                    ratio: 10.65,
                    t60: 0.15,
                    gain: 0.25,
                },
            ],
            rate,
        )
    }

    /// A glockenspiel bar (free–free bar: 1 : 2.756 : 5.404 : 8.933).
    #[must_use]
    pub fn glockenspiel(rate: f32) -> Self {
        Self::new(
            vec![
                Mode {
                    ratio: 1.0,
                    t60: 3.0,
                    gain: 1.0,
                },
                Mode {
                    ratio: 2.756,
                    t60: 1.5,
                    gain: 0.6,
                },
                Mode {
                    ratio: 5.404,
                    t60: 0.8,
                    gain: 0.4,
                },
                Mode {
                    ratio: 8.933,
                    t60: 0.4,
                    gain: 0.25,
                },
            ],
            rate,
        )
    }

    /// A wood block (short, inharmonic).
    #[must_use]
    pub fn wood(rate: f32) -> Self {
        Self::new(
            vec![
                Mode {
                    ratio: 1.0,
                    t60: 0.12,
                    gain: 1.0,
                },
                Mode {
                    ratio: 2.57,
                    t60: 0.08,
                    gain: 0.7,
                },
                Mode {
                    ratio: 4.21,
                    t60: 0.05,
                    gain: 0.4,
                },
            ],
            rate,
        )
    }

    fn tune(&mut self, hz: f32) {
        self.hz = hz;
        for (m, r) in self.modes.iter().zip(&mut self.res) {
            let f = (hz * m.ratio).min(self.rate * 0.49);
            let w = std::f32::consts::TAU * f / self.rate;
            let rr = 0.001f32.powf(1.0 / (m.t60.max(1e-3) * self.rate));
            r.a1 = 2.0 * rr * w.cos();
            r.a2 = -rr * rr;
            r.g = m.gain * w.sin();
        }
    }

    /// Strike at `hz` with `velocity`.
    pub fn strike(&mut self, hz: f32, velocity: f32) {
        self.tune(hz);
        // Half-sine contact pulse: 0.3 ms (hard) to 3 ms (soft).
        let ms = 3.0 - 2.7 * self.hardness.clamp(0.0, 1.0);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "length"
        )]
        let n = ((ms * 0.001 * self.rate) as usize).max(1);
        #[expect(clippy::cast_precision_loss, reason = "length")]
        let nf = n as f32;
        self.strike = (0..n)
            .map(|i| {
                #[expect(clippy::cast_precision_loss, reason = "index")]
                let x = (i as f32 + 0.5) / nf;
                velocity * (std::f32::consts::PI * x).sin() / nf * 4.0
            })
            .collect();
        self.strike_pos = 0;
        self.level = velocity;
        // A struck note rings from its full level: a pending kill fade
        // (this object was stolen and retriggered) is absorbed here.
        self.fade_k = f32::MAX;
        self.fade_left = 0;
    }

    /// The next sample.
    pub fn tick(&mut self) -> f32 {
        let x = if self.strike_pos < self.strike.len() {
            self.strike_pos += 1;
            self.strike[self.strike_pos - 1]
        } else {
            0.0
        };
        let mut sum = 0.0;
        for r in &mut self.res {
            let y = r.a1 * r.y1 + r.a2 * r.y2 + r.g * x;
            r.y2 = r.y1;
            r.y1 = y;
            sum += y;
        }
        // The kill fade rides on the summed output — the resonators keep
        // ringing into it, all partials decaying together — and retires the
        // bank once it has taken them past silence.
        if self.fade_k != f32::MAX {
            sum *= self.fade_k;
            self.fade_left = self.fade_left.saturating_sub(1);
            if self.fade_left == 0 {
                for r in &mut self.res {
                    r.y1 = 0.0;
                    r.y2 = 0.0;
                }
                self.strike.clear();
                self.strike_pos = 0;
                self.fade_k = f32::MAX;
                self.level = 0.0;
            }
        }
        self.level = self.level * 0.9995 + sum.abs() * 0.0005;
        sum
    }
}

impl Voice for Modal {
    fn note_on(&mut self, pitch: f32, velocity: f32) {
        self.strike(pitch_to_hz(pitch), velocity);
    }
    fn note_off(&mut self) {}
    fn set_pitch(&mut self, _pitch: f32) {}
    fn is_active(&self) -> bool {
        self.level > 1e-5 || self.strike_pos < self.strike.len()
    }
    fn next(&mut self) -> f32 {
        self.tick()
    }
    fn level(&self) -> f32 {
        self.level
    }
    fn kill(&mut self) {
        // A fade, not an amputation: zeroing the resonators — what this
        // replaced — cut every partial at whatever amplitude it had, which
        // is the click voice stealing exists to avoid, and the trait's own
        // doc asks engines to fade over a few milliseconds instead. The
        // pending strike is dropped (a stolen voice takes no further
        // excitation) and the bank is cleared once the fade has taken it
        // past silence.
        if !self.is_active() {
            return; // already silent: nothing to fade
        }
        self.strike.clear();
        self.strike_pos = 0;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "fade length"
        )]
        let fade_n = (Modal::KILL_FADE * 12.0 * self.rate) as u32;
        self.fade_k = (-1.0 / (Modal::KILL_FADE * self.rate)).exp();
        self.fade_left = fade_n.max(1);
    }
}

/// A fixed-ends chain of masses joined by springs.
#[derive(Debug, Clone)]
pub struct MassSpring {
    x: Vec<f64>,
    v: Vec<f64>,
    k_over_m: f64,
    damping: f64,
    dt: f64,
    /// Which mass is listened to.
    pub pickup: usize,
}

impl MassSpring {
    /// `n` masses, tuned so the first mode is at `hz`, with `damping`
    /// (per-second velocity decay), at `rate`.
    #[must_use]
    pub fn new(n: usize, hz: f64, damping: f64, rate: f64) -> Self {
        let n = n.max(1);
        #[expect(clippy::cast_precision_loss, reason = "small")]
        let s = (std::f64::consts::PI / (2.0 * (n as f64 + 1.0))).sin();
        let omega = std::f64::consts::TAU * hz;
        let rate = if rate.is_finite() && rate > 0.0 {
            rate
        } else {
            48_000.0
        };
        let dt = 1.0 / rate;
        // Symplectic Euler on this chain is stable while the highest mode
        // obeys dt·ω_max < 2, and ω_max approaches 2·sqrt(k/m). Past that
        // the masses gain energy every step and the output runs to ±∞
        // within a few hundred samples — a high `hz` at an ordinary rate
        // was enough. So the stiffness is clamped just inside the bound
        // (the top modes detune slightly; the instrument stays an
        // instrument) and a non-finite request falls back to the bound.
        let k_stable = (0.98 / dt).powi(2);
        let k_over_m = (omega / (2.0 * s)).powi(2);
        let k_over_m = if k_over_m.is_finite() {
            k_over_m.min(k_stable)
        } else {
            k_stable
        };
        Self {
            x: vec![0.0; n],
            v: vec![0.0; n],
            k_over_m,
            // Negative damping is energy injection — a guaranteed blow-up.
            damping: if damping.is_finite() {
                damping.max(0.0)
            } else {
                0.0
            },
            dt,
            pickup: n / 3,
        }
    }

    /// The angular frequency of mode `k` (1-based).
    #[must_use]
    pub fn mode_hz(&self, k: usize) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "small")]
        let (kf, nf) = (k as f64, self.x.len() as f64);
        2.0 * self.k_over_m.sqrt() * (kf * std::f64::consts::PI / (2.0 * (nf + 1.0))).sin()
            / std::f64::consts::TAU
    }

    /// Hit mass `i` with a velocity.
    pub fn hit(&mut self, i: usize, velocity: f64) {
        if let Some(v) = self.v.get_mut(i) {
            *v += velocity;
        }
    }

    /// Total energy (kinetic + spring, per unit mass).
    #[must_use]
    pub fn energy(&self) -> f64 {
        let n = self.x.len();
        let mut e = 0.0;
        for i in 0..n {
            e += 0.5 * self.v[i] * self.v[i];
            let left = if i == 0 { 0.0 } else { self.x[i - 1] };
            e += 0.5 * self.k_over_m * (self.x[i] - left).powi(2);
        }
        e += 0.5 * self.k_over_m * self.x[n - 1].powi(2);
        e
    }

    /// Step once (symplectic Euler) and return the pickup displacement.
    pub fn tick(&mut self) -> f32 {
        let n = self.x.len();
        let decay = (-self.damping * self.dt).exp();
        for i in 0..n {
            let left = if i == 0 { 0.0 } else { self.x[i - 1] };
            let right = if i + 1 == n { 0.0 } else { self.x[i + 1] };
            let a = self.k_over_m * (left - 2.0 * self.x[i] + right);
            self.v[i] = (self.v[i] + a * self.dt) * decay;
        }
        for i in 0..n {
            self.x[i] += self.v[i] * self.dt;
        }
        #[expect(clippy::cast_possible_truncation, reason = "f32 audio")]
        let out = self.x[self.pickup.min(n - 1)] as f32;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fft::{Complex, Plan};

    #[test]
    fn a_mass_spring_tuned_past_stability_stays_finite_and_bounded() {
        // 20 kHz first mode at 48 kHz, eight masses, no damping: the top
        // modes sit far past dt·ω < 2. Before the clamp this ran to ±∞.
        for (hz, damping) in [(20_000.0, 0.0), (5_000.0, -50.0), (f64::NAN, 1.0)] {
            let mut m = MassSpring::new(8, hz, damping, 48_000.0);
            m.hit(2, 1.0);
            let mut peak = 0.0f32;
            for _ in 0..200_000 {
                let y = m.tick();
                assert!(y.is_finite(), "hz {hz}, damping {damping}: {y}");
                peak = peak.max(y.abs());
            }
            assert!(peak < 1.0, "hz {hz}, damping {damping}: peak {peak}");
        }
    }

    #[test]
    fn the_stability_clamp_leaves_an_ordinary_tuning_alone() {
        // 220 Hz is nowhere near the bound: the first mode is where it was asked.
        let m = MassSpring::new(8, 220.0, 1.0, 48_000.0);
        assert!((m.mode_hz(1) - 220.0).abs() < 1e-6, "{}", m.mode_hz(1));
    }

    const RATE: f32 = 48_000.0;

    /// The spectral peak within ±12% of `near` (parabolic interpolation) —
    /// the fundamental, even where a harmonic is louder.
    fn pitch_of(x: &[f32], near: f64) -> f64 {
        let n = 65_536.min(x.len().next_power_of_two() / 2);
        let plan = Plan::new(n);
        #[expect(clippy::cast_precision_loss, reason = "test")]
        let mut buf: Vec<Complex> = (0..n)
            .map(|i| {
                let w = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos();
                Complex::new(f64::from(x[i]) * w, 0.0)
            })
            .collect();
        plan.forward(&mut buf);
        let mags: Vec<f64> = buf[..n / 2]
            .iter()
            .map(|z| z.norm().max(1e-30).ln())
            .collect();
        #[expect(clippy::cast_precision_loss, reason = "test")]
        let bin = f64::from(RATE) / n as f64;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "test"
        )]
        let (lo, hi) = ((near * 0.88 / bin) as usize, (near * 1.12 / bin) as usize);
        let k = (lo.max(1)..hi.min(n / 2 - 1))
            .max_by(|a, b| mags[*a].total_cmp(&mags[*b]))
            .unwrap();
        let (a, b, c) = (mags[k - 1], mags[k], mags[k + 1]);
        let p = 0.5 * (a - c) / (a - 2.0 * b + c);
        #[expect(clippy::cast_precision_loss, reason = "test")]
        let hz = (k as f64 + p) * bin;
        hz
    }

    fn cents(a: f64, b: f64) -> f64 {
        1200.0 * (a / b).log2()
    }

    #[test]
    fn karplus_strong_is_in_tune() {
        for hz in [110.0, 261.63, 659.26, 1318.5] {
            let mut s = KarplusStrong::new(RATE, 1);
            s.decay = 8.0;
            s.pluck(hz, 1.0);
            let x: Vec<f32> = (0..131_072).map(|_| s.tick()).collect();
            let got = pitch_of(&x, f64::from(hz));
            assert!(cents(got, f64::from(hz)).abs() < 3.0, "{hz}: {got}");
        }
    }

    #[test]
    fn karplus_strong_decays_at_its_t60() {
        let mut s = KarplusStrong::new(RATE, 2);
        s.decay = 1.0;
        s.pluck(220.0, 1.0);
        let x: Vec<f32> = (0..96_000).map(|_| s.tick()).collect();
        let rms = |a: usize| crate::synth::testing::rms(&x[a..a + 4_800]);
        let drop_db = 20.0 * (rms(48_000) / rms(4_800)).log10();
        // ~0.9 s apart: roughly −50 dB at T60 = 1 s (highs die faster).
        assert!(drop_db < -35.0 && drop_db > -90.0, "{drop_db}");
    }

    #[test]
    fn plucking_the_middle_removes_even_harmonics() {
        let mut s = KarplusStrong::new(RATE, 3);
        s.position = 0.5;
        s.decay = 10.0;
        s.pluck(200.0, 1.0);
        let x: Vec<f32> = (0..32_768).map(|_| s.tick()).collect();
        let (m, bin) = crate::synth::testing::spectrum(&x[..16_384], RATE);
        let h1 = crate::synth::testing::level_near(&m, bin, 200.0, 3);
        let h2 = crate::synth::testing::level_near(&m, bin, 400.0, 3);
        assert!(h2 < h1 * 0.15, "{h1} {h2}");
    }

    #[test]
    fn clarinet_sustains_near_its_pitch_and_stops() {
        let mut c = Clarinet::new(RATE, 4);
        c.note_on(57.0, 1.0); // A3 = 220 Hz
        let x: Vec<f32> = (0..96_000).map(|_| c.tick()).collect();
        let tail = &x[48_000..];
        assert!(crate::synth::testing::rms(tail) > 0.05, "sustains");
        assert!(tail.iter().all(|v| v.is_finite() && v.abs() < 2.0));
        let got = pitch_of(tail, 220.0);
        assert!(cents(got, 220.0).abs() < 30.0, "{got}");
        c.note_off();
        let rest: Vec<f32> = (0..96_000).map(|_| c.tick()).collect();
        assert!(crate::synth::testing::rms(&rest[80_000..]) < 1e-3);
    }

    #[test]
    fn bowed_string_sustains_near_its_pitch() {
        let mut b = Bowed::new(RATE);
        b.note_on(55.0, 1.0); // G3 = 196 Hz
        let x: Vec<f32> = (0..144_000).map(|_| b.tick()).collect();
        let tail = &x[96_000..];
        assert!(
            crate::synth::testing::rms(tail) > 1e-3,
            "{}",
            crate::synth::testing::rms(tail)
        );
        assert!(tail.iter().all(|v| v.is_finite()));
        let got = pitch_of(tail, 196.0);
        assert!(cents(got, 196.0).abs() < 40.0, "{got}");
    }

    #[test]
    fn modal_bar_rings_at_its_modes() {
        let mut m = Modal::marimba(RATE);
        m.strike(200.0, 1.0);
        let x: Vec<f32> = (0..32_768).map(|_| m.tick()).collect();
        let (s, bin) = crate::synth::testing::spectrum(&x[..16_384], RATE);
        let f = crate::synth::testing::level_near(&s, bin, 200.0, 2);
        let m2 = crate::synth::testing::level_near(&s, bin, 798.0, 2);
        let off = crate::synth::testing::level_near(&s, bin, 500.0, 2);
        assert!(
            f > 0.0 && m2 > off * 10.0 && f > off * 10.0,
            "{f} {m2} {off}"
        );
    }

    #[test]
    fn harder_mallets_are_brighter() {
        let ratio = |hard: f32| {
            let mut m = Modal::glockenspiel(RATE);
            m.hardness = hard;
            m.strike(800.0, 1.0);
            let x: Vec<f32> = (0..16_384).map(|_| m.tick()).collect();
            let (s, bin) = crate::synth::testing::spectrum(&x, RATE);
            crate::synth::testing::level_near(&s, bin, 800.0 * 8.933, 3)
                / crate::synth::testing::level_near(&s, bin, 800.0, 3)
        };
        assert!(ratio(1.0) > ratio(0.0) * 2.0);
    }

    #[test]
    fn mass_spring_first_mode_and_energy_decay() {
        let mut ms = MassSpring::new(8, 220.0, 0.0, f64::from(RATE));
        assert!((ms.mode_hz(1) - 220.0).abs() < 1e-9);
        ms.hit(2, 1.0);
        let e0 = ms.energy();
        let x: Vec<f32> = (0..131_072).map(|_| ms.tick()).collect();
        let got = pitch_of(&x, 220.0);
        assert!(cents(got, 220.0).abs() < 5.0, "{got}");
        assert!(
            (ms.energy() - e0).abs() / e0 < 0.02,
            "undamped conserves energy"
        );
        let mut d = MassSpring::new(8, 220.0, 3.0, f64::from(RATE));
        d.hit(2, 1.0);
        let e0 = d.energy();
        for _ in 0..48_000 {
            d.tick();
        }
        // Damping acts on velocity only, so energy falls at the damping rate
        // (half the time it is kinetic): e^−3 after a second.
        assert!(d.energy() < e0 * 0.1, "damped decays: {}", d.energy() / e0);
    }

    #[test]
    fn physical_voices_work_in_a_pool() {
        use crate::synth::voice::Poly;
        let mut p = Poly::new((0..4).map(|i| KarplusStrong::new(RATE, i)).collect(), RATE);
        for k in [40, 45, 50, 55] {
            p.note_on(0, k, 0.9);
        }
        let x = p.render(9_600);
        assert!(x.iter().any(|v| v.abs() > 0.05));
        assert_eq!(p.active(), 4);
    }

    #[test]
    fn karplus_strong_kill_mutes_instead_of_flushing() {
        let mut s = KarplusStrong::new(RATE, 7);
        s.decay = 8.0;
        s.pluck(220.0, 0.8);
        let pre: Vec<f32> = (0..9_600).map(|_| s.tick()).collect(); // 200 ms
        let peak = pre.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.2, "string sounding: {peak}");
        s.kill();
        let post: Vec<f32> = (0..24_000).map(|_| s.tick()).collect(); // 500 ms
                                                                      // The flush this replaced dropped the output to exact zero on the
                                                                      // very next tick — a step of the string's full amplitude. Damping
                                                                      // instead keeps the waveform continuous and runs it down. The
                                                                      // comparison is against the string's amplitude at the kill (its
                                                                      // highs died long ago; the pluck's initial burst is no reference).
        let peak_now = pre[pre.len() - 480..]
            .iter()
            .fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(
            post.iter().take(480).any(|v| v.abs() > peak_now * 0.3),
            "output continues from where the string was: {peak_now}"
        );
        // And the muted string is spent, and its voice reclaimable.
        assert!(crate::synth::testing::rms(&post[22_000..]) < 1e-3);
        assert!(!s.is_active(), "voice reclaimable");
    }

    #[test]
    fn replucking_a_ringing_string_crossfades_instead_of_stepping() {
        let mut s = KarplusStrong::new(RATE, 8);
        s.decay = 8.0;
        s.pluck(220.0, 0.8);
        let pre: Vec<f32> = (0..9_600).map(|_| s.tick()).collect();
        let peak = pre.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.2, "string sounding: {peak}");
        // What the ringing string is about to output next; a same-pitch
        // repluck keeps the read window put, so this is exactly what
        // `pluck` blends its excitation out of.
        let expected = s.line.tap(s.len);
        s.pluck(220.0, 0.8);
        let post: Vec<f32> = (0..96).map(|_| s.tick()).collect();
        // The repluck's first sample continues the old string exactly —
        // the wholesale window replacement this fixed jumped straight to a
        // fresh noise sample at full excitation amplitude.
        assert_eq!(post[0], expected);
        // And the crossfade lands in the new pluck: two periods later the
        // new string is at its own full amplitude.
        let peak2 = (0..480)
            .map(|_| s.tick())
            .fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak2 > 0.3, "the new pluck rings: {peak2}");
    }

    #[test]
    fn modal_kill_fades_instead_of_amputating() {
        let mut m = Modal::marimba(RATE);
        m.strike(220.0, 1.0);
        let pre: Vec<f32> = (0..4_800).map(|_| m.tick()).collect(); // 100 ms
        let peak = pre.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.02, "bar ringing: {peak}");
        m.kill();
        let post: Vec<f32> = (0..48_000).map(|_| m.tick()).collect(); // 1 s
                                                                      // The amputation this replaced zeroed the resonators: exact zeros
                                                                      // from the first post-kill sample. A fade keeps the partials
                                                                      // decaying together from wherever they were.
        assert!(
            post.iter().take(16).any(|v| v.abs() > peak * 0.3),
            "continues from the struck level"
        );
        // The fade is over and the bank retired within a few dozen
        // milliseconds (twelve 3 ms time constants), and the voice is
        // reclaimable.
        assert!(
            crate::synth::testing::rms(&post[2_400..]) < 1e-5,
            "retired: {}",
            crate::synth::testing::rms(&post[2_400..])
        );
        assert!(!m.is_active(), "voice reclaimable");
    }

    #[test]
    fn modal_retrigger_rings_after_a_steal() {
        let mut m = Modal::marimba(RATE);
        m.strike(220.0, 1.0);
        for _ in 0..480 {
            let _ = m.tick();
        }
        m.kill(); // stolen mid-ring
        for _ in 0..100 {
            let _ = m.tick(); // part-way into the fade
        }
        // Retriggered into the fading object: the new strike must ring at
        // its full level and keep ringing — the pending fade is absorbed
        // by the retrigger, not left running underneath the new note (in
        // which case the bar would thud away to nothing in ~35 ms).
        m.strike(330.0, 1.0);
        let x: Vec<f32> = (0..16_384).map(|_| m.tick()).collect();
        let (s, bin) = crate::synth::testing::spectrum(&x, RATE);
        let f = crate::synth::testing::level_near(&s, bin, 330.0, 2);
        assert!(f > 0.0, "the new note rings: {f}");
        assert!(
            crate::synth::testing::rms(&x[8_000..]) > 0.01,
            "rings past 170 ms: {}",
            crate::synth::testing::rms(&x[8_000..])
        );
    }
}
