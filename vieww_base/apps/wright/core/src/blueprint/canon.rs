//! The canonical serialization of a blueprint.
//!
//! One serialization, one byte stream, one hash. Two blueprints that
//! differ only in comments or whitespace produce identical canonical
//! bytes — which is what makes "same intent, same commit" true, and
//! what the task cache keys off.
//!
//! Format rules: fixed section order, fixed field order, agents sorted
//! by name, edges sorted, strings escaped. No comments, no floats.

use super::model::*;
use crate::hash;

/// Escape a string for canonical output (and canon output only).
pub fn escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out
}

/// Canonical text of one agent definition.
///
/// Used by both the full-document canonicalizer and the diff engine
/// (per-agent equality = identical canonical text).
pub fn agent_canon(a: &AgentDef) -> String {
    let mut s = String::new();
    s.push_str(&format!("agent {} {{\n", escape_string(&a.name)));
    s.push_str(&format!("  goal \"{}\"\n", escape_string(&a.goal)));
    s.push_str(&format!("  model tier=\"{}\"\n", a.model_tier.as_str()));
    for t in &a.tools {
        s.push_str(&format!("  tools \"{}\"\n", escape_string(t)));
    }
    s.push_str(&format!("  memory scope=\"{}\"\n", a.memory.as_str()));
    for i in &a.inputs {
        s.push_str(&format!("  in \"{}\"\n", escape_string(i)));
    }
    for o in &a.outputs {
        s.push_str(&format!("  out \"{}\"\n", escape_string(o)));
    }
    s.push_str(&format!("  approval \"{}\"\n", a.approval.as_str()));
    s.push_str(&format!("  retries {}\n", a.retries));
    s.push_str(&format!(
        "  permissions files-read={} files-write={} network={} shell={} spawn={}\n",
        a.permissions.bits() & PermissionSet::READ_FILES != 0,
        a.permissions.bits() & PermissionSet::WRITE_FILES != 0,
        a.permissions.bits() & PermissionSet::NETWORK != 0,
        a.permissions.bits() & PermissionSet::SHELL != 0,
        a.permissions.bits() & PermissionSet::SPAWN != 0
    ));
    s.push_str("}\n");
    s
}

/// Canonical text of the whole blueprint.
pub fn canonical(bp: &Blueprint) -> String {
    let mut s = String::new();

    s.push_str("meta {\n");
    s.push_str(&format!("  name \"{}\"\n", escape_string(&bp.meta.name)));
    s.push_str(&format!("  format {}\n", bp.meta.format));
    s.push_str(&format!("  min-app \"{}\"\n", escape_string(&bp.meta.min_app)));
    s.push_str(&format!("  created \"{}\"\n", escape_string(&bp.meta.created)));
    s.push_str(&format!("  lineage \"{}\"\n", escape_string(&bp.meta.lineage)));
    s.push_str("}\n");

    s.push_str("intent {\n");
    s.push_str(&format!("  summary \"{}\"\n", escape_string(&bp.intent.summary)));
    s.push_str(&format!("  confirmed \"{}\"\n", escape_string(&bp.intent.confirmed)));
    s.push_str(&format!("  evidence \"{}\"\n", escape_string(&bp.intent.evidence)));
    s.push_str("}\n");

    let mut agents: Vec<&AgentDef> = bp.agents.iter().collect();
    agents.sort_by(|a, b| a.name.cmp(&b.name));
    for a in agents {
        s.push_str(&agent_canon(a));
    }

    let mut edges = bp.edges.clone();
    edges.sort();
    for e in &edges {
        s.push_str(&format!(
            "edge from=\"{}\" to=\"{}\"\n",
            escape_string(&e.from),
            escape_string(&e.to)
        ));
    }

    s.push_str("chips {\n");
    if let Some(sched) = &bp.chips.scheduling {
        s.push_str(&format!(
            "  scheduling cron=\"{}\" tz=\"{}\"\n",
            escape_string(&sched.cron),
            escape_string(&sched.tz)
        ));
    }
    let mut secrets = bp.chips.secrets.clone();
    secrets.sort_by(|a, b| a.name.cmp(&b.name));
    for sec in &secrets {
        s.push_str(&format!(
            "  secrets {}=\"{}\"\n",
            escape_string(&sec.name),
            escape_string(&sec.reference)
        ));
    }
    s.push_str(&format!("  logging {}\n", on_off(bp.chips.logging)));
    s.push_str(&format!("  tests {}\n", on_off(bp.chips.tests)));
    s.push_str(&format!("  ci {}\n", on_off(bp.chips.ci)));
    s.push_str(&format!("  webhooks {}\n", on_off(bp.chips.webhooks)));
    s.push_str("}\n");

    let g = &bp.guardrails;
    s.push_str(&format!(
        "guardrails {{\n  budget per-firing={} daily={}\n  wall-clock {}\n  max-turns {}\n}}\n",
        g.per_firing_micro_usd, g.daily_micro_usd, g.wall_clock_secs, g.max_turns
    ));

    for d in &bp.deliverables {
        s.push_str(&format!(
            "deliverable kind=\"{}\" {{\n  check \"{}\"\n}}\n",
            escape_string(&d.kind),
            escape_string(&d.check)
        ));
    }

    s.push_str(&format!(
        "project {{\n  language \"{}\"\n  test \"{}\"\n  run \"{}\"\n}}\n",
        escape_string(&bp.project.language),
        escape_string(&bp.project.test_cmd),
        escape_string(&bp.project.run_cmd)
    ));

    s.push_str("models {\n");
    s.push_str(&format!(
        "  tier cheap prefer=\"{}\" fallback=\"{}\"\n",
        escape_string(&bp.models.cheap.prefer),
        escape_string(&bp.models.cheap.fallback)
    ));
    s.push_str(&format!(
        "  tier quality prefer=\"{}\" fallback=\"{}\"\n",
        escape_string(&bp.models.quality.prefer),
        escape_string(&bp.models.quality.fallback)
    ));
    s.push_str("}\n");

    s
}

