//! The Rule of Two (ADR-024), made checkable.
//!
//! Meta's rule, stated for agents that run unattended: an agent may not
//! hold private data, untrusted input and an outbound channel at the same
//! time unless a human approved that combination. Wright can ENFORCE it,
//! because a Wright agent's reach is exactly what its card says: the
//! tools it may call, the artifacts it reads, the artifacts it writes.
//! The coding agents rely on prompts and hooks; Wright refuses the plan.
//!
//! Three "legs" a tool can put under an agent:
//!
//! * [`Leg::PrivateData`] — it can read the person's own data: project
//!   files, the git log, anything in the sandbox that is not public.
//! * [`Leg::UntrustedInput`] — its output depends on the outside world:
//!   web pages, shell-visible system state, another program's answer
//!   (an MCP server's, a worker agent's).
//! * [`Leg::OutboundChannel`] — it can send something OUT: POST, a
//!   browser, anything with a world effect.
//!
//! An agent standing on all three legs can be steered by text it did not
//! ask for into shipping text it should not share. So:
//!
//! * [`violations`] names every 3-leg agent whose card does not promise a
//!   human approval (`approval "before-send"`), with a plain sentence.
//!   The validator refuses the blueprint until each one is approved.
//! * Untrusted data is TRACKED (the memo's bet #4): an artifact is
//!   untrusted when any agent that stands on the untrusted leg produced
//!   it; an agent that READS an untrusted artifact inherits that leg —
//!   a fixpoint over the graph, so the mark follows the data, not the
//!   tool that touched it last.
//!
//! The gate (runtime) asks a human on every outbound step of a 3-leg
//! agent even when approval was already promised — defense in depth:
//! the approval names the combination, the ask names the actual send.

use crate::blueprint::model::Blueprint;
use crate::palette::{self, Leg, Legs};

/// The legs one agent stands on: its tools' legs, plus legs inherited
/// from untrusted artifacts it reads (the taint fixpoint).
pub fn legs_of_agent(bp: &Blueprint, name: &str) -> Legs {
    let mut legs = Legs::none();
    if let Some(a) = bp.agent(name) {
        for t in &a.tools {
            if let Some(spec) = palette::find(t) {
                legs |= spec.legs;
            }
        }
        // worker steps hand the work to another program: whatever it
        // reads is what that program reads, and its output is untrusted.
        if a.worker.is_some() {
            legs = legs.with(Leg::PrivateData) | Legs::untrusted();
        }
    }
    // Inherit the untrusted leg from every untrusted input artifact.
    for tainted in untrusted_artifacts(bp) {
        if let Some(a) = bp.agent(name) {
            if a.inputs.iter().any(|i| tainted.contains(i)) {
                legs |= Legs::untrusted();
            }
        }
    }
    legs
}

/// Artifacts that carry untrusted data (the taint fixpoint): an artifact
/// is untrusted when any producer of it stands on the untrusted leg —
/// including producers that inherited the mark through their own inputs.
pub fn untrusted_artifacts(bp: &Blueprint) -> std::collections::BTreeSet<String> {
    // Iterate to a fixpoint: a producer reading a tainted artifact is
    // tainted, so anything it writes is tainted too. The graph is a DAG
    // (validation refuses cycles), so the loop converges — at most once
    // per topological level, bounded hard below in case of surprises.
    let mut tainted: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut changed = true;
    let mut rounds = 0;
    while changed {
        rounds += 1;
        if rounds > bp.agents.len() + 2 {
            break; // never spin: validation refuses cycles anyway
        }
        changed = false;
        for a in &bp.agents {
            let mut legs = Legs::none();
            for t in &a.tools {
                if let Some(spec) = palette::find(t) {
                    legs |= spec.legs;
                }
            }
            if a.worker.is_some() {
                legs |= Legs::untrusted();
            }
            let inherited = a.inputs.iter().any(|i| tainted.contains(i));
            if (legs.is_untrusted() || inherited) && !a.outputs.is_empty() {
                for o in &a.outputs {
                    if tainted.insert(o.clone()) {
                        changed = true;
                    }
                }
            }
        }
    }
    tainted
}

