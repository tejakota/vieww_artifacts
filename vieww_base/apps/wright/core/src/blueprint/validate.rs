//! Structural validation. Nothing invalid ever reaches the compiler,
//! let alone a firing.
//!
//! Every rejection carries a plain-language message: these strings go
//! straight into the UI ("The Mailer card points at an agent that
//! does not exist"), so they are written for users, not stack traces.

use super::model::*;
use crate::palette;
use crate::rule_of_two;
use crate::schema::Shape;

/// Validate a blueprint. Ok(()) or every problem we found, together.
pub fn validate(bp: &Blueprint) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();

    if bp.agents.is_empty() {
        errs.push("A blueprint needs at least one agent (the canvas is empty).".into());
    }

    // unique agent names
    let mut seen = std::collections::BTreeSet::new();
    for a in &bp.agents {
        if !seen.insert(a.name.clone()) {
            errs.push(format!(
                "Two agents are both named '{}' — names must be unique.",
                a.name
            ));
        }
        if a.goal.trim().is_empty() {
            errs.push(format!("Agent '{}' has no goal sentence.", a.name));
        }
    }

    // edges reference real agents
    for e in &bp.edges {
        if bp.agent(&e.from).is_none() {
            errs.push(format!(
                "Edge starts at '{}', which does not exist.",
                e.from
            ));
        }
        if bp.agent(&e.to).is_none() {
            errs.push(format!("Edge points at '{}', which does not exist.", e.to));
        }
    }

    // no cycles
    if let Err(cycle) = topo_order(bp) {
        errs.push(format!(
            "The graph has a cycle: {}. Agents cannot depend on themselves.",
            cycle
        ));
    }

    // tools are known, and the agent holds every capability they need
    // (otherwise every call would be denied at 3AM; say so now instead)
    for a in &bp.agents {
        if crate::compiler::SystemKind::from_name(&a.name).is_some() {
            errs.push(format!(
                "'{}' is the name of a built-in system agent; pick another name.",
                a.name
            ));
        }
        for t in &a.tools {
            match palette::find(t) {
                None => errs.push(format!(
                    "Agent '{}' uses tool '{}', which is not in the palette.",
                    a.name, t
                )),
                Some(spec) => {
                    for cap in spec.caps {
                        if !a.permissions.allows(*cap) {
                            errs.push(format!(
                                "Agent '{}' uses '{}' but is not allowed to {} — turn that permission on or remove the tool.",
                                a.name,
                                t,
                                cap_words(*cap)
                            ));
                        }
                    }
                    if t == "run-tests"
                        && bp.project.test_cmd.is_empty()
                        && crate::convention::default_test_cmd(&bp.project.language).is_empty()
                    {
                        errs.push(format!(
                            "Agent '{}' runs tests, but the project has no test command (set project.language or project.test).",
                            a.name
                        ));
                    }
                }
            }
        }
        for o in &a.outputs {
            if !crate::convention::is_safe_artifact_name(o) {
                errs.push(format!(
                    "Agent '{}' declares output '{}': artifact names are plain file names (letters, digits, - _ .).",
                    a.name, o
                ));
            }
        }
    }

    // every input is produced by an upstream agent
    for a in &bp.agents {
        for i in &a.inputs {
            let producers: Vec<&str> = bp
                .agents
                .iter()
                .filter(|p| p.outputs.contains(i))
                .map(|p| p.name.as_str())
                .collect();
            if producers.is_empty() {
                errs.push(format!(
                    "Agent '{}' needs '{}', but no agent produces it.",
                    a.name, i
                ));
            } else if !producers.iter().any(|p| reaches(bp, p, &a.name)) {
                errs.push(format!(
                    "Agent '{}' needs '{}' from '{}', but there is no edge path between them — connect the cards.",
                    a.name, i, producers[0]
                ));
            }
        }
    }

    // the schedule is a real cron expression
    if let Some(s) = &bp.chips.scheduling {
        if let Err(e) = crate::schedule::Cron::parse(&s.cron) {
            errs.push(format!("The schedule '{}' is not valid: {e}.", s.cron));
        }
    }

    // a project must define done
    if bp.deliverables.is_empty() {
        errs.push("A blueprint needs at least one deliverable — the definition of done.".into());
    }
    for d in &bp.deliverables {
        if d.check.trim().is_empty() {
            errs.push(format!("Deliverable '{}' has an empty check.", d.kind));
        }
        for a in &d.accept {
            // Acceptance sentences are promises a person approves and
            // Testwright turns into checks; a blank one is a lie.
            if a.trim().len() < 8 {
                errs.push(format!(
                    "Deliverable '{}' has an acceptance sentence that is too short to check: \"{}\" — write what a reader could verify.",
                    d.kind, a
                ));
            }
            if a.trim().len() > 300 {
                errs.push(format!(
                    "Deliverable '{}' has an acceptance sentence longer than 300 characters — split it into two sentences.",
                    d.kind
                ));
            }
        }
    }

    // the Rule of Two (ADR-024): no agent holds private data, untrusted
    // input and an outbound channel at once without a promised human ask
    for v in rule_of_two::violations(bp) {
        let a = bp.agent(&v.agent);
        let approved = a
            .map(|x| x.approval == ApprovalPolicy::BeforeSend)
            .unwrap_or(false);
        if !approved {
            errs.push(v.sentence());
        }
    }

    // typed edges (ADR-027): names must match the cards, shapes must be named
    for e in &bp.edges {
        let (Some(from), Some(to)) = (bp.agent(&e.from), bp.agent(&e.to)) else {
            continue; // dangling edge: named above
        };
        if let Some(o) = &e.out {
            if !from.outputs.iter().any(|x| x == o) {
                errs.push(format!(
                    "The edge {} -> {} carries out=\"{}\", but '{}' does not declare that output.",
                    e.from, e.to, o, e.from
                ));
            }
        }
        if let Some(i) = &e.input {
            if !to.inputs.iter().any(|x| x == i) {
                errs.push(format!(
                    "The edge {} -> {} carries in=\"{}\", but '{}' does not read that input.",
                    e.from, e.to, i, e.to
                ));
            }
        }
        if e.schema.is_some() && e.out.is_none() {
            errs.push(format!(
                "The edge {} -> {} promises a schema but no out=\"...\" — a promise needs a name.",
                e.from, e.to
            ));
        }
        let _ = Shape::Any; // (shapes parse in the parser; this validates binding)
    }

    // per-agent secrets name the blueprint's own secrets
    for a in &bp.agents {
        for sec in &a.secrets {
            if !bp.chips.secrets.iter().any(|s| &s.name == sec) {
                errs.push(format!(
                    "Agent '{}' uses the secret '{}', which the blueprint does not declare (chips {{ secrets {}=\"keychain:...\" }}).",
                    a.name, sec, sec
                ));
            }
        }
    }

    // worker steps: the files they may change must exist in the plan's
    // world, and the card must carry the permissions the worker tool needs
    for a in &bp.agents {
        if let Some(w) = &a.worker {
            if a.goal.trim().is_empty() {
                errs.push(format!("Worker '{}' has no goal sentence — the worker gets exactly that text and nothing else.", a.name));
            }
            let _ = w;
            for f in &a.worker_files {
                if f.contains("..") || f.starts_with('/') {
                    errs.push(format!(
                        "Worker '{}' may change '{}': worker-files are project-relative paths, not absolute or parent-escaping.",
                        a.name, f
                    ));
                }
            }
            if a.retries > 1 {
                errs.push(format!(
                    "Worker '{}' has retries={} — external agents are slow and expensive; at most one retry is allowed.",
                    a.name, a.retries
                ));
            }
        }
        if let Some(server) = &a.mcp_server {
            if a.mcp_allow.is_empty() {
                errs.push(format!(
                    "Agent '{}' names the MCP server '{}' but allows no tools — add allow=\"tool,tool\" or remove the card line.",
                    a.name, server
                ));
            }
        }
    }

    // budgets are sane
    if bp.guardrails.per_firing_micro_usd > bp.guardrails.daily_micro_usd {
        errs.push("The per-firing budget is larger than the daily budget.".into());
    }
    if bp.guardrails.max_turns == 0 {
        errs.push("max-turns must be at least 1.".into());
    }

    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

