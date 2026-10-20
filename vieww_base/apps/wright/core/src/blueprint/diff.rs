//! Structural diff and the refire plan.
//!
//! A diff between two blueprint versions does not just say "what
//! changed" — it compiles into a refire plan: which agents must run
//! again, and which are cache hits (free, instant, zero tokens).
//!
//! The rules, as a table (this is the moat, keep it true):
//!
//! | Change                         | Refire                                  | Cached          |
//! |--------------------------------|-----------------------------------------|-----------------|
//! | agent definition               | that agent + downstream                 | upstream        |
//! | edge added / removed           | the edge's TARGET + downstream          | everything else |
//! | edge list merely reordered     | nothing (same commit hash)              | everything      |
//! | chip toggled                   | its system agents only                  | project agents  |
//! | guardrails changed             | nothing (compile-time)                  | everything      |
//! | schedule/tz changed            | nothing (schedule-time)                 | everything      |
//! | deliverable check changed      | Testwright + Reviewer + delivery agents | project agents  |
//! | model tier preference swapped  | agents on that tier + downstream        | others          |
//! | intent summary changed         | nothing (display only)                  | everything      |
//! | project language/commands      | every agent (the skeleton changed)      | nothing         |
//!
//! Two kinds of agents are NEVER served from the cache, whatever the
//! diff says, and the plan marks them so:
//!
//! * **volatile** agents — they read the outside world (`http-read`,
//!   `shell`), so yesterday's answer is not today's;
//! * **delivery** agents — they touch the world (`http-post`), and a
//!   cached send is a send that never happened.
//!
//! System agents (Testwright, Reviewer, ...) appear in the plan too, so
//! the UI can say the whole truth: "refires Summarizer + Testwright".

use std::collections::{BTreeMap, BTreeSet};

use super::canon;
use super::model::*;

/// What changed between two blueprint versions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlueprintDiff {
    pub agent_changed: BTreeSet<String>,
    pub agent_added: BTreeSet<String>,
    pub agent_removed: BTreeSet<String>,
    /// Edges present in the new version only.
    pub edges_added: BTreeSet<Edge>,
    /// Edges present in the old version only.
    pub edges_removed: BTreeSet<Edge>,
    /// Chip names whose state changed ("tests", "logging", ...).
    pub chip_changed: BTreeSet<String>,
    pub guardrails_changed: bool,
    pub deliverables_changed: bool,
    /// Tier names whose preference changed ("cheap", "quality").
    pub models_changed: BTreeSet<String>,
    pub intent_changed: bool,
    pub project_changed: bool,
}

impl BlueprintDiff {
    pub fn is_empty(&self) -> bool {
        *self == BlueprintDiff::default()
    }

    /// True when the edge SET differs (order never matters).
    pub fn edges_changed(&self) -> bool {
        !self.edges_added.is_empty() || !self.edges_removed.is_empty()
    }
}

