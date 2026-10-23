//! Device loopback: play a known signal, capture it back, prove it intact.
//!
//! The device tests are `#[ignore]`d — they need a loopback that the CI job
//! (or a person) provides, named in the environment:
//!
//! | Test | Needs | Set by |
//! |---|---|---|
//! | `alsa_aloop_*` | the `snd-aloop` kernel module (a card named `Loopback`) | `audio-devices.yml` Linux job, or `sudo modprobe snd-aloop` |
//! | `midi_port_round_trip` | `VIEWW_MIDI_LOOP=<out id>><in id>`: two ports wired together | the Linux job (`snd-virmidi` + `aconnect`); loopMIDI on Windows by hand |
//! | `default_device_loopback` | `VIEWW_LOOP_BACKEND=<backend>`: default output wired to default input | the Windows (VB-CABLE) and macOS (BlackHole) jobs; a cable on real hardware |
//! | `coremidi_virtual_round_trip` | macOS | the macOS job |
//! | `default_output_soak` | any output; `VIEWW_SOAK_BACKEND` (else the first that opens), `VIEWW_SOAK_SECS` (default 10), `VIEWW_LIVE_GAIN` (default 0: silent but fully streamed), `VIEWW_MAX_XRUNS` | `ci/hardware` `audio` step on real sound cards |
//!
//! With `VIEWW_REQUIRE_LOOPBACK=1` a missing loopback fails the test
//! instead of skipping it — the jobs set it, so a job that silently lost
//! its device cannot pass.
//!
//! What "intact" means: each channel carries a sine at its own frequency
//! (997 Hz left, 1499 Hz right — prime, so no window aligns with a cycle).
//! The capture is cut into 10 ms windows and each is least-squares fitted
//! against that frequency **referenced to the capture's own sample
//! clock**: amplitude, phase and residual. A clean loop gives one
//! amplitude, one phase and a residual at the format's noise floor. A
//! dropped or repeated frame moves the phase of every later window (0.13
//! rad per frame at 48 kHz); a gap or click drops a window's amplitude or
//! raises its residual. The analyzer's own tests below show it catching
//! each.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "signal analysis"
)]

use std::f64::consts::TAU;
use std::time::Duration;

const LEFT_HZ: f64 = 997.0;
const RIGHT_HZ: f64 = 1_499.0;
const LEVEL: f32 = 0.5; // −6 dBFS

fn required() -> bool {
    std::env::var("VIEWW_REQUIRE_LOOPBACK").is_ok_and(|v| v == "1")
}

/// Skip (or, when required, fail) with a reason.
fn unavailable(why: &str) {
    assert!(!required(), "loopback required but unavailable: {why}");
    eprintln!("skipped: {why}");
}

/// The test signal: interleaved stereo, frames `[start, start + n)`.
fn signal(rate: u32, start: u64, n: usize, out: &mut Vec<f32>) {
    let r = f64::from(rate);
    for i in 0..n as u64 {
        let t = (start + i) as f64 / r;
        out.push(((TAU * LEFT_HZ * t).sin() as f32) * LEVEL);
        out.push(((TAU * RIGHT_HZ * t).sin() as f32) * LEVEL);
    }
}

/// One window's fit.
#[derive(Debug, Clone, Copy)]
struct Fit {
    amp: f64,
    phase: f64,
    signal: f64,
    residual: f64,
}

