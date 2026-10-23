//! Tunings, measured: render notes through `Poly` and check the frequency
//! that comes out, not the number that went in.

use vieww_audio::quality::estimate_frequency;
use vieww_audio::synth::osc::{Oscillator, Shape};
use vieww_audio::synth::physical::KarplusStrong;
use vieww_audio::synth::tuning::{EqualTemperament, ScaleTuning, TuningTable};
use vieww_audio::synth::voice::{pitch_to_hz, Poly, Voice};

const RATE: f32 = 48_000.0;

fn cents(a: f64, b: f64) -> f64 {
    1200.0 * (a / b).log2()
}

/// Render one held note for half a second and measure its frequency.
fn measure<V: vieww_audio::synth::voice::Voice>(
    poly: &mut Poly<V>,
    press: impl FnOnce(&mut Poly<V>),
) -> f64 {
    press(poly);
    let mut buf = vec![0.0f32; 2 * 24_000];
    poly.render_into(&mut buf);
    let left: Vec<f32> = buf.iter().step_by(2).copied().skip(4_800).collect();
    poly.all_notes_off(true);
    estimate_frequency(&left, f64::from(RATE))
}

/// A pure sine voice built the way the crate's own voices are: pitch to
/// Hz through `pitch_to_hz`. One partial, so the estimate cannot lock to
/// a harmonic.
struct SineVoice {
    osc: Oscillator,
    on: bool,
}

impl Voice for SineVoice {
    fn note_on(&mut self, pitch: f32, _velocity: f32) {
        self.set_pitch(pitch);
        self.on = true;
    }
    fn note_off(&mut self) {
        self.on = false;
    }
    fn set_pitch(&mut self, pitch: f32) {
        self.osc.set_frequency(pitch_to_hz(pitch));
    }
    fn is_active(&self) -> bool {
        self.on
    }
    fn next(&mut self) -> f32 {
        if self.on {
            0.5 * self.osc.tick()
        } else {
            0.0
        }
    }
}

fn sine_like() -> Poly<SineVoice> {
    Poly::new(
        vec![SineVoice {
            osc: Oscillator::new(Shape::Sine, 440.0, RATE),
            on: false,
        }],
        RATE,
    )
}

// Measurements are relative: the subtractive voice's detuned second
// oscillator and a string's strong upper partials bias an absolute
// frequency estimate by a few cents (or lock it to a harmonic), but the
// same voice measured twice cancels that bias exactly.

#[test]
fn the_default_table_renders_identically() {
    let mut a = sine_like();
    let mut b = sine_like();
    b.set_tuning(Some(TuningTable::default()));
    for key in [48u8, 60, 69, 81] {
        let fa = measure(&mut a, |p| p.note_on(0, key, 1.0));
        let fb = measure(&mut b, |p| p.note_on(0, key, 1.0));
        assert!(cents(fa, fb).abs() < 0.1, "key {key}: {fa} vs {fb}");
    }
}

#[test]
fn a_just_scale_sounds_its_ratios() {
    // Mayamalavagowla on C: S R1 G3 M1 P D1 N3 as just ratios, seven
    // degrees on seven keys per octave from middle C.
    let ratios = [
        16.0 / 15.0,
        5.0 / 4.0,
        4.0 / 3.0,
        3.0 / 2.0,
        8.0 / 5.0,
        15.0 / 8.0,
    ];
    let tonic = 261.63;
    #[expect(clippy::cast_possible_truncation, reason = "cents")]
    let scale = ScaleTuning {
        tonic_pitch: 60,
        tonic_hz: tonic as f32,
        cents: ratios
            .iter()
            .map(|r: &f64| (1200.0 * r.log2()) as f32)
            .collect(),
        period_cents: 1200.0,
    };
    let mut p = sine_like();
    p.set_tuning(Some(TuningTable::from_tuning(&scale)));
    let s = measure(&mut p, |p| p.note_on(0, 60, 1.0));
    let reference = measure(&mut sine_like(), |p| p.note_on(0, 60, 1.0));
    assert!(
        cents(s, reference).abs() < 0.5,
        "the tonic stays at middle C"
    );
    for (i, r) in ratios.iter().enumerate() {
        #[expect(clippy::cast_possible_truncation, reason = "test keys")]
        let key = 61 + i as u8;
        let f = measure(&mut p, |p| p.note_on(0, key, 1.0));
        assert!(
            cents(f / s, *r).abs() < 2.0,
            "degree {}: {} vs {r}",
            i + 1,
            f / s
        );
    }
}

#[test]
fn fractional_pitch_and_tuning_compose() {
    let mut p = sine_like();
    let a4 = measure(&mut p, |p| p.note_on(0, 69, 1.0));
    // A quarter-tone above A4 in default tuning.
    let f = measure(&mut p, |p| p.note_on_pitch(0, 69, 69.5, 1.0));
    assert!((cents(f, a4) - 50.0).abs() < 1.0, "{}", cents(f, a4));
    // 24-EDO: key 70 is one quarter-tone above key 69.
    p.set_tuning(Some(TuningTable::from_tuning(&EqualTemperament {
        divisions: 24.0,
        ..EqualTemperament::default()
    })));
    let f = measure(&mut p, |p| p.note_on(0, 70, 1.0));
    assert!((cents(f, a4) - 50.0).abs() < 1.0, "{}", cents(f, a4));
}

#[test]
fn every_voice_kind_follows_the_tuning() {
    // A physical model calls pitch_to_hz itself; retuning happens before.
    let t = TuningTable::from_tuning(&EqualTemperament {
        reference_pitch: 69.0,
        reference_hz: 432.0,
        ..EqualTemperament::default()
    });
    let mut plain = Poly::new(vec![KarplusStrong::new(RATE, 7)], RATE);
    let f440 = measure(&mut plain, |p| p.note_on(0, 69, 1.0));
    let mut strings = Poly::new(vec![KarplusStrong::new(RATE, 7)], RATE);
    strings.set_tuning(Some(t));
    let f432 = measure(&mut strings, |p| p.note_on(0, 69, 1.0));
    let want = cents(432.0, f64::from(pitch_to_hz(69.0)));
    assert!(
        (cents(f432, f440) - want).abs() < 2.0,
        "{} vs {want}",
        cents(f432, f440)
    );
}
