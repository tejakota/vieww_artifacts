//! Live backends under load: telemetry end to end.
//!
//! Needs running sound servers (as the `audio-backends` workflow starts
//! them: `jackd -d dummy`, `pulseaudio` with a null sink), so it is
//! ignored by default. Run with
//!
//! `VIEWW_SOAK_SECS=10 cargo test --release -p vieww-audio --test live_backends -- --ignored --nocapture`
//!
//! `VIEWW_LIVE_GAIN` (default 0.2) scales what is played — 0 streams
//! silence, every frame still rendered and delivered (unattended runs on a
//! desktop's real speakers).
//!
//! For each Linux server backend: a 16-voice synth through a reverb is
//! played for `VIEWW_SOAK_SECS` (default 5) seconds while every core is
//! saturated by spinning threads, and the stream's own statistics are
//! printed and checked — the callback's scheduling class, render load,
//! renders that overran their block, and xruns the server reported.

#![cfg(all(target_os = "linux", not(target_os = "android")))]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use vieww_audio::backend::{jack::Jack, pulse::Pulse, Backend, BlockInfo, StreamConfig};
use vieww_audio::fx::reverb::Freeverb;
use vieww_audio::fx::Stereo;
use vieww_audio::synth::voice::{Poly, SubtractiveVoice};

fn soak_secs() -> u64 {
    std::env::var("VIEWW_SOAK_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5)
}

fn run(backend: &dyn Backend) {
    let rate = 48_000.0;
    let mut poly = Poly::new((0..16).map(|_| SubtractiveVoice::new(rate)).collect(), rate);
    for k in 0..16u8 {
        poly.note_on(0, 36 + k * 2, 0.6);
    }
    let mut verb = Freeverb::new(0.8, 0.3, rate);
    let gain: f32 = std::env::var("VIEWW_LIVE_GAIN")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.2);
    let nonfinite = Arc::new(Mutex::new(0usize));
    let nf = Arc::clone(&nonfinite);
    let render = Box::new(move |out: &mut [f32], info: &BlockInfo| {
        let ch = info.channels.max(1);
        for f in out.chunks_mut(ch) {
            let (l, r) = poly.next_stereo();
            let (l, r) = verb.process_stereo(l, r);
            f[0] = l * gain;
            if ch > 1 {
                f[1] = r * gain;
            }
        }
        if out.iter().any(|v| !v.is_finite()) {
            if let Ok(mut n) = nf.try_lock() {
                *n += 1;
            }
        }
    });
    let stream = match backend.open_output(
        StreamConfig {
            rate: 48_000,
            channels: 2,
            block: 256,
        },
        render,
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: unavailable ({e:?}) — skipped", backend.name());
            return;
        }
    };

    let stop = Arc::new(AtomicBool::new(false));
    let cores = std::thread::available_parallelism().map_or(2, std::num::NonZero::get);
    let load: Vec<_> = (0..cores)
        .map(|_| {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let mut x = 1u64;
                while !stop.load(Ordering::Relaxed) {
                    x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                }
                x
            })
        })
        .collect();
    std::thread::sleep(Duration::from_secs(soak_secs()));
    stop.store(true, Ordering::Relaxed);
    for l in load {
        let _ = l.join();
    }
    let st = stream.stats();
    drop(stream);

    eprintln!(
        "{:>6}: {} callbacks, {:.1} s of audio | callback thread {:?} | render mean {:?}, max {:?}, peak load {:.0}% | {} late renders | {} xruns",
        backend.name(),
        st.callbacks,
        st.frames as f64 / 48_000.0,
        st.priority,
        st.render_mean,
        st.render_max,
        st.load_peak * 100.0,
        st.late,
        st.xruns,
    );
    assert!(st.callbacks > 10, "{}: the stream ran", backend.name());
    assert!(
        st.priority.is_some(),
        "{}: priority recorded",
        backend.name()
    );
    assert!(st.render_max >= st.render_mean);
    assert_eq!(*nonfinite.lock().unwrap(), 0);
}

#[test]
#[ignore = "needs running JACK / PulseAudio servers"]
fn jack_under_load() {
    run(&Jack::default());
}

#[test]
#[ignore = "needs running JACK / PulseAudio servers"]
fn pulse_under_load() {
    run(&Pulse);
}

/// Hot-plug, for real: a resilient stream on a dedicated PulseAudio null
/// sink survives the sink being removed and loaded again, with the same
/// render closure (its frame count carries on).
#[test]
#[ignore = "needs a running PulseAudio server and pactl"]
fn pulse_sink_unplug_and_replug() {
    use std::process::Command;
    use std::sync::atomic::AtomicU64;
    use vieww_audio::backend::resilient::{ResilientStream, Target};

    let pactl = |args: &[&str]| {
        Command::new("pactl")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };
    let load = || pactl(&["load-module", "module-null-sink", "sink_name=vieww-hotplug"]);
    let Some(mut module) = load() else {
        eprintln!("no pactl / server — skipped");
        return;
    };
    let frames = Arc::new(AtomicU64::new(0));
    let f2 = Arc::clone(&frames);
    let rs = ResilientStream::open(
        Box::new(Pulse),
        Target::Named {
            id: "vieww-hotplug".into(),
            fallback_after: None,
        },
        StreamConfig::default(),
        Box::new(move |out: &mut [f32], info: &BlockInfo| {
            f2.fetch_add(info.frames as u64, Ordering::Relaxed);
            out.fill(0.0);
        }),
    )
    .expect("open on the dedicated sink");
    let wait_for_frames = |more: u64, secs: u64| {
        let start = frames.load(Ordering::Relaxed);
        let t = std::time::Instant::now();
        while t.elapsed() < Duration::from_secs(secs) {
            if frames.load(Ordering::Relaxed) >= start + more {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    };
    assert!(wait_for_frames(48_000, 10), "playing");
    for round in 0..2 {
        pactl(&["unload-module", &module]).expect("unplug");
        // The server moves the stream to its fallback sink rather than
        // ending it: no silence while the device is away.
        std::thread::sleep(Duration::from_secs(2));
        assert_eq!(
            rs.reopens(),
            round,
            "round {round}: not reopened while away"
        );
        module = load().expect("replug");
        let t = std::time::Instant::now();
        while rs.reopens() == round && t.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(
            rs.reopens(),
            round + 1,
            "round {round}: back on the named sink"
        );
        assert!(
            wait_for_frames(48_000, 15),
            "round {round}: playback resumed"
        );
        let inputs = pactl(&["list", "sink-inputs"]).unwrap_or_default();
        let sinks = pactl(&["list", "sinks", "short"]).unwrap_or_default();
        let ours = sinks
            .lines()
            .find(|l| l.contains("vieww-hotplug"))
            .and_then(|l| l.split_whitespace().next())
            .expect("sink listed");
        assert!(
            inputs.lines().any(|l| l.trim() == format!("Sink: {ours}")),
            "round {round}: a stream plays on vieww-hotplug ({ours})"
        );
    }
    assert!(!rs.following_default(), "it waited for its own device");
    drop(rs);
    pactl(&["unload-module", &module]);
}
