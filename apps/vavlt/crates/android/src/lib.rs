//! vavlt for Android.
//!
//! Rust end to end: `android-activity` hosts a **vieww** tree, `vavlt-app`
//! draws every screen, `vavlt-engine` does the encoding and proves its own
//! results. The one Java file in the repo is the picker shim SPEC §6 shows is
//! unavoidable, and it is compiled and dexed by `build.rs` — there is no Gradle
//! project, no Kotlin, and no AGP anywhere in this build.
//!
//! # What this crate is now
//!
//! A `Platform` impl and an `android_main`. That is all. It used to be 663
//! lines of state plus eleven `.slint` files; the state moved to
//! `vavlt_app::VavltState` — where the desktop and iOS builds read the same
//! signals — and the screens moved to `vavlt_app`'s own `screens` module. What
//! is left here is the two things only Android can answer: where its private
//! storage is, and how to reach the system photo picker.
//!
//! The generated manifest still has **zero `<uses-permission>` entries**. No
//! `INTERNET`, no media permissions. Files arrive only as content URIs from the
//! out-of-process photo picker, and the descriptor is opened after the
//! Transform grant, not before — so the tier and estimate screens genuinely run
//! on metadata alone.

// Both are JNI, so both are Android-only. On a laptop this crate builds to an
// empty cdylib, which is the honest answer: an Android host with no Android
// under it has nothing to offer. The *interface* it mounts is checked from a
// laptop by `vavlt-app`'s own suite, which is where it belongs.
#[cfg(target_os = "android")]
mod jni_bridge;
#[cfg(target_os = "android")]
mod picker;

#[cfg(target_os = "android")]
use std::path::PathBuf;
use std::time::Duration;
#[cfg(target_os = "android")]
use std::rc::Rc;
#[cfg(target_os = "android")]
use std::sync::mpsc::Sender;
#[cfg(target_os = "android")]
use std::sync::Arc;

#[cfg(target_os = "android")]
use vavlt_app::{Message, Platform, Pump, Scrolls, VavltApp, VavltState};
#[cfg(target_os = "android")]
use vavlt_engine::Source;
#[cfg(target_os = "android")]
use vieww::foundation::task::FrameWaker;
#[cfg(target_os = "android")]
use vieww::prelude::*;

#[cfg(target_os = "android")]
use jni_bridge::Picker;
#[cfg(target_os = "android")]
use picker::PickerSource;

/// The Android host.
#[cfg(target_os = "android")]
struct Android {
    picker: Option<Arc<Picker>>,
    data_dir: PathBuf,
}

#[cfg(target_os = "android")]
impl Platform for Android {
    fn data_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }

    fn source(&self) -> Arc<dyn Source> {
        match &self.picker {
            Some(picker) => Arc::new(PickerSource::new(picker.clone())),
            // The shim failed to load. Every read fails with that message
            // rather than the app pretending it can open things.
            None => Arc::new(picker::Unavailable),
        }
    }

    fn pick(&self, max: usize, reply: Sender<Message>, waker: Arc<dyn FrameWaker>) {
        let Some(picker) = self.picker.clone() else {
            let _ = reply.send(Message::PickFailed(
                "The photo picker is unavailable on this device.".into(),
            ));
            waker.wake();
            return;
        };

        // Off the UI thread, parking on the shim's state rather than blocking a
        // frame. `PickState` is *polled* because `android-activity` does not
        // surface `onActivityResult` at all — which is the whole reason the
        // Java shim exists (SPEC §6).
        std::thread::spawn(move || {
            let message = match picker::await_selection(&picker, max) {
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
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(android: vieww_platform_winit::AndroidApp) {
    use vieww_platform_winit::App;

    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );

    let data_dir = android
        .internal_data_path()
        .unwrap_or_else(|| PathBuf::from("/data/local/tmp"));
    if let Err(error) = std::fs::create_dir_all(&data_dir) {
        log::error!("could not create {}: {error:#}", data_dir.display());
    }

    let picker = match Picker::new(&android) {
        Ok(picker) => Some(Arc::new(picker)),
        Err(error) => {
            // The app is still usable — it just cannot be handed anything,
            // which is exactly what it should say rather than crashing.
            log::error!("picker shim unavailable: {error:#}");
            None
        }
    };

    let pump = Pump::new();
    let app = App::new().title("vavlt").on_frame({
        // The pump is what turns a codec worker's results into signal writes.
        // Without it every message sits in its channel for ever and the run
        // screen never moves.
        let mut hook = pump.hook();
        move |_log| hook()
    });
    let waker = Arc::new(app.waker());

    let result = app.run_android(android, move |driver: &mut FrameDriver| {
        let runtime = driver.elements().runtime().clone();

        let platform = Rc::new(Android { picker, data_dir });
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

    if let Err(error) = result {
        log::error!("vavlt failed: {error}");
    }
}


