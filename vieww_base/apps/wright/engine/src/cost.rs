//! The spending promise (the memo's item 3): the number BEFORE the run.
//!
//! Budgets existed per run and per day before this; what was missing is
//! the plain sentence a person decides on: "about $0.04 — the built-in
//! model would be free but slower". This module computes the estimate
//! from the plan itself (tier weights, prompt sizes, prices), never
//! from a config file a person did not read, and turns it into the two
//! choices the CLI actually offers:
//!
//! 1. run as planned;
//! 2. run on the built-in model — free, on this machine, slower.
//!
//! The estimate is honest about what it cannot know: model answers vary,
//! so the number is a floor-plus-shape, printed with "about", and the
//! guardrails stay the hard ceiling regardless of what was promised.

use wright_core::blueprint::Blueprint;
use wright_core::compiler::TaskGraph;
use wright_runtime::config::Config;

/// The promise shown before a run.
#[derive(Debug, Clone, PartialEq)]
pub struct CostPlan {
    /// Micro-USD, the honest floor-plus-shape estimate for the plan.
    pub est_micro_usd: u64,
    /// True when the plan can run entirely on the built-in model.
    pub builtin_possible: bool,
    /// The sentence shown to the person (what the CLI prints).
    pub sentence: String,
}

/// Rough tokens for one prompt: the goal, the rules and the inputs'
/// shapes. Deliberately crude and stated as such — the estimate is a
/// promise to check the guardrails, not a quote.
const PROMPT_TOKENS: u64 = 900;

/// Estimate the plan's cost from its compiled shape.
pub fn estimate(bp: &Blueprint, graph: &TaskGraph, cfg: &Config) -> CostPlan {
    let mut total: u64 = 0;
    let mut model_tasks = 0u64;
    for t in &graph.tasks {
        let Some(m) = &t.model else { continue };
        let price = cfg.price_of(&m.primary);
        // Each model task: its prompt (crudely tokened) plus a
        // middling answer (512 tokens), at the tier's price.
        let task_cost = price.cost_micro(PROMPT_TOKENS, 0, 512);
        let _ = t.kind;
        total = total.saturating_add(task_cost);
        model_tasks += 1;
    }
    let builtin_possible = cfg.falls_back_to_builtin() && wright_runtime::local::available();
    let checks_line = if bp.chips.tests {
        "with Testwright and Reviewer checking the work before anything leaves"
    } else {
        "without the tests chip — nothing checks the work before it ships"
    };
    let sentence = if model_tasks == 0 {
        format!("nothing here calls a model; the checks cost nothing. {checks_line}")
    } else if total == 0 {
        format!(
            "about ${:.4} for {} model step(s) — every model on this plan is free (mock or built-in). {checks_line}",
            0.0,
            model_tasks
        )
    } else {
        let builtin_note = if builtin_possible {
            " The built-in model would be free but slower; you can choose it."
        } else {
            ""
        };
        format!(
            "about ${:.4} for {} model step(s) (a floor, not a quote — the guardrails stay the ceiling).{builtin_note} {checks_line}",
            total as f64 / 1e6,
            model_tasks
        )
    };
    CostPlan {
        est_micro_usd: total,
        builtin_possible,
        sentence,
    }
}

/// The two choices `wright run` offers, as (key, label) pairs for the
/// CLI. Kept here so the wording is tested with the math.
pub fn choices(plan: &CostPlan) -> Vec<(String, String)> {
    let mut out = vec![("1".to_string(), "run as planned".to_string())];
    if plan.builtin_possible && plan.est_micro_usd > 0 {
        out.push((
            "2".to_string(),
            "run on the built-in model instead — free, on this machine, slower".to_string(),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use wright_core::blueprint::parse;
    use wright_core::compiler::{SystemKind, TaskKind};

    const HEAD: &str = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\n";

    fn graph_for(src: &str) -> (Blueprint, TaskGraph) {
        let bp = parse::blueprint(src).unwrap();
        let g = wright_core::compiler::compile(&bp).unwrap();
        (bp, g)
    }

    #[test]
    fn a_priced_plan_estimates_from_tier_weights() {
        let (bp, g) = graph_for(&format!(
            "{HEAD}agent A {{ goal \"one\" }}\nagent B {{ goal \"two\" }}\ndeliverable kind=\"k\" {{ check \"c\" }}"
        ));
        // default cheap tier = deepseek-v4-flash at $0.30/M in, $1.20/M out
        let cfg = Config::builtin();
        let plan = estimate(&bp, &g, &cfg);
        assert!(
            plan.est_micro_usd > 0,
            "a priced model plan must estimate above zero"
        );
        assert!(plan.sentence.contains("about $"), "{}", plan.sentence);
        assert!(plan.sentence.contains("model step(s)"), "{}", plan.sentence);
        assert!(
            plan.sentence.contains("guardrails stay the ceiling"),
            "{}",
            plan.sentence
        );
    }

    #[test]
    fn mock_and_builtin_plans_read_as_free() {
        let (bp, g) = graph_for(&format!(
            "{HEAD}models {{ tier cheap prefer=\"mock-1\" }}\nagent A {{ goal \"one\" }}\ndeliverable kind=\"k\" {{ check \"c\" }}"
        ));
        let mut cfg = Config::builtin();
        // a local mock provider serving mock-*: price 0
        cfg.prices.push((
            "mock-*".to_string(),
            wright_runtime::config::Price {
                input: 0.0,
                output: 0.0,
                cached_input: 0.0,
            },
        ));
        let plan = estimate(&bp, &g, &cfg);
        assert_eq!(plan.est_micro_usd, 0);
        assert!(plan.sentence.contains("free"), "{}", plan.sentence);
        assert!(
            plan.sentence.contains("checks the work"),
            "{}",
            plan.sentence
        );
    }

    #[test]
    fn worker_and_builtin_steps_cost_nothing_to_estimate() {
        // worker steps never call models; Shipwright is free; the
        // estimate counts model tasks only.
        let (bp, g) = graph_for(&format!(
            "{HEAD}agent W {{ goal \"code it\" worker \"claude-code\" out \"w.txt\" }}\ndeliverable kind=\"k\" {{ check \"c\" }}"
        ));
        let cfg = Config::builtin();
        let plan = estimate(&bp, &g, &cfg);
        assert_eq!(
            plan.est_micro_usd, 0,
            "the worker's own account pays for its model, not this plan"
        );
        assert!(
            plan.sentence.contains("nothing here calls a model"),
            "{}",
            plan.sentence
        );
    }

    #[test]
    fn the_choice_list_offers_builtin_only_when_it_helps() {
        let (bp, g) = graph_for(&format!(
            "{HEAD}agent A {{ goal \"one\" }}\ndeliverable kind=\"k\" {{ check \"c\" }}"
        ));
        let cfg = Config::builtin();
        let plan = estimate(&bp, &g, &cfg);
        let ch = choices(&plan);
        assert_eq!(ch[0].1, "run as planned");
        // the built-in model only exists when the engine can load it
        if plan.builtin_possible {
            assert_eq!(
                ch.len(),
                2,
                "a paid plan with a working builtin offers both"
            );
            assert!(ch[1].1.contains("free"), "{}", ch[1].1);
        } else {
            assert_eq!(ch.len(), 1);
        }
        let _ = SystemKind::Shipwright;
        let _ = TaskKind::System(SystemKind::Operator);
    }
}
