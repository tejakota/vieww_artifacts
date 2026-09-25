//! One state object, one place — the prototype's rule that all screen state
//! lives in a single object, one field per future `Signal`.
//!
//! `AppState` is `Clone` because every field is `Rc`-backed, so handlers,
//! the frame clock and widget `build`s can all hold one.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use vieww::element::{ScrollController, ScrollTimeline, TimelinePlayer};
use vieww::gestures::ScrollPhysics;
use vieww::prelude::*;

use vieww::foundation::Storage;

use crate::embed::{Embedder, EmbeddingIndex};
use crate::library::{self, Loader, Origin};
use crate::photo::{self, Photo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Text,
    Photo,
}

/// A photo and how well it matched, `0.0..=1.0`.
pub type Match = (Photo, f32);

#[derive(Clone)]
pub struct AppState {
    pub photos: Signal<Rc<Vec<Photo>>>,
    /// `None` until a search has run. `Some(empty)` is a real, different
    /// state — "we looked, and nothing was close enough" — and the screen
    /// says so rather than showing the whole library back.
    pub results: Signal<Option<Rc<Vec<Match>>>>,
    pub mode: Signal<SearchMode>,
    pub text_query: Signal<String>,
    pub ref_photo: Signal<Option<Photo>>,
    pub sheet_open: Signal<bool>,
    pub searching: Signal<bool>,
    pub progress: Signal<f32>,
    pub toast: Signal<Option<String>>,
    /// A shared, slow clock driving the masonry's ambient drift and
    /// live-replace. One signal, and each tile derives its own phase from
    /// its index — the prototype's per-tile `animation-delay` trick.
    pub tick: Signal<u64>,

    /// The encoder behind search, and the index of the library it built.
    ///
    /// Both are plain `Rc`s rather than `Signal`s: nothing rebuilds when
    /// they change because they do not change — the index is built once at
    /// startup and read on every search.
    pub embedder: Arc<dyn Embedder>,

    /// The vector index. Grows as photos arrive, so it is behind a
    /// `RefCell` rather than being built once and frozen.
    pub index: Rc<RefCell<EmbeddingIndex>>,

    /// Where this run's photos came from, so the header can say.
    pub origin: Signal<Origin>,

    /// The background reader/decoder/embedder.
    pub loader: Rc<RefCell<Loader>>,

    /// How many photos are still being read in. Drives the indexing line.
    pub loading: Signal<usize>,

    /// The photo open full-screen, if any.
    pub detail: Signal<Option<Photo>>,

    /// The full-resolution decode of [`Self::detail`], once a worker has
    /// finished it. `None` means "still showing the thumbnail".
    pub detail_full: Signal<Option<crate::photo::Image_>>,

    /// Where background work goes. Held here as well as in the loader
    /// because opening a photo starts a one-off decode.
    pub spawner: Arc<dyn vieww::foundation::task::Spawn>,

    full_rx: Rc<std::sync::mpsc::Receiver<(String, crate::photo::Image_)>>,
    full_tx: std::sync::mpsc::Sender<(String, crate::photo::Image_)>,

    /// Persistent key/value store, when the platform provides one.
    /// `None` on a platform without it — the app then re-embeds every
    /// launch rather than failing.
    pub storage: Option<Rc<dyn Storage>>,

    /// Where the library is scrolled to.
    ///
    /// Lives here rather than on the screen so the frame clock can read it
    /// without going through the widget tree — [`Self::sync_scroll`] needs
    /// it every frame.
    pub scroll: ScrollController,

    /// 0→1 across the first [`HEADER_RANGE`] pixels of scroll, driving the
    /// collapsing header.
    ///
    /// `ScrollTimeline` is **pull-based** — it is never pushed to. Something
    /// has to call `update(offset)`, and that is `sync_scroll`.
    pub header: Rc<ScrollTimeline>,

    /// One reveal value per result slot, staggered 0→1 when a search lands.
    ///
    /// A fixed pool rather than one per photo: the timeline is built once at
    /// start-up, and a timeline holds its `Signal`s by value, so the set it
    /// animates cannot change afterwards.
    pub reveal: Rc<Vec<Signal<f32>>>,

