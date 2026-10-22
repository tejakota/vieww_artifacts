//! The typed tool vocabulary.
//!
//! This is the *language* of tools shared by blueprints, the permission
//! gate, the task cache and the model (tool schemas). Tool
//! *implementations* live in the `wright-tools` crate; this module only
//! carries names, the capabilities each tool demands, what each tool
//! reads and touches, a version that feeds cache keys, and the JSON
//! schema the model sees.
//!
//! Two classifications drive correctness, so read them carefully:
//!
//! * [`Reads`] — where a tool's *answer* comes from. A tool that reads
//!   the outside world (`http-read`, `shell`) makes its agent's output
//!   depend on something the blueprint does not pin, so that agent is
//!   **volatile**: it re-runs on every firing and is never served from
//!   the cache. A tool that reads the project workspace makes the
//!   workspace tree hash part of the cache key.
//! * [`Effect`] — what a tool *changes*. `World` effects (sending,
//!   posting) are the ones approval policies pause on, the ones that
//!   are never retried, and the ones the compiler schedules **after**
//!   the checks (tests + review) instead of before them.
//!
//! Extending the palette: add a `ToolSpec` here, implement it in
//! `wright-tools`, and add tests on both sides.

// ── the Rule of Two legs (ADR-024) ───────────────────────────────

/// One leg of the Rule of Two: what a tool puts an agent in reach of.
/// An agent holding all three at once can be steered by text it did
/// not ask for into shipping text it should not share — the validator
/// refuses that combination without a human approval on the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leg {
    /// It can read the person's own data: project files, git history,
    /// anything in the sandbox that is not public.
    PrivateData,
    /// Its output depends on the outside world: web pages, system state,
    /// another program's answer (an MCP server, a worker agent).
    UntrustedInput,
    /// It can send something out: POST, a browser session, a world effect.
    OutboundChannel,
}

/// A set of legs — a tiny bitset with named queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Legs(pub u8);

impl Legs {
    pub const PRIVATE_DATA: u8 = 1 << 0;
    pub const UNTRUSTED_INPUT: u8 = 1 << 1;
    pub const OUTBOUND: u8 = 1 << 2;

    pub const fn none() -> Legs {
        Legs(0)
    }

    /// All three — the combination the Rule of Two names.
    pub const fn all() -> Legs {
        Legs(0b111)
    }

    pub const fn private_data() -> Legs {
        Legs(Legs::PRIVATE_DATA)
    }

    /// The untrusted mark, as taint inheritance adds it.
    pub const fn untrusted() -> Legs {
        Legs(Legs::UNTRUSTED_INPUT)
    }

    pub const fn outbound() -> Legs {
        Legs(Legs::OUTBOUND)
    }

    pub const fn with(self, leg: Leg) -> Legs {
        Legs(self.0 | Legs::bit_of(leg))
    }

    pub const fn has(self, leg: Leg) -> bool {
        self.0 & Legs::bit_of(leg) != 0
    }

    pub fn is_none(self) -> bool {
        self.0 == 0
    }

    pub fn is_untrusted(self) -> bool {
        self.0 & Legs::UNTRUSTED_INPUT != 0
    }

    pub fn is_three_legged(self) -> bool {
        self.0 & Legs::all().0 == Legs::all().0
    }

    pub const fn union(self, other: Legs) -> Legs {
        Legs(self.0 | other.0)
    }

    const fn bit_of(leg: Leg) -> u8 {
        match leg {
            Leg::PrivateData => Legs::PRIVATE_DATA,
            Leg::UntrustedInput => Legs::UNTRUSTED_INPUT,
            Leg::OutboundChannel => Legs::OUTBOUND,
        }
    }
}

impl std::ops::BitOr for Legs {
    type Output = Legs;
    fn bitor(self, other: Legs) -> Legs {
        self.union(other)
    }
}

impl std::ops::BitOrAssign for Legs {
    fn bitor_assign(&mut self, other: Legs) {
        self.0 |= other.0;
    }
}

/// What a tool needs the agent to be allowed to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    ReadFiles,
    WriteFiles,
    Network,
    Shell,
    Spawn,
}

/// Where a tool's result comes from (drives cache correctness).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reads {
    /// Only its own arguments. Pure.
    Nothing,
    /// The project workspace. The workspace tree hash joins the cache key.
    Workspace,
    /// The outside world (network, system state). The agent is volatile.
    Outside,
}

/// What a tool changes (drives approvals, retries and ordering).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    None,
    /// Files inside the project workspace. Reversible (git).
    Workspace,
    /// Something outside the machine: a message sent, a POST made.
    World,
}

