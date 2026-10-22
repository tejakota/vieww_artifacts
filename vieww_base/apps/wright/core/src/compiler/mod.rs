//! The compiler: blueprint -> task graph.
//!
//! This is where the two-tier workforce trick happens. The user's
//! project agents arrive in the blueprint; the compiler wraps them in
//! the system agents each capability chip requires.
//!
//! The firing order, and why (read this before changing anything):
//!
//! ```text
//! stage 0   Shipwright (built-in)       lays the project skeleton — no model, milliseconds
//! stage 1+  Logwright, Firewright       set up logging / CI inside the skeleton (chips)
//!           project agents              the user's graph, in dependency order
//!           Testwright                  writes + runs the checks over everything produced
//!           Reviewer                    reads it all; can refuse delivery
//!           delivery agents             anything that touches the world (http-post ...)
//! last      Operator (built-in)         registers the schedule, writes the run receipt
//! ```
//!
//! The one rule that matters most: **nothing touches the world before
//! the checks pass.** A delivery agent (one with a World-effect tool)
//! depends on the Reviewer — always: a plan that delivers, or that makes
//! acceptance promises, gets Testwright and the Reviewer whether or not
//! its tests chip is on — so a bad digest is caught before it is
//! emailed, not reported after. (v0.1 had this backwards: tests ran
//! after the Mailer had already sent.)
//!
//! Built-in tasks (Shipwright, Operator) are deterministic code in the
//! engine, not model calls: they are free, instant, and identical every
//! time — which is what "project setup disappears" should mean.

use std::collections::BTreeMap;

use super::blueprint::model::*;
use super::blueprint::validate;
use super::palette::Legs;
use super::rule_of_two;

/// The system-agent workforce. Prompts live in the `wright-agents`
/// crate; this enum is the shared vocabulary between core and runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SystemKind {
    Interviewer,
    Architect,
    Shipwright,
    Testwright,
    Logwright,
    Reviewer,
    Firewright,
    Operator,
    Explainer,
}

impl SystemKind {
    pub const ALL: [SystemKind; 9] = [
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

    pub fn name(&self) -> &'static str {
        match self {
            SystemKind::Interviewer => "Interviewer",
            SystemKind::Architect => "Architect",
            SystemKind::Shipwright => "Shipwright",
            SystemKind::Testwright => "Testwright",
            SystemKind::Logwright => "Logwright",
            SystemKind::Reviewer => "Reviewer",
            SystemKind::Firewright => "Firewright",
            SystemKind::Operator => "Operator",
            SystemKind::Explainer => "Explainer",
        }
    }

    pub fn from_name(name: &str) -> Option<SystemKind> {
        SystemKind::ALL.iter().copied().find(|k| k.name() == name)
    }

    /// Built-in kinds run as deterministic engine code, not a model.
    pub fn is_builtin(&self) -> bool {
        matches!(self, SystemKind::Shipwright | SystemKind::Operator)
    }
}

/// What a task is: a project agent, or an injected system agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskKind {
    Project(String),
    System(SystemKind),
}

impl TaskKind {
    pub fn display_name(&self) -> String {
        match self {
            TaskKind::Project(name) => name.clone(),
            TaskKind::System(kind) => kind.name().to_string(),
        }
    }
}

/// A model resolved from a tier: primary and fallback, both strings.
/// Vendor-swap = edit the tier table, never the agents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModel {
    pub tier: ModelTier,
    pub primary: String,
    pub fallback: String,
}

/// One unit of scheduled work.
#[derive(Debug, Clone)]
pub struct Task {
    pub id: usize,
    pub kind: TaskKind,
    /// Indices of tasks that must complete first.
    pub deps: Vec<usize>,
    /// None for built-in tasks and worker steps (they never call a model).
    pub model: Option<ResolvedModel>,
    pub permissions: PermissionSet,
    /// Tool allowlist. The gate refuses any tool not on this list.
    pub tools: Vec<String>,
    pub approval: ApprovalPolicy,
    /// Retries on failure. Always 0 for delivery tasks.
    pub retries: u8,
    /// Soft budget for this task (its weighted share of the firing).
    pub budget_micro_usd: u64,
    /// Touches the world: scheduled after the checks, never cached.
    pub delivery: bool,
    /// Reads the outside world: never cached.
    pub volatile: bool,
    /// The Rule of Two legs this task stands on (ADR-024): its tools'
    /// legs plus legs inherited from untrusted artifacts it reads.
    pub legs: Legs,
    /// A worker step (ADR-026): the engine hands this to an external
    /// agent binary in a sandboxed copy; no model call is made.
    pub worker: Option<String>,
    /// The files a worker may change; empty = the whole project.
    pub worker_files: Vec<String>,
    /// Wall-clock seconds for a worker run.
    pub worker_timeout_secs: u64,
}

