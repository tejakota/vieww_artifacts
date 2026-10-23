//! Percussion: a synthesised drum kit and the mridangam.
//!
//! Both are [`Voice`]s, so a [`Poly`](super::voice::Poly) plays them from
//! note numbers, with velocity, and allocates nothing per hit.
//!
//! * [`DrumKit`] — the General MIDI drum map from first principles: a kick
//!   as a sine whose pitch falls fast, a snare as a tone plus a noise burst,
//!   hats and cymbals as high-passed noise, toms as falling sines. Leave a
//!   kit's [`Poly`](super::voice::Poly) untuned: the key chooses the drum.
//! * [`Mridangam`] — the double-headed drum of Carnatic music, as two banks
//!   of resonators. Its right head (*valanthalai*) is loaded with a black
//!   paste disc (*soru*) that makes its overtones nearly **harmonic** —
//!   1 : 2 : 3 : 4 : 5, the property C. V. Raman measured in 1920 and the
//!   reason the drum is tuned to the singer's Sa. The left head (*thoppi*)
//!   is a low, inharmonic membrane whose pitch the player bends up by
//!   pressing it (*gumki*). Each [`Stroke`] is a way of striking: which
//!   head, which partials ring, how long, how hard.

use super::voice::Voice;
use crate::rng::Rng;

/// A two-pole resonator: one decaying mode.
#[derive(Debug, Clone, Copy, Default)]
///
/// Band-pass form: `y = (x − x₂)/2 + a₁y₁ + a₂y₂`. The zero at DC matters:
/// a plain two-pole passes DC with gain ≈ 1/ω, so a low mode struck by a
/// one-signed pulse adds a subsonic thump no membrane makes. With the
/// zeros, a unit impulse rings as ≈ cos(nω) — unit amplitude at any
/// frequency, no DC.
struct Res {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
    a1: f32,
    a2: f32,
}

impl Res {
    fn tune(&mut self, hz: f32, t60: f32, rate: f32) {
        let r = 0.001f32.powf(1.0 / (t60.max(1e-3) * rate));
        let w = std::f32::consts::TAU * hz.clamp(1.0, rate * 0.45) / rate;
        self.a1 = 2.0 * r * w.cos();
        self.a2 = -r * r;
    }

    fn tick(&mut self, x: f32) -> f32 {
        let y = 0.5 * (x - self.x2) + self.a1 * self.y1 + self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    const fn clear(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

/// Which drum a key plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Drum {
    Kick,
    Snare,
    ClosedHat,
    OpenHat,
    Crash,
    Ride,
    Tom(u8),
}

impl Drum {
    const fn from_key(key: u8) -> Self {
        match key {
            0..=36 => Self::Kick,
            37..=40 => Self::Snare,
            42 | 44 => Self::ClosedHat,
            46 => Self::OpenHat,
            49 | 52 | 55 | 57 => Self::Crash,
            51 | 53 | 59 => Self::Ride,
            k => Self::Tom(k),
        }
    }

    /// Seconds after which the hit is inaudible.
    const fn length(self) -> f32 {
        match self {
            Self::Kick => 0.9,
            Self::Snare => 0.5,
            Self::ClosedHat => 0.15,
            Self::OpenHat => 0.9,
            Self::Crash => 3.0,
            Self::Ride => 2.0,
            Self::Tom(_) => 1.0,
        }
    }
}

/// A synthesised drum kit on the General MIDI drum map: 35–36 kick,
/// 37–40 snare, 42/44 closed hat, 46 open hat, 49/52/55/57 crash, 51/53/59
/// ride, everything else a tom pitched by its key.
#[derive(Debug, Clone)]
pub struct DrumKit {
    rate: f32,
    rng: Rng,
    drum: Drum,
    t: f32,
    phase: f32,
    vel: f32,
    hp: f32,
    hp_prev: f32,
    lp: f32,
    active: bool,
}

impl DrumKit {
    /// A kit at `rate`; `seed` sets its noise.
    #[must_use]
    pub const fn new(rate: f32, seed: u64) -> Self {
        Self {
            rate,
            rng: Rng::new(seed),
            drum: Drum::Kick,
            t: 0.0,
            phase: 0.0,
            vel: 0.0,
            hp: 0.0,
            hp_prev: 0.0,
            lp: 0.0,
            active: false,
        }
    }

    /// Noise through a first-difference high-pass (`bright` 0..1 mixes in
    /// a second difference for cymbal sizzle).
    fn hiss(&mut self, bright: f32) -> f32 {
        let n = self.rng.bipolar();
        let d1 = n - self.hp_prev;
        self.hp_prev = n;
        let d2 = d1 - self.hp;
        self.hp = d1;
        d1 * (1.0 - bright) * 0.5 + d2 * bright * 0.25
    }

    fn sine_at(&mut self, hz: f32) -> f32 {
        self.phase = (self.phase + hz / self.rate).fract();
        (std::f32::consts::TAU * self.phase).sin()
    }
}

impl Voice for DrumKit {
    fn note_on(&mut self, pitch: f32, velocity: f32) {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a GM key"
        )]
        let key = pitch.round().clamp(0.0, 127.0) as u8;
        self.drum = Drum::from_key(key);
        self.t = 0.0;
        self.phase = 0.0;
        self.vel = velocity.clamp(0.0, 1.0);
        self.lp = 0.0;
        self.active = true;
    }

