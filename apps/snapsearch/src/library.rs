//! Getting real photos into the app.
//!
//! # Where photos come from
//!
//! In order: `$SNAPSEARCH_PHOTOS`, then the platform's own pictures folder.
//! If neither has images, the app opens on the generated sample library and
//! says so in the header — a fresh checkout should still show something, but
//! it should never pretend a gradient is a photograph.
//!
//! Files can also be dropped onto the window at any time; the platform layer
//! gathers them into `FrameDriver::take_dropped_files()` and they join the
//! same queue as a scan.
//!
//! # Why the work is on another thread
//!
//! Decoding a JPEG is milliseconds and embedding it through CLIP is tens of
//! milliseconds. A thousand photos is therefore a minute of work, and doing
//! it on the frame thread means a minute of frozen application. [`Loader`]
//! hands the whole job — read, decode, downscale, embed — to
//! `vieww_foundation::task::Spawn`, and the UI thread does nothing but drain
//! finished photos in `pump`, a few per frame, so the grid fills in while
//! staying responsive.
//!
//! That is also why [`crate::embed::Embedder`] is `Send + Sync`: the
//! embedding happens on the worker, not here.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use vieww::foundation::task::Spawn;

use crate::embed::Embedder;
use crate::photo::{label_from_path, Image_, Photo, Source, THUMB_MAX};

/// Extensions worth trying to decode. Matches what `vieww_asset` supports.
const EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];

/// How deep a scan walks. Photo libraries nest by year and event; they do
/// not nest twenty deep, and an unbounded walk on a home directory is how
/// this would appear to hang.
const MAX_DEPTH: usize = 4;

/// The most photos a single scan will queue.
///
/// A stated limit rather than a silent one: `Loader::skipped` reports what
/// was left out, and the UI says so. Truncating without saying is how a
/// library quietly looks smaller than it is.
pub const SCAN_LIMIT: usize = 2000;

/// How many finished photos are taken per frame.
///
/// Building the `Image` and inserting into the index is UI-thread work, so
/// it is spread out. Twelve a frame fills a screen in well under a second
/// and never costs a visible hitch.
const DRAIN_PER_FRAME: usize = 12;

/// A decoded, embedded photo on its way back from a worker.
///
/// Deliberately plain data: `Image_` is `Arc`-backed and crosses threads,
/// but `Photo` is only assembled on the UI thread.
pub struct Loaded {
    pub id: String,
    pub label: String,
    pub image: Image_,
    pub aspect: f32,
    pub path: PathBuf,
    pub vector: Vec<f32>,
    /// True when the vector came out of the cache rather than the model.
    pub cached: bool,
}

/// Whatever went wrong with one file, kept rather than swallowed.
pub struct Failed {
    pub path: PathBuf,
    pub why: String,
}

pub enum Message {
    Ready(Box<Loaded>),
    Failed(Box<Failed>),
}

/// Where this run's photos are coming from, for the UI to say plainly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// A real directory.
    Directory(PathBuf),
    /// Nothing was found; these are generated.
    Samples,
}

/// The photo directory to use, and how it was chosen.
pub fn photo_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SNAPSEARCH_PHOTOS") {
        let dir = PathBuf::from(dir);
        return dir.is_dir().then_some(dir);
    }
    default_pictures_dir().filter(|dir| dir.is_dir())
}

/// The platform's pictures folder.
///
/// Android's is `DCIM` under external storage, which an app can only read
/// once `READ_MEDIA_IMAGES` has been granted — `vieww` has no permission
/// service, so on a phone this will usually come back empty and the app will
/// fall back to samples. That is a real gap, and it is written down in the
/// README rather than papered over.
fn default_pictures_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        return Some(PathBuf::from("/sdcard/DCIM"));
    }
    #[cfg(not(target_os = "android"))]
    {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
        Some(PathBuf::from(home).join("Pictures"))
    }
}

/// Every image file under `dir`, breadth-limited and depth-limited.
///
/// Sorted, so the grid is stable between launches rather than reshuffling
/// with whatever order the filesystem happened to return.
pub fn scan(dir: &Path, limit: usize) -> (Vec<PathBuf>, usize) {
    let mut found = Vec::new();
    let mut skipped = 0usize;
    let mut queue = vec![(dir.to_path_buf(), 0usize)];

    while let Some((current, depth)) = queue.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                // Symlinked directories are not followed: a link back up is
                // an infinite walk, and photo libraries do not need them.
                if depth + 1 < MAX_DEPTH && !kind.is_symlink() {
                    queue.push((path, depth + 1));
                }
            } else if is_image(&path) {
                if found.len() < limit {
                    found.push(path);
                } else {
                    skipped += 1;
                }
            }
        }
    }

    found.sort();
    (found, skipped)
}

