//! One state, owned here, read by every screen.
//!
//! This is `android/ui/state.slint`'s `Vavlt` global, with the difference that
//! it is now Rust the compiler checks rather than a schema two `.slint` files
//! agreed to honour. Navigation lives here beside consent for the same reason
//! it did there: "which screen is on top" and "what has the user agreed to" are
//! the same question, and splitting them is how a consent flow ends up letting
//! you skip a step.
//!
//! # The threading rule, and why it is a channel
//!
//! `Signal<T>` is `Rc<RefCell<T>>` and `!Send` by construction — vieww chose
//! that deliberately. So a worker thread **cannot** write state. Everything a
//! worker produces crosses back as a [`Message`] on an `mpsc` channel, and
//! [`VavltState::pump`] applies it on the UI thread between frames, where
//! writing is allowed. There is exactly one place messages are applied, and it
//! logs.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use vavlt_core::{estimate_library, Tier};
use vavlt_engine::{fmt_bytes, fmt_time, manifest_hash, Audit, Item, Outcome, Proof};
use vieww::element::{Runtime, Signal};
use vieww::foundation::task::FrameWaker;

use crate::model::{
    AuditKind, AuditRow, Estimate, Photo, PhotoState, RunResult, Screen, Tab, Thumb,
};
use crate::platform::Platform;

/// SPEC §3. Deep Move keeps originals for this long before they become
/// eligible for deletion; the number appears on three screens, so it lives in
/// one place.
pub const RETENTION_DAYS: u32 = 14;

/// The picker's cap. High enough to be useful, low enough that a mis-tap does
/// not hand over an entire camera roll.
///
/// **100, not 200.** Android's photo picker refuses an
/// `EXTRA_PICK_IMAGES_MAX` above `MediaStore.getPickImagesMaxLimit()`, which is
/// 100 on every device that has shipped it — and it refuses by *throwing as the
/// picker activity starts*, so the symptom is a button that does nothing rather
/// than an error. The shim clamps as well, because a device could lower the
/// limit; this is the number the rest of the app is designed around, and the
/// two agreeing is what keeps the desktop's folder walk comparable.
pub const MAX_PICK: usize = 100;

/// Something that happened off the UI thread.
///
/// Deliberately data, not a callback: a variant is a thing that can be logged,
/// replayed in a test, and matched exhaustively — none of which a boxed closure
/// crossing a thread boundary can be.
#[derive(Debug)]
pub enum Message {
    /// The picker returned. An empty vector is a cancel, not an error.
    Picked(Vec<Item>),
    /// The picker could not be reached at all.
    PickFailed(String),
    /// A tile's pixels arrived.
    Thumb {
        index: usize,
        thumb: Thumb,
    },
    RunStarted,
    RunFile {
        index: usize,
        outcome: Outcome,
    },
    RunFinished {
        cancelled: bool,
    },
}

/// A frame hook registered before the state it drains exists.
///
/// `App::on_frame` is a builder method — it is consumed before `run`, and the
/// runtime the state is built from does not exist until `run` calls back with a
/// driver. So the hook is registered empty and filled in from inside that
/// callback. One indirection, in one place, instead of three hosts each
/// inventing their own.
/// The hook's body, once it has one. Named because `clippy` is right that the
/// nesting is unreadable inline, and because the three layers each mean
/// something: shared with the closure (`Rc`), written once from inside the
/// build callback (`RefCell`), and absent until then (`Option`).
type PumpBody = Rc<RefCell<Option<Box<dyn FnMut()>>>>;

#[derive(Clone, Default)]
pub struct Pump(PumpBody);

impl std::fmt::Debug for Pump {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pump")
            .field("installed", &self.0.borrow().is_some())
            .finish()
    }
}

impl Pump {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The closure to hand to `App::on_frame`. Does nothing until installed,
    /// which is correct: there is nothing to drain before the state exists.
    pub fn hook(&self) -> impl FnMut() + 'static {
        let slot = self.0.clone();
        move || {
            if let Some(pump) = slot.borrow_mut().as_mut() {
                pump();
            }
        }
    }

    /// Point the hook at a state. Call once, from inside the build callback.
    pub fn install(&self, state: &Rc<VavltState>) {
        *self.0.borrow_mut() = Some(Box::new(state.pump()));
    }
}

