//! The picker, as the app sees it: a `Source` and one blocking call.
//!
//! `jni_bridge` is the JNI. This is the thin layer that turns it into the two
//! things `vavlt-app` asks a host for, and it is separate so the JNI file stays
//! about JNI — every `unsafe` and every method signature in this crate is in
//! that file and none is in this one.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use vavlt_engine::{Item, Source};

use crate::jni_bridge::{PickState, Picker};

/// How long to wait between asking the shim whether the user has finished.
///
/// 120ms, not 16. Nothing is waiting on this — it runs on its own thread and
/// reports through a channel — and a picker session lasts seconds, so a tighter
/// poll would only wake a core to learn nothing. `android-activity` does not
/// surface `onActivityResult`, which is why this is a poll at all (SPEC §6).
const POLL: Duration = Duration::from_millis(120);

/// Give up rather than leaking a thread if the shim never answers.
///
/// Ten minutes is long enough that a user browsing an enormous library is not
/// cut off, and short enough that a shim wedged by an OEM launcher does not
/// leave a thread parked for the life of the process.
const PATIENCE: Duration = Duration::from_secs(600);

/// Reads content URIs through the grant the picker issued.
pub struct PickerSource {
    picker: Arc<Picker>,
}

impl PickerSource {
    #[must_use]
    pub const fn new(picker: Arc<Picker>) -> Self {
        Self { picker }
    }
}

impl std::fmt::Debug for PickerSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PickerSource")
    }
}

impl Source for PickerSource {
    fn read(&self, handle: &str) -> Result<Vec<u8>> {
        self.picker.read(handle)
    }

    fn thumbnail(&self, handle: &str, edge: u32) -> Option<Vec<u8>> {
        // `ContentResolver.loadThumbnail` reads the camera's own embedded
        // preview rather than decoding a twelve-megapixel JPEG, which is the
        // difference between a grid that fills in and one that stalls.
        self.picker.thumbnail(handle, edge as i32)
    }
}

/// A host with no picker at all.
///
/// Used when the dex fails to load. Every read fails with a message the UI
/// already knows how to show, rather than the app pretending it can open things
/// and crashing on the first tap.
#[derive(Debug)]
pub struct Unavailable;

impl Source for Unavailable {
    fn read(&self, _handle: &str) -> Result<Vec<u8>> {
        Err(anyhow!("the photo picker is unavailable on this device"))
    }
}

/// Launch the picker and block until the user is done.
///
/// Runs on a worker thread. Returns an empty vector for a cancel — that is not
/// an error, and reporting it as one would put a red card on the vault screen
/// every time somebody changed their mind.
pub fn await_selection(picker: &Picker, max: usize) -> Result<Vec<Item>> {
    log::info!("picker: launching for up to {max} files");
    picker.reset();
    picker.launch(max.min(i32::MAX as usize) as i32)?;

    let deadline = std::time::Instant::now() + PATIENCE;
    let mut last = PickState::Idle;
    loop {
        let state = picker.state();
        if state != last {
            // Every transition, because the failure this logging was added for
            // is one where nothing appears and nothing errors — and the only
            // way to tell "never launched" from "launched and refused" is to
            // watch the state.
            log::info!("picker: {last:?} -> {state:?}");
            last = state;
        }
        match state {
            PickState::Done => break,
            PickState::Cancelled => {
                log::info!("picker: cancelled by the user");
                return Ok(Vec::new());
            }
            PickState::Idle | PickState::Pending => {
                if std::time::Instant::now() > deadline {
                    log::error!("picker: gave up after {PATIENCE:?} in {state:?}");
                    return Err(anyhow!("the picker never came back"));
                }
                std::thread::sleep(POLL);
            }
        }
    }

    let picked = picker.take_selection()?;
    picker.reset();
    log::info!("picker: {} files came back", picked.len());

    Ok(picked
        .into_iter()
        .map(|item| Item::new(item.uri, item.name, item.bytes))
        .collect())
}