/// Least-squares fit of `x[i] ≈ a·sin(ω(i0+i)) + b·cos(ω(i0+i))`.
fn fit(x: &[f32], i0: usize, hz: f64, rate: f64) -> Fit {
    let w = TAU * hz / rate;
    let (mut ss, mut cc, mut sc, mut xs, mut xc) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (k, &v) in x.iter().enumerate() {
        let p = w * (i0 + k) as f64;
        let (s, c) = p.sin_cos();
        let v = f64::from(v);
        ss += s * s;
        cc += c * c;
        sc += s * c;
        xs += v * s;
        xc += v * c;
    }
    let det = ss * cc - sc * sc;
    let a = (xs * cc - xc * sc) / det;
    let b = (xc * ss - xs * sc) / det;
    let (mut sig, mut res) = (0.0, 0.0);
    for (k, &v) in x.iter().enumerate() {
        let p = w * (i0 + k) as f64;
        let m = a * p.sin() + b * p.cos();
        sig += m * m;
        res += (f64::from(v) - m).powi(2);
    }
    Fit {
        amp: a.hypot(b),
        phase: b.atan2(a),
        signal: sig,
        residual: res,
    }
}

/// What the capture of one channel looked like.
#[derive(Debug)]
struct Report {
    windows: usize,
    snr_db: f64,
    amp_spread_db: f64,
    phase_spread: f64,
    worst_window_snr_db: f64,
}

impl Report {
    fn check(&self, what: &str, min_snr_db: f64) {
        eprintln!("{what}: {self:?}");
        assert!(
            self.windows >= 50,
            "{what}: only {} windows of signal",
            self.windows
        );
        assert!(
            self.snr_db >= min_snr_db,
            "{what}: SNR {:.1} dB < {min_snr_db}",
            self.snr_db
        );
        assert!(
            self.worst_window_snr_db >= (min_snr_db - 20.0).min(40.0),
            "{what}: a window at {:.1} dB — a click or a gap",
            self.worst_window_snr_db
        );
        assert!(
            self.amp_spread_db < 0.5,
            "{what}: level moved {:.2} dB",
            self.amp_spread_db
        );
        assert!(
            self.phase_spread < 0.05,
            "{what}: phase moved {:.3} rad — frames were dropped or repeated",
            self.phase_spread
        );
    }
}

/// Analyze channel `ch` of interleaved `data` (`channels` wide) for a sine
/// at `hz`: from 50 ms after it starts until it stops.
fn analyze(data: &[f32], channels: usize, ch: usize, hz: f64, rate: u32) -> Report {
    let x: Vec<f32> = data.iter().skip(ch).step_by(channels).copied().collect();
    let r = f64::from(rate);
    let win = (r / 100.0) as usize;
    // Onset: the first 1 ms block above a tenth of the expected level.
    let block = (r / 1_000.0) as usize;
    let onset = x
        .chunks(block)
        .position(|b| b.iter().map(|v| v * v).sum::<f32>() / b.len() as f32 > (LEVEL * 0.1).powi(2))
        .map_or(x.len(), |i| i * block);
    let mut fits = Vec::new();
    let mut at = onset + 5 * win;
    while at + win <= x.len() {
        let f = fit(&x[at..at + win], at, hz, r);
        if f.amp < f64::from(LEVEL) * 0.5 && !fits.is_empty() {
            // Either the end of the signal, or a gap: a gap has signal
            // again after it.
            let more = x[at..]
                .chunks(win)
                .skip(2)
                .any(|c| fit(c, 0, hz, r).amp > f64::from(LEVEL) * 0.5);
            if !more {
                break;
            }
        }
        fits.push(f);
        at += win;
    }
    // Leave the last two windows out: the signal's own fade or cut.
    fits.truncate(fits.len().saturating_sub(2));
    if fits.is_empty() {
        return Report {
            windows: 0,
            snr_db: 0.0,
            amp_spread_db: f64::INFINITY,
            phase_spread: f64::INFINITY,
            worst_window_snr_db: 0.0,
        };
    }
    let db = |a: f64| 20.0 * a.max(1e-12).log10();
    let (sig, res) = fits
        .iter()
        .fold((0.0, 0.0), |(s, r), f| (s + f.signal, r + f.residual));
    let amps: Vec<f64> = fits.iter().map(|f| db(f.amp)).collect();
    let p0 = fits[0].phase;
    let dev: Vec<f64> = fits
        .iter()
        .map(|f| {
            let d = f.phase - p0;
            (d + std::f64::consts::PI).rem_euclid(TAU) - std::f64::consts::PI
        })
        .collect();
    let spread = |v: &[f64]| {
        v.iter().copied().fold(f64::MIN, f64::max) - v.iter().copied().fold(f64::MAX, f64::min)
    };
    Report {
        windows: fits.len(),
        snr_db: 10.0 * (sig / res.max(1e-300)).log10(),
        amp_spread_db: spread(&amps),
        phase_spread: spread(&dev),
        worst_window_snr_db: fits
            .iter()
            .map(|f| 10.0 * (f.signal / f.residual.max(1e-300)).log10())
            .fold(f64::MAX, f64::min),
    }
}