fn cap_words(c: palette::Capability) -> &'static str {
    match c {
        palette::Capability::ReadFiles => "read files",
        palette::Capability::WriteFiles => "write files",
        palette::Capability::Network => "use the network",
        palette::Capability::Shell => "run shell commands",
        palette::Capability::Spawn => "start programs",
    }
}

/// True when `to` is reachable from `from` along edges.
fn reaches(bp: &Blueprint, from: &str, to: &str) -> bool {
    let mut stack = vec![from.to_string()];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(n) = stack.pop() {
        if n == to {
            return true;
        }
        if seen.insert(n.clone()) {
            stack.extend(bp.downstream_of(&n));
        }
    }
    false
}

/// Kahn's algorithm. Ok(topological order) or Err(cycle description).
pub fn topo_order(bp: &Blueprint) -> Result<Vec<String>, String> {
    let names: Vec<&str> = bp.agents.iter().map(|a| a.name.as_str()).collect();
    let mut indegree: std::collections::BTreeMap<&str, usize> =
        names.iter().map(|n| (*n, 0usize)).collect();
    let mut downstream: std::collections::BTreeMap<&str, Vec<&str>> =
        names.iter().map(|n| (*n, Vec::new())).collect();

    for e in &bp.edges {
        if let Some(d) = downstream.get_mut(e.from.as_str()) {
            d.push(e.to.as_str());
        }
        if let Some(deg) = indegree.get_mut(e.to.as_str()) {
            *deg += 1;
        }
    }

    let mut queue: Vec<&str> = indegree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(n, _)| *n)
        .collect();
    queue.sort_unstable();

    let mut order = Vec::with_capacity(names.len());
    while let Some(n) = queue.pop() {
        order.push(n.to_string());
        if let Some(children) = downstream.get(n) {
            for c in children {
                if let Some(deg) = indegree.get_mut(c) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push(c);
                    }
                }
            }
        }
        queue.sort_unstable();
    }

    if order.len() != names.len() {
        let stuck: Vec<String> = indegree
            .iter()
            .filter(|(n, d)| **d > 0 && !order.contains(&n.to_string()))
            .map(|(n, _)| n.to_string())
            .collect();
        Err(stuck.join(" -> "))
    } else {
        Ok(order)
    }
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::*;

    const GOOD: &str = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\nagent A { goal \"1\" }\nagent B { goal \"2\" }\nedge from=\"A\" to=\"B\"\ndeliverable kind=\"k\" { check \"c\" }\n";

    #[test]
    fn good_example_passes() {
        let bp = parse::blueprint(GOOD).unwrap();
        assert!(validate(&bp).is_ok());
        // A -> B: A first (no dependencies), then B.
        assert_eq!(
            topo_order(&bp).unwrap(),
            vec!["A".to_string(), "B".to_string()]
        );
    }

    #[test]
    fn cycles_are_rejected() {
        let src = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\nagent A { goal \"1\" }\nagent B { goal \"2\" }\nedge from=\"A\" to=\"B\"\nedge from=\"B\" to=\"A\"\ndeliverable kind=\"k\" { check \"c\" }\n";
        let bp = parse::blueprint(src).unwrap();
        let errs = validate(&bp).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("cycle")));
    }

    #[test]
    fn unknown_tool_is_rejected() {
        let src = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\nagent A { goal \"1\" tools \"teleport\" }\ndeliverable kind=\"k\" { check \"c\" }\n";
        let bp = parse::blueprint(src).unwrap();
        let errs = validate(&bp).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("teleport")));
    }

    #[test]
    fn missing_deliverable_is_rejected() {
        let src = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\nagent A { goal \"1\" }\n";
        let bp = parse::blueprint(src).unwrap();
        let errs = validate(&bp).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("deliverable")));
    }

    #[test]
    fn missing_capability_is_caught_before_any_firing() {
        let src = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\nagent A { goal \"1\" tools \"http-read\" }\ndeliverable kind=\"k\" { check \"c\" }\n";
        let errs = validate(&parse::blueprint(src).unwrap()).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.contains("not allowed to use the network")),
            "{errs:?}"
        );
    }

    #[test]
    fn inputs_need_a_connected_producer() {
        let src = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\nagent A { goal \"1\" out \"a.json\" }\nagent B { goal \"2\" in \"a.json\" }\ndeliverable kind=\"k\" { check \"c\" }\n";
        let errs = validate(&parse::blueprint(src).unwrap()).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("no edge path")), "{errs:?}");
        let ok = src.replace("deliverable", "edge from=\"A\" to=\"B\"\ndeliverable");
        assert!(validate(&parse::blueprint(&ok).unwrap()).is_ok());
    }

    #[test]
    fn system_names_are_reserved_and_cron_is_checked() {
        let src = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\nagent Reviewer { goal \"1\" }\nchips { scheduling cron=\"99 * * * *\" }\ndeliverable kind=\"k\" { check \"c\" }\n";
        let errs = validate(&parse::blueprint(src).unwrap()).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("built-in system agent")));
        assert!(errs.iter().any(|e| e.contains("not valid")));
    }
}
