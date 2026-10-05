//! score — the film's soundtrack, **synthesised by `vieww-audio`**.
//!
//! The film was silent by construction. This module gives it a score made
//! the same way its pictures are: by the framework, deterministically,
//! from the film's own timeline. Every note is a `vieww_audio::Tone`
//! scheduled on a `vieww_audio::Mixer`; the pad is shaped by the crate's
//! `Biquad`, the plucks echo through its `Delay`, the master goes through
//! its `Compressor`, and the result is written by its `write_wav`. Nothing
//! is sampled, nothing is imported.
//!
//! **The score is cut to the picture, not the other way round.** Every
//! cue is a time derived from [`super::scenes`] — a scene's start, a toll
//! landing in Z02, a crate popping onto the orbit in Z06, a real action in
//! the studio's session — so re-timing a scene re-times its music.
//!
//! | layer | what | where |
//! |---|---|---|
//! | pad | one chord per scene, long attack/release, low-passed | everywhere |
//! | pulse | the root on the beat, sine, short | the engine through the proof |
//! | plucks | the chord arpeggiated in eighths, stereo echoes | the engine through the proof |
//! | ui | the studio's own clicks: tolls, crate pops, session actions | where they happen |
//! | hits | a sub thump and a shimmer | each movement's first frame, the end card |

use std::path::Path;
use std::time::Duration;

use vieww_audio::dsp::{apply, Biquad, Compressor, Delay, FilterKind};
use vieww_audio::{midi::note_to_hz, Envelope, Mixer, Samples, Tone, Waveform};

/// Output rate.
const RATE: u32 = 48_000;
/// The pulse's tempo: 100 bpm.
const BEAT: f32 = 0.6;

fn ms(s: f32) -> Duration {
    Duration::from_secs_f32(s.max(0.0))
}

/// The chord under each scene, as MIDI notes (low to high).
fn chord(id: &str) -> &'static [u8] {
    match id {
        "Z00" => &[33, 45, 52, 57, 60],         // A minor, bare
        "Z01" => &[33, 45, 52, 55, 60],         // Am7
        "Z02" => &[29, 41, 48, 53, 57],         // F
        "Z05" => &[28, 40, 47, 52, 57],         // E sus4 — the question
        "Z06" => &[36, 48, 55, 60, 64, 74],     // C add9 — the relief
        "Z10C" => &[29, 41, 48, 52, 57, 64],    // Fmaj7
        "Z10D" => &[31, 43, 50, 55, 59, 64],    // G6
        "Z10B" => &[33, 45, 52, 55, 60, 64],    // Am7
        "Z11" => &[36, 48, 55, 59, 62, 64],     // Cmaj9
        "Z12" => &[33, 45, 52, 55, 60],         // Am7
        "Z12B" => &[29, 41, 48, 52, 57, 64],    // Fmaj7
        "Z13" => &[31, 43, 50, 55, 59],         // G
        "Z13B" => &[28, 40, 47, 52, 55, 59],    // Em7
        "Z14" => &[29, 41, 48, 52, 57],         // Fmaj7
        "Z15" => &[36, 48, 55, 60, 64, 67],     // C
        "Z16" => &[33, 45, 52, 57, 60, 64],     // Am
        "Z17" => &[31, 43, 50, 55, 60, 62],     // G sus4 → the build
        "Z18" => &[33, 45, 52, 55, 60, 64],     // Am7
        "Z19" => &[29, 41, 48, 52, 57, 60],     // Fmaj7
        "Z19B" => &[31, 43, 50, 55, 59, 62],    // G
        "Z20" => &[31, 43, 50, 55, 60, 62],     // G sus — the lift
        _ => &[24, 36, 43, 48, 52, 55, 60, 64, 67], // C — home
    }
}

/// Whether the pulse and plucks play under a scene.
fn moving(id: &str) -> bool {
    !matches!(id, "Z00" | "Z01" | "Z02" | "Z05" | "Z20" | "Z21" | "Z22")
}

fn tone(note: f32, wave: Waveform, hold: f32, attack: f32, release: f32) -> Tone {
    Tone::held(note_to_hz(note), wave, ms(hold)).envelope(Envelope::attack_release(ms(attack), ms(release)))
}

/// Pad the buffer (mono) to `n` frames.
fn fit(mut s: Samples, n: usize) -> Vec<f32> {
    s.data.resize(n, 0.0);
    s.data
}