    /// Filled in by `app::build_root`, which is the only place with the
    /// `&mut Tickers` that `Timeline::attach` needs. Every `TimelinePlayer`
    /// method takes `&self`, so from here on it can be driven from a
    /// handler like any other captured value.
    pub reveal_player: Rc<RefCell<Option<TimelinePlayer>>>,

    search_started: Rc<Cell<Option<Instant>>>,
    toast_shown_at: Rc<Cell<u64>>,
}

/// How far the library scrolls before the header is fully collapsed.
pub const HEADER_RANGE: f32 = 150.0;

/// How long between one result tile appearing and the next.
pub const REVEAL_STAGGER_MS: u64 = 45;

/// How long the search animation runs before results appear.
///
/// The local encoder answers in well under a millisecond, so this is
/// deliberate pacing rather than measured work: a result grid that
/// swaps instantly reads as "nothing happened". With CLIP the query
/// encode is real work and lands inside this same window.
const SEARCH_DURATION_SECS: f32 = 1.1;

/// How many ticks a toast stays up.
const TOAST_TICKS: u64 = 12;

/// The most results to show.
const MAX_RESULTS: usize = 12;

impl AppState {
    pub fn new(
        runtime: &Runtime,
        embedder: Arc<dyn Embedder>,
        spawner: Arc<dyn vieww::foundation::task::Spawn>,
        loader: Loader,
        storage: Option<Rc<dyn Storage>>,
    ) -> Self {
        let (full_tx, full_rx) = std::sync::mpsc::channel();
        // Opens on nothing. Either a scan fills it with real photos, or
        // `fall_back_to_samples` puts the generated set in — and the header
        // says which.
        let photos: Rc<Vec<Photo>> = Rc::new(Vec::new());
        let index = Rc::new(RefCell::new(EmbeddingIndex::new()));

        // Android physics on every platform for now: the difference from
        // iOS's is subtle, and the framework's iOS rendering is still
        // unverified, so matching it would be tuning against nothing.
        let scroll = ScrollController::new(runtime, ScrollPhysics::android());

        let reveal = Rc::new(
            (0..MAX_RESULTS)
                .map(|_| runtime.signal(1.0f32))
                .collect::<Vec<_>>(),
        );

        Self {
            origin: runtime.signal(Origin::Samples),
            loader: Rc::new(RefCell::new(loader)),
            loading: runtime.signal(0),
            detail: runtime.signal(None),
            detail_full: runtime.signal(None),
            spawner,
            full_rx: Rc::new(full_rx),
            full_tx,
            storage,
            scroll,
            header: Rc::new(ScrollTimeline::new(runtime, 0.0, HEADER_RANGE)),
            reveal,
            reveal_player: Rc::new(RefCell::new(None)),
            photos: runtime.signal(photos),
            results: runtime.signal(None),
            mode: runtime.signal(SearchMode::Text),
            text_query: runtime.signal(String::new()),
            ref_photo: runtime.signal(None),
            sheet_open: runtime.signal(false),
            searching: runtime.signal(false),
            progress: runtime.signal(0.0),
            toast: runtime.signal(None),
            tick: runtime.signal(0u64),
            embedder,
            index,
            search_started: Rc::new(Cell::new(None)),
            toast_shown_at: Rc::new(Cell::new(0)),
        }
    }

    /// Point the app at a directory and start reading it in.
    pub fn scan_directory(&self, dir: &std::path::Path) {
        let (paths, skipped) = library::scan(dir, library::SCAN_LIMIT);
        if paths.is_empty() {
            self.fall_back_to_samples();
            return;
        }

        self.origin.set(Origin::Directory(dir.to_path_buf()));
        self.photos.set(Rc::new(Vec::new()));
        self.index.borrow_mut().clear();
        self.results.set(None);

        let mut loader = self.loader.borrow_mut();
        loader.skipped = skipped;
        loader.failures.clear();
        loader.enqueue(paths);
        self.loading.set(loader.outstanding());
    }

    /// Nothing to read — show the generated set, and be clear about it.
    pub fn fall_back_to_samples(&self) {
        let samples = photo::sample_library(48);
        self.index
            .replace(EmbeddingIndex::build(self.embedder.as_ref(), &samples));
        self.photos.set(samples);
        self.origin.set(Origin::Samples);
        self.loading.set(0);
    }

