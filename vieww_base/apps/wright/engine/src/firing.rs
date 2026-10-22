//! One firing: blueprint in, receipt out.
//!
//! ```text
//! parse + validate + compile the blueprint     (refuse early, in plain words)
//! commit it to the store's history             (every firing names its blueprint)
//! lock the project                             (one firing at a time)
//! budget = min(per-firing, daily cap - spent today)
//! for each stage, in order:
//!     for each task in the stage, in parallel:
//!         a dependency failed?          -> skipped
//!         delivery + Reviewer said no?  -> skipped (nothing leaves the machine)
//!         built-in?                     -> run it (Shipwright / Operator)
//!         cacheable and key known?      -> restore artifacts, free
//!         otherwise                     -> the agent loop, with retries
//! receipt: what ran, what it cost, what it made, the verdict, the log's hash
//! ```
//!
//! Resume: a crashed firing's event log says which tasks finished. `resume`
//! reopens the same run, skips those, and carries on — the log gains a
//! "resumed" event and nothing is ever rewritten.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use wright_core::blueprint::{canon, parse, validate, Blueprint, PermissionSet};
use wright_core::compiler::{self, SystemKind, Task, TaskKind};
use wright_core::convention;
use wright_core::events::{now_ms, Event, EventKind, RunId};
use wright_core::hash;
use wright_core::schedule;
use wright_core::wire::json::{self, Value};
use wright_runtime::cache::{self, CacheEntry, Freshness, KeyInputs};
use wright_runtime::config::Config;
use wright_runtime::executor::{spawn_blocking, Executor, TaskSet};
use wright_runtime::gate::{BudgetMeter, Gate};
use wright_runtime::net::NetConfig;
use wright_runtime::store::{write_atomic, Store};
use wright_tools::ToolCtx;

use crate::agent::FiringEnv;
use crate::approve::{Approver, AutoDeny};
use crate::spine::Spine;

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// How a firing was started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Manual,
    Schedule,
    Resume,
    /// The public benchmark (ADR-030): same firing, measured.
    Bench,
}

impl Trigger {
    pub fn as_str(&self) -> &'static str {
        match self {
            Trigger::Manual => "manual",
            Trigger::Schedule => "schedule",
            Trigger::Resume => "resume",
            Trigger::Bench => "bench",
        }
    }
}

type Listener = Box<dyn Fn(&Event) + Send + Sync>;

/// Options for one firing.
pub struct FireOptions {
    pub trigger: Trigger,
    pub approver: Box<dyn Approver>,
    pub listeners: Vec<Listener>,
    /// Provider config; None loads `~/.wright/config.kdl`.
    pub config: Option<Config>,
    /// Let http tools reach private addresses (local dev, tests).
    pub allow_private_network: bool,
    /// Resume this run instead of starting a new one.
    pub resume: Option<RunId>,
    /// Ignore the cache (re-run everything).
    pub no_cache: bool,
}

impl Default for FireOptions {
    fn default() -> Self {
        FireOptions {
            trigger: Trigger::Manual,
            approver: Box::new(AutoDeny),
            listeners: Vec::new(),
            config: None,
            allow_private_network: false,
            resume: None,
            no_cache: false,
        }
    }
}

/// How one task ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Ran { cost_micro_usd: u64 },
    Cached { saved_micro_usd: u64 },
    Skipped(String),
    Failed(String),
}

impl TaskStatus {
    pub fn ok(&self) -> bool {
        matches!(self, TaskStatus::Ran { .. } | TaskStatus::Cached { .. })
    }

    pub fn label(&self) -> &'static str {
        match self {
            TaskStatus::Ran { .. } => "ran",
            TaskStatus::Cached { .. } => "cached",
            TaskStatus::Skipped(_) => "skipped",
            TaskStatus::Failed(_) => "failed",
        }
    }
}

/// The outcome of a firing.
#[derive(Debug, Clone)]
pub struct RunSummary {
    pub run: RunId,
    pub ok: bool,
    pub headline: String,
    pub cost_micro_usd: u64,
    pub tasks: Vec<(String, TaskStatus)>,
    pub verdict: Option<Result<(), String>>,
    pub receipt: PathBuf,
}

/// Read, parse and validate `blueprint.kdl` from a project directory.
pub fn load_blueprint(project_dir: &Path) -> Result<Blueprint, String> {
    let path = project_dir.join("blueprint.kdl");
    let src = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let bp = parse::blueprint(&src).map_err(|e| format!("{}: {e}", path.display()))?;
    validate::validate(&bp)
        .map_err(|errs| format!("the blueprint is not valid:\n  - {}", errs.join("\n  - ")))?;
    Ok(bp)
}

