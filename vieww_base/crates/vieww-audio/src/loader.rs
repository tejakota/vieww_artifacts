//! Loading audio without stalling anyone.
//!
//! [`Loader`] decodes files on a pool of background threads. Asking for a
//! file returns a [`Handle`] at once; the audio or UI thread polls it
//! ([`Handle::poll`] never blocks) or waits with a timeout. Two requests
//! for the same file while it loads share one decode, and a finished
//! decode is cached by weak reference — reused while anyone still holds
//! it, freed when nobody does. A request can be cancelled, reports its
//! progress, and can resample to the engine's rate on the way in, so the
//! audio thread only ever receives buffers it can play.
//!
//! [`DiskStream`] is for files too long to hold in memory (a two-hour
//! stem, a multi-gigabyte sample library): a reader thread decodes a WAV
//! ahead of playback into the lock-free [`ring`], and
//! the audio thread [`read`](DiskStream::read)s from it without blocking,
//! silence and an underrun count standing in for data that is not there
//! yet. Seeking and looping are handled by the reader, and a seek never
//! lets stale audio through.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::codec::{self, CodecError, Decoded};
use crate::resample::Quality;
use crate::stream::{ring, Consumer, Producer};
use crate::wav::{self, WavInfo};

/// Why a load failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// The file could not be read.
    Io(String),
    /// It was read but did not decode.
    Codec(CodecError),
    /// It was cancelled.
    Cancelled,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "could not read: {e}"),
            Self::Codec(e) => write!(f, "could not decode: {e}"),
            Self::Cancelled => f.write_str("cancelled"),
        }
    }
}

impl std::error::Error for LoadError {}

/// What a finished load produced.
pub type LoadResult = Result<Arc<Decoded>, LoadError>;

/// How to load.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadOptions {
    /// Resample to this rate (the engine's), if it differs.
    pub target_rate: Option<u32>,
    /// Resampling quality.
    pub quality: Quality,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            target_rate: None,
            quality: Quality::Sinc(16),
        }
    }
}

#[derive(Debug, Default)]
struct Slot {
    result: Mutex<Option<LoadResult>>,
    ready: Condvar,
    done: AtomicBool,
    cancelled: AtomicBool,
    progress: AtomicU32, // 0..=1000
}

impl Slot {
    fn finish(&self, r: LoadResult) {
        if let Ok(mut g) = self.result.lock() {
            *g = Some(r);
        }
        self.progress.store(1_000, Ordering::Release);
        self.done.store(true, Ordering::Release);
        self.ready.notify_all();
    }
}

/// A load in flight (or finished).
#[derive(Debug, Clone)]
pub struct Handle {
    slot: Arc<Slot>,
}

impl Handle {
    /// The result, if it is ready. Never blocks.
    #[must_use]
    pub fn poll(&self) -> Option<LoadResult> {
        if !self.slot.done.load(Ordering::Acquire) {
            return None;
        }
        // Set before `done`, so this lock is uncontended.
        self.slot.result.lock().ok().and_then(|g| g.clone())
    }

    /// Wait up to `timeout` for the result.
    #[must_use]
    pub fn wait(&self, timeout: Duration) -> Option<LoadResult> {
        let g = self.slot.result.lock().ok()?;
        let (g, _) = self
            .slot
            .ready
            .wait_timeout_while(g, timeout, |r| r.is_none())
            .ok()?;
        g.clone()
    }

    /// Whether the load has finished (either way).
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.slot.done.load(Ordering::Acquire)
    }

    /// Progress, 0..=1.
    #[must_use]
    pub fn progress(&self) -> f32 {
        #[expect(clippy::cast_precision_loss, reason = "per-mille")]
        let p = self.slot.progress.load(Ordering::Acquire) as f32 / 1_000.0;
        p
    }

    /// Ask for the load to stop. A load that already finished keeps its
    /// result; otherwise it finishes as [`LoadError::Cancelled`].
    pub fn cancel(&self) {
        self.slot.cancelled.store(true, Ordering::Release);
    }
}