    /// Files dropped on the window join the library.
    pub fn import(&self, paths: Vec<std::path::PathBuf>) {
        let paths: Vec<_> = paths.into_iter().filter(|p| p.is_file()).collect();
        if paths.is_empty() {
            return;
        }
        // A drop onto the sample set replaces it: mixing generated tiles
        // with real photos would make the header's "these are samples"
        // note a lie.
        if self.origin.peek() == Origin::Samples {
            self.photos.set(Rc::new(Vec::new()));
            self.index.borrow_mut().clear();
            self.origin.set(Origin::Directory(
                paths[0].parent().map(Into::into).unwrap_or_default(),
            ));
        }
        self.show_toast(format!(
            "Adding {} photo{}",
            paths.len(),
            if paths.len() == 1 { "" } else { "s" }
        ));
        let mut loader = self.loader.borrow_mut();
        loader.enqueue(paths);
        self.loading.set(loader.outstanding());
    }

    /// Take whatever the workers have finished and put it on screen.
    ///
    /// Called once a frame. Keeps the grid filling in while the rest of the
    /// library is still being read.
    pub fn pump_loader(&self) {
        self.pump_detail();
        let ready = self.loader.borrow_mut().pump();
        if ready.is_empty() {
            let outstanding = self.loader.borrow().outstanding();
            if self.loading.peek() != outstanding {
                self.loading.set(outstanding);
            }
            return;
        }

        let mut photos = (*self.photos.peek()).clone();
        {
            let mut index = self.index.borrow_mut();
            for loaded in ready {
                let was_cached = loaded.cached;
                let (photo, vector, key) = loaded.into_photo();

                // Write back what the worker had to compute, so the next
                // launch starts from the cache instead of the model.
                if !was_cached {
                    if let (Some(storage), Some(path)) =
                        (self.storage.as_ref(), photo.source.path())
                    {
                        let cache_key = library::cache_key(path, self.embedder.name());
                        let _ = storage.set(&cache_key, &library::encode_vector(&vector));
                    }
                }

                index.insert(key, vector);
                photos.push(photo);
            }
        }

        self.photos.set(Rc::new(photos));
        self.loading.set(self.loader.borrow().outstanding());
    }

    /// Open a photo full screen, and start decoding it properly.
    pub fn open_detail(&self, photo: Photo) {
        self.detail_full.set(None);

        // Only a real file has a full resolution to go and get; a generated
        // sample is already at its own size.
        if let Some(path) = photo.source.path().cloned() {
            let id = photo.id.clone();
            let tx = self.full_tx.clone();
            self.spawner.spawn(Box::new(move || {
                if let Ok(bytes) = std::fs::read(&path) {
                    // Bounded rather than truly full: a 48-megapixel photo
                    // decoded at native size is ~190MB of RGBA, and nothing
                    // on a phone screen can show it. 2048 is past what any
                    // display here resolves.
                    if let Ok((image, _)) = vieww_asset::decode_sized(&bytes, 2048, 2048) {
                        let _ = tx.send((id, image));
                    }
                }
            }));
        }

        self.detail.set(Some(photo));
    }

    pub fn close_detail(&self) {
        self.detail.set(None);
        self.detail_full.set(None);
    }

    /// Take a finished full-resolution decode, if it belongs to what is
    /// still on screen.
    ///
    /// The identity check is the point: opening one photo, closing it and
    /// opening another leaves the first decode in flight, and dropping it on
    /// arrival is what stops the wrong picture appearing a second later.
    fn pump_detail(&self) {
        while let Ok((id, image)) = self.full_rx.try_recv() {
            if self.detail.peek().is_some_and(|photo| photo.id == id) {
                self.detail_full.set(Some(image));
            }
        }
    }

    pub fn open_sheet(&self) {
        self.sheet_open.set(true);
    }

    pub fn close_sheet(&self) {
        self.sheet_open.set(false);
    }

    pub fn set_mode(&self, mode: SearchMode) {
        self.mode.set(mode);
    }

    pub fn can_search(&self) -> bool {
        match self.mode.peek() {
            SearchMode::Text => !self.text_query.peek().trim().is_empty(),
            SearchMode::Photo => self.ref_photo.peek().is_some(),
        }
    }

