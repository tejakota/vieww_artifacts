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
    println!("realtime_no_alloc: the render path, mixer, parallel voices, disk stream, polyphony, every stereo effect, NaN recovery, the sampler and struck instruments allocate nothing");
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
    voices_and_every_effect_allocate_nothing();
    sampler_and_struck_instruments_allocate_nothing();
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

/// Single-threaded polyphony (`Poly::render_into`) with note-ons, voice
/// stealing and a tuning table, physical models, every
/// stateful stereo effect, and the mixer's NaN-recovery path — an insert
/// faulting and being reset mid-block must not allocate either.
fn voices_and_every_effect_allocate_nothing() {
    use vieww_audio::fx::delay::{DelayTime, StereoDelay};
    use vieww_audio::fx::dynamics::{Dynamics, Limiter};
    use vieww_audio::fx::modfx::Chorus;
    use vieww_audio::fx::reverb::{ConvolutionReverb, Fdn, Freeverb};
    use vieww_audio::fx::Stereo;
    use vieww_audio::mixer::Mixer;
    use vieww_audio::synth::physical::KarplusStrong;
    use vieww_audio::synth::voice::{Poly, SubtractiveVoice};

    let rate = 48_000.0;
    let mut poly = Poly::new((0..16).map(|_| SubtractiveVoice::new(rate)).collect(), rate);
    let mut strings = Poly::new(
        (0..6u64).map(|i| KarplusStrong::new(rate, i + 1)).collect(),
        rate,
    );
    for k in [48u8, 52, 55, 60] {
        poly.note_on(0, k, 0.8);
        strings.note_on(0, k, 0.8);
    }
    // A tuned pool (19-EDO), built before arming: retuning on the audio
    // thread must cost no allocation either.
    strings.set_tuning(Some(vieww_audio::synth::tuning::TuningTable::from_tuning(
        &vieww_audio::synth::tuning::EqualTemperament {
            divisions: 19.0,
            ..Default::default()
        },
    )));
    let ir: Vec<f32> = (0..4_096).map(|i| 0.4 * 0.999f32.powi(i)).collect();
    let mut fx: Vec<Box<dyn Stereo + Send>> = vec![
        Box::new(Freeverb::new(0.8, 0.3, rate)),
        Box::new(Fdn::new(2.0, 6_000.0, 0.01, rate)),
        Box::new(ConvolutionReverb::new(&ir, &ir, 256)),
        Box::new(StereoDelay::new(DelayTime::Ms(120.0), 0.6, 1.0, rate)),
        Box::new(Chorus::new(3, 0.5, rate)),
        Box::new(Dynamics::compressor(-18.0, 4.0, 0.005, 0.1, rate)),
        Box::new(Limiter::true_peak(-1.0, 0.005, 0.1, rate)),
    ];
    let mut m = Mixer::new(48_000);
    let t = m.add_track("glitchy");
    m.add_insert(t, Box::new(Freeverb::new(0.8, 0.3, rate)), None)
        .unwrap();

    let mut buf = vec![0.0f32; 2 * 256];
    let mut out = vec![0.0f32; 2 * 256];
    let mut run = |poly: &mut Poly<SubtractiveVoice>,
                   strings: &mut Poly<KarplusStrong>,
                   fx: &mut [Box<dyn Stereo + Send>],
                   m: &mut Mixer,
                   poison: bool| {
        poly.render_into(&mut buf);
        strings.render_into(&mut out);
        for (b, o) in buf.iter_mut().zip(&out) {
            *b += *o;
        }
        for e in fx.iter_mut() {
            e.run_stereo(&mut buf);
            if poison {
                e.process_stereo(f32::NAN, f32::NAN);
                e.reset();
            }
        }
        m.process(&[&buf], &mut out);
    };
    run(&mut poly, &mut strings, &mut fx, &mut m, false); // warm-up

    ARMED.store(true, Ordering::SeqCst);
    for i in 0..190 {
        // Note-ons every block: 16 voices, 24 keys cycling, so most
        // presses steal — the path that used to build two Vecs.
        let k = 40 + (i % 24) as u8;
        poly.note_on(0, k, 0.7);
        strings.note_on_pitch(0, k, f32::from(k) + 0.25, 0.7);
        if i % 3 == 0 {
            poly.note_off(0, k.saturating_sub(5));
        }
        run(&mut poly, &mut strings, &mut fx, &mut m, i % 50 == 7);
    }
    ARMED.store(false, Ordering::SeqCst);

    assert_eq!(
        ALLOCS.load(Ordering::SeqCst),
        0,
        "polyphony, an effect or a reset allocated"
    );
    assert!(out.iter().all(|v| v.is_finite()));
    assert!(out.iter().any(|v| v.abs() > 1e-4), "and it produced sound");
}

/// Sampler note-ons and note-offs (zone selection, round robin, envelope
/// re-arming, tuning) and modal strikes — every note event is audio-thread
/// work in a live instrument.
fn sampler_and_struck_instruments_allocate_nothing() {
    use std::sync::Arc;
    use vieww_audio::synth::physical::Modal;
    use vieww_audio::synth::sampler::{Instrument, SampleData, Sampler, Zone};
    use vieww_audio::synth::tuning::{EqualTemperament, TuningTable};
    use vieww_audio::synth::voice::Poly;

    let rate = 48_000.0;
    let tone: Vec<f32> = (0..24_000)
        .map(|i| (i as f32 * 220.0 * std::f32::consts::TAU / rate).sin() * 0.5)
        .collect();
    let data = Arc::new(SampleData::mono(tone, rate));
    let mut zones = Vec::new();
    for pos in 1..=3 {
        let mut z = Zone::new(Arc::clone(&data), 57.0);
        z.seq_length = 3;
        z.seq_position = pos;
        zones.push(z);
    }
    zones.push(Zone::new(Arc::clone(&data), 45.0));
    let mut sampler = Sampler::new(
        Instrument {
            zones,
            ..Instrument::default()
        },
        8,
        rate,
    );
    sampler.set_tuning(Some(TuningTable::from_tuning(&EqualTemperament {
        divisions: 22.0,
        ..Default::default()
    })));
    let mut bells = Poly::new((0..6).map(|_| Modal::glockenspiel(rate)).collect(), rate);

    // Warm-up: touch every key once so the round-robin table holds them.
    for k in 0..128u8 {
        sampler.note_on(0, k, 100);
        sampler.note_off(0, k);
    }
    bells.note_on(0, 72, 0.8);
    let mut out = vec![0.0f32; 2 * 256];

    ARMED.store(true, Ordering::SeqCst);
    for i in 0..400u32 {
        let k = 48 + (i % 30) as u8;
        sampler.note_on(0, k, 90);
        if i % 2 == 0 {
            sampler.note_off(0, k.saturating_sub(3));
        }
        bells.note_on(0, k + 12, 0.6);
        for f in out.as_chunks_mut::<2>().0 {
            let (l, r) = sampler.next_stereo();
            let (bl, br) = bells.next_stereo();
            f[0] = l + bl;
            f[1] = r + br;
        }
    }
    ARMED.store(false, Ordering::SeqCst);

    assert_eq!(
        ALLOCS.load(Ordering::SeqCst),
        0,
        "a sampler note event or a strike allocated"
    );
    assert!(out.iter().any(|v| v.abs() > 1e-4), "and it produced sound");
}