fn is_image(path: &Path) -> bool {
    // Hidden files are skipped the way any photo browser skips them: a
    // `.thumbnails` cache or a stray `._IMG_0001.jpg` resource fork is not
    // a photograph, and indexing them is work with a wrong answer at the
    // end of it.
    let hidden = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with('.'));
    if hidden {
        return false;
    }
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| EXTENSIONS.contains(&e.as_str()))
}

/// A cache key that changes when the file does.
///
/// Path, size and modification time. Not a content hash: hashing every file
/// would mean reading every file, which is most of the work the cache exists
/// to avoid.
pub fn cache_key(path: &Path, embedder_name: &str) -> String {
    let meta = std::fs::metadata(path).ok();
    let len = meta.as_ref().map_or(0, |m| m.len());
    let modified = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    // The encoder is part of the key: a vector from the local encoder means
    // nothing to CLIP, and serving one to the other silently produces
    // confident nonsense.
    format!("v1:{embedder_name}:{}:{len}:{modified}", path.display())
}

/// Vectors are stored as hex of their little-endian bytes — lossless,
/// fixed-width, and it survives a `String`-valued store without a base64
/// dependency.
pub fn encode_vector(v: &[f32]) -> String {
    let mut out = String::with_capacity(v.len() * 8);
    for x in v {
        for byte in x.to_le_bytes() {
            out.push_str(&format!("{byte:02x}"));
        }
    }
    out
}

pub fn decode_vector(s: &str) -> Option<Vec<f32>> {
    if s.len() % 8 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 8);
    for chunk in s.as_bytes().chunks_exact(8) {
        let text = std::str::from_utf8(chunk).ok()?;
        let mut bytes = [0u8; 4];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?;
        }
        out.push(f32::from_le_bytes(bytes));
    }
    Some(out)
}

/// What a worker needs to look something up and write it back.
///
/// A plain pair of closures rather than the `Storage` trait itself, because
/// `Storage` is not `Send` — it is a UI-thread service. The UI thread reads
/// the whole cache once at start-up and hands the workers a snapshot plus a
/// channel for what they learn.
pub type CacheLookup = Arc<dyn Fn(&str) -> Option<Vec<f32>> + Send + Sync>;

/// Runs the load queue and hands finished photos back.
pub struct Loader {
    tx: Sender<Message>,
    rx: Receiver<Message>,
    spawner: Arc<dyn Spawn>,
    embedder: Arc<dyn Embedder>,
    cache: CacheLookup,
    /// Still in flight — what the progress line counts down.
    outstanding: usize,
    /// Files a scan refused to queue, so the UI can say so.
    pub skipped: usize,
    pub failures: Vec<Failed>,
}

impl Loader {
    pub fn new(
        spawner: Arc<dyn Spawn>,
        embedder: Arc<dyn Embedder>,
        cache: CacheLookup,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            spawner,
            embedder,
            cache,
            outstanding: 0,
            skipped: 0,
            failures: Vec::new(),
        }
    }

    pub const fn outstanding(&self) -> usize {
        self.outstanding
    }

    pub const fn is_idle(&self) -> bool {
        self.outstanding == 0
    }

    /// Queue `paths`, one task each.
    pub fn enqueue(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            let tx = self.tx.clone();
            let embedder = self.embedder.clone();
            let cache = self.cache.clone();
            self.outstanding += 1;
            self.spawner.spawn(Box::new(move || {
                let message = load_one(&path, embedder.as_ref(), cache.as_ref());
                // A closed receiver means the app is shutting down; there is
                // nobody to tell, and that is not an error.
                let _ = tx.send(message);
            }));
        }
    }

    /// Take up to [`DRAIN_PER_FRAME`] finished photos.
    ///
    /// Returns what is ready to go into the grid and the index. Failures are
    /// accumulated on `self` rather than returned, because the screen
    /// reports them as a count, not one at a time.
    pub fn pump(&mut self) -> Vec<Loaded> {
        let mut ready = Vec::new();
        while ready.len() < DRAIN_PER_FRAME {
            match self.rx.try_recv() {
                Ok(Message::Ready(loaded)) => {
                    self.outstanding = self.outstanding.saturating_sub(1);
                    ready.push(*loaded);
                }
                Ok(Message::Failed(failed)) => {
                    self.outstanding = self.outstanding.saturating_sub(1);
                    self.failures.push(*failed);
                }
                Err(_) => break,
            }
        }
        ready
    }
}

