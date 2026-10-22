//! The blueprint data model.
//!
//! Design rule: every field here must be editable from the UI — a card,
//! a chip, a slider, or a toggle. No field without a home on screen;
//! no UI state without a field here. The blueprint round-trips.

use crate::palette::{self, Capability, Effect, Reads};
use crate::schema::Shape;

// ── meta ─────────────────────────────────────────────────────────

/// Project identity. `lineage` records recipe ancestry forever.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    pub name: String,
    pub format: u32,
    pub min_app: String,
    pub created: String,
    pub lineage: String,
}

impl Default for Meta {
    fn default() -> Self {
        Meta {
            name: "Untitled".into(),
            format: 1,
            min_app: "0.1.0".into(),
            created: String::new(),
            lineage: String::new(),
        }
    }
}

/// What the user asked for, in their words, with the Confirm Gate stamp.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Intent {
    pub summary: String,
    /// Last Confirm Gate version that passed, e.g. "v7".
    pub confirmed: String,
    /// Event-spine pointer to the interview thread that produced this.
    pub evidence: String,
}

// ── agents ───────────────────────────────────────────────────────

/// Vendor-proof model routing. Models are commodities; tiers are ours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelTier {
    Cheap,
    Quality,
    Auto,
}

impl ModelTier {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "cheap" => Some(ModelTier::Cheap),
            "quality" => Some(ModelTier::Quality),
            "auto" => Some(ModelTier::Auto),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ModelTier::Cheap => "cheap",
            ModelTier::Quality => "quality",
            ModelTier::Auto => "auto",
        }
    }
}

/// How long an agent remembers. Scoped, never global by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryScope {
    None,
    Firing,
    Project,
}

impl MemoryScope {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "none" => Some(MemoryScope::None),
            "firing" => Some(MemoryScope::Firing),
            "project" => Some(MemoryScope::Project),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryScope::None => "none",
            MemoryScope::Firing => "firing",
            MemoryScope::Project => "project",
        }
    }
}

/// Approval gates. Side-effectful tools pause the Run Theater.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalPolicy {
    None,
    /// Pause and ask before any side-effectful tool call.
    BeforeSend,
}

impl ApprovalPolicy {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "none" => Some(ApprovalPolicy::None),
            "before-send" => Some(ApprovalPolicy::BeforeSend),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ApprovalPolicy::None => "none",
            ApprovalPolicy::BeforeSend => "before-send",
        }
    }
}

/// Per-agent capability grants. Phone-OS permissions, not config files.
///
/// Defaults OFF. The Interview may propose grants; the Confirm Gate
/// shows them; the gate enforces them at every tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PermissionSet(pub u32);

impl PermissionSet {
    pub const READ_FILES: u32 = 1 << 0;
    pub const WRITE_FILES: u32 = 1 << 1;
    pub const NETWORK: u32 = 1 << 2;
    pub const SHELL: u32 = 1 << 3;
    pub const SPAWN: u32 = 1 << 4;

    pub const fn none() -> Self {
        PermissionSet(0)
    }

    pub fn with(&self, bit: u32) -> Self {
        PermissionSet(self.0 | bit)
    }

    pub fn allows(&self, cap: Capability) -> bool {
        let bit = match cap {
            Capability::ReadFiles => Self::READ_FILES,
            Capability::WriteFiles => Self::WRITE_FILES,
            Capability::Network => Self::NETWORK,
            Capability::Shell => Self::SHELL,
            Capability::Spawn => Self::SPAWN,
        };
        self.0 & bit != 0
    }

    pub fn bits(&self) -> u32 {
        self.0
    }

    /// Plain-language names of the granted capabilities, in a fixed order.
    /// Used by the Confirm Gate summary and the UI shield.
    pub fn names(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        for (bit, name) in [
            (Self::READ_FILES, "read files"),
            (Self::WRITE_FILES, "write files"),
            (Self::NETWORK, "use the network"),
            (Self::SHELL, "run shell commands"),
            (Self::SPAWN, "start programs"),
        ] {
            if self.0 & bit != 0 {
                out.push(name);
            }
        }
        out
    }
}