    fn note_off(&mut self) {}

    fn set_pitch(&mut self, _pitch: f32) {}

    fn is_active(&self) -> bool {
        self.active
    }

    fn next(&mut self) -> f32 {
        if !self.active {
            return 0.0;
        }
        let t = self.t;
        let e = |tau: f32| (-t / tau).exp();
        let v = match self.drum {
            Drum::Kick => {
                let hz = 45.0 + 105.0 * e(0.035);
                let body = self.sine_at(hz) * e(0.28);
                let click = self.rng.bipolar() * e(0.002) * 0.25;
                body + click
            }
            Drum::Snare => {
                let tone = self.sine_at(185.0) * e(0.05) * 0.5;
                let n = self.hiss(0.2);
                self.lp += 0.5 * (n - self.lp);
                tone + self.lp * e(0.11) * 1.4
            }
            Drum::ClosedHat => self.hiss(0.8) * e(0.025) * 1.2,
            Drum::OpenHat => self.hiss(0.8) * e(0.22) * 1.0,
            Drum::Crash => self.hiss(0.6) * e(0.9) * 0.9,
            Drum::Ride => {
                let ping = (self.sine_at(3_100.0) * 0.4) * e(0.6);
                ping + self.hiss(0.9) * e(0.5) * 0.4
            }
            Drum::Tom(k) => {
                let base = 55.0 * ((f32::from(k) - 41.0) / 9.0).exp2();
                let hz = base * (1.0 + 0.5 * e(0.03));
                self.sine_at(hz) * e(0.25)
            }
        };
        self.t += 1.0 / self.rate;
        if self.t > self.drum.length() {
            self.active = false;
        }
        v * self.vel
    }

    fn kill(&mut self) {
        self.active = false;
    }
}

/// A mridangam stroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stroke {
    /// *Nam* (ta): right head near the rim, the ring of the harmonics.
    Nam,
    /// *Dhin*: right head open, the fundamental — Sa — rings longest.
    Dhin,
    /// *Chapu*: a slap at the edge of the right head; the upper harmonics
    /// ring.
    Chapu,
    /// *Thi* (ki): right head centre, damped — a dry click.
    Thi,
    /// *Tha*: left head, muted slap.
    Tha,
    /// *Thom*: left head open, the bass.
    Thom,
    /// *Gumki*: left head open while pressing — the bass bends up.
    Gumki,
}

impl Stroke {
    /// All strokes.
    pub const ALL: [Self; 7] = [
        Self::Nam,
        Self::Dhin,
        Self::Chapu,
        Self::Thi,
        Self::Tha,
        Self::Thom,
        Self::Gumki,
    ];