/// Everything the screens read, and the only place any of it is written.
pub struct VavltState {
    // --- navigation -------------------------------------------------------
    pub screen: Signal<Screen>,
    pub tab: Signal<Tab>,
    pub dark: Signal<bool>,

    // --- the selection ----------------------------------------------------
    pub photos: Signal<Vec<Photo>>,
    pub picking: Signal<bool>,
    pub pick_error: Signal<String>,
    pub thumbs_loading: Signal<bool>,
    /// Which photograph carries the billboard, by index.
    ///
    /// Cached rather than computed per build: choosing it reads pixels, and the
    /// answer only changes when a new thumbnail decodes. See
    /// [`ui::brightest`](crate::ui::brightest).
    pub billboard_pick: Signal<Option<usize>>,

    // --- the plan ---------------------------------------------------------
    pub tier: Signal<Tier>,
    pub estimate: Signal<Estimate>,
    pub manifest: Signal<String>,
    /// The only switch in the app. Off, and the only other decision besides the
    /// tier — Access was granted by the picker and Transform is the button.
    pub grant_delete: Signal<bool>,

    // --- the run ----------------------------------------------------------
    pub running: Signal<bool>,
    pub progress: Signal<f32>,
    pub done_count: Signal<usize>,
    pub current_file: Signal<String>,

    // --- the outcome ------------------------------------------------------
    pub result: Signal<RunResult>,
    pub has_run: Signal<bool>,
    /// Which tile the photo screen is showing.
    pub detail: Signal<usize>,

    // --- lifetime ---------------------------------------------------------
    pub lifetime_saved: Signal<String>,
    pub lifetime_files: Signal<usize>,

    // --- the log ----------------------------------------------------------
    pub audit_rows: Signal<Vec<AuditRow>>,

    // --- machinery --------------------------------------------------------
    /// The engine's view of the selection. Parallel to `photos` by index; the
    /// UI never sees a handle, and the engine never sees a label.
    items: RefCell<Vec<Item>>,
    /// What the engine measured, by the same index.
    ///
    /// Kept rather than folded into the tile's label, because the headline
    /// figure is a sum of *bytes* while a tile carries a rounded percentage.
    /// Recovering one from the other would be parsing our own output, and would
    /// quietly make the total disagree with the rows it is a total of.
    outcomes: RefCell<Vec<Option<Outcome>>>,
    audit: RefCell<Audit>,
    cancel: RefCell<Arc<AtomicBool>>,
    outbox: Sender<Message>,
    inbox: Receiver<Message>,
    waker: Arc<dyn FrameWaker>,
    platform: Rc<dyn Platform>,
}

impl std::fmt::Debug for VavltState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VavltState")
            .field("screen", &self.screen.peek())
            .field("photos", &self.photos.with(Vec::len))
            .field("running", &self.running.peek())
            .finish_non_exhaustive()
    }
}

impl VavltState {
    /// Build the state and open the log.
    #[must_use]
    pub fn new(
        runtime: &Runtime,
        platform: Rc<dyn Platform>,
        waker: Arc<dyn FrameWaker>,
    ) -> Rc<Self> {
        let (outbox, inbox) = channel();
        let audit = Audit::open(&platform.data_dir());

        let state = Rc::new(Self {
            screen: runtime.signal(Screen::Root),
            tab: runtime.signal(Tab::Vavlt),
            dark: runtime.signal(true),

            photos: runtime.signal(Vec::new()),
            picking: runtime.signal(false),
            pick_error: runtime.signal(String::new()),
            thumbs_loading: runtime.signal(false),
            billboard_pick: runtime.signal(None),

            tier: runtime.signal(Tier::Move),
            estimate: runtime.signal(Estimate::default()),
            manifest: runtime.signal(String::new()),
            grant_delete: runtime.signal(false),

            running: runtime.signal(false),
            progress: runtime.signal(0.0),
            done_count: runtime.signal(0),
            current_file: runtime.signal(String::new()),

            result: runtime.signal(RunResult::default()),
            has_run: runtime.signal(false),
            detail: runtime.signal(0),

            lifetime_saved: runtime.signal(fmt_bytes(0)),
            lifetime_files: runtime.signal(0),

            audit_rows: runtime.signal(Vec::new()),

            items: RefCell::new(Vec::new()),
            outcomes: RefCell::new(Vec::new()),
            audit: RefCell::new(audit),
            cancel: RefCell::new(Arc::new(AtomicBool::new(false))),
            outbox,
            inbox,
            waker,
            platform,
        });

        state.refresh_audit();
        state
    }

