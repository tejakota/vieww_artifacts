//! vavlt on a laptop.
//!
//! ```console
//! cargo run --release -p vavlt-desktop              # asks for a folder on launch
//! cargo run --release -p vavlt-desktop ~/Pictures   # takes one
//! ```
//!
//! # What this crate is, and is not
//!
//! It is a `Platform` impl and a `main`. Roughly a hundred lines, and none of
//! them draw anything: every screen, every colour and every rule lives in
//! `vavlt-app`, which this mounts. That is the whole point of the port — the
//! Slint build had a *second* UI here, about 1,500 lines of `.slint` that had
//! already grown a comparison viewer the phone did not have and lost a consent
//! flow the phone did.
//!
//! It is also the development harness for the two phone builds. A laptop has no
//! system photo picker worth the name, so [`FolderSource`] walks a directory
//! instead — which is a *weaker* claim than the phone hosts make and says so:
//! see the invariant note on that type.

use std::path::{Path, PathBuf};
use std::time::Duration;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use vavlt_app::{Message, Platform, Pump, Scrolls, VavltApp, VavltState, MAX_PICK};
use vavlt_engine::{Item, Source};
use vieww::foundation::task::FrameWaker;
use vieww::foundation::Size;
use vieww::prelude::*;
use vieww_platform_winit::App;

mod folder;

use folder::FolderSource;

/// A window shaped like the desktop layout, above the 640 breakpoint.
///
/// **A request, not a size.** The window manager decides, and nothing in the
/// tree may depend on getting what it asked for — `VavltApp` reads the surface
/// width during layout and picks its form factor from that, so a window dragged
/// narrow becomes the phone layout rather than a broken desktop one.
const WINDOW: Size = Size {
    width: 1024.0,
    height: 768.0,
};

/// The desktop host.
struct Desktop {
    root: PathBuf,
    data_dir: PathBuf,
}

impl Platform for Desktop {
    fn data_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }

    fn source(&self) -> Arc<dyn Source> {
        Arc::new(FolderSource::new(self.root.clone()))
    }

    fn pick(&self, max: usize, reply: Sender<Message>, waker: Arc<dyn FrameWaker>) {
        let root = self.root.clone();
        // Off the UI thread even here: walking a camera roll is thousands of
        // `stat` calls, and a first frame that waits for them is a launch that
        // looks like a hang.
        std::thread::spawn(move || {
            let message = match folder::scan(&root, max) {
                Ok(items) => Message::Picked(items),
                Err(error) => Message::PickFailed(format!("{error:#}")),
            };
            if reply.send(message).is_ok() {
                waker.wake();
            }
        });
    }

    fn spawn(&self, work: Box<dyn FnOnce() + Send + 'static>) {
        std::thread::spawn(work);
    }

    fn share(&self, path: &Path) {
        // A desktop's share sheet is the file system. Printing the path is the
        // whole feature, and the alternative — shelling out to `xdg-open` —
        // would be this app's first process spawn for no gain.
        log::info!("audit exported to {}", path.display());
        println!("audit exported to {}", path.display());
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .or_else(dirs_pictures)
        .unwrap_or_else(|| PathBuf::from("."));

    let data_dir = data_dir();
    if let Err(error) = std::fs::create_dir_all(&data_dir) {
        // Not fatal: the audit log degrades to in-memory, and an app that
        // refuses to launch over its own log file is worse than one that says
        // it could not write it.
        log::error!("could not create {}: {error:#}", data_dir.display());
    }

    log::info!("vavlt: scanning {}", root.display());

    let pump = Pump::new();
    let app = App::new()
        .title("vavlt")
        .size(WINDOW)
        // `on_frame` is a builder method and the state it drains does not exist
        // until `run` calls back with a driver, so the hook is registered empty
        // and filled in below. See `vavlt_app::Pump`.
        .on_frame({
            let mut hook = pump.hook();
            move |_log| hook()
        });
    let waker = Arc::new(app.waker());

    let result = app.run(move |driver: &mut FrameDriver| {
        let runtime = driver.elements().runtime().clone();

        let platform = Rc::new(Desktop {
            root: root.clone(),
            data_dir: data_dir.clone(),
        });
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

/// Where the audit log lives. Not next to the user's photographs, ever.
fn data_dir() -> PathBuf {
    // Hand-rolled rather than a `dirs` dependency: this is two environment
    // variables and a fallback, and the crate would be the only thing in the
    // workspace that knows about XDG.
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("vavlt");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".local/share/vavlt");
    }
    std::env::temp_dir().join("vavlt")
}

fn dirs_pictures() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let pictures = PathBuf::from(home).join("Pictures");
    pictures.is_dir().then_some(pictures)
}

/// Keeps `MAX_PICK` honest across hosts: the desktop's walk caps at the same
/// number the phone pickers do, or the two builds disagree about what a
/// selection is.
///
/// Deliberately an equality and not a `<=`. When the Android cap moved from 200
/// to 100 this line is what makes the desktop notice, and a range check would
/// have let the two drift apart silently.
const _: () = assert!(MAX_PICK == 100);

/// Named so `Item` is not an unused import in a build where `folder` changes.
#[allow(dead_code)]
fn _item(_: &Item) {}
