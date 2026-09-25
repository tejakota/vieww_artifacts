//! Everything the screens read and write.
//!
//! The prototype's `app.js` kept this in a plain object and re-rendered the
//! whole document on every change. Here each field is a [`Signal`], and reading
//! one during `build` *is* the subscription — so writing `picked` rebuilds the
//! picker's grid and its confirm button and nothing else. That is the entire
//! difference between the two, and it is why the masonry does not re-decode
//! twelve photographs when a checkbox moves.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use vieww_element::{NavigatorController, Runtime, ScrollController, Signal};
use vieww_foundation::task::{FrameWaker, Spawn};
use vieww_gestures::ScrollPhysics;

use crate::photos::{self, Library, Photo};
use crate::render::{Progress, RenderJob, STAGES_PER_PHOTO};

/// One cell of the landing masonry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridItem {
    pub id: &'static str,
    /// Whether this cell shows the upscaled result or the source.
    pub upscaled: bool,
    /// Set for a beat after a render finishes, so the tile can glow and then
    /// stop. Transient by design — it is view state, not a fact about the
    /// photograph.
    pub fresh: bool,
}

impl GridItem {
    #[must_use]
    pub fn photo(&self) -> Option<&'static Photo> {
        photos::by_id(self.id)
    }
}

/// What the progress overlay displays.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderState {
    /// Stages finished, out of `stages_total`.
    pub stages_done: usize,
    pub stages_total: usize,
    /// Photographs finished.
    pub photos_done: usize,
    pub photos_total: usize,
    /// The line under the title.
    pub status: String,
}

impl RenderState {
    #[must_use]
    pub fn idle() -> Self {
        Self {
            stages_done: 0,
            stages_total: 0,
            photos_done: 0,
            photos_total: 0,
            status: String::new(),
        }
    }

    /// 0..=1, for the determinate bar. Zero stages means zero progress rather
    /// than a division by zero.
    #[must_use]
    pub fn fraction(&self) -> f32 {
        if self.stages_total == 0 {
            0.0
        } else {
            (self.stages_done as f32 / self.stages_total as f32).clamp(0.0, 1.0)
        }
    }

    #[must_use]
    pub fn percent(&self) -> u32 {
        (self.fraction() * 100.0).round() as u32
    }

    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.stages_total > 0 && self.photos_done >= self.photos_total
    }
}

/// The route names this app pushes. Strings are what
/// [`NavigatorController`] deals in; naming them once stops a typo from
/// becoming a route that never pops.
pub mod routes {
    pub const LANDING: &str = "landing";
    pub const PICKER: &str = "picker";
    pub const PROGRESS: &str = "progress";
    pub const COMPARE: &str = "compare";
}

/// The application's state, shared by every screen.
///
/// Cloning this is cheap and shares — `Signal` is a handle, `Rc` is a handle,
/// and `NavigatorController` is a handle. Screens take one by value.
#[derive(Clone)]
pub struct AppState {
    pub runtime: Runtime,
    pub library: Rc<Library>,
    pub nav: NavigatorController,

    /// What the landing masonry shows.
    pub grid: Signal<Vec<GridItem>>,
    /// Ids ticked inside the picker. Separate from `grid` on purpose: the
    /// prototype conflated them and had to reset the grid's selection every
    /// time the picker was dismissed.
    pub picked: Signal<Vec<String>>,
    /// What the progress overlay shows.
    pub progress: Signal<RenderState>,
    /// The photo the compare sheet is showing, if it is up.
    pub comparing: Signal<Option<String>>,
    /// Where the compare sheet's divider sits, 0..=1.
    pub divider: Signal<f32>,

    /// The masonry's scroll position, and the picker grid's. Two controllers
    /// rather than one: they scroll independently, and sharing would mean
    /// opening the picker jumps it to wherever the grid happened to be.
    ///
    /// Both must be handed to `Tickers` before a fling will decelerate —
    /// `main.rs` does that.
    pub grid_scroll: ScrollController,
    pub picker_scroll: ScrollController,

    /// The batch in flight, if any.
    job: Rc<RefCell<Option<RenderJob>>>,
    /// How the render reaches a thread. Held so a screen can start one.
    spawner: Rc<dyn Spawn>,
    waker: Arc<dyn FrameWaker>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("depth", &self.nav.depth())
            .field("rendering", &self.job.borrow().is_some())
            .finish()
    }
}

