//! What the app does with the files, and the rules it does it under.
//!
//! This used to live inside the Android crate, which meant the desktop build
//! reimplemented the same run loop against `std::fs` and the two could drift.
//! They cannot now: there is one run, and the only thing a host supplies is a
//! [`Source`] — the answer to "give me the bytes behind this handle". On a
//! phone that is a content URI opened through the system picker's grant; on a
//! desktop it is a path the user chose in a file dialog. Neither the codecs nor
//! the policy can tell the difference, which is the point.
//!
//! Everything here is the same `vavlt-core` / `vavlt-codecs` code the CLI
//! harness runs, so a claim proven on a laptop and a claim proven on a phone
//! are proven by the same bytes. Nothing in this crate is a mock.
//!
//! One deliberate limitation, stated rather than hidden: this build has no
//! perceptual scorer (SSIMULACRA2 and libvmaf are both pending in SPEC §5), so
//! Deep Move cannot qualify a lossy encode. Invariant 4 says the fallback is
//! lossless, never a bad encode, and invariant 8 says an unscored ratio is
//! never quoted as a saving. So Deep Move runs the lossless path and reports
//! the difference as `unqualified` — visible in the UI, logged in the audit,
//! and excluded from the headline figure.

mod audit;
mod fmt;
mod item;
mod run;
mod source;
mod thumb;

pub use audit::{Audit, Entry};
pub use fmt::{fmt_bytes, fmt_time};
pub use item::{manifest_hash, Item, Outcome, Proof};
pub use run::{run, Event};
pub use source::{Handle, Source};
pub use thumb::{thumbnail, THUMB_EDGE};
