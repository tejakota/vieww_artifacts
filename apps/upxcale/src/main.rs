//! Entry point.
//!
//! ```console
//! # desktop harness — a phone-shaped window
//! cargo run --release
//!
//! # on an attached Android phone
//! cargo apk run --release
//! ```
//!
//! **Run it in release.** A debug `vello` is roughly an order of magnitude
//! slower, and a frame-rate impression from one is an impression of
//! `rustc -O0` rather than of this app. vieww's own README says the same thing
//! about its examples, for the same reason.
//!
//! # What this file is responsible for
//!
//! Everything that has to happen exactly once, in the right order, before the
//! tree can run:
//!
//! 1. Take a [`Waker`](vieww_platform_winit::App::waker) — *before* `run`
//!    consumes the `App`. A background render that finishes with nothing to wake
//!    the loop leaves the progress bar frozen at whatever it last painted.
//! 2. Register the platform services and find the asset bundle the photographs
//!    live in.
//! 3. Build the navigator's home route and `attach` it to the frame's tickers.
//!    Without the attach, a pushed route sits at t=0 and never transitions in.
//! 4. `attach` both scroll controllers, for the same reason in a different
//!    place: without it a fling stops dead the moment the finger lifts.
//! 5. Install the per-frame hook that drains the render worker.
//! 6. `driver.set_root(...)` — **not** `driver.elements().set_root(...)`. Every
//!    example in the framework warns about this: the second one skips the step
//!    that publishes view metrics, and `SafeArea` then silently insets by zero,
//!    which on a phone means content under the notch.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use upxcale::photos::Library;
use upxcale::screens::landing::{landing_route, Landing};
use upxcale::state::AppState;
use upxcale::SURFACE;

use vieww_asset::{AssetBundle, DirectoryBundle};
use vieww_element::NavigatorController;
use vieww_foundation::task::{FrameWaker, Spawn, Threads};
use vieww_foundation::SharedServices;
use vieww_platform_winit::App;
use vieww_render::FrameDriver;
use vieww_widget::prelude::*;

/// How long a freshly-rendered tile keeps its accent ring.
///
/// Long enough to be noticed on a screen the user is looking back at, short
/// enough not to become part of the tile's normal appearance.
const FRESH_HOLD: Duration = Duration::from_millis(2200);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The frame hook has to be installed on the `App` *builder*, but the state
    // it pumps cannot exist until `run` hands over a `FrameDriver` to take a
    // `Runtime` from. This cell is the join: the hook reads it every frame and
    // does nothing until `boot` has filled it in, which happens on the first.
    let pump: Rc<RefCell<Option<Pump>>> = Rc::new(RefCell::new(None));
    let hook_pump = Rc::clone(&pump);

    let app = App::new()
        .title("upxcale")
        .size(SURFACE)
        .background(upxcale::theme::scheme().surface)
        .before_frame(move |_driver| {
            if let Some(pump) = hook_pump.borrow_mut().as_mut() {
                pump.tick();
            }
        });

    // Taken before `run` consumes the app. This is the handle the render worker
    // uses to say "there is a new frame's worth of progress"; without it a batch
    // that finishes while the screen is idle leaves the bar frozen at whatever
    // it last painted.
    let waker: Arc<dyn FrameWaker> = Arc::new(app.waker());

    let report = app.run(move |driver| {
        let state = boot(driver, waker);
        *pump.borrow_mut() = Some(Pump::new(state));
    })?;

    println!("{report}");
    Ok(())
}

/// The per-frame pump.
///
/// This is the only place the render worker's results enter the tree, and it is
/// deliberately the whole of the app's frame-loop involvement — everything else
/// is signals and the rebuilds they cause.
struct Pump {
    state: AppState,
    /// When the post-render glow is due to be dropped, if it is showing.
    settle_at: Option<Instant>,
}

impl Pump {
    const fn new(state: AppState) -> Self {
        Self {
            state,
            settle_at: None,
        }
    }

    fn tick(&mut self) {
        if self.state.poll_render() {
            self.settle_at = Some(Instant::now() + FRESH_HOLD);
        }
        if matches!(self.settle_at, Some(at) if Instant::now() >= at) {
            self.settle_at = None;
            self.state.settle_grid();
        }
    }
}

/// Wire the tree up. Shared by the desktop entry above and `android_main`
/// below, because the only thing that differs between them is where the
/// services come from.
fn boot(driver: &mut FrameDriver, waker: Arc<dyn FrameWaker>) -> AppState {
    let services = SharedServices::new(vieww_platform_winit::services::platform());
    boot_with(driver, waker, services)
}

fn boot_with(
    driver: &mut FrameDriver,
    waker: Arc<dyn FrameWaker>,
    services: SharedServices,
) -> AppState {
    let runtime = driver.elements().runtime().clone();

    // The platform registers a bundle pointing at `assets/` beside the
    // executable. On a desktop `cargo run` that is not where the assets are, so
    // fall back to the crate's own directory — a development convenience, and
    // the reason it is a fallback rather than the primary is that on a phone
    // the platform's answer is the only correct one.
    let bundle: Rc<dyn AssetBundle> = services.get::<dyn AssetBundle>().unwrap_or_else(|| {
        Rc::new(DirectoryBundle::at(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets"
        )))
    });
    let library = Rc::new(Library::new(bundle));

    // A placeholder home route, replaced immediately: `NavigatorController::new`
    // needs *a* route to be constructed, and the real one needs the controller
    // to exist so its buttons can push. This is the standard two-step from the
    // framework's own `screens.rs` example.
    let nav = NavigatorController::new(
        &runtime,
        Route::new("bootstrap", Rc::new(|_| SizedBox::shrink().into())),
    );

    // `Threads` is one thread per task. vieww owns no executor — `Spawn` is
    // `Box<dyn FnOnce() + Send>` — so an app that already ran tokio would pass
    // its own here and nothing else would change.
    let spawner: Rc<dyn Spawn> = Rc::new(Threads);

    let state = AppState::new(runtime, library, nav.clone(), spawner, Arc::clone(&waker));

    let landing = Landing {
        state: state.clone(),
    };
    nav.replace(landing_route(landing.clone()));

    // Three attaches, three different things that stop working without them.
    nav.attach(driver.tickers());
    state.grid_scroll.attach(driver.tickers());
    state.picker_scroll.attach(driver.tickers());

    driver.set_root(landing);
    state
}

/// Android's entry point.
///
/// `cargo apk run --release`. The crate has to be a `cdylib` for this; see
/// `Cargo.toml`.
#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(android: vieww_platform_winit::AndroidApp) {
    let pump: Rc<RefCell<Option<Pump>>> = Rc::new(RefCell::new(None));
    let hook_pump = Rc::clone(&pump);

    let app = App::new()
        .title("upxcale")
        .background(upxcale::theme::scheme().surface)
        .before_frame(move |_driver| {
            if let Some(pump) = hook_pump.borrow_mut().as_mut() {
                pump.tick();
            }
        });
    let waker: Arc<dyn FrameWaker> = Arc::new(app.waker());

    // Android's registry knows where the APK's assets live, which is the whole
    // reason this does not just call `boot`.
    let services = SharedServices::new(vieww_platform_winit::services::for_android(&android));

    if let Err(error) = app.run_android(android, move |driver| {
        let state = boot_with(driver, waker, services);
        *pump.borrow_mut() = Some(Pump::new(state));
    }) {
        eprintln!("upxcale: {error}");
    }
}
