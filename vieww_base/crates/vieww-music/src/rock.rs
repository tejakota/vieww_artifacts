//! Rock: power chords, riffs, backbeats and the 12-bar form.
//!
//! The vocabulary is small and structural: a **power chord** is root and
//! fifth (and the octave), so it sounds the same over major and minor;
//! the **backbeat** puts the snare on beats 2 and 4 against a kick on 1
//! and 3; the **12-bar** is I I I I / IV IV I I / V IV I V (the last bar
//! the turnaround); riffs live in the minor pentatonic and blues scales.

use crate::score::{Event, Instrument, Part, Score};
use crate::western::chord::{progression, Chord};
use crate::western::rhythm::Pattern;
use crate::western::scale::{Scale, BLUES};

/// General MIDI drum keys.
pub mod kit {
    /// Kick.
    pub const KICK: u8 = 36;
    /// Snare.
    pub const SNARE: u8 = 38;
    /// Closed hi-hat.
    pub const HAT: u8 = 42;
    /// Open hi-hat.
    pub const OPEN_HAT: u8 = 46;
    /// Crash.
    pub const CRASH: u8 = 49;
}

/// The 12-bar blues/rock form in a key, as roman numerals per bar.
pub const TWELVE_BAR: &str = "I I I I IV IV I I V IV I V";

/// The 12-bar form's chords in `key` (pitch class), one per bar.
#[must_use]
pub fn twelve_bar(key: u8) -> Vec<Chord> {
    progression(key, TWELVE_BAR).expect("the 12-bar numerals are valid")
}

/// A power chord's pitches: root, fifth, octave.
#[must_use]
pub const fn power_chord(root: u8) -> [u8; 3] {
    [root, root.saturating_add(7), root.saturating_add(12)]
}

/// A drum groove: per-instrument step patterns over one bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Groove {
    /// Steps per bar (16 = sixteenths in 4/4).
    pub steps: usize,
    /// `(GM key, pattern, velocity × 100)` lanes.
    pub lanes: Vec<(u8, Pattern, u8)>,
}

impl Groove {
    /// The rock backbeat: kick on 1 and 3, snare on 2 and 4, eighth-note
    /// hats.
    #[must_use]
    pub fn backbeat() -> Self {
        Self {
            steps: 16,
            lanes: vec![
                (kit::KICK, Pattern::parse("x.......x.x....."), 95),
                (kit::SNARE, Pattern::parse("....x.......x..."), 90),
                (kit::HAT, Pattern::parse("x.x.x.x.x.x.x.x."), 60),
            ],
        }
    }

    /// A driving variant: four-on-the-floor kick, backbeat, sixteenth
    /// hats.
    #[must_use]
    pub fn driving() -> Self {
        Self {
            steps: 16,
            lanes: vec![
                (kit::KICK, Pattern::euclid(4, 16), 95),
                (kit::SNARE, Pattern::parse("....x.......x..."), 95),
                (kit::HAT, Pattern::parse("xxxxxxxxxxxxxxxx"), 50),
            ],
        }
    }

    /// The groove over `bars` bars of `beats_per_bar` beats from `at`,
    /// swung by `swing`, crash on the first downbeat.
    #[must_use]
    pub fn render(&self, at: f64, bars: usize, beats_per_bar: f64, swing: f64) -> Vec<Event> {
        #[expect(clippy::cast_precision_loss, reason = "steps per bar")]
        let step = beats_per_bar / self.steps as f64;
        let mut out = vec![Event::new(at, 2.0, f32::from(kit::CRASH), 0.8)];
        for b in 0..bars {
            #[expect(clippy::cast_precision_loss, reason = "bar index")]
            let bar = at + b as f64 * beats_per_bar;
            for (key, pat, vel) in &self.lanes {
                for t in pat.beats(step, swing) {
                    out.push(Event::new(
                        bar + t,
                        step,
                        f32::from(*key),
                        f32::from(*vel) / 100.0,
                    ));
                }
            }
        }
        out
    }
}

/// A riff: scale degrees (of the blues scale on the key) and their lengths
/// in beats, repeated — the classic one-bar figure.
#[derive(Debug, Clone, PartialEq)]
pub struct Riff {
    /// `(blues-scale degree, beats)`; degree may be negative or above 5.
    pub notes: Vec<(i32, f64)>,
}

impl Riff {
    /// A riff's events over one statement, the key's root at `root_pitch`,
    /// transposed by `transpose` semitones, starting `at`.
    #[must_use]
    pub fn render(&self, root_pitch: i32, transpose: i32, at: f64, velocity: f32) -> Vec<Event> {
        let blues = Scale::new(0, &BLUES);
        let mut t = at;
        let mut out = Vec::with_capacity(self.notes.len());
        for &(deg, len) in &self.notes {
            let p = blues.degree_pitch(root_pitch, deg) + transpose;
            #[expect(clippy::cast_precision_loss, reason = "MIDI pitch")]
            out.push(Event::new(t, len * 0.9, p as f32, velocity));
            t += len;
        }
        out
    }

