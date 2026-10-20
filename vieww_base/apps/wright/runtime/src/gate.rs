//! The permission gate: every tool call passes through here.
//!
//! Four questions, in order:
//!   1. Is this tool on the agent's allowlist?         (the blueprint's `tools`)
//!   2. Does the agent hold every capability it needs? (permission bits)
//!   3. Can we afford it?                              (budget meter)
//!   4. Should a human confirm?                        (approval policy + world effects)
//!
//! The answer to an allowed call is a [`Permit`]. A permit can only be
//! made inside this module (its constructor is private), and every tool
//! in `wright-tools` demands one. So "the gate is the only path to a
//! tool" is not a convention — it is a type the compiler checks. In
//! v0.1 tools took no proof, the approval policy was an argument the
//! caller could simply pass as `None`, and the tool allowlist was never
//! consulted. All three holes are closed here and pinned by tests.
//!
//! Budget is ONE meter per firing, shared by every task (behind a mutex
//! in the engine); per-task budgets are soft caps the engine checks
//! before each model call. Denials and asks cost nothing.

use std::collections::HashMap;

use wright_core::blueprint::{ApprovalPolicy, PermissionSet};
use wright_core::compiler::TaskGraph;
use wright_core::palette::{self, Capability, Effect, ToolSpec};

/// A spending meter, in micro-USD. Charging past the cap fails.
#[derive(Debug, Clone)]
pub struct BudgetMeter {
    pub cap_micro_usd: u64,
    pub spent_micro_usd: u64,
}

impl BudgetMeter {
    pub fn new(cap_micro_usd: u64) -> Self {
        BudgetMeter { cap_micro_usd, spent_micro_usd: 0 }
    }

    pub fn can_afford(&self, amount: u64) -> bool {
        self.spent_micro_usd.saturating_add(amount) <= self.cap_micro_usd
    }

    /// Charge if affordable; false means the budget is exhausted.
    pub fn charge(&mut self, amount: u64) -> bool {
        if self.can_afford(amount) {
            self.spent_micro_usd += amount;
            true
        } else {
            false
        }
    }

    /// Record spend that already happened (a model call's actual cost).
    /// This may push the meter past its cap; the next check will deny.
    pub fn record(&mut self, amount: u64) {
        self.spent_micro_usd = self.spent_micro_usd.saturating_add(amount);
    }

    pub fn remaining_micro_usd(&self) -> u64 {
        self.cap_micro_usd.saturating_sub(self.spent_micro_usd)
    }
}

/// Proof that the gate allowed one specific tool call. Unforgeable
/// outside this module: the `_seal` field is private.
#[derive(Debug)]
pub struct Permit {
    agent: String,
    tool: &'static ToolSpec,
    grants: PermissionSet,
    _seal: Seal,
}

#[derive(Debug)]
struct Seal;

impl Permit {
    pub fn agent(&self) -> &str {
        &self.agent
    }

    pub fn tool(&self) -> &'static ToolSpec {
        self.tool
    }

    /// The agent's full grant set (tools like `shell` use it to decide
    /// whether the sandbox may open the network).
    pub fn grants(&self) -> PermissionSet {
        self.grants
    }

    /// Test-only constructor, so tool unit tests can run tools directly.
    /// It exists only under `cfg(test)` or the `test-permits` feature,
    /// which only `wright-tools`' dev-dependencies turn on — the shipped
    /// binary cannot forge a permit.
    #[cfg(any(test, feature = "test-permits"))]
    pub fn for_tests(agent: &str, tool: &'static ToolSpec, grants: PermissionSet) -> Permit {
        Permit { agent: agent.into(), tool, grants, _seal: Seal }
    }
}

/// A call waiting for a human. Resolve it with [`Gate::approve`] or
/// drop it (a denial).
#[derive(Debug)]
pub struct Pending {
    pub agent: String,
    pub tool: &'static ToolSpec,
    pub reason: String,
    est_cost_micro_usd: u64,
    grants: PermissionSet,
}

/// The verdict for one tool call.
#[derive(Debug)]
pub enum Verdict {
    /// Proceed. The estimate has been charged.
    Allow(Permit),
    /// Hard no, with a plain-language reason for the Event River.
    Deny(String),
    /// Pause; a human taps approve or deny.
    Ask(Pending),
}

impl Verdict {
    pub fn is_allow(&self) -> bool {
        matches!(self, Verdict::Allow(_))
    }
}

/// What one task may do.
#[derive(Debug, Clone)]
pub struct Grant {
    pub permissions: PermissionSet,
    pub tools: Vec<String>,
    pub approval: ApprovalPolicy,
}

/// Per-task grants, built from the compiled task graph.
pub struct Gate {
    grants: HashMap<String, Grant>,
}

impl Gate {
    /// Build from the task graph (project AND system agents).
    pub fn from_graph(graph: &TaskGraph) -> Self {
        Gate {
            grants: graph
                .tasks
                .iter()
                .map(|t| {
                    (t.name(), Grant { permissions: t.permissions, tools: t.tools.clone(), approval: t.approval })
                })
                .collect(),
        }
    }

    pub fn grant_of(&self, agent: &str) -> Option<&Grant> {
        self.grants.get(agent)
    }