/// Both channels, each at its own frequency, and no crosstalk swap.
fn check_stereo(what: &str, data: &[f32], rate: u32, min_snr_db: f64) {
    analyze(data, 2, 0, LEFT_HZ, rate).check(&format!("{what} left"), min_snr_db);
    analyze(data, 2, 1, RIGHT_HZ, rate).check(&format!("{what} right"), min_snr_db);
}

// ---- the analyzer, tested (always runs) ---------------------------------

fn synthetic(rate: u32, lead: usize, frames: usize) -> Vec<f32> {
    let mut v = vec![0.0; lead * 2];
    signal(rate, 0, frames, &mut v);
    v.extend(std::iter::repeat_n(0.0, 4_800 * 2));
    v
}

#[test]
fn the_analyzer_passes_a_clean_loop() {
    let v = synthetic(48_000, 3_000, 48_000);
    check_stereo("clean", &v, 48_000, 120.0);
}

#[test]
fn the_analyzer_catches_a_dropped_frame() {
    let mut v = synthetic(48_000, 3_000, 48_000);
    let at = (3_000 + 24_000) * 2;
    v.drain(at..at + 2);
    let r = analyze(&v, 2, 0, LEFT_HZ, 48_000);
    assert!(r.phase_spread > 0.1, "{r:?}");
}

#[test]
fn the_analyzer_catches_a_gap_and_a_click() {
    let mut v = synthetic(48_000, 3_000, 48_000);
    let at = (3_000 + 20_000) * 2;
    for s in &mut v[at..at + 480] {
        *s = 0.0;
    }
    let r = analyze(&v, 2, 0, LEFT_HZ, 48_000);
    assert!(
        r.worst_window_snr_db < 20.0 || r.amp_spread_db > 1.0,
        "gap: {r:?}"
    );
    let mut v = synthetic(48_000, 3_000, 48_000);
    v[(3_000 + 30_000) * 2] = 1.0;
    let r = analyze(&v, 2, 0, LEFT_HZ, 48_000);
    assert!(r.worst_window_snr_db < 40.0, "click: {r:?}");
}

#[test]
fn the_analyzer_reads_quantisation_noise() {
    // 16-bit with TPDF dither: ≈ 80 dB at −6 dBFS. The ALSA S16 threshold
    // sits below that.
    let v: Vec<f32> = synthetic(48_000, 3_000, 48_000)
        .into_iter()
        .map(|s| (s * 32_767.0).round() / 32_767.0)
        .collect();
    let r = analyze(&v, 2, 0, LEFT_HZ, 48_000);
    assert!(r.snr_db > 80.0 && r.snr_db < 110.0, "{r:?}");
}

// ---- ALSA through snd-aloop ------------------------------------------------

#[cfg(all(target_os = "linux", not(target_os = "android")))]
mod alsa {
    use super::*;
    use std::sync::mpsc;
    use vieww_audio::backend::{alsa::Alsa, Backend, BlockInfo, StreamConfig};
    use vieww_audio::device::{AlsaInput, AlsaOutput, PcmFormat};

