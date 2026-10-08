//! score — **THE SPARK's soundtrack, synthesised by `vieww-audio`.**
//!
//! The film worked muted by construction; this module gives it a score
//! made the same way its pictures are: **by the framework,
//! deterministically, from the film's own timeline.** Every note is a
//! `vieww_audio::Tone` scheduled on a `vieww_audio::Mixer`; the pad is
//! shaped by the crate's `Biquad`, the plucks echo through its `Delay`,
//! the master goes through its `Compressor`, and the result is written
//! by its `write_wav`. Nothing is sampled, nothing is imported.
//!
//! **The score is cut to the picture, not the other way round.** Every
//! cue is a time derived from [`super::scenes`] — the break's crack,
//! each letter of the wordmark landing, a crate popping onto the grid,
//! a keystroke through the real studio, the tap's ripple, `rustc`
//! running, the package landing, the ledger's stamp, the end card — so
//! re-timing a scene re-times its music. The cut still works muted
//! (house rule): no beat *needs* its sound; the sound only warms what
//! the picture already says.
//!
//! **The musical arc is the film's arc.** The wait is still — no pulse,
//! no pluck, only a weary pad and the old world's slow clock ticks.
//! The groove enters at the hard cut into the machine (K1: silence and
//! 60 fps), sits back under the studio's narration, builds through the
//! cross-build, and is carried home by the ship. The end card is the
//! tonic it has been walking toward the whole film; the loop fades back
//! to the caret's drone.
//!
//! | layer | what | where |
//! |---|---|---|
//! | pad | one chord per scene, long attack/release, low-passed | everywhere |
//! | pulse | the chord's root on the beat (100 bpm), sine, short | the machine through the ship |
//! | plucks | the chord arpeggiated in eighths, stereo echoes | the machine through the ship |
//! | ui | the film's own sounds: ticks, cracks, letters, crate pops, keystrokes, taps, compiles, the stamp | where the picture makes them |
//! | hits | a sub thump and a shimmer | each act's first frame, the end card |

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

/// The chord under each scene, as MIDI notes (low to high). A-minor
/// family throughout — the studio_film score's key, kept: the spark's
/// world is the same world.
fn chord(id: &str) -> &'static [u8] {
    match id {
        "S01" => &[33, 45, 52],               // A, bare — the drone under the blank
        "S02" => &[33, 45, 52, 60, 64],       // Am add9 — the wait, weary
        "S03" => &[28, 40, 47, 51, 55],       // Em(maj7) — tension with no way down
        "S04" => &[29, 41, 48, 52, 57, 64],   // Fmaj7 — the light arrives
        "S05" => &[33, 45, 52, 55, 60],       // Am7 — the dive
        "S06" => &[36, 48, 55, 62, 64],       // C add9 — the city wakes
        "S07" => &[29, 41, 48, 52, 57, 60],   // Fmaj9 — the signal, apart
        "S08" => &[36, 48, 55, 59, 62, 64],   // Cmaj9 — the grid's clarity
        "S09" => &[33, 45, 52, 55, 60],       // Am7 — damage, economy
        "S10" => &[31, 43, 50, 55, 60, 62],   // G sus4 — the cadence, unresolved into the act
        "S11" => &[36, 48, 55, 59, 62, 64],   // Cmaj9 — the product's first light
        "S12" => &[29, 41, 48, 52, 57, 64],   // Fmaj7 — first paint
        "S13" => &[33, 45, 52, 55, 60],       // Am7 — live compose
        "S14" => &[29, 41, 48, 52, 57, 64],   // Fmaj7 — the tap's ripple
        "S15" => &[31, 43, 50, 55, 59, 64],   // G6 — say → rust
        "S16" => &[36, 48, 55, 60, 64, 67],   // C — the screens
        "S17" => &[28, 40, 47, 52, 55, 59],   // Em7 — the tokens, teal
        "S18" => &[29, 41, 48, 52, 57],       // F — the build, rising to G
        "S19" => &[33, 45, 52, 55, 60],       // Am7 — the mirror
        "S20" => &[36, 48, 55, 60, 64, 67],   // C — the shipped world
        "S21" => &[29, 41, 48, 52, 57, 60],   // Fmaj7 — the green ledger
        "S22" => &[33, 45, 52, 57, 60, 64],   // Am — the pull-back, the wide truth
        "S23" => &[36, 48, 55, 60, 64, 67, 72], // C, home — the end card
        "S24" => &[33, 45, 52, 60, 64],       // Am add9, fading — the loop
        _ => &[24, 36, 43, 48, 52, 55, 60, 64, 67], // C — home
    }
}