enum Source {
    Path(PathBuf),
    Bytes(Vec<u8>),
}

struct Job {
    source: Source,
    opts: LoadOptions,
    slot: Arc<Slot>,
}

type Cache = Arc<Mutex<HashMap<PathBuf, Weak<Decoded>>>>;
type Inflight = Arc<Mutex<HashMap<PathBuf, Handle>>>;

/// A background loader.
pub struct Loader {
    tx: Option<mpsc::Sender<Job>>,
    workers: Vec<JoinHandle<()>>,
    cache: Cache,
    inflight: Inflight,
}

impl std::fmt::Debug for Loader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Loader")
            .field("threads", &self.workers.len())
            .finish_non_exhaustive()
    }
}

fn run(job: &Job, cache: &Cache, inflight: &Inflight) {
    let slot = &job.slot;
    let cancelled = || slot.cancelled.load(Ordering::Acquire);
    let key = match &job.source {
        Source::Path(p) => Some(p.clone()),
        Source::Bytes(_) => None,
    };
    let result = (|| {
        if cancelled() {
            return Err(LoadError::Cancelled);
        }
        let owned;
        let bytes: &[u8] = match &job.source {
            Source::Path(p) => {
                owned = read_with_progress(p, slot)?;
                &owned
            }
            Source::Bytes(b) => b,
        };
        if cancelled() {
            return Err(LoadError::Cancelled);
        }
        let mut d = codec::decode(bytes).map_err(LoadError::Codec)?;
        slot.progress.store(800, Ordering::Release);
        if let Some(rate) = job.opts.target_rate {
            if cancelled() {
                return Err(LoadError::Cancelled);
            }
            d = d.resampled(rate, job.opts.quality);
        }
        Ok(Arc::new(d))
    })();
    if let (Some(k), Ok(d)) = (&key, &result) {
        if let Ok(mut c) = cache.lock() {
            // The insert is also the cheap moment to sweep: loads are rare
            // and IO-bound, and every entry whose last holder dropped in
            // the meantime goes out with this one's arrival. Without a
            // sweep the map only ever grows — one dead key per file ever
            // loaded, held for the loader's lifetime.
            c.retain(|_, w| w.strong_count() > 0);
            c.insert(k.clone(), Arc::downgrade(d));
        }
    }
    if let Some(k) = &key {
        if let Ok(mut i) = inflight.lock() {
            i.remove(k);
        }
    }
    slot.finish(result);
}

fn read_with_progress(p: &Path, slot: &Slot) -> Result<Vec<u8>, LoadError> {
    let mut f = std::fs::File::open(p).map_err(|e| LoadError::Io(e.to_string()))?;
    let total = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mut out = Vec::with_capacity(usize::try_from(total).unwrap_or(0));
    let mut buf = vec![0u8; 1 << 16];
    loop {
        if slot.cancelled.load(Ordering::Acquire) {
            return Err(LoadError::Cancelled);
        }
        let n = f.read(&mut buf).map_err(|e| LoadError::Io(e.to_string()))?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
        if let Some(pm) = (out.len() as u64 * 700).checked_div(total) {
            let pm = pm.min(700);
            slot.progress
                .store(u32::try_from(pm).unwrap_or(700), Ordering::Release);
        }
    }
    Ok(out)
}

