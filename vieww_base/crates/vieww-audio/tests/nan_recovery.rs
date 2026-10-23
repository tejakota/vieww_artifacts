//! A non-finite sample must never silence a channel for good.
//!
//! Every stateful stereo effect is fed one NaN, reset, and must then
//! process ordinary audio to finite, audible output; the DAW mixer must
//! reset a faulting insert by itself and count the fault.

use vieww_audio::fx::delay::{DelayTime, StereoDelay};
use vieww_audio::fx::dynamics::{Dynamics, Limiter};
use vieww_audio::fx::modfx::Chorus;
use vieww_audio::fx::reverb::{ConvolutionReverb, Fdn, Freeverb};
use vieww_audio::fx::Stereo;
use vieww_audio::mixer::{Channel, Mixer};

const RATE: f32 = 48_000.0;

fn effects() -> Vec<(&'static str, Box<dyn Stereo + Send>)> {
    let ir: Vec<f32> = (0..2_048).map(|i| 0.5 * 0.998f32.powi(i)).collect();
    vec![
        ("Freeverb", Box::new(Freeverb::new(0.8, 0.3, RATE))),
        ("Fdn", Box::new(Fdn::new(2.0, 6_000.0, 0.01, RATE))),
        (
            "ConvolutionReverb",
            Box::new(ConvolutionReverb::new(&ir, &ir, 256)),
        ),
        (
            "StereoDelay",
            Box::new(StereoDelay::new(DelayTime::Ms(120.0), 0.7, 1.0, RATE)),
        ),
        ("Chorus", Box::new(Chorus::new(3, 0.5, RATE))),
        (
            "Compressor",
            Box::new(Dynamics::compressor(-18.0, 4.0, 0.005, 0.1, RATE)),
        ),
        ("Limiter", Box::new(Limiter::new(-1.0, 0.005, 0.1, RATE))),
        (
            "TruePeakLimiter",
            Box::new(Limiter::true_peak(-1.0, 0.005, 0.1, RATE)),
        ),
    ]
}

fn tone(i: usize) -> f32 {
    0.3 * (i as f32 * 440.0 * std::f32::consts::TAU / RATE).sin()
}

#[test]
fn every_stateful_effect_recovers_after_reset() {
    for (name, mut fx) in effects() {
        for i in 0..4_800 {
            let x = if i == 100 { f32::NAN } else { tone(i) };
            fx.process_stereo(x, x);
        }
        fx.reset();
        let mut finite = true;
        let mut loudest = 0.0f32;
        for i in 0..48_000 {
            let (l, r) = fx.process_stereo(tone(i), tone(i));
            finite &= l.is_finite() && r.is_finite();
            loudest = loudest.max(l.abs()).max(r.abs());
        }
        assert!(finite, "{name}: still non-finite after reset");
        assert!(loudest > 1e-3, "{name}: silent after reset");
    }
}

/// An insert that emits one NaN, then behaves.
struct Glitch {
    n: u64,
    resets: u32,
}

impl Stereo for Glitch {
    fn process_stereo(&mut self, l: f32, r: f32) -> (f32, f32) {
        self.n += 1;
        if self.n == 50 {
            (f32::NAN, f32::INFINITY)
        } else {
            (l, r)
        }
    }

    fn reset(&mut self) {
        self.resets += 1;
    }
}

#[test]
fn the_mixer_resets_a_faulting_insert_and_keeps_the_channel_alive() {
    let mut m = Mixer::new(48_000);
    let t = m.add_track("plugin");
    m.add_insert(t, Box::new(Glitch { n: 0, resets: 0 }), None)
        .unwrap();
    m.add_insert(t, Box::new(Freeverb::new(0.8, 0.3, RATE)), None)
        .unwrap();
    let input: Vec<f32> = (0..512).flat_map(|i| [tone(i), tone(i)]).collect();
    let mut out = vec![0.0f32; 512];
    let mut nonfinite = 0;
    for _ in 0..200 {
        m.process(&[&input], &mut out);
        nonfinite += out.iter().filter(|x| !x.is_finite()).count();
    }
    assert_eq!(nonfinite, 0, "a NaN escaped the mixer");
    assert!(
        out.iter().any(|x| x.abs() > 1e-3),
        "the channel went silent"
    );
    assert_eq!(
        m.insert_faults(t, 0).unwrap(),
        1,
        "the fault is counted once"
    );
    assert_eq!(m.insert_faults(t, 1).unwrap(), 0, "the reverb never saw it");
    assert!(m.insert_faults(Channel::Master, 0).is_err());
}

#[test]
fn non_finite_track_input_never_enters_a_channel() {
    let mut m = Mixer::new(48_000);
    let t = m.add_track("bad source");
    m.add_insert(t, Box::new(Freeverb::new(0.8, 0.3, RATE)), None)
        .unwrap();
    let mut input = vec![0.2f32; 512];
    input[7] = f32::NAN;
    input[300] = f32::NEG_INFINITY;
    let mut out = vec![0.0f32; 512];
    for _ in 0..20 {
        m.process(&[&input], &mut out);
        assert!(out.iter().all(|x| x.is_finite()));
    }
    assert_eq!(m.insert_faults(t, 0).unwrap(), 0);
}
