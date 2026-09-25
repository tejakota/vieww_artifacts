//! The iOS ingress: `PHPickerViewController`, and the container it writes to.
//!
//! # Status: written, never run
//!
//! Stated here rather than in a release note. Everything in the `ios` half of
//! this file has been compiled against `objc2`'s types and has **not executed
//! on a device or a simulator**, because this workspace has no Mac attached.
//! The Android path opposite it has run on real hardware; this has not, and
//! nothing below should be read as if it had.
//!
//! What that means in practice: treat the selector names, the delegate
//! lifetime, and the security-scoped URL dance as *the design*, and expect the
//! first device run to correct at least one of them. The interface above it is
//! unaffected either way — `vavlt-app` never learns that its handles are
//! `file://` URLs rather than `content://` ones.
//!
//! # Why PHPicker and not `UIImagePickerController`
//!
//! Same reason Android uses the photo picker rather than `READ_MEDIA_IMAGES`:
//! `PHPickerViewController` runs **out of process** and hands back item
//! providers for exactly what the user chose. The app declares no
//! `NSPhotoLibraryUsageDescription` and iOS shows no permission prompt, because
//! there is no library access to grant — which is SPEC §1.1 and §6 holding on a
//! second platform for the same structural reason rather than by convention.

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use anyhow::Result;
use vavlt_app::Message;
use vavlt_engine::Source;
use vieww::foundation::task::FrameWaker;

/// Reads the files a picker handed over.
///
/// The handles are `file://` URLs into the app's own temporary container:
/// `PHPickerViewController` gives item providers, and the standard way to get
/// bytes out of one is to have it write a copy somewhere the app owns. So the
/// copy *is* the capability here, in the same way the descriptor is on Android
/// — and it is why [`present`] copies before it reports rather than handing
/// back a library identifier the engine would have to resolve later.
#[derive(Debug, Default)]
pub struct PhotoSource {
    _private: (),
}

impl PhotoSource {
    #[must_use]
    pub const fn new() -> Self {
        Self { _private: () }
    }
}

impl Source for PhotoSource {
    fn read(&self, handle: &str) -> Result<Vec<u8>> {
        // Deliberately `std::fs` and not an `NSData` round trip. The file is in
        // the app's own container by the time this runs — `present` put it
        // there — so there is no security-scoped resource to start and stop,
        // and reaching for Foundation would only add a copy.
        let path = PathBuf::from(handle);
        anyhow::ensure!(
            path.starts_with(picked_dir()),
            "{handle} is not one of the files the picker handed over"
        );
        std::fs::read(&path).map_err(|error| anyhow::anyhow!("reading {handle}: {error}"))
    }
}

/// Where the picker's copies live. Inside the app container, and nowhere near
/// the user's library.
fn picked_dir() -> PathBuf {
    app_support_dir().join("picked")
}

// --- the device path -------------------------------------------------------

#[cfg(target_os = "ios")]
mod device {
    use super::*;

    use objc2::rc::Retained;
    use objc2_foundation::{NSSearchPathDirectory, NSSearchPathDomainMask, NSString};

    /// `~/Library/Application Support`, inside this app's sandbox container.
    ///
    /// Not `Documents`: that directory is exposed to the Files app when the
    /// bundle opts in, and the audit log is the app's own record rather than a
    /// user document. It is also backed up, which is correct — a hash chain
    /// that does not survive a device migration is a hash chain that ends every
    /// time somebody buys a phone.
    pub fn app_support_dir() -> PathBuf {
        let paths = unsafe {
            objc2_foundation::NSSearchPathForDirectoriesInDomains(
                NSSearchPathDirectory::NSApplicationSupportDirectory,
                NSSearchPathDomainMask::NSUserDomainMask,
                true,
            )
        };

        let first: Option<Retained<NSString>> = paths.firstObject();
        match first {
            Some(path) => PathBuf::from(path.to_string()).join("vavlt"),
            // A sandboxed app always has this directory. If Foundation ever
            // says otherwise, the temporary directory keeps the app running
            // with a log that does not survive a restart, which is strictly
            // better than refusing to launch.
            None => std::env::temp_dir().join("vavlt"),
        }
    }

    /// Present the system photo picker.
    ///
    /// **Unimplemented, and failing loudly rather than silently.** The delegate
    /// class this needs is a `declare_class!` with one method
    /// (`picker:didFinishPicking:`) that has to outlive the presentation, copy
    /// each `NSItemProvider` into [`picked_dir`], and post the results. That is
    /// forty lines of `objc2` that must not be guessed at: a delegate that is
    /// released too early is a crash on the user's first tap, and it cannot be
    /// found without running it.
    ///
    /// So the app reports a picker it cannot open, which is the same thing the
    /// Android host does when its shim fails to load — a state the UI already
    /// draws, on the vault screen, in a red card.
    pub fn present(_max: usize, reply: Sender<Message>, waker: Arc<dyn FrameWaker>) {
        log::error!("the iOS photo picker is not wired up in this build");
        let _ = reply.send(Message::PickFailed(
            "The photo picker is not available in this build of vavlt for iPhone.".into(),
        ));
        waker.wake();
    }
}

// --- everywhere else -------------------------------------------------------

#[cfg(not(target_os = "ios"))]
mod device {
    use super::*;

    /// The desktop stand-in, so this crate builds and runs on a laptop.
    pub fn app_support_dir() -> PathBuf {
        std::env::var("HOME")
            .map(|home| PathBuf::from(home).join(".local/share/vavlt-ios"))
            .unwrap_or_else(|_| std::env::temp_dir().join("vavlt-ios"))
    }

    pub fn present(_max: usize, reply: Sender<Message>, waker: Arc<dyn FrameWaker>) {
        let _ = reply.send(Message::PickFailed(
            "This is the iOS build running on a desktop — there is no photo picker here. \
             Use `cargo run -p vavlt-desktop` to walk a folder instead."
                .into(),
        ));
        waker.wake();
    }
}

pub use device::{app_support_dir, present};