impl Loader {
    /// A loader with `threads` decoding threads (at least one).
    #[must_use]
    pub fn new(threads: usize) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        let cache: Cache = Arc::default();
        let inflight: Inflight = Arc::default();
        let workers = (0..threads.max(1))
            .filter_map(|k| {
                let (rx, cache, inflight) = (rx.clone(), cache.clone(), inflight.clone());
                std::thread::Builder::new()
                    .name(format!("vieww-loader-{k}"))
                    .spawn(move || loop {
                        let job = match rx.lock() {
                            Ok(r) => r.recv(),
                            Err(_) => return,
                        };
                        let Ok(job) = job else { return };
                        run(&job, &cache, &inflight);
                    })
                    .ok()
            })
            .collect();
        Self {
            tx: Some(tx),
            workers,
            cache,
            inflight,
        }
    }

    fn submit(&self, source: Source, opts: LoadOptions) -> Handle {
        let slot = Arc::new(Slot::default());
        let h = Handle { slot: slot.clone() };
        let job = Job { source, opts, slot };
        let sent = self.tx.as_ref().is_some_and(|tx| tx.send(job).is_ok());
        if !sent {
            h.slot
                .finish(Err(LoadError::Io("the loader has shut down".into())));
        }
        h
    }

    /// Load a file. A file that is cached or already loading is shared.
    pub fn load(&self, path: impl AsRef<Path>, opts: LoadOptions) -> Handle {
        let path = path.as_ref().to_path_buf();
        if opts.target_rate.is_none() {
            if let Some(d) = self.cached(&path) {
                let slot = Arc::new(Slot::default());
                slot.finish(Ok(d));
                return Handle { slot };
            }
            if let Ok(mut i) = self.inflight.lock() {
                if let Some(h) = i.get(&path) {
                    if !h.slot.cancelled.load(Ordering::Acquire) {
                        return h.clone();
                    }
                }
                let h = self.submit(Source::Path(path.clone()), opts);
                i.insert(path, h.clone());
                return h;
            }
        }
        self.submit(Source::Path(path), opts)
    }

    /// Decode bytes already in memory (not cached).
    pub fn load_bytes(&self, bytes: Vec<u8>, opts: LoadOptions) -> Handle {
        self.submit(Source::Bytes(bytes), opts)
    }

    /// A cached decode of `path`, if someone still holds it. A dead entry
    /// is removed on sight rather than left to accumulate.
    #[must_use]
    pub fn cached(&self, path: impl AsRef<Path>) -> Option<Arc<Decoded>> {
        let mut c = self.cache.lock().ok()?;
        let d = c.get(path.as_ref()).and_then(Weak::upgrade);
        if d.is_none() {
            // Absent or dead — the remove is a no-op in the first case, a
            // prune in the second.
            c.remove(path.as_ref());
        }
        d
    }

    /// Forget the cache (buffers in use stay alive with their users).
    pub fn clear_cache(&self) {
        if let Ok(mut c) = self.cache.lock() {
            c.clear();
        }
    }

    /// How many entries the cache holds, dead or alive — the growth the
    /// pruning keeps in check.
    #[cfg(test)]
    fn cache_entries(&self) -> usize {
        self.cache.lock().map(|c| c.len()).unwrap_or(0)
    }
}

impl Drop for Loader {
    fn drop(&mut self) {
        self.tx = None; // closes the channel: workers exit after their job
        for w in self.workers.drain(..) {
            w.join().ok();
        }
    }
}

struct StreamShared {
    stop: AtomicBool,
    looping: AtomicBool,
    ended: AtomicBool,
    seek_gen: AtomicU64,
    seek_ack: AtomicU64,
    seek_to: AtomicU64,
}

/// Disk streaming of a WAV file.
pub struct DiskStream {
    consumer: Consumer,
    shared: Arc<StreamShared>,
    thread: Option<JoinHandle<()>>,
    info: WavInfo,
    frames: u64,
    position: u64,
    underruns: u64,
}

impl std::fmt::Debug for DiskStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiskStream")
            .field("info", &self.info)
            .field("frames", &self.frames)
            .field("position", &self.position)
            .finish_non_exhaustive()
    }
}

