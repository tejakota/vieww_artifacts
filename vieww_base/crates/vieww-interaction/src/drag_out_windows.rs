//! Windows's outbound drag, through OLE `DoDragDrop` — the platform bridge
//! PENDING §3.1 names first.
//!
//! # Status: type-checked, never run
//!
//! The same standing as [`crate::ios`] on the other end of the platform
//! list, with one further honest limit: `cargo check
//! --target x86_64-pc-windows-msvc` proves the signatures line up against
//! the fetched `windows` 0.58 crate (every feature name and every method
//! shape here was verified against that source, not against memory), and
//! proves nothing about what the shell does with them. The parts most
//! likely to be wrong on a first real run are the ones this file's own
//! docs mark **[session-verifies]**. The pure half — the `CF_HDROP` byte
//! layout and the effect mapping — lives in [`super::drag_out`] and *is*
//! unit-tested on every machine, because those are the parts where a
//! mistake ships silent garbage.
//!
//! # What a run must confirm (the [session-verifies] list)
//!
//! 1. **`SHCreateDataObject(None, None, None)` accepts `SetData` with
//!    `CF_HDROP`.** The empty shell data object is documented as a
//!    standard `IDataObject`; some shells refuse `SetData` on formats it
//!    did not create. If it refuses, the fix is a hand-rolled
//!    `IDataObject` (~200 lines of `EnumFormatEtc` plumbing this file
//!    deliberately does not carry until a session says it is needed).
//! 2. **The thread calling `start_drag` has `OleInitialize`d.** winit's
//!    Windows event loop initialises OLE for its own drag *target* support,
//!    but that is a property of winit's implementation, not its contract —
//!    a session run settles it, and the fallback is calling `OleInitialize`
//!    here with the documented pairing `OleUninitialize`.
//! 3. **Blocking is acceptable on the calling path.** `DoDragDrop` runs
//!    its own modal loop until the drop finishes; `PlatformDragStarter`
//!    is a synchronous trait by design, but the caller that invokes it on
//!    the event-loop thread blocks the loop for the drag's duration —
//!    which is how every native Windows application does it, and what the
//!    adapter that wires this starter should document.
//!
//! # Why the data object is the shell's, not ours
//!
//! A drag's data object is mostly bookkeeping: format enumeration, global
//! memory juggling, `GetData`/`SetData` plumbing with no decisions in it.
//! The shell has shipped that bookkeeping for thirty years, and
//! `SHCreateDataObject` with no pidls hands it over empty and standard.
//! Writing our own would be ~200 lines of unsafe whose only failure mode
//! is a target that reads garbage; delegating makes the one custom part —
//! the drop source, which *does* hold decisions (when to cancel, what to
//! tell the cursor) — small enough to review line by line.

use std::path::PathBuf;

use windows::core::implement;
use windows::Win32::Foundation::{
    GlobalFree, BOOL, DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, HGLOBAL, S_OK,
};
use windows::Win32::System::Com::{
    IDataObject, DVASPECT_CONTENT, FORMATETC, STGMEDIUM, TYMED_HGLOBAL,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT,
};
use windows::Win32::System::Ole::{
    DoDragDrop, IDropSource, IDropSource_Impl, ReleaseStgMedium, CF_HDROP, DROPEFFECT,
    DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE, DROPEFFECT_NONE,
};
use windows::Win32::UI::Shell::SHCreateDataObject;

use super::drag_out::{effect_from_flags, hdrop_bytes, DragError, DropEffect, OutgoingDrag};

/// The Windows [`PlatformDragStarter`](super::drag_out::PlatformDragStarter).
///
/// Holds the effect the last drag ended with — `DoDragDrop` reports it as
/// an out-parameter, and [`DropEffect`] reporting is the source's
/// business, so the starter keeps it where the adapter can read it after
/// the call returns.
#[derive(Debug, Default)]
pub struct WindowsDragStarter {
    /// The effect the last drag ended with, behind a `Mutex` for the
    /// mundane reason: [`PlatformDragStarter::start_drag`] takes `&self`
    /// (the trait's shape was fixed before any platform implementation
    /// existed, and `&mut self` there would ripple through every existing
    /// caller for one field), and `DoDragDrop` reports its outcome as an
    /// out-parameter.
    last_effect: std::sync::Mutex<Option<DropEffect>>,
}

impl WindowsDragStarter {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            last_effect: std::sync::Mutex::new(None),
        }
    }

    /// What the last drag this starter ran ended with — `None` before any
    /// drag and after a cancelled one, which is the same distinction
    /// [`DragSessionState::Finished`](super::drag_out::DragSessionState::Finished)
    /// carries, and the reason it is `Option` there too.
    ///
    /// # Panics
    /// Never returns normally poisoned — the mutex guards one
    /// uncontended `Option` write per drag — but this is a `Result`-free
    /// accessor, so a genuinely poisoned lock (a panic *inside*
    /// `start_drag` between assignment attempts) surfaces here.
    #[must_use]
    pub fn last_effect(&self) -> Option<DropEffect> {
        *self.last_effect.lock().expect("effect lock poisoned")
    }
}

