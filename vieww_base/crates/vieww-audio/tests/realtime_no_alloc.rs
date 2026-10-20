//! The real-time render path allocates nothing.
//!
//! A device callback that allocates can block on the allocator's lock and
//! miss its deadline — the click real-time audio exists to avoid. This test
//! installs a counting global allocator, builds a graph exercising every
//! node kind on the render path (oscillator, per-channel processor,
//! automated gain, panner, stereo reverb, a delay in a feedback loop), warms
//! it up, and then requires that rendering a second of audio performs zero
//! allocations. The same holds for the mixer (tracks, a bus, pre- and
//! post-fader sends, a keyed compressor, a lookahead limiter with delay
//! compensation, metering and EBU R128 loudness), for multi-core voice
//! rendering (the worker threads are counted too), and for reading a disk
//! stream.
//!
//! **No test harness** (`harness = false` in Cargo.toml): the counter sees
//! every thread in the process, and it has to — the voice workers and the
//! disk reader are part of what is checked — so the process must contain
//! nothing else. Under libtest it did not: the runner's own thread made 3
//! allocations (852 bytes) while the render was armed in about one run in
//! five on a loaded machine, none on the render thread, and failed CI's
//! workspace test stage (run 39445768442) on a render path that allocated
//! nothing. Here `main` is the whole program.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Counting;

static ARMED: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: forwards every call to the system allocator unchanged; the only
// addition is an atomic counter, which cannot affect the returned memory.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ARMED.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `System.alloc` via `alloc` above.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn main() {
    rendering_allocates_nothing();
    println!("realtime_no_alloc: the render path, mixer, parallel voices and disk stream allocate nothing");
}

fn rendering_allocates_nothing() {
    use vieww_audio::fx::filter::{Svf, SvfMode};
    use vieww_audio::fx::reverb::Freeverb;
    use vieww_audio::graph::{
        DelayNode, GainNode, Graph, PanNode, ProcessorNode, SourceNode, StereoNode,
    };
    use vieww_audio::synth::osc::{Oscillator, Shape};

    let rate = 48_000.0;
    let mut g = Graph::new(rate);
    let osc = g.add(SourceNode(Oscillator::new(Shape::Saw, 110.0, rate)));
    let filt = g.add(ProcessorNode::new(1, || {
        Svf::new(SvfMode::LowPass, 1200.0, 2.0, rate)
    }));
    let mut amp = GainNode::new(0.0, 1);
    amp.gain
        .set_at(0.0, 0.0)
        .linear_to(0.8, 0.5)
        .target_at(0.2, 0.6, 0.1);
    let amp = g.add(amp);
    let mix = g.add(GainNode::new(1.0, 1));
    let fb = g.add(GainNode::new(0.4, 1));
    let echo = g.add(DelayNode::new(0.25, 1.0, 1, rate));
    let pan = g.add(PanNode::new(-0.3, 1));
    let verb = g.add(StereoNode(Freeverb::new(0.7, 0.4, rate)));
    g.connect(osc, filt).unwrap();
    g.connect(filt, amp).unwrap();
    g.connect(amp, mix).unwrap();
    g.connect(mix, fb).unwrap();
    g.connect(fb, echo).unwrap();
    g.connect(echo, mix).unwrap();
    g.connect(mix, pan).unwrap();
    g.connect(pan, verb).unwrap();
    g.set_output(verb);

    let mut out = vec![0.0f32; 2 * 1024];
    g.render_interleaved(&mut out, 2); // warm-up

    ARMED.store(true, Ordering::SeqCst);
    for _ in 0..47 {
        g.render_interleaved(&mut out, 2);
    }
    ARMED.store(false, Ordering::SeqCst);

    assert_eq!(
        ALLOCS.load(Ordering::SeqCst),
        0,
        "the render path allocated"
    );
    assert!(out.iter().any(|v| v.abs() > 1e-4), "and it produced sound");

    mixer_allocates_nothing();
    parallel_voices_allocate_nothing();
    disk_streaming_allocates_nothing();
}

