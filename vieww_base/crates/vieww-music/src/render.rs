//! A score to sound, through the engine.
//!
//! Each [`Part`] plays on a [`Poly`] of the voice its [`Instrument`]
//! names, in the score's tuning (percussion untuned), as MPE: every
//! sounding note gets its own channel, and its gamaka is applied as an
//! exact per-channel bend ([`Poly::set_channel_bend`]) every few samples.
//! The parts then go through the engine's DAW [`Mixer`] — fader, pan and a
//! reverb send per part — and a mastering stage, to a stereo [`Samples`]
//! buffer and, if wanted, a WAV or FLAC file.
//!
//! **Mastering** is gain staging done properly: the mix is summed with
//! full floating-point headroom (nothing clips); everything above 20 kHz
//! is filtered out (inaudible, and the source of inter-sample peaks that
//! short true-peak filters under-read — a plucked string's noise burst
//! reaches 24 kHz); its integrated loudness is measured (EBU R128) and
//! brought to [`Settings::target_lufs`]; only then does a true-peak
//! limiter catch the peaks that remain, with its lookahead latency
//! compensated, and a final trim if any inter-sample peak still reads
//! above the ceiling. A limiter that is slammed by an
//! un-staged mix compresses everything and overshoots its own ceiling;
//! one fed a staged mix touches only the loudest transients.
//!
//! Rendering is offline and deterministic: the same score gives the same
//! samples, bit for bit, on the same platform.

use vieww_audio::fx::dynamics::Limiter;
use vieww_audio::fx::reverb::Freeverb;
use vieww_audio::mixer::{Channel, Mixer, Tap};
pub use vieww_audio::synth::percussion::Stroke;
use vieww_audio::synth::percussion::{DrumKit, Mridangam};
use vieww_audio::synth::physical::{Bowed, Clarinet, KarplusStrong, Modal, Tambura};
use vieww_audio::synth::tuning::TuningTable;
use vieww_audio::synth::voice::{Poly, SubtractiveVoice, Voice};
use vieww_audio::Samples;

use crate::score::{Event, Instrument, Part, Score};

/// Rendering settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// Sample rate.
    pub rate: u32,
    /// Voices per part.
    pub polyphony: usize,
    /// Seconds of tail after the last note (reverb and release).
    pub tail: f64,
    /// Master ceiling in dBTP.
    pub ceiling_db: f32,
    /// Integrated loudness to master to, LUFS (−16 leaves room for
    /// dynamics; streaming services normalise to −14 or −16).
    pub target_lufs: f64,
    /// The tonic for the mridangam's right head, Hz (its strokes are
    /// keyed, not pitched).
    pub mridangam_tonic_hz: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            rate: 48_000,
            polyphony: 16,
            tail: 2.5,
            ceiling_db: -1.0,
            target_lufs: -16.0,
            mridangam_tonic_hz: 261.63,
        }
    }
}

/// Samples between gamaka updates (a curve at 1.5 kHz control rate).
const CONTROL: usize = 32;

/// A note on its channel: when it starts and stops, in samples.
#[derive(Debug, Clone)]
struct Scheduled {
    on: usize,
    off: usize,
    channel: u8,
    key: u8,
    event: Event,
}

/// Schedule a part's notes on MPE channels 1–15, round-robin, each
/// overlapping note on its own channel.
fn schedule(part: &Part, spb: f64, rate: f64) -> Vec<Scheduled> {
    let mut ev: Vec<&Event> = part.events.iter().collect();
    ev.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut busy = [0usize; 15];
    let mut next = 0usize;
    ev.into_iter()
        .map(|e| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "sample index"
            )]
            let on = (e.start.max(0.0) * spb * rate).round() as usize;
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "sample index"
            )]
            let off = (e.end().max(0.0) * spb * rate).round() as usize;
            let ch = (0..15)
                .map(|k| (next + k) % 15)
                .find(|&c| busy[c] <= on)
                .unwrap_or(next % 15);
            busy[ch] = off;
            next = (ch + 1) % 15;
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "clamped"
            )]
            let key = e.pitch.round().clamp(0.0, 127.0) as u8;
            Scheduled {
                on,
                off: off.max(on + 1),
                channel: u8::try_from(ch + 1).unwrap_or(1),
                key,
                event: e.clone(),
            }
        })
        .collect()
}

