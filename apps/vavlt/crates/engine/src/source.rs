//! The only ingress, as a trait.
//!
//! SPEC §1.1 says the engine receives OS-issued handles and never paths it
//! chose itself, and §6 says access comes exclusively from out-of-process
//! pickers. A trait is how that invariant survives having two hosts: this crate
//! cannot enumerate anything, cannot open anything it was not handed, and has
//! no `std::fs` call in it at all. Whatever the host is willing to open is
//! exactly what the engine can see.
//!
//! It is deliberately one method. A larger surface — list, stat, walk — would
//! be the ambient authority the invariant exists to refuse.

use anyhow::Result;

/// An opaque reference to one file, issued by the host.
///
/// A `content://` URI on Android, a path on a desktop. The engine never parses
/// one: it hashes it into the manifest and hands it back to the [`Source`], so
/// a host is free to make it a token that means nothing outside itself.
pub type Handle = String;

/// The host's answer to "give me the bytes behind this handle".
///
/// `Send + Sync` because the run happens on a worker thread — the whole reason
/// the UI stays responsive while a hundred JPEGs go through Lepton.
pub trait Source: Send + Sync {
    /// The file's bytes, or why they could not be read.
    ///
    /// An `Err` here is a per-file failure, not a run failure: it is reported
    /// as [`Proof::Failed`](crate::Proof) against that one file, logged, and
    /// the run carries on. A picker grant that lapsed mid-run should cost the
    /// user the rest of the file, not the rest of the run.
    fn read(&self, handle: &str) -> Result<Vec<u8>>;

    /// A pre-rendered thumbnail, if the platform has one cheaper than decoding.
    ///
    /// Android does — `ContentResolver.loadThumbnail` reads the camera's own
    /// embedded preview instead of a 12-megapixel JPEG. The default returns
    /// `None`, which sends the caller to [`thumbnail`](crate::thumbnail) over
    /// the full bytes: correct everywhere, just slower.
    ///
    /// **This is metadata-adjacent, not metadata.** It opens the file, so a
    /// host must not call it before the Transform grant — which is why the tier
    /// and estimate screens ask for counts and sizes and never for a picture.
    fn thumbnail(&self, handle: &str, edge: u32) -> Option<Vec<u8>> {
        let _ = (handle, edge);
        None
    }
}
