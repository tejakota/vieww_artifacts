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

pub mod agent;
pub mod approve;
pub mod export;
pub mod firing;
pub mod interview;
pub mod mock;
pub mod scheduler;
pub mod spine;

pub use firing::{fire, load_blueprint, FireOptions, RunSummary, TaskStatus, Trigger};
