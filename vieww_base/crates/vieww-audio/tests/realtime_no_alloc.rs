//! The real-time render path allocates nothing.
//!
//! A device callback that allocates can block on the allocator's lock and
//! miss its deadline — the click real-time audio exists to avoid. This test
//! installs a counting global allocator, builds a graph exercising every
//! node kind on the render path (oscillator, per-channel processor,
//! automated gain, panner, stereo reverb, a delay in a feedback loop), warms
//! it up, and then requires that rendering a second of audio performs zero
//! allocations. It is the only test in this binary, so no other test's
//! allocations can be counted against it.

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

#[test]
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
}