/// One project agent — a card on the canvas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDef {
    /// Stable identifier; also the card title.
    pub name: String,
    /// One sentence. Enforced by the Interviewer, displayed on the card.
    pub goal: String,
    pub model_tier: ModelTier,
    /// Tool names from the palette.
    pub tools: Vec<String>,
    pub memory: MemoryScope,
    /// Input artifact names (the `in` lines).
    pub inputs: Vec<String>,
    /// Output artifact names (the `out` lines).
    pub outputs: Vec<String>,
    pub approval: ApprovalPolicy,
    /// Retries on failure. 0 for side-effectful agents (never double-send).
    pub retries: u8,
    pub permissions: PermissionSet,
    /// Hand this step to an external coding agent (ADR-026): the worker
    /// id as named in config (`worker "claude-code" command=...`).
    /// None for ordinary model-backed agents.
    pub worker: Option<String>,
    /// The files/dirs a worker may change (`worker-files "src/"`).
    /// Empty = the whole sandboxed copy is fair game, and every change
    /// is applied. Narrowed scopes are checked against the diff.
    pub worker_files: Vec<String>,
    /// Wall-clock seconds for the worker run (default 300).
    pub worker_timeout_secs: u64,
    /// The MCP server this agent may talk to (ADR-029), as named in
    /// config (`mcp "weather" command=... hash=...`).
    pub mcp_server: Option<String>,
    /// The server tools this agent may call (`mcp allow="tool1,tool2"`).
    /// Empty = none — the card must name them explicitly.
    pub mcp_allow: Vec<String>,
}

impl Default for AgentDef {
    fn default() -> Self {
        AgentDef {
            name: "Agent".into(),
            goal: String::new(),
            model_tier: ModelTier::Cheap,
            tools: Vec::new(),
            memory: MemoryScope::None,
            inputs: Vec::new(),
            outputs: Vec::new(),
            approval: ApprovalPolicy::None,
            retries: 2,
            permissions: PermissionSet::none(),
            worker: None,
            worker_files: Vec::new(),
            worker_timeout_secs: 300,
            mcp_server: None,
            mcp_allow: Vec::new(),
        }
    }
}

impl AgentDef {
    /// True when this agent's output depends on the outside world (it
    /// uses a tool that reads the network or system state). Volatile
    /// agents re-run on every firing; the cache never serves them.
    pub fn is_volatile(&self) -> bool {
        self.tools
            .iter()
            .filter_map(|t| palette::find(t))
            .any(|t| t.reads == Reads::Outside)
    }

    /// True when this agent touches the world (sends, posts). Delivery
    /// agents run after the checks, are never cached, never retried.
    pub fn is_delivery(&self) -> bool {
        self.tools
            .iter()
            .filter_map(|t| palette::find(t))
            .any(|t| t.effect == Effect::World)
    }

    /// True when any tool reads the project workspace, so the workspace
    /// tree hash must be part of this agent's cache key.
    pub fn reads_workspace(&self) -> bool {
        self.tools
            .iter()
            .filter_map(|t| palette::find(t))
            .any(|t| t.reads == Reads::Workspace)
            || self.worker.is_some()
    }

    /// True when this step runs as an external worker (ADR-026): no
    /// model call — the engine hands the goal to the configured agent
    /// binary in a sandboxed copy and applies its diff whole or not at all.
    pub fn is_worker_step(&self) -> bool {
        self.worker.is_some()
    }
}

/// A dependency edge. Drag one card onto another and this is born.
/// A *typed* edge (ADR-027) also names the output and input artifacts
/// it binds, and the shape those bytes are promised to have.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Edge {
    pub from: String,
    pub to: String,
    /// The producer's output artifact this edge carries.
    pub out: Option<String>,
    /// The consumer's input artifact it lands in.
    pub input: Option<String>,
    /// The promised shape of the hand-off.
    pub schema: Option<Shape>,
}

// ── chips ────────────────────────────────────────────────────────

/// `scheduling` chip parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    pub cron: String,
    pub tz: String,
}

/// A secret is a REFERENCE, never a value. `keychain:` scheme only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRef {
    pub name: String,
    pub reference: String,
}

/// Capability chips. Each ON chip injects system agents at compile time.
/// This is how "project setup" disappears: the user toggles, we inject.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Chips {
    pub scheduling: Option<Schedule>,
    pub secrets: Vec<SecretRef>,
    pub logging: bool,
    pub tests: bool,
    pub ci: bool,
    pub webhooks: bool,
}