impl super::drag_out::PlatformDragStarter for WindowsDragStarter {
    fn start_drag(&self, drag: &OutgoingDrag) -> Result<(), DragError> {
        // Only files go out as a `CF_HDROP`. Text and URIs need
        // `CF_UNICODETEXT`/`CFSTR_INETURLW` globals — the same shape as
        // `hdrop_bytes` with a different format id — and adding them is
        // the honest next step once a session has confirmed the file path
        // end to end; landing all three formats unverified would be three
        // guesses instead of one.
        let paths: Vec<PathBuf> = drag
            .forms
            .iter()
            .filter_map(|form| match form {
                super::drag_out::DragContent::Files(files) => Some(files.clone()),
                _ => None,
            })
            .flatten()
            .collect();
        if paths.is_empty() {
            // An honest refusal, not a drag with nothing in it: the
            // session-verifies list above has not confirmed the text
            // format, so a text-only drag cannot be started yet.
            return Err(DragError::NotSupportedByPlatform);
        }

        // SAFETY: `SHCreateDataObject` with no pidls creates the standard
        // empty data object; no resources are borrowed past the call.
        let data_object: IDataObject =
            unsafe { SHCreateDataObject::<_, IDataObject>(None, None, None) }
                .map_err(|_| DragError::NotSupportedByPlatform)?;

        let bytes = hdrop_bytes(&paths);
        // SAFETY (whole block): `GMEM_MOVEABLE | GMEM_ZEROINIT` allocation,
        // one lock, one copy, one unlock — the exact sequence `DROPFILES`
        // payloads are documented to need. `GlobalUnlock` is a no-op error
        // on the last lock, which is why its result is discarded.
        let hglobal: HGLOBAL = unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len()) }
            .map_err(|_| DragError::NotSupportedByPlatform)?;
        unsafe {
            let locked = GlobalLock(hglobal);
            if locked.is_null() {
                let _ = GlobalFree(hglobal);
                return Err(DragError::NotSupportedByPlatform);
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), locked.cast(), bytes.len());
            let _ = GlobalUnlock(hglobal);
        }

        // SAFETY: zeroing the union before `hGlobal` is set — a `STGMEDIUM`
        // must not be read before every field is written, and none is.
        let mut medium: STGMEDIUM = unsafe { std::mem::zeroed() };
        medium.tymed = TYMED_HGLOBAL.0 as u32;
        medium.pUnkForRelease = std::mem::ManuallyDrop::new(None);
        medium.u.hGlobal = hglobal;

        let format = FORMATETC {
            cfFormat: CF_HDROP.0,
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            ptd: std::ptr::null_mut(),
            tymed: medium.tymed,
        };

        // SAFETY: a live data object, a fully initialised format and
        // medium, and `fRelease: TRUE` — ownership of `hglobal` transfers
        // to the data object, which is why nothing here frees it.
        unsafe {
            if let Err(error) = data_object.SetData(&format, &medium, BOOL(1)) {
                ReleaseStgMedium(&mut medium);
                drop(error);
                return Err(DragError::NotSupportedByPlatform);
            }
        }

        let allowed = allowed_mask(drag);
        // The `implement` macro's generated `Into<IDropSource>` — the
        // interface hands the shell its own ref-counted view of this
        // struct, and the shell's releases drive its lifetime.
        let source: IDropSource = DropSource.into();
        let mut effect = DROPEFFECT_NONE;

        // SAFETY: live data object and drop source; this thread is the one
        // the drag runs its modal loop on (session-verifies #2); the
        // out-parameter is written by the shell before the call returns.
        let _hr = unsafe { DoDragDrop(&data_object, &source, allowed, &mut effect) };
        // The HRESULT is deliberately not inspected: `DRAGDROP_S_DROP` and
        // `DRAGDROP_S_CANCEL` are *success* codes the shell returns for the
        // two ordinary endings, `effect` carries which, and an error here
        // (no OLE, bad object) leaves `effect` at NONE — the same answer a
        // cancelled drag gets, which is the truthful one for the source's
        // bookkeeping either way. A session run that wants the distinction
        // logs `_hr` first; this starter has no log to write it to.

        // The starter is `&self`, so the effect goes out through
        // interior mutability — `PlatformDragStarter`'s shape was fixed by
        // the trait before any platform implementation existed, and a
        // `&mut self` there would ripple through every existing caller for
        // one field.
        *self.last_effect.lock().expect("effect lock poisoned") = effect_from_flags(effect.0);
        Ok(())
    }
}

fn allowed_mask(drag: &OutgoingDrag) -> DROPEFFECT {
    let mut mask = DROPEFFECT_NONE;
    for effect in &drag.allowed_effects {
        mask |= match effect {
            DropEffect::Copy => DROPEFFECT_COPY,
            DropEffect::Move => DROPEFFECT_MOVE,
            DropEffect::Link => DROPEFFECT_LINK,
        };
    }
    mask
}

/// The drop source. Two decisions, both the shell's contract:
/// `QueryContinueDrag` cancels on Escape and finishes when every button is
/// up; `GiveFeedback` returns `S_OK` to let the shell draw its own cursor —
/// which is what every stock source does, and anything custom here would
/// be a claim about visuals no session has verified.
#[implement(IDropSource)]
struct DropSource;

impl IDropSource_Impl for DropSource_Impl {
    fn QueryContinueDrag(
        &self,
        fescapepressed: BOOL,
        grfkeystate: windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS,
    ) -> windows::core::HRESULT {
        let _ = grfkeystate;
        if fescapepressed.as_bool() {
            DRAGDROP_S_CANCEL
        } else if grfkeystate == windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS(0) {
            // Every button up: the drop happened. (A source that wanted to
            // distinguish buttons would read the flags; this one does what
            // the shell's own stock source does.)
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    fn GiveFeedback(&self, _dweffect: DROPEFFECT) -> windows::core::HRESULT {
        S_OK
    }
}
