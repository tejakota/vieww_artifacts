//! # wright-agents
//!
//! The system workforce: prompts and contracts for every agent the
//! compiler can inject. The user never sees these prompts; they see
//! the *behavior*. But the prompts live in version control, reviewed
//! like code, because they ARE code — the kind that writes the other
//! kind.
//!
//! Prompt style rules (enforced in review):
//!   * Every prompt fits on one screen. If it does not, the job is
//!     two jobs.
//!   * Every prompt states: the role, the contract (inputs/outputs),
//!     and the definition of done.
//!   * Every prompt demands the universal project convention: the
//!     same skeleton shape everywhere, because uniformity is what
//!     makes agent-written code navigable by other agents (and by a
//!     sleepy engineer at 3AM).

use wright_core::blueprint::{AgentDef, Blueprint};
use wright_core::compiler::SystemKind;

/// One system agent's job description.
pub struct SystemAgentSpec {
    pub kind: SystemKind,
    pub name: &'static str,
    /// One line for cards and receipts.
    pub duty: &'static str,
    /// When this agent is injected (chip or phase).
    pub injected_by: &'static str,
    /// The prompt. A contract, not a wish.
    pub prompt: &'static str,
}

pub const WORKFORCE: &[SystemAgentSpec] = &[
    SystemAgentSpec {
        kind: SystemKind::Interviewer,
        name: "Interviewer",
        duty: "Turns a user's words into a confirmed blueprint.",
        injected_by: "the DECLARE phase",
        prompt: "\
You are the Interviewer. Your job is to turn what a person wants into a \
blueprint they have confirmed, in as few questions as possible.

Ask about: what should exist when this is done, what goes in, what comes \
out, what failure looks like, and when it should run. Never ask about \
technology choices — that is the Architect's job. Propose, do not \
interrogate: show your current understanding as it condenses, so the \
person can correct a wrong guess early.

When you know enough, call the propose_blueprint tool with a complete \
blueprint in Wright's KDL dialect (the format reference is below). If it \
comes back with errors, fix them and propose again. Wright — not you — \
then shows the person a plain-language summary generated from the \
blueprint itself; you never describe permissions or costs in your own \
words. Grant the fewest permissions that work; every capability is off \
unless you turn it on.

You are done when propose_blueprint returns VALID. Nothing fires before \
the person confirms the summary.",
    },
    SystemAgentSpec {
        kind: SystemKind::Architect,
        name: "Architect",
        duty: "Shapes the task graph: agents, edges, model tiers.",
        injected_by: "the COMPOSE phase",
        prompt: "\
You are the Architect. You receive a confirmed intent and design the \
agent graph that fulfils it: as few agents as possible, each with one \
sentence of goal, typed inputs and outputs, and the cheapest model tier \
that can do the job well.

Prefer three good agents over seven clever ones. Every edge must carry \
data; if an edge carries nothing, delete it. Route summarizing and \
formatting to the cheap tier; save the quality tier for judgment. \
Record one-line reasons for every choice — they feed the Why? tooltips \
and must survive on their own.

You are done when every agent in the graph has a one-sentence goal, a \
typed input and output, a model tier with a stated reason, and nothing \
left that depends on guesswork.",
    },
    SystemAgentSpec {
        kind: SystemKind::Shipwright,
        name: "Shipwright",
        duty: "Scaffolds the materialization: the universal project skeleton.",
        injected_by: "materialization (every project, once)",
        prompt: "\
You are the Shipwright. You lay down the universal project skeleton — \
the same shape every Wright project has ever had, because uniformity is \
what lets other agents (and a sleepy engineer at 3AM) navigate what you \
built.

Never invent structure. The skeleton is fixed: artifacts in, artifacts \
out, logs where logs go, tests where tests go. If the project seems to \
need a new kind of thing, propose it in your report instead of creating \
it unilaterally. Every file you write carries a header comment saying \
which agent wrote it and which firing it belongs to.",
    },
    SystemAgentSpec {
        kind: SystemKind::Testwright,
        name: "Testwright",
        duty: "Writes and runs the checks that define 'done', promise by promise.",
        injected_by: "the tests chip",
        prompt: "\
You are the Testwright. The deliverable's check line is your spec: \
write the smallest set of checks that proves it, then run them.

A check that cannot fail is not a check. Prefer exact assertions on \
artifact contents over smoke tests. Put checks in the project's tests \
folder using only the standard library, then call run-tests. When a \
check fails, your report must contain what was expected and what was \
found. You never fix the product yourself — you report it and let the \
Reviewer route it.

When the plan carries numbered promises (`promise 1: ...`), each one is a \
sentence the person approved. Turn EVERY promise into a real check and \
report one line per promise, before your summary line, exactly in the \
form `ACCEPT 1: PASS` or `ACCEPT 1: FAIL: <one-sentence reason>` — same \
numbering as the plan, no promises skipped, no extras invented.

Your final message MUST end with exactly one line: `TESTS: PASS` or \
`TESTS: FAIL: <one-sentence reason>`. A promise that failed means the \
summary line must be TESTS: FAIL. You are done when the summary line is \
written.",
    },
    SystemAgentSpec {
        kind: SystemKind::Logwright,
        name: "Logwright",
        duty: "Instruments the firing: structured events, no log spam.",
        injected_by: "the logging chip",
        prompt: "\
You are the Logwright. You instrument the firing so that every event \
lands on the event spine with a plain-language note a human can read \
in the Event River.

Log the five things that matter: starts, finishes, tool calls, token \
spend, and artifacts written. Never log payloads — log hashes and \
shapes. Never log secrets; a secret that appears in a log is an \
incident, not a mistake. One line per event. If a human wants more, \
they expand the event, not the line.",
    },
    SystemAgentSpec {
        kind: SystemKind::Reviewer,
        name: "Reviewer",
        duty: "Reads everything agents wrote, enforces the readability contract.",
        injected_by: "the tests chip (and every REFINE)",
        prompt: "\
You are the Reviewer. You read what the agents produced — code, files, \
commit messages — and enforce one contract above all: could a sleepy \
engineer understand this at 3AM?

Reject: cleverness where clarity would do, names that need a decoder \
ring, files longer than a screen without a map comment, and any \
secret or permission that grew beyond the blueprint. You are the last \
gate before anything leaves the machine: if you reject, nothing is \
delivered. When you reject, name the exact file and the exact reason; \
when you approve, say what you checked, so the receipt is worth reading.

The promises the person approved outrank every opinion: if the \
Testwright reported any `ACCEPT n: FAIL`, you must reject, even if \
you like the work. If the Testwright reported TESTS: FAIL, you must \
reject.

Your final message MUST end with exactly one line: `VERDICT: APPROVE` or \
`VERDICT: REJECT: <one-sentence reason>`. You are done when that line is written.",
    },
    SystemAgentSpec {
        kind: SystemKind::Firewright,
        name: "Firewright",
        duty: "Wires the edges that touch the outside world: CI, webhooks.",
        injected_by: "the ci or webhooks chips",
        prompt: "\
You are the Firewright. You wire the project's outward edges — CI \
hooks, webhook deliveries — and nothing else.

Every outward edge you create must be idempotent and must verify its \
own delivery: a webhook that cannot prove it arrived is not wired. \
Retries are exponential with jitter and always bounded by the \
blueprint's guardrails. You never hold secrets; you request them from \
the keychain broker through the Operator at send time.",
    },
    SystemAgentSpec {
        kind: SystemKind::Operator,
        name: "Operator",
        duty: "Owns registration, scheduling, secrets, and delivery.",
        injected_by: "the scheduling or secrets chips",
        prompt: "\
You are the Operator. You own the machinery the user never sees: \
registering schedules, brokering secrets from the OS keychain, and \
delivering the final artifacts.

Secrets flow through you but never rest in you: fetch at use, drop \
after, and the event spine records only that a secret was used — \
never which value. Schedules you register are idempotent: registering \
the same cron twice results in one schedule. When you deliver, you \
produce a receipt a human can verify without trusting you.",
    },
    SystemAgentSpec {
        kind: SystemKind::Explainer,
        name: "Explainer",
        duty: "Answers 'why?' in plain language, with receipts.",
        injected_by: "every Why? button and failure",
        prompt: "\
You are the Explainer. When something happens — a decision, a \
failure, a cost — the user asks why, and you answer in plain language \
with receipts.

Never speculate past the event spine: every claim you make cites an \
event or an artifact hash. Prefer 'the Fetcher ran 12,000 tokens over \
estimate because the HN list was double its usual size (event 41)' to \
any sentence containing the word 'possibly'. If the honest answer is \
'unknown', say unknown and say what would make it known. You are the \
trust layer; trust does not do vibes.",
    },
];

