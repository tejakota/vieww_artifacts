//! The three things a host has to supply, and nothing else.
//!
//! Everything above this line is one program. Below it there are three: an
//! Android activity, an iOS bundle and a desktop window. The seam is this small
//! on purpose — a host that could do more than these four things would be a
//! place for behaviour to diverge, and two hosts that diverge is the situation
//! the Slint build was in with two separate UIs.
//!
//! Note what is *not* here: no navigation, no theming, no formatting, no
//! policy. A host cannot change what the app does. It can open a picker, open
//! bytes, run work off the UI thread, and say where its own storage is.

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use vavlt_engine::Source;
use vieww::foundation::task::FrameWaker;

use crate::state::Message;

/// What the app needs from the thing it is running inside.
pub trait Platform: 'static {
    /// Where the audit log lives.
    ///
    /// The app's own private storage on every target — internal data on
    /// Android, the Application Support directory on iOS, a config dir on a
    /// desktop. Never anywhere the user's photographs are.
    fn data_dir(&self) -> PathBuf;

    /// How to read the bytes behind a handle this host issued.
    fn source(&self) -> Arc<dyn Source>;

    /// Ask the OS for up to `max` files.
    ///
    /// Asynchronous on every platform — an out-of-process picker cannot be
    /// anything else — so the result arrives as a [`Message`] on `reply`, and
    /// `waker` is what turns that into a frame rather than a value sitting in a
    /// channel until the user happens to touch the screen.
    ///
    /// **This is the only ingress.** SPEC §6: the app declares no media
    /// permission on any platform, so a file that did not come through here
    /// cannot be reached at all.
    fn pick(&self, max: usize, reply: Sender<Message>, waker: Arc<dyn FrameWaker>);

    /// Run `work` somewhere that is not the UI thread.
    ///
    /// The same seam `vieww_foundation::task::Spawn` is, and for the same
    /// reason: a host that already has a runtime should not be made to start a
    /// second one. A bare `std::thread::spawn` is a perfectly good answer.
    fn spawn(&self, work: Box<dyn FnOnce() + Send + 'static>);

    /// Hand an exported file to the OS to do something with — a share sheet, a
    /// file manager, a "reveal in finder".
    ///
    /// The default does nothing but log, because on a desktop the file being
    /// written where the user can find it *is* the whole feature.
    fn share(&self, path: &std::path::Path) {
        log::info!("exported: {}", path.display());
    }
}