/// One entry in the tool palette.
#[derive(Debug)]
pub struct ToolSpec {
    /// Blueprint-facing name, e.g. `http-read`.
    pub name: &'static str,
    /// Capabilities the agent must be granted for this tool to pass the gate.
    pub caps: &'static [Capability],
    /// Version feeding cache keys. Bump when behavior changes.
    pub version: u64,
    pub reads: Reads,
    pub effect: Effect,
    /// The Rule of Two legs this tool puts the agent on (ADR-024).
    pub legs: Legs,
    /// One line for Inspector Mode tooltips and the model's tool list.
    pub summary: &'static str,
    /// JSON schema of the tool's input object, as the model sees it.
    pub schema: &'static str,
    /// False for tools that exist in the vocabulary but refuse to run
    /// yet. They are shown honestly in `wright doctor`.
    pub implemented: bool,
}

impl ToolSpec {
    /// True when the tool touches the world outside the machine.
    pub fn side_effect(&self) -> bool {
        self.effect == Effect::World
    }
}

/// The whole palette. Order is the Inspector Mode display order.
pub const PALETTE: &[ToolSpec] = &[
    // The output contract. Every agent with `out` lines gets this tool
    // automatically; it can only write the artifact names the blueprint
    // declared, into the project's `artifacts/` folder. No capability is
    // needed because the blueprint itself is the grant.
    ToolSpec {
        name: "write-output",
        caps: &[],
        version: 1,
        reads: Reads::Nothing,
        effect: Effect::Workspace,
        legs: Legs::none(),
        summary: "Write one of this agent's declared output artifacts (by name).",
        schema: r#"{"type":"object","properties":{"name":{"type":"string","description":"One of your declared outputs"},"content":{"type":"string"}},"required":["name","content"]}"#,
        implemented: true,
    },
    ToolSpec {
        name: "files-read",
        caps: &[Capability::ReadFiles],
        version: 2,
        reads: Reads::Workspace,
        effect: Effect::None,
        legs: Legs::private_data(),
        summary: "Read a text file inside the project (max 64 KiB per call; use offset to page).",
        schema: r#"{"type":"object","properties":{"path":{"type":"string","description":"Project-relative path"},"offset":{"type":"integer","description":"Byte offset to start from (default 0)"}},"required":["path"]}"#,
        implemented: true,
    },
    ToolSpec {
        name: "files-list",
        caps: &[Capability::ReadFiles],
        version: 1,
        reads: Reads::Workspace,
        effect: Effect::None,
        legs: Legs::private_data(),
        summary: "List files under a project directory (recursive, max 500 entries).",
        schema: r#"{"type":"object","properties":{"path":{"type":"string","description":"Project-relative directory, default ."}}}"#,
        implemented: true,
    },
    ToolSpec {
        name: "files-write",
        caps: &[Capability::WriteFiles],
        version: 2,
        reads: Reads::Nothing,
        effect: Effect::Workspace,
        legs: Legs::private_data(),
        summary: "Create or overwrite a text file inside the project.",
        schema: r#"{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}"#,
        implemented: true,
    },
    ToolSpec {
        name: "edit",
        caps: &[Capability::ReadFiles, Capability::WriteFiles],
        version: 1,
        reads: Reads::Workspace,
        effect: Effect::Workspace,
        legs: Legs::private_data(),
        summary: "Replace one exact, unique snippet of text in a project file.",
        schema: r#"{"type":"object","properties":{"path":{"type":"string"},"old":{"type":"string","description":"Exact text that occurs exactly once"},"new":{"type":"string"}},"required":["path","old","new"]}"#,
        implemented: true,
    },
    ToolSpec {
        name: "apply-patch",
        caps: &[Capability::ReadFiles, Capability::WriteFiles],
        version: 1,
        reads: Reads::Workspace,
        effect: Effect::Workspace,
        legs: Legs::private_data(),
        summary: "Apply a unified diff (---/+++/@@ hunks) to project files. All hunks apply or none do.",
        schema: r#"{"type":"object","properties":{"patch":{"type":"string"}},"required":["patch"]}"#,
        implemented: true,
    },
    ToolSpec {
        name: "git",
        caps: &[Capability::ReadFiles, Capability::WriteFiles, Capability::Spawn],
        version: 1,
        reads: Reads::Workspace,
        effect: Effect::Workspace,
        legs: Legs::private_data(),
        summary: "Run one git subcommand in the project: status, diff, log, add, commit.",
        schema: r#"{"type":"object","properties":{"args":{"type":"array","items":{"type":"string"},"description":"e.g. [\"diff\",\"--stat\"]; first item must be status|diff|log|add|commit"}},"required":["args"]}"#,
        implemented: true,
    },
    ToolSpec {
        name: "run-tests",
        caps: &[Capability::ReadFiles, Capability::Spawn],
        version: 1,
        reads: Reads::Workspace,
        effect: Effect::Workspace,
        legs: Legs::private_data(),
        summary: "Run the project's test command (from the blueprint) in the sandbox and return the output.",
        schema: r#"{"type":"object","properties":{}}"#,
        implemented: true,
    },
    ToolSpec {
        name: "shell",
        caps: &[Capability::Shell, Capability::Spawn],
        version: 2,
        reads: Reads::Outside,
        effect: Effect::Workspace,
        legs: Legs::private_data().with(Leg::UntrustedInput),
        summary: "Run a shell command in the sandbox: writes only inside the project, network only if granted.",
        schema: r#"{"type":"object","properties":{"command":{"type":"string"},"timeout_secs":{"type":"integer"}},"required":["command"]}"#,
        implemented: true,
    },
    ToolSpec {
        name: "http-read",
        caps: &[Capability::Network],
        version: 2,
        reads: Reads::Outside,
        effect: Effect::None,
        legs: Legs::untrusted(),
        summary: "GET a URL and return the body (max 1 MiB, text only).",
        schema: r#"{"type":"object","properties":{"url":{"type":"string"}},"required":["url"]}"#,
        implemented: true,
    },
    ToolSpec {
        name: "http-post",
        caps: &[Capability::Network],
        version: 2,
        reads: Reads::Outside,
        effect: Effect::World,
        legs: Legs::outbound(),
        summary: "POST a payload to a URL. Touches the world: approval-eligible, never retried.",
        schema: r#"{"type":"object","properties":{"url":{"type":"string"},"body":{"type":"string"},"content_type":{"type":"string","description":"default application/json"},"auth_secret":{"type":"string","description":"Name of a blueprint secret to send as a Bearer token"}},"required":["url","body"]}"#,
        implemented: true,
    },
    ToolSpec {
        name: "browser",
        caps: &[Capability::Network],
        version: 1,
        reads: Reads::Outside,
        effect: Effect::World,
        legs: Legs::untrusted().with(Leg::OutboundChannel),
        summary: "Headless browser session. Not implemented yet: refuses loudly.",
        schema: r#"{"type":"object","properties":{"url":{"type":"string"}},"required":["url"]}"#,
        implemented: false,
    },
    ToolSpec {
        name: "worker",
        caps: &[Capability::ReadFiles, Capability::WriteFiles, Capability::Spawn, Capability::Network],
        // v2: a worker talks to its own vendor, so it is an outbound
        // channel (ADR-026, revised): the version is part of the cache key.
        version: 2,
        reads: Reads::Workspace,
        effect: Effect::Workspace,
        legs: Legs::private_data().with(Leg::UntrustedInput).with(Leg::OutboundChannel),
        summary: "Hand this step to an external coding agent (claude-code, codex, kimi-code, zcode) in a sandboxed copy; it talks to its own vendor, so it asks you first, and its diff is applied whole or not at all.",
        schema: r#"{"type":"object","properties":{"prompt":{"type":"string","description":"The full instruction for the worker agent, including which files it may change"}},"required":["prompt"]}"#,
        implemented: true,
    },
    ToolSpec {
        name: "mcp-call",
        caps: &[Capability::Spawn, Capability::Network],
        version: 1,
        reads: Reads::Outside,
        effect: Effect::World,
        legs: Legs::untrusted().with(Leg::OutboundChannel),
        summary: "Call one tool on a hash-pinned MCP server named in the card; only the card's allow-listed tools exist.",
        schema: r#"{"type":"object","properties":{"server":{"type":"string","description":"The MCP server named on your card"},"tool":{"type":"string","description":"One of your allowed tools on that server"},"args":{"type":"object","description":"The tool's arguments object"}},"required":["server","tool","args"]}"#,
        implemented: true,
    },
];

