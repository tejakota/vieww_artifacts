//! Three demonstration pieces, from Carnatic to rock — each built from the
//! crate's parts, so each is also an integration test of them.
//!
//! These are demonstrations of the machinery, not compositions in any
//! tradition's sense: a raga walk is grammatical, not a kriti a musician
//! would sing. The structure is real, though — the korvai *does* land on
//! sam, the chorale *is* minimally voice-led, the rock song *is* a 12-bar.

use crate::carnatic::gamaka::Gamaka;
use crate::carnatic::korvai::{fit_mora, sollu, Mora};
use crate::carnatic::raga::{library, Raga};
use crate::carnatic::swara::{parse_phrase, Note, Swara};
use crate::carnatic::tala::{Kriya, Tala};
use crate::generate::raga_walk;
use crate::pitch::{chromatic_just, Ratio, JUST_CHROMATIC};
use crate::render::Stroke;
use crate::rock::{twelve_bar_song, Riff};
use crate::score::{Event, Instrument, Part, Score, ScoreTuning};
use crate::western::chord::progression;
use crate::western::voice_leading::{lead, Voicing};

/// What the Carnatic demo's structure is, for checking.
#[derive(Debug, Clone, PartialEq)]
pub struct CarnaticPlan {
    /// The raga.
    pub raga: Raga,
    /// The tala.
    pub tala: Tala,
    /// The melody, as swaras, in order (korvai included).
    pub melody: Vec<Note>,
    /// The korvai's mora.
    pub mora: Mora,
    /// Beat where the korvai starts.
    pub korvai_start: f64,
    /// Beat of the sam it lands on.
    pub sam: f64,
}

/// A mridangam stroke for a konnakol syllable.
fn stroke_for(syllable: &str) -> Stroke {
    match syllable {
        "ta" | "na" => Stroke::Nam,
        "di" => Stroke::Dhin,
        "thom" => Stroke::Thom,
        _ => Stroke::Thi,
    }
}