impl Task {
    pub fn name(&self) -> String {
        self.kind.display_name()
    }

    pub fn is_builtin(&self) -> bool {
        matches!(self.kind, TaskKind::System(k) if k.is_builtin())
    }
}

/// The compiled firing plan.
#[derive(Debug, Clone, Default)]
pub struct TaskGraph {
    pub tasks: Vec<Task>,
    /// Stages by dependency depth: stage 0 runs first; tasks inside one
    /// stage are independent and run concurrently.
    pub stages: Vec<Vec<usize>>,
}

impl TaskGraph {
    pub fn by_name(&self, name: &str) -> Option<&Task> {
        self.tasks.iter().find(|t| t.name() == name)
    }

    /// The widest stage — how many tasks can actually run at once.
    pub fn max_parallelism(&self) -> usize {
        self.stages.iter().map(|s| s.len()).max().unwrap_or(0)
    }
}

/// Relative cost weight of a tier when splitting the firing budget.
fn tier_weight(tier: ModelTier) -> u64 {
    match tier {
        ModelTier::Quality => 4,
        ModelTier::Cheap | ModelTier::Auto => 1,
    }
}

fn tools(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// Compile a validated blueprint into a task graph.
///
/// Returns an error string on structural failure; `validate` should
/// have caught all of these first. Defense in depth.
pub fn compile(bp: &Blueprint) -> Result<TaskGraph, String> {
    let order = validate::topo_order(bp).map_err(|c| format!("cycle in blueprint: {c}"))?;

    let mut tasks: Vec<Task> = Vec::new();
    let cheap = ResolvedModel {
        tier: ModelTier::Cheap,
        primary: bp.models.cheap.prefer.clone(),
        fallback: bp.models.cheap.fallback.clone(),
    };
    let rw = PermissionSet::none()
        .with(PermissionSet::READ_FILES)
        .with(PermissionSet::WRITE_FILES);

    let push = |tasks: &mut Vec<Task>,
                kind: TaskKind,
                model: Option<ResolvedModel>,
                perms: PermissionSet,
                tl: Vec<String>|
     -> usize {
        let id = tasks.len();
        tasks.push(Task {
            id,
            kind,
            deps: Vec::new(),
            model,
            permissions: perms,
            tools: tl,
            approval: ApprovalPolicy::None,
            retries: 1,
            budget_micro_usd: 0,
            delivery: false,
            volatile: false,
            legs: Legs::none(),
            worker: None,
            worker_files: Vec::new(),
            worker_timeout_secs: 0,
        });
        id
    };

    // Stage 0: the skeleton. Built-in, deterministic, free.
    let shipwright = push(
        &mut tasks,
        TaskKind::System(SystemKind::Shipwright),
        None,
        rw,
        Vec::new(),
    );

    // Setup agents from chips: they shape the skeleton, so they run
    // right after it and before project work.
    let mut setup: Vec<usize> = Vec::new();
    if bp.chips.logging {
        let id = push(
            &mut tasks,
            TaskKind::System(SystemKind::Logwright),
            Some(cheap.clone()),
            rw,
            tools(&["files-read", "files-list", "files-write", "edit"]),
        );
        tasks[id].deps.push(shipwright);
        setup.push(id);
    }
    if bp.chips.ci || bp.chips.webhooks {
        let id = push(
            &mut tasks,
            TaskKind::System(SystemKind::Firewright),
            Some(cheap.clone()),
            rw,
            tools(&["files-read", "files-list", "files-write", "edit"]),
        );
        tasks[id].deps.push(shipwright);
        setup.push(id);
    }

    // Project agents in topological order.
    let mut name_to_id: BTreeMap<String, usize> = BTreeMap::new();
    for name in &order {
        let agent = bp
            .agent(name)
            .ok_or_else(|| format!("agent '{name}' vanished mid-compile"))?;
        let pref = bp.models.tier(agent.model_tier);
        let mut tl = agent.tools.clone();
        if !agent.outputs.is_empty() && !tl.iter().any(|t| t == "write-output") {
            tl.push("write-output".into());
        }
        // Worker steps never call a model; the engine runs the `worker`
        // tool directly, so they cost no model budget at all.
        let model = if agent.is_worker_step() {
            None
        } else {
            Some(ResolvedModel {
                tier: agent.model_tier,
                primary: pref.prefer,
                fallback: pref.fallback,
            })
        };
        let id = push(
            &mut tasks,
            TaskKind::Project(name.clone()),
            model,
            agent.permissions,
            tl,
        );
        let t = &mut tasks[id];
        t.approval = agent.approval;
        t.delivery = agent.is_delivery();
        // Worker steps are never cached either: their result is an
        // external agent's answer, not a deterministic function of the
        // workspace bytes (ADR-026).
        t.volatile = agent.is_volatile() || agent.is_worker_step();
        t.legs = rule_of_two::legs_of_agent(bp, name);
        t.worker = agent.worker.clone();
        t.worker_files = agent.worker_files.clone();
        t.worker_timeout_secs = agent.worker_timeout_secs;
        // Side effects are never retried: a retry is a double-send.
        t.retries = if t.delivery { 0 } else { agent.retries };
        t.deps.push(shipwright);
        t.deps.extend(setup.iter().copied());
        name_to_id.insert(name.clone(), id);
    }

    // Wire edges as dependencies.
    for e in &bp.edges {
        if let (Some(&f), Some(&t)) = (name_to_id.get(&e.from), name_to_id.get(&e.to)) {
            tasks[t].deps.push(f);
        }
    }

    // The checks: over everything produced by non-delivery agents.
    let producers: Vec<usize> = name_to_id
        .values()
        .copied()
        .filter(|&id| !tasks[id].delivery)
        .collect();
    let mut gate_for_delivery: Option<usize> = None;
    // The checks run when the tests chip asks for them — AND whenever the
    // plan makes a promise only they can keep: an acceptance sentence
    // (the Confirm Gate tells the person "Testwright must check this"),
    // or a delivery agent (nothing leaves the machine before the checks,
    // AGENTS.md rule 7). Without this, a plan with `accept` lines and the
    // chip off — the gallery's own recipes — promised a check that never
    // ran, and its Mailer sent unreviewed work.
    let promises = bp.deliverables.iter().any(|d| !d.accept.is_empty());
    let delivers = name_to_id.values().any(|&id| tasks[id].delivery);
    if bp.chips.tests || promises || delivers {
        let tw = push(
            &mut tasks,
            TaskKind::System(SystemKind::Testwright),
            Some(cheap.clone()),
            rw.with(PermissionSet::SPAWN),
            tools(&[
                "files-read",
                "files-list",
                "files-write",
                "edit",
                "run-tests",
            ]),
        );
        tasks[tw].deps.push(shipwright);
        tasks[tw].deps.extend(setup.iter().copied());
        tasks[tw].deps.extend(producers.iter().copied());

        let rv = push(
            &mut tasks,
            TaskKind::System(SystemKind::Reviewer),
            Some(cheap.clone()),
            PermissionSet::none().with(PermissionSet::READ_FILES),
            tools(&["files-read", "files-list"]),
        );
        tasks[rv].deps.push(tw);
        gate_for_delivery = Some(rv);
    }

    // Delivery agents wait for the Reviewer's verdict.
    if let Some(rv) = gate_for_delivery {
        for &id in name_to_id.values() {
            if tasks[id].delivery {
                tasks[id].deps.push(rv);
            }
        }
    }

    // The Operator closes every firing that has a schedule or secrets:
    // registration and the run receipt. Built-in, so it costs nothing.
    if bp.chips.scheduling.is_some() || !bp.chips.secrets.is_empty() {
        let op = push(
            &mut tasks,
            TaskKind::System(SystemKind::Operator),
            None,
            PermissionSet::none(),
            Vec::new(),
        );
        let all_before: Vec<usize> = (0..op).collect();
        tasks[op].deps = all_before;
    }

    for t in &mut tasks {
        t.deps.sort_unstable();
        t.deps.dedup();
    }

    // Budget split by tier weight. Built-ins cost nothing and get nothing.
    let weights: Vec<u64> = tasks
        .iter()
        .map(|t| t.model.as_ref().map(|m| tier_weight(m.tier)).unwrap_or(0))
        .collect();
    let total: u64 = weights.iter().sum::<u64>().max(1);
    for (t, w) in tasks.iter_mut().zip(&weights) {
        t.budget_micro_usd = bp.guardrails.per_firing_micro_usd * w / total;
    }

    let stages = stages_of(&tasks)?;
    Ok(TaskGraph { tasks, stages })
}

/// Group tasks into stages by longest-path depth (Kahn's algorithm).
/// Delivery agents depend on the Reviewer, which has a larger id, so
/// ids are NOT a topological order; this does not assume they are.
fn stages_of(tasks: &[Task]) -> Result<Vec<Vec<usize>>, String> {
    let n = tasks.len();
    let mut indegree: Vec<usize> = tasks.iter().map(|t| t.deps.len()).collect();
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); n];
    for t in tasks {
        for &d in &t.deps {
            dependents[d].push(t.id);
        }
    }
    let mut depth = vec![0usize; n];
    let mut ready: Vec<usize> = (0..n).filter(|&i| indegree[i] == 0).collect();
    let mut seen = 0;
    while let Some(i) = ready.pop() {
        seen += 1;
        for &j in &dependents[i] {
            depth[j] = depth[j].max(depth[i] + 1);
            indegree[j] -= 1;
            if indegree[j] == 0 {
                ready.push(j);
            }
        }
    }
    if seen != n {
        return Err("the compiled task graph has a cycle (this is a compiler bug)".into());
    }
    let mut stages: Vec<Vec<usize>> = Vec::new();
    for (id, &d) in depth.iter().enumerate() {
        while stages.len() <= d {
            stages.push(Vec::new());
        }
        stages[d].push(id);
    }
    Ok(stages)
}

