//! The task cache: Bazel's action-cache semantics, in miniature.
//!
//! A cache key is a pure function of everything that can change what a
//! task produces:
//!
//! ```text
//! key = BLAKE3(
//!     task definition (canonical),   // project agent: goal, tier, tools, memory,
//!                                    //   in/out, retries, permissions
//!                                    // system agent: its prompt contract
//!     resolved model id,             // primary + fallback
//!     input artifact hashes,         // the `in` contract, content-hashed
//!     tool versions,                 // palette version per tool
//!     permission set,                // provenance: what it was ALLOWED to do
//!     workspace tree hash,           // only if a tool reads the workspace
//!     app version,
//! )
//! ```
//!
//! NOT in the key: guardrails, schedules, the intent summary, other
//! agents' definitions (their artifacts enter via input hashes).
//!
//! **Freshness (the v0.2 fix).** Some tasks must never be served from
//! the cache, whatever their key: tasks that read the outside world
//! (volatile) and tasks that touch it (delivery). v0.1 cached them, so
//! a daily digest replayed yesterday's stories and never sent the email.
//! [`Freshness`] makes the decision explicit and the engine obeys it.

use wright_core::blueprint::canon;
use wright_core::blueprint::model::AgentDef;
use wright_core::compiler::{ResolvedModel, Task};
use wright_core::hash;
use wright_core::palette;

/// Whether a task may be served from the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// Pure function of its key: cacheable.
    Cacheable,
    /// Reads the outside world: run every firing.
    Volatile,
    /// Touches the world: run every firing, exactly once, never retried.
    Delivery,
}

impl Freshness {
    pub fn of(task: &Task) -> Freshness {
        if task.delivery {
            Freshness::Delivery
        } else if task.volatile {
            Freshness::Volatile
        } else {
            Freshness::Cacheable
        }
    }

    pub fn cacheable(&self) -> bool {
        *self == Freshness::Cacheable
    }
}

/// Everything a key is made of. Build one per task per firing.
pub struct KeyInputs<'a> {
    /// Canonical definition text (agent canon, or system prompt contract).
    pub definition: &'a str,
    pub resolved: Option<&'a ResolvedModel>,
    pub tools: &'a [String],
    pub permission_bits: u32,
    /// Content hashes of declared inputs, in declaration order.
    pub input_hashes: &'a [String],
    /// Hash of the project tree, when a tool reads the workspace.
    pub workspace_hash: Option<&'a str>,
    pub app_version: &'a str,
}

/// Compute the cache key.
pub fn cache_key(k: &KeyInputs<'_>) -> String {
    let tools_text: Vec<String> = k.tools.iter().map(|t| format!("{}@{}", t, palette::version_of(t))).collect();
    let (primary, fallback, tier) = match k.resolved {
        Some(r) => (r.primary.as_str(), r.fallback.as_str(), r.tier.as_str()),
        None => ("", "", "builtin"),
    };
    hash::hash_hex(&[
        b"wright-cache-key-v2",
        k.definition.as_bytes(),
        primary.as_bytes(),
        fallback.as_bytes(),
        tier.as_bytes(),
        tools_text.join(",").as_bytes(),
        k.input_hashes.join(",").as_bytes(),
        // Permission bits ARE the provenance. Changing what an agent may
        // do changes what it may have done.
        &k.permission_bits.to_le_bytes(),
        k.workspace_hash.unwrap_or("").as_bytes(),
        k.app_version.as_bytes(),
    ])
}

/// Convenience: the key of a project agent.
pub fn agent_key(
    agent: &AgentDef,
    task: &Task,
    input_hashes: &[String],
    workspace_hash: Option<&str>,
    app_version: &str,
) -> String {
    let def = canon::agent_canon(agent);
    cache_key(&KeyInputs {
        definition: &def,
        resolved: task.model.as_ref(),
        tools: &task.tools,
        permission_bits: task.permissions.bits(),
        input_hashes,
        workspace_hash: if agent.reads_workspace() { workspace_hash } else { None },
        app_version,
    })
}

/// What the cache remembers about one completed task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheEntry {
    pub key: String,
    /// (artifact name, content hash) for each output, in order.
    pub artifacts: Vec<(String, String)>,
    /// What the task cost, so estimates stay honest.
    pub cost_micro_usd: u64,
    /// The run that produced it (provenance for the receipt).
    pub produced_by_run: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use wright_core::blueprint::parse;
    use wright_core::blueprint::PermissionSet;
    use wright_core::compiler::compile;

    fn setup(extra: &str) -> (AgentDef, Task) {
        let src = format!(
            "meta {{ name \"x\" }}\nintent {{ summary \"s\" confirmed \"v1\" }}\n\
agent Fetcher {{ goal \"g\" {extra} }}\ndeliverable kind=\"k\" {{ check \"c\" }}"
        );
        let bp = parse::blueprint(&src).unwrap();
        let g = compile(&bp).unwrap();
        (bp.agent("Fetcher").unwrap().clone(), g.by_name("Fetcher").unwrap().clone())
    }

    #[test]
    fn same_inputs_same_key() {
        let (a, t) = setup("");
        let k1 = agent_key(&a, &t, &["abc".into()], None, "0.2.0");
        let k2 = agent_key(&a, &t, &["abc".into()], None, "0.2.0");
        assert_eq!(k1, k2);
        assert_eq!(k1.len(), 64, "BLAKE3-256 hex");
    }

    #[test]
    fn goal_change_invalidates() {
        let (mut a, t) = setup("");
        let before = agent_key(&a, &t, &[], None, "0.2.0");
        a.goal = "different goal".into();
        assert_ne!(before, agent_key(&a, &t, &[], None, "0.2.0"));
    }

    #[test]
    fn permission_change_invalidates_provenance() {
        let (a, mut t) = setup("tools \"http-read\" permissions network=true");
        let before = agent_key(&a, &t, &[], None, "0.2.0");
        t.permissions = PermissionSet::none();
        assert_ne!(before, agent_key(&a, &t, &[], None, "0.2.0"), "an artifact made with broader powers is a different artifact");
    }

    #[test]
    fn input_hash_change_invalidates() {
        let (a, t) = setup("");
        assert_ne!(
            agent_key(&a, &t, &["old".into()], None, "0.2.0"),
            agent_key(&a, &t, &["new".into()], None, "0.2.0")
        );
    }

    #[test]
    fn workspace_hash_counts_only_for_workspace_readers() {
        let (a, t) = setup("");
        assert_eq!(agent_key(&a, &t, &[], Some("w1"), "0.2.0"), agent_key(&a, &t, &[], Some("w2"), "0.2.0"));
        let (a, t) = setup("tools \"files-read\" permissions files-read=true");
        assert_ne!(agent_key(&a, &t, &[], Some("w1"), "0.2.0"), agent_key(&a, &t, &[], Some("w2"), "0.2.0"));
    }

    #[test]
    fn outside_readers_and_senders_are_never_cacheable() {
        let (_, t) = setup("tools \"http-read\" permissions network=true");
        assert_eq!(Freshness::of(&t), Freshness::Volatile);
        let (_, t) = setup("tools \"http-post\" permissions network=true");
        assert_eq!(Freshness::of(&t), Freshness::Delivery);
        let (_, t) = setup("");
        assert!(Freshness::of(&t).cacheable());
    }

    #[test]
    fn model_change_invalidates() {
        let (a, mut t) = setup("");
        let before = agent_key(&a, &t, &[], None, "0.2.0");
        t.model.as_mut().unwrap().primary = "another".into();
        assert_ne!(before, agent_key(&a, &t, &[], None, "0.2.0"));
    }
}