    /// Decide one tool call. On Allow the estimate is charged.
    pub fn check(&self, agent: &str, tool_name: &str, budget: &mut BudgetMeter, est_cost_micro_usd: u64) -> Verdict {
        // Unknown agents have no grants at all.
        let Some(grant) = self.grants.get(agent) else {
            return Verdict::Deny(format!("'{agent}' is not part of this firing and may not use tools"));
        };
        // 1. The allowlist.
        if !grant.tools.iter().any(|t| t == tool_name) {
            return Verdict::Deny(format!("'{agent}' does not have the '{tool_name}' tool (not in its blueprint card)"));
        }
        let Some(tool) = palette::find(tool_name) else {
            return Verdict::Deny(format!("'{tool_name}' is not a known tool"));
        };
        // 2. Capabilities.
        for cap in tool.caps {
            if !grant.permissions.allows(*cap) {
                return Verdict::Deny(format!(
                    "'{agent}' is not allowed to {} (required by '{}')",
                    cap_name(*cap),
                    tool.name
                ));
            }
        }
        // 3. Budget.
        if !budget.can_afford(est_cost_micro_usd) {
            return Verdict::Deny(format!(
                "budget exhausted: ${:.4} of ${:.2} spent this firing",
                budget.spent_micro_usd as f64 / 1e6,
                budget.cap_micro_usd as f64 / 1e6
            ));
        }
        // 4. Approval: the GATE reads the policy, the caller cannot skip it.
        if tool.effect == Effect::World && grant.approval == ApprovalPolicy::BeforeSend {
            return Verdict::Ask(Pending {
                agent: agent.into(),
                tool,
                reason: format!("'{agent}' wants to use '{}': {}", tool.name, tool.summary),
                est_cost_micro_usd,
                grants: grant.permissions,
            });
        }
        budget.charge(est_cost_micro_usd);
        Verdict::Allow(Permit { agent: agent.into(), tool, grants: grant.permissions, _seal: Seal })
    }

    /// A human approved: re-check the budget (time passed), then permit.
    pub fn approve(&self, pending: Pending, budget: &mut BudgetMeter) -> Result<Permit, String> {
        if !budget.charge(pending.est_cost_micro_usd) {
            return Err("approved, but the budget ran out while waiting".into());
        }
        Ok(Permit { agent: pending.agent, tool: pending.tool, grants: pending.grants, _seal: Seal })
    }
}

fn cap_name(c: Capability) -> &'static str {
    match c {
        Capability::ReadFiles => "read files",
        Capability::WriteFiles => "write files",
        Capability::Network => "use the network",
        Capability::Shell => "run shell commands",
        Capability::Spawn => "start programs",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wright_core::blueprint::parse;
    use wright_core::compiler::compile;

    fn gate() -> Gate {
        let src = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\n\
agent Fetcher { goal \"f\" tools \"http-read\" out \"a.json\" permissions network=true }\n\
agent Summarizer { goal \"s\" in \"a.json\" out \"b.json\" }\n\
agent Mailer { goal \"m\" tools \"http-post\" in \"b.json\" approval \"before-send\" permissions network=true }\n\
edge from=\"Fetcher\" to=\"Summarizer\"\nedge from=\"Summarizer\" to=\"Mailer\"\n\
deliverable kind=\"k\" { check \"c\" }";
        Gate::from_graph(&compile(&parse::blueprint(src).unwrap()).unwrap())
    }

    #[test]
    fn allowed_when_listed_capable_and_funded() {
        let mut budget = BudgetMeter::new(1_000_000);
        let v = gate().check("Fetcher", "http-read", &mut budget, 10_000);
        assert!(v.is_allow());
        assert_eq!(budget.spent_micro_usd, 10_000);
    }

    #[test]
    fn tool_not_on_the_card_is_denied() {
        // Regression: v0.1 never consulted the allowlist, so any agent
        // with network could call ANY network tool.
        let mut budget = BudgetMeter::new(1_000_000);
        match gate().check("Fetcher", "http-post", &mut budget, 1) {
            Verdict::Deny(msg) => assert!(msg.contains("does not have the 'http-post' tool")),
            other => panic!("expected Deny, got {other:?}"),
        }
        assert_eq!(budget.spent_micro_usd, 0, "a denial is free");
    }

    #[test]
    fn approval_policy_is_read_by_the_gate_not_the_caller() {
        // Regression: v0.1 took the policy as an argument; a caller
        // could pass None and skip the human.
        let g = gate();
        let mut budget = BudgetMeter::new(1_000_000);
        let pending = match g.check("Mailer", "http-post", &mut budget, 5_000) {
            Verdict::Ask(p) => p,
            other => panic!("expected Ask, got {other:?}"),
        };
        assert_eq!(budget.spent_micro_usd, 0, "asking is free");
        let permit = g.approve(pending, &mut budget).unwrap();
        assert_eq!(permit.tool().name, "http-post");
        assert_eq!(budget.spent_micro_usd, 5_000);
    }

    #[test]
    fn output_contract_tool_is_always_available_to_producers() {
        let mut budget = BudgetMeter::new(1);
        assert!(gate().check("Summarizer", "write-output", &mut budget, 0).is_allow());
        assert!(!gate().check("Summarizer", "files-write", &mut budget, 0).is_allow());
    }

    #[test]
    fn budget_exhaustion_denies() {
        let mut budget = BudgetMeter::new(100);
        match gate().check("Fetcher", "http-read", &mut budget, 150) {
            Verdict::Deny(msg) => assert!(msg.contains("budget exhausted")),
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[test]
    fn unknown_agent_has_no_grants() {
        let mut budget = BudgetMeter::new(100);
        assert!(!gate().check("Ghost", "files-read", &mut budget, 0).is_allow());
    }

    #[test]
    fn system_agents_get_their_own_grants() {
        let src = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\nagent A { goal \"a\" }\nchips { tests on }\ndeliverable kind=\"k\" { check \"c\" }";
        let g = Gate::from_graph(&compile(&parse::blueprint(src).unwrap()).unwrap());
        let mut budget = BudgetMeter::new(100);
        assert!(g.check("Testwright", "run-tests", &mut budget, 0).is_allow());
        assert!(!g.check("Reviewer", "files-write", &mut budget, 0).is_allow(), "the Reviewer only reads");
    }
}