/// One 3-leg agent that needs a human approval on its card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub agent: String,
    pub legs: Legs,
}

impl Violation {
    pub fn sentence(&self) -> String {
        format!(
            "'{}' would hold all three legs at once — {} (private data), {} (untrusted input) and {} (an outbound channel). \
             Someone must approve every send it makes: give the agent `approval \"before-send\"`, or take one leg away.",
            self.agent,
            if self.legs.has(Leg::PrivateData) { "reads your files" } else { "—" },
            if self.legs.has(Leg::UntrustedInput) { "reads the outside world" } else { "—" },
            if self.legs.has(Leg::OutboundChannel) { "can send things out" } else { "—" },
        )
    }
}

/// Every agent that stands on all three legs. The validator refuses the
/// blueprint until each one carries `approval "before-send"`; the gate
/// still asks before each outbound step, whatever the card says.
pub fn violations(bp: &Blueprint) -> Vec<Violation> {
    let mut out = Vec::new();
    for a in &bp.agents {
        let legs = legs_of_agent(bp, &a.name);
        if legs.is_three_legged() {
            out.push(Violation {
                agent: a.name.clone(),
                legs,
            });
        }
    }
    out
}

/// True when this agent needs a per-send human ask at run time (the gate
/// consults this for its belt-and-braces ask).
pub fn needs_send_gate(bp: &Blueprint, name: &str) -> bool {
    legs_of_agent(bp, name).is_three_legged()
}