impl AppState {
    /// Build the state and seed the landing grid.
    ///
    /// The seed is the whole catalogue, un-upscaled. That is the first-run
    /// behaviour the app wants: open to a full screen of photographs with the
    /// Render button waiting at the bottom, rather than to an empty screen
    /// behind a picker the user did not ask for.
    pub fn new(
        runtime: Runtime,
        library: Rc<Library>,
        nav: NavigatorController,
        spawner: Rc<dyn Spawn>,
        waker: Arc<dyn FrameWaker>,
    ) -> Self {
        let seed: Vec<GridItem> = photos::CATALOGUE
            .iter()
            .map(|photo| GridItem {
                id: photo.id,
                upscaled: false,
                fresh: false,
            })
            .collect();

        // `ScrollPhysics::ios()` matches the theme's Apple metrics: the
        // rubber-band at the edges and the deceleration rate are part of how a
        // list feels native, and mixing an Android curve with Apple corners is
        // the kind of detail that reads as wrong without being nameable.
        let grid_scroll = ScrollController::new(&runtime, ScrollPhysics::ios());
        let picker_scroll = ScrollController::new(&runtime, ScrollPhysics::ios());

        Self {
            grid: runtime.signal(seed),
            picked: runtime.signal(Vec::new()),
            progress: runtime.signal(RenderState::idle()),
            comparing: runtime.signal(None),
            divider: runtime.signal(0.5),
            job: Rc::new(RefCell::new(None)),
            grid_scroll,
            picker_scroll,
            runtime,
            library,
            nav,
            spawner,
            waker,
        }
    }

    // ── picker ──────────────────────────────────────────────────────────────

    /// Tick or untick one photograph in the picker.
    pub fn toggle_pick(&self, id: &str) {
        self.picked.update(|picked| {
            if let Some(at) = picked.iter().position(|p| p == id) {
                picked.remove(at);
            } else {
                picked.push(id.to_string());
            }
        });
    }

    #[must_use]
    pub fn is_picked(&self, id: &str) -> bool {
        self.picked.with(|picked| picked.iter().any(|p| p == id))
    }

    #[must_use]
    pub fn pick_count(&self) -> usize {
        self.picked.with(Vec::len)
    }

    pub fn clear_picks(&self) {
        self.picked.set(Vec::new());
    }

    // ── the render ──────────────────────────────────────────────────────────

    /// Start upscaling everything currently ticked.
    ///
    /// Returns `false` if nothing was ticked or no source could be decoded, in
    /// which case no route is pushed and the picker stays up — a button that
    /// silently does nothing is worse than one that stays put.
    pub fn start_render(&self) -> bool {
        let chosen: Vec<&'static Photo> = self
            .picked
            .with(|picked| picked.iter().filter_map(|id| photos::by_id(id)).collect());
        if chosen.is_empty() {
            return false;
        }

        // Decode on this thread — it is fast, it is cached, and it keeps the
        // worker's inputs to plain pixels.
        let mut work = Vec::with_capacity(chosen.len());
        for photo in &chosen {
            match self.library.source(photo) {
                Ok(pixels) => work.push((photo.id.to_string(), pixels)),
                // One unreadable file should not sink the batch. It is dropped
                // from the work list and simply never appears in the result.
                Err(error) => eprintln!("upxcale: {}: {error}", photo.asset_path()),
            }
        }
        if work.is_empty() {
            return false;
        }

        let total = work.len();
        self.progress.set(RenderState {
            stages_done: 0,
            stages_total: total * STAGES_PER_PHOTO as usize,
            photos_done: 0,
            photos_total: total,
            status: first_status(&chosen),
        });

        let job = RenderJob::start(self.spawner.as_ref(), Arc::clone(&self.waker), work);
        *self.job.borrow_mut() = Some(job);
        true
    }

    /// Drain the worker and fold what it sent into the signals.
    ///
    /// Called once per frame from `main.rs`'s `before_frame` hook. Returns true
    /// when the batch finished on this call, which is the moment the progress
    /// route is popped and the grid is swapped.
    pub fn poll_render(&self) -> bool {
        let messages = match self.job.borrow().as_ref() {
            Some(job) => job.drain(),
            None => return false,
        };
        if messages.is_empty() {
            return false;
        }

        let mut state = self.progress.peek();
        let mut finished_ids: Vec<&'static str> = Vec::new();

        for message in messages {
            match message {
                Progress::Stage { index, stage } => {
                    // Recompute rather than increment: a dropped or duplicated
                    // message then cannot leave the bar permanently off.
                    let done = index * STAGES_PER_PHOTO as usize + stage as usize;
                    state.stages_done = state.stages_done.max(done);
                }
                Progress::Done { id, image } => {
                    if let Some(photo) = photos::by_id(&id) {
                        self.library.adopt(photo, image);
                        finished_ids.push(photo.id);
                        state.photos_done += 1;
                        state.status = next_status(state.photos_done, state.photos_total);
                    }
                }
                Progress::Failed { id, error } => {
                    eprintln!("upxcale: {id}: {error}");
                    state.photos_done += 1;
                }
            }
        }

        self.progress.set(state.clone());

        if state.is_complete() {
            self.finish_render(&finished_ids);
            true
        } else {
            false
        }
    }