    // --- navigation -------------------------------------------------------

    pub fn go(&self, screen: Screen) {
        log::debug!("go: {:?} -> {screen:?}", self.screen.peek());
        self.screen.set(screen);
    }

    /// The system back gesture, the back key, and the top bar's chevron all
    /// arrive here. One implementation, so a step cannot be skipped by
    /// whichever affordance the user reached for.
    pub fn back(&self) {
        // A run in progress owns the back gesture: leaving mid-encode would
        // leave the working screen unreachable while it kept writing results.
        if self.screen.peek() == Screen::Working && self.running.peek() {
            log::debug!("back refused: a run is in progress");
            return;
        }
        self.go(self.screen.peek().back());
    }

    pub fn set_tab(&self, tab: Tab) {
        self.tab.set(tab);
    }

    pub fn toggle_theme(&self) {
        self.dark.set(!self.dark.peek());
    }

    // --- the selection ----------------------------------------------------

    /// Ask the OS for photographs.
    pub fn pick(&self) {
        if self.picking.peek() {
            return;
        }
        log::info!("picker: requesting up to {MAX_PICK} files");
        self.picking.set(true);
        self.pick_error.set(String::new());
        self.platform
            .pick(MAX_PICK, self.outbox.clone(), self.waker.clone());
    }

    /// Drop a photo from the plan.
    ///
    /// Excluded photos leave the manifest, so the hash changes and any grant
    /// against the old plan lapses. That is the whole reason exclusion is a tap
    /// on the picture rather than a checkbox in a list.
    pub fn toggle_photo(&self, index: usize) {
        let mut changed = false;
        self.photos.update(|photos| {
            if let Some(photo) = photos.get_mut(index) {
                // RAW is protected: it is never re-encoded in any tier, so
                // there is nothing to exclude it from.
                if !photo.protected {
                    photo.included = !photo.included;
                    changed = true;
                }
            }
        });
        if changed {
            self.reprice();
        }
    }

    pub fn set_tier(&self, tier: Tier) {
        log::info!("tier: {tier:?}");
        self.tier.set(tier);
        self.reprice();
    }

    pub fn set_grant_delete(&self, granted: bool) {
        // The one place a delete grant can be set, and it logs — which is what
        // makes the consent flow checkable rather than merely described.
        log::info!("grant: delete-originals = {granted}");
        self.grant_delete.set(granted);
        self.record(
            "grant",
            if granted {
                "Delete-original granted"
            } else {
                "Delete-original withdrawn"
            },
            format!("Eligible after {RETENTION_DAYS} days. Nothing is deleted by a run."),
        );
    }

    /// Give the files back. Ends the Access grant this app can end.
    pub fn revoke_access(&self) {
        log::info!("access revoked: dropping the selection");
        self.items.borrow_mut().clear();
        self.outcomes.borrow_mut().clear();
        self.billboard_pick.set(None);
        self.photos.set(Vec::new());
        self.grant_delete.set(false);
        self.reprice();
        self.go(Screen::Root);
        self.record(
            "grant",
            "Access released",
            "The selection was dropped. The app holds no handles.",
        );
    }

    // --- the run ----------------------------------------------------------