impl DiskStream {
    /// Open `path`, keeping up to `buffer_frames` decoded ahead.
    ///
    /// # Errors
    /// The file cannot be opened, or is not a WAV this crate decodes.
    pub fn open(path: impl AsRef<Path>, buffer_frames: usize) -> std::io::Result<Self> {
        let mut f = std::fs::File::open(path)?;
        let len = f.metadata()?.len();
        let mut head = vec![0u8; 64 * 1024];
        let n = read_fully(&mut f, &mut head)?;
        head.truncate(n);
        let (info, data_off, data_len) = wav::read_wav_header(&head, len)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        let ch = usize::from(info.channels);
        let frame_bytes = (info.format.bytes() * ch) as u64;
        let frames = data_len / frame_bytes;
        let (producer, consumer) = ring(buffer_frames.max(256) * ch);
        let shared = Arc::new(StreamShared {
            stop: AtomicBool::new(false),
            looping: AtomicBool::new(false),
            ended: AtomicBool::new(false),
            seek_gen: AtomicU64::new(0),
            seek_ack: AtomicU64::new(0),
            seek_to: AtomicU64::new(0),
        });
        let s2 = shared.clone();
        let cap = buffer_frames.max(256) * ch;
        let thread = std::thread::Builder::new()
            .name("vieww-disk-stream".into())
            .spawn(move || {
                // Feeds a real-time reader but blocks on disk: elevated,
                // never real-time.
                let _rt = crate::rt::promote_helper_thread();
                reader(f, producer, &s2, info, data_off, frames, cap);
            })
            .ok();
        Ok(Self {
            consumer,
            shared,
            thread,
            info,
            frames,
            position: 0,
            underruns: 0,
        })
    }

    /// The file's format.
    #[must_use]
    pub const fn info(&self) -> WavInfo {
        self.info
    }

    /// Total frames in the file.
    #[must_use]
    pub const fn frames(&self) -> u64 {
        self.frames
    }

    /// Frames delivered since the start (or the last seek).
    #[must_use]
    pub const fn position(&self) -> u64 {
        self.position
    }

    /// Reads that came up short.
    #[must_use]
    pub const fn underruns(&self) -> u64 {
        self.underruns
    }

    /// Loop at the end of the file.
    pub fn set_looping(&self, on: bool) {
        self.shared.looping.store(on, Ordering::Release);
    }

    /// Jump to `frame`. Audio from before the seek is never delivered
    /// after it; until the reader catches up, reads return silence.
    pub fn seek(&mut self, frame: u64) {
        self.shared
            .seek_to
            .store(frame.min(self.frames), Ordering::Release);
        self.shared.seek_gen.fetch_add(1, Ordering::AcqRel);
        self.shared.ended.store(false, Ordering::Release);
        self.position = frame.min(self.frames);
    }

    fn seeking(&self) -> bool {
        self.shared.seek_gen.load(Ordering::Acquire) != self.shared.seek_ack.load(Ordering::Acquire)
    }

    /// Fill `out` (interleaved in the file's channel count) and return the
    /// samples that were real audio. Never blocks; never allocates.
    pub fn read(&mut self, out: &mut [f32]) -> usize {
        // Count what is buffered *before* asking whether the seek is done:
        // the reader pushes post-seek audio only after acknowledging, so
        // if the acknowledgement has not happened yet, everything counted
        // is stale — and nothing pushed after it is touched.
        let mut stale = self.consumer.available();
        if self.seeking() {
            let mut sink = [0.0f32; 256];
            while stale > 0 {
                let k = stale.min(sink.len());
                stale -= self.consumer.pop(&mut sink[..k]);
            }
            out.fill(0.0);
            return 0;
        }
        let got = self.consumer.pop(out);
        out[got..].fill(0.0);
        if got < out.len() && !self.is_finished() {
            self.underruns += 1;
        }
        self.position += (got / usize::from(self.info.channels.max(1))) as u64;
        got
    }

    /// Whether the file has played out (not looping, all delivered).
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.shared.ended.load(Ordering::Acquire)
            && !self.seeking()
            && self.consumer.available() == 0
    }

    /// Wait until `frames` are buffered (or the file is shorter), up to
    /// `timeout`.
    #[must_use]
    pub fn prime(&self, frames: usize, timeout: Duration) -> bool {
        let start = std::time::Instant::now();
        let want = frames * usize::from(self.info.channels.max(1));
        while self.consumer.available() < want && !self.shared.ended.load(Ordering::Acquire) {
            if start.elapsed() > timeout {
                return false;
            }
            std::thread::sleep(Duration::from_micros(200));
        }
        true
    }
}