/// Read, decode, downscale, embed — all of it off the UI thread.
fn load_one(
    path: &Path,
    embedder: &dyn Embedder,
    cache: &(dyn Fn(&str) -> Option<Vec<f32>> + Send + Sync),
) -> Message {
    let fail = |why: String| {
        Message::Failed(Box::new(Failed {
            path: path.to_path_buf(),
            why,
        }))
    };

    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return fail(format!("reading: {error}")),
    };

    // `decode_sized`, not `decode`: a 48-megapixel phone photo decoded at
    // full size is 190MB of RGBA, and a few of those is the whole heap.
    let (image, _format) = match vieww_asset::decode_sized(&bytes, THUMB_MAX, THUMB_MAX) {
        Ok(decoded) => decoded,
        Err(error) => return fail(format!("decoding: {error}")),
    };

    let aspect = if image.width() == 0 {
        1.0
    } else {
        (image.height() as f32 / image.width() as f32).clamp(0.5, 2.2)
    };

    let key = cache_key(path, embedder.name());
    let (vector, cached) = match cache(&key) {
        // A cached vector of the wrong width is from an encoder that has
        // since changed shape. Recompute rather than trust it.
        Some(v) if v.len() == embedder.dim() => (v, true),
        _ => (embedder.embed_image(&image), false),
    };

    Message::Ready(Box::new(Loaded {
        id: path.display().to_string(),
        label: label_from_path(path),
        image,
        aspect,
        path: path.to_path_buf(),
        vector,
        cached,
    }))
}

