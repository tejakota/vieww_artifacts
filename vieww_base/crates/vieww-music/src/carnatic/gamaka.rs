//! Gamakas: ornaments as continuous pitch curves.
//!
//! In Carnatic music a note is often not a fixed pitch but a movement —
//! an oscillation, a slide into the note, a glide toward the next. That
//! movement is part of the raga's identity, so it is modelled as data: a
//! [`Gamaka`] is a function from a point in the note (`t` in 0..=1) to a
//! pitch deviation in cents.
//!
//! Every curve is smooth inside the note: value and slope are continuous,
//! with smoothstep ramps and raised-cosine envelopes, because a kink in
//! pitch is as audible as a step in level. Curves that end on the note
//! (oscillations, slides into it) return there with zero slope; a glide
//! toward the next note ends at that note's interval.
//!
//! The names follow common usage (kampita, jaru, sphurita, nokku); schools
//! describe the shapes in different words and depths vary by raga and
//! singer, so every parameter is explicit.

/// Smoothstep on 0..=1: value and slope continuous at both ends.
fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// A pitch movement within one note.
#[derive(Debug, Clone, PartialEq)]
pub enum Gamaka {
    /// No movement.
    Plain,
    /// *Kampita*: an oscillation between the note and `depth_cents` above
    /// it (below, if negative) at `rate_hz`, faded in and out over the
    /// first and last tenth of the note.
    Kampita {
        /// Swing size in cents (positive: up toward the next swara).
        depth_cents: f32,
        /// Oscillations per second.
        rate_hz: f32,
    },
    /// *Jaru*: a slide into the note from `from_cents` away (negative:
    /// from below) over the first `portion` of the note.
    Jaru {
        /// Where the slide starts, in cents from the note.
        from_cents: f32,
        /// Fraction of the note the slide takes.
        portion: f32,
    },
    /// A glide (meend) out of the note toward `to_cents` over its last
    /// `portion` — into the next note's pitch.
    Glide {
        /// Where the glide ends, cents from this note.
        to_cents: f32,
        /// Fraction of the note it takes.
        portion: f32,
    },
    /// *Sphurita*: a quick dip to `depth_cents` below and back, at the
    /// start, over `portion` of the note.
    Sphurita {
        /// How far below, in cents (positive number).
        depth_cents: f32,
        /// Fraction of the note it takes.
        portion: f32,
    },
    /// *Nokku*: a stress from above — start `from_cents` above and fall to
    /// the note over `portion`.
    Nokku {
        /// How far above, in cents (positive number).
        from_cents: f32,
        /// Fraction of the note it takes.
        portion: f32,
    },
    /// Breakpoints `(t, cents)`, `t` ascending in 0..=1, joined by
    /// smoothstep segments.
    Curve(Vec<(f32, f32)>),
}

impl Gamaka {
    /// The deviation in cents at `t` (0..=1 through the note) of a note
    /// lasting `seconds`.
    #[must_use]
    pub fn cents(&self, t: f32, seconds: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match *self {
            Self::Plain => 0.0,
            Self::Kampita {
                depth_cents,
                rate_hz,
            } => {
                let secs = t * seconds.max(1e-3);
                let swing = 0.5 * (1.0 - (std::f32::consts::TAU * rate_hz * secs).cos());
                let env = smooth(t / 0.1) * smooth((1.0 - t) / 0.1);
                depth_cents * swing * env
            }
            Self::Jaru {
                from_cents,
                portion,
            } => from_cents * (1.0 - smooth(t / portion.max(1e-3))),
            Self::Glide { to_cents, portion } => {
                let p = portion.clamp(1e-3, 1.0);
                to_cents * smooth((t - (1.0 - p)) / p)
            }
            Self::Sphurita {
                depth_cents,
                portion,
            } => {
                let x = t / portion.max(1e-3);
                if x >= 1.0 {
                    0.0
                } else {
                    // Down and back: a smooth bump, zero at both ends.
                    -depth_cents * (std::f32::consts::PI * smooth(x)).sin().powi(2)
                }
            }
            Self::Nokku {
                from_cents,
                portion,
            } => from_cents * (1.0 - smooth(t / portion.max(1e-3))),
            Self::Curve(ref pts) => match pts.as_slice() {
                [] => 0.0,
                [only] => only.1,
                _ => {
                    let i = pts
                        .iter()
                        .rposition(|p| p.0 <= t)
                        .unwrap_or(0)
                        .min(pts.len() - 2);
                    let (a, b) = (pts[i], pts[i + 1]);
                    let x = (t - a.0) / (b.0 - a.0).max(1e-6);
                    a.1 + (b.1 - a.1) * smooth(x)
                }
            },
        }
    }

