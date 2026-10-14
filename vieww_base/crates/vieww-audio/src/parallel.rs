//! Multi-core rendering.
//!
//! [`Pool`] renders independent jobs — instrument parts, groups of voices,
//! tracks — on persistent worker threads, the way a DAW spreads a project
//! over cores. It is built for the audio thread:
//!
//! * the workers are created once; a block costs no thread creation, no
//!   lock and no allocation — workers wait on an epoch counter (a short
//!   spin, then a park) and claim jobs with an atomic counter;
//! * the audio thread works too, claiming jobs alongside the workers, so a
//!   pool of `n` threads uses `n + 1` cores and a pool of 0 threads is
//!   simply serial rendering;
//! * results are summed in job order, so the output is **bit-identical**
//!   whatever the thread count and whatever order the jobs finished in.
//!
//! Between blocks, [`Pool::job_mut`] gives exclusive access to a job —
//! that is where note events and parameter changes go.
//!
//! [`ParallelPoly`] applies this to polyphony: one voice pool split into
//! groups, a new note going to the group with the fewest sounding voices
//! (and its release to the same group), controllers and pedals broadcast.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use crate::midi::Message;
use crate::synth::voice::{Poly, Voice};

/// Something rendered in blocks of interleaved stereo.
pub trait Render: Send {
    /// Render `out.len() / 2` frames into `out` (which arrives zeroed).
    fn render(&mut self, out: &mut [f32]);
}

impl<V: Voice + Send> Render for Poly<V> {
    fn render(&mut self, out: &mut [f32]) {
        for f in out.as_chunks_mut::<2>().0 {
            let (l, r) = self.next_stereo();
            f[0] = l;
            f[1] = r;
        }
    }
}

struct Shared<J> {
    jobs: Box<[UnsafeCell<J>]>,
    bufs: Box<[UnsafeCell<Vec<f32>>]>,
    frames: AtomicUsize,
    next: AtomicUsize,
    done: AtomicUsize,
    epoch: AtomicU64,
    quit: AtomicBool,
}

// SAFETY: a job and its buffer are touched by exactly one thread at a time:
// during a block by whichever thread claimed its index from `next` (each
// index is handed out once per epoch), and between blocks only through
// `&mut Pool`. `J: Send` lets it move between those threads.
unsafe impl<J: Send> Sync for Shared<J> {}

impl<J: Render> Shared<J> {
    /// Claim and render jobs until none are left.
    fn work(&self) {
        let n = self.jobs.len();
        loop {
            let i = self.next.fetch_add(1, Ordering::AcqRel);
            if i >= n {
                return;
            }
            let frames = self.frames.load(Ordering::Acquire);
            // SAFETY: index `i` was claimed by this thread alone (see the
            // `Sync` impl).
            let (job, buf) = unsafe { (&mut *self.jobs[i].get(), &mut *self.bufs[i].get()) };
            let out = &mut buf[..frames * 2];
            out.fill(0.0);
            job.render(out);
            self.done.fetch_add(1, Ordering::AcqRel);
        }
    }
}

/// A pool of jobs rendered across threads.
pub struct Pool<J: Render + 'static> {
    shared: Arc<Shared<J>>,
    workers: Vec<JoinHandle<()>>,
    max_block: usize,
}

impl<J: Render + 'static> std::fmt::Debug for Pool<J> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pool")
            .field("jobs", &self.shared.jobs.len())
            .field("threads", &self.workers.len())
            .field("max_block", &self.max_block)
            .finish()
    }
}