    /// The `snd-aloop` card's number: playback on device 0 comes back as
    /// capture on device 1 (same subdevice).
    fn loopback_card() -> Option<u32> {
        let cards = std::fs::read_to_string("/proc/asound/cards").ok()?;
        cards.lines().find_map(|l| {
            let (n, rest) = l.trim_start().split_once(' ')?;
            rest.contains("[Loopback").then(|| n.parse().ok()).flatten()
        })
    }

    /// Run `f` on a thread; fail if it does not finish in `secs`.
    fn within<T: Send + 'static>(
        secs: u64,
        what: &str,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(Duration::from_secs(secs))
            .unwrap_or_else(|_| panic!("{what}: no result in {secs} s — the device stalled"))
    }

    /// Lowest SNR a format must reach at −6 dBFS: its word length's noise
    /// floor, less margin for dither (16 and 24 bits) — bits × 6 − 25 dB.
    fn min_snr(f: PcmFormat) -> f64 {
        (f64::from(f.bits()) * 6.02 - 25.0).min(120.0)
    }

    #[test]
    #[ignore = "needs snd-aloop (see the module docs)"]
    fn alsa_aloop_every_format_round_trips() {
        let Some(card) = loopback_card() else {
            return unavailable("no snd-aloop card");
        };
        let rate = 48_000;
        let mut tried = 0;
        for fmt in PcmFormat::PREFERENCE {
            // Capture first: aloop then holds playback to its parameters.
            let Ok(mut input) = AlsaInput::open_with_formats(card, 1, rate, 2, 256, &[fmt]) else {
                eprintln!("{fmt:?}: the loopback does not take it — skipped");
                continue;
            };
            let mut output = AlsaOutput::open_with_formats(card, 0, rate, 2, 256, &[fmt])
                .unwrap_or_else(|e| panic!("{fmt:?}: capture opened but playback refused: {e}"));
            assert_eq!(input.format(), fmt);
            assert_eq!(output.format(), fmt);
            tried += 1;
            let total = rate as usize * 2; // 2 s captured
            let reader = std::thread::spawn(move || {
                let mut got = vec![0.0f32; total * 2];
                for chunk in got.chunks_mut(512) {
                    input.read_f32(chunk).expect("capture");
                }
                got
            });
            within(20, "playback", move || {
                let mut buf = Vec::new();
                for k in 0..(rate as u64 * 3 / 2 / 256) {
                    buf.clear();
                    signal(rate, k * 256, 256, &mut buf);
                    output.write_f32(&buf).expect("playback");
                }
                output.write_f32(&vec![0.0; rate as usize]).ok(); // let capture finish
            });
            let got = within(20, "capture", move || reader.join().expect("reader"));
            check_stereo(&format!("aloop {fmt:?}"), &got, rate, min_snr(fmt));
        }
        assert!(tried >= 2, "the loopback took only {tried} formats");
    }

    #[test]
    #[ignore = "needs snd-aloop (see the module docs)"]
    fn alsa_aloop_backend_output_matches_a_rate_the_device_refuses() {
        let Some(card) = loopback_card() else {
            return unavailable("no snd-aloop card");
        };
        // Capture holds the cable at 48 kHz; the backend asks for 44.1 kHz,
        // is refused it, opens at 48 kHz and resamples.
        let mut input = AlsaInput::open_with_formats(card, 1, 48_000, 2, 256, &[PcmFormat::F32])
            .expect("capture side");
        let mut n = 0u64;
        let stream = Alsa { card, device: 0 }
            .open_output(
                StreamConfig {
                    rate: 44_100,
                    channels: 2,
                    block: 256,
                },
                Box::new(move |out: &mut [f32], info: &BlockInfo| {
                    let mut v = Vec::with_capacity(out.len());
                    signal(info.rate, n, info.frames, &mut v);
                    out.copy_from_slice(&v);
                    n += info.frames as u64;
                }),
            )
            .expect("the backend opens at the device's rate");
        let got = within(20, "capture", move || {
            let mut got = vec![0.0f32; 48_000 * 2 * 2];
            for chunk in got.chunks_mut(512) {
                input.read_f32(chunk).expect("capture");
            }
            got
        });
        let st = stream.stats();
        drop(stream);
        assert_eq!(st.xruns, 0);
        // Sinc-16 conversion: well above 16-bit quality.
        check_stereo("backend 44.1 → device 48 kHz", &got, 48_000, 75.0);
    }

    #[test]
    #[ignore = "needs snd-aloop (see the module docs)"]
    fn alsa_aloop_backend_capture_matches_a_rate_the_device_refuses() {
        let Some(card) = loopback_card() else {
            return unavailable("no snd-aloop card");
        };
        let mut output = AlsaOutput::open_with_formats(card, 0, 48_000, 2, 256, &[PcmFormat::F32])
            .expect("playback side");
        let got = std::sync::Arc::new(std::sync::Mutex::new(Vec::<f32>::new()));
        let g2 = got.clone();
        let stream = Alsa { card, device: 1 }
            .open_input(
                StreamConfig {
                    rate: 44_100,
                    channels: 2,
                    block: 256,
                },
                Box::new(move |input: &[f32], info: &BlockInfo| {
                    assert_eq!(info.rate, 44_100, "the closure sees the rate it asked for");
                    g2.lock().unwrap().extend_from_slice(input);
                }),
            )
            .expect("the backend opens at the device's rate");
        within(20, "playback", move || {
            let mut buf = Vec::new();
            for k in 0..(48_000u64 * 5 / 2 / 256) {
                buf.clear();
                signal(48_000, k * 256, 256, &mut buf);
                output.write_f32(&buf).expect("playback");
            }
        });
        drop(stream);
        let got = got.lock().unwrap().clone();
        check_stereo("device 48 → backend 44.1 kHz", &got, 44_100, 75.0);
    }

    #[test]
    #[ignore = "needs snd-aloop (see the module docs)"]
    fn alsa_lists_the_loopback() {
        let Some(card) = loopback_card() else {
            return unavailable("no snd-aloop card");
        };
        let devices = Alsa::default().devices().expect("list");
        assert!(
            devices.iter().any(|d| d.id == format!("hw:{card},0")),
            "{devices:?}"
        );
    }
}