/// Play one part on a pool of `voices`, `frames` long, interleaved stereo.
fn play<V: Voice>(
    mut poly: Poly<V>,
    part: &Part,
    tuning: Option<TuningTable>,
    spb: f64,
    rate: f64,
    frames: usize,
) -> Vec<f32> {
    poly.mpe = true;
    poly.bend_range = 48.0;
    poly.set_tuning(tuning);
    let notes = schedule(part, spb, rate);
    let mut ons: Vec<usize> = (0..notes.len()).collect();
    ons.sort_by_key(|&i| notes[i].on);
    let mut offs = ons.clone();
    offs.sort_by_key(|&i| notes[i].off);
    let (mut oi, mut fi) = (0usize, 0usize);
    let mut sounding: Vec<usize> = Vec::new();
    let mut out = vec![0.0f32; frames * 2];
    for f in 0..frames {
        while fi < offs.len() && notes[offs[fi]].off <= f {
            let n = &notes[offs[fi]];
            poly.note_off(n.channel, n.key);
            sounding.retain(|&i| i != offs[fi]);
            fi += 1;
        }
        while oi < ons.len() && notes[ons[oi]].on <= f {
            let i = ons[oi];
            let n = &notes[i];
            poly.set_channel_bend(n.channel, 0.0);
            poly.note_on_pitch(n.channel, n.key, n.event.pitch, n.event.velocity);
            sounding.push(i);
            oi += 1;
        }
        if f % CONTROL == 0 {
            for &i in &sounding {
                let n = &notes[i];
                #[expect(clippy::cast_precision_loss, reason = "sample counts")]
                let len = (n.off - n.on) as f32;
                #[expect(clippy::cast_precision_loss, reason = "sample counts")]
                let t = (f - n.on) as f32 / len;
                #[expect(clippy::cast_possible_truncation, reason = "seconds in f32")]
                let secs = (len / rate as f32).max(1e-3);
                let cents = n.event.gamaka.cents(t, secs);
                poly.set_channel_bend(n.channel, cents / 100.0);
            }
        }
        let (l, r) = poly.next_stereo();
        out[f * 2] = l;
        out[f * 2 + 1] = r;
    }
    out
}

/// Render one part alone (no mixer), interleaved stereo.
fn render_part(part: &Part, score: &Score, s: Settings, frames: usize) -> Vec<f32> {
    #[expect(clippy::cast_precision_loss, reason = "sample rates")]
    let rate = s.rate as f32;
    let spb = score.seconds_per_beat();
    let n = s.polyphony.max(1);
    let tuned = Some(score.tuning.table());
    let seeds = 1..=u64::try_from(n).unwrap_or(16);
    let r = f64::from(rate);
    match part.instrument {
        Instrument::Pluck => {
            let v = seeds.map(|k| KarplusStrong::new(rate, k)).collect();
            play(Poly::new(v, rate), part, tuned, spb, r, frames)
        }
        Instrument::Bowed => {
            let v = (0..n).map(|_| Bowed::new(rate)).collect();
            play(Poly::new(v, rate), part, tuned, spb, r, frames)
        }
        Instrument::Reed => {
            let v = seeds.map(|k| Clarinet::new(rate, k)).collect();
            play(Poly::new(v, rate), part, tuned, spb, r, frames)
        }
        Instrument::Mallet => {
            let v = (0..n).map(|_| Modal::marimba(rate)).collect();
            play(Poly::new(v, rate), part, tuned, spb, r, frames)
        }
        Instrument::Pad => {
            let v = (0..n)
                .map(|_| {
                    let mut v = SubtractiveVoice::new(rate);
                    v.cutoff = 1_800.0;
                    v.env_amount = 1.0;
                    v
                })
                .collect();
            play(Poly::new(v, rate), part, tuned, spb, r, frames)
        }
        Instrument::Bass => {
            let v = (0..n)
                .map(|_| {
                    let mut v = SubtractiveVoice::new(rate);
                    v.cutoff = 500.0;
                    v.osc2_semis = -12.0;
                    v.osc2_mix = 0.4;
                    v
                })
                .collect();
            play(Poly::new(v, rate), part, tuned, spb, r, frames)
        }
        Instrument::Lead => {
            let v = (0..n)
                .map(|_| {
                    let mut v = SubtractiveVoice::new(rate);
                    v.cutoff = 3_500.0;
                    v.resonance = 1.6;
                    v
                })
                .collect();
            let mut x = play(Poly::new(v, rate), part, tuned, spb, r, frames);
            // Overdrive: the rock guitar's grit.
            for v in &mut x {
                *v = (*v * 4.0).tanh() * 0.6;
            }
            x
        }
        Instrument::Drums => {
            let v = seeds.map(|k| DrumKit::new(rate, k)).collect();
            play(Poly::new(v, rate), part, None, spb, r, frames)
        }
        Instrument::Tambura => {
            let v = seeds.map(|k| Tambura::new(rate, k)).collect();
            play(Poly::new(v, rate), part, tuned, spb, r, frames)
        }
        Instrument::Mridangam => {
            let v = (0..n)
                .map(|_| Mridangam::new(rate, s.mridangam_tonic_hz))
                .collect();
            play(Poly::new(v, rate), part, None, spb, r, frames)
        }
    }
}

