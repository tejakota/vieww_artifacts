//! Spatial audio — the Web Audio `PannerNode` / `StereoPannerNode`, Unity's
//! and Godot's 3D audio sources, a game engine's listener.
//!
//! * [`pan`] — the equal-power stereo pan law (`StereoPannerNode`).
//! * [`Distance`] — Web Audio's three distance models (linear, inverse,
//!   exponential) with reference distance, max distance and rolloff.
//! * [`Spatializer`] — a source and a listener in 3D: azimuth from the
//!   listener's orientation, distance attenuation, a sound cone, an
//!   **interaural time difference** (Woodworth's spherical-head formula)
//!   and a **head-shadow** one-pole low-pass on the far ear — the
//!   two cues of HRTF rendering, closed-form — plus Doppler pitch for a
//!   moving source. [`Spatializer::render`] turns a mono buffer into a
//!   positioned stereo one.

use crate::Samples;

/// Equal-power pan: `p` in −1 (left) … +1 (right) → `(left_gain, right_gain)`.
#[must_use]
pub fn pan(p: f32) -> (f32, f32) {
    let a = (p.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    (a.cos(), a.sin())
}

/// A distance model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Distance {
    Linear {
        reference: f32,
        max: f32,
        rolloff: f32,
    },
    Inverse {
        reference: f32,
        rolloff: f32,
    },
    Exponential {
        reference: f32,
        rolloff: f32,
    },
}

impl Distance {
    /// The gain at distance `d` (Web Audio's formulas).
    #[must_use]
    pub fn gain(&self, d: f32) -> f32 {
        match *self {
            Self::Linear {
                reference,
                max,
                rolloff,
            } => {
                let d = d.clamp(reference, max);
                1.0 - rolloff.clamp(0.0, 1.0) * (d - reference) / (max - reference).max(1e-6)
            }
            Self::Inverse { reference, rolloff } => {
                let d = d.max(reference);
                reference / (reference + rolloff * (d - reference))
            }
            Self::Exponential { reference, rolloff } => {
                (d.max(reference) / reference).powf(-rolloff)
            }
        }
    }
}

/// A positioned source and a listener.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spatializer {
    pub source: [f32; 3],
    pub source_velocity: [f32; 3],
    /// Direction the source faces and its cone (inner, outer half-angles in
    /// radians, gain outside the outer cone).
    pub cone: Option<([f32; 3], f32, f32, f32)>,
    pub listener: [f32; 3],
    /// Listener forward and up (unit).
    pub forward: [f32; 3],
    pub up: [f32; 3],
    pub distance: Distance,
    /// Head radius in metres (Woodworth).
    pub head_radius: f32,
    pub speed_of_sound: f32,
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn len(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

impl Default for Spatializer {
    fn default() -> Self {
        Self {
            source: [0.0, 0.0, -1.0],
            source_velocity: [0.0; 3],
            cone: None,
            listener: [0.0; 3],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            distance: Distance::Inverse {
                reference: 1.0,
                rolloff: 1.0,
            },
            head_radius: 0.0875,
            speed_of_sound: 343.0,
        }
    }
}

/// The per-ear result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cues {
    /// −π..π, positive to the right.
    pub azimuth: f32,
    pub distance: f32,
    pub gain: (f32, f32),
    /// Seconds the left ear lags the right (positive: the source is to the
    /// right, so the right ear hears it first).
    pub itd: f32,
    /// One-pole low-pass coefficient for the shadowed ear (0 = open).
    pub shadow: (f32, f32),
    /// Pitch ratio from Doppler.
    pub doppler: f32,
}

impl Spatializer {
    /// The localisation cues for the current geometry.
    #[must_use]
    pub fn cues(&self) -> Cues {
        let to = sub(self.source, self.listener);
        let d = len(to).max(1e-6);
        let right = cross(self.forward, self.up);
        let (x, z) = (dot(to, right) / d, dot(to, self.forward) / d);
        let azimuth = x.atan2(z);
        let mut g = self.distance.gain(d);
        if let Some((dir, inner, outer, outside)) = self.cone {
            let away = sub(self.listener, self.source);
            let c = (dot(away, dir) / (len(away).max(1e-6) * len(dir).max(1e-6)))
                .clamp(-1.0, 1.0)
                .acos();
            let k = if c <= inner {
                1.0
            } else if c >= outer {
                outside
            } else {
                1.0 + (outside - 1.0) * (c - inner) / (outer - inner).max(1e-6)
            };
            g *= k;
        }
        let (l, r) = pan(azimuth.sin());
        // Woodworth: ITD = (a/c)(θ + sin θ), θ the lateral angle.
        let lateral = azimuth.sin().clamp(-1.0, 1.0).asin();
        let itd = self.head_radius / self.speed_of_sound * (lateral + lateral.sin());
        // Head shadow strengthens with laterality on the far ear.
        let s = lateral.abs() / std::f32::consts::FRAC_PI_2 * 0.85;
        let shadow = if lateral > 0.0 { (s, 0.0) } else { (0.0, s) };
        // Doppler with a stationary listener.
        let v = dot(self.source_velocity, sub(self.listener, self.source)) / d;
        let doppler = self.speed_of_sound / (self.speed_of_sound - v).max(1.0);
        Cues {
            azimuth,
            distance: d,
            gain: (l * g, r * g),
            itd,
            shadow,
            doppler,
        }
    }

