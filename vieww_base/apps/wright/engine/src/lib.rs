//! # wright-engine
//!
//! The firing machine, built on the other crates:
//!
//! * [`firing`] — one firing, blueprint in, receipt out: stages,
//!   parallelism, the cache, the delivery gate, resume, receipts.
//! * [`agent`] — the agent loop (model turns, tool calls through the
//!   gate, approvals, budgets, compaction).
//! * [`interview`] — DECLARE: words in, a confirmed blueprint out.
//! * [`scheduler`] — the background service that fires schedules.
//! * [`approve`] — how a paused call reaches a human.
//! * [`spine`] — the event sink every step writes through.
//! * [`mock`] — an offline model provider for tests and demos.
//! * [`verify`] — `wright verify`: five checks over a receipt (ADR-025).
//! * [`explain`] — one screen that says what a run did (the memo's
//!   "explain any run in one screen").
//! * [`cost`] — the spending promise: the estimate shown before a run.
//! * [`handoff`] — the project as a PR folder for a human (ADR: the
//!   memo's "hand to a developer").
//! * [`bench`] — time to a checked outcome, interventions per recipe:
//!   the public benchmark (ADR-030).
//! * [`recipe`] — the CLI-facing recipe gallery (list/install/sign/verify
//!   on top of wright_core::recipe).

pub mod agent;
pub mod approve;
pub mod bench;
pub mod cost;
pub mod explain;
pub mod export;
pub mod firing;
pub mod handoff;
pub mod interview;
pub mod mock;
pub mod recipe;
pub mod scheduler;
pub mod spine;
pub mod test_support;
pub mod verify;

pub use firing::{fire, load_blueprint, FireOptions, RunSummary, TaskStatus, Trigger};