impl Drop for DiskStream {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            t.join().ok();
        }
    }
}

fn read_fully(f: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

fn reader(
    mut f: std::fs::File,
    mut producer: Producer,
    s: &StreamShared,
    info: WavInfo,
    data_off: u64,
    frames: u64,
    capacity: usize,
) {
    let ch = usize::from(info.channels);
    let width = info.format.bytes();
    let chunk_frames = 4_096usize;
    let mut bytes = vec![0u8; chunk_frames * ch * width];
    let mut samples = vec![0.0f32; chunk_frames * ch];
    let mut pos: u64 = 0; // next frame to read
    let mut pending = 0usize;
    let mut offset = 0usize;
    let mut gen_seen = 0u64;
    if f.seek(SeekFrom::Start(data_off)).is_err() {
        s.ended.store(true, Ordering::Release);
        return;
    }
    while !s.stop.load(Ordering::Acquire) {
        let gen = s.seek_gen.load(Ordering::Acquire);
        if gen != gen_seen {
            // Stop pushing stale audio; wait for the reader side to drain.
            pending = 0;
            if producer.free() < capacity {
                std::thread::sleep(Duration::from_micros(200));
                continue;
            }
            pos = s.seek_to.load(Ordering::Acquire).min(frames);
            let at = data_off + pos * (ch * width) as u64;
            if f.seek(SeekFrom::Start(at)).is_err() {
                pos = frames;
            }
            gen_seen = gen;
            s.ended.store(false, Ordering::Release);
            s.seek_ack.store(gen, Ordering::Release);
            continue;
        }
        if pending == 0 {
            if pos >= frames {
                if s.looping.load(Ordering::Acquire) && frames > 0 {
                    pos = 0;
                    if f.seek(SeekFrom::Start(data_off)).is_err() {
                        break;
                    }
                } else {
                    s.ended.store(true, Ordering::Release);
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                }
            }
            let want = usize::try_from((frames - pos).min(chunk_frames as u64)).unwrap_or(0);
            let nb = want * ch * width;
            let got = read_fully(&mut f, &mut bytes[..nb]).unwrap_or(0);
            let got_frames = got / (ch * width);
            if got_frames == 0 {
                pos = frames; // the file was shorter than its header said
                continue;
            }
            for (o, b) in samples
                .iter_mut()
                .zip(bytes[..got_frames * ch * width].chunks_exact(width))
            {
                *o = info.format.decode_le(b);
            }
            pos += got_frames as u64;
            pending = got_frames * ch;
            offset = 0;
        }
        let w = producer.push(&samples[offset..offset + pending]);
        pending -= w;
        offset += w;
        if pending > 0 {
            std::thread::sleep(Duration::from_micros(300));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wav::SampleFormat;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    fn temp_wav(name: &str, frames: usize, ch: u16, fmt: SampleFormat) -> (PathBuf, Vec<f32>) {
        let data: Vec<f32> = (0..frames * usize::from(ch))
            .map(|i| ((i as f32) * 0.001).sin() * 0.5)
            .collect();
        let m = crate::channels::MultiSamples {
            data: data.clone(),
            layout: crate::channels::Layout::for_channels(usize::from(ch)),
            rate: 48_000,
        };
        let bytes = wav::write_wav_format(&m, fmt).unwrap();
        let p = std::env::temp_dir().join(format!("vieww-{}-{name}.wav", std::process::id()));
        std::fs::write(&p, bytes).unwrap();
        let back = wav::read_wav_multi(&std::fs::read(&p).unwrap())
            .unwrap()
            .0
            .data;
        (p, back)
    }

    #[test]
    fn loads_in_the_background_like_a_direct_decode() {
        let loader = Loader::new(2);
        let h = loader.load(fixture("src16.flac"), LoadOptions::default());
        let got = h.wait(Duration::from_secs(30)).unwrap().unwrap();
        let direct = codec::decode(&std::fs::read(fixture("src16.flac")).unwrap()).unwrap();
        assert_eq!(*got, direct);
        assert!(h.is_done());
        assert_eq!(h.progress(), 1.0);
        assert!(h.poll().unwrap().is_ok());
    }

    #[test]
    fn the_same_file_is_decoded_once_and_cached_weakly() {
        let loader = Loader::new(2);
        let a = loader.load(fixture("src.ogg"), LoadOptions::default());
        let b = loader.load(fixture("src.ogg"), LoadOptions::default());
        let da = a.wait(Duration::from_secs(30)).unwrap().unwrap();
        let db = b.wait(Duration::from_secs(30)).unwrap().unwrap();
        assert!(Arc::ptr_eq(&da, &db));
        // Cached while held…
        let c = loader.load(fixture("src.ogg"), LoadOptions::default());
        assert!(Arc::ptr_eq(&c.poll().unwrap().unwrap(), &da));
        assert!(loader.cached(fixture("src.ogg")).is_some());
        // …and gone when nobody holds it.
        drop((a, b, c, da, db));
        // (The worker drops its own reference just after publishing.)
        let t0 = std::time::Instant::now();
        while loader.cached(fixture("src.ogg")).is_some() && t0.elapsed().as_secs() < 5 {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(loader.cached(fixture("src.ogg")).is_none());
    }

    #[test]
    fn dead_cache_entries_are_pruned_not_accumulated() {
        // The cache is weak, but a dead entry used to stay in the map
        // forever — one stale key per file ever loaded, held for the
        // loader's lifetime. Now a lookup removes what it cannot upgrade,
        // and every completed load sweeps the rest.
        let loader = Loader::new(1);
        let (p1, _) = temp_wav("prune1", 100, 1, SampleFormat::F32);
        let (p2, _) = temp_wav("prune2", 100, 1, SampleFormat::F32);
        let a = loader.load(&p1, LoadOptions::default());
        let d1 = a.wait(Duration::from_secs(30)).unwrap().unwrap();
        assert_eq!(loader.cache_entries(), 1, "held: in the cache");
        drop((a, d1));
        let t0 = std::time::Instant::now();
        while loader.cached(&p1).is_some() && t0.elapsed().as_secs() < 5 {
            std::thread::sleep(Duration::from_millis(1));
        }
        // The lookup pruned the dead entry on sight.
        assert_eq!(loader.cache_entries(), 0, "pruned by the lookup");
        // And a completed load sweeps entries nobody looked up: hold p1
        // again, drop it without a lookup, then load p2 — the sweep at
        // p2's insert retires p1's dead entry with it.
        let a = loader.load(&p1, LoadOptions::default());
        let d1 = a.wait(Duration::from_secs(30)).unwrap().unwrap();
        drop((a, d1));
        let t0 = std::time::Instant::now();
        while loader.cache_entries() == 0 && t0.elapsed().as_secs() < 5 {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(loader.cache_entries(), 1, "dead but still listed");
        let b = loader.load(&p2, LoadOptions::default());
        let d2 = b.wait(Duration::from_secs(30)).unwrap().unwrap();
        assert_eq!(
            loader.cache_entries(),
            1,
            "the load swept the dead entry: only p2 remains"
        );
        assert!(loader.cached(&p2).is_some());
        assert!(loader.cached(&p1).is_none());
        drop((b, d2));
    }

    #[test]
    fn resamples_on_the_way_in_and_reports_errors() {
        let loader = Loader::new(1);
        let h = loader.load(
            fixture("source16.wav"),
            LoadOptions {
                target_rate: Some(48_000),
                quality: Quality::Cubic,
            },
        );
        let d = h.wait(Duration::from_secs(30)).unwrap().unwrap();
        let src = codec::decode(&std::fs::read(fixture("source16.wav")).unwrap()).unwrap();
        assert_eq!(d.rate, 48_000);
        let want = (src.frames() as f64 * 48_000.0 / f64::from(src.rate)).round() as usize;
        assert_eq!(d.frames(), want);

        let missing = loader.load("/definitely/not/here.wav", LoadOptions::default());
        assert!(matches!(
            missing.wait(Duration::from_secs(5)),
            Some(Err(LoadError::Io(_)))
        ));
        let junk = loader.load_bytes(b"not audio at all".to_vec(), LoadOptions::default());
        assert!(matches!(
            junk.wait(Duration::from_secs(5)),
            Some(Err(LoadError::Codec(_)))
        ));
    }

    #[test]
    fn a_queued_load_can_be_cancelled() {
        let loader = Loader::new(1);
        // Occupy the only worker with a large decode…
        let big = crate::channels::MultiSamples {
            data: vec![0.25; 48_000 * 2 * 20],
            layout: crate::channels::Layout::Stereo,
            rate: 48_000,
        };
        let bytes = wav::write_wav_format(&big, SampleFormat::I24).unwrap();
        let busy = loader.load_bytes(
            bytes,
            LoadOptions {
                target_rate: Some(44_100),
                quality: Quality::Sinc(16),
            },
        );
        // …so this one is still queued when it is cancelled.
        let queued = loader.load_bytes(vec![1, 2, 3], LoadOptions::default());
        queued.cancel();
        assert_eq!(
            queued.wait(Duration::from_secs(60)),
            Some(Err(LoadError::Cancelled))
        );
        assert!(busy.wait(Duration::from_secs(60)).unwrap().is_ok());
    }

    #[test]
    fn disk_stream_delivers_the_file_exactly() {
        for (fmt, ch) in [
            (SampleFormat::I16, 2),
            (SampleFormat::I24, 1),
            (SampleFormat::F32, 6),
        ] {
            let (p, want) = temp_wav(&format!("exact{ch}"), 30_000, ch, fmt);
            let mut s = DiskStream::open(&p, 2_048).unwrap();
            assert_eq!(s.frames(), 30_000);
            assert_eq!(s.info().channels, ch);
            let mut got = Vec::new();
            let mut buf = vec![0.0; 512 * usize::from(ch)];
            let t0 = std::time::Instant::now();
            while !s.is_finished() && t0.elapsed().as_secs() < 20 {
                let n = s.read(&mut buf);
                got.extend_from_slice(&buf[..n]);
                if n == 0 {
                    std::thread::sleep(Duration::from_micros(200));
                }
            }
            assert_eq!(got, want, "{fmt:?}");
            assert_eq!(s.position(), 30_000);
            std::fs::remove_file(&p).ok();
        }
    }

    #[test]
    fn disk_stream_seeks_without_stale_audio_and_loops() {
        let (p, want) = temp_wav("seek", 20_000, 2, SampleFormat::F32);
        let mut s = DiskStream::open(&p, 4_096).unwrap();
        assert!(s.prime(1_000, Duration::from_secs(10)));
        let mut buf = vec![0.0; 200];
        let _ = s.read(&mut buf);
        s.seek(15_000);
        // Until the seek completes reads are silent; then audio resumes at
        // exactly frame 15 000.
        let t0 = std::time::Instant::now();
        let mut first = Vec::new();
        while first.is_empty() && t0.elapsed().as_secs() < 10 {
            let n = s.read(&mut buf);
            first.extend_from_slice(&buf[..n]);
        }
        assert_eq!(first[..], want[30_000..30_000 + first.len()]);

        s.set_looping(true);
        let mut total = first.len();
        let mut wrapped = false;
        let t0 = std::time::Instant::now();
        while total < 2 * 20_000 * 2 && t0.elapsed().as_secs() < 10 {
            let n = s.read(&mut buf);
            if n > 0 {
                let start = (30_000 + total) % (20_000 * 2);
                if start + n <= want.len() {
                    assert_eq!(buf[..n], want[start..start + n]);
                }
                wrapped |= 30_000 + total + n > 40_000;
            }
            total += n;
        }
        assert!(wrapped);
        assert!(!s.is_finished());
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn disk_stream_rejects_non_wav() {
        assert!(DiskStream::open(fixture("src.mp3"), 1_024).is_err());
        assert!(DiskStream::open("/no/such/file.wav", 1_024).is_err());
    }
}
