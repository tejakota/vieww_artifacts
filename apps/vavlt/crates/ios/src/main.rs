//! vavlt for iPhone.
//!
//! ```console
//! ci/ios-app.sh --sim        # simulator: no signing, no device
//! ci/ios-app.sh --device     # a real iPhone. Needs a signing identity.
//! cargo run -p vavlt-ios     # the same tree, in a window, on whatever you are sitting at
//! ```
//!
//! # Why this is a plain binary and Android's is a cdylib
//!
//! iOS calls `main`. winit starts `UIApplicationMain` from inside it, so
//! [`App::run`] is already correct here and there is no equivalent of
//! `run_android`. Android is the odd one out: its activity loads a shared
//! library and calls `android_main` inside it, so that target has to be a
//! `cdylib`.
//!
//! # Nothing here is `cfg`-gated away on a desktop
//!
//! Deliberately, and it is the same rule vieww's own `examples/ios.rs` follows:
//! an entry point that only compiles on the platform nobody is sitting at is
//! one that rots between releases. So this builds and runs on Linux and macOS
//! too, with [`picker`] falling back to a stub that reports it is not on a
//! phone. What differs between the three hosts in this workspace is under a
//! hundred lines each, and none of it is a screen.
//!
//! # What is proven and what is not
//!
//! **vieww renders on a real iPhone** — an iPhone 14 on iOS 17, through a
//! device cloud: the rasteriser's frames reach the screen, taps reach their
//! handlers, text shapes, animation runs inside budget. (The device-cloud runs
//! predate the renderer migration, which replaced vello with the in-house
//! CPU rasteriser — the claim those runs established, "a vieww tree draws and
//! responds on real hardware", is what carries forward, not the backend's
//! name.) That is the framework's result, not this app's.
//!
//! **This crate has not been on a device.** The tree it mounts is the one the
//! screenshot suite photographs and the one Android runs, so the *interface* is
//! as verified as the other two hosts. [`picker`] is not: the PHPicker bridge is
//! written and has never executed. It is marked as such at the top of that
//! file, and it is the first thing to run when a Mac is available.

use std::path::PathBuf;
use std::time::Duration;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use vavlt_app::{Message, Platform, Pump, Scrolls, VavltApp, VavltState};
use vavlt_engine::Source;
use vieww::foundation::task::FrameWaker;
use vieww::foundation::Size;
use vieww::prelude::*;
use vieww_platform_winit::App;

mod picker;

/// A phone-shaped window for the desktop run.
///
/// **A request, not a size.** On a device the OS decides and this is ignored
/// entirely; `VavltApp` reads the surface during layout and picks its form
/// factor from that, so nothing depends on getting what it asked for.
const PREVIEW: Size = Size {
    width: 412.0,
    height: 892.0,
};

/// The iOS host.
struct Ios {
    data_dir: PathBuf,
}

impl Platform for Ios {
    fn data_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }

    fn source(&self) -> Arc<dyn Source> {
        Arc::new(picker::PhotoSource::new())
    }

    fn pick(&self, max: usize, reply: Sender<Message>, waker: Arc<dyn FrameWaker>) {
        // Presenting a `PHPickerViewController` has to happen on the main
        // thread, and its delegate answers later — so unlike Android, where a
        // worker parks on a polled state, this posts and returns. `picker`
        // owns the whole dance and sends the message when the delegate fires.
        picker::present(max, reply, waker);
    }

    fn spawn(&self, work: Box<dyn FnOnce() + Send + 'static>) {
        std::thread::spawn(work);
    }

    fn share(&self, path: &std::path::Path) {
        // A `UIActivityViewController` is the right answer and is not written.
        // Saying so beats a silent no-op: the file is in the app's container
        // and reachable over the Files app if the bundle declares it.
        log::info!(
            "audit exported to {} — the iOS share sheet is not wired up yet",
            path.display()
        );
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let data_dir = picker::app_support_dir();
    if let Err(error) = std::fs::create_dir_all(&data_dir) {
        log::error!("could not create {}: {error:#}", data_dir.display());
    }
    log::info!("vavlt: data at {}", data_dir.display());

    let pump = Pump::new();
    let app = App::new()
        .title("vavlt")
        .size(PREVIEW)
        // Registered here because `on_frame` is a builder method, and the state
        // it drains does not exist until `run` calls back with a driver. See
        // `vavlt_app::Pump`.
        .on_frame({
            let mut hook = pump.hook();
            move |_log| hook()
        });
    let waker = Arc::new(app.waker());

    let result = app.run(move |driver: &mut FrameDriver| {
        let runtime = driver.elements().runtime().clone();

        let platform = Rc::new(Ios { data_dir });
        let state = VavltState::new(&runtime, platform, waker.clone());
        pump.install(&state);

        let scrolls = Rc::new(Scrolls::new(&runtime));
        scrolls.attach(driver.tickers());

        let splash = driver
            .animation(
                Tween::new(0.0_f32, 1.0_f32),
                Duration::from_millis(1450),
            )
            .curve(Curve::EASE_IN_OUT);
        splash.forward(Duration::ZERO);
        splash.attach(driver.tickers());

        driver.set_root(VavltApp::new_with_splash(state, scrolls, splash));
    });

    match result {
        Ok(report) => log::info!("{report}"),
        Err(error) => log::error!("vavlt failed: {error}"),
    }
}
