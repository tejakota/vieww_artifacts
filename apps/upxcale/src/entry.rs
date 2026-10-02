//! The shared entry-point logic.
//!
//! Both the desktop binary (`src/main.rs`) and the Android entry point
//! (`android_main`, below) call [`boot_with`] — the only thing that differs
//! between the two hosts is where the [`SharedServices`] come from.
//!
//! `android_main` lives here rather than in `main.rs` because cargo-apk
//! builds the crate's `cdylib` target (the library), and the `#[no_mangle]`
//! entry symbol must be exported from that library. A binary's `main` fn is
//! never visible to the Android runtime.

use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::photos::Library;
use crate::screens::landing::{landing_route, Landing};
use crate::state::AppState;

use vieww_asset::{AssetBundle, DirectoryBundle};
use vieww_element::NavigatorController;
use vieww_foundation::task::{FrameWaker, Spawn, Threads};
use vieww_foundation::SharedServices;
use vieww_render::FrameDriver;
use vieww_widget::prelude::*;

/// How long a freshly-rendered tile keeps its accent ring.
///
/// Long enough to be noticed on a screen the user is looking back at, short
/// enough not to become part of the tile's normal appearance.
const FRESH_HOLD: Duration = Duration::from_millis(2200);

/// The per-frame pump.
///
/// This is the only place the render worker's results enter the tree, and it is
/// deliberately the whole of the app's frame-loop involvement — everything else
/// is signals and the rebuilds they cause.
pub struct Pump {
    state: AppState,
    /// When the post-render glow is due to be dropped, if it is showing.
    settle_at: Option<Instant>,
}

impl Pump {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            settle_at: None,
        }
    }

    pub fn tick(&mut self) {
        if self.state.poll_render() {
            self.settle_at = Some(Instant::now() + FRESH_HOLD);
        }
        if matches!(self.settle_at, Some(at) if Instant::now() >= at) {
            self.settle_at = None;
            self.state.settle_grid();
        }
    }
}

/// Wire the tree up with platform-default services (desktop).
pub fn boot(driver: &mut FrameDriver, waker: Arc<dyn FrameWaker>) -> AppState {
    let services = SharedServices::new(vieww_platform_winit::services::platform());
    boot_with(driver, waker, services)
}

/// Wire the tree up with caller-supplied services (Android, tests).
///
/// The only thing that differs between hosts is where the services come from;
/// everything past this point is the same tree.
pub fn boot_with(
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

// ── Android entry point ─────────────────────────────────────────────────────
//
// cargo-apk builds the crate's `cdylib` target and looks for an `android_main`
// symbol in it; this fn is that symbol. It mirrors the desktop `main` in
// `src/main.rs`, the only difference being that the services come from the
// Android host rather than the platform defaults.

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(android: vieww_platform_winit::AndroidApp) {
    let pump: Rc<RefCell<Option<Pump>>> = Rc::new(RefCell::new(None));
    let hook_pump = Rc::clone(&pump);

    let app = vieww_platform_winit::App::new()
        .title("upxcale")
        .background(crate::theme::scheme().surface)
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