/// The Carnatic demonstration: Mayamalavagowla in just intonation on
/// C♯ (277.18 Hz), Adi tala at 72 beats per minute.
///
/// One cycle of tambura and mridangam alone; three cycles of melody on a
/// plucked (veena-like) string, grown from the raga's grammar by
/// [`raga_walk`] from `seed`, ornamented — kampita on held Ga and Ni, a
/// slide into notes reached by a leap, a glide out of each phrase's last
/// note; then a korvai whose mora is fitted to one cycle, so it lands on
/// sam; then the final Sa on that sam.
#[must_use]
pub fn carnatic(seed: u64) -> (Score, CarnaticPlan) {
    let raga = library::mayamalavagowla();
    let tala = Tala::adi();
    let tonic = 61.0f32; // C#4
    let mut score = Score::new("Mayamalavagowla in Adi tala", 72.0);
    score.tuning = ScoreTuning::Scale(chromatic_just(61, 277.18, &JUST_CHROMATIC));
    let cycle = f64::from(tala.aksharas());

    // Melody: three cycles of 4-beat groups, rhythm ♪♪♪♪♩♩ per group.
    let rhythm = [0.5, 0.5, 0.5, 0.5, 1.0, 1.0];
    let groups = 6;
    let walk = raga_walk(&raga, seed, rhythm.len() * groups, Note::mid(Swara::S));
    let mut melody_notes = walk.clone();
    let mut veena = Part::new("veena", Instrument::Pluck);
    veena.gain_db = -1.0;
    veena.reverb_db = -12.0;
    let mut t = cycle;
    for (i, n) in walk.iter().enumerate() {
        let len = rhythm[i % rhythm.len()];
        let pitch = n.pitch(tonic);
        let next = walk.get(i + 1).map(|m| m.semitones());
        let leap = i > 0 && (n.semitones() - walk[i - 1].semitones()).abs() > 3;
        let last_of_group = i % rhythm.len() == rhythm.len() - 1;
        let gamaka = if len >= 1.0 && matches!(n.swara, Swara::G3 | Swara::N3) && !last_of_group {
            Gamaka::Kampita {
                depth_cents: 90.0,
                rate_hz: 5.5,
            }
        } else if last_of_group && next.is_some_and(|m| (m - n.semitones()).abs() <= 2) {
            #[expect(clippy::cast_precision_loss, reason = "small intervals")]
            let to = (next.unwrap_or(0) - n.semitones()) as f32 * 100.0;
            Gamaka::Glide {
                to_cents: to,
                portion: 0.3,
            }
        } else if leap {
            Gamaka::Jaru {
                from_cents: -100.0,
                portion: 0.2,
            }
        } else {
            Gamaka::Plain
        };
        veena
            .events
            .push(Event::new(t, len * 0.98, pitch, 0.8).with_gamaka(gamaka));
        t += len;
    }

    // Korvai: one cycle of pulses, mora fitted to land on the next sam.
    let korvai_start = t;
    let mora = fit_mora(&tala, 0, 1, 6)
        .into_iter()
        .find(|m| m.gap >= 2)
        .expect("one cycle of Adi holds a mora");
    let phrase = parse_phrase("S' N3 D1 P M1 G3 R1 S").expect("valid notation");
    let pulse = 1.0 / f64::from(tala.nadai);
    let mut mridangam = Part::new("mridangam", Instrument::Mridangam);
    mridangam.gain_db = -3.0;
    mridangam.reverb_db = -18.0;
    let syllables: Vec<String> = sollu(mora.phrase).split(' ').map(str::to_owned).collect();
    for start in mora.starts() {
        for k in 0..mora.phrase {
            let note = phrase[(k as usize) % phrase.len()];
            let at = korvai_start + f64::from(start + k) * pulse;
            veena
                .events
                .push(Event::new(at, pulse * 0.95, note.pitch(tonic), 0.85));
            melody_notes.push(note);
            let s = stroke_for(&syllables[k as usize % syllables.len()]);
            mridangam
                .events
                .push(Event::new(at, pulse, f32::from(s.key()), 0.9));
        }
    }
    let sam = korvai_start + f64::from(mora.span()) * pulse;

    // The final Sa, on sam, with a full stroke.
    veena.events.push(Event::new(sam, 4.0, tonic, 0.9));
    melody_notes.push(Note::mid(Swara::S));
    for s in [Stroke::Thom, Stroke::Nam] {
        mridangam
            .events
            .push(Event::new(sam, 1.0, f32::from(s.key()), 1.0));
    }

    // Mridangam theka through the intro and melody: a stroke pattern per
    // beat, the bass on every clap.
    let kriyas = tala.kriyas();
    let mut beat = 0.0;
    while beat < korvai_start {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "beat index"
        )]
        let k = kriyas[(beat as usize) % kriyas.len()];
        let pattern: [(f64, Stroke); 3] = if k == Kriya::Clap {
            [(0.0, Stroke::Thom), (0.5, Stroke::Thi), (0.75, Stroke::Nam)]
        } else if k == Kriya::Wave {
            [
                (0.0, Stroke::Gumki),
                (0.5, Stroke::Thi),
                (0.75, Stroke::Dhin),
            ]
        } else {
            [(0.0, Stroke::Dhin), (0.5, Stroke::Thi), (0.75, Stroke::Nam)]
        };
        for (off, s) in pattern {
            mridangam
                .events
                .push(Event::new(beat + off, 0.25, f32::from(s.key()), 0.75));
        }
        beat += 1.0;
    }

    // Tambura: P S S S(low), a pluck a beat, ringing, to the end.
    let mut tambura = Part::new("tambura", Instrument::Tambura);
    tambura.gain_db = -9.0;
    tambura.reverb_db = -10.0;
    let strings = [tonic - 17.0, tonic - 12.0, tonic - 12.0, tonic - 24.0];
    let mut b = 0.0;
    let mut i = 0;
    while b < sam + 4.0 {
        tambura.events.push(Event::new(b, 3.9, strings[i % 4], 0.6));
        b += 1.0;
        i += 1;
    }

    score.parts = vec![tambura, mridangam, veena];
    let plan = CarnaticPlan {
        raga,
        tala,
        melody: melody_notes,
        mora,
        korvai_start,
        sam,
    };
    (score, plan)
}

/// The chorale: I vi ii⁷ V⁷ I IV ii⁷ V⁷ I in D, four voices led by
/// [`lead`] for least motion, in half notes at 66. `just` tunes it to
/// 5-limit just intonation on D; otherwise 12-TET — render both and listen
/// to the thirds settle.
#[must_use]
pub fn chorale(just: bool) -> Score {
    let d = 2u8;
    let chords = progression(d, "I vi ii7 V7 I IV ii7 V7 I").expect("valid numerals");
    let voicings = lead(
        &chords,
        Voicing {
            root_in_bass: true,
            ..Voicing::default()
        },
        None,
    );
    let mut s = Score::new(
        if just {
            "Chorale in D, just"
        } else {
            "Chorale in D, 12-TET"
        },
        66.0,
    );
    if just {
        s.tuning = ScoreTuning::Scale(chromatic_just(62, 293.66, &JUST_CHROMATIC));
    }
    let mut choir = Part::new("organ", Instrument::Pad);
    choir.reverb_db = -8.0;
    for (i, v) in voicings.iter().enumerate() {
        #[expect(clippy::cast_precision_loss, reason = "chord index")]
        let at = i as f64 * 2.0;
        let last = i == voicings.len() - 1;
        for p in v {
            choir.events.push(Event::new(
                at,
                if last { 4.0 } else { 1.95 },
                f32::from(*p),
                0.7,
            ));
        }
    }
    s.parts.push(choir);
    s
}