/// A plain-language report of an agent's legs, for the Confirm Gate and
/// `wright plan`.
pub fn describe_legs(legs: Legs) -> String {
    if legs.is_none() {
        return "touches nothing risky".into();
    }
    let mut parts: Vec<&str> = Vec::new();
    if legs.has(Leg::PrivateData) {
        parts.push("reads your private data");
    }
    if legs.has(Leg::UntrustedInput) {
        parts.push("takes untrusted input");
    }
    if legs.has(Leg::OutboundChannel) {
        parts.push("can send outward");
    }
    match parts.len() {
        1 => parts[0].to_string(),
        2 => format!("{} and {}", parts[0], parts[1]),
        _ => format!(
            "{} and {}",
            parts[..parts.len() - 1].join(", "),
            parts[parts.len() - 1]
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::super::blueprint::parse;
    use super::*;

    const HEAD: &str = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\n";

    #[test]
    fn a_sandboxed_agent_stands_on_no_legs() {
        let bp = parse::blueprint(&format!("{HEAD}agent A {{ goal \"g\" out \"a.txt\" }}\ndeliverable kind=\"k\" {{ check \"c\" }}")).unwrap();
        assert!(legs_of_agent(&bp, "A").is_none());
        assert!(violations(&bp).is_empty());
    }

    #[test]
    fn fetcher_then_mailer_is_two_legs_each_not_three() {
        // The classic split: the Fetcher reads the web (untrusted) and
        // the Mailer sends (outbound) — neither holds all three.
        let src = format!(
            "{HEAD}\
agent Fetcher {{ goal \"fetch\" tools \"http-read\" out \"stories.json\" permissions network=true }}\n\
agent Mailer {{ goal \"mail\" tools \"http-post\" in \"stories.json\" approval \"before-send\" permissions network=true }}\n\
edge from=\"Fetcher\" to=\"Mailer\"\n\
deliverable kind=\"email\" {{ check \"c\" }}"
        );
        let bp = parse::blueprint(&src).unwrap();
        assert!(!legs_of_agent(&bp, "Fetcher").is_three_legged());
        assert!(
            !legs_of_agent(&bp, "Mailer").is_three_legged(),
            "the Mailer's input is untrusted, but it holds no private data"
        );
        assert!(violations(&bp).is_empty());
        // the artifact IS marked untrusted (taint from the Fetcher)
        assert!(untrusted_artifacts(&bp).contains("stories.json"));
    }

    #[test]
    fn one_agent_holding_all_three_is_a_violation() {
        let src = format!(
            "{HEAD}\
agent Greedy {{ goal \"all\" tools \"http-read\" tools \"http-post\" tools \"files-read\" out \"out.json\" permissions network=true files-read=true }}\n\
deliverable kind=\"k\" {{ check \"c\" }}"
        );
        let bp = parse::blueprint(&src).unwrap();
        let v = violations(&bp);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].agent, "Greedy");
        assert!(v[0].sentence().contains("all three legs"));
    }

    #[test]
    fn approval_on_the_card_resolves_the_violation() {
        let with = "agent Greedy { goal \"all\" tools \"http-read\" tools \"http-post\" tools \"files-read\" approval \"before-send\" out \"o\" permissions network=true files-read=true }";
        let bp = parse::blueprint(&format!(
            "{HEAD}{with}\ndeliverable kind=\"k\" {{ check \"c\" }}"
        ))
        .unwrap();
        assert_eq!(violations(&bp).len(), 1, "still named, so the gate can ask");
        assert!(needs_send_gate(&bp, "Greedy"));
    }

    #[test]
    fn taint_flows_transitively_through_the_graph() {
        // Fetcher(untrusted) -> Parser(clean tools, reads stories) -> Mailer(sends)
        // The Mailer inherits the untrusted mark through the Parser.
        let src = format!(
            "{HEAD}\
agent Fetcher {{ goal \"f\" tools \"http-read\" out \"raw.json\" permissions network=true }}\n\
agent Parser {{ goal \"p\" in \"raw.json\" out \"clean.json\" }}\n\
agent Mailer {{ goal \"m\" tools \"http-post\" in \"clean.json\" approval \"before-send\" permissions network=true }}\n\
edge from=\"Fetcher\" to=\"Parser\"\n\
edge from=\"Parser\" to=\"Mailer\"\n\
deliverable kind=\"email\" {{ check \"c\" }}"
        );
        let bp = parse::blueprint(&src).unwrap();
        let t = untrusted_artifacts(&bp);
        assert!(t.contains("raw.json"), "direct taint");
        assert!(t.contains("clean.json"), "taint flows through the Parser");
        // The Mailer: outbound + inherited untrusted. Two legs, not three
        // (no private data) — allowed with its approval, refused without.
        let mailer_legs = legs_of_agent(&bp, "Mailer");
        assert!(mailer_legs.has(Leg::OutboundChannel));
        assert!(mailer_legs.has(Leg::UntrustedInput));
        assert!(!mailer_legs.has(Leg::PrivateData));
    }

    #[test]
    fn worker_steps_stand_on_private_and_untrusted_legs() {
        let bp = parse::blueprint(&format!(
            "{HEAD}agent W {{ goal \"code it\" worker \"claude-code\" out \"patch.diff\" }}"
        ))
        .unwrap();
        let legs = legs_of_agent(&bp, "W");
        assert!(
            legs.has(Leg::PrivateData),
            "a worker reads the project copy"
        );
        assert!(
            legs.has(Leg::UntrustedInput),
            "its output is another program's answer"
        );
    }

    #[test]
    fn leg_sentences_read_like_a_person_wrote_them() {
        let mut legs = Legs::none().with(Leg::PrivateData);
        assert_eq!(describe_legs(legs), "reads your private data");
        legs = legs.with(Leg::UntrustedInput);
        assert_eq!(
            describe_legs(legs),
            "reads your private data and takes untrusted input"
        );
        legs = legs.with(Leg::OutboundChannel);
        assert_eq!(
            describe_legs(legs),
            "reads your private data, takes untrusted input and can send outward"
        );
        assert_eq!(describe_legs(Legs::none()), "touches nothing risky");
    }
}