/// The movement a scene belongs to — the film's four acts.
fn act_of(id: &str) -> &'static str {
    match id {
        "S01" | "S02" | "S03" | "S04" => "I",
        "S05" | "S06" | "S07" | "S08" | "S09" | "S10" => "II",
        "S11" | "S12" | "S13" | "S14" | "S15" | "S16" | "S17" | "S18" | "S19" => "III",
        _ => "IV",
    }
}

/// Whether the pulse and plucks play under a scene. The wait (S01–S03)
/// is still — the 24 Hz world has no groove; the groove **is** K1, the
/// hard cut into the machine. The end card and the loop hold one bloom.
fn moving(id: &str) -> bool {
    matches!(
        id,
        "S05" | "S06" | "S07" | "S08" | "S09" | "S10" | // the machine
        "S11" | "S12" | "S13" | "S14" | "S15" | "S16" | "S17" | "S18" | "S19" | // the studio
        "S20" | "S21" | "S22" // the ship, until the end card
    )
}

/// The loudness arc per scene: the need is quiet, the machine opens up,
/// the studio sits back under its narration, the ship carries it home.
fn arc_of(id: &str) -> f32 {
    match id {
        "S01" => 0.30,
        "S02" => 0.45,
        "S03" => 0.55,
        "S04" => 0.70,
        "S18" => 0.80, // the cross-build, rising
        "S24" => 0.50, // the loop, fading
        _ => match act_of(id) {
            "I" => 0.55,
            "II" => 0.90,
            "III" => 0.65,
            _ => 0.90,
        },
    }
}

fn tone(note: f32, wave: Waveform, hold: f32, attack: f32, release: f32) -> Tone {
    Tone::held(note_to_hz(note), wave, ms(hold))
        .envelope(Envelope::attack_release(ms(attack), ms(release)))
}

/// Pad the buffer (mono) to `n` frames.
fn fit(mut s: Samples, n: usize) -> Vec<f32> {
    s.data.resize(n, 0.0);
    s.data
}