    /// The key that plays this stroke on a [`Mridangam`] voice (60 + n).
    #[must_use]
    pub const fn key(self) -> u8 {
        match self {
            Self::Nam => 60,
            Self::Dhin => 61,
            Self::Chapu => 62,
            Self::Thi => 63,
            Self::Tha => 64,
            Self::Thom => 65,
            Self::Gumki => 66,
        }
    }

    /// The stroke a key plays (anything unknown is Nam).
    #[must_use]
    pub const fn from_key(key: u8) -> Self {
        match key {
            61 => Self::Dhin,
            62 => Self::Chapu,
            63 => Self::Thi,
            64 => Self::Tha,
            65 => Self::Thom,
            66 => Self::Gumki,
            _ => Self::Nam,
        }
    }

    /// Its sollu.
    #[must_use]
    pub const fn sollu(self) -> &'static str {
        match self {
            Self::Nam => "nam",
            Self::Dhin => "dhin",
            Self::Chapu => "chapu",
            Self::Thi => "thi",
            Self::Tha => "tha",
            Self::Thom => "thom",
            Self::Gumki => "gumki",
        }
    }
}

/// Right-head partial ratios: harmonic, slightly stretched as real heads
/// are.
const RIGHT: [f32; 5] = [1.0, 2.0, 3.005, 4.01, 5.02];
/// Left-head partial ratios: an ordinary (inharmonic) membrane.
const LEFT: [f32; 3] = [1.0, 1.59, 2.14];

/// The mridangam, its right head tuned to `tonic_hz` (the singer's Sa).
/// Strike with [`Mridangam::strike`] or, as a [`Voice`], with
/// [`Stroke::key`] note numbers.
#[derive(Debug, Clone)]
pub struct Mridangam {
    rate: f32,
    /// The right head's pitch: Sa.
    pub tonic_hz: f32,
    /// The left head's open pitch.
    pub bass_hz: f32,
    right: [Res; 5],
    left: [Res; 3],
    right_gain: [f32; 5],
    left_gain: [f32; 3],
    /// Excitation: remaining samples and length of the contact pulse.
    hit_left: u32,
    hit_len: u32,
    hit_amp: f32,
    hits_right: bool,
    /// Gumki: seconds into the bend, or negative when not bending.
    bend_t: f32,
    level: f32,
}

impl Mridangam {
    /// A mridangam at `rate`, right head on `tonic_hz`, left head two
    /// octaves and a fifth below (a typical relation, adjustable).
    #[must_use]
    pub fn new(rate: f32, tonic_hz: f32) -> Self {
        Self {
            rate,
            tonic_hz,
            bass_hz: tonic_hz * 0.375,
            right: [Res::default(); 5],
            left: [Res::default(); 3],
            right_gain: [0.0; 5],
            left_gain: [0.0; 3],
            hit_left: 0,
            hit_len: 1,
            hit_amp: 0.0,
            hits_right: true,
            bend_t: -1.0,
            level: 0.0,
        }
    }

    fn tune_left(&mut self, scale: f32, t60: f32) {
        let rate = self.rate;
        for (r, ratio) in self.left.iter_mut().zip(LEFT) {
            r.tune(self.bass_hz * ratio * scale, t60 / ratio.sqrt(), rate);
        }
    }

