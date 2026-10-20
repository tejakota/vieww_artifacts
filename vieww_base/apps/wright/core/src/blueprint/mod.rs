//! The blueprint: one typed file that IS the project.
//!
//! * [`model`] — the data model (agents, edges, chips, guardrails, ...).
//! * [`parse`] — the KDL-dialect parser that reads `blueprint.kdl`.
//! * [`canon`] — the canonical serialization; its hash IS the commit.
//! * [`validate`] — structural checks before anything is allowed to run.
//! * [`diff`] — structural diff and the refire plan (the moat).

pub mod canon;
pub mod diff;
pub mod model;
pub mod parse;
pub mod validate;

// Flat re-export: the rest of the workspace says
// `wright_core::blueprint::Blueprint`, not `...::model::Blueprint`.
pub use model::*;