/// Render `score` to interleaved stereo at `settings.rate`.
#[must_use]
pub fn render(score: &Score, settings: Settings) -> Samples {
    let rate = f64::from(settings.rate);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "frame count"
    )]
    let frames = ((score.seconds() + settings.tail) * rate).ceil() as usize;
    let parts: Vec<Vec<f32>> = score
        .parts
        .iter()
        .map(|p| render_part(p, score, settings, frames))
        .collect();

    #[expect(clippy::cast_precision_loss, reason = "sample rates")]
    let fr = settings.rate as f32;
    let mut mixer = Mixer::new(settings.rate);
    let verb_bus = mixer.add_bus("reverb");
    let mut verb = Freeverb::new(0.82, 0.35, fr);
    verb.wet = 1.0;
    verb.dry = 0.0;
    let _ = mixer.add_insert(verb_bus, Box::new(verb), None);
    let bus_index = match verb_bus {
        Channel::Bus(b) => b,
        _ => 0,
    };
    for p in &score.parts {
        let t = mixer.add_track(&p.name);
        if let Ok(params) = mixer.params_mut(t) {
            params.fader_db = p.gain_db;
            params.pan = p.pan.clamp(-1.0, 1.0);
        }
        if p.reverb_db > -90.0 {
            let _ = mixer.add_send(t, bus_index, p.reverb_db, Tap::PostFader);
        }
    }
    let inputs: Vec<&[f32]> = parts.iter().map(Vec::as_slice).collect();
    let mut out = vec![0.0f32; frames * 2];
    mixer.process(&inputs, &mut out);
    master(&mut out, settings);
    Samples::stereo(out, settings.rate)
}