    /// Play `stroke` at `velocity` (0..=1).
    pub fn strike(&mut self, stroke: Stroke, velocity: f32) {
        let rate = self.rate;
        // (right gains, right t60, left gains, left t60, contact ms, head)
        let (rg, rt, lg, lt, ms, right): ([f32; 5], f32, [f32; 3], f32, f32, bool) = match stroke {
            Stroke::Nam => ([0.5, 1.0, 0.8, 0.5, 0.3], 0.7, [0.0; 3], 0.1, 0.6, true),
            Stroke::Dhin => ([1.0, 0.5, 0.3, 0.15, 0.08], 1.3, [0.0; 3], 0.1, 1.2, true),
            Stroke::Chapu => ([0.15, 0.4, 0.8, 1.0, 0.9], 1.6, [0.0; 3], 0.1, 0.4, true),
            Stroke::Thi => ([1.0, 1.0, 0.8, 0.7, 0.6], 0.06, [0.0; 3], 0.1, 0.5, true),
            Stroke::Tha => ([0.0; 5], 0.1, [1.0, 0.6, 0.4], 0.07, 1.5, false),
            Stroke::Thom | Stroke::Gumki => ([0.0; 5], 0.1, [1.0, 0.5, 0.25], 0.9, 3.0, false),
        };
        for (r, ratio) in self.right.iter_mut().zip(RIGHT) {
            r.tune(self.tonic_hz * ratio, rt / ratio.sqrt(), rate);
        }
        self.tune_left(1.0, lt);
        self.right_gain = rg;
        self.left_gain = lg;
        self.hits_right = right;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "pulse length"
        )]
        let len = ((ms * 0.001 * rate) as u32).max(1);
        self.hit_len = len;
        self.hit_left = len;
        self.hit_amp = velocity.clamp(0.0, 1.0);
        self.bend_t = if stroke == Stroke::Gumki { 0.0 } else { -1.0 };
        self.level = 1.0;
    }
}