#[cfg(test)]
mod tests {
    use super::super::blueprint::parse;
    use super::super::blueprint::validate;
    use super::*;

    /// The HN digest, condensed. logging + tests + scheduling + secrets ON.
    fn hn() -> Blueprint {
        parse::blueprint(&hn_src()).unwrap()
    }

    fn hn_src() -> String {
        let src = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\n\
agent Fetcher { goal \"fetch\" tools \"http-read\" out \"stories.json\" permissions network=true }\n\
agent Summarizer { goal \"sum\" model tier=\"quality\" in \"stories.json\" out \"summaries.json\" }\n\
agent Mailer { goal \"mail\" tools \"http-post\" in \"summaries.json\" approval \"before-send\" permissions network=true }\n\
edge from=\"Fetcher\" to=\"Summarizer\"\n\
edge from=\"Summarizer\" to=\"Mailer\"\n\
chips { scheduling cron=\"0 8 * * *\" secrets smtp=\"keychain:x/y\" logging on tests on }\n\
guardrails { budget per-firing=900000 daily=3000000 max-turns 40 }\n\
deliverable kind=\"email\" { check \"c\" }\n\
models { tier cheap prefer=\"deepseek-v4-flash\" tier quality prefer=\"claude-opus-4.6\" }";
        src.to_string()
    }

    fn id_of(g: &TaskGraph, name: &str) -> usize {
        g.by_name(name)
            .unwrap_or_else(|| panic!("no task {name}"))
            .id
    }

    #[test]
    fn a_plan_that_delivers_is_checked_even_with_the_tests_chip_off() {
        // The gallery's own HN recipe: `accept` sentences, a Mailer, and no
        // `tests on`. Before, it compiled with no Testwright and no Reviewer:
        // the promise was printed and never checked, and the Mailer sent.
        let src = hn_src().replace(" tests on", "");
        let bp = parse::blueprint(&src).unwrap();
        assert!(!bp.chips.tests);
        let g = compile(&bp).unwrap();
        let (rv, mailer) = (id_of(&g, "Reviewer"), id_of(&g, "Mailer"));
        assert!(g.tasks[mailer].deps.contains(&rv), "Mailer waits for the Reviewer");
        assert!(g.by_name("Testwright").is_some());

        // A plan with nothing to deliver and no promise stays as light as
        // the chip says.
        let quiet = parse::blueprint(
            "meta { name \"q\" }\nintent { summary \"s\" confirmed \"v1\" }\n\
agent Writer { goal \"write\" out \"a.md\" }\n\
deliverable kind=\"report\" { check \"c\" }\n",
        )
        .unwrap();
        assert!(compile(&quiet).unwrap().by_name("Reviewer").is_none());
        // ...but a promise brings the checks.
        let promised = parse::blueprint(
            "meta { name \"q\" }\nintent { summary \"s\" confirmed \"v1\" }\n\
agent Writer { goal \"write\" out \"a.md\" }\n\
deliverable kind=\"report\" { check \"c\" accept \"it has a title\" }\n",
        )
        .unwrap();
        assert!(compile(&promised).unwrap().by_name("Reviewer").is_some());
    }

    #[test]
    fn checks_run_before_anything_touches_the_world() {
        let bp = hn();
        assert!(
            validate::validate(&bp).is_ok(),
            "{:?}",
            validate::validate(&bp)
        );
        let g = compile(&bp).unwrap();

        let (tw, rv, mailer) = (
            id_of(&g, "Testwright"),
            id_of(&g, "Reviewer"),
            id_of(&g, "Mailer"),
        );
        assert!(
            g.tasks[mailer].deps.contains(&rv),
            "Mailer waits for the Reviewer"
        );
        assert!(g.tasks[rv].deps.contains(&tw));
        assert!(g.tasks[tw].deps.contains(&id_of(&g, "Summarizer")));
        assert!(
            !g.tasks[tw].deps.contains(&mailer),
            "tests never wait on a send"
        );
        assert!(g.tasks[mailer].delivery);
        assert_eq!(g.tasks[mailer].retries, 0, "side effects are never retried");
    }

    #[test]
    fn builtins_bracket_the_firing() {
        let g = compile(&hn()).unwrap();
        let ship = id_of(&g, "Shipwright");
        assert_eq!(g.stages[0], vec![ship], "the skeleton comes first, alone");
        assert!(g.tasks[ship].model.is_none());
        let op = id_of(&g, "Operator");
        assert_eq!(
            g.tasks[op].deps.len(),
            g.tasks.len() - 1,
            "Operator closes the firing"
        );
        assert_eq!(g.stages.last().unwrap(), &vec![op]);
    }

    #[test]
    fn budget_follows_tier_weight() {
        let g = compile(&hn()).unwrap();
        // model tasks: Logwright 1, Fetcher 1, Summarizer 4, Mailer 1,
        // Testwright 1, Reviewer 1 = 9 weight units over $0.90.
        assert_eq!(g.tasks[id_of(&g, "Summarizer")].budget_micro_usd, 400_000);
        assert_eq!(g.tasks[id_of(&g, "Fetcher")].budget_micro_usd, 100_000);
        assert_eq!(g.tasks[id_of(&g, "Shipwright")].budget_micro_usd, 0);
    }

    #[test]
    fn output_contract_tool_is_granted() {
        let g = compile(&hn()).unwrap();
        assert!(g.tasks[id_of(&g, "Fetcher")]
            .tools
            .contains(&"write-output".to_string()));
        assert!(!g.tasks[id_of(&g, "Mailer")]
            .tools
            .contains(&"write-output".to_string()));
        assert!(g.tasks[id_of(&g, "Fetcher")].volatile);
    }

    #[test]
    fn independent_agents_share_a_stage() {
        let src = "meta { name \"x\" }\nintent { summary \"s\" confirmed \"v1\" }\n\
agent A { goal \"1\" }\nagent B { goal \"2\" }\nagent C { goal \"3\" }\n\
edge from=\"C\" to=\"A\"\n\
deliverable kind=\"k\" { check \"c\" }";
        let g = compile(&parse::blueprint(src).unwrap()).unwrap();
        // stage 0 = Shipwright; B and C together; then A.
        assert_eq!(g.stages[1].len(), 2);
        assert_eq!(g.stages[2].len(), 1);
        assert_eq!(g.max_parallelism(), 2);
    }

    #[test]
    fn model_routing_resolved_per_tier() {
        let g = compile(&hn()).unwrap();
        assert_eq!(
            g.tasks[id_of(&g, "Summarizer")]
                .model
                .as_ref()
                .unwrap()
                .primary,
            "claude-opus-4.6"
        );
        assert_eq!(
            g.tasks[id_of(&g, "Fetcher")]
                .model
                .as_ref()
                .unwrap()
                .primary,
            "deepseek-v4-flash"
        );
    }
}