// ---- MIDI ports wired together -------------------------------------------------

#[test]
#[ignore = "needs VIEWW_MIDI_LOOP=<out>><in> (see the module docs)"]
fn midi_port_round_trip() {
    use vieww_audio::midi::Message;
    use vieww_audio::midi_io::{input_ports, output_ports, MidiInput, MidiOutput};
    let Some((out_id, in_id)) = std::env::var("VIEWW_MIDI_LOOP")
        .ok()
        .and_then(|v| v.split_once('>').map(|(a, b)| (a.to_owned(), b.to_owned())))
    else {
        return unavailable("VIEWW_MIDI_LOOP not set");
    };
    eprintln!("inputs: {:?}\noutputs: {:?}", input_ports(), output_ports());
    let input = MidiInput::open(&in_id).expect("open the input port");
    let mut output = MidiOutput::open(&out_id).expect("open the output port");
    let sent = vec![
        Message::NoteOn {
            channel: 0,
            key: 60,
            velocity: 100,
        },
        Message::ControlChange {
            channel: 3,
            controller: 74,
            value: 12,
        },
        Message::PitchBend {
            channel: 1,
            value: -4_096,
        },
        Message::SysEx(vec![0x7D, 0x01, 0x02, 0x03, 0x04]),
        Message::NoteOff {
            channel: 0,
            key: 60,
            velocity: 0,
        },
    ];
    std::thread::sleep(Duration::from_millis(100));
    for m in &sent {
        output.send(m).expect("send");
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut got = Vec::new();
    let t = std::time::Instant::now();
    while got.len() < sent.len() && t.elapsed() < Duration::from_secs(5) {
        got.extend(input.poll().into_iter().map(|m| m.message));
        std::thread::sleep(Duration::from_millis(5));
    }
    // A note-off sent as velocity 0 may arrive as either spelling.
    let norm = |m: &Message| match m {
        Message::NoteOn {
            channel,
            key,
            velocity: 0,
        } => Message::NoteOff {
            channel: *channel,
            key: *key,
            velocity: 0,
        },
        m => m.clone(),
    };
    let got: Vec<Message> = got.iter().map(norm).collect();
    let want: Vec<Message> = sent.iter().map(norm).collect();
    assert_eq!(
        got, want,
        "every message, in order, through {out_id} → {in_id}"
    );
}

// ---- the default output wired to the default input ------------------------------

#[test]
#[ignore = "needs VIEWW_LOOP_BACKEND and a wired default device (see the module docs)"]
fn default_device_loopback() {
    use std::sync::{Arc, Mutex};
    use vieww_audio::backend::{backend, BlockInfo, StreamConfig};
    let Ok(name) = std::env::var("VIEWW_LOOP_BACKEND") else {
        return unavailable("VIEWW_LOOP_BACKEND not set");
    };
    let min_snr: f64 = std::env::var("VIEWW_LOOP_MIN_SNR")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(40.0);
    let b = backend(&name).unwrap_or_else(|| panic!("no backend {name:?} on this platform"));
    let config = StreamConfig {
        rate: 48_000,
        channels: 2,
        block: 256,
    };
    let got = Arc::new(Mutex::new(Vec::<f32>::new()));
    let g2 = got.clone();
    let capture_rate;
    let input = match b.open_input(
        config,
        Box::new(move |input: &[f32], info: &BlockInfo| {
            if let Ok(mut g) = g2.lock() {
                // Stereo, whatever the device delivered.
                if info.channels == 2 {
                    g.extend_from_slice(input);
                } else {
                    for f in input.chunks(info.channels.max(1)) {
                        g.push(f[0]);
                        g.push(*f.get(1).unwrap_or(&f[0]));
                    }
                }
            }
        }),
    ) {
        Ok(s) => {
            capture_rate = s.config().rate;
            s
        }
        Err(e) => return unavailable(&format!("{name}: no default input ({e:?})")),
    };
    let mut n = 0u64;
    let output = b
        .open_output(
            config,
            Box::new(move |out: &mut [f32], info: &BlockInfo| {
                let mut v = Vec::with_capacity(info.frames * 2);
                signal(info.rate, n, info.frames, &mut v);
                for (f, s) in out.chunks_mut(info.channels.max(1)).zip(v.chunks(2)) {
                    f[0] = s[0];
                    if f.len() > 1 {
                        f[1] = s[1];
                    }
                }
                n += info.frames as u64;
            }),
        )
        .unwrap_or_else(|e| panic!("{name}: the default output refused: {e:?}"));
    std::thread::sleep(Duration::from_secs(3));
    let (out_st, in_st) = (output.stats(), input.stats());
    drop(output);
    drop(input);
    eprintln!("{name}: output {out_st:?}\n{name}: input {in_st:?}");
    let data = got.lock().unwrap().clone();
    check_stereo(
        &format!("{name} default loopback"),
        &data,
        capture_rate,
        min_snr,
    );
    assert_eq!(
        out_st.xruns + in_st.xruns,
        0,
        "{name}: xruns during the loop"
    );
}

// ---- the default output, under load, for as long as asked ---------------------------

#[test]
#[ignore = "plays on the default output (see the module docs)"]
fn default_output_soak() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use vieww_audio::backend::{self, BlockInfo, StreamConfig};
    let secs: u64 = std::env::var("VIEWW_SOAK_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let gain: f32 = std::env::var("VIEWW_LIVE_GAIN")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    let config = StreamConfig {
        rate: 48_000,
        channels: 2,
        block: 256,
    };
    let nonfinite = Arc::new(AtomicUsize::new(0));
    let make = || {
        let nf = nonfinite.clone();
        let mut n = 0u64;
        let mut v = Vec::with_capacity(8_192);
        Box::new(move |out: &mut [f32], info: &BlockInfo| {
            v.clear();
            signal(info.rate, n, info.frames, &mut v);
            for (f, s) in out.chunks_mut(info.channels.max(1)).zip(v.chunks(2)) {
                for (c, o) in f.iter_mut().enumerate() {
                    *o = s[c.min(1)] * gain;
                }
            }
            if out.iter().any(|x| !x.is_finite()) {
                nf.fetch_add(1, Ordering::Relaxed);
            }
            n += info.frames as u64;
        }) as backend::RenderFn
    };
    let stream = match std::env::var("VIEWW_SOAK_BACKEND") {
        Ok(name) => backend::backend(&name)
            .unwrap_or_else(|| panic!("no backend {name:?}"))
            .open_output(config, make())
            .map_err(|e| vec![("named", e)]),
        Err(_) => backend::open_output(config, make),
    };
    let stream = match stream {
        Ok(s) => s,
        Err(e) => return unavailable(&format!("no output opens: {e:?}")),
    };
    // Every core busy: the deadline has to be met by priority, not luck.
    let stop = Arc::new(AtomicBool::new(false));
    let cores = std::thread::available_parallelism().map_or(2, std::num::NonZero::get);
    let load: Vec<_> = (0..cores)
        .map(|_| {
            let stop = stop.clone();
            std::thread::spawn(move || {
                let mut x = 1u64;
                while !stop.load(Ordering::Relaxed) {
                    x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                }
                x
            })
        })
        .collect();
    let t = std::time::Instant::now();
    while t.elapsed() < Duration::from_secs(secs) {
        std::thread::sleep(Duration::from_secs(1).min(Duration::from_secs(secs)));
        if stream.stats().ended {
            break;
        }
    }
    stop.store(true, Ordering::Relaxed);
    for l in load {
        let _ = l.join();
    }
    let st = stream.stats();
    let name = stream.backend();
    drop(stream);
    eprintln!(
        "soak {name}: {secs} s | {} callbacks, {:.1} s of audio | thread {:?} | render mean {:?}, max {:?}, peak load {:.0}% | {} late | {} xruns | latency {:?}",
        st.callbacks,
        st.frames as f64 / 48_000.0,
        st.priority,
        st.render_mean,
        st.render_max,
        st.load_peak * 100.0,
        st.late,
        st.xruns,
        st.latency,
    );
    assert!(!st.ended, "{name}: the stream ended during the soak");
    assert!(st.callbacks > 0, "{name}: no callbacks");
    assert!(st.priority.is_some(), "{name}: priority not recorded");
    assert_eq!(nonfinite.load(Ordering::Relaxed), 0);
    if let Some(max) = std::env::var("VIEWW_MAX_XRUNS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        assert!(st.xruns <= max, "{name}: {} xruns > {max}", st.xruns);
    }
}