    /// The curve as `(beat offset, semitones)` points, `n` of them across
    /// a note of `beats` beats lasting `seconds` — the shape
    /// [`vieww_audio::sequencer::Note::bend`] takes.
    #[must_use]
    pub fn bend_points(&self, beats: f64, seconds: f32, n: usize) -> Vec<(f64, f32)> {
        if *self == Self::Plain {
            return Vec::new();
        }
        let n = n.max(2);
        (0..n)
            .map(|i| {
                #[expect(clippy::cast_precision_loss, reason = "point index")]
                let t = i as f32 / (n - 1) as f32;
                (f64::from(t) * beats, self.cents(t, seconds) / 100.0)
            })
            .collect()
    }

    /// The deviation at the note's end — nonzero only for a glide out.
    #[must_use]
    pub fn end_cents(&self, seconds: f32) -> f32 {
        self.cents(1.0, seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(g: &Gamaka, secs: f32) -> Vec<f32> {
        (0..=2_000)
            .map(|i| g.cents(i as f32 / 2_000.0, secs))
            .collect()
    }

    #[test]
    fn every_shape_is_smooth() {
        let shapes = [
            Gamaka::Kampita {
                depth_cents: 120.0,
                rate_hz: 6.0,
            },
            Gamaka::Jaru {
                from_cents: -200.0,
                portion: 0.3,
            },
            Gamaka::Glide {
                to_cents: 300.0,
                portion: 0.4,
            },
            Gamaka::Sphurita {
                depth_cents: 100.0,
                portion: 0.2,
            },
            Gamaka::Nokku {
                from_cents: 100.0,
                portion: 0.25,
            },
            Gamaka::Curve(vec![(0.0, 0.0), (0.3, 80.0), (0.6, -40.0), (1.0, 0.0)]),
        ];
        for g in &shapes {
            let v = samples(g, 1.0);
            let max_step = v
                .windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .fold(0.0, f32::max);
            assert!(max_step < 5.0, "{g:?}: a jump of {max_step} cents");
            // Slope (second difference small): no kinks.
            let max_kink = v
                .windows(3)
                .map(|w| (w[2] - 2.0 * w[1] + w[0]).abs())
                .fold(0.0, f32::max);
            assert!(max_kink < 0.5, "{g:?}: a kink of {max_kink}");
        }
    }

    #[test]
    fn shapes_land_where_they_say() {
        let k = Gamaka::Kampita {
            depth_cents: 100.0,
            rate_hz: 5.0,
        };
        assert!(k.cents(0.0, 1.0).abs() < 1e-3 && k.end_cents(1.0).abs() < 1e-3);
        let peak = samples(&k, 1.0).into_iter().fold(0.0, f32::max);
        assert!(peak > 95.0 && peak <= 100.0, "{peak}");

        let j = Gamaka::Jaru {
            from_cents: -200.0,
            portion: 0.25,
        };
        assert!((j.cents(0.0, 1.0) + 200.0).abs() < 1e-3);
        assert!(j.cents(0.25, 1.0).abs() < 1e-3 && j.end_cents(1.0).abs() < 1e-3);

        let g = Gamaka::Glide {
            to_cents: 200.0,
            portion: 0.5,
        };
        assert!(g.cents(0.4, 1.0).abs() < 1e-3);
        assert!((g.end_cents(1.0) - 200.0).abs() < 1e-3);

        let s = Gamaka::Sphurita {
            depth_cents: 100.0,
            portion: 0.2,
        };
        let low = samples(&s, 1.0).into_iter().fold(0.0, f32::min);
        assert!((low + 100.0).abs() < 1.0, "{low}");
        assert!(s.cents(0.5, 1.0).abs() < 1e-6);
    }

    #[test]
    fn bend_points_are_semitones_over_beats() {
        let pts = Gamaka::Glide {
            to_cents: 200.0,
            portion: 1.0,
        }
        .bend_points(2.0, 1.0, 5);
        assert_eq!(pts.len(), 5);
        assert_eq!(pts[0], (0.0, 0.0));
        assert!((pts[4].0 - 2.0).abs() < 1e-9 && (pts[4].1 - 2.0).abs() < 1e-6);
        assert!(Gamaka::Plain.bend_points(1.0, 1.0, 8).is_empty());
    }
}