impl Loaded {
    /// The `Photo` this becomes on the UI thread.
    pub fn into_photo(self) -> (Photo, Vec<f32>, String) {
        let key = self.id.clone();
        (
            Photo {
                id: self.id,
                label: self.label,
                image: self.image,
                aspect: self.aspect,
                source: Source::File(self.path),
            },
            self.vector,
            key,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vector_survives_a_round_trip_through_the_cache_encoding() {
        let v = vec![0.0, 1.0, -0.5, 3.141_592_7, f32::MIN_POSITIVE];
        let encoded = encode_vector(&v);
        assert_eq!(decode_vector(&encoded).unwrap(), v, "must be lossless");
    }

    #[test]
    fn a_truncated_cache_entry_is_rejected_rather_than_half_decoded() {
        assert!(decode_vector("abc").is_none());
        assert!(decode_vector("zzzzzzzz").is_none(), "non-hex");
    }

    #[test]
    fn the_cache_key_changes_with_the_encoder() {
        let path = Path::new("/tmp/nonexistent-snapsearch-test.jpg");
        assert_ne!(
            cache_key(path, "local appearance"),
            cache_key(path, "CLIP ViT-B/32"),
            "one encoder's vectors are meaningless to another"
        );
    }

    #[test]
    fn only_image_extensions_are_picked_up() {
        assert!(is_image(Path::new("a/b/c.JPG")));
        assert!(is_image(Path::new("a/b/c.png")));
        assert!(!is_image(Path::new("a/b/c.txt")));
        assert!(!is_image(Path::new("a/b/c")));
        assert!(!is_image(Path::new("a/b/._IMG_0001.jpg")), "hidden files are skipped");
    }

    #[test]
    fn a_scan_reports_what_it_refused_to_queue() {
        let dir = std::env::temp_dir().join("snapsearch-scan-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..5 {
            std::fs::write(dir.join(format!("p{i}.png")), b"not really a png").unwrap();
        }
        std::fs::write(dir.join("notes.txt"), b"ignored").unwrap();

        let (found, skipped) = scan(&dir, 3);
        assert_eq!(found.len(), 3, "limit honoured");
        assert_eq!(skipped, 2, "and the remainder is reported, not hidden");
        assert!(found.iter().all(|p| p.extension().unwrap() == "png"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod integration {
    use super::*;
    use crate::embed::local::LocalEmbedder;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A handful of real PNG files on disk.
    fn fixture(dir: &Path, count: usize) {
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir).unwrap();
        for i in 0..count {
            let image = crate::embed::local::gradient_image(
                vieww::foundation::Color::rgb(20 + (i as u8) * 20, 90, 200),
                vieww::foundation::Color::rgb(200, 60, 20 + (i as u8) * 15),
                40,
                50,
                0.0,
            );
            // A real encoded PNG, so the decoder is genuinely exercised.
            let png = encode_png(&image);
            std::fs::write(dir.join(format!("shot_{i}.png")), png).unwrap();
        }
    }

    fn encode_png(image: &Image_) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, image.width(), image.height());
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(image.pixels()).unwrap();
        }
        out
    }

    fn drain(loader: &mut Loader) -> Vec<Loaded> {
        let mut all = Vec::new();
        for _ in 0..400 {
            all.extend(loader.pump());
            if loader.is_idle() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        all.extend(loader.pump());
        all
    }

    /// The whole real path: scan a directory, read and decode real PNG
    /// files on worker threads, embed them, and come back with usable
    /// photos — none of which touches the generated sample set.
    #[test]
    fn real_files_are_scanned_decoded_and_embedded() {
        let dir = std::env::temp_dir().join("snapsearch-int-load");
        fixture(&dir, 6);

        let embedder: Arc<dyn Embedder> = Arc::new(LocalEmbedder::new());
        let (paths, skipped) = scan(&dir, SCAN_LIMIT);
        assert_eq!(paths.len(), 6);
        assert_eq!(skipped, 0);

        let mut loader = Loader::new(
            Arc::new(vieww::foundation::task::Threads),
            embedder.clone(),
            Arc::new(|_: &str| None),
        );
        loader.enqueue(paths);
        let loaded = drain(&mut loader);

        assert_eq!(loaded.len(), 6, "failures: {:?}", loader.failures.len());
        for item in &loaded {
            assert_eq!(item.vector.len(), embedder.dim());
            assert!(item.image.width() > 0 && item.image.height() > 0);
            assert!(!item.cached, "nothing was in the cache on a first run");
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// And the second launch: with the cache populated, no photo is
    /// re-embedded. This is what makes CLIP viable on a real library —
    /// without it every start-up re-runs the model over everything.
    #[test]
    fn a_second_run_reads_vectors_from_the_cache_instead_of_re_embedding() {
        let dir = std::env::temp_dir().join("snapsearch-int-cache");
        fixture(&dir, 4);

        let embedder: Arc<dyn Embedder> = Arc::new(LocalEmbedder::new());
        let (paths, _) = scan(&dir, SCAN_LIMIT);

        // ---- first run: nothing cached, everything computed -----------
        let mut loader = Loader::new(
            Arc::new(vieww::foundation::task::Threads),
            embedder.clone(),
            Arc::new(|_: &str| None),
        );
        loader.enqueue(paths.clone());
        let first = drain(&mut loader);
        assert_eq!(first.len(), 4);

        // Whatever the app would have written to `Storage`.
        let store: std::collections::HashMap<String, String> = first
            .iter()
            .map(|item| {
                (
                    cache_key(&item.path, embedder.name()),
                    encode_vector(&item.vector),
                )
            })
            .collect();

        // ---- second run: served from the cache ------------------------
        let embed_calls = Arc::new(AtomicUsize::new(0));
        let counted = embed_calls.clone();
        let lookup: CacheLookup = Arc::new(move |key: &str| {
            counted.fetch_add(1, Ordering::SeqCst);
            store.get(key).and_then(|raw| decode_vector(raw))
        });

        let mut loader = Loader::new(
            Arc::new(vieww::foundation::task::Threads),
            embedder.clone(),
            lookup,
        );
        loader.enqueue(paths);
        let second = drain(&mut loader);

        assert_eq!(second.len(), 4);
        assert!(
            second.iter().all(|item| item.cached),
            "every photo should have come from the cache"
        );
        assert_eq!(embed_calls.load(Ordering::SeqCst), 4, "one lookup each");

        // And the cached vectors are the same ones, not merely the same
        // shape — a cache that returns plausible-but-different vectors
        // would rank differently on every launch.
        for item in &second {
            let original = first
                .iter()
                .find(|f| f.path == item.path)
                .expect("same file");
            assert_eq!(item.vector, original.vector);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file that is not really an image is reported, not swallowed and
    /// not fatal to the rest of the scan.
    #[test]
    fn an_undecodable_file_is_reported_and_the_rest_still_load() {
        let dir = std::env::temp_dir().join("snapsearch-int-bad");
        fixture(&dir, 2);
        std::fs::write(dir.join("broken.png"), b"this is not a png").unwrap();

        let embedder: Arc<dyn Embedder> = Arc::new(LocalEmbedder::new());
        let (paths, _) = scan(&dir, SCAN_LIMIT);
        let mut loader = Loader::new(
            Arc::new(vieww::foundation::task::Threads),
            embedder,
            Arc::new(|_: &str| None),
        );
        loader.enqueue(paths);
        let loaded = drain(&mut loader);

        assert_eq!(loaded.len(), 2, "the good files still arrive");
        assert_eq!(loader.failures.len(), 1);
        assert!(loader.failures[0].why.contains("decoding"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
