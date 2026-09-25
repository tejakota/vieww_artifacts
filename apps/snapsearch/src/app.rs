//! Shared app wiring between the desktop/iOS entry point (`main.rs`) and the
//! Android entry point (`lib.rs`'s `android_main`) — both just call
//! [`configure`] then [`build_root`], so there is exactly one copy of "how
//! SnapSearch starts up".

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use std::sync::Arc;

use vieww::animation::SpringPreset;
use vieww::element::TimelineBuilder;
use vieww::foundation::task::Threads;
use vieww::foundation::Storage;
use vieww::prelude::*;
use vieww::render::FrameDriver;
use vieww_platform_winit::App;

use crate::screens::LibraryScreen;
use crate::library::Loader;
use crate::state::{AppState, REVEAL_STAGGER_MS};
use crate::theme::build_theme;

pub type SharedState = Rc<RefCell<Option<AppState>>>;

/// The platform's key/value store, if it has one.
fn platform_storage() -> Option<Rc<dyn Storage>> {
    #[cfg(not(target_os = "android"))]
    {
        Some(Rc::new(vieww_platform_winit::storage::FileStorage::platform()))
    }
    // On Android `FileStorage::for_android` needs the `AndroidApp`, which
    // `build_root` does not receive. Wiring it means threading that handle
    // through; until then the app re-embeds each launch rather than
    // pretending to have a cache.
    #[cfg(target_os = "android")]
    {
        None
    }
}

/// The whole cache, read once, as plain data the workers can share.
fn read_cache(storage: Option<&dyn Storage>) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    let Some(storage) = storage else {
        return out;
    };
    let Ok(keys) = storage.keys() else {
        return out;
    };
    for key in keys {
        if let Ok(Some(value)) = storage.get(&key) {
            out.insert(key, value);
        }
    }
    out
}

/// How often the ambient clock advances — throttled well under frame rate,
/// since it exists to drive slow drift/live-replace motion and a progress
/// bar, not to redraw at 60fps for no visual gain.
const CLOCK_INTERVAL: Duration = Duration::from_millis(140);

/// Sets the window chrome and wires the `before_frame` clock. Returns the
/// configured `App` plus the slot `build_root` will fill in once the tree
/// exists — `before_frame` is registered on the builder *before* `run`, so
/// it cannot close over state that only exists once `run`'s build closure
/// has executed; the slot is how the two sides meet.
pub fn configure(app: App) -> (App, SharedState) {
    let slot: SharedState = Rc::new(RefCell::new(None));
    let last_tick = Rc::new(RefCell::new(Instant::now()));
    let slot_for_frame = slot.clone();

    let app = app
        .title("SnapSearch")
        .background(Color::rgb(0x12, 0x10, 0x20))
        .before_frame(move |driver| {
            let Some(state) = slot_for_frame.borrow().clone() else {
                return;
            };

            // Files dropped on the window. The platform layer gathers them;
            // taking them is what clears the delivery.
            let dropped = driver.take_dropped_files();
            if !dropped.is_empty() {
                state.import(dropped.paths().to_vec());
            }

            // Photos finished by the workers, a few per frame.
            state.pump_loader();
            // Every frame: the collapsing header is a division, and a
            // header that moves in 140ms steps reads as stutter.
            state.sync_scroll();

            // Throttled: the ambient drift and the progress bar.
            let mut last = last_tick.borrow_mut();
            if last.elapsed() >= CLOCK_INTERVAL {
                *last = Instant::now();
                state.advance_clock();
            }
        });

    (app, slot)
}

/// Builds the initial widget tree. Called once, from inside `App::run`/
/// `App::run_android`'s build closure.
pub fn build_root(driver: &mut FrameDriver, slot: &SharedState) {
    let runtime = driver.elements().runtime().clone();
    let embedder = crate::embed::default_embedder();

    // Persistence, when the platform has any. Its whole job is the embedding
    // cache: re-embedding a thousand photos through CLIP on every launch is
    // a minute of work the app has already done once.
    let storage: Option<Rc<dyn Storage>> = platform_storage();

    // The workers read the cache but must not hold the store — `Storage` is
    // a UI-thread service and is not `Send`. So the whole cache is read out
    // once here into a plain map, and the workers get a lookup closure over
    // that snapshot. Writes go back through the UI thread in `pump_loader`.
    let snapshot = read_cache(storage.as_deref());
    let lookup: crate::library::CacheLookup = Arc::new(move |key: &str| {
        snapshot.get(key).and_then(|raw| crate::library::decode_vector(raw))
    });

    // `Threads` is a thread per task, which is honest for a few hundred
    // photos and wrong for a few thousand. A real deployment wants a pool
    // behind `Spawn`; the trait is the seam for exactly that.
    let spawner: Arc<dyn vieww::foundation::task::Spawn> = Arc::new(Threads);
    let loader = Loader::new(spawner.clone(), embedder.clone(), lookup);

    let state = AppState::new(&runtime, embedder, spawner, loader, storage);

    // Real photos if there are any, the generated set if not — and the
    // header says which.
    match crate::library::photo_dir() {
        Some(dir) => state.scan_directory(&dir),
        None => state.fall_back_to_samples(),
    }

    // Without this a fling stops dead the instant the finger leaves.
    state.scroll.attach(driver.tickers());

    // The staggered entrance for search results.
    //
    // Built here because `Timeline::attach` needs `&mut Tickers`, which only
    // set-up code has — and `attach` rather than `play` because the whole
    // point is to fire it later, from a search completing. The returned
    // `TimelinePlayer` takes `&self` on every method, so it travels into a
    // handler like any other captured value.
    //
    // `fold`, not a `for` loop: the stagger builder consumes and returns
    // `self`, so a `for` evaluates to `()` and does not compile.
    let reveal = state.reveal.clone();
    let timeline = TimelineBuilder::new()
        .stagger(Duration::from_millis(REVEAL_STAGGER_MS), move |stagger| {
            reveal.iter().fold(stagger, |builder, signal| {
                // Spatial, and allowed to overshoot: these tiles travel.
                builder.action_with(signal, 0.0, 1.0, SpringPreset::Expressive)
            })
        })
        .build();
    *state.reveal_player.borrow_mut() = Some(timeline.attach(driver.tickers()));

    *slot.borrow_mut() = Some(state.clone());

    let theme = build_theme(TargetPlatform::current());
    driver.set_root(Theme::new(theme).child(LibraryScreen { state }));
}
