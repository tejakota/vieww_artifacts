//! # wright-core
//!
//! The pure heart of the harness. Everything here is deterministic,
//! std-only, side-effect free logic:
//!
//! * [`blueprint`] — the typed project definition: model, KDL-dialect
//!   parser, canonical form, validation, structural diff and the
//!   refire plan (the incremental-rebuild moat).
//! * [`compiler`] — turns a validated blueprint into a scheduled,
//!   permission-annotated [`compiler::TaskGraph`], injecting the
//!   system-agent workforce from capability chips.
//! * [`events`] — the event spine types: append-only, deterministically
//!   ordered, replayable, forkable.
//! * [`wire`] — from-scratch wire protocols: JSON, HTTP/1.1, SSE.
//! * [`hash`] — BLAKE3 (ported) for commits, cache keys and the store.
//! * [`schedule`] — cron expressions and TZif/POSIX time zones.
//! * [`convention`] — the universal project skeleton (Shipwright's job).
//! * [`confirm`] — the Confirm Gate summary, generated from the typed
//!   blueprint by code, never by a model.
//! * [`palette`] — the typed tool vocabulary shared by blueprints,
//!   the permission gate, and the cache.
//!
//! Layering rule: core depends on nothing. Everything else may depend
//! on core. Nothing in core may reach outward.

pub mod blueprint;
pub mod compiler;
pub mod confirm;
pub mod convention;
pub mod ed25519;
pub mod events;
pub mod hash;
pub mod palette;
pub mod recipe;
pub mod rule_of_two;
pub mod schedule;
pub mod schema;
pub mod sha256;
pub mod sha512;
pub mod wire;
pub mod zip;