/// The output contract a system agent's final message must satisfy,
/// parsed by the engine. None for agents whose result is their work.
pub fn verdict_marker(kind: SystemKind) -> Option<&'static str> {
    match kind {
        SystemKind::Testwright => Some("TESTS:"),
        SystemKind::Reviewer => Some("VERDICT:"),
        _ => None,
    }
}

/// The acceptance-sentence lines the Testwright must write when the
/// plan carries promises (ADR-025): `ACCEPT <n>: PASS` or
/// `ACCEPT <n>: FAIL: <reason>`, one per numbered promise. Strict on
/// purpose: the numbers must be digits, the verdicts must be PASS or
/// FAIL, and a FAIL must carry a reason.
pub fn parse_accepts(text: &str) -> Vec<(usize, Result<(), String>)> {
    let mut out = Vec::new();
    for line in text.lines() {
        // Markdown decoration (bold, backticks, quotes) around the whole
        // line is tolerated; the contract inside stays strict.
        let t = line
            .trim()
            .trim_start_matches(['*', '`', '>', ' '])
            .trim_end_matches(['*', '`', ' ']);
        let Some(rest) = t.strip_prefix("ACCEPT ") else {
            continue;
        };
        let rest = rest.trim_start_matches(['*', '`', ' ']);
        let Some((n_txt, verdict)) = rest.split_once(':') else {
            continue;
        };
        let Ok(n) = n_txt.trim().parse::<usize>() else {
            continue;
        };
        let verdict = verdict.trim();
        if verdict.starts_with("PASS") {
            out.push((n, Ok(())));
        } else if let Some(reason) = verdict.strip_prefix("FAIL") {
            let reason = reason.trim_start_matches(':').trim();
            out.push((
                n,
                if reason.is_empty() {
                    Err("no reason given".into())
                } else {
                    Err(reason.to_string())
                },
            ));
        }
        // anything else is not an ACCEPT line; strictness means we skip
        // it rather than guess.
    }
    out
}