// ── project ──────────────────────────────────────────────────────

/// What kind of project gets materialized, and how to test and run it.
///
/// `language` picks the skeleton from the universal project convention
/// (`docs/PROJECT-CONVENTION.md`). The commands are what `run-tests`
/// and the Operator execute; if left empty, the convention's defaults
/// for the language apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSpec {
    /// One of the convention's languages: python, node, react, rust, shell, none.
    pub language: String,
    pub test_cmd: String,
    pub run_cmd: String,
}

impl Default for ProjectSpec {
    fn default() -> Self {
        ProjectSpec {
            language: "none".into(),
            test_cmd: String::new(),
            run_cmd: String::new(),
        }
    }
}

// ── guardrails ───────────────────────────────────────────────────

/// Hard limits, enforced by the compiler and runtime. Compile-time
/// values never wake anyone at 3AM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guardrails {
    /// Micro-USD (1_000_000 == $1.00).
    pub per_firing_micro_usd: u64,
    pub daily_micro_usd: u64,
    /// Wall-clock seconds per firing.
    pub wall_clock_secs: u64,
    pub max_turns: u32,
}

impl Default for Guardrails {
    fn default() -> Self {
        Guardrails {
            per_firing_micro_usd: 1_000_000,
            daily_micro_usd: 5_000_000,
            wall_clock_secs: 600,
            max_turns: 60,
        }
    }
}

// ── deliverable & models ─────────────────────────────────────────

/// The definition of done, in plain language the user confirmed.
/// An `accept` line is an acceptance sentence (ADR-025): the person
/// reads it as a promise, Testwright turns it into a check, and the
/// Reviewer may not approve a run whose sentences failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deliverable {
    pub kind: String,
    pub check: String,
    /// Numbered acceptance sentences, in blueprint order.
    pub accept: Vec<String>,
}

/// One tier's routing preference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TierPref {
    pub prefer: String,
    pub fallback: String,
}

impl Default for TierPref {
    fn default() -> Self {
        TierPref {
            prefer: "deepseek-v4-flash".into(),
            fallback: "glm-5.3-flash".into(),
        }
    }
}

/// The model tier table. Swap vendors by editing strings, not structure.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModelTiers {
    pub cheap: TierPref,
    pub quality: TierPref,
}

impl ModelTiers {
    pub fn tier(&self, tier: ModelTier) -> TierPref {
        match tier {
            ModelTier::Cheap | ModelTier::Auto => self.cheap.clone(),
            ModelTier::Quality => self.quality.clone(),
        }
    }
}

// ── the blueprint ────────────────────────────────────────────────

/// The whole project. One file. The source of truth.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Blueprint {
    pub meta: Meta,
    pub intent: Intent,
    pub agents: Vec<AgentDef>,
    pub edges: Vec<Edge>,
    pub chips: Chips,
    pub guardrails: Guardrails,
    pub deliverables: Vec<Deliverable>,
    pub models: ModelTiers,
    pub project: ProjectSpec,
}

impl Blueprint {
    pub fn agent(&self, name: &str) -> Option<&AgentDef> {
        self.agents.iter().find(|a| a.name == name)
    }

    pub fn agent_mut(&mut self, name: &str) -> Option<&mut AgentDef> {
        self.agents.iter_mut().find(|a| a.name == name)
    }

    /// Names of agents that feed `name` directly.
    pub fn upstream_of(&self, name: &str) -> Vec<String> {
        self.edges
            .iter()
            .filter(|e| e.to == name)
            .map(|e| e.from.clone())
            .collect()
    }

    /// Names of agents that `name` feeds directly.
    pub fn downstream_of(&self, name: &str) -> Vec<String> {
        self.edges
            .iter()
            .filter(|e| e.from == name)
            .map(|e| e.to.clone())
            .collect()
    }

    /// Agents with no outgoing edges — the natural tail of the graph.
    pub fn terminal_agents(&self) -> Vec<String> {
        self.agents
            .iter()
            .filter(|a| !self.edges.iter().any(|e| e.from == a.name))
            .map(|a| a.name.clone())
            .collect()
    }
}