    /// Begin a search. The progress bar is advanced by `advance_clock`,
    /// and the results land when it reaches 1.0.
    pub fn run_search(&self) {
        if !self.can_search() {
            return;
        }
        self.close_sheet();
        self.searching.set(true);
        self.progress.set(0.0);
        self.search_started.set(Some(Instant::now()));
    }

    pub fn clear_search(&self) {
        self.results.set(None);
    }

    pub fn show_toast(&self, message: impl Into<String>) {
        self.toast.set(Some(message.into()));
        self.toast_shown_at.set(self.tick.peek());
    }

    /// Encode the query and rank the library against it.
    ///
    /// Pure apart from reading the signals, and public so tests can check
    /// retrieval without driving the UI.
    pub fn matches_for_query(&self) -> Vec<Match> {
        let library = self.photos.peek();
        let queries = match self.mode.peek() {
            SearchMode::Text => self.embedder.embed_query(&self.text_query.peek()),
            SearchMode::Photo => match self.ref_photo.peek() {
                Some(reference) => vec![self.embedder.embed_image(&reference.image)],
                None => return Vec::new(),
            },
        };

        self.index
            .borrow()
            .search_any(&queries, MAX_RESULTS, self.embedder.min_similarity())
            .into_iter()
            .filter_map(|(i, score)| library.get(i).map(|photo| (photo.clone(), score)))
            .collect()
    }

    fn finish_search(&self) {
        let matches = self.matches_for_query();
        let count = matches.len();
        self.results.set(Some(Rc::new(matches)));
        self.searching.set(false);

        // Replay the staggered entrance. Every `Action` carries its own
        // `from`, so this always runs 0→1 — the current value is ignored and
        // there is nothing to reset by hand.
        if let Some(player) = self.reveal_player.borrow().as_ref() {
            player.play();
        }

        let subject = match self.mode.peek() {
            SearchMode::Text => format!("\"{}\"", self.text_query.peek().trim()),
            SearchMode::Photo => "that photo".to_string(),
        };
        self.show_toast(if count == 0 {
            format!("Nothing close to {subject}")
        } else {
            format!("{count} matches for {subject}")
        });
    }

    /// Push the live scroll offset into the header timeline.
    ///
    /// Every frame and deliberately un-throttled, unlike [`Self::advance_clock`]:
    /// a header that collapses in 140ms steps reads as stuttering, and this
    /// is one clamped division.
    pub fn sync_scroll(&self) {
        self.header.update(self.scroll.offset());
    }

