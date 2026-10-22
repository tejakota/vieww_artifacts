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

/// The secrets an agent's identity is delegated. Cards that name them
/// (`secret "smtp"`) get exactly those. A blueprint where no card names
/// any keeps working the old way, narrowed: only agents holding a tool
/// that sends (an outbound channel) get the blueprint's secrets; nobody
/// else ever did need them.
pub fn secrets_for(bp: &Blueprint, a: &AgentDef) -> Vec<String> {
    if bp.agents.iter().any(|x| !x.secrets.is_empty()) {
        return a.secrets.clone();
    }
    let sends = a.tools.iter().any(|t| {
        crate::palette::find(t).is_some_and(|spec| spec.effect == crate::palette::Effect::World)
    });
    if sends {
        bp.chips.secrets.iter().map(|s| s.name.clone()).collect()
    } else {
        Vec::new()
    }
}