impl<J: Render + 'static> Pool<J> {
    /// A pool of `jobs` rendered by `threads` workers plus the calling
    /// thread, in blocks of up to `max_block` frames.
    #[must_use]
    pub fn new(jobs: Vec<J>, threads: usize, max_block: usize) -> Self {
        let max_block = max_block.max(1);
        let n = jobs.len();
        let shared = Arc::new(Shared {
            jobs: jobs.into_iter().map(UnsafeCell::new).collect(),
            bufs: (0..n)
                .map(|_| UnsafeCell::new(vec![0.0; max_block * 2]))
                .collect(),
            frames: AtomicUsize::new(0),
            next: AtomicUsize::new(n),
            done: AtomicUsize::new(0),
            epoch: AtomicU64::new(0),
            quit: AtomicBool::new(false),
        });
        let workers = (0..threads)
            .filter_map(|k| {
                let s = Arc::clone(&shared);
                std::thread::Builder::new()
                    .name(format!("vieww-render-{k}"))
                    .spawn(move || {
                        let mut seen = 0;
                        loop {
                            let mut spins = 0u32;
                            loop {
                                let e = s.epoch.load(Ordering::Acquire);
                                if e != seen {
                                    seen = e;
                                    break;
                                }
                                if spins < 2_000 {
                                    spins += 1;
                                    std::hint::spin_loop();
                                } else {
                                    std::thread::park_timeout(std::time::Duration::from_millis(5));
                                }
                            }
                            if s.quit.load(Ordering::Acquire) {
                                return;
                            }
                            s.work();
                        }
                    })
                    .ok()
            })
            .collect();
        Self {
            shared,
            workers,
            max_block,
        }
    }

    /// Number of jobs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.shared.jobs.len()
    }

    /// Whether there are no jobs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.shared.jobs.is_empty()
    }

    /// Worker threads (besides the caller).
    #[must_use]
    pub fn threads(&self) -> usize {
        self.workers.len()
    }

    /// A job, between blocks.
    pub fn job_mut(&mut self, i: usize) -> Option<&mut J> {
        // SAFETY: `&mut self` means no block is running, and after a block
        // every index has been claimed (`next ≥ len`), so no worker can
        // reach a job until the next `render`.
        self.shared.jobs.get(i).map(|c| unsafe { &mut *c.get() })
    }

    /// Every job, between blocks.
    pub fn jobs_mut(&mut self) -> impl Iterator<Item = &mut J> {
        // SAFETY: as in `job_mut`.
        self.shared.jobs.iter().map(|c| unsafe { &mut *c.get() })
    }

    /// Render into interleaved stereo `out`: every job, summed. Never
    /// allocates.
    pub fn render(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        let frames = out.len() / 2;
        let mut at = 0;
        while at < frames {
            let n = (frames - at).min(self.max_block);
            self.block(n);
            let dst = &mut out[at * 2..(at + n) * 2];
            for b in &*self.shared.bufs {
                // SAFETY: the block is complete (`done == len`), so no
                // worker holds a buffer.
                let b = unsafe { &*b.get() };
                crate::simd::add_scaled(dst, &b[..n * 2], 1.0);
            }
            at += n;
        }
    }

    fn block(&self, frames: usize) {
        let s = &self.shared;
        s.frames.store(frames, Ordering::Release);
        s.done.store(0, Ordering::Release);
        s.next.store(0, Ordering::Release);
        s.epoch.fetch_add(1, Ordering::AcqRel);
        for w in &self.workers {
            w.thread().unpark();
        }
        s.work();
        let n = s.jobs.len();
        let mut spins = 0u32;
        while s.done.load(Ordering::Acquire) < n {
            if spins < 10_000 {
                spins += 1;
                std::hint::spin_loop();
            } else {
                std::thread::yield_now();
            }
        }
    }
}

impl<J: Render + 'static> Drop for Pool<J> {
    fn drop(&mut self) {
        self.shared.quit.store(true, Ordering::Release);
        self.shared.epoch.fetch_add(1, Ordering::AcqRel);
        for w in self.workers.drain(..) {
            w.thread().unpark();
            w.join().ok();
        }
    }
}

/// A polyphonic instrument whose voices render on several cores.
pub struct ParallelPoly<V: Voice + Send + 'static> {
    pool: Pool<Poly<V>>,
    owner: Box<[[u8; 128]; 16]>,
}

impl<V: Voice + Send + 'static> std::fmt::Debug for ParallelPoly<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParallelPoly")
            .field("pool", &self.pool)
            .finish_non_exhaustive()
    }
}

const NONE: u8 = u8::MAX;