    /// Called every frame from the app's `before_frame` hook, throttled by
    /// the caller. Advances the ambient clock, expires the toast, and
    /// drives the search's progress against real elapsed time.
    pub fn advance_clock(&self) {
        self.tick.update(|t| *t = t.wrapping_add(1));

        if self.toast.peek().is_some()
            && self.tick.peek().saturating_sub(self.toast_shown_at.get()) > TOAST_TICKS
        {
            self.toast.set(None);
        }

        if self.searching.peek() {
            if let Some(started) = self.search_started.get() {
                let t = (started.elapsed().as_secs_f32() / SEARCH_DURATION_SECS).min(1.0);
                self.progress.set(t);
                if t >= 1.0 {
                    self.search_started.set(None);
                    self.finish_search();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::local::LocalEmbedder;

    fn state() -> (Runtime, AppState) {
        let runtime = Runtime::new();
        let embedder: Arc<dyn Embedder> = Arc::new(LocalEmbedder::new());
        let spawner: Arc<dyn vieww::foundation::task::Spawn> =
            Arc::new(vieww::foundation::task::Threads);
        let loader = Loader::new(spawner.clone(), embedder.clone(), Arc::new(|_: &str| None));
        let state = AppState::new(&runtime, embedder, spawner, loader, None);
        // The tests are about retrieval, so they run against the generated
        // set — deterministic, and present without touching the filesystem.
        state.fall_back_to_samples();
        (runtime, state)
    }

    #[test]
    fn every_photo_in_the_library_gets_indexed() {
        let (_rt, state) = state();
        assert_eq!(state.index.borrow().len(), state.photos.peek().len());
    }

    #[test]
    fn an_empty_directory_falls_back_to_samples_rather_than_an_empty_grid() {
        let (_rt, state) = state();
        let dir = std::env::temp_dir().join("snapsearch-empty-test");
        std::fs::create_dir_all(&dir).unwrap();

        state.scan_directory(&dir);

        assert_eq!(state.origin.peek(), Origin::Samples);
        assert!(!state.photos.peek().is_empty(), "something must be on screen");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The real end-to-end check: pixels in, vector out, cosine ranked —
    /// and the encoder never sees the label it is being judged on.
    #[test]
    fn a_text_query_retrieves_photos_of_that_subject() {
        let (_rt, state) = state();
        for (query, expected) in [
            ("sunset", "sunset"),
            ("the ocean", "ocean"),
            ("a walk in the forest", "forest"),
            ("flowers", "flowers"),
        ] {
            state.text_query.set(query.to_string());
            let matches = state.matches_for_query();
            assert!(!matches.is_empty(), "{query:?} found nothing");
            assert_eq!(
                matches[0].0.label, expected,
                "{query:?} ranked {:?} first, not {expected:?}",
                matches[0].0.label
            );
        }
    }

    #[test]
    fn a_query_that_names_nothing_returns_nothing_rather_than_arbitrary_photos() {
        let (_rt, state) = state();
        state.text_query.set("qwertyuiop".to_string());
        assert!(state.matches_for_query().is_empty());
    }

    #[test]
    fn photo_as_reference_finds_other_photos_that_look_like_it() {
        let (_rt, state) = state();
        let library = state.photos.peek();
        let reference = library
            .iter()
            .find(|p| p.label == "ocean")
            .expect("an ocean photo")
            .clone();

        state.mode.set(SearchMode::Photo);
        state.ref_photo.set(Some(reference));
        let matches = state.matches_for_query();

        assert!(!matches.is_empty());
        assert_eq!(matches[0].0.label, "ocean", "a photo should match its own kind first");
    }

    /// The failure this test exists for: averaging a warm vector with a
    /// cold one produced magenta, and the grid came back full of flowers.
    #[test]
    fn a_two_subject_query_returns_both_subjects_and_not_their_average() {
        let (_rt, state) = state();
        state.text_query.set("sunset over water".to_string());
        let labels: Vec<String> = state
            .matches_for_query()
            .iter()
            .map(|(p, _)| p.label.clone())
            .collect();

        assert!(labels.contains(&"sunset".to_string()), "got {labels:?}");
        assert!(labels.contains(&"ocean".to_string()), "got {labels:?}");
        assert!(
            !labels.iter().any(|l| l == "flowers"),
            "magenta is the average of the two, not a match for either: {labels:?}"
        );
    }

    #[test]
    fn unrelated_photos_are_left_out_rather_than_padding_the_grid() {
        let (_rt, state) = state();
        state.text_query.set("sunset over water".to_string());
        let labels: Vec<String> = state
            .matches_for_query()
            .iter()
            .map(|(p, _)| p.label.clone())
            .collect();
        for unrelated in ["city at night", "desert", "forest"] {
            assert!(
                !labels.contains(&unrelated.to_string()),
                "{unrelated:?} is not a sunset or water: {labels:?}"
            );
        }
    }

    #[test]
    fn the_header_collapses_across_its_declared_range_and_then_stops() {
        let (_rt, state) = state();
        assert_eq!(state.header.peek(), 0.0, "starts expanded");

        state.header.update(HEADER_RANGE / 2.0);
        assert!((state.header.peek() - 0.5).abs() < 1e-4);

        // Clamped: scrolling further must not keep shrinking the header.
        state.header.update(HEADER_RANGE * 10.0);
        assert_eq!(state.header.peek(), 1.0);
    }

    #[test]
    fn there_is_one_reveal_slot_per_result_the_grid_can_show() {
        let (_rt, state) = state();
        assert_eq!(
            state.reveal.len(),
            MAX_RESULTS,
            "a result with no reveal slot would never fade in"
        );
    }


    #[test]
    fn results_come_back_best_first() {
        let (_rt, state) = state();
        state.text_query.set("sunset".to_string());
        let scores: Vec<f32> = state.matches_for_query().iter().map(|(_, s)| *s).collect();
        assert!(
            scores.windows(2).all(|w| w[0] >= w[1]),
            "not sorted: {scores:?}"
        );
    }
}