    /// Start encoding. **This is the Transform grant** — the button says what
    /// it will do, and pressing it is the consent.
    pub fn start_run(&self) {
        if self.running.peek() {
            return;
        }

        let items = self.included_items();
        if items.is_empty() {
            log::warn!("start_run: nothing included");
            return;
        }

        let tier = self.tier.peek();
        let hash = manifest_hash(&items, tier);

        // Invariant 3: the engine refuses to run if the plan drifted from what
        // the user was shown. The manifest is recomputed here rather than
        // trusted, so an exclusion that raced the button press is caught.
        if self.manifest.peek() != hash {
            log::error!(
                "start_run refused: plan drifted ({} shown, {hash} now)",
                self.manifest.peek()
            );
            self.pick_error
                .set("The plan changed. Check the selection and try again.".into());
            return;
        }

        self.record(
            "grant",
            format!("Transform granted — {} files", items.len()),
            format!("{} · plan {hash}", tier.label()),
        );

        self.running.set(true);
        self.progress.set(0.0);
        self.done_count.set(0);
        *self.outcomes.borrow_mut() = vec![None; self.photos.with(Vec::len)];
        self.current_file.set(String::new());
        self.photos.update(|photos| {
            for photo in photos.iter_mut().filter(|p| p.included) {
                photo.state = PhotoState::Working;
            }
        });
        self.go(Screen::Working);

        let cancel = Arc::new(AtomicBool::new(false));
        *self.cancel.borrow_mut() = cancel.clone();

        // Index by *position in the full photo list*, not in the included
        // subset, or a result lands on the wrong tile the moment anything is
        // excluded. Resolved here, on the UI thread, where both lists exist.
        let positions = self.included_positions();
        let source = self.platform.source();
        let outbox = self.outbox.clone();
        let waker = self.waker.clone();

        self.platform.spawn(Box::new(move || {
            vavlt_engine::run(source, items, tier, cancel, |event| {
                let message = match event {
                    vavlt_engine::Event::Started => Message::RunStarted,
                    vavlt_engine::Event::File { index, outcome, .. } => Message::RunFile {
                        index: positions.get(index).copied().unwrap_or(index),
                        outcome,
                    },
                    vavlt_engine::Event::Finished { cancelled } => {
                        Message::RunFinished { cancelled }
                    }
                };
                // A closed channel means the window went away mid-run. Nothing
                // to report to, and nothing to do about it.
                if outbox.send(message).is_ok() {
                    waker.wake();
                }
            });
        }));
    }

    pub fn cancel_run(&self) {
        log::info!("run: cancel requested");
        self.cancel.borrow().store(true, Ordering::Relaxed);
    }

    pub fn open_photo(&self, index: usize) {
        self.detail.set(index);
        self.go(Screen::Photo);
    }

    /// The 14-day safety net's real button (SPEC §1.6).
    ///
    /// This build writes nothing, so there is nothing to put back — and it says
    /// so rather than pretending to succeed. The entry is still logged, because
    /// a restore that did nothing is exactly the kind of thing a user needs to
    /// find in the log later.
    pub fn restore_all(&self) {
        log::info!("restore-all requested");
        self.record(
            "restore",
            "Restore requested",
            "Nothing to restore: this build writes no vavlt copies, so every original is \
             already the only copy and is untouched.",
        );
    }

    pub fn export_audit(&self) {
        match self.audit.borrow().export() {
            Ok(path) => {
                self.platform.share(&path);
            }
            Err(error) => log::error!("audit export failed: {error:#}"),
        }
    }

    // --- reading ----------------------------------------------------------

    /// How many photos are in the plan.
    #[must_use]
    pub fn included_count(&self) -> usize {
        self.photos
            .with(|photos| photos.iter().filter(|p| p.included).count())
    }

    #[must_use]
    pub fn excluded_count(&self) -> usize {
        self.photos
            .with(|photos| photos.iter().filter(|p| !p.included).count())
    }

    #[must_use]
    pub fn protected_count(&self) -> usize {
        self.photos
            .with(|photos| photos.iter().filter(|p| p.protected).count())
    }

    /// Total bytes of what is actually in the plan, formatted.
    #[must_use]
    pub fn total_label(&self) -> String {
        let items = self.items.borrow();
        let included = self.photos.with(|photos| {
            photos
                .iter()
                .enumerate()
                .filter(|(_, p)| p.included)
                .filter_map(|(i, _)| items.get(i))
                .map(|item| item.bytes)
                .sum::<u64>()
        });
        fmt_bytes(included)
    }

    // --- the pump ---------------------------------------------------------

