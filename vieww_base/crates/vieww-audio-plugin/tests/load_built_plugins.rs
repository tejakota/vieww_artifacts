//! Load the example plugins as real shared libraries through vieww's own
//! CLAP and VST3 hosts — the `dlopen`/`LoadLibrary` path, bundle layout and
//! module entry points, not the in-process entries the unit tests use.
//!
//! Needs the plugins built first (with a static `std`, as a host requires):
//! `tools/check-plugins.sh` does that and sets `VIEWW_PLUGIN_DIR` to the
//! directory holding `libvieww_gain.so` / `vieww_gain.dll` /
//! `libvieww_gain.dylib`. Without the variable the test does nothing.

use std::path::PathBuf;

use vieww_audio_plugin::clap::host::{HostEvent as ClapEvent, Library};
use vieww_audio_plugin::vst3::host::{HostEvent as VstEvent, Module};
use vieww_audio_plugin::vst3::sys::{kAudio, kEvent, kInput, kOutput};

fn plugin(name: &str) -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("VIEWW_PLUGIN_DIR")?);
    let file = if cfg!(windows) {
        format!("{name}.dll")
    } else if cfg!(target_os = "macos") {
        format!("lib{name}.dylib")
    } else {
        format!("lib{name}.so")
    };
    let p = dir.join(file);
    assert!(
        p.exists(),
        "VIEWW_PLUGIN_DIR is set but {} is missing",
        p.display()
    );
    Some(p)
}

#[test]
fn clap_plugins_load_and_run_from_disk() {
    let Some(gain) = plugin("vieww_gain") else {
        eprintln!("VIEWW_PLUGIN_DIR not set; skipping");
        return;
    };
    // SAFETY: our own example plugin.
    let lib = unsafe { Library::open(&gain) }.expect("load gain");
    let entries = lib.plugins();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "dev.vieww.gain");
    let mut p = lib.instantiate("dev.vieww.gain").unwrap();
    assert!(p.activate(48_000.0, 1, 256));
    let input = vec![vec![0.5f32; 256]; 2];
    let (out, _) = p.process(
        &input,
        2,
        256,
        &[ClapEvent::Param {
            time: 128,
            id: 0,
            value: -60.0,
        }],
        None,
    );
    assert!((out[0][10] - 0.5).abs() < 1e-3);
    // The change lands at frame 128 and is smoothed (5 ms), never stepped.
    assert!((out[0][127] - 0.5).abs() < 1e-3);
    assert!(out[0][128] < 0.5 && out[0][255] < out[0][128] && out[0][255] < 0.35);
    assert!(
        out[0][129..].windows(2).all(|w| w[1] <= w[0]),
        "monotonic glide"
    );
    let state = p.save().unwrap();
    let mut q = lib.instantiate("dev.vieww.gain").unwrap();
    assert!(q.load(&state));
    assert_eq!(q.value(0), p.value(0));

    let synth = plugin("vieww_synth").unwrap();
    // SAFETY: our own example plugin.
    let lib = unsafe { Library::open(&synth) }.expect("load synth");
    let mut s = lib.instantiate("dev.vieww.synth").unwrap();
    assert_eq!(s.note_ports(), 1);
    assert!(s.activate(44_100.0, 1, 512));
    let (out, _) = s.process(
        &[],
        2,
        512,
        &[ClapEvent::NoteOn {
            time: 0,
            channel: 0,
            key: 57,
            velocity: 1.0,
        }],
        None,
    );
    assert!(out[0].iter().any(|v| v.abs() > 0.01));
}

#[test]
fn vst3_plugins_load_and_run_from_disk_and_bundles() {
    let Some(gain) = plugin("vieww_gain") else {
        eprintln!("VIEWW_PLUGIN_DIR not set; skipping");
        return;
    };
    // The binary directly…
    // SAFETY: our own example plugin.
    let m = unsafe { Module::open(&gain) }.expect("load gain module");
    let classes = m.classes();
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0].subcategories, "Fx|Tools|Distortion|Stereo");
    let mut p = m.instantiate(&classes[0].cid).unwrap();
    assert_eq!(p.bus_channels(kAudio, kInput), vec![2]);
    assert!(p.setup(48_000.0, 128));
    let out = p.process(&[vec![0.25; 128], vec![0.25; 128]], 2, 128, &[], None);
    assert!((out[1][100] - 0.25).abs() < 1e-3);
    drop(p);
    drop(m);

    // …and laid out as a .vst3 bundle, the way hosts find it.
    let bundle = std::env::temp_dir().join(format!(
        "vieww-test-{}/vieww_synth.vst3",
        std::process::id()
    ));
    let binary = vieww_audio_plugin::vst3::host::bundle_binary(&bundle);
    std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
    std::fs::copy(plugin("vieww_synth").unwrap(), &binary).unwrap();
    // SAFETY: our own example plugin.
    let m = unsafe { Module::open(&bundle) }.expect("load synth bundle");
    let cid = m.classes()[0].cid;
    let mut s = m.instantiate(&cid).unwrap();
    assert_eq!(s.bus_channels(kEvent, kInput), vec![16]);
    assert_eq!(s.bus_channels(kAudio, kOutput), vec![2]);
    assert!(s.setup(48_000.0, 256));
    let out = s.process(
        &[],
        2,
        256,
        &[VstEvent::NoteOn {
            time: 0,
            channel: 0,
            pitch: 64,
            velocity: 0.7,
            note_id: 1,
        }],
        Some((120.0, 0.0)),
    );
    assert!(out[0].iter().any(|v| v.abs() > 0.01));
    drop(s);
    drop(m);
    std::fs::remove_dir_all(bundle.parent().unwrap()).ok();
}