// ---- CoreMIDI virtual endpoints (macOS) -------------------------------------------

#[cfg(target_os = "macos")]
mod coremidi {
    use super::*;
    use std::ffi::c_void;
    use std::sync::Mutex;
    use vieww_audio::midi::Message;
    use vieww_audio::midi_io::{MidiInput, MidiOutput};

    #[link(name = "CoreMIDI", kind = "framework")]
    unsafe extern "C" {
        fn MIDIClientCreate(
            name: *const c_void,
            n: *const c_void,
            r: *mut c_void,
            out: *mut u32,
        ) -> i32;
        fn MIDISourceCreate(client: u32, name: *const c_void, out: *mut u32) -> i32;
        fn MIDIDestinationCreate(
            client: u32,
            name: *const c_void,
            read: unsafe extern "C" fn(*const u8, *mut c_void, *mut c_void),
            refcon: *mut c_void,
            out: *mut u32,
        ) -> i32;
        fn MIDIReceived(source: u32, list: *const u8) -> i32;
        fn MIDIEndpointDispose(e: u32) -> i32;
        fn MIDIClientDispose(c: u32) -> i32;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithBytes(
            a: *const c_void,
            b: *const u8,
            n: isize,
            e: u32,
            x: u8,
        ) -> *const c_void;
    }