    /// Position a mono buffer: gains, ITD delay, head shadow and Doppler.
    #[must_use]
    pub fn render(&self, mono: &Samples) -> Samples {
        let c = self.cues();
        let rate = mono.rate;
        let src: Vec<f32> = if mono.channels == 2 {
            mono.data
                .chunks(2)
                .map(|f| (f[0] + f.get(1).copied().unwrap_or(0.0)) * 0.5)
                .collect()
        } else {
            mono.data.clone()
        };
        // Doppler: resample by the pitch ratio.
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let n = (src.len() as f32 / c.doppler).round() as usize;
        let at = |pos: f32| -> f32 {
            if pos < 0.0 {
                return 0.0;
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let i = pos as usize;
            #[allow(clippy::cast_precision_loss)]
            let f = pos - i as f32;
            let a = src.get(i).copied().unwrap_or(0.0);
            let b = src.get(i + 1).copied().unwrap_or(0.0);
            a + (b - a) * f
        };
        #[allow(clippy::cast_precision_loss)]
        let delay = c.itd * rate as f32;
        let (dl, dr) = if delay > 0.0 {
            (delay, 0.0)
        } else {
            (0.0, -delay)
        };
        let mut out = Vec::with_capacity(n * 2);
        let (mut lp_l, mut lp_r) = (0.0f32, 0.0f32);
        for i in 0..n {
            #[allow(clippy::cast_precision_loss)]
            let p = i as f32 * c.doppler;
            let (mut l, mut r) = (at(p - dl * c.doppler), at(p - dr * c.doppler));
            lp_l += (1.0 - c.shadow.0) * (l - lp_l);
            lp_r += (1.0 - c.shadow.1) * (r - lp_r);
            l = lp_l * c.gain.0;
            r = lp_r * c.gain.1;
            out.push(l);
            out.push(r);
        }
        Samples::stereo(out, rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_power_pan_keeps_power() {
        for p in [-1.0, -0.3, 0.0, 0.7, 1.0] {
            let (l, r) = pan(p);
            assert!((l * l + r * r - 1.0).abs() < 1e-6);
        }
        assert!((pan(-1.0).1).abs() < 1e-6 && (pan(1.0).0).abs() < 1e-6);
    }

    #[test]
    fn distance_models_match_web_audio() {
        let inv = Distance::Inverse {
            reference: 1.0,
            rolloff: 1.0,
        };
        assert!((inv.gain(4.0) - 0.25).abs() < 1e-6);
        assert_eq!(inv.gain(0.5), 1.0);
        let lin = Distance::Linear {
            reference: 1.0,
            max: 11.0,
            rolloff: 1.0,
        };
        assert!((lin.gain(6.0) - 0.5).abs() < 1e-6);
        assert_eq!(lin.gain(100.0), 0.0);
        let exp = Distance::Exponential {
            reference: 1.0,
            rolloff: 2.0,
        };
        assert!((exp.gain(2.0) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn a_source_on_the_right_is_louder_and_earlier_on_the_right() {
        let s = Spatializer {
            source: [2.0, 0.0, 0.0],
            ..Spatializer::default()
        };
        let c = s.cues();
        assert!((c.azimuth - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
        assert!(c.gain.1 > c.gain.0);
        assert!(
            c.itd > 0.0006 && c.itd < 0.0007,
            "≈0.66 ms for a 8.75 cm head: {}",
            c.itd
        );
        let tone: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.05).sin()).collect();
        let out = s.render(&Samples::mono(tone, 48_000));
        let e = |ch: usize| {
            out.data
                .iter()
                .skip(ch)
                .step_by(2)
                .map(|v| v * v)
                .sum::<f32>()
        };
        assert!(e(1) > e(0) * 4.0);
        // The right ear leads: its first non-zero sample comes first (a
        // source at 45° so the far ear is not silent).
        let s45 = Spatializer {
            source: [1.0, 0.0, -1.0],
            ..Spatializer::default()
        };
        let tone: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.05).sin()).collect();
        let out = s45.render(&Samples::mono(tone, 48_000));
        let first = |ch: usize| {
            out.data
                .iter()
                .skip(ch)
                .step_by(2)
                .position(|v| v.abs() > 1e-4)
                .unwrap_or(0)
        };
        assert!(first(1) < first(0));
    }

    #[test]
    fn approaching_sources_rise_in_pitch_and_cones_attenuate() {
        let s = Spatializer {
            source: [0.0, 0.0, -10.0],
            source_velocity: [0.0, 0.0, 34.3],
            ..Spatializer::default()
        };
        assert!((s.cues().doppler - 1.0 / 0.9).abs() < 1e-3);
        let facing_away = Spatializer {
            source: [0.0, 0.0, -2.0],
            cone: Some(([0.0, 0.0, -1.0], 0.3, 0.6, 0.1)),
            ..Spatializer::default()
        };
        let facing = Spatializer {
            cone: Some(([0.0, 0.0, 1.0], 0.3, 0.6, 0.1)),
            ..facing_away
        };
        assert!(facing.cues().gain.0 > facing_away.cues().gain.0 * 5.0);
    }
}
