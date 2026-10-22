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
use wright_core::palette::{self, Capability, Effect, Leg, Legs, ToolSpec};

/// A spending meter, in micro-USD. Charging past the cap fails.
#[derive(Debug, Clone)]
pub struct BudgetMeter {
    pub cap_micro_usd: u64,
    pub spent_micro_usd: u64,
}

impl BudgetMeter {
    pub fn new(cap_micro_usd: u64) -> Self {
        BudgetMeter {
            cap_micro_usd,
            spent_micro_usd: 0,
        }
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
    /// The task-scoped identity this call is made under, when the firing
    /// issues identities (see `crate::identity`).
    identity: Option<String>,
    /// Secrets this identity may resolve; `None` when identities are off.
    secrets: Option<Vec<String>>,
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

    /// The `did:key` this call is made under, if identities are on.
    pub fn identity(&self) -> Option<&str> {
        self.identity.as_deref()
    }

    /// Whether this call may resolve the secret `name`. With identities
    /// on, only the secrets the agent's identity was delegated.
    pub fn may_use_secret(&self, name: &str) -> bool {
        match &self.secrets {
            None => true,
            Some(list) => list.iter().any(|s| s == name),
        }
    }

    /// Test-only constructor, so tool unit tests can run tools directly.
    /// It exists only under `cfg(test)` or the `test-permits` feature,
    /// which only `wright-tools`' dev-dependencies turn on — the shipped
    /// binary cannot forge a permit.
    #[cfg(any(test, feature = "test-permits"))]
    pub fn for_tests(agent: &str, tool: &'static ToolSpec, grants: PermissionSet) -> Permit {
        Permit {
            agent: agent.into(),
            tool,
            grants,
            identity: None,
            secrets: None,
            _seal: Seal,
        }
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
    /// The Rule of Two legs this task stands on (ADR-024): the gate
    /// reads them, not the caller's conscience.
    pub legs: Legs,
}

/// Per-task grants, built from the compiled task graph.
pub struct Gate {
    /// Task-scoped identities; when set, no agent acts without a live one.
    identities: Option<std::sync::Arc<crate::identity::Registry>>,
    /// Secrets each agent's identity is delegated (agent -> names).
    secret_scope: HashMap<String, Vec<String>>,
    grants: HashMap<String, Grant>,
}

impl Gate {
    /// Build from the task graph (project AND system agents).
    pub fn from_graph(graph: &TaskGraph) -> Self {
        Gate {
            identities: None,
            secret_scope: HashMap::new(),
            grants: graph
                .tasks
                .iter()
                .map(|t| {
                    (
                        t.name(),
                        Grant {
                            permissions: t.permissions,
                            tools: t.tools.clone(),
                            approval: t.approval,
                            legs: t.legs,
                        },
                    )
                })
                .collect(),
        }
    }

    /// Require a live task-scoped identity for every call, and scope
    /// secrets per agent (`scope`: agent -> the secrets it may use).
    pub fn with_identities(mut self, registry: std::sync::Arc<crate::identity::Registry>, scope: HashMap<String, Vec<String>>) -> Self {
        self.identities = Some(registry);
        self.secret_scope = scope;
        self
    }

    /// The registry, when identities are on.
    pub fn identities(&self) -> Option<&std::sync::Arc<crate::identity::Registry>> {
        self.identities.as_ref()
    }

    /// The secrets an agent's identity is delegated.
    pub fn secrets_of(&self, agent: &str) -> Vec<String> {
        self.secret_scope.get(agent).cloned().unwrap_or_default()
    }

    fn identity_for(&self, agent: &str) -> Result<(Option<String>, Option<Vec<String>>), String> {
        match &self.identities {
            None => Ok((None, None)),
            Some(r) => r.check(agent).map(|did| (Some(did), Some(self.secrets_of(agent)))),
        }
    }

    pub fn grant_of(&self, agent: &str) -> Option<&Grant> {
        self.grants.get(agent)
    }

    /// Decide one tool call. On Allow the estimate is charged.
    pub fn check(
        &self,
        agent: &str,
        tool_name: &str,
        budget: &mut BudgetMeter,
        est_cost_micro_usd: u64,
    ) -> Verdict {
        // 0. The kill switch beats everything: while Wright is stopped no
        // tool runs — so a firing in flight cannot send or fetch more.
        if let Some(why) = crate::killswitch::refusal() {
            return Verdict::Deny(why);
        }
        // 0b. Identity: with identities on, an agent acts only under its
        // own live, task-scoped identity — none before its task starts,
        // none after it ends or the clock runs out.
        let (identity, secrets) = match self.identity_for(agent) {
            Ok(x) => x,
            Err(why) => return Verdict::Deny(why),
        };
        // Unknown agents have no grants at all.
        let Some(grant) = self.grants.get(agent) else {
            return Verdict::Deny(format!(
                "'{agent}' is not part of this firing and may not use tools"
            ));
        };
        // 1. The allowlist.
        if !grant.tools.iter().any(|t| t == tool_name) {
            return Verdict::Deny(format!(
                "'{agent}' does not have the '{tool_name}' tool (not in its blueprint card)"
            ));
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
        // 4. The Rule of Two (ADR-024): an agent holding all three legs
        // asks before EVERY outbound step, even when the card already
        // promised `approval "before-send"` — the promise names the
        // combination; the ask names the actual send. Defense in depth:
        // the validator should have refused an unapproved 3-leg agent
        // before the firing ever started.
        if grant.legs.is_three_legged() && tool.legs.has(Leg::OutboundChannel) {
            return Verdict::Ask(Pending {
                agent: agent.into(),
                tool,
                reason: format!(
                    "'{agent}' holds private data, untrusted input and an outbound channel at once — the Rule of Two. It wants to use '{}': {}",
                    tool.name, tool.summary
                ),
                est_cost_micro_usd,
                grants: grant.permissions,
            });
        }
        // 5. Approval: the GATE reads the policy, the caller cannot skip it.
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
        Verdict::Allow(Permit {
            agent: agent.into(),
            tool,
            grants: grant.permissions,
            identity,
            secrets,
            _seal: Seal,
        })
    }

    /// A human approved: re-check the budget (time passed), then permit.
    pub fn approve(&self, pending: Pending, budget: &mut BudgetMeter) -> Result<Permit, String> {
        // Approved while Wright was being stopped: the stop wins.
        if let Some(why) = crate::killswitch::refusal() {
            return Err(why);
        }
        // The identity must still be live when the human answers.
        let (identity, secrets) = self.identity_for(&pending.agent)?;
        if !budget.charge(pending.est_cost_micro_usd) {
            return Err("approved, but the budget ran out while waiting".into());
        }
        Ok(Permit {
            agent: pending.agent,
            tool: pending.tool,
            grants: pending.grants,
            identity,
            secrets,
            _seal: Seal,
        })
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
        assert!(gate()
            .check("Summarizer", "write-output", &mut budget, 0)
            .is_allow());
        assert!(!gate()
            .check("Summarizer", "files-write", &mut budget, 0)
            .is_allow());
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
        assert!(!gate()
            .check("Ghost", "files-read", &mut budget, 0)
            .is_allow());
    }

    #[test]
    fn system_agents_get_their_own_grants() {
        let src = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\nagent A { goal \"a\" }\nchips { tests on }\ndeliverable kind=\"k\" { check \"c\" }";
        let g = Gate::from_graph(&compile(&parse::blueprint(src).unwrap()).unwrap());
        let mut budget = BudgetMeter::new(100);
        assert!(g
            .check("Testwright", "run-tests", &mut budget, 0)
            .is_allow());
        assert!(
            !g.check("Reviewer", "files-write", &mut budget, 0)
                .is_allow(),
            "the Reviewer only reads"
        );
    }

    #[test]
    fn the_rule_of_two_asks_on_every_outbound_step_of_a_three_leg_agent() {
        // One agent that reads files, reads the web and can POST: three
        // legs. Even with NO approval policy on the card, the gate asks
        // before the send — the validator refuses such a blueprint, so
        // this is the runtime's belt-and-braces (ADR-024).
        let src = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\n\
agent Greedy { goal \"g\" tools \"files-read\" tools \"http-read\" tools \"http-post\" out \"o.json\" permissions network=true files-read=true }\n\
deliverable kind=\"k\" { check \"c\" }";
        let g = Gate::from_graph(&compile(&parse::blueprint(src).unwrap()).unwrap());
        let mut budget = BudgetMeter::new(1_000_000);
        // reading is allowed without a stop...
        assert!(g.check("Greedy", "files-read", &mut budget, 0).is_allow());
        assert!(g.check("Greedy", "http-read", &mut budget, 0).is_allow());
        // ...but every outbound step asks, naming the rule.
        match g.check("Greedy", "http-post", &mut budget, 5_000) {
            Verdict::Ask(p) => assert!(p.reason.contains("Rule of Two"), "{}", p.reason),
            other => panic!("expected the Rule of Two ask, got {other:?}"),
        }
        assert_eq!(budget.spent_micro_usd, 0, "asking is free");
        // A second outbound step asks again — every step, not the first.
        match g.check("Greedy", "http-post", &mut budget, 5_000) {
            Verdict::Ask(p) => assert!(p.reason.contains("Rule of Two")),
            other => panic!("the ask must repeat, got {other:?}"),
        }
    }

    #[test]
    fn two_leg_agents_do_not_hit_the_rule_ask() {
        // The classic split (fetch, then mail) never trips the Rule of
        // Two: the Mailer has outbound + untrusted but no private data.
        let src = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\n\
agent Fetcher { goal \"f\" tools \"http-read\" out \"stories.json\" permissions network=true }\n\
agent Mailer { goal \"m\" tools \"http-post\" in \"stories.json\" approval \"before-send\" permissions network=true }\n\
edge from=\"Fetcher\" to=\"Mailer\"\n\
deliverable kind=\"email\" { check \"c\" }";
        let g = Gate::from_graph(&compile(&parse::blueprint(src).unwrap()).unwrap());
        let mut budget = BudgetMeter::new(1_000_000);
        match g.check("Mailer", "http-post", &mut budget, 5_000) {
            Verdict::Ask(p) => assert!(
                p.reason.contains("wants to use"),
                "the ordinary approval ask: {}",
                p.reason
            ),
            other => panic!("expected the ordinary before-send ask, got {other:?}"),
        }
    }
}