impl<V: Voice + Send + 'static> ParallelPoly<V> {
    /// Split `voices` into `groups` pools (polyphonic mode), rendered by
    /// `threads` workers plus the caller.
    #[must_use]
    pub fn new(voices: Vec<V>, groups: usize, threads: usize, rate: f32, max_block: usize) -> Self {
        let groups = groups.clamp(1, voices.len().max(1)).min(usize::from(NONE));
        let mut buckets: Vec<Vec<V>> = (0..groups).map(|_| Vec::new()).collect();
        for (i, v) in voices.into_iter().enumerate() {
            buckets[i % groups].push(v);
        }
        let polys = buckets.into_iter().map(|b| Poly::new(b, rate)).collect();
        Self {
            pool: Pool::new(polys, threads, max_block),
            owner: Box::new([[NONE; 128]; 16]),
        }
    }

    /// Voices sounding across all groups.
    pub fn active(&mut self) -> usize {
        self.pool.jobs_mut().map(|p| p.active()).sum()
    }

    /// The pool (for per-group settings such as glide or bend range).
    pub fn pool_mut(&mut self) -> &mut Pool<Poly<V>> {
        &mut self.pool
    }

    /// Start a note on the least busy group.
    pub fn note_on(&mut self, channel: u8, key: u8, velocity: f32) {
        let (c, k) = (usize::from(channel & 15), usize::from(key & 127));
        // A retrigger stays on the group that owns the key.
        let g = if self.owner[c][k] == NONE {
            let mut best = (usize::MAX, 0);
            for (i, p) in self.pool.jobs_mut().enumerate() {
                let a = p.active();
                if a < best.0 {
                    best = (a, i);
                }
            }
            best.1
        } else {
            usize::from(self.owner[c][k])
        };
        self.owner[c][k] = u8::try_from(g).unwrap_or(0);
        if let Some(p) = self.pool.job_mut(g) {
            p.note_on(channel, key, velocity);
        }
    }

    /// Release a note on the group that has it.
    pub fn note_off(&mut self, channel: u8, key: u8) {
        let (c, k) = (usize::from(channel & 15), usize::from(key & 127));
        let g = std::mem::replace(&mut self.owner[c][k], NONE);
        if g != NONE {
            if let Some(p) = self.pool.job_mut(usize::from(g)) {
                p.note_off(channel, key);
            }
        }
    }

    /// Feed a MIDI message: notes are routed, everything else broadcast.
    pub fn midi(&mut self, m: &Message) {
        match m.clone().normalized() {
            Message::NoteOn {
                channel,
                key,
                velocity,
            } => self.note_on(channel, key, f32::from(velocity) / 127.0),
            Message::NoteOff { channel, key, .. } => self.note_off(channel, key),
            other => {
                if let Message::ControlChange {
                    controller: 120 | 123,
                    ..
                } = other
                {
                    *self.owner = [[NONE; 128]; 16];
                }
                for p in self.pool.jobs_mut() {
                    p.midi(&other);
                }
            }
        }
    }

    /// Render interleaved stereo. Never allocates.
    pub fn render(&mut self, out: &mut [f32]) {
        self.pool.render(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::voice::SubtractiveVoice;

    struct Sine {
        phase: f32,
        step: f32,
    }

    impl Render for Sine {
        fn render(&mut self, out: &mut [f32]) {
            for f in out.as_chunks_mut::<2>().0 {
                let v = self.phase.sin() * 0.1;
                self.phase += self.step;
                f[0] = v;
                f[1] = -v;
            }
        }
    }

    fn sines(n: usize) -> Vec<Sine> {
        (0..n)
            .map(|i| Sine {
                phase: 0.0,
                step: 0.01 * (i + 1) as f32,
            })
            .collect()
    }

    #[test]
    fn threads_do_not_change_a_single_bit() {
        let mut serial = Pool::new(sines(13), 0, 64);
        let mut par = Pool::new(sines(13), 3, 64);
        assert_eq!(par.threads(), 3);
        for len in [1, 64, 65, 300, 7] {
            let mut a = vec![0.0; len * 2];
            let mut b = vec![1.0; len * 2];
            serial.render(&mut a);
            par.render(&mut b);
            assert_eq!(a, b, "len {len}");
        }
        // And the sum is the sum of the jobs.
        let mut one = Pool::new(sines(1), 2, 64);
        let mut out = vec![0.0; 4];
        one.render(&mut out);
        assert_eq!(out[2], (0.01f32).sin() * 0.1);
        assert_eq!(out[3], -out[2]);
    }

    #[test]
    fn jobs_are_reachable_between_blocks() {
        let mut p = Pool::new(sines(4), 2, 32);
        p.job_mut(2).unwrap().step = 0.0;
        p.job_mut(2).unwrap().phase = 1.0;
        let mut out = vec![0.0; 64];
        p.render(&mut out);
        assert_eq!(p.len(), 4);
        assert!(!p.is_empty());
        assert!(p.job_mut(9).is_none());
        let empty: Pool<Sine> = Pool::new(Vec::new(), 1, 32);
        assert!(empty.is_empty());
    }

    #[test]
    fn many_blocks_under_contention_stay_deterministic() {
        let mut serial = Pool::new(sines(32), 0, 128);
        let mut par = Pool::new(sines(32), 4, 128);
        let mut a = vec![0.0; 256];
        let mut b = vec![0.0; 256];
        for _ in 0..500 {
            serial.render(&mut a);
            par.render(&mut b);
            assert_eq!(a, b);
        }
    }

    #[test]
    fn parallel_polyphony_routes_notes_and_matches_serial() {
        let voices = |n| {
            (0..n)
                .map(|_| SubtractiveVoice::new(48_000.0))
                .collect::<Vec<_>>()
        };
        let mut serial = ParallelPoly::new(voices(16), 4, 0, 48_000.0, 128);
        let mut par = ParallelPoly::new(voices(16), 4, 3, 48_000.0, 128);
        for p in [&mut serial, &mut par] {
            for k in [60, 64, 67, 71, 74] {
                p.note_on(0, k, 0.8);
            }
        }
        assert_eq!(par.active(), 5);
        // Spread: no group holds more than two of five notes.
        let per_group: Vec<usize> = par.pool_mut().jobs_mut().map(|p| p.active()).collect();
        assert!(per_group.iter().all(|a| *a <= 2), "{per_group:?}");
        let mut a = vec![0.0; 2_048];
        let mut b = vec![0.0; 2_048];
        serial.render(&mut a);
        par.render(&mut b);
        assert_eq!(a, b);
        assert!(a.iter().any(|v| v.abs() > 0.01));
        // Releases find their group; sustained tails then die away.
        for p in [&mut serial, &mut par] {
            for k in [60, 64, 67, 71, 74] {
                p.note_off(0, k);
            }
            p.midi(&Message::ControlChange {
                channel: 0,
                controller: 123,
                value: 0,
            });
        }
        let mut long = vec![0.0; 48_000 * 2];
        par.render(&mut long);
        assert_eq!(par.active(), 0);
    }
}