/// Render the whole score; returns interleaved stereo at [`RATE`].
pub fn render() -> Samples {
    let scenes = super::scenes();
    let total: f32 = scenes.iter().map(|s| s.seconds).sum();
    let n = (total * RATE as f32).ceil() as usize + RATE as usize;
    let starts: Vec<f32> = (0..scenes.len()).map(super::scene_start).collect();
    let start_of = |id: &str| scenes.iter().position(|s| s.id == id).map(|i| starts[i]).unwrap_or(0.0);

    let mut pad = Mixer::new(RATE);
    let mut pulse = Mixer::new(RATE);
    let mut pluck = Mixer::new(RATE);
    let mut ui = Mixer::new(RATE);
    let mut hits = Mixer::new(RATE);

    // ── The pad and the moving layers, scene by scene ──────────────────
    let mut beat_clock = 0.0_f32;
    for (si, s) in scenes.iter().enumerate() {
        let at = starts[si];
        let notes = chord(s.id);
        // The arc in loudness: the need is quiet, the engine opens up, the
        // studio sits back under the narration, the proof and the release
        // carry it home.
        let arc = match super::movement_of(s.id) {
            "MOVEMENT I" => 0.55,
            "MOVEMENT II" => 0.9,
            "MOVEMENT III" => 0.75,
            "MOVEMENT IV" => 0.9,
            _ => 1.0,
        };
        let v = 0.055 / notes.len() as f32 * 3.0 * arc;
        for (k, &m) in notes.iter().enumerate() {
            let m = f32::from(m);
            // The bass of the chord as a sine an octave down; the rest as
            // two slightly detuned triangles — width without noise.
            if k == 0 {
                pad.tone_at(tone(m, Waveform::Sine, s.seconds, 1.4, 2.2), ms(at), 0.16 * arc);
            } else {
                pad.tone_at(tone(m + 0.04, Waveform::Triangle, s.seconds, 1.6, 2.4), ms(at), v);
                pad.tone_at(tone(m - 0.04, Waveform::Sine, s.seconds, 1.9, 2.6), ms(at), v * 0.8);
            }
        }
        if moving(s.id) {
            // The pulse: the chord's root, on the beat, kept on one clock
            // across scenes so the tempo never stumbles at a cut.
            let end = at + s.seconds;
            if beat_clock < at {
                beat_clock = at + (BEAT - ((at - beat_clock) % BEAT)) % BEAT;
            }
            let studio = s.id.starts_with("Z11") || s.id.starts_with("Z12") || s.id.starts_with("Z13");
            let gain = if studio { 0.10 } else { 0.15 };
            let mut b = beat_clock;
            let mut step = 0usize;
            let upper: Vec<f32> = notes[1..].iter().map(|&m| f32::from(m) + 12.0).collect();
            const ARP: [usize; 8] = [0, 2, 1, 3, 2, 4, 3, 1];
            while b < end - 0.05 {
                pulse.tone_at(tone(f32::from(notes[0]) + 12.0, Waveform::Sine, 0.04, 0.004, 0.32), ms(b), gain);
                for half in 0..2 {
                    let t = b + half as f32 * BEAT * 0.5;
                    if t >= end - 0.05 {
                        break;
                    }
                    let m = upper[ARP[step % ARP.len()] % upper.len()];
                    let g = if studio { 0.030 } else { 0.042 } * if step % 4 == 0 { 1.2 } else { 1.0 };
                    pluck.tone_at(tone(m, Waveform::Triangle, 0.01, 0.003, 0.42), ms(t), g);
                    step += 1;
                }
                b += BEAT;
            }
            beat_clock = b;
        }
    }

    // ── The lift into the end card: a swell that stops on the cut ──────
    let z20 = start_of("Z20");
    let z21 = start_of("Z21");
    for (k, &m) in [43.0_f32, 50.0, 55.0, 62.0, 67.0].iter().enumerate() {
        let a = z20 + 2.0 + k as f32 * 0.6;
        pad.tone_at(tone(m, Waveform::Saw, (z21 - a - 0.05).max(0.1), 5.0, 0.12), ms(a), 0.012);
    }

    // ── Hits: each movement's first frame, and the end card ────────────
    for id in ["Z06", "Z11", "Z18", "Z21"] {
        let at = start_of(id);
        let big = id == "Z21";
        hits.tone_at(tone(28.0, Waveform::Sine, 0.08, 0.005, if big { 2.4 } else { 1.1 }), ms(at), if big { 0.55 } else { 0.38 });
        hits.tone_at(tone(96.0, Waveform::Sine, 0.02, 0.01, if big { 3.0 } else { 1.6 }), ms(at + 0.02), 0.035);
        hits.tone_at(tone(103.0, Waveform::Sine, 0.02, 0.01, if big { 3.4 } else { 1.8 }), ms(at + 0.09), 0.025);
    }

    // ── The UI's own sounds, where the picture makes them ───────────────
    // Z02: each toll tile slaps flat (see m1_need::toll_flip).
    let z02 = start_of("Z02");
    for g in 0..4 {
        let land = z02 + (0.14 + g as f32 * 0.14 + 0.22) * 12.0 + 0.46 * 0.62;
        ui.tone_at(tone(40.0, Waveform::Sine, 0.03, 0.002, 0.22), ms(land), 0.22);
        ui.tone_at(tone(88.0, Waveform::Square, 0.004, 0.001, 0.05), ms(land), 0.018);
    }
    // Z06: every new crate that pops onto the orbit, rising.
    let z06 = start_of("Z06");
    const PENTA: [f32; 5] = [0.0, 2.0, 4.0, 7.0, 9.0];
    for k in 0..super::NEW_CRATES.len() {
        let land = z06 + (0.58 + k as f32 * 0.016 + 0.05) * 15.0;
        let m = 72.0 + PENTA[k % 5] + 12.0 * (k / 5) as f32;
        ui.tone_at(tone(m, Waveform::Sine, 0.01, 0.002, 0.5), ms(land), 0.05);
    }
    // The studio: a soft click on every action the session takes.
    for (at, _) in super::script::session() {
        ui.tone_at(tone(91.0, Waveform::Sine, 0.004, 0.001, 0.05), ms(at), 0.03);
        ui.tone_at(tone(79.0, Waveform::Triangle, 0.004, 0.001, 0.08), ms(at), 0.02);
    }

    // ── Mix ─────────────────────────────────────────────────────────────
    let pad = apply(&pad.render(), || Biquad::new(FilterKind::LowPass, 1600.0, 0.707, RATE as f32));
    let pulse = apply(&pulse.render(), || Biquad::new(FilterKind::LowPass, 900.0, 0.707, RATE as f32));
    let pluck = pluck.render();
    let pluck_l = apply(&pluck, || Delay::new(BEAT * 0.75, RATE as f32, 0.38, 0.32));
    let pluck_r = apply(&pluck, || Delay::new(BEAT * 0.5, RATE as f32, 0.42, 0.36));
    let (pad, pulse, pluck_l, pluck_r) = (fit(pad, n), fit(pulse, n), fit(pluck_l, n), fit(pluck_r, n));
    let (ui, hits) = (fit(ui.render(), n), fit(hits.render(), n));
    let mut l = vec![0.0_f32; n];
    let mut r = vec![0.0_f32; n];
    for i in 0..n {
        let common = pad[i] + pulse[i] + hits[i] + ui[i];
        l[i] = common + pluck_l[i];
        r[i] = common + pluck_r[i];
    }
    let comp = || Compressor::new(-14.0, 3.0, 0.01, 0.25, RATE as f32);
    let l = apply(&Samples::mono(l, RATE), comp).data;
    let r = apply(&Samples::mono(r, RATE), comp).data;
    // Normalise to −1 dBFS, and fade the last two seconds of the film.
    let peak = l.iter().chain(r.iter()).fold(0.0_f32, |m, x| m.max(x.abs())).max(1e-6);
    let gain = 0.89 / peak;
    let film_n = (total * RATE as f32) as usize;
    let fade = (2.0 * RATE as f32) as usize;
    let mut out = Vec::with_capacity(film_n * 2);
    for i in 0..film_n {
        let f = if i + fade > film_n { (film_n - i) as f32 / fade as f32 } else { 1.0 };
        out.push(l[i] * gain * f);
        out.push(r[i] * gain * f);
    }
    Samples::stereo(out, RATE)
}

/// Render the score and write it as `score.wav` under `root`.
pub fn write(root: &Path) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let samples = render();
    let bytes = vieww_audio::wav::write_wav(&samples).map_err(|e| format!("{e:?}"))?;
    let path = root.join("score.wav");
    std::fs::write(&path, bytes)?;
    Ok(path)
}