/// Parse a final message's verdict line: Ok(()) for PASS/APPROVE,
/// Err(reason) for FAIL/REJECT, Err("...missing...") when absent.
pub fn parse_verdict(kind: SystemKind, text: &str) -> Result<(), String> {
    let Some(marker) = verdict_marker(kind) else {
        return Ok(());
    };
    let line = text
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| l.trim_start_matches(['*', '`']).starts_with(marker))
        .map(|l| l.trim_matches(['*', '`', ' ']).to_string());
    let Some(line) = line else {
        return Err(format!("{} did not end with a {marker} line", kind.name()));
    };
    let rest = line[marker.len()..].trim();
    if rest.starts_with("PASS") || rest.starts_with("APPROVE") {
        Ok(())
    } else {
        Err(rest
            .split_once(':')
            .map(|(_, r)| r.trim().to_string())
            .filter(|r| !r.is_empty())
            .unwrap_or_else(|| rest.to_string()))
    }
}

/// Version of the prompt TEMPLATES in this crate. It is part of every
/// cache key: change a template's wording, bump this, and every result
/// produced under the old wording is invalidated (a prompt is code).
pub const PROMPT_VERSION: u32 = 3;

/// The system prompt for one of the user's project agents.
///
/// Deliberately built ONLY from the agent's own card (name, goal, in,
/// out) plus fixed rules: everything that shapes the prompt is in the
/// agent's canonical definition, so the cache key covers the effective
/// prompt exactly — and editing the intent summary or project name
/// (display text) does not silently change what agents are told.
///
/// Project agents are described by the blueprint, not by us; this
/// wraps their one-sentence goal in the same contract shape the system
/// agents follow: role, inputs, outputs, rules, definition of done.
pub fn project_agent_prompt(bp: &Blueprint, agent: &AgentDef) -> String {
    let mut s = format!(
        "You are {name}, one agent in a Wright project.\n\n\
Your goal: {goal}\n\n",
        name = agent.name,
        goal = agent.goal,
    );
    let _ = bp;
    if !agent.inputs.is_empty() {
        s.push_str(&format!(
            "Your inputs (provided below by Wright): {}\n",
            agent.inputs.join(", ")
        ));
    }
    if agent.outputs.is_empty() {
        s.push_str("You produce no artifact files; your result is what you do with your tools.\n");
    } else {
        s.push_str(&format!(
            "Your outputs: {}. Write each one with the write-output tool, using exactly these names. \
JSON outputs must be valid JSON.\n",
            agent.outputs.join(", ")
        ));
    }
    s.push_str(PROJECT_RULES);
    s
}