    /// Its length in beats.
    #[must_use]
    pub fn beats(&self) -> f64 {
        self.notes.iter().map(|n| n.1).sum()
    }
}

/// A complete 12-bar rock piece in `key` (pitch class) at `bpm`: drums,
/// a root-following bass, power-chord rhythm guitar, and a riff that
/// moves with the chords. `choruses` times through the form.
#[must_use]
pub fn twelve_bar_song(key: u8, bpm: f64, choruses: usize, riff: &Riff) -> Score {
    let mut s = Score::new("12-bar rock", bpm);
    let chords = twelve_bar(key);
    let bar = 4.0;
    // Gains balance the parts by measured level (the plucked string is
    // far quieter than the synth voices at unity): drums, bass and riff
    // within a dB or two of each other, rhythm guitar a few dB behind.
    let mut drums = Part::new("drums", Instrument::Drums);
    drums.gain_db = -6.0;
    drums.reverb_db = -20.0;
    let mut bass = Part::new("bass", Instrument::Bass);
    bass.gain_db = -2.0;
    bass.reverb_db = -40.0;
    let mut gtr = Part::new("rhythm guitar", Instrument::Lead);
    gtr.pan = -0.35;
    gtr.gain_db = -8.5;
    let mut lead = Part::new("riff", Instrument::Pluck);
    lead.pan = 0.35;
    lead.gain_db = 10.0;
    for c in 0..choruses {
        #[expect(clippy::cast_precision_loss, reason = "bar index")]
        let start = (c * chords.len()) as f64 * bar;
        drums
            .events
            .extend(Groove::backbeat().render(start, chords.len(), bar, 0.0));
        for (i, ch) in chords.iter().enumerate() {
            #[expect(clippy::cast_precision_loss, reason = "bar index")]
            let t = start + i as f64 * bar;
            let root = 36 + i32::from(ch.root);
            // Bass: root on 1, fifth, octave, fifth — eighth-note drive.
            for (k, iv) in [0, 0, 7, 7, 12, 12, 7, 7].iter().enumerate() {
                #[expect(clippy::cast_precision_loss, reason = "MIDI pitch")]
                bass.events.push(Event::new(
                    t + k as f64 * 0.5,
                    0.45,
                    (root + iv) as f32,
                    0.85,
                ));
            }
            // Rhythm guitar: power chord stabs on the off-beats.
            for beat in [0.0, 1.5, 2.0, 3.5] {
                for p in power_chord(u8::try_from(root + 12).unwrap_or(48)) {
                    gtr.events
                        .push(Event::new(t + beat, 0.4, f32::from(p), 0.75));
                }
            }
            // The riff, transposed with the chord, on even bars.
            if i % 2 == 0 {
                let shift = (i32::from(ch.root) - i32::from(key)).rem_euclid(12);
                lead.events
                    .extend(riff.render(60 + i32::from(key), shift, t, 0.8));
            }
        }
    }
    s.parts = vec![drums, bass, gtr, lead];
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::western::chord::Quality;

    #[test]
    fn the_form() {
        let c = twelve_bar(4);
        assert_eq!(c.len(), 12);
        let roots: Vec<u8> = c.iter().map(|c| c.root).collect();
        assert_eq!(roots, [4, 4, 4, 4, 9, 9, 4, 4, 11, 9, 4, 11], "E A B");
        assert!(c.iter().all(|c| c.quality == Quality::Major));
        assert_eq!(power_chord(40), [40, 47, 52]);
    }

    #[test]
    fn the_backbeat_falls_on_two_and_four() {
        let ev = Groove::backbeat().render(0.0, 1, 4.0, 0.0);
        let at = |key: u8| -> Vec<f64> {
            ev.iter()
                .filter(|e| e.pitch == f32::from(key))
                .map(|e| e.start)
                .collect()
        };
        assert_eq!(at(kit::SNARE), [1.0, 3.0], "beats 2 and 4");
        assert_eq!(at(kit::KICK), [0.0, 2.0, 2.5]);
        assert_eq!(at(kit::HAT).len(), 8);
        assert_eq!(at(kit::CRASH), [0.0]);
    }

    #[test]
    fn a_song_has_every_part_in_time() {
        let riff = Riff {
            notes: vec![(0, 0.5), (1, 0.5), (2, 0.5), (3, 0.5), (2, 1.0), (0, 1.0)],
        };
        assert!((riff.beats() - 4.0).abs() < 1e-12);
        let s = twelve_bar_song(4, 120.0, 2, &riff);
        assert_eq!(s.parts.len(), 4);
        assert!((s.beats() - 96.0).abs() < 0.6, "{}", s.beats());
        let r = riff.render(64, 0, 0.0, 0.8);
        let pitches: Vec<f32> = r.iter().map(|e| e.pitch).collect();
        assert_eq!(
            pitches,
            [64.0, 67.0, 69.0, 70.0, 69.0, 64.0],
            "E blues: E G A Bb"
        );
    }
}
