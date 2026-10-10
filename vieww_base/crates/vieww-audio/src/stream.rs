//! Real-time streaming: a lock-free ring buffer between a render thread
//! and a device callback, with underrun and overrun accounting.
//!
//! Offline, the crate renders a whole buffer and hands it over. Live audio
//! cannot: a device asks for a few milliseconds of samples on its own
//! schedule, from a thread that must never block, allocate or wait on a
//! lock — miss its deadline and the listener hears a click. The standard
//! shape is two threads joined by a single-producer, single-consumer ring:
//! the *render* thread runs the graph ahead of time and pushes; the *device*
//! side pops exactly what it needs, and if the ring is empty it plays
//! silence and counts an underrun rather than waiting.
//!
//! [`ring`] is that ring, built from atomics alone (each slot an `AtomicU32`
//! holding an `f32`'s bits; head and tail with acquire/release ordering) —
//! no `unsafe`, no locks, no allocation after construction.
//! [`Stream`] owns a render thread that keeps the ring topped up from any
//! render callback; [`Stream::pull`] is the device side.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Debug)]
struct Shared {
    slots: Box<[AtomicU32]>,
    /// Next slot to write (only the producer stores).
    head: AtomicUsize,
    /// Next slot to read (only the consumer stores).
    tail: AtomicUsize,
}

/// The writing end of a ring.
#[derive(Debug)]
pub struct Producer {
    shared: Arc<Shared>,
}

/// The reading end of a ring.
#[derive(Debug)]
pub struct Consumer {
    shared: Arc<Shared>,
}

/// A single-producer, single-consumer ring of `capacity` samples.
#[must_use]
pub fn ring(capacity: usize) -> (Producer, Consumer) {
    let n = capacity.max(1) + 1; // one slot stays empty to tell full from empty
    let shared = Arc::new(Shared {
        slots: (0..n).map(|_| AtomicU32::new(0)).collect(),
        head: AtomicUsize::new(0),
        tail: AtomicUsize::new(0),
    });
    (
        Producer {
            shared: shared.clone(),
        },
        Consumer { shared },
    )
}

impl Producer {
    /// Free space in samples.
    #[must_use]
    pub fn free(&self) -> usize {
        let n = self.shared.slots.len();
        let h = self.shared.head.load(Ordering::Relaxed);
        let t = self.shared.tail.load(Ordering::Acquire);
        (t + n - h - 1) % n
    }

    /// Push as many of `data` as fit; returns how many were written.
    pub fn push(&mut self, data: &[f32]) -> usize {
        let n = self.shared.slots.len();
        let k = data.len().min(self.free());
        let mut h = self.shared.head.load(Ordering::Relaxed);
        for v in &data[..k] {
            self.shared.slots[h].store(v.to_bits(), Ordering::Relaxed);
            h = (h + 1) % n;
        }
        self.shared.head.store(h, Ordering::Release);
        k
    }
}

impl Consumer {
    /// Samples available to read.
    #[must_use]
    pub fn available(&self) -> usize {
        let n = self.shared.slots.len();
        let h = self.shared.head.load(Ordering::Acquire);
        let t = self.shared.tail.load(Ordering::Relaxed);
        (h + n - t) % n
    }

    /// Pop up to `out.len()` samples; returns how many were read.
    pub fn pop(&mut self, out: &mut [f32]) -> usize {
        let n = self.shared.slots.len();
        let k = out.len().min(self.available());
        let mut t = self.shared.tail.load(Ordering::Relaxed);
        for o in &mut out[..k] {
            *o = f32::from_bits(self.shared.slots[t].load(Ordering::Relaxed));
            t = (t + 1) % n;
        }
        self.shared.tail.store(t, Ordering::Release);
        k
    }
}

/// Counters a stream keeps.
#[derive(Debug, Default)]
pub struct Stats {
    /// Device pulls that found too few samples (silence was played).
    pub underruns: AtomicU64,
    /// Samples handed to the device.
    pub delivered: AtomicU64,
    /// Blocks the render thread produced.
    pub rendered_blocks: AtomicU64,
}

/// A render thread feeding a ring, and the device-side reader.
#[derive(Debug)]
pub struct Stream {
    consumer: Consumer,
    stats: Arc<Stats>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    channels: usize,
}