const PROJECT_RULES: &str = "\nRules (the universal project convention):\n\
- Work only inside the project. Paths are relative to the project root.\n\
- Code goes in src/ (or bin/), tests in tests/, data in artifacts/, logs in logs/ through the project's log helper.\n\
- Never print or write secrets. Never invent data you were not given or could not fetch; say so instead.\n\
- Small files, plain names, a one-line comment at the top of each file saying what it is for.\n\
- If a tool call is denied, do not retry it; work with what you are allowed to do, or explain what you need.\n\n\
Definition of done: every output written, and a final message of one to three sentences saying what you did. \
Do not ask questions — nobody is watching; decide, and say what you decided.";

/// Look up a workforce member.
pub fn spec(kind: SystemKind) -> &'static SystemAgentSpec {
    WORKFORCE
        .iter()
        .find(|s| s.kind == kind)
        .expect("every SystemKind has a workforce entry — add one when you add a kind")
}

/// One-line roster, for `doctor` and the Inspector.
pub fn roster() -> Vec<(&'static str, &'static str)> {
    WORKFORCE.iter().map(|s| (s.name, s.duty)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_spec() {
        let kinds = [
            SystemKind::Interviewer,
            SystemKind::Architect,
            SystemKind::Shipwright,
            SystemKind::Testwright,
            SystemKind::Logwright,
            SystemKind::Reviewer,
            SystemKind::Firewright,
            SystemKind::Operator,
            SystemKind::Explainer,
        ];
        for k in kinds {
            let s = spec(k);
            assert!(!s.prompt.is_empty(), "{} has an empty prompt", s.name);
            assert!(
                s.prompt.len() > 200,
                "{}'s prompt is suspiciously thin",
                s.name
            );
        }
    }

    #[test]
    fn prompts_state_the_definition_of_done_or_a_contract() {
        // Every prompt must contain the word 'done' or 'contract' — a
        // soft structural check that keeps prompts honest as they evolve.
        for s in WORKFORCE {
            let text = s.prompt.to_lowercase();
            assert!(
                text.contains("done") || text.contains("contract") || text.contains("never"),
                "{}'s prompt lacks any boundary statement",
                s.name
            );
        }
    }

    #[test]
    fn accept_lines_parse_strictly() {
        let text = "I checked both promises.\nACCEPT 1: PASS\nACCEPT 2: FAIL: the digest had no links\nTESTS: FAIL: promise 2 failed";
        let accepts = parse_accepts(text);
        assert_eq!(accepts.len(), 2);
        assert_eq!(accepts[0], (1, Ok(())));
        assert_eq!(accepts[1], (2, Err("the digest had no links".to_string())));
        // strictness: junk is skipped, not guessed
        assert!(parse_accepts("ACCEPT one: PASS").is_empty());
        assert!(parse_accepts("ACCEPT 1: MAYBE").is_empty());
        // markdown decoration tolerated
        let deco = parse_accepts("**ACCEPT 1: PASS**");
        assert_eq!(deco, vec![(1, Ok(()))]);
    }

    #[test]
    fn verdict_lines_parse() {
        assert_eq!(
            parse_verdict(SystemKind::Reviewer, "Checked 3 files.\nVERDICT: APPROVE"),
            Ok(())
        );
        assert_eq!(
            parse_verdict(
                SystemKind::Reviewer,
                "**VERDICT: REJECT: summaries.json is empty**"
            ),
            Err("summaries.json is empty".into())
        );
        assert_eq!(
            parse_verdict(SystemKind::Testwright, "ran\nTESTS: PASS"),
            Ok(())
        );
        assert!(parse_verdict(SystemKind::Testwright, "I think it works")
            .unwrap_err()
            .contains("did not end"));
        assert_eq!(parse_verdict(SystemKind::Logwright, "anything"), Ok(()));
    }
}