/// Look up a tool by blueprint-facing name.
pub fn find(name: &str) -> Option<&'static ToolSpec> {
    PALETTE.iter().find(|t| t.name == name)
}

/// Version of a named tool, or 0 if unknown (validation rejects
/// unknown names before this ever matters — defense in depth).
pub fn version_of(name: &str) -> u64 {
    find(name).map(|t| t.version).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::json;

    #[test]
    fn palette_lookup_works() {
        assert!(find("http-read").is_some());
        assert!(find("no-such-tool").is_none());
        assert!(find("http-post").unwrap().side_effect());
        assert!(!find("http-read").unwrap().side_effect());
    }

    #[test]
    fn names_are_unique() {
        let names: Vec<&str> = PALETTE.iter().map(|t| t.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names.len(), sorted.len(), "duplicate tool name in palette");
    }

    #[test]
    fn every_schema_is_valid_json_object() {
        for t in PALETTE {
            let v = json::parse(t.schema).unwrap_or_else(|e| panic!("{}: {e}", t.name));
            assert_eq!(
                v.get("type").and_then(|t| t.as_str()),
                Some("object"),
                "{}",
                t.name
            );
        }
    }

    #[test]
    fn outside_readers_are_classified() {
        assert_eq!(find("http-read").unwrap().reads, Reads::Outside);
        assert_eq!(find("files-read").unwrap().reads, Reads::Workspace);
        assert_eq!(find("files-write").unwrap().effect, Effect::Workspace);
    }
}