/// Diff two blueprints by canonical content, per structure.
pub fn diff(old: &Blueprint, new: &Blueprint) -> BlueprintDiff {
    let mut d = BlueprintDiff::default();

    let old_agents: BTreeMap<&str, String> =
        old.agents.iter().map(|a| (a.name.as_str(), canon::agent_canon(a))).collect();
    let new_agents: BTreeMap<&str, String> =
        new.agents.iter().map(|a| (a.name.as_str(), canon::agent_canon(a))).collect();

    for (name, text) in &new_agents {
        match old_agents.get(name) {
            None => {
                d.agent_added.insert((*name).to_string());
            }
            Some(old_text) if old_text != text => {
                d.agent_changed.insert((*name).to_string());
            }
            _ => {}
        }
    }
    for name in old_agents.keys() {
        if !new_agents.contains_key(name) {
            d.agent_removed.insert(name.to_string());
        }
    }

    // Edges are a SET. The canonical form sorts them, so two files that
    // differ only in edge order have the same commit hash — and must
    // have an empty diff, or the cache would burn money on a reorder.
    let old_edges: BTreeSet<Edge> = old.edges.iter().cloned().collect();
    let new_edges: BTreeSet<Edge> = new.edges.iter().cloned().collect();
    d.edges_added = new_edges.difference(&old_edges).cloned().collect();
    d.edges_removed = old_edges.difference(&new_edges).cloned().collect();

    let chips = [
        ("scheduling", old.chips.scheduling != new.chips.scheduling),
        ("secrets", old.chips.secrets != new.chips.secrets),
        ("logging", old.chips.logging != new.chips.logging),
        ("tests", old.chips.tests != new.chips.tests),
        ("ci", old.chips.ci != new.chips.ci),
        ("webhooks", old.chips.webhooks != new.chips.webhooks),
    ];
    for (name, changed) in chips {
        if changed {
            d.chip_changed.insert(name.into());
        }
    }

    d.guardrails_changed = old.guardrails != new.guardrails;
    d.deliverables_changed = old.deliverables != new.deliverables;
    if old.models.cheap != new.models.cheap {
        d.models_changed.insert("cheap".to_string());
    }
    if old.models.quality != new.models.quality {
        d.models_changed.insert("quality".to_string());
    }
    d.intent_changed = old.intent != new.intent;
    d.project_changed = old.project != new.project;
    d
}

/// What to run again, and what the cache hands back for free.
#[derive(Debug, Clone, Default)]
pub struct RefirePlan {
    /// Project agents that run again.
    pub refire: BTreeSet<String>,
    /// Project agents served from the cache.
    pub cached: BTreeSet<String>,
    /// Untouched by the edit, but never cached (live data / delivery):
    /// they run every firing anyway. Kept apart so the sentence never
    /// promises "cached — free" for work that will run.
    pub fresh: BTreeSet<String>,
    /// System agents that run again (by display name).
    pub system_refire: BTreeSet<String>,
    /// Human-readable reason per refired agent (project or system).
    pub reasons: BTreeMap<String, String>,
    /// Display order: project agents in dependency order, then system
    /// agents in firing order. Sentences read like the firing does.
    pub order: Vec<String>,
}

impl RefirePlan {
    /// One sentence a human enjoys reading, for the REFINE view and CLI.
    pub fn sentence(&self) -> String {
        let pos = |n: &String| self.order.iter().position(|o| o == n).unwrap_or(usize::MAX);
        let mut runs: Vec<String> = self.refire.iter().cloned().collect();
        runs.sort_by_key(pos);
        let mut sys: Vec<String> = self.system_refire.iter().cloned().collect();
        sys.sort_by_key(pos);
        runs.extend(sys);
        let mut cached: Vec<String> = self.cached.iter().cloned().collect();
        cached.sort_by_key(pos);
        let mut fresh: Vec<String> = self.fresh.iter().cloned().collect();
        fresh.sort_by_key(pos);
        let is_are = |v: &[String]| if v.len() == 1 { format!("{} is", v[0]) } else { format!("{} are", v.join(" + ")) };
        let mut s = if runs.is_empty() {
            "Nothing needs to run again because of this change.".to_string()
        } else {
            format!("This change refires {}.", runs.join(" + "))
        };
        if !cached.is_empty() {
            s.push_str(&format!(" {} cached — free.", is_are(&cached)));
        }
        if !fresh.is_empty() {
            s.push_str(&format!(" {} fresh every run anyway (live data or delivery).", is_are(&fresh)));
        }
        s
    }
}

/// Every agent that can never be served from the cache in a firing:
/// volatile and delivery agents, plus everything downstream of them.
pub fn never_cached(bp: &Blueprint) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for a in &bp.agents {
        if a.is_delivery() {
            out.insert(a.name.clone(), "touches the world (never cached)".into());
        } else if a.is_volatile() {
            out.insert(a.name.clone(), "reads the outside world (fresh every firing)".into());
        }
    }
    close_downstream(bp, &mut out, "upstream is fresh every firing");
    out
}