/// The mastering stage, in place: normalise to the target loudness, then
/// true-peak limit (latency compensated), then trim any residual
/// overshoot so the ceiling holds.
fn master(out: &mut [f32], settings: Settings) {
    use vieww_audio::channels::Layout;
    use vieww_audio::fx::Stereo;
    use vieww_audio::meter::normalization_gain;

    let rate = settings.rate;
    #[expect(clippy::cast_precision_loss, reason = "sample rates")]
    let fr = rate as f32;
    // 4th-order Butterworth low-pass at 20 kHz (two biquads, Q 0.541 and
    // 1.307), per channel; below 44.1 kHz rates the corner sits at 0.45 fs.
    {
        use vieww_audio::dsp::{Biquad, FilterKind, Processor};
        let corner = 20_000.0f32.min(0.45 * fr);
        for ch in 0..2 {
            let mut a = Biquad::new(FilterKind::LowPass, corner, 0.541_2, fr);
            let mut b = Biquad::new(FilterKind::LowPass, corner, 1.306_6, fr);
            for v in out.iter_mut().skip(ch).step_by(2) {
                *v = b.process(a.process(*v));
            }
        }
    }
    let gain_db =
        normalization_gain(out, Layout::Stereo, rate, settings.target_lufs).clamp(-40.0, 30.0);
    #[expect(clippy::cast_possible_truncation, reason = "a gain")]
    let g = 10f32.powf(gain_db as f32 / 20.0);
    for v in out.iter_mut() {
        *v *= g;
    }
    let mut lim = Limiter::true_peak(settings.ceiling_db, 0.005, 0.1, fr);
    let lat = lim.latency();
    let frames = out.len() / 2;
    let mut limited = Vec::with_capacity(out.len());
    for f in 0..frames + lat {
        let (l, r) = if f < frames {
            (out[f * 2], out[f * 2 + 1])
        } else {
            (0.0, 0.0)
        };
        let (l, r) = lim.process_stereo(l, r);
        if f >= lat {
            limited.push(l);
            limited.push(r);
        }
    }
    out.copy_from_slice(&limited);
    // A limiter's ceiling is a target, not a promise, between samples:
    // its realtime 4× detector reads true peaks about 0.1 dB low. Offline
    // there is time to measure exactly — 4× through the engine's −100 dB
    // sinc resampler — and trim the master if anything is still over.
    let tp = exact_true_peak_db(out, rate);
    let over = tp - f64::from(settings.ceiling_db);
    if over > 0.0 {
        #[expect(clippy::cast_possible_truncation, reason = "a gain")]
        let trim = 10f32.powf(-(over as f32 + 0.02) / 20.0);
        for v in out.iter_mut() {
            *v *= trim;
        }
    }
}

/// True peak in dBTP of interleaved stereo, measured by 4× oversampling
/// with the engine's 32-tap Kaiser sinc (−100 dB residual).
#[must_use]
pub fn exact_true_peak_db(interleaved: &[f32], rate: u32) -> f64 {
    let s = Samples::stereo(interleaved.to_vec(), rate);
    let up = s.resampled(rate * 4, vieww_audio::resample::Quality::Sinc(32));
    let peak = up.data.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    20.0 * f64::from(peak.max(1e-12)).log10()
}

/// Render `score` and encode it as a 16-bit WAV file's bytes.
///
/// # Errors
/// The encoder's error, if the buffer cannot be written.
pub fn render_wav(score: &Score, settings: Settings) -> Result<Vec<u8>, vieww_audio::PcmError> {
    vieww_audio::write_wav(&render(score, settings))
}