    fn cf(s: &str) -> *const c_void {
        // SAFETY: valid UTF-8 and length (leaked: a test's few strings).
        unsafe {
            CFStringCreateWithBytes(
                std::ptr::null(),
                s.as_ptr(),
                s.len() as isize,
                0x0800_0100,
                0,
            )
        }
    }

    fn packet(bytes: &[u8]) -> Vec<u8> {
        let mut l = 1u32.to_le_bytes().to_vec();
        l.extend_from_slice(&0u64.to_le_bytes());
        l.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
        l.extend_from_slice(bytes);
        l
    }

    static RECEIVED: Mutex<Vec<u8>> = Mutex::new(Vec::new());

    unsafe extern "C" fn on_read(list: *const u8, _: *mut c_void, _: *mut c_void) {
        // SAFETY: CoreMIDI's packet list; one packet per send here.
        unsafe {
            let n = list.cast::<u32>().read_unaligned();
            if n >= 1 {
                let len = usize::from(list.add(12).cast::<u16>().read_unaligned());
                let data = std::slice::from_raw_parts(list.add(14), len);
                RECEIVED.lock().unwrap().extend_from_slice(data);
            }
        }
    }

    #[test]
    #[ignore = "macOS CoreMIDI; run by the macOS job"]
    fn coremidi_virtual_round_trip() {
        let (mut client, mut src, mut dst) = (0, 0, 0);
        // SAFETY: CoreMIDI calls with out-pointers; the endpoints are
        // disposed at the end.
        unsafe {
            assert_eq!(
                MIDIClientCreate(
                    cf("vieww-test"),
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    &raw mut client
                ),
                0
            );
            assert_eq!(
                MIDISourceCreate(client, cf("vieww-test-source"), &raw mut src),
                0
            );
            assert_eq!(
                MIDIDestinationCreate(
                    client,
                    cf("vieww-test-dest"),
                    on_read,
                    std::ptr::null_mut(),
                    &raw mut dst
                ),
                0
            );
        }
        std::thread::sleep(Duration::from_millis(300));
        let input =
            MidiInput::open("vieww-test-source").expect("the virtual source is listed and opens");
        std::thread::sleep(Duration::from_millis(100));
        let l = packet(&[0x92, 64, 99]);
        // SAFETY: a one-packet list.
        assert_eq!(unsafe { MIDIReceived(src, l.as_ptr()) }, 0);
        let mut got = Vec::new();
        let t = std::time::Instant::now();
        while got.is_empty() && t.elapsed() < Duration::from_secs(3) {
            got.extend(input.poll());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            got.first().map(|t| t.message.clone()),
            Some(Message::NoteOn {
                channel: 2,
                key: 64,
                velocity: 99
            })
        );
        let mut output =
            MidiOutput::open("vieww-test-dest").expect("the virtual destination opens");
        output
            .send(&Message::ControlChange {
                channel: 0,
                controller: 7,
                value: 100,
            })
            .expect("send");
        let t = std::time::Instant::now();
        while RECEIVED.lock().unwrap().is_empty() && t.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(*RECEIVED.lock().unwrap(), vec![0xB0, 7, 100]);
        drop(input);
        drop(output);
        // SAFETY: endpoints and client we created.
        unsafe {
            MIDIEndpointDispose(src);
            MIDIEndpointDispose(dst);
            MIDIClientDispose(client);
        }
    }
}
