//! The desktop entry point — the same demo, on Linux, macOS and Windows.
//!
//! ```console
//! ci/certify/desktop-suite.sh                 # build, run, verify
//! cargo run -p vieww-platform-winit --example desktop --release
//! ```
//!
//! # Why this exists when `examples/ios.rs` already runs on a desktop
//!
//! It runs there as a *preview*: an entry point for a platform nobody is
//! sitting at, kept compiling by being runnable on the machine they are. What
//! it does not do is ask the desktop anything. Every check it installs is one
//! `device_tests.rs` wrote for a phone, and the questions a workstation can
//! answer and a phone cannot — a second window, the shortcut modifier, the
//! pasteboard, the conventional data directory — were asked by nothing at all.
//!
//! That was the shape of the gap: `ci/mobile/device-suite.sh` could say "this Android
//! phone is behaving", and `ci/certify/release-check.sh` could say "the studio's
//! controls are all reachable and here are the pictures", and between them
//! nobody could say whether the *platform layer* works on a Mac. This is the
//! third leg.
//!
//! # The same tree, deliberately
//!
//! `shared/screen.rs` and `shared/device_tests.rs`, unchanged, exactly as the
//! Android and iOS entry points take them — so a report from this run and a
//! report from a phone carry the same check names against the same widgets, and
//! the two are comparable line for line. `shared/desktop_tests.rs` is the only
//! thing added, and it holds only what a phone would have to answer
//! `Unsupported` to.
//!
//! # What a passing run means
//!
//! That the platform layer behaved on **this** machine, in **this** session. It
//! is not a claim about the GPU (this example presents CPU-rasterized pixels;
//! the GPU path is [`App::prefer_gpu`], off by default), and it is not a claim
//! about a second monitor, a HiDPI
//! display or a keyboard layout this machine does not have — those are what
//! `ci/certify/desktop-suite.sh`'s flags are for, because only the operator knows them.

// On Android the suite is `examples/android.rs`; this file compiles there
// (it is in the same package) and does nothing — see `main` below.
#![cfg_attr(target_os = "android", allow(dead_code, unused_imports))]

use vieww_foundation::{Size, TargetPlatform};
use vieww_platform_winit::App;

/// A window shaped like the demo column, for the reason `examples/ios.rs` gives
/// at length: this is a **request**, the window manager decides, and nothing in
/// the tree may depend on getting it.
const WINDOW: Size = Size {
    width: 420.0,
    height: 880.0,
};

#[path = "shared/screen.rs"]
mod screen;

#[path = "shared/device_tests.rs"]
mod device_tests;

#[cfg(not(target_os = "android"))]
#[path = "shared/desktop_tests.rs"]
mod desktop_tests;

#[cfg(not(target_os = "android"))]
use desktop_tests::DesktopSuite;
use device_tests::DeviceSuite;

/// The desktop suite needs a desktop: on Android the pasteboard is reached
/// through the activity (`PlatformClipboard::new(&AndroidApp)`), and the
/// device suite there is `examples/android.rs`.
#[cfg(target_os = "android")]
fn main() {
    eprintln!("the desktop suite does not run on Android; see examples/android.rs");
}

/// `log` records to stderr when `RUST_LOG` is set — `info`, `debug` or `trace`
/// (anything else, or a module filter, means `info`).
///
/// # Why an example needs a logger
///
/// The platform crate reports through `log` — the refresh rate a window was
/// given (`vieww: window#primary reports 60.0Hz`), insets, accessibility —
/// and nothing installed a logger on the desktop, so `RUST_LOG=info` printed
/// nothing at all. PENDING §3.2's own instructions ("drag the window between
/// monitors, with `RUST_LOG=info`, and watch for the line") could not have
/// worked. Android routes `log` into logcat (`examples/android.rs`); this is
/// the desktop's equivalent, without a new dependency.
#[cfg(not(target_os = "android"))]
struct StderrLog(log::LevelFilter);

#[cfg(not(target_os = "android"))]
impl log::Log for StderrLog {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= self.0
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("[{} {}] {}", record.level(), record.target(), record.args());
        }
    }

    fn flush(&self) {}
}

#[cfg(not(target_os = "android"))]
fn install_logger() {
    let Some(spec) = std::env::var_os("RUST_LOG") else {
        return;
    };
    let level = match spec.to_string_lossy().trim() {
        "trace" => log::LevelFilter::Trace,
        "debug" => log::LevelFilter::Debug,
        "warn" => log::LevelFilter::Warn,
        "error" => log::LevelFilter::Error,
        _ => log::LevelFilter::Info,
    };
    // `set_logger` wants a `'static`; leaking one small value once, at
    // startup, is the plain way to give it that (no `alloc` feature needed,
    // which `set_boxed_logger` would be).
    let logger: &'static StderrLog = Box::leak(Box::new(StderrLog(level)));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(level);
    }
}

#[cfg(not(target_os = "android"))]
fn main() {
    install_logger();
    // The suite reports the CPU raster by kind of command
    // (`raster_by_kind`); set before the renderer exists, which reads it.
    std::env::set_var("VIEWW_RASTER_PROFILE", "1");
    let perf = screen::PerfLink::new();
    let stress = screen::StressLink::new();
    let device = DeviceSuite::new();
    let desktop = DesktopSuite::new(&device);

    // Before the loop: none of it needs a frame, and `data_dir` and the
    // shortcut modifier are answers a broken build gets wrong before it draws
    // anything.
    desktop.environment();

    let app = App::new().title("vieww — desktop suite").size(WINDOW);
    // `VIEWW_GPU=1` for the planned GPU path, `=auto` to measure both on this
    // machine (`App::auto_rasterizer`), as the phone entry points take them.
    let app = match std::env::var("VIEWW_GPU").as_deref() {
        Ok("1") => app.prefer_gpu(),
        Ok("auto") => app.auto_rasterizer(),
        _ => app,
    };
    // Taken before `run`, per `PlatformWindows`' own docs: the handle has to
    // exist before there is a loop to back it.
    let windows = app.windows();

    let result = app
        .on_frame(perf.hook())
        .on_frame(stress.hook())
        .on_frame(device.hook())
        .after_frame(device.tap_targets())
        .after_frame(desktop.surface(WINDOW))
        .before_frame(device.modal_semantics())
        .before_frame(desktop.clipboard())
        .before_frame(desktop.windows(windows))
        .run(|driver| {
            let demo = screen::Demo::new(driver);
            perf.attach(&demo);
            stress.attach(&demo);
            let probe = device.probe(demo.taps());
            device.watch_drawer(demo.drawer());
            // `VIEWW_SCREEN=stress` for the heavy screen the phones run too.
            driver.set_root(if screen::wants_stress() {
                demo.stress_screen(TargetPlatform::current(), probe)
            } else {
                demo.screen("A frame, on a desktop.", TargetPlatform::current(), probe)
            });
        });

    match result {
        Ok(report) => println!("{report}"),
        Err(error) => eprintln!("vieww failed: {error}"),
    }
}