/// Today's date (UTC) as YYYY-MM-DD, for daily budgets.
pub fn today_utc() -> String {
    let (y, m, d) = schedule::civil_from_days((now_ms() / 1000 / 86400) as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

/// A lock file: one firing per project at a time.
struct ProjectLock(PathBuf);

impl ProjectLock {
    fn take(store: &Store) -> Result<ProjectLock, String> {
        let p = store.root().join("firing.lock");
        for _ in 0..2 {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&p)
            {
                Ok(mut f) => {
                    use std::io::Write;
                    let _ = write!(f, "{}", std::process::id());
                    return Ok(ProjectLock(p));
                }
                Err(_) => {
                    let pid: u32 = std::fs::read_to_string(&p)
                        .ok()
                        .and_then(|t| t.trim().parse().ok())
                        .unwrap_or(0);
                    let alive = pid != 0
                        && (cfg!(not(target_os = "linux"))
                            || Path::new(&format!("/proc/{pid}")).exists());
                    if alive && pid != std::process::id() {
                        return Err(format!("this project is already firing (process {pid}); wait for it, or stop it"));
                    }
                    let _ = std::fs::remove_file(&p); // stale lock from a crash
                }
            }
        }
        Err("could not take the project lock".into())
    }
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn usd(micro: u64) -> String {
    format!("${:.4}", micro as f64 / 1e6)
}

/// Fire the project in `project_dir`.
pub fn fire(project_dir: &Path, opts: FireOptions) -> Result<RunSummary, String> {
    let project_dir = project_dir
        .canonicalize()
        .map_err(|e| format!("{}: {e}", project_dir.display()))?;
    // The kill switch: nothing starts while Wright is stopped.
    if let Some(why) = wright_runtime::killswitch::refusal() {
        return Err(why);
    }
    let bp = load_blueprint(&project_dir)?;
    let graph = compiler::compile(&bp)?;
    let store = Store::open(&project_dir)?;
    let canonical = canon::canonical(&bp);
    let commit =
        store.commit_blueprint(&canonical, &format!("fired ({})", opts.trigger.as_str()))?;
    let _lock = ProjectLock::take(&store)?;

    let config = match opts.config {
        Some(c) => c,
        None => Config::load()?,
    };
    // The organization's policy, when there is one: a plan it does not
    // allow does not fire (and a policy that cannot be trusted stops all).
    let policy = crate::org::enforce(&bp, &config)?;

    // Daily budget: what is left today caps this firing too.
    let day = today_utc();
    let spent_today = store.spent_on(&day);
    let daily_left = bp.guardrails.daily_micro_usd.saturating_sub(spent_today);
    if daily_left == 0 {
        return Err(format!(
            "the daily budget ({}) is already spent today; nothing fired",
            usd(bp.guardrails.daily_micro_usd)
        ));
    }
    let cap = bp.guardrails.per_firing_micro_usd.min(daily_left);

    // A new run, or the one we are resuming.
    let (writer, done) = match opts.resume {
        Some(run) => {
            let w = store.reopen_run(run)?;
            let done = finished_tasks(w.log().events());
            (w, done)
        }
        None => (store.new_run()?, BTreeMap::new()),
    };
    let mut spine = Spine::new(writer);
    for l in opts.listeners {
        spine.listen(l);
    }
    let run = spine.run();

    let mut tools = ToolCtx::new(&project_dir)?;
    tools.test_cmd = convention::test_cmd(&bp);
    tools.secrets = bp.chips.secrets.clone();
    tools.allowed_hosts = policy.as_ref().and_then(|p| p.allow_hosts.clone());
    tools.allow_private_network = opts.allow_private_network || tools.allow_private_network;
    if bp.project.language == "rust" {
        let cargo_home = std::env::var("CARGO_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cargo")
            });
        tools.sandbox_extra_rw.push(cargo_home);
    }

    let mut budget = BudgetMeter::new(cap);
    // On resume, what the first attempt spent still counts.
    for e in spine.events() {
        if e.kind == EventKind::TokensRecorded {
            budget.record(
                e.data_value()
                    .and_then(|v| v.get("cost_micro_usd").and_then(|c| c.as_u64()))
                    .unwrap_or(0),
            );
        }
    }

    let env = Arc::new(FiringEnv {
        gate: Gate::from_graph(&graph).with_identities(
            std::sync::Arc::new(wright_runtime::identity::Registry::new(
                wright_runtime::signing::SigningKey::resolve()
                    .map_err(|e| format!("cannot issue agent identities without a signing key: {e}"))?,
            )),
            graph
                .tasks
                .iter()
                .map(|t| {
                    let scope = match &t.kind {
                        TaskKind::Project(n) => bp.agent(n).map(|a| wright_core::blueprint::secrets_for(&bp, a)).unwrap_or_default(),
                        TaskKind::System(_) => Vec::new(),
                    };
                    (t.name(), scope)
                })
                .collect(),
        ),
        budget: Mutex::new(budget),
        turns_left: AtomicI64::new(bp.guardrails.max_turns as i64),
        spine,
        config,
        net: NetConfig::from_env(),
        store: store.clone(),
        tools,
        approver: opts.approver,
        approval_seq: AtomicU32::new(0),
        deadline: Instant::now() + Duration::from_secs(bp.guardrails.wall_clock_secs.max(1)),
        cancel: AtomicBool::new(false),
        reports: Mutex::new(BTreeMap::new()),
        written: Mutex::new(BTreeMap::new()),
        accepts: Mutex::new(BTreeMap::new()),
        bp: bp.clone(),
        graph: graph.clone(),
    });

    let started_ms = now_ms();
    env.spine.emit(
        EventKind::RunStarted,
        None,
        match opts.resume {
            Some(_) => format!("resumed run {run} ({} task(s) already done)", done.len()),
            None => format!(
                "firing \"{}\" ({}), budget {}",
                bp.meta.name,
                opts.trigger.as_str(),
                usd(cap)
            ),
        },
        json::to_string(&Value::obj(vec![
            ("blueprint", Value::str(commit.clone())),
            ("trigger", Value::str(opts.trigger.as_str())),
            ("budget_micro_usd", Value::int(cap as i64)),
            ("app", Value::str(APP_VERSION)),
        ])),
    )?;

    // Secrets are checked up front: a missing key fails now, not at 3AM
    // halfway through a firing.
    for s in &bp.chips.secrets {
        if let Err(e) = wright_runtime::secrets::resolve(&s.reference) {
            env.spine.note(
                EventKind::Note,
                None,
                format!("warning: secret '{}' is not available: {e}", s.name),
            );
        }
    }

    let status: Arc<Mutex<BTreeMap<usize, TaskStatus>>> = Arc::new(Mutex::new(BTreeMap::new()));
    let verdict: Arc<Mutex<Option<Result<(), String>>>> = Arc::new(Mutex::new(None));
    let no_cache = opts.no_cache;
    let ex = Executor::new();

    for stage in &graph.stages {
        let mut set = TaskSet::new();
        for &id in stage {
            let task = graph.tasks[id].clone();
            let env = Arc::clone(&env);
            let status = Arc::clone(&status);
            let verdict = Arc::clone(&verdict);
            let already = done.get(&task.name()).cloned();
            set.spawn(&ex, async move {
                let st = Arc::clone(&status);
                let v = Arc::clone(&verdict);
                let outcome =
                    spawn_blocking(move || run_task(&env, &task, &st, &v, already, no_cache)).await;
                let (id_, s) = match outcome {
                    Ok(pair) => pair,
                    Err(panic) => (id, TaskStatus::Failed(format!("crashed: {panic}"))),
                };
                status
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .insert(id_, s);
            });
        }
        set.block_until_all_done(&ex);
    }

    // ── the receipt ──────────────────────────────────────────────
    let status = status.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let verdict = verdict.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let tasks: Vec<(String, TaskStatus)> = graph
        .tasks
        .iter()
        .map(|t| {
            (
                t.name(),
                status
                    .get(&t.id)
                    .cloned()
                    .unwrap_or(TaskStatus::Skipped("never scheduled".into())),
            )
        })
        .collect();
    let cost: u64 = env
        .budget
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .spent_micro_usd;
    let failed: Vec<&(String, TaskStatus)> = tasks
        .iter()
        .filter(|(_, s)| matches!(s, TaskStatus::Failed(_)))
        .collect();
    let rejected = matches!(verdict, Some(Err(_)));
    let ok = failed.is_empty() && !rejected;
    let ran = tasks
        .iter()
        .filter(|(_, s)| matches!(s, TaskStatus::Ran { .. }))
        .count();
    let cached = tasks
        .iter()
        .filter(|(_, s)| matches!(s, TaskStatus::Cached { .. }))
        .count();
    let saved: u64 = tasks
        .iter()
        .filter_map(|(_, s)| {
            if let TaskStatus::Cached { saved_micro_usd } = s {
                Some(*saved_micro_usd)
            } else {
                None
            }
        })
        .sum();
    let headline = if ok {
        format!(
            "done: {ran} ran, {cached} cached (saved {}), cost {}",
            usd(saved),
            usd(cost)
        )
    } else if rejected {
        format!(
            "stopped: the Reviewer rejected the work — {} — nothing was delivered",
            verdict
                .as_ref()
                .and_then(|v| v.clone().err())
                .unwrap_or_default()
        )
    } else {
        format!(
            "failed: {}",
            failed
                .iter()
                .map(|(n, s)| format!(
                    "{n} ({})",
                    if let TaskStatus::Failed(r) = s {
                        r.as_str()
                    } else {
                        ""
                    }
                ))
                .collect::<Vec<_>>()
                .join("; ")
        )
    };

    let mut artifacts: Vec<(String, String)> = Vec::new();
    for a in &bp.agents {
        for o in &a.outputs {
            if let Ok(bytes) = std::fs::read(project_dir.join("artifacts").join(o)) {
                artifacts.push((o.clone(), store.put_object(&bytes).unwrap_or_default()));
            }
        }
    }
    env.spine.emit(
        if ok {
            EventKind::RunFinished
        } else {
            EventKind::RunFailed
        },
        None,
        headline.clone(),
        json::to_string(&Value::obj(vec![
            ("cost_micro_usd", Value::int(cost as i64)),
            ("saved_micro_usd", Value::int(saved as i64)),
        ])),
    )?;
    store.record_spend(&day, run, cost)?;

    let events_path = store.run_dir(run).join("events.jsonl");
    let events_hash = std::fs::read(&events_path)
        .map(|b| hash::blake3_hex(&b))
        .unwrap_or_default();
    // Receipt v2 (ADR-025): the inputs the plan consumed, with their
    // hashes, so a verifier can name what the work was based on; the
    // Rule of Two agents, so the risky ones are visible; the acceptance
    // sentences with their verdicts; and an Ed25519 signature over the
    // exact receipt bytes, so none of the above can be edited quietly.
    let mut input_set: BTreeMap<String, String> = BTreeMap::new();
    for a in &bp.agents {
        for i in &a.inputs {
            if let Ok(bytes) = std::fs::read(project_dir.join("artifacts").join(i)) {
                input_set.insert(i.clone(), store.put_object(&bytes).unwrap_or_default());
            }
        }
    }
    let rule_of_two_agents: Vec<String> = bp
        .agents
        .iter()
        .filter(|a| wright_core::rule_of_two::legs_of_agent(&bp, &a.name).is_three_legged())
        .map(|a| a.name.clone())
        .collect();
    let accepts_state = env
        .accepts
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    let mut accept_records: Vec<Value> = Vec::new();
    {
        // Numbered sentences in blueprint order; the state is filled by
        // the Testwright's ACCEPT lines (missing = never checked).
        let mut n = 0usize;
        for d in &bp.deliverables {
            for sentence in &d.accept {
                n += 1;
                let ok = accepts_state.get(&n).copied();
                accept_records.push(Value::obj(vec![
                    ("n", Value::int(n as i64)),
                    ("sentence", Value::str(sentence.clone())),
                    (
                        "ok",
                        match ok {
                            Some(true) => Value::Bool(true),
                            Some(false) => Value::Bool(false),
                            None => Value::Null,
                        },
                    ),
                ]));
            }
        }
    }
    let mut receipt = Value::obj(vec![
        ("format", Value::str("wright.run-receipt/2")),
        ("run", Value::int(run as i64)),
        ("project", Value::str(bp.meta.name.clone())),
        ("project_dir", Value::str(project_dir.display().to_string())),
        ("blueprint", Value::str(commit)),
        ("trigger", Value::str(opts.trigger.as_str())),
        ("started_ms", Value::int(started_ms as i64)),
        ("finished_ms", Value::int(now_ms() as i64)),
        ("ok", Value::Bool(ok)),
        ("headline", Value::str(headline.clone())),
        ("cost_micro_usd", Value::int(cost as i64)),
        ("saved_micro_usd", Value::int(saved as i64)),
        (
            "inputs",
            Value::Arr(
                input_set
                    .iter()
                    .map(|(k, h)| {
                        Value::obj(vec![
                            ("name", Value::str(k.clone())),
                            ("blake3", Value::str(h.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "rule_of_two",
            Value::Arr(
                rule_of_two_agents
                    .iter()
                    .map(|a| Value::str(a.clone()))
                    .collect(),
            ),
        ),
        ("accepts", Value::Arr(accept_records)),
        (
            "tasks",
            Value::Arr(
                tasks
                    .iter()
                    .map(|(n, s)| {
                        let detail = match s {
                            TaskStatus::Ran { cost_micro_usd } => {
                                Value::int(*cost_micro_usd as i64)
                            }
                            TaskStatus::Cached { saved_micro_usd } => {
                                Value::int(*saved_micro_usd as i64)
                            }
                            TaskStatus::Skipped(r) | TaskStatus::Failed(r) => Value::str(r.clone()),
                        };
                        Value::obj(vec![
                            ("name", Value::str(n.clone())),
                            ("status", Value::str(s.label())),
                            ("detail", detail),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "artifacts",
            Value::Arr(
                artifacts
                    .iter()
                    .map(|(n, h)| {
                        Value::obj(vec![
                            ("name", Value::str(n.clone())),
                            ("blake3", Value::str(h.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "verdict",
            match &verdict {
                None => Value::Null,
                Some(Ok(())) => Value::str("approve"),
                Some(Err(r)) => Value::str(format!("reject: {r}")),
            },
        ),
        // Where the money went, per agent, model and tool — from the
        // event log, signed with the rest (the receipt's `spend`).
        ("spend", crate::spend::attribute(&env.spine.events()).to_value()),
        // Every task-scoped identity: delegated by this machine's key,
        // attested by itself over its calls, revoked at its task's end.
        (
            "identities",
            Value::Arr(
                env.gate
                    .identities()
                    .map(|r| r.close_all().iter().map(|c| c.to_value()).collect())
                    .unwrap_or_default(),
            ),
        ),
        ("events_blake3", Value::str(events_hash)),
        ("wright", Value::str(APP_VERSION)),
    ]);
    // Sign the exact bytes of the receipt as it stands (no signature
    // field yet), then append the signature block. The JSON writer is
    // insertion-ordered, so a verifier can re-parse, strip the field
    // and reproduce these bytes exactly.
    let receipt_path = store.run_dir(run).join("receipt.json");
    let mut credential: Option<Value> = None;
    if let Ok(key) = wright_runtime::signing::SigningKey::resolve() {
        let bytes = json::to_string(&receipt);
        let sig = key.sign(bytes.as_bytes());
        let hex_sig: String = sig.iter().map(|b| format!("{b:02x}")).collect();
        receipt.set(
            "signature",
            Value::obj(vec![
                ("public_key", Value::str(key.public_hex())),
                ("provenance", Value::str(key.provenance_str())),
                ("value", Value::str(hex_sig)),
            ]),
        );
        // The same facts as a standard credential (eddsa-jcs-2022), which
        // anyone can check without Wright.
        credential = crate::credential::issue(&receipt, &key).ok();
    }
    write_atomic(&receipt_path, json::to_string(&receipt).as_bytes())?;
    if let Some(vc) = &credential {
        write_atomic(&store.run_dir(run).join("receipt.vc.json"), json::to_string(vc).as_bytes())?;
    }

    Ok(RunSummary {
        run,
        ok,
        headline,
        cost_micro_usd: cost,
        tasks,
        verdict,
        receipt: receipt_path,
    })
}

/// Tasks a previous attempt finished, with their final report text.
fn finished_tasks(events: &[Event]) -> BTreeMap<String, String> {
    let mut done = BTreeMap::new();
    for e in events {
        if matches!(e.kind, EventKind::AgentFinished | EventKind::CacheHit) {
            if let Some(a) = &e.agent {
                let report = e
                    .data_value()
                    .and_then(|v| v.str_of("report").map(String::from))
                    .unwrap_or_default();
                done.insert(a.clone(), report);
            }
        }
    }
    done
}

fn artifact_hash(env: &FiringEnv, name: &str) -> String {
    std::fs::read(env.tools.artifacts_dir().join(name))
        .map(|b| hash::blake3_hex(&b))
        .unwrap_or_else(|_| "missing".into())
}

/// The cache key for a task, or None if it may never be cached.
fn key_for(env: &FiringEnv, task: &Task) -> Option<String> {
    if !Freshness::of(task).cacheable() || task.is_builtin() {
        return None;
    }
    let bp = &env.bp;
    match &task.kind {
        TaskKind::Project(name) => {
            let agent = bp.agent(name)?;
            let inputs: Vec<String> = agent.inputs.iter().map(|i| artifact_hash(env, i)).collect();
            let ws = if agent.reads_workspace() {
                Some(env.store.workspace_hash())
            } else {
                None
            };
            // The effective prompt = the agent's canonical card + the
            // template version + inference settings. All of it is keyed.
            let def = format!(
                "{}prompt-template=v{}\nmax-tokens={}\n",
                canon::agent_canon(agent),
                wright_agents::PROMPT_VERSION,
                crate::agent::MAX_TOKENS
            );
            Some(cache::cache_key(&KeyInputs {
                definition: &def,
                resolved: task.model.as_ref(),
                tools: &task.tools,
                permission_bits: task.permissions.bits(),
                input_hashes: &inputs,
                workspace_hash: ws.as_deref(),
                app_version: APP_VERSION,
            }))
        }
        TaskKind::System(kind) => {
            let spec = wright_agents::spec(*kind);
            let setup = matches!(kind, SystemKind::Logwright | SystemKind::Firewright);
            // Setup agents depend on the project shape only; checkers on
            // everything produced (all artifacts + the workspace).
            let mut def = format!(
                "{}\nprompt-template=v{}\nmax-tokens={}\n{:?}\n{:?}\n",
                spec.prompt,
                wright_agents::PROMPT_VERSION,
                crate::agent::MAX_TOKENS,
                bp.project,
                bp.chips
            );
            for d in &bp.deliverables {
                def.push_str(&format!("{}|{}\n", d.kind, d.check));
            }
            let mut inputs = Vec::new();
            let mut ws = None;
            if !setup {
                // The Reviewer judges the Testwright's report too: a
                // cached approval must never outlive a new test result.
                if *kind == SystemKind::Reviewer {
                    let report = env
                        .reports
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .get("Testwright")
                        .cloned()
                        .unwrap_or_default();
                    inputs.push(format!(
                        "testwright-report={}",
                        hash::blake3_hex(report.as_bytes())
                    ));
                }
                for a in &bp.agents {
                    for o in &a.outputs {
                        inputs.push(format!("{o}={}", artifact_hash(env, o)));
                    }
                }
                ws = Some(env.store.workspace_hash());
            }
            Some(cache::cache_key(&KeyInputs {
                definition: &def,
                resolved: task.model.as_ref(),
                tools: &task.tools,
                permission_bits: task.permissions.bits(),
                input_hashes: &inputs,
                workspace_hash: ws.as_deref(),
                app_version: APP_VERSION,
            }))
        }
    }
}

/// Run one worker step (ADR-026): the external agent does the work,
/// Wright keeps the contract, the tests and the receipt. Costs nothing
/// on this plan's budget — the worker's own account pays for its model;
/// the clock and the scope are enforced by the tool itself.
fn run_worker_step(
    env: &FiringEnv,
    task: &Task,
    agent: &wright_core::blueprint::AgentDef,
    worker_name: &str,
    me: Option<&str>,
) -> (usize, TaskStatus) {
    let name = task.name();
    env.spine.note(
        EventKind::AgentStarted,
        me,
        format!(
            "handing this step to {worker_name}, an external coding agent, in a sandboxed copy"
        ),
    );
    let Some(entry) = env.config.worker_for(worker_name).cloned() else {
        let why = format!(
            "the card names the worker '{worker_name}', but no worker with that name is configured on this machine. Add a `worker \"{worker_name}\" command=\"...\"` line to config.kdl (ADR-026)."
        );
        env.spine.note(EventKind::AgentFailed, me, why.clone());
        return (task.id, TaskStatus::Failed(why));
    };
    // The tool context: worker + scope + clock, on top of the shared one.
    let mut ctx = env.tools.clone();
    ctx.worker = Some(entry);
    ctx.worker_name = Some(worker_name.to_string());
    ctx.worker_files = agent.worker_files.clone();
    ctx.worker_timeout_secs = agent.worker_timeout_secs;

    // The brief: the goal, the inputs this copy holds, the outputs the
    // plan expects. The worker sees nothing else of Wright.
    let mut prompt = format!("{}\n", agent.goal);
    if !agent.inputs.is_empty() {
        prompt.push_str(&format!(
            "\nInput files (in this copy): {}.\n",
            agent
                .inputs
                .iter()
                .map(|i| format!("artifacts/{i}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !agent.outputs.is_empty() {
        prompt.push_str(&format!(
            "\nYou must produce these output files in the copy: {}.\n",
            agent.outputs.join(", ")
        ));
    }

    // The gate: same as every tool call.
    let verdict = {
        let mut budget = env.budget.lock().unwrap_or_else(|p| p.into_inner());
        env.gate.check(&name, "worker", &mut budget, 0)
    };
    let permit = match verdict {
        wright_runtime::gate::Verdict::Allow(p) => p,
        wright_runtime::gate::Verdict::Deny(why) => {
            env.spine.note(EventKind::AgentFailed, me, why.clone());
            return (task.id, TaskStatus::Failed(why));
        }
        wright_runtime::gate::Verdict::Ask(pending) => {
            // A worker talks to its own vendor with the project's copy in
            // hand: the Rule of Two's three legs, so the gate asks before
            // the copy is handed over — the same approver every other
            // outbound step goes through (a person in the window, a file
            // under schedule, or a refusal when nobody can answer).
            let n = env.approval_seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            let ask = crate::approve::Ask {
                run: env.spine.run(),
                n,
                agent: name.clone(),
                tool: "worker".into(),
                reason: pending.reason.clone(),
                preview: format!(
                    "hand a copy of this project to {worker_name} ({}), which sends it to its own model service, with this brief:\n{}",
                    entry_command(env, worker_name),
                    prompt.chars().take(1200).collect::<String>()
                ),
            };
            env.spine.note(
                EventKind::ApprovalRequested,
                me,
                format!(
                    "{} (request {n}, asking via {})",
                    pending.reason,
                    env.approver.name()
                ),
            );
            let approved = env.approver.decide(&ask);
            env.spine.note(
                EventKind::ApprovalResolved,
                me,
                if approved {
                    format!("request {n} approved")
                } else {
                    format!("request {n} denied")
                },
            );
            if !approved {
                let why = format!("a person declined handing this step to {worker_name}; nothing was sent and nothing changed");
                env.spine.note(EventKind::AgentFailed, me, why.clone());
                return (task.id, TaskStatus::Failed(why));
            }
            let mut budget = env.budget.lock().unwrap_or_else(|p| p.into_inner());
            match env.gate.approve(pending, &mut budget) {
                Ok(p) => p,
                Err(e) => {
                    let why = e.to_string();
                    env.spine.note(EventKind::AgentFailed, me, why.clone());
                    return (task.id, TaskStatus::Failed(why));
                }
            }
        }
    };
    let input = wright_core::wire::json::Value::obj(vec![(
        "prompt",
        wright_core::wire::json::Value::str(prompt.clone()),
    )]);
    let identity = permit.identity().unwrap_or("").to_string();
    let input_hash = wright_core::hash::blake3_hex(json::to_string(&input).as_bytes());
    let result = wright_tools::run(permit, &input, &ctx);
    if let Some(r) = env.gate.identities() {
        r.record(&name, "worker", &input_hash, result.is_ok());
    }
    let _ = env.spine.emit(
        EventKind::ToolCalled,
        me,
        format!("worker {worker_name} — {}", if result.is_ok() { "ok" } else { "error" }),
        json::to_string(&Value::obj(vec![
            ("tool", Value::str("worker")),
            ("ok", Value::Bool(result.is_ok())),
            ("identity", Value::str(identity)),
            ("input_blake3", Value::str(input_hash)),
        ])),
    );
    match result {
        Ok(o) => {
            if !o.wrote.is_empty() {
                env.written
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .insert(name.clone(), o.wrote.clone());
            }
            let report = format!("the worker {worker_name} finished. {}", o.text);
            env.reports
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(name.clone(), report.clone());
            let _ = env.spine.emit(
                EventKind::AgentFinished,
                me,
                first_sentence_of(&o.text),
                wright_core::wire::json::to_string(&wright_core::wire::json::Value::obj(vec![
                    ("cost_micro_usd", wright_core::wire::json::Value::int(0)),
                    (
                        "report",
                        wright_core::wire::json::Value::str(o.text.clone()),
                    ),
                ])),
            );
            (task.id, TaskStatus::Ran { cost_micro_usd: 0 })
        }
        Err(e) => {
            let why = e.to_string();
            env.spine.note(EventKind::AgentFailed, me, why.clone());
            (task.id, TaskStatus::Failed(why))
        }
    }
}

/// The program a configured worker runs, for the approval's words.
fn entry_command(env: &FiringEnv, worker_name: &str) -> String {
    env.config
        .worker_for(worker_name)
        .map(|w| w.command.clone())
        .unwrap_or_else(|| worker_name.to_string())
}

fn first_sentence_of(s: &str) -> String {
    let t = s.trim();
    match t.find(['.', '\n']) {
        Some(i) if i > 3 => t[..=i].to_string(),
        _ => t.lines().next().unwrap_or("").to_string(),
    }
}

/// Run (or skip, or restore) one task. Returns its id and status.
fn run_task(
    env: &FiringEnv,
    task: &Task,
    status: &Mutex<BTreeMap<usize, TaskStatus>>,
    verdict: &Mutex<Option<Result<(), String>>>,
    already: Option<String>,
    no_cache: bool,
) -> (usize, TaskStatus) {
    let out = run_task_body(env, task, status, verdict, already, no_cache);
    // The task is over: its identity attests to its calls and is revoked,
    // whatever the outcome — nothing can act under it again.
    if let Some(r) = env.gate.identities() {
        r.revoke(&task.name());
    }
    out
}

/// Issue the task's own identity: its card's grant, nothing more, valid
/// until the firing's clock runs out.
fn issue_identity(env: &FiringEnv, task: &Task) -> Result<(), String> {
    let Some(r) = env.gate.identities() else { return Ok(()) };
    let left = env.deadline.saturating_duration_since(Instant::now()).as_millis() as u64;
    let p = task.permissions.bits();
    let words: Vec<&str> = [
        (PermissionSet::READ_FILES, "files-read"),
        (PermissionSet::WRITE_FILES, "files-write"),
        (PermissionSet::NETWORK, "network"),
        (PermissionSet::SHELL, "shell"),
        (PermissionSet::SPAWN, "spawn"),
    ]
    .iter()
    .filter(|(bit, _)| p & bit != 0)
    .map(|(_, w)| *w)
    .collect();
    let did = r.issue(wright_runtime::identity::Claims {
        agent: task.name(),
        run: env.spine.run(),
        tools: task.tools.clone(),
        permissions: words.join(" "),
        secrets: env.gate.secrets_of(&task.name()),
        issued_ms: now_ms(),
        expires_ms: now_ms() + left,
    })?;
    env.spine.note(EventKind::Note, Some(&task.name()), format!("acting as {did} (this task only)"));
    Ok(())
}

fn run_task_body(
    env: &FiringEnv,
    task: &Task,
    status: &Mutex<BTreeMap<usize, TaskStatus>>,
    verdict: &Mutex<Option<Result<(), String>>>,
    already: Option<String>,
    no_cache: bool,
) -> (usize, TaskStatus) {
    let name = task.name();
    let me = Some(name.as_str());

    // 1. Dependencies.
    {
        let st = status.lock().unwrap_or_else(|p| p.into_inner());
        for d in &task.deps {
            match st.get(d) {
                Some(s) if s.ok() => {}
                Some(_) | None => {
                    let dep = env.graph.tasks[*d].name();
                    let why = format!("{dep} did not finish");
                    env.spine.note(EventKind::TaskSkipped, me, why.clone());
                    return (task.id, TaskStatus::Skipped(why));
                }
            }
        }
    }
    // 2. Nothing leaves the machine without the Reviewer's approval.
    if task.delivery {
        if let Some(Err(reason)) = verdict.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            let why = format!("the Reviewer rejected the work: {reason}");
            env.spine.note(EventKind::TaskSkipped, me, why.clone());
            return (task.id, TaskStatus::Skipped(why));
        }
    }
    // 3. Finished by an earlier attempt of this run (resume).
    if let Some(report) = already {
        record_verdict(env, task, &report, verdict);
        env.spine.note(
            EventKind::Note,
            me,
            "already finished before the resume; kept",
        );
        return (task.id, TaskStatus::Ran { cost_micro_usd: 0 });
    }

    // 3b. Typed edges (ADR-027): every promised hand-off this task
    // consumes is checked BEFORE it runs — a broken promise stops the
    // plan here, in words, instead of two agents later in an apology.
    if let TaskKind::Project(n) = &task.kind {
        for e in &env.bp.edges {
            if &e.to != n {
                continue;
            }
            if let (Some(out), Some(shape)) = (&e.out, &e.schema) {
                let path = env.tools.artifacts_dir().join(out);
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        if let Err(why) = shape.check_bytes(&bytes) {
                            let reason = format!(
                                "the hand-off {out} from {} to {} promised to be {}, but {why} — nothing downstream runs",
                                e.from,
                                e.to,
                                shape.promise()
                            );
                            env.spine.note(EventKind::AgentFailed, me, reason.clone());
                            return (task.id, TaskStatus::Failed(reason));
                        }
                    }
                    Err(_) => {
                        let reason = format!(
                            "the hand-off {out} from {} to {} was promised, but {} never produced it",
                            e.from, e.to, e.from
                        );
                        env.spine.note(EventKind::AgentFailed, me, reason.clone());
                        return (task.id, TaskStatus::Failed(reason));
                    }
                }
            }
        }
    }

    // 3b'. From here on the task acts: under its own identity.
    if let Err(e) = issue_identity(env, task) {
        env.spine.note(EventKind::AgentFailed, me, e.clone());
        return (task.id, TaskStatus::Failed(e));
    }

    // 3c. Worker steps (ADR-026): no model call — the engine hands the
    // goal to the configured external agent, through the gate like
    // every other tool call. Their output lands in the workspace and
    // goes through the same Testwright and Reviewer as everything else.
    if let TaskKind::Project(n) = &task.kind {
        let agent = env.bp.agent(n).expect("compiled from this blueprint");
        if let Some(worker_name) = &agent.worker {
            return run_worker_step(env, task, agent, worker_name, me);
        }
    }

    // 4. Built-ins.
    if let TaskKind::System(kind) = task.kind {
        if kind.is_builtin() {
            env.spine.note(EventKind::AgentStarted, me, "built-in step");
            let r = match kind {
                SystemKind::Shipwright => shipwright(env),
                SystemKind::Operator => operator(env),
                _ => Ok("nothing to do".into()),
            };
            return match r {
                Ok(note) => {
                    env.spine.note(EventKind::AgentFinished, me, note);
                    (task.id, TaskStatus::Ran { cost_micro_usd: 0 })
                }
                Err(e) => {
                    env.spine.note(EventKind::AgentFailed, me, e.clone());
                    (task.id, TaskStatus::Failed(e))
                }
            };
        }
    }

    // 5. The cache.
    let key = if no_cache { None } else { key_for(env, task) };
    if let Some(k) = &key {
        if let Some(entry) = env.store.cache_get(k) {
            match restore(env, &entry) {
                Ok(report) => {
                    record_verdict(env, task, &report, verdict);
                    let _ = env.spine.emit(
                        EventKind::CacheHit,
                        me,
                        format!(
                            "cached — free (saved ~{} from run {})",
                            usd(entry.cost_micro_usd),
                            entry.produced_by_run
                        ),
                        json::to_string(&Value::obj(vec![
                            ("key", Value::str(k.clone())),
                            ("report", Value::str(report)),
                            ("saved_micro_usd", Value::int(entry.cost_micro_usd as i64)),
                        ])),
                    );
                    return (
                        task.id,
                        TaskStatus::Cached {
                            saved_micro_usd: entry.cost_micro_usd,
                        },
                    );
                }
                Err(e) => env.spine.note(
                    EventKind::Note,
                    me,
                    format!("cache entry unusable ({e}); running instead"),
                ),
            }
        }
    }

    // 6. The agent, with retries.
    let why_fresh = match Freshness::of(task) {
        Freshness::Volatile => " (reads live data: always fresh)",
        Freshness::Delivery => " (touches the world: never cached, never retried)",
        Freshness::Cacheable => "",
    };
    let model = task
        .model
        .as_ref()
        .map(|m| m.primary.clone())
        .unwrap_or_default();
    env.spine.note(
        EventKind::AgentStarted,
        me,
        format!("working on {model}{why_fresh}"),
    );
    let attempts = 1 + task.retries as u32;
    let mut last_err = String::new();
    let mut spent = 0;
    for attempt in 1..=attempts {
        match env
            .run_agent(task)
            .and_then(|r| check_contract(env, task, r))
        {
            Ok(r) => {
                spent += r.cost_micro_usd;
                env.reports
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .insert(name.clone(), r.final_text.clone());
                record_verdict(env, task, &r.final_text, verdict);
                if let Some(k) = key {
                    let _ = save_to_cache(env, task, &k, spent, &r.final_text);
                }
                let _ = env.spine.emit(
                    EventKind::AgentFinished,
                    me,
                    format!("done in {} turn(s), {}", r.turns, usd(spent)),
                    json::to_string(&Value::obj(vec![
                        ("cost_micro_usd", Value::int(spent as i64)),
                        ("report", Value::str(r.final_text)),
                    ])),
                );
                return (
                    task.id,
                    TaskStatus::Ran {
                        cost_micro_usd: spent,
                    },
                );
            }
            Err(e) => {
                last_err = e;
                let hopeless = [
                    "budget",
                    "cancelled",
                    "wall clock",
                    "max-turns",
                    "no provider",
                    "is not in your keychain",
                    "not available",
                ]
                .iter()
                .any(|w| last_err.contains(w));
                if attempt < attempts && !hopeless {
                    env.spine.note(
                        EventKind::AgentRetrying,
                        me,
                        format!("attempt {attempt} failed: {last_err}; retrying"),
                    );
                } else {
                    break;
                }
            }
        }
    }
    env.spine.note(EventKind::AgentFailed, me, last_err.clone());
    // A failed checker is a rejection: delivery must not proceed.
    if matches!(
        task.kind,
        TaskKind::System(SystemKind::Reviewer) | TaskKind::System(SystemKind::Testwright)
    ) {
        *verdict.lock().unwrap_or_else(|p| p.into_inner()) =
            Some(Err(format!("{name} failed: {last_err}")));
    }
    (task.id, TaskStatus::Failed(last_err))
}

/// Project agents must have written every declared output.
fn check_contract(
    env: &FiringEnv,
    task: &Task,
    r: crate::agent::AgentResult,
) -> Result<crate::agent::AgentResult, String> {
    if let TaskKind::Project(n) = &task.kind {
        let agent = env.bp.agent(n).expect("compiled from this blueprint");
        let missing: Vec<&String> = agent
            .outputs
            .iter()
            .filter(|o| !env.tools.artifacts_dir().join(o).is_file())
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "finished without writing {}",
                missing
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    if let TaskKind::System(kind) = task.kind {
        if wright_agents::verdict_marker(kind).is_some() {
            if let Err(e) = wright_agents::parse_verdict(kind, &r.final_text) {
                if e.contains("did not end with") {
                    return Err(e);
                }
            }
        }
    }
    Ok(r)
}

fn record_verdict(
    env: &FiringEnv,
    task: &Task,
    report: &str,
    verdict: &Mutex<Option<Result<(), String>>>,
) {
    let TaskKind::System(kind) = task.kind else {
        return;
    };
    if wright_agents::verdict_marker(kind).is_none() {
        return;
    }
    let v = wright_agents::parse_verdict(kind, report);
    env.reports
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(task.name(), report.to_string());
    // Acceptance sentences (ADR-025): the Testwright reports one line
    // per promise; a broken one is a failure no matter what the summary
    // line said, and the Reviewer may not approve past it.
    if kind == SystemKind::Testwright {
        for (n, r) in wright_agents::parse_accepts(report) {
            env.accepts
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(n, r.is_ok());
        }
    }
    let broken_promises: Vec<usize> = env
        .accepts
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .filter(|(_, ok)| !**ok)
        .map(|(n, _)| *n)
        .collect();
    let mut slot = verdict.lock().unwrap_or_else(|p| p.into_inner());
    if kind == SystemKind::Reviewer && !broken_promises.is_empty() {
        // A verdict may not approve past a broken promise, whatever the
        // Reviewer said: the promises are the person's, not the model's.
        *slot = Some(Err(format!(
            "acceptance promise(s) {} failed — the Reviewer may not approve a run whose sentences fail (ADR-025)",
            broken_promises.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")
        )));
        env.spine.note(
            EventKind::Verdict,
            Some(&task.name()),
            format!(
                "{} : NOT PASSED — broken promise(s) {}",
                task.name(),
                broken_promises
                    .iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
        return;
    }
    match (kind, &v) {
        // The Reviewer has the final word; a Testwright failure is
        // recorded too, so delivery stops even if the Reviewer errs.
        // An approval never overrides a recorded test failure.
        (SystemKind::Reviewer, Ok(())) if matches!(slot.as_ref(), Some(Err(_))) => {}
        (SystemKind::Reviewer, _) => *slot = Some(v.clone()),
        (SystemKind::Testwright, Err(e)) => *slot = Some(Err(format!("tests failed: {e}"))),
        _ => {}
    }
    env.spine.note(
        EventKind::Verdict,
        Some(&task.name()),
        match &v {
            Ok(()) => format!("{} : PASS", task.name()),
            Err(e) => format!("{} : NOT PASSED — {e}", task.name()),
        },
    );
}

fn save_to_cache(
    env: &FiringEnv,
    task: &Task,
    key: &str,
    cost: u64,
    report: &str,
) -> Result<(), String> {
    let mut artifacts = Vec::new();
    if let TaskKind::Project(n) = &task.kind {
        for o in &env.bp.agent(n).expect("compiled").outputs {
            let bytes =
                std::fs::read(env.tools.artifacts_dir().join(o)).map_err(|e| e.to_string())?;
            artifacts.push((o.clone(), env.store.put_object(&bytes)?));
        }
    }
    artifacts.push(("__report".into(), env.store.put_object(report.as_bytes())?));
    env.store.cache_put(&CacheEntry {
        key: key.into(),
        artifacts,
        cost_micro_usd: cost,
        produced_by_run: env.spine.run(),
    })
}

/// Put cached artifacts back in place; return the cached report text.
fn restore(env: &FiringEnv, entry: &CacheEntry) -> Result<String, String> {
    let mut report = String::new();
    for (name, h) in &entry.artifacts {
        let bytes = env.store.get_object(h)?;
        if name == "__report" {
            report = String::from_utf8_lossy(&bytes).into_owned();
        } else {
            if !convention::is_safe_artifact_name(name) {
                return Err(format!("cache entry names an unsafe artifact '{name}'"));
            }
            write_atomic(&env.tools.artifacts_dir().join(name), &bytes)?;
        }
    }
    Ok(report)
}

// ── built-ins ────────────────────────────────────────────────────

/// Shipwright: lay the universal skeleton. Never overwrites a file.
fn shipwright(env: &FiringEnv) -> Result<String, String> {
    let root = &env.tools.project;
    let mut created = Vec::new();
    for f in convention::skeleton(&env.bp) {
        let p = root.join(&f.path);
        if p.exists() {
            continue;
        }
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&p, &f.contents).map_err(|e| format!("{}: {e}", p.display()))?;
        #[cfg(unix)]
        if f.executable {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755));
        }
        created.push(f.path);
    }
    let git_init = if !root.join(".git").exists() {
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(root)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    } else {
        false
    };
    if !created.is_empty() {
        env.written
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert("Shipwright".into(), created.clone());
    }
    Ok(match (created.len(), git_init) {
        (0, _) => "skeleton already in place".into(),
        (n, g) => format!(
            "laid the skeleton: {n} file(s){}",
            if g { ", git initialized" } else { "" }
        ),
    })
}

/// Operator: register the schedule (and say when the next run is).
fn operator(env: &FiringEnv) -> Result<String, String> {
    let Some(sched) = &env.bp.chips.scheduling else {
        return Ok("no schedule to register".into());
    };
    let next = crate::scheduler::register(&env.tools.project, &sched.cron, &sched.tz)?;
    let note = match crate::scheduler::entry_for(&env.tools.project).and_then(|e| crate::scheduler::hold_reason(&e)) {
        None => format!("scheduled: next run {next} ({})", sched.tz),
        Some(why) => format!(
            "scheduled for {next} ({}), but it will not run unattended yet: {why}. Approve it with `wright schedule approve`, or Let it run unattended in the window.",
            sched.tz
        ),
    };
    env.spine.note(
        EventKind::ScheduleRegistered,
        Some("Operator"),
        note.clone(),
    );
    Ok(note)
}

/// Agents that will refire on the next firing, and why — for `wright plan`.
pub fn never_cached_names(bp: &Blueprint) -> BTreeSet<String> {
    wright_core::blueprint::diff::never_cached(bp)
        .into_keys()
        .collect()
}