fn mixer_allocates_nothing() {
    use vieww_audio::fx::dynamics::{Dynamics, Limiter};
    use vieww_audio::fx::reverb::Freeverb;
    use vieww_audio::mixer::{Channel, Dest, Key, Mixer, Tap};

    let mut m = Mixer::new(48_000);
    let vox = m.add_track("vox");
    let kick = m.add_track("kick");
    let verb = m.add_bus("verb");
    m.add_send(vox, 0, -12.0, Tap::PostFader).unwrap();
    m.add_send(kick, 0, -20.0, Tap::PreFader).unwrap();
    m.add_insert(verb, Box::new(Freeverb::new(0.6, 0.5, 48_000.0)), None)
        .unwrap();
    let duck = Dynamics::compressor(-24.0, 4.0, 0.005, 0.1, 48_000.0);
    m.add_insert(vox, Box::new(duck), Some(Key::Input(1)))
        .unwrap();
    m.add_insert(
        Channel::Master,
        Box::new(Limiter::new(-1.0, 0.002, 0.05, 48_000.0)),
        None,
    )
    .unwrap();
    m.route(kick, Dest::Master).unwrap();
    m.params_mut(vox).unwrap().mono_input = true;
    let vox_in: Vec<f32> = (0..1_000).map(|i| (i as f32 * 0.03).sin() * 0.5).collect();
    let kick_in: Vec<f32> = (0..2_000).map(|i| (i as f32 * 0.01).sin() * 0.8).collect();
    let mut out = vec![0.0f32; 2_000];
    m.process(&[&vox_in, &kick_in], &mut out); // warm-up

    ARMED.store(true, Ordering::SeqCst);
    for i in 0..48 {
        // Parameter changes are allowed on the audio thread.
        m.params_mut(vox).unwrap().fader_db = -(i as f32) * 0.1;
        m.params_mut(kick).unwrap().pan = (i as f32 * 0.1).sin();
        m.process(&[&vox_in, &kick_in], &mut out);
    }
    let _ = m.loudness().integrated();
    let _ = m.meter(Channel::Master).unwrap();
    ARMED.store(false, Ordering::SeqCst);
    assert_eq!(ALLOCS.load(Ordering::SeqCst), 0, "the mixer allocated");
    assert!(out.iter().any(|v| v.abs() > 1e-3));
}

fn parallel_voices_allocate_nothing() {
    use vieww_audio::parallel::ParallelPoly;
    use vieww_audio::synth::voice::SubtractiveVoice;

    let voices = (0..16).map(|_| SubtractiveVoice::new(48_000.0)).collect();
    let mut p = ParallelPoly::new(voices, 4, 3, 48_000.0, 256);
    for k in [48, 52, 55, 60, 64, 67] {
        p.note_on(0, k, 0.9);
    }
    let mut out = vec![0.0f32; 512];
    p.render(&mut out); // warm-up
    ARMED.store(true, Ordering::SeqCst);
    for _ in 0..100 {
        p.render(&mut out);
    }
    p.note_off(0, 60);
    p.render(&mut out);
    ARMED.store(false, Ordering::SeqCst);
    assert_eq!(
        ALLOCS.load(Ordering::SeqCst),
        0,
        "parallel rendering allocated"
    );
    assert!(out.iter().any(|v| v.abs() > 1e-3));
}

fn disk_streaming_allocates_nothing() {
    use vieww_audio::channels::{Layout, MultiSamples};
    use vieww_audio::loader::DiskStream;
    use vieww_audio::wav::{write_wav_format, SampleFormat};

    let m = MultiSamples {
        data: (0..96_000)
            .map(|i| (i as f32 * 0.002).sin() * 0.4)
            .collect(),
        layout: Layout::Stereo,
        rate: 48_000,
    };
    let path = std::env::temp_dir().join(format!("vieww-rt-{}.wav", std::process::id()));
    std::fs::write(&path, write_wav_format(&m, SampleFormat::I24).unwrap()).unwrap();
    let mut s = DiskStream::open(&path, 8_192).unwrap();
    assert!(s.prime(4_096, std::time::Duration::from_secs(10)));
    let mut out = vec![0.0f32; 256];
    let _ = s.read(&mut out);
    ARMED.store(true, Ordering::SeqCst);
    let mut got = 0;
    for _ in 0..200 {
        got += s.read(&mut out);
        std::thread::sleep(std::time::Duration::from_micros(100));
    }
    s.seek(1_000);
    for _ in 0..50 {
        got += s.read(&mut out);
        std::thread::sleep(std::time::Duration::from_micros(100));
    }
    ARMED.store(false, Ordering::SeqCst);
    drop(s);
    std::fs::remove_file(&path).ok();
    assert_eq!(ALLOCS.load(Ordering::SeqCst), 0, "disk streaming allocated");
    assert!(got > 0);
}