fn close_downstream(bp: &Blueprint, marked: &mut BTreeMap<String, String>, why: &str) {
    loop {
        let mut grown = false;
        for e in &bp.edges {
            if marked.contains_key(&e.from) && !marked.contains_key(&e.to) {
                marked.insert(e.to.clone(), why.to_string());
                grown = true;
            }
        }
        if !grown {
            break;
        }
    }
}

/// Compile a diff into a refire plan against the NEW blueprint.
///
/// This answers "what does this EDIT cost?" — the REFINE question. It
/// is about invalidation by the edit alone; [`never_cached`] answers
/// the separate question of what any firing must re-run anyway.
pub fn refire_plan(d: &BlueprintDiff, new: &Blueprint) -> RefirePlan {
    let all: BTreeSet<String> = new.agents.iter().map(|a| a.name.clone()).collect();
    let mut marked: BTreeMap<String, String> = BTreeMap::new();
    let mark = |name: &str, why: &str, marked: &mut BTreeMap<String, String>| {
        if all.contains(name) {
            marked.entry(name.to_string()).or_insert_with(|| why.to_string());
        }
    };

    if d.project_changed {
        for a in &new.agents {
            mark(&a.name, "project skeleton changed", &mut marked);
        }
    }

    for name in &d.agent_changed {
        mark(name, "definition changed", &mut marked);
    }
    for name in &d.agent_added {
        mark(name, "new agent", &mut marked);
    }

    for tier in &d.models_changed {
        for a in &new.agents {
            if a.model_tier.as_str() == tier.as_str()
                || (tier == "cheap" && a.model_tier == ModelTier::Auto)
            {
                mark(&a.name, &format!("model tier '{tier}' changed"), &mut marked);
            }
        }
    }

    // An edge carries an input. Adding or removing one changes what the
    // TARGET receives; the source's own work is untouched.
    for e in d.edges_added.iter().chain(d.edges_removed.iter()) {
        mark(&e.to, &format!("inputs changed (edge {} -> {})", e.from, e.to), &mut marked);
    }

    close_downstream(new, &mut marked, "upstream refired");

    // ── system agents ──────────────────────────────────────────
    let mut system: BTreeMap<String, String> = BTreeMap::new();
    let any_project = !marked.is_empty();
    if new.chips.tests && (any_project || d.deliverables_changed || d.chip_changed.contains("tests")) {
        let why = if d.deliverables_changed { "definition of done changed" } else { "checks the new work" };
        system.insert("Testwright".into(), why.into());
        system.insert("Reviewer".into(), why.into());
    }
    if d.deliverables_changed {
        // Delivery agents re-deliver against the new definition of done.
        for a in &new.agents {
            if a.is_delivery() {
                marked.entry(a.name.clone()).or_insert_with(|| "definition of done changed".into());
            }
        }
    }
    if new.chips.logging && d.chip_changed.contains("logging") {
        system.insert("Logwright".into(), "logging chip toggled".into());
    }
    if (new.chips.ci || new.chips.webhooks)
        && (d.chip_changed.contains("ci") || d.chip_changed.contains("webhooks"))
    {
        system.insert("Firewright".into(), "ci/webhooks chip toggled".into());
    }
    if (new.chips.scheduling.is_some() || !new.chips.secrets.is_empty())
        && (d.chip_changed.contains("scheduling") || d.chip_changed.contains("secrets"))
    {
        system.insert("Operator".into(), "schedule or secrets changed".into());
    }

    let refire: BTreeSet<String> = marked.keys().cloned().collect();
    let never: BTreeSet<String> = never_cached(new).into_keys().collect();
    let untouched: BTreeSet<String> = all.difference(&refire).cloned().collect();
    let fresh: BTreeSet<String> = untouched.intersection(&never).cloned().collect();
    let cached: BTreeSet<String> = untouched.difference(&never).cloned().collect();
    let system_refire: BTreeSet<String> = system.keys().cloned().collect();
    let mut reasons = marked;
    reasons.extend(system);
    let mut order = super::validate::topo_order(new).unwrap_or_default();
    order.extend(["Logwright", "Firewright", "Testwright", "Reviewer", "Operator"].map(String::from));
    RefirePlan { refire, cached, fresh, system_refire, reasons, order }
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::*;

    /// The canonical example: Fetcher -> Summarizer -> Mailer.
    fn hn() -> Blueprint {
        let src = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\n\
agent Fetcher { goal \"fetch\" model tier=\"cheap\" }\n\
agent Summarizer { goal \"sum\" model tier=\"quality\" }\n\
agent Mailer { goal \"mail\" model tier=\"cheap\" }\n\
edge from=\"Fetcher\" to=\"Summarizer\"\n\
edge from=\"Summarizer\" to=\"Mailer\"\n\
chips { logging on tests on }\n\
guardrails { budget per-firing=500000 daily=3000000 }\n\
deliverable kind=\"email\" { check \"c\" }";
        parse::blueprint(src).unwrap()
    }

    fn names(v: &[&str]) -> BTreeSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn goal_change_refires_downstream_only() {
        let old = hn();
        let mut new = old.clone();
        new.agent_mut("Summarizer").unwrap().goal = "much punchier".into();
        let plan = refire_plan(&diff(&old, &new), &new);
        assert_eq!(plan.refire, names(&["Summarizer", "Mailer"]));
        assert_eq!(plan.cached, names(&["Fetcher"]));
        // the checks re-run on new work, and the plan says so
        assert!(plan.system_refire.contains("Testwright"));
        assert!(plan.sentence().contains("Fetcher is cached"));
    }

    #[test]
    fn guardrail_change_refires_nothing() {
        let old = hn();
        let mut new = old.clone();
        new.guardrails.per_firing_micro_usd = 250_000;
        let d = diff(&old, &new);
        assert!(d.guardrails_changed);
        let plan = refire_plan(&d, &new);
        assert!(plan.refire.is_empty(), "guardrails must never invalidate work");
        assert!(plan.system_refire.is_empty());
        assert_eq!(plan.cached.len(), 3);
    }

    #[test]
    fn chip_change_is_system_only() {
        let old = hn();
        let mut new = old.clone();
        new.chips.logging = false;
        new.chips.tests = false;
        new.chips.ci = true;
        let d = diff(&old, &new);
        let plan = refire_plan(&d, &new);
        assert!(plan.refire.is_empty());
        assert_eq!(plan.system_refire, names(&["Firewright"]));
    }

    #[test]
    fn edge_reorder_is_not_a_change() {
        // Regression: v0.1 compared edges as a Vec, so a reorder (same
        // commit hash!) refired every agent.
        let old = hn();
        let mut new = old.clone();
        new.edges.reverse();
        assert_eq!(canon::content_hash(&old), canon::content_hash(&new));
        let d = diff(&old, &new);
        assert!(d.is_empty(), "{d:?}");
        assert!(refire_plan(&d, &new).refire.is_empty());
    }

    #[test]
    fn adding_an_edge_refires_only_its_target() {
        // Regression: v0.1 refired every agent touching ANY edge.
        let old = hn();
        let mut new = old.clone();
        new.edges.push(Edge { from: "Fetcher".into(), to: "Mailer".into() });
        let plan = refire_plan(&diff(&old, &new), &new);
        assert_eq!(plan.refire, names(&["Mailer"]));
        assert_eq!(plan.cached, names(&["Fetcher", "Summarizer"]));
    }

    #[test]
    fn deliverable_change_rechecks_and_redelivers() {
        let src = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\n\
agent Writer { goal \"w\" }\n\
agent Mailer { goal \"m\" tools \"http-post\" permissions network=true }\n\
edge from=\"Writer\" to=\"Mailer\"\nchips { tests on }\n\
deliverable kind=\"email\" { check \"c\" }";
        let old = parse::blueprint(src).unwrap();
        let mut new = old.clone();
        new.deliverables[0].check = "stricter".into();
        let plan = refire_plan(&diff(&old, &new), &new);
        assert_eq!(plan.refire, names(&["Mailer"]));
        assert!(plan.system_refire.contains("Reviewer"));
        assert_eq!(plan.cached, names(&["Writer"]));
    }

    #[test]
    fn the_sentence_never_calls_live_work_cached() {
        let src = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\n\
agent Fetcher { goal \"f\" tools \"http-read\" out \"a.json\" permissions network=true }\n\
agent Summarizer { goal \"s\" in \"a.json\" }\nagent Other { goal \"o\" }\n\
edge from=\"Fetcher\" to=\"Summarizer\"\ndeliverable kind=\"k\" { check \"c\" }";
        let old = parse::blueprint(src).unwrap();
        let mut new = old.clone();
        new.agent_mut("Summarizer").unwrap().goal = "punchier".into();
        let plan = refire_plan(&diff(&old, &new), &new);
        assert_eq!(plan.fresh, names(&["Fetcher"]));
        assert_eq!(plan.cached, names(&["Other"]));
        let s = plan.sentence();
        assert!(s.contains("Other is cached") && s.contains("Fetcher is fresh every run"), "{s}");
    }

    #[test]
    fn volatile_and_delivery_agents_are_never_cached() {
        let src = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\n\
agent Fetcher { goal \"f\" tools \"http-read\" permissions network=true }\n\
agent Summarizer { goal \"s\" }\n\
agent Archiver { goal \"a\" }\n\
agent Mailer { goal \"m\" tools \"http-post\" permissions network=true }\n\
edge from=\"Fetcher\" to=\"Summarizer\"\nedge from=\"Summarizer\" to=\"Mailer\"\n\
deliverable kind=\"email\" { check \"c\" }";
        let bp = parse::blueprint(src).unwrap();
        let never = never_cached(&bp);
        assert!(never.contains_key("Fetcher"));
        assert!(never.contains_key("Summarizer"), "downstream of fresh data is fresh");
        assert!(never.contains_key("Mailer"));
        assert!(!never.contains_key("Archiver"), "independent pure agent stays cacheable");
    }

    #[test]
    fn model_tier_swap_refires_that_tier() {
        let old = hn();
        let mut new = old.clone();
        new.models.quality.prefer = "another-model".into();
        let plan = refire_plan(&diff(&old, &new), &new);
        assert_eq!(plan.refire, names(&["Summarizer", "Mailer"]));
    }

    #[test]
    fn removing_an_upstream_agent_refires_everything_downstream() {
        // Raised in an external review: Fetcher -> Summarizer -> Mailer,
        // remove Fetcher (and its edge). Summarizer lost an input, so it
        // and Mailer must refire.
        let old = hn();
        let mut new = old.clone();
        new.agents.retain(|a| a.name != "Fetcher");
        new.edges.retain(|e| e.from != "Fetcher");
        let d = diff(&old, &new);
        assert!(d.agent_removed.contains("Fetcher"));
        let plan = refire_plan(&d, &new);
        assert_eq!(plan.refire, names(&["Summarizer", "Mailer"]));
        assert!(plan.cached.is_empty());
    }

    #[test]
    fn removing_a_middle_edge_refires_its_former_target() {
        let old = hn();
        let mut new = old.clone();
        new.edges.retain(|e| !(e.from == "Summarizer" && e.to == "Mailer"));
        let plan = refire_plan(&diff(&old, &new), &new);
        assert_eq!(plan.refire, names(&["Mailer"]));
    }
}