impl Voice for Mridangam {
    fn note_on(&mut self, pitch: f32, velocity: f32) {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a key"
        )]
        let key = pitch.round().clamp(0.0, 127.0) as u8;
        self.strike(Stroke::from_key(key), velocity);
    }

    fn note_off(&mut self) {}

    fn set_pitch(&mut self, _pitch: f32) {}

    fn is_active(&self) -> bool {
        self.level > 1e-5 || self.hit_left > 0
    }

    fn level(&self) -> f32 {
        self.level
    }

    fn next(&mut self) -> f32 {
        let x = if self.hit_left > 0 {
            #[expect(clippy::cast_precision_loss, reason = "pulse index")]
            let k = (self.hit_len - self.hit_left) as f32 + 0.5;
            #[expect(clippy::cast_precision_loss, reason = "pulse length")]
            let n = self.hit_len as f32;
            self.hit_left -= 1;
            self.hit_amp * (std::f32::consts::PI * k / n).sin() * 4.0 / n
        } else {
            0.0
        };
        // Gumki: the left head rises about a fifth over 0.25 s, then holds.
        if self.bend_t >= 0.0 {
            let u = (self.bend_t / 0.25).min(1.0);
            let s = u * u * (3.0 - 2.0 * u);
            self.tune_left(1.0 + 0.45 * s, 0.9);
            self.bend_t += 1.0 / self.rate;
        }
        let (xr, xl) = if self.hits_right { (x, 0.0) } else { (0.0, x) };
        let mut y = 0.0;
        for (r, g) in self.right.iter_mut().zip(self.right_gain) {
            y += r.tick(xr) * g;
        }
        for (r, g) in self.left.iter_mut().zip(self.left_gain) {
            y += r.tick(xl) * g;
        }
        self.level = self.level * 0.999 + y.abs() * 0.001;
        y * 0.12
    }

    fn kill(&mut self) {
        for r in self.right.iter_mut().chain(self.left.iter_mut()) {
            r.clear();
        }
        self.hit_left = 0;
        self.level = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{level_near, rms, spectrum};
    use super::*;

    const RATE: f32 = 48_000.0;

    fn hit<V: Voice>(v: &mut V, key: u8, n: usize) -> Vec<f32> {
        v.note_on(f32::from(key), 1.0);
        (0..n).map(|_| v.next()).collect()
    }

    #[test]
    fn kit_pieces_have_their_character() {
        let mut k = DrumKit::new(RATE, 1);
        let kick = hit(&mut k, 36, 24_000);
        let (m, bin) = spectrum(&kick[2_400..], RATE);
        assert!(
            level_near(&m, bin, 50.0, 6) > level_near(&m, bin, 2_000.0, 6) * 20.0,
            "kick is low"
        );
        let hat = hit(&mut k, 42, 4_800);
        let (m, bin) = spectrum(&hat, RATE);
        assert!(
            level_near(&m, bin, 8_000.0, 40) > level_near(&m, bin, 100.0, 4) * 5.0,
            "hat is high"
        );
        assert!(
            rms(&hat[4_000..]) < rms(&hat[..400]) * 0.05,
            "a closed hat is short"
        );
        let crash = hit(&mut k, 49, 96_000);
        assert!(
            rms(&crash[48_000..52_000]) > 1e-3,
            "a crash rings a second later"
        );
        assert!(k.is_active());
        let _ = hit(&mut k, 38, 30_000);
        assert!(!k.is_active(), "a hit ends");
    }

    #[test]
    fn the_right_head_is_harmonic_on_sa() {
        let sa = 277.18; // C#
        let mut m = Mridangam::new(RATE, sa);
        let x = hit(&mut m, Stroke::Nam.key(), 16_384);
        let (mags, bin) = spectrum(&x, RATE);
        let f = f64::from(sa);
        for h in 1..=5 {
            let at = level_near(&mags, bin, f * f64::from(h), 2);
            let between = level_near(&mags, bin, f * (f64::from(h) + 0.5), 2);
            assert!(at > between * 8.0, "partial {h}: {at} vs {between}");
        }
    }

    #[test]
    fn strokes_differ_as_the_tradition_says() {
        let mut m = Mridangam::new(RATE, 261.63);
        let dhin = hit(&mut m, Stroke::Dhin.key(), 48_000);
        let thi = hit(&mut m, Stroke::Thi.key(), 48_000);
        assert!(
            rms(&dhin[24_000..30_000]) > rms(&thi[24_000..30_000]) * 100.0,
            "dhin rings, thi is dry"
        );
        let chapu = hit(&mut m, Stroke::Chapu.key(), 16_384);
        let (c, bin) = spectrum(&chapu, RATE);
        assert!(
            level_near(&c, bin, 261.63 * 4.0, 2) > level_near(&c, bin, 261.63, 2),
            "chapu rings high"
        );
        // Gumki: the bass rises.
        let mut a = Mridangam::new(RATE, 261.63);
        let thom = hit(&mut a, Stroke::Thom.key(), 32_768);
        let gumki = hit(&mut a, Stroke::Gumki.key(), 32_768);
        let peak = |x: &[f32]| {
            let (m, bin) = spectrum(x, RATE);
            let (k, _) =
                m.iter()
                    .enumerate()
                    .skip(2)
                    .fold((0, 0.0), |b, (i, v)| if *v > b.1 { (i, *v) } else { b });
            #[expect(clippy::cast_precision_loss, reason = "bin index")]
            let hz = k as f64 * bin;
            hz
        };
        let (f_thom, f_gumki) = (peak(&thom[..16_384]), peak(&gumki[16_384..]));
        assert!(
            f_gumki > f_thom * 1.3,
            "gumki bends up: {f_thom} → {f_gumki}"
        );
    }

    #[test]
    fn strokes_have_no_dc_and_sit_at_a_voice_level() {
        let mut m = Mridangam::new(RATE, 261.63);
        for s in Stroke::ALL {
            let x = hit(&mut m, s.key(), 48_000);
            #[expect(clippy::cast_precision_loss, reason = "test")]
            let mean = x.iter().map(|v| f64::from(*v)).sum::<f64>() / x.len() as f64;
            let peak = x.iter().fold(0.0f32, |p, v| p.max(v.abs()));
            assert!(mean.abs() < 1e-4, "{s:?}: DC {mean}");
            assert!(peak > 0.02 && peak < 1.0, "{s:?}: peak {peak}");
        }
    }

    #[test]
    fn keys_and_strokes_round_trip() {
        for s in Stroke::ALL {
            assert_eq!(Stroke::from_key(s.key()), s);
        }
    }
}