    /// Drain the channel into the signals.
    ///
    /// Hand the returned closure to `App::on_frame`, which runs it on the UI
    /// thread between frames — the only place a signal may be written from
    /// something that started on a worker.
    ///
    /// It takes no argument on purpose. `on_frame` passes a `FrameLog`, which
    /// lives in `vieww-platform-winit`, and naming it here would give this
    /// crate a dependency on a window. The host writes `.on_frame(move |_|
    /// pump())`, which is one closure and no dependency.
    pub fn pump(self: &Rc<Self>) -> impl FnMut() + 'static {
        let state = self.clone();
        move || {
            // `try_iter` rather than a loop with a match on `Empty`: a burst of
            // twenty results in one frame should cost one frame, not twenty.
            let drained: Vec<Message> = state.inbox.try_iter().collect();
            for message in drained {
                state.apply(message);
            }
        }
    }

    /// Apply one message now, on this thread.
    ///
    /// The pump's body, exposed. It is public for the screenshot harness and
    /// the interaction tests, which need to drive the app through the *real*
    /// transitions rather than writing signals behind its back — a fixture that
    /// sets `photos` directly photographs a state the app can never actually be
    /// in, which is the failure mode of every UI screenshot suite.
    pub fn deliver(&self, message: Message) {
        self.apply(message);
    }

    fn apply(&self, message: Message) {
        match message {
            Message::Picked(items) => self.on_picked(items),
            Message::PickFailed(reason) => {
                log::error!("picker: {reason}");
                self.picking.set(false);
                self.pick_error.set(reason);
            }
            Message::Thumb { index, thumb } => {
                self.photos.update(|photos| {
                    if let Some(photo) = photos.get_mut(index) {
                        photo.thumb = Some(thumb);
                    }
                });
                // A new picture can change which one is brightest, so the
                // billboard's choice is refreshed here — once per arriving
                // thumbnail — rather than on every build of every screen.
                self.billboard_pick
                    .set(self.photos.with(|photos| crate::ui::brightest(photos)));

                // The last tile to arrive turns the indicator off. Counted
                // rather than flagged, because thumbnails land out of order.
                let outstanding = self
                    .photos
                    .with(|photos| photos.iter().filter(|p| p.thumb.is_none()).count());
                if outstanding == 0 {
                    self.thumbs_loading.set(false);
                }
            }
            Message::RunStarted => {
                self.running.set(true);
            }
            Message::RunFile { index, outcome } => self.on_file(index, outcome),
            Message::RunFinished { cancelled } => self.on_finished(cancelled),
        }
    }

    fn on_picked(&self, items: Vec<Item>) {
        self.picking.set(false);

        if items.is_empty() {
            log::info!("picker: cancelled");
            return;
        }

        log::info!("picker: {} files", items.len());
        let photos: Vec<Photo> = items
            .iter()
            .map(|item| {
                Photo::new(
                    item.name.clone(),
                    fmt_bytes(item.bytes),
                    item.class.label(),
                    item.class.is_protected(),
                )
            })
            .collect();

        self.thumbs_loading.set(true);
        self.photos.set(photos);
        *self.items.borrow_mut() = items.clone();
        self.outcomes.borrow_mut().clear();
        self.reprice();

        self.record(
            "grant",
            format!("Access granted — {} files", items.len()),
            "By the system picker. These files and nothing else; no media permission is \
             declared."
                .to_string(),
        );

        self.load_thumbnails(items);
        self.go(Screen::Plan);
    }

    /// Decode tiles off the UI thread, one message per picture.
    ///
    /// One task for the whole batch rather than one per file: a hundred
    /// `std::thread::spawn` calls is a hundred threads, and the grid wants them
    /// in order anyway.
    fn load_thumbnails(&self, items: Vec<Item>) {
        let source = self.platform.source();
        let outbox = self.outbox.clone();
        let waker = self.waker.clone();

        self.platform.spawn(Box::new(move || {
            let mut sent = 0usize;
            for (index, item) in items.iter().enumerate() {
                // The platform's own cheap thumbnail first — Android reads the
                // camera's embedded preview instead of a 12MP JPEG.
                let decoded = match source.thumbnail(&item.handle, vavlt_engine::THUMB_EDGE) {
                    Some(bytes) => vavlt_engine::thumbnail(&bytes, vavlt_engine::THUMB_EDGE),
                    None => source.read(&item.handle).and_then(|bytes| {
                        vavlt_engine::thumbnail(&bytes, vavlt_engine::THUMB_EDGE)
                    }),
                };

                match decoded {
                    Ok((pixels, edge)) => {
                        if outbox
                            .send(Message::Thumb {
                                index,
                                thumb: Thumb::new(pixels, edge),
                            })
                            .is_err()
                        {
                            return;
                        }
                        sent += 1;
                        // Rebuilding the complete photo wall for every single
                        // thumbnail is expensive on Android. Four thumbnails
                        // per UI wake keeps progressive loading while cutting
                        // rebuilds by roughly 4x for a large selection.
                        if sent % 4 == 0 {
                            waker.wake();
                        }
                    }
                    // A tile that will not decode stays a placeholder with its
                    // class label on it — still a tile rather than a hole.
                    Err(error) => log::warn!("thumbnail for {}: {error:#}", item.name),
                }
            }
            // Flush the final partial batch.
            if sent % 4 != 0 {
                waker.wake();
            }
        }));
    }

    fn on_file(&self, index: usize, outcome: Outcome) {
        if let Some(slot) = self.outcomes.borrow_mut().get_mut(index) {
            *slot = Some(outcome.clone());
        }

        let state = PhotoState::from_proof(outcome.proof);
        let after = fmt_bytes(outcome.after);
        let pct = if outcome.proof.countable() {
            format!("−{:.0}%", outcome.pct())
        } else {
            outcome.proof.label().to_string()
        };

        self.photos.update(|photos| {
            if let Some(photo) = photos.get_mut(index) {
                photo.state = state;
                photo.after_label = after;
                photo.pct_label = pct;
                photo.note = outcome.note.clone();
            }
        });

        let done = self.done_count.peek() + 1;
        let total = self.included_count().max(1);
        self.done_count.set(done);
        self.progress.set(done as f32 / total as f32);
        self.current_file.set(outcome.name.clone());

        if outcome.proof == Proof::Failed {
            self.record(
                "fail",
                format!("Failed: {}", outcome.name),
                outcome.note.clone(),
            );
        } else if outcome.proof == Proof::Skipped {
            self.record(
                "skip",
                format!("Skipped: {}", outcome.name),
                outcome.note.clone(),
            );
        }
    }

    fn on_finished(&self, cancelled: bool) {
        self.running.set(false);
        self.current_file.set(String::new());
        self.tally();

        let result = self.result.peek();
        self.record(
            "run",
            if cancelled {
                "Run stopped"
            } else {
                "Run finished"
            },
            format!(
                "{} proven · {} unqualified · {} failed · {} saved, net",
                result.proven, result.unqualified, result.failed, result.saved_label
            ),
        );

        if !cancelled {
            self.has_run.set(true);
            self.go(Screen::Outcome);
        }
    }

    /// Turn the per-file results into the headline figure.
    ///
    /// Invariant 8: net of vavlt overhead, and a lossy ratio with no perceptual
    /// score is never counted. Which is why `unqualified` is tallied separately
    /// and subtracted from nothing — it simply is not in the figure.
    fn tally(&self) {
        let mut before = 0u64;
        let mut after = 0u64;
        let mut proven = 0usize;
        let mut unqualified = 0usize;
        let mut failed = 0usize;
        let mut countable = 0usize;

        for outcome in self.outcomes.borrow().iter().flatten() {
            match outcome.proof {
                Proof::Proven | Proof::PixelLossless => {
                    proven += 1;
                    countable += 1;
                    before += outcome.before;
                    after += outcome.after;
                }
                Proof::Unqualified => unqualified += 1,
                Proof::Failed => failed += 1,
                Proof::Skipped => {}
            }
        }

        // vavlt overhead is a real cost and comes off the top, or the figure is
        // a gross number dressed as a net one.
        let overhead = countable as u64
            * (vavlt_core::estimate::THUMB_BYTES + vavlt_core::estimate::PREVIEW_BYTES);
        let saved = before.saturating_sub(after).saturating_sub(overhead);

        let pct = if before == 0 {
            0.0
        } else {
            saved as f32 / before as f32
        };

        self.result.set(RunResult {
            saved_label: fmt_bytes(saved),
            before_label: fmt_bytes(before),
            after_label: fmt_bytes(after + overhead),
            pct,
            proven,
            unqualified,
            failed,
        });

        self.lifetime_saved.set(fmt_bytes(saved));
        self.lifetime_files.set(proven + unqualified + failed);
    }

    // --- estimation -------------------------------------------------------

    /// Price both tiers against what is currently in the plan.
    ///
    /// Called on every change to the selection or the tier, because the
    /// manifest hash has to change with them — a figure and a hash that were
    /// computed at different moments is exactly the drift invariant 3 exists to
    /// catch.
    fn reprice(&self) {
        let items = self.included_items();
        let metas: Vec<_> = items.iter().map(Item::meta).collect();

        let for_move = estimate_library(&metas, Tier::Move, None);
        let for_deep = estimate_library(&metas, Tier::DeepMove, None);
        let selected = if self.tier.peek() == Tier::Move {
            &for_move
        } else {
            &for_deep
        };

        self.estimate.set(Estimate {
            move_label: fmt_bytes(for_move.net_saved),
            deep_label: fmt_bytes(for_deep.net_saved),
            selected_label: fmt_bytes(selected.net_saved),
            selected_ratio: if selected.total_bytes == 0 {
                0.0
            } else {
                selected.net_saved as f32 / selected.total_bytes as f32
            },
            codec_label: fmt_bytes(selected.codec_saved),
            dedup_label: fmt_bytes(selected.dedup_saved),
            overhead_label: fmt_bytes(selected.overhead_bytes),
        });

        self.manifest.set(manifest_hash(&items, self.tier.peek()));
    }

    fn included_items(&self) -> Vec<Item> {
        let items = self.items.borrow();
        self.photos.with(|photos| {
            photos
                .iter()
                .enumerate()
                .filter(|(_, photo)| photo.included)
                .filter_map(|(index, _)| items.get(index).cloned())
                .collect()
        })
    }

    fn included_positions(&self) -> Vec<usize> {
        self.photos.with(|photos| {
            photos
                .iter()
                .enumerate()
                .filter(|(_, photo)| photo.included)
                .map(|(index, _)| index)
                .collect()
        })
    }

    // --- the log ----------------------------------------------------------

    fn record(&self, kind: &'static str, title: impl Into<String>, detail: impl Into<String>) {
        self.audit.borrow_mut().record(kind, title, detail);
        self.refresh_audit();
    }

    /// Rebuild the view model from the log.
    ///
    /// Newest first: the Activity screen is read to find out what just
    /// happened, not to read a history from the beginning.
    fn refresh_audit(&self) {
        let rows: Vec<AuditRow> = self
            .audit
            .borrow()
            .entries
            .iter()
            .rev()
            .map(|entry| AuditRow {
                time: fmt_time(entry.at),
                title: entry.title.clone(),
                detail: entry.detail.clone(),
                hash: entry.hash.clone(),
                kind: AuditKind::parse(entry.kind),
            })
            .collect();
        self.audit_rows.set(rows);
    }

    /// Where the log lives, for the Settings screen.
    #[must_use]
    pub fn data_dir(&self) -> PathBuf {
        self.platform.data_dir()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The headline figure is a sum of measured bytes, net of overhead, and an
    /// unqualified row contributes nothing to it. That is invariant 8, and it
    /// is the one arithmetic in this file worth pinning down.
    #[test]
    fn an_unqualified_row_is_not_in_the_figure() {
        let proven = Outcome {
            name: "a.jpg".into(),
            codec: "lepton".into(),
            before: 4_000_000,
            after: 3_000_000,
            proof: Proof::Proven,
            note: String::new(),
            fell_back: false,
        };
        let unqualified = Outcome {
            before: 8_000_000,
            after: 1_000_000,
            proof: Proof::Unqualified,
            ..proven.clone()
        };

        let counted: u64 = [&proven, &unqualified]
            .iter()
            .filter(|o| o.proof.countable())
            .map(|o| o.before - o.after)
            .sum();

        assert_eq!(
            counted, 1_000_000,
            "the unqualified row's seven megabytes are not a saving anyone can have"
        );
    }
}