/// Render `score` and encode it as a 16-bit FLAC file's bytes (lossless,
/// about half a WAV's size).
///
/// # Errors
/// The encoder's error.
pub fn render_flac(
    score: &Score,
    settings: Settings,
) -> Result<Vec<u8>, vieww_audio::codec::CodecError> {
    let s = render(score, settings);
    vieww_audio::codec::flac::encode(
        &vieww_audio::codec::Decoded {
            data: s.data,
            channels: s.channels,
            rate: s.rate,
        },
        16,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carnatic::gamaka::Gamaka;
    use crate::pitch::{chromatic_just, JUST_CHROMATIC};
    use crate::score::ScoreTuning;
    use vieww_audio::quality::estimate_frequency;

    fn settings() -> Settings {
        Settings {
            tail: 0.5,
            ..Settings::default()
        }
    }

    fn left(s: &Samples, from: f64, to: f64) -> Vec<f32> {
        let r = f64::from(s.rate);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "test"
        )]
        let (a, b) = ((from * r) as usize, (to * r) as usize);
        s.data
            .iter()
            .step_by(2)
            .skip(a)
            .take(b - a)
            .copied()
            .collect()
    }

    #[test]
    fn a_score_renders_bounded_and_finite() {
        let mut sc = Score::new("t", 120.0);
        let p = sc.part("pad", Instrument::Pad);
        for (i, k) in [60.0, 64.0, 67.0].into_iter().enumerate() {
            #[expect(clippy::cast_precision_loss, reason = "test")]
            p.push(Event::new(i as f64 * 0.5, 2.0, k, 0.9));
        }
        sc.part("drums", Instrument::Drums)
            .push(Event::new(0.0, 0.5, 36.0, 1.0))
            .push(Event::new(1.0, 0.5, 38.0, 1.0));
        let out = render(&sc, settings());
        assert_eq!(out.channels, 2);
        assert!(out.data.iter().all(|v| v.is_finite()));
        let ceiling = 10f32.powf(-1.0 / 20.0);
        assert!(out.peak() <= ceiling + 0.01, "limited: {}", out.peak());
        assert!(out.peak() > 0.05, "audible");
    }

    #[test]
    fn the_master_meets_loudness_and_ceiling() {
        use vieww_audio::channels::Layout;
        use vieww_audio::meter::measure;
        let mut sc = Score::new("t", 120.0);
        let p = sc.part("pad", Instrument::Pad);
        for i in 0..16 {
            #[expect(clippy::cast_precision_loss, reason = "test")]
            p.push(Event::new(
                f64::from(i) * 0.5,
                0.5,
                48.0 + (i % 12) as f32,
                1.0,
            ));
        }
        let d = sc.part("drums", Instrument::Drums);
        for i in 0..16 {
            d.push(Event::new(
                f64::from(i) * 0.5,
                0.25,
                if i % 2 == 0 { 36.0 } else { 38.0 },
                1.0,
            ));
        }
        let s = Settings::default();
        let out = render(&sc, s);
        let (lufs, _, _) = measure(&out.data, Layout::Stereo, out.rate);
        assert!((lufs - s.target_lufs).abs() < 1.5, "loudness {lufs}");
        let tp = exact_true_peak_db(&out.data, out.rate);
        assert!(tp <= f64::from(s.ceiling_db) + 1e-3, "true peak {tp}");
    }

    #[test]
    fn the_tuning_reaches_the_sound() {
        let mut et = Score::new("t", 60.0);
        let mut b = et.part("reed", Instrument::Reed).clone();
        b.reverb_db = -120.0;
        b.events.push(Event::new(0.0, 2.0, 64.0, 0.8));
        et.parts = vec![b];
        let mut ji = et.clone();
        ji.tuning = ScoreTuning::Scale(chromatic_just(60, 261.63, &JUST_CHROMATIC));
        let f = |s: &Score| estimate_frequency(&left(&render(s, settings()), 0.8, 1.8), 48_000.0);
        let (fe, fj) = (f(&et), f(&ji));
        let cents = 1200.0 * (fj / fe).log2();
        assert!(
            (cents + 13.7).abs() < 3.0,
            "a just third sounds 13.7 c flat: {cents}"
        );
    }

    #[test]
    fn a_gamaka_bends_the_sound() {
        let mut plain = Score::new("t", 60.0);
        let mut b = plain.part("veena", Instrument::Pluck).clone();
        b.reverb_db = -120.0;
        b.events.push(Event::new(0.0, 2.0, 57.0, 0.9));
        plain.parts = vec![b];
        let mut glide = plain.clone();
        glide.parts[0].events[0] = Event::new(0.0, 2.0, 57.0, 0.9).with_gamaka(Gamaka::Glide {
            to_cents: 200.0,
            portion: 0.5,
        });
        let f = |s: &Score, a, b| estimate_frequency(&left(&render(s, settings()), a, b), 48_000.0);
        let start = 1200.0 * (f(&glide, 0.1, 0.9) / f(&plain, 0.1, 0.9)).log2();
        let end = 1200.0 * (f(&glide, 1.9, 2.0) / f(&plain, 1.9, 2.0)).log2();
        assert!(start.abs() < 5.0, "before the glide: {start}");
        assert!((end - 200.0).abs() < 15.0, "glided a whole tone: {end}");
    }

    #[test]
    fn rendering_is_deterministic() {
        let mut sc = Score::new("t", 100.0);
        sc.part("strings", Instrument::Pluck)
            .push(Event::new(0.0, 1.0, 62.0, 0.7));
        sc.part("mridangam", Instrument::Mridangam).push(Event::new(
            0.0,
            0.5,
            f32::from(Stroke::Dhin.key()),
            0.9,
        ));
        let s = Settings {
            tail: 0.2,
            ..Settings::default()
        };
        assert_eq!(render(&sc, s).data, render(&sc, s).data);
        let wav = render_wav(&sc, s).unwrap();
        assert_eq!(&wav[..4], b"RIFF");
    }
}