impl Stream {
    /// Start a render thread calling `render(block)` to fill interleaved
    /// blocks of `block_frames × channels`, keeping up to `buffer_frames`
    /// ahead of the device. `render` runs on its own thread and is handed a
    /// preallocated block — it should not allocate.
    pub fn start<F>(
        channels: usize,
        block_frames: usize,
        buffer_frames: usize,
        mut render: F,
    ) -> Self
    where
        F: FnMut(&mut [f32]) + Send + 'static,
    {
        let ch = channels.max(1);
        let (mut producer, consumer) = ring(buffer_frames.max(block_frames) * ch);
        let stats = Arc::new(Stats::default());
        let stop = Arc::new(AtomicBool::new(false));
        let (s2, st2) = (stats.clone(), stop.clone());
        let thread = std::thread::Builder::new()
            .name("vieww-audio-render".into())
            .spawn(move || {
                let mut block = vec![0.0f32; block_frames.max(1) * ch];
                let mut pending = 0usize; // samples of `block` not yet pushed
                let mut offset = 0usize;
                while !st2.load(Ordering::Acquire) {
                    if pending == 0 {
                        if producer.free() < block.len() {
                            std::thread::sleep(Duration::from_micros(500));
                            continue;
                        }
                        render(&mut block);
                        s2.rendered_blocks.fetch_add(1, Ordering::Relaxed);
                        pending = block.len();
                        offset = 0;
                    }
                    let w = producer.push(&block[offset..offset + pending]);
                    pending -= w;
                    offset += w;
                    if pending > 0 {
                        std::thread::sleep(Duration::from_micros(200));
                    }
                }
            })
            .ok();
        Self {
            consumer,
            stats,
            stop,
            thread,
            channels: ch,
        }
    }

    /// The device side: fill `out` (interleaved). Never blocks: missing
    /// samples become silence and count as an underrun.
    pub fn pull(&mut self, out: &mut [f32]) {
        let got = self.consumer.pop(out);
        if got < out.len() {
            out[got..].fill(0.0);
            self.stats.underruns.fetch_add(1, Ordering::Relaxed);
        }
        self.stats
            .delivered
            .fetch_add(got as u64, Ordering::Relaxed);
    }

    /// Block until `frames` frames are buffered (or `timeout` passes) —
    /// for priming before a device starts.
    pub fn prime(&self, frames: usize, timeout: Duration) -> bool {
        let start = std::time::Instant::now();
        while self.consumer.available() < frames * self.channels {
            if start.elapsed() > timeout {
                return false;
            }
            std::thread::sleep(Duration::from_micros(200));
        }
        true
    }

    /// The counters.
    #[must_use]
    pub fn stats(&self) -> &Stats {
        &self.stats
    }

    /// Stop the render thread.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_preserves_order_and_capacity() {
        let (mut p, mut c) = ring(4);
        assert_eq!(p.push(&[1.0, 2.0, 3.0, 4.0, 5.0]), 4, "full at capacity");
        let mut out = [0.0; 3];
        assert_eq!(c.pop(&mut out), 3);
        assert_eq!(out, [1.0, 2.0, 3.0]);
        assert_eq!(p.push(&[6.0, 7.0, 8.0]), 3);
        let mut out = [0.0; 8];
        assert_eq!(c.pop(&mut out), 4);
        assert_eq!(&out[..4], &[4.0, 6.0, 7.0, 8.0]);
    }

    #[test]
    fn ring_across_threads_loses_nothing() {
        let (mut p, mut c) = ring(1024);
        let n = 500_000u32;
        let t = std::thread::spawn(move || {
            let mut i = 0u32;
            while i < n {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "test counter fits f32 exactly below 2^24"
                )]
                let chunk: Vec<f32> = (i..(i + 97).min(n)).map(|v| v as f32).collect();
                let mut off = 0;
                while off < chunk.len() {
                    off += p.push(&chunk[off..]);
                    std::hint::spin_loop();
                }
                i += 97;
            }
        });
        let mut next = 0u32;
        let mut buf = [0.0f32; 61];
        while next < n {
            let k = c.pop(&mut buf);
            for v in &buf[..k] {
                #[expect(clippy::cast_precision_loss, reason = "test")]
                let want = next as f32;
                assert_eq!(*v, want);
                next += 1;
            }
        }
        t.join().unwrap();
    }

    #[test]
    fn stream_delivers_a_continuous_signal() {
        let mut counter = 0u64;
        let mut s = Stream::start(2, 64, 2048, move |block| {
            for f in block.chunks_exact_mut(2) {
                #[expect(clippy::cast_precision_loss, reason = "test")]
                let v = (counter % 1000) as f32;
                f[0] = v;
                f[1] = -v;
                counter += 1;
            }
        });
        assert!(s.prime(1024, Duration::from_secs(5)));
        let mut expect = 0u64;
        let mut buf = [0.0f32; 2 * 100];
        for _ in 0..40 {
            s.pull(&mut buf);
            for f in buf.chunks_exact(2) {
                #[expect(clippy::cast_precision_loss, reason = "test")]
                let want = (expect % 1000) as f32;
                assert_eq!(f[0], want);
                assert_eq!(f[1], -want);
                expect += 1;
            }
            std::thread::sleep(Duration::from_micros(300));
        }
        assert_eq!(s.stats().underruns.load(Ordering::Relaxed), 0);
        s.stop();
    }

    #[test]
    fn starved_device_gets_silence_and_an_underrun() {
        let mut s = Stream::start(1, 64, 128, |b| b.fill(0.5));
        s.stop();
        let mut drained = vec![0.0; 4096];
        s.pull(&mut drained);
        let mut out = [1.0f32; 64];
        s.pull(&mut out);
        assert!(out.iter().all(|v| *v == 0.0));
        assert!(s.stats().underruns.load(Ordering::Relaxed) >= 1);
    }
}
