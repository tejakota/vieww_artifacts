//! MIDI, audio DSP and OSC — the show-control and sound-reactive plumbing
//! of TouchDesigner, openFrameworks, Processing and Web Audio,
//! photographed.
//!
//! 1. A Standard MIDI File built in code, written to bytes and parsed back
//!    (`Smf`), shown as a piano roll with a tempo change halfway; the
//!    playhead converts ticks to seconds through the tempo map.
//! 2. The same notes synthesised, run through a swept low-pass `Biquad`
//!    and a `Compressor`; spectral-flux `OnsetDetector` marks and
//!    `estimate_tempo` give the beat.
//! 3. Magnitude responses of the RBJ filter family on a log axis.
//! 4. A live MIDI byte stream with running status fed to `Parser` and
//!    folded into `MidiState` — held keys, a mod-wheel CC, pitch bend.
//! 5. OSC over real UDP on localhost: bundles of `/mixer/fader/N` sent each
//!    frame, received, and dispatched by address pattern through a `Router`.

use std::cell::RefCell;
use std::rc::Rc;

use feature_harness::draw::{grid, page, painted, plot, xywh, DIM, HUES, INK};
use vieww_audio::dsp::{apply, estimate_tempo, Biquad, Chain, Compressor, FilterKind, OnsetDetector, Processor};
use vieww_audio::midi::{note_name, note_to_hz, Message, MidiState, Parser, Smf, TrackEvent};
use vieww_audio::{Samples, Waveform};
use vieww_foundation::{Offset, Size};
use vieww_network::osc::{Arg, Bundle, Message as OscMessage, OscSocket, Packet, Router, IMMEDIATELY};

const SPAN: f32 = 5.0;
const RATE: u32 = 22_050;

fn song() -> Smf {
    let mut f = Smf::new(480);
    let mut tr = vec![(0, TrackEvent::TrackName("lead".into())), (0, TrackEvent::Tempo(500_000)), (3840, TrackEvent::Tempo(375_000))];
    let melody = [60u8, 64, 67, 72, 71, 67, 64, 62, 60, 64, 67, 69, 67, 65, 64, 60];
    for (i, n) in melody.iter().enumerate() {
        let at = i as u64 * 480;
        tr.push((at, TrackEvent::Midi(Message::NoteOn { channel: 0, key: *n, velocity: 90 })));
        tr.push((at + 400, TrackEvent::Midi(Message::NoteOff { channel: 0, key: *n, velocity: 0 })));
        if i % 4 == 0 {
            tr.push((at, TrackEvent::Midi(Message::NoteOn { channel: 0, key: n - 24, velocity: 70 })));
            tr.push((at + 1800, TrackEvent::Midi(Message::NoteOff { channel: 0, key: n - 24, velocity: 0 })));
        }
    }
    f.tracks.push(tr);
    f
}

fn render(notes: &[(f64, f64, u8, u8, u8)], seconds: f32) -> Samples {
    let n = (seconds * RATE as f32) as usize;
    let mut data = vec![0.0f32; n];
    for &(start, dur, _, key, vel) in notes {
        let hz = note_to_hz(f32::from(key));
        let (s0, len) = ((start * f64::from(RATE)) as usize, (dur * f64::from(RATE)) as usize);
        for k in 0..len.min(n.saturating_sub(s0)) {
            let t = k as f32 / RATE as f32;
            let env = (1.0 - (-t * 200.0).exp()) * (-t * 3.0).exp();
            data[s0 + k] += Waveform::Saw.at(hz * t) * env * f32::from(vel) / 127.0 * 0.35;
        }
    }
    Samples::mono(data, RATE)
}

struct Sweep {
    k: usize,
    f: Biquad,
}