fn on_off(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

/// The canonical text is also the on-disk form Wright writes: the UI
/// edits the typed model and saves `canonical(bp)`. That only works
/// because parse(canonical(bp)) == bp, which a test below pins.
///
/// The content hash of a blueprint. This IS the commit identity.
pub fn content_hash(bp: &Blueprint) -> String {
    hash::hash_hex(&[canonical(bp).as_bytes()])
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::*;

    #[test]
    fn formatting_changes_do_not_change_the_hash() {
        let a = "meta { name \"x\" created \"t\" }\nintent { summary \"s\" confirmed \"v1\" }\nagent B { goal \"g\" }\nagent A { goal \"h\" }\nedge from=\"A\" to=\"B\"\ndeliverable kind=\"artifact\" { check \"c\" }\n";
        let b = "// different comments\n\nmeta {\n  // different layout\n  name \"x\"\n  created \"t\"\n}\n\nintent { summary \"s\" confirmed \"v1\" }\n\nagent A { goal \"h\" }\nagent B { goal \"g\" }\n\nedge from=\"A\" to=\"B\"\n\ndeliverable kind=\"artifact\" { check \"c\" }\n";
        let bp_a = parse::blueprint(a).unwrap();
        let bp_b = parse::blueprint(b).unwrap();
        assert_eq!(canonical(&bp_a), canonical(&bp_b));
        assert_eq!(content_hash(&bp_a), content_hash(&bp_b));
    }

    #[test]
    fn agent_order_canonicalizes() {
        let a = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\nagent A { goal \"1\" }\nagent B { goal \"2\" }\ndeliverable kind=\"k\" { check \"c\" }\n";
        let b = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\nagent B { goal \"2\" }\nagent A { goal \"1\" }\ndeliverable kind=\"k\" { check \"c\" }\n";
        let bp_a = parse::blueprint(a).unwrap();
        let bp_b = parse::blueprint(b).unwrap();
        assert_eq!(canonical(&bp_a), canonical(&bp_b));
    }

    #[test]
    fn goal_change_changes_the_hash() {
        let a = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\nagent A { goal \"one\" }\ndeliverable kind=\"k\" { check \"c\" }\n";
        let b = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\nagent A { goal \"two\" }\ndeliverable kind=\"k\" { check \"c\" }\n";
        assert_ne!(
            content_hash(&parse::blueprint(a).unwrap()),
            content_hash(&parse::blueprint(b).unwrap())
        );
    }

    #[test]
    fn canonical_text_round_trips_through_the_parser() {
        let src = include_str!("../../../examples/hn-digest/blueprint.kdl");
        let bp = parse::blueprint(src).unwrap();
        let again = parse::blueprint(&canonical(&bp)).expect("canonical text must parse");
        // Canonical order sorts agents; everything else must survive exactly.
        let mut sorted = bp.clone();
        sorted.agents.sort_by(|a, b| a.name.cmp(&b.name));
        sorted.edges.sort();
        assert_eq!(sorted, again);
        assert_eq!(canonical(&bp), canonical(&again));
    }
}