/// The rock demo: a 12-bar in E at 132, two choruses, with a blues riff.
#[must_use]
pub fn rock() -> Score {
    let riff = Riff {
        notes: vec![
            (0, 0.5),
            (0, 0.25),
            (1, 0.25),
            (2, 0.5),
            (3, 0.5),
            (4, 0.5),
            (3, 0.5),
            (2, 0.5),
            (0, 0.5),
        ],
    };
    let mut s = twelve_bar_song(4, 132.0, 2, &riff);
    s.title = "12-bar in E".into();
    s
}

/// The just ratios behind [`chorale`]'s tuning, for display.
#[must_use]
pub fn just_ratios() -> [Ratio; 11] {
    JUST_CHROMATIC
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carnatic::korvai::lands_on_sam;
    use crate::western::voice_leading::motion;

    #[test]
    fn the_carnatic_piece_keeps_its_grammar_and_lands_on_sam() {
        for seed in 0..8 {
            let (score, plan) = carnatic(seed);
            assert_eq!(score.parts.len(), 3);
            // The melody (walk, korvai and final Sa) breaks no rule of
            // the raga — the korvai's restart leaps included, phrase by
            // phrase.
            let walk_len = plan.melody.len() - 3 * plan.mora.phrase as usize - 1;
            assert!(
                plan.raga.check(&plan.melody[..walk_len]).is_ok(),
                "seed {seed}"
            );
            let korvai_phrase = &plan.melody[walk_len..walk_len + plan.mora.phrase as usize];
            assert!(plan.raga.check(korvai_phrase).is_ok());
            // The korvai lands exactly on sam.
            let pulses = (plan.sam - plan.korvai_start) * f64::from(plan.tala.nadai);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "whole pulses"
            )]
            let span = pulses.round() as u32;
            assert_eq!(span, plan.mora.span());
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "whole pulses"
            )]
            let start = (plan.korvai_start * f64::from(plan.tala.nadai)).round() as u32;
            assert!(lands_on_sam(&plan.tala, start, span));
            assert_eq!(
                plan.sam % f64::from(plan.tala.aksharas()),
                0.0,
                "sam is a cycle start"
            );
            // The final Sa sounds on sam.
            let veena = &score.parts[2];
            assert!(veena
                .events
                .iter()
                .any(|e| e.start == plan.sam && (e.pitch - 61.0).abs() < 1e-6));
        }
    }

    #[test]
    fn the_chorale_is_voice_led_and_retunes() {
        let et = chorale(false);
        let ji = chorale(true);
        assert_eq!(et.parts[0].events, ji.parts[0].events, "same notes");
        assert_ne!(et.tuning, ji.tuning);
        let voicings = lead(
            &progression(2, "I vi ii7 V7 I IV ii7 V7 I").unwrap(),
            Voicing {
                root_in_bass: true,
                ..Voicing::default()
            },
            None,
        );
        assert_eq!(voicings.len(), 9);
        // The bass leaps root to root (it must: root position); the upper
        // three voices move by step — the definition of smooth leading.
        let mut moves = Vec::new();
        for w in voicings.windows(2) {
            assert!(
                motion(&w[0][1..], &w[1][1..]) <= 7,
                "{:?} → {:?}",
                w[0],
                w[1]
            );
            moves.extend(
                w[0][1..]
                    .iter()
                    .zip(&w[1][1..])
                    .map(|(a, b)| a.abs_diff(*b)),
            );
        }
        assert!(
            moves.iter().all(|m| *m <= 4),
            "no upper voice leaps: {moves:?}"
        );
        let steps = moves.iter().filter(|m| **m <= 2).count();
        assert!(steps * 10 >= moves.len() * 9, "≥ 90% common tones or steps");
    }

    #[test]
    fn the_rock_song_is_a_twelve_bar() {
        let s = rock();
        assert_eq!(s.parts.len(), 4);
        assert!((s.beats() - 96.0).abs() < 0.6);
    }
}