impl Processor for Sweep {
    fn process(&mut self, x: f32) -> f32 {
        if self.k % 256 == 0 {
            let t = self.k as f32 / RATE as f32;
            let cutoff = 300.0 + 3500.0 * (0.5 + 0.5 * (t * 1.3).sin());
            // New coefficients every 256 samples (control rate).
            self.f = Biquad::new(FilterKind::LowPass, cutoff, 2.0, RATE as f32);
        }
        self.k += 1;
        self.f.process(x)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = song().to_bytes();
    let smf = Smf::parse(&bytes)?;
    let notes = smf.notes();
    let length = notes.iter().map(|n| n.0 + n.1).fold(0.0, f64::max) as f32;
    let dry = render(&notes, length + 0.2);
    let wet = apply(&dry, || Chain::new().then(Sweep { k: 0, f: Biquad::new(FilterKind::LowPass, 1000.0, 2.0, RATE as f32) }).then(Compressor::new(-18.0, 4.0, 0.005, 0.1, RATE as f32)));
    let onsets = OnsetDetector { multiplier: 2.0, window: 16, delta: 0.08, ..OnsetDetector::default() }.detect(&dry);
    let bpm = estimate_tempo(&onsets, 100.0, 180.0).unwrap_or(0.0);
    let env = |s: &Samples| -> Vec<f32> { s.data.chunks(RATE as usize / 100).map(|c| c.iter().fold(0.0f32, |m, v| m.max(v.abs()))).collect() };
    let (dry_env, wet_env) = (env(&dry), env(&wet));

    let rx = OscSocket::bind("127.0.0.1:0")?;
    let tx = OscSocket::bind("127.0.0.1:0")?;
    let to = rx.local_addr()?;

    feature_harness::launch("90 — midi, dsp, osc", Size::new(1000.0, 560.0), move |d| {
        let notes = notes.clone();
        let smf = smf.clone();
        let onsets = onsets.clone();
        let (dry_env, wet_env) = (dry_env.clone(), wet_env.clone());
        let faders = Rc::new(RefCell::new([0.0f32; 4]));
        let mut router = Router::new();
        for i in 0..4 {
            let f = faders.clone();
            router.on(&format!("/mixer/fader/{}", i + 1), move |m| f.borrow_mut()[i] = m.args.first().and_then(Arg::as_f32).unwrap_or(0.0));
        }
        let router = Rc::new(RefCell::new(router));
        let received = Rc::new(RefCell::new(0usize));
        let (tx, rx) = (Rc::new(tx), Rc::new(rx));
        let bytes_len = bytes.len();
        let view = feature_harness::clocked(d, SPAN, move |t| {
            let song_t = t / SPAN * length;
            let (lo, hi) = (36.0, 76.0);
            let tempo = smf.tempo_map();
            let notes1 = notes.clone();
            let p1 = painted("SMF piano roll", &format!("{bytes_len} bytes · {} notes · tempo map {:?}", notes.len(), tempo.iter().map(|x| 60_000_000 / x.1).collect::<Vec<_>>()), Size::new(300.0, 170.0), move |g, _| {
                for &(s, dur, _, key, _) in &notes1 {
                    let x = s as f32 / length * 290.0 + 5.0;
                    let y = 160.0 - (f32::from(key) - lo) / (hi - lo) * 150.0;
                    let on = song_t as f64 >= s && (song_t as f64) < s + dur;
                    g.rrect(xywh(x, y - 3.0, (dur as f32 / length * 290.0).max(2.0), 6.0), 2.0, if on { HUES[3] } else { HUES[0] });
                }
                let px = 5.0 + song_t / length * 290.0;
                g.line(Offset::new(px, 0.0), Offset::new(px, 170.0), INK, 1.0);
            });
            let on2 = onsets.clone();
            let (de, we) = (dry_env.clone(), wet_env.clone());
            let p2 = painted("synth → swept low-pass → compressor", &format!("{} onsets · tempo ≈ {bpm:.0} bpm", onsets.len()), Size::new(300.0, 170.0), move |g, _| {
                let n = de.len() as f32;
                g.stroke(plot(&de, Offset::new(5.0, 5.0), Size::new(290.0, 75.0), 0.0, 1.0), DIM, 1.0);
                g.stroke(plot(&we, Offset::new(5.0, 90.0), Size::new(290.0, 75.0), 0.0, 1.0), HUES[2], 1.2);
                for o in &on2 {
                    let x = 5.0 + o * 100.0 / n * 290.0;
                    g.line(Offset::new(x, 0.0), Offset::new(x, 82.0), HUES[1].with_alpha(160), 1.0);
                }
                let px = 5.0 + song_t * 100.0 / n * 290.0;
                g.line(Offset::new(px, 0.0), Offset::new(px, 170.0), INK, 1.0);
            });
            let gain_db = 6.0 * (t * 1.5).sin();
            let p3 = painted("biquad responses", &format!("LP · HP · BP · notch · peak {gain_db:+.1} dB · shelves"), Size::new(300.0, 170.0), move |g, _| {
                let rate = 48_000.0;
                let kinds = [
                    FilterKind::LowPass,
                    FilterKind::HighPass,
                    FilterKind::BandPass,
                    FilterKind::Notch,
                    FilterKind::Peaking(gain_db),
                    FilterKind::LowShelf(-8.0),
                ];
                for (i, k) in kinds.iter().enumerate() {
                    let f = Biquad::new(*k, 1000.0, 1.2, rate);
                    let vals: Vec<f32> = (0..=150).map(|j| (20.0 * f.magnitude(20.0 * 1000f32.powf(j as f32 / 150.0), rate).max(1e-4).log10()).clamp(-30.0, 10.0)).collect();
                    g.stroke(plot(&vals, Offset::new(5.0, 5.0), Size::new(290.0, 160.0), -30.0, 10.0), HUES[i], 1.5);
                }
                let y0 = 5.0 + 160.0 * (10.0 / 40.0);
                g.line(Offset::new(5.0, y0), Offset::new(295.0, y0), DIM, 1.0);
            });

            // Live MIDI: a byte stream with running status, up to now.
            let mut stream = Vec::new();
            let step = (t * 8.0) as usize;
            for k in 0..=step {
                let key = [60u8, 64, 67, 71, 72, 67, 64, 62][k % 8];
                if k > 0 {
                    stream.extend([0x80, [60u8, 64, 67, 71, 72, 67, 64, 62][(k - 1) % 8], 0]);
                }
                stream.extend([0x90, key, 100, key - 12, 80]); // running status: two notes, one status byte
                if k > 0 {
                    stream.extend([0x80, [60u8, 64, 67, 71, 72, 67, 64, 62][(k - 1) % 8] - 12, 0]);
                }
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let cc = ((t / SPAN) * 127.0) as u8;
            let bend = ((t * 2.0).sin() * 8000.0) as i32 + 8192;
            stream.extend([0xb0, 1, cc, 0xe0, (bend & 0x7f) as u8, ((bend >> 7) & 0x7f) as u8, 0xf8]);
            let mut state = MidiState::default();
            let msgs = Parser::new().feed(&stream);
            for m in &msgs {
                state.apply(m);
            }
            let held: Vec<String> = state.notes.keys().map(|(_, k)| note_name(*k)).collect();
            let st2 = state.clone();
            let p4 = painted("MIDI parser + state", &format!("{} bytes → {} msgs · held {:?} · CC1 {:.2}", stream.len(), msgs.len(), held, state.cc01(0, 1)), Size::new(300.0, 170.0), move |g, _| {
                for k in 0..25u8 {
                    let key = 48 + k;
                    let x = 5.0 + f32::from(k) * 11.6;
                    let black = matches!(key % 12, 1 | 3 | 6 | 8 | 10);
                    let held = st2.notes.contains_key(&(0, key));
                    let c = if held { HUES[3] } else if black { vieww_foundation::Color::rgb(30, 32, 40) } else { vieww_foundation::Color::rgb(220, 224, 232) };
                    g.rrect(xywh(x, if black { 10.0 } else { 10.0 }, 10.6, if black { 60.0 } else { 90.0 }), 2.0, c);
                }
                let cc = st2.cc01(0, 1);
                g.rrect(xywh(5.0, 115.0, 290.0, 10.0), 5.0, DIM.with_alpha(60));
                g.rrect(xywh(5.0, 115.0, 290.0 * cc, 10.0), 5.0, HUES[0]);
                let b = st2.bend[0];
                g.rrect(xywh(150.0 + (140.0 * b).min(0.0), 140.0, 140.0 * b.abs(), 10.0), 5.0, HUES[4]);
                g.line(Offset::new(150.0, 136.0), Offset::new(150.0, 154.0), INK, 1.0);
            });

            // OSC: send a bundle over UDP, drain, dispatch.
            let vals: Vec<f32> = (0..4).map(|i| 0.5 + 0.5 * (t * (1.0 + i as f32 * 0.6)).sin()).collect();
            let bundle = Packet::Bundle(Bundle { time: IMMEDIATELY, content: vals.iter().enumerate().map(|(i, v)| Packet::Message(OscMessage::new(&format!("/mixer/fader/{}", i + 1), vec![Arg::Float(*v)]))).collect() });
            let _ = tx.send_to(&bundle, to);
            std::thread::sleep(std::time::Duration::from_millis(2));
            if let Ok(pk) = rx.poll() {
                for p in &pk {
                    *received.borrow_mut() += router.borrow_mut().dispatch(p);
                }
            }
            let fv = *faders.borrow();
            let wire = bundle.encode().len();
            let p5 = painted("OSC over UDP", &format!("bundle {wire} B · {} messages dispatched via /mixer/fader/*", received.borrow()), Size::new(300.0, 170.0), move |g, _| {
                for (i, v) in fv.iter().enumerate() {
                    let x = 30.0 + i as f32 * 65.0;
                    g.rrect(xywh(x, 10.0, 22.0, 150.0), 6.0, DIM.with_alpha(50));
                    g.rrect(xywh(x, 10.0 + 150.0 * (1.0 - v), 22.0, 150.0 * v), 6.0, HUES[i]);
                }
            });
            page("90 · MIDI, DSP and OSC", "vieww-audio (midi, dsp) · vieww-network (osc)", grid(3, vec![p1, p2, p3, p4, p5]))
        });
        feature_harness::set_page(d, view);
    })
}