/// Render the whole score; returns interleaved stereo at [`RATE`].
pub(crate) fn render() -> Samples {
    let scenes = super::scenes();
    let total: f32 = scenes.iter().map(|s| s.seconds).sum();
    let n = (total * RATE as f32).ceil() as usize + RATE as usize;
    let starts: Vec<f32> = {
        let mut v = Vec::with_capacity(scenes.len());
        let mut acc = 0.0;
        for s in &scenes {
            v.push(acc);
            acc += s.seconds;
        }
        v
    };
    let start_of = |id: &str| {
        scenes
            .iter()
            .position(|s| s.id == id)
            .map(|i| starts[i])
            .unwrap_or(0.0)
    };

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
        let arc = arc_of(s.id);
        let v = 0.055 / notes.len() as f32 * 3.0 * arc;
        for (k, &m) in notes.iter().enumerate() {
            let m = f32::from(m);
            // The bass of the chord as a sine an octave down; the rest as
            // two slightly detuned triangles — width without noise.
            if k == 0 {
                pad.tone_at(
                    tone(m, Waveform::Sine, s.seconds, 1.4, 2.2),
                    ms(at),
                    0.16 * arc,
                );
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
            // Under the studio act the groove sits back (it is narration
            // now); the machine and the ship carry it full.
            let studio = act_of(s.id) == "III";
            let gain = if studio { 0.10 } else { 0.15 };
            let mut b = beat_clock;
            let mut step = 0usize;
            let upper: Vec<f32> = notes[1..].iter().map(|&m| f32::from(m) + 12.0).collect();
            const ARP: [usize; 8] = [0, 2, 1, 3, 2, 4, 3, 1];
            while b < end - 0.05 {
                pulse.tone_at(
                    tone(f32::from(notes[0]) + 12.0, Waveform::Sine, 0.04, 0.004, 0.32),
                    ms(b),
                    gain,
                );
                for half in 0..2 {
                    let t = b + half as f32 * BEAT * 0.5;
                    if t >= end - 0.05 {
                        break;
                    }
                    let m = upper[ARP[step % ARP.len()] % upper.len()];
                    let g = if studio { 0.030 } else { 0.042 }
                        * if step % 4 == 0 { 1.2 } else { 1.0 };
                    pluck.tone_at(tone(m, Waveform::Triangle, 0.01, 0.003, 0.42), ms(t), g);
                    step += 1;
                }
                b += BEAT;
            }
            beat_clock = b;
        }
    }

    // ── S18's build: F rising into G under the cross-build ─────────────
    let s18 = start_of("S18");
    for (k, &m) in [43.0_f32, 50.0, 55.0, 59.0].iter().enumerate() {
        let a = s18 + 5.0 + k as f32 * 0.45;
        pad.tone_at(tone(m, Waveform::Saw, 2.2, 1.8, 0.8), ms(a), 0.012);
    }

    // ── The lift into the end card: a swell that stops on the cut ──────
    let s23 = start_of("S23");
    for (k, &m) in [43.0_f32, 50.0, 55.0, 62.0, 67.0].iter().enumerate() {
        let a = s23 - 5.0 + k as f32 * 0.6;
        pad.tone_at(tone(m, Waveform::Saw, (s23 - a - 0.05).max(0.1), 4.6, 0.10), ms(a), 0.012);
    }

    // ── Hits: the acts, and the end card ───────────────────────────────
    // S05 — the machine (K1's landing); S11 — the studio; S20 — the ship;
    // S23 — the end card, the big one.
    for id in ["S05", "S11", "S20", "S23"] {
        let at = start_of(id);
        let big = id == "S23";
        hits.tone_at(
            tone(28.0, Waveform::Sine, 0.08, 0.005, if big { 2.4 } else { 1.1 }),
            ms(at),
            if big { 0.55 } else { 0.38 },
        );
        hits.tone_at(tone(96.0, Waveform::Sine, 0.02, 0.01, if big { 3.0 } else { 1.6 }), ms(at + 0.02), 0.035);
        hits.tone_at(tone(103.0, Waveform::Sine, 0.02, 0.01, if big { 3.4 } else { 1.8 }), ms(at + 0.09), 0.025);
    }

    // ── The film's own sounds, where the picture makes them ─────────────
    // S02 · the wait: the old world's clock, slow and low — five ticks
    // wearing down to the break.
    let s02 = start_of("S02");
    for k in 0..5u32 {
        let at = s02 + 0.6 + k as f32 * 2.4;
        ui.tone_at(tone(45.0, Waveform::Sine, 0.02, 0.002, 0.18), ms(at), 0.10);
        ui.tone_at(tone(81.0, Waveform::Square, 0.003, 0.001, 0.04), ms(at), 0.012);
    }
    // S03 · the break: the crack (t = 0.20), the shear (0.48), and the
    // flood of light (0.76) — three cues, read from the scene's own
    // phase constants.
    let s03 = start_of("S03");
    let crack = s03 + 0.20 * 7.0;
    ui.tone_at(tone(34.0, Waveform::Sine, 0.05, 0.002, 0.30), ms(crack), 0.30);
    ui.tone_at(tone(88.0, Waveform::Square, 0.006, 0.001, 0.09), ms(crack), 0.030);
    let shear = s03 + 0.48 * 7.0;
    ui.tone_at(tone(31.0, Waveform::Sine, 0.04, 0.002, 0.24), ms(shear), 0.20);
    let flood = s03 + 0.76 * 7.0;
    for (k, &m) in [84.0_f32, 91.0, 96.0].iter().enumerate() {
        ui.tone_at(tone(m, Waveform::Sine, 0.04, 0.010, 0.80), ms(flood + k as f32 * 0.07), 0.028);
    }
    // S04 · the spark: the bloom (t = 0.14) shimmers; the wordmark's five
    // letters land one per beat (t = 0.51 + 0.03·i) — a rising motif,
    // the pentatonic the crate pops will climb again in S06.
    let s04 = start_of("S04");
    let bloom = s04 + 0.14 * 11.0;
    ui.tone_at(tone(96.0, Waveform::Sine, 0.03, 0.012, 1.20), ms(bloom), 0.030);
    for (i, &m) in [69.0_f32, 72.0, 76.0, 79.0, 83.0].iter().enumerate() {
        let land = s04 + (0.51 + 0.03 * i as f32) * 11.0;
        ui.tone_at(tone(m, Waveform::Triangle, 0.02, 0.004, 0.60), ms(land), 0.055);
    }
    // S06 · the crates: twelve of the thirty-six pops, rising — the city
    // building itself (the count window runs t = 0.10 → 0.65).
    let s06 = start_of("S06");
    const PENTA: [f32; 5] = [0.0, 2.0, 4.0, 7.0, 9.0];
    for k in 0..12usize {
        let land = s06 + 1.0 + k as f32 * (5.5 / 12.0);
        let m = 72.0 + PENTA[k % 5] + 12.0 * (k / 5) as f32;
        ui.tone_at(tone(m, Waveform::Sine, 0.01, 0.002, 0.5), ms(land), 0.05);
    }
    // S10 · the cadence: the 60 fps claim lands at the scene's head; the
    // release into the studio act shimmers at its tail.
    let s10 = start_of("S10");
    ui.tone_at(tone(41.0, Waveform::Sine, 0.03, 0.003, 0.20), ms(s10), 0.16);
    for &m in [88.0_f32, 95.0].iter() {
        ui.tone_at(tone(m, Waveform::Sine, 0.02, 0.010, 0.90), ms(s10 + 4.6), 0.024);
    }
    // S12 · the alive keystroke (EDIT_T = 0.78): one keystroke through
    // the real studio, answered by a bloom.
    let s12 = start_of("S12");
    let keystroke = s12 + 0.78 * 12.0;
    ui.tone_at(tone(91.0, Waveform::Sine, 0.004, 0.001, 0.05), ms(keystroke), 0.05);
    ui.tone_at(tone(76.0, Waveform::Triangle, 0.03, 0.006, 0.70), ms(keystroke + 0.05), 0.040);
    // S14 · the tap: the ripple needs its thump (the script's PointerDown).
    let tap = 124.38;
    ui.tone_at(tone(38.0, Waveform::Sine, 0.06, 0.003, 0.34), ms(tap), 0.26);
    ui.tone_at(tone(93.0, Waveform::Sine, 0.02, 0.008, 0.60), ms(tap + 0.03), 0.030);
    // The compiles: a rising swell while `rustc` runs, a click when it
    // settles — the real Render/Settle pairs, from the session script.
    for &(fire, settle) in &[(134.2_f32, 135.0), (138.1, 139.0), (142.0, 142.8)] {
        for (k, &m) in [55.0_f32, 62.0, 67.0].iter().enumerate() {
            ui.tone_at(
                tone(m, Waveform::Saw, 0.70, 0.30 + k as f32 * 0.10, 0.10),
                ms(fire + k as f32 * 0.12),
                0.016,
            );
        }
        ui.tone_at(tone(84.0, Waveform::Sine, 0.004, 0.001, 0.06), ms(settle), 0.035);
    }
    // S18 · the export: the build fires long (161.3) and the package
    // lands (166.8) with a thunk and a shimmer.
    for (k, &m) in [48.0_f32, 55.0, 60.0, 64.0].iter().enumerate() {
        ui.tone_at(
            tone(m, Waveform::Saw, 1.30, 0.55 + k as f32 * 0.14, 0.14),
            ms(161.3 + k as f32 * 0.22),
            0.016,
        );
    }
    ui.tone_at(tone(36.0, Waveform::Sine, 0.07, 0.004, 0.40), ms(166.8), 0.28);
    ui.tone_at(tone(100.0, Waveform::Sine, 0.02, 0.010, 1.10), ms(166.85), 0.028);
    // S21 · the ledger's stamp (t = 0.74): zero open regressions, thumped.
    let s21 = start_of("S21");
    let stamp = s21 + 0.74 * 9.0;
    ui.tone_at(tone(40.0, Waveform::Sine, 0.05, 0.003, 0.28), ms(stamp), 0.24);
    // S23 · the end card's underline (t = 0.56): the last chime.
    let chime = s23 + 0.56 * 11.0;
    for (k, &m) in [96.0_f32, 100.0, 103.0].iter().enumerate() {
        ui.tone_at(tone(m, Waveform::Sine, 0.03, 0.010, 1.40), ms(chime + k as f32 * 0.05), 0.024);
    }
    // The studio: a soft click on every action the session takes — the
    // session's own percussive track, straight from the script's times.
    for (at, _) in super::script::script() {
        ui.tone_at(tone(91.0, Waveform::Sine, 0.004, 0.001, 0.05), ms(at), 0.022);
        ui.tone_at(tone(79.0, Waveform::Triangle, 0.004, 0.001, 0.08), ms(at), 0.015);
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
    // Normalise to −1 dBFS, and fade the last two seconds of the film —
    // the loop's last breath, back to the caret.
    let peak = l.iter().chain(r.iter()).fold(0.0_f32, |m, x| m.max(x.abs())).max(1e-6);
    let gain = 0.89 / peak;
    let film_n = (total * RATE as f32) as usize;
    let fade = (2.0 * RATE as f32) as usize;
    let mut out = Vec::with_capacity(film_n * 2);
    for i in 0..film_n {
        let f = if i + fade > film_n {
            (film_n - i) as f32 / fade as f32
        } else {
            1.0
        };
        out.push(l[i] * gain * f);
        out.push(r[i] * gain * f);
    }
    Samples::stereo(out, RATE)
}

/// Render the score and write it as `score.wav` under `root`.
pub(crate) fn write(root: &Path) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let samples = render();
    let bytes = vieww_audio::wav::write_wav(&samples).map_err(|e| format!("{e:?}"))?;
    let path = root.join("score.wav");
    std::fs::write(&path, bytes)?;
    Ok(path)
}