    /// Swap the grid for the freshly-rendered set and drop the job.
    fn finish_render(&self, _just_finished: &[&'static str]) {
        let rendered: Vec<GridItem> = self
            .picked
            .peek()
            .iter()
            .filter_map(|id| photos::by_id(id))
            .filter(|photo| self.library.is_upscaled(photo))
            .map(|photo| GridItem {
                id: photo.id,
                upscaled: true,
                fresh: true,
            })
            .collect();

        if !rendered.is_empty() {
            self.grid.set(rendered);
        }
        self.clear_picks();
        *self.job.borrow_mut() = None;

        // The progress route is the top of the stack here; drop it so the grid
        // is what the user lands back on.
        if self.nav.current_name().as_deref() == Some(routes::PROGRESS) {
            self.nav.pop();
        }
    }

    /// Clear the transient post-render glow.
    ///
    /// Called from the frame hook a beat after the render lands. Kept separate
    /// from [`finish_render`](Self::finish_render) because "the render is done"
    /// and "the highlight has been seen" are different moments, and conflating
    /// them is how the glow ends up either permanent or invisible.
    pub fn settle_grid(&self) {
        let any_fresh = self.grid.with(|grid| grid.iter().any(|item| item.fresh));
        if any_fresh {
            self.grid.update(|grid| {
                for item in grid.iter_mut() {
                    item.fresh = false;
                }
            });
        }
    }

    #[must_use]
    pub fn is_rendering(&self) -> bool {
        self.job.borrow().is_some()
    }

    // ── compare ─────────────────────────────────────────────────────────────

    /// Open the compare sheet on one photograph.
    pub fn open_compare(&self, id: &'static str) {
        self.comparing.set(Some(id.to_string()));
        self.divider.set(0.5);
    }

    #[must_use]
    pub fn comparing_photo(&self) -> Option<&'static Photo> {
        self.comparing
            .with(|id| id.as_deref().and_then(photos::by_id))
    }

    /// Move the compare divider. Clamped, because a drag can leave the stage.
    pub fn set_divider(&self, fraction: f32) {
        self.divider.set(fraction.clamp(0.0, 1.0));
    }

    /// Write the photograph being compared to disk as a PNG.
    ///
    /// # This is not "Save to Photos"
    ///
    /// The button says "Save to Photos" because that is what it would say on a
    /// phone, and on a phone it would hand the pixels to the system photo
    /// library. vieww has no such service: `docs/PRODUCTION-GAPS.md` lists
    /// camera, geolocation, push and biometrics as each needing their own
    /// per-platform FFI, and the photo library is the same shape of gap.
    ///
    /// So this writes a file into [`export_dir`] and returns where it went.
    /// That is a real save — the pixels are on disk and openable — and it is
    /// deliberately not dressed up as the platform integration it is standing
    /// in for. When vieww grows a `PhotoLibrary` service, this function is the
    /// one that changes.
    pub fn save_current(&self) -> Option<std::path::PathBuf> {
        let photo = self.comparing_photo()?;
        let image = self.library.result(photo)?;
        match crate::export::write_png(&image, photo.slug) {
            Ok(path) => {
                eprintln!("upxcale: saved {}", path.display());
                Some(path)
            }
            Err(error) => {
                eprintln!("upxcale: could not save {}: {error}", photo.slug);
                None
            }
        }
    }
}

fn first_status(chosen: &[&Photo]) -> String {
    match chosen {
        [one] => format!("Reconstructing {}", one.title.to_lowercase()),
        many => format!("Reconstructing {} photos", many.len()),
    }
}

fn next_status(done: usize, total: usize) -> String {
    if done >= total {
        "Finishing up".to_string()
    } else {
        format!("Photo {} of {total}", done + 1)
    }
}
