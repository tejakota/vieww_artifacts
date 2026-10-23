//! Audit probes (2026-10-23). Each test asserts the CORRECT behaviour;
//! a failure here is a finding, not a broken probe.

use std::time::Duration;
use vieww_audio::{Mixer, Samples};

/// P1: a stereo buffer of equal channels must downmix to the same level.
#[test]
fn probe_stereo_downmix_equal_channels() {
    let buf = Samples::stereo(vec![0.25; 200], 8_000); // 100 frames of [0.25, 0.25]
    let mut m = Mixer::new(8_000);
    m.add_buffer(&buf, Duration::ZERO, 1.0).unwrap();
    let out = m.render();
    eprintln!("stereo [0.25,0.25] -> mono {:?}", &out.data[..3]);
    assert!((out.data[10] - 0.25).abs() < 1e-6, "expected 0.25, got {}", out.data[10]);
}

/// P2: a mix must not depend on the order sources were added.
#[test]
fn probe_mix_is_order_independent() {
    let pos = Samples::mono(vec![0.8; 100], 8_000);
    let neg = Samples::mono(vec![-0.8; 100], 8_000);
    let render = |order: &[&Samples]| {
        let mut m = Mixer::new(8_000);
        for s in order {
            m.add_buffer(s, Duration::ZERO, 1.0).unwrap();
        }
        m.render().data[50]
    };
    let a = render(&[&pos, &pos, &neg]);
    let b = render(&[&neg, &pos, &pos]);
    eprintln!("(+0.8,+0.8,-0.8) -> {a};  (-0.8,+0.8,+0.8) -> {b};  true sum 0.8");
    assert!((a - b).abs() < 1e-6, "order changed the mix: {a} vs {b}");
}

/// P3: a zero sample rate must be refused or handled, not panic in duration().
#[test]
fn probe_zero_rate_duration_does_not_panic() {
    let r = std::panic::catch_unwind(|| Samples::mono(vec![0.0; 10], 0).duration());
    eprintln!("Samples::mono(.., rate 0).duration() panicked: {}", r.is_err());
    assert!(r.is_ok(), "duration() panics on rate 0");
}

/// P4: NaN in a source must not reach the output.
#[test]
fn probe_nan_is_not_propagated_by_mixer() {
    let mut data = vec![0.1; 100];
    data[50] = f32::NAN;
    let mut m = Mixer::new(8_000);
    m.add_buffer(&Samples::mono(data, 8_000), Duration::ZERO, 1.0).unwrap();
    let out = m.render();
    let nans = out.data.iter().filter(|x| !x.is_finite()).count();
    eprintln!("non-finite samples in mixer output: {nans}");
    assert_eq!(nans, 0);
}

/// P5: one NaN through a stateful insert in the DAW mixer must not poison
/// every later block (a reverb's feedback loop holds whatever it is given).
#[test]
#[ignore = "open finding: a NaN in a stateful insert poisons the channel until reset"]
fn probe_advanced_mixer_recovers_after_one_nan() {
    use vieww_audio::fx::reverb::Freeverb;
    use vieww_audio::mixer::Mixer as Daw;
    let mut m = Daw::new(48_000);
    let t = m.add_track("a");
    m.add_insert(t, Box::new(Freeverb::new(0.8, 0.3, 48_000.0)), None).unwrap();
    let mut input = vec![0.1f32; 256 * 2];
    input[10] = f32::NAN;
    let mut out = vec![0.0f32; 256 * 2];
    m.process(&[&input], &mut out);
    let clean = vec![0.1f32; 256 * 2];
    for _ in 0..200 {
        m.process(&[&clean], &mut out); // ~1 s of clean input
    }
    let bad = out.iter().filter(|x| !x.is_finite()).count();
    eprintln!("non-finite samples after 1 s of clean input following one NaN: {bad}/{}", out.len());
    assert_eq!(bad, 0, "one NaN poisoned the mixer permanently");
}

/// P6: does the DAW mixer master bus clip or limit over-range sums? (record only)
#[test]
fn probe_advanced_mixer_headroom() {
    use vieww_audio::mixer::Mixer as Daw;
    let mut m = Daw::new(48_000);
    m.add_track("a");
    m.add_track("b");
    let x = vec![0.8f32; 128];
    let mut out = vec![0.0f32; 128];
    for _ in 0..50 { m.process(&[&x, &x], &mut out); }
    eprintln!("two tracks at 0.8 -> master sample {}", out[64]);
}

/// P7: CPU headroom. Renders 10 s of a 32-voice subtractive pad through a
/// DAW-mixer track with a Freeverb insert; prints real-time factor and the
/// worst single 128-frame block (2.67 ms budget at 48 kHz).
#[test]
#[ignore = "benchmark: timing-dependent, run with --ignored --nocapture"]
fn probe_cpu_headroom() {
    use std::time::Instant;
    use vieww_audio::fx::reverb::Freeverb;
    use vieww_audio::mixer::Mixer as Daw;
    use vieww_audio::synth::voice::{Poly, SubtractiveVoice};
    let rate = 48_000.0;
    for nv in [8usize, 32, 64] {
        let mut p = Poly::new((0..nv).map(|_| SubtractiveVoice::new(rate)).collect(), rate);
        for k in 0..nv { p.note_on(0, 36 + (k as u8 % 48), 0.8); }
        let mut m = Daw::new(48_000);
        let t = m.add_track("pad");
        m.add_insert(t, Box::new(Freeverb::new(0.8, 0.3, rate)), None).unwrap();
        let (mut inb, mut out) = (vec![0.0f32; 256], vec![0.0f32; 256]);
        let blocks = 48_000 * 10 / 128;
        let (mut worst, t0) = (0f64, Instant::now());
        for _ in 0..blocks {
            let b = Instant::now();
            for f in 0..128 { let (l, r) = p.next_stereo(); inb[2*f] = l; inb[2*f+1] = r; }
            m.process(&[&inb], &mut out);
            worst = worst.max(b.elapsed().as_secs_f64());
        }
        let total = t0.elapsed().as_secs_f64();
        eprintln!("{nv:>2} voices + reverb: {:.1}x realtime, worst block {:.3} ms of 2.667 ms budget",
            10.0 / total, worst * 1e3);
    }
}
