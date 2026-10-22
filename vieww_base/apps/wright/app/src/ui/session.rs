//! One session = one project folder, and everything the screens read about it.
//!
//! Threads: vieww's tree is single-threaded (signals are `!Send`). The
//! engine and the interview run on worker threads and hand results back
//! through [`Shared`]; the window's `before_frame` hook calls
//! [`Session::pump`], which drains it into signals, and the app's waker
//! makes an idle window draw when something arrives.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use vieww::foundation::task::FrameWaker;
use vieww::prelude::*;

use wright_core::blueprint::model::PermissionSet;
use wright_core::blueprint::Blueprint;
use wright_core::compiler::{self, TaskGraph};
use wright_core::confirm::{self, Line};
use wright_core::events::{Event, EventKind, RunId};
use wright_core::wire::json;
use wright_engine::approve::StoreApprover;
use wright_engine::export::{self, Exported, Kind};
use wright_engine::interview::{self, InterviewIo};
use wright_engine::{fire, FireOptions, Trigger};
use wright_runtime::config::Config;
use wright_runtime::store::Store;

// ── what the screens show ────────────────────────────────────────

/// One agent, as the plan and the engine show it.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub name: String,
    /// "built-in", "helper", "live data", "delivers" or "agent".
    pub role: &'static str,
    pub model: String,
    pub tools: Vec<String>,
    /// What it may touch, in words.
    pub touch: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanView {
    pub name: String,
    /// The convention language (what the project can be exported as).
    pub language: String,
    pub request: String,
    pub lines: Vec<Line>,
    /// Steps in order; cards in one step run at the same time.
    pub stages: Vec<Vec<Card>>,
    pub per_run: f64,
    pub per_day: f64,
    pub commit: String,
}

impl PlanView {
    pub fn cards(&self) -> impl Iterator<Item = &Card> {
        self.stages.iter().flatten()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResultView {
    pub run: RunId,
    pub ok: bool,
    pub headline: String,
    pub verdict: String,
    pub artifacts: Vec<(String, u64)>,
    pub tests: String,
    pub review: String,
    pub cost: f64,
}

/// The receipt's own words, as `wright verify` reads them: every check,
/// its verdict and its sentence. Loaded beside the result so "trust is
/// one glance away" is a place in the window, not a trip to the CLI.
#[derive(Debug, Clone, PartialEq)]
pub struct ReceiptView {
    pub run: RunId,
    /// Every check passed.
    pub ok: bool,
    /// (name, passed, the check's own sentence).
    pub checks: Vec<(String, bool, String)>,
    /// The whole report, one line per check plus the verdict — what
    /// "Copy the report" puts on the pasteboard.
    pub report: String,
}

/// Where an agent is, projected from the event list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Queued,
    Working,
    Waiting,
    Done,
    Cached,
    Skipped,
    Failed,
}

impl State {
    pub fn label(self) -> &'static str {
        match self {
            State::Queued => "Queued",
            State::Working => "Working",
            State::Waiting => "Needs you",
            State::Done => "Done",
            State::Cached => "Reused",
            State::Skipped => "Skipped",
            State::Failed => "Failed",
        }
    }
    pub fn settled(self) -> bool {
        matches!(
            self,
            State::Done | State::Cached | State::Skipped | State::Failed
        )
    }
}

/// Every agent's state and last note, plus the run's spend (micro-USD).
pub struct Progress {
    pub agents: BTreeMap<String, (State, String)>,
    /// First AgentStarted and last settle time per agent (ms since epoch).
    pub times: BTreeMap<String, (u64, u64)>,
    pub spent: u64,
    pub tokens: (u64, u64),
    pub started_ms: u64,
}

impl Progress {
    pub fn of(events: &[Event]) -> Progress {
        let mut p = Progress {
            agents: BTreeMap::new(),
            times: BTreeMap::new(),
            spent: 0,
            tokens: (0, 0),
            started_ms: events.first().map(|e| e.at_ms).unwrap_or(0),
        };
        for e in events {
            if e.kind == EventKind::TokensRecorded {
                let d = e.data_value();
                let n = |k: &str| {
                    d.as_ref()
                        .and_then(|d| d.get(k).and_then(|c| c.as_u64()))
                        .unwrap_or(0)
                };
                p.spent += n("cost_micro_usd");
                p.tokens.0 += n("in");
                p.tokens.1 += n("out");
            }
            let Some(a) = &e.agent else { continue };
            let st = match e.kind {
                EventKind::AgentStarted
                | EventKind::ApprovalResolved
                | EventKind::AgentRetrying => State::Working,
                EventKind::AgentFinished => State::Done,
                EventKind::AgentFailed => State::Failed,
                EventKind::CacheHit => State::Cached,
                EventKind::TaskSkipped => State::Skipped,
                EventKind::ApprovalRequested => State::Waiting,
                _ => {
                    if let Some(entry) = p.agents.get_mut(a) {
                        entry.1 = e.note.clone();
                    }
                    continue;
                }
            };
            let t = p.times.entry(a.clone()).or_insert((e.at_ms, e.at_ms));
            t.1 = e.at_ms;
            p.agents.insert(a.clone(), (st, e.note.clone()));
        }
        p
    }

    pub fn state(&self, agent: &str) -> State {
        self.agents.get(agent).map(|a| a.0).unwrap_or(State::Queued)
    }

    pub fn seconds(&self, agent: &str) -> f64 {
        self.times
            .get(agent)
            .map(|(a, b)| (b.saturating_sub(*a)) as f64 / 1000.0)
            .unwrap_or(0.0)
    }
}

// ── loading (pure functions of the project on disk) ──────────────

fn touch_words(perm: PermissionSet, tools: &[String]) -> String {
    let has = |bit: u32| perm.0 & bit != 0;
    let mut out = Vec::new();
    if has(PermissionSet::NETWORK) {
        out.push("the internet");
    }
    if has(PermissionSet::SHELL) {
        out.push("a sandboxed shell");
    }
    if has(PermissionSet::WRITE_FILES) {
        out.push("writes project files");
    } else if has(PermissionSet::READ_FILES) {
        out.push("reads project files");
    }
    if tools
        .iter()
        .any(|t| t.contains("email") || t.contains("post") || t.contains("send"))
    {
        out.push("sends to the outside world (asks first)");
    }
    if out.is_empty() {
        "nothing outside its own work".into()
    } else {
        out.join(", ")
    }
}

pub fn load_plan(project: &Path) -> Option<PlanView> {
    let bp: Blueprint = wright_engine::load_blueprint(project).ok()?;
    let graph: TaskGraph = compiler::compile(&bp).ok()?;
    let lines = confirm::summary(&bp, &graph);
    let stages = graph
        .stages
        .iter()
        .map(|s| {
            s.iter()
                .map(|&id| {
                    let t = &graph.tasks[id];
                    let role = if t.is_builtin() {
                        "built-in"
                    } else if t.delivery {
                        "delivers"
                    } else if t.volatile {
                        "live data"
                    } else if matches!(t.kind, compiler::TaskKind::System(_)) {
                        "helper"
                    } else {
                        "agent"
                    };
                    Card {
                        name: t.name(),
                        role,
                        model: t
                            .model
                            .as_ref()
                            .map(|m| m.primary.clone())
                            .unwrap_or_default(),
                        tools: t.tools.clone(),
                        touch: touch_words(t.permissions, &t.tools),
                    }
                })
                .collect()
        })
        .collect();
    Some(PlanView {
        name: bp.meta.name.clone(),
        language: bp.project.language.clone(),
        request: bp.intent.summary.clone(),
        lines,
        stages,
        per_run: bp.guardrails.per_firing_micro_usd as f64 / 1e6,
        per_day: bp.guardrails.daily_micro_usd as f64 / 1e6,
        commit: wright_core::hash::short(&wright_core::blueprint::canon::content_hash(&bp))
            .to_string(),
    })
}

/// The receipt, in `wright verify`'s words — the same six checks the
/// CLI prints, run over the same store, so the window and the terminal
/// can never disagree about what the evidence says.
pub fn load_receipt(project: &Path, run: RunId) -> Option<ReceiptView> {
    let v = wright_engine::verify::project_run(project, Some(run)).ok()?;
    let report = v.report();
    Some(ReceiptView {
        run: v.run.unwrap_or(run),
        ok: v.ok,
        checks: v
            .checks
            .into_iter()
            .map(|c| (c.name.to_string(), c.ok, c.note))
            .collect(),
        report,
    })
}

pub fn load_result(project: &Path, run: RunId) -> Option<ResultView> {
    let store = Store::open(project).ok()?;
    let text = std::fs::read_to_string(store.run_dir(run).join("receipt.json")).ok()?;
    let v = json::parse(&text).ok()?;
    let mut r = ResultView {
        run,
        ok: v.get("ok").and_then(|b| b.as_bool()).unwrap_or(false),
        headline: v.str_of("headline").unwrap_or("").to_string(),
        verdict: v.str_of("verdict").unwrap_or("no review").to_string(),
        cost: v
            .get("cost_micro_usd")
            .and_then(|c| c.as_u64())
            .unwrap_or(0) as f64
            / 1e6,
        ..ResultView::default()
    };
    for a in v.get("artifacts").and_then(|a| a.as_arr()).unwrap_or(&[]) {
        let name = a.str_of("name").unwrap_or("").to_string();
        let size = std::fs::metadata(project.join("artifacts").join(&name))
            .map(|m| m.len())
            .unwrap_or(0);
        r.artifacts.push((name, size));
    }
    if let Ok(log) = store.read_events(run) {
        for e in log.events() {
            if e.kind == EventKind::AgentFinished || e.kind == EventKind::CacheHit {
                let report = e
                    .data_value()
                    .and_then(|d| d.str_of("report").map(String::from))
                    .unwrap_or_default();
                match e.agent.as_deref() {
                    Some("Testwright") => r.tests = report,
                    Some("Reviewer") => r.review = report,
                    _ => {}
                }
            }
        }
    }
    Some(r)
}

fn last_run(project: &Path) -> Option<RunId> {
    Store::open(project).ok()?.runs().last().copied()
}

// ── the conversation, kept ───────────────────────────────────────

/// Where a session's conversation is kept: beside its run history, in the
/// project's store (outside the project folder, like everything else Wright
/// remembers about it), one JSON line per message:
/// `{"you":true,"text":"..."}`. Appended as messages arrive, so a crash or a
/// close loses nothing, and read back when the session opens — from Saved,
/// or at the next launch.
fn conversation_file(project: &Path) -> Option<PathBuf> {
    Store::open(project)
        .ok()
        .map(|s| s.root().join("conversation.jsonl"))
}

/// The conversation, and how many of its messages came before the plan
/// was written (a `{"plan":true}` line marks the moment): those drafted
/// the plan, the rest asked for changes to it.
pub fn load_conversation(project: &Path) -> (Vec<(bool, String)>, usize) {
    let Some(text) = conversation_file(project).and_then(|f| std::fs::read_to_string(f).ok())
    else {
        return (Vec::new(), 0);
    };
    let (mut chat, mut drafted) = (Vec::new(), None);
    for v in text.lines().filter_map(|l| json::parse(l).ok()) {
        if v.get("plan").and_then(|p| p.as_bool()) == Some(true) {
            drafted.get_or_insert(chat.len());
        } else if let (Some(you), Some(text)) =
            (v.get("you").and_then(|y| y.as_bool()), v.str_of("text"))
        {
            chat.push((you, text.to_string()));
        }
    }
    let drafted = drafted.unwrap_or(chat.len());
    (chat, drafted)
}

/// What a saved (closed) session is, for its card: a title (the plan's
/// name, else what was asked, else the folder), the last message, and how
/// many there are.
pub struct SavedSummary {
    pub title: String,
    pub last: Option<String>,
    pub messages: usize,
}

pub fn saved_summary(project: &Path) -> SavedSummary {
    let (chat, _) = load_conversation(project);
    let one_line = |s: &str, n: usize| {
        let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
        if s.chars().count() > n {
            format!("{}…", s.chars().take(n - 1).collect::<String>().trim_end())
        } else {
            s
        }
    };
    let folder = project
        .file_name()
        .map(|n| n.to_string_lossy().replace('-', " "))
        .unwrap_or_default();
    let title = load_plan(project)
        .map(|p| p.name)
        .filter(|n| !n.is_empty())
        .or_else(|| {
            chat.iter()
                .find(|(you, _)| *you)
                .map(|(_, t)| one_line(t, 48))
        })
        .unwrap_or(folder);
    let last = chat.last().map(|(you, t)| {
        if *you {
            format!("You: {}", one_line(t, 90))
        } else {
            one_line(t, 96)
        }
    });
    SavedSummary {
        title,
        last,
        messages: chat.len(),
    }
}

fn mark_plan_written(project: &Path) {
    use std::io::Write as _;
    if let Some(Ok(mut f)) = conversation_file(project).map(|p| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
    }) {
        let _ = writeln!(f, "{{\"plan\":true}}");
    }
}

fn keep_conversation(project: &Path, messages: &[(bool, String)]) {
    use std::io::Write as _;
    let Some(path) = conversation_file(project) else {
        return;
    };
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    for (you, text) in messages {
        let line = json::to_string(&json::Value::obj(vec![
            ("you", json::Value::Bool(*you)),
            ("text", json::Value::str(text.clone())),
        ]));
        let _ = writeln!(f, "{line}");
    }
}

/// While a reply is on its way: what Wright is doing and what it has
/// written so far.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    /// "Thinking", "Drafting the plan", "Checking the draft plan", a
    /// download line…
    pub what: String,
    /// The reply as it streams in (empty until the first words).
    pub text: String,
    pub since: Instant,
}

// ── the worker bridge ────────────────────────────────────────────

/// The worker-thread side of a session.
#[derive(Default)]
pub struct Shared {
    events: Mutex<Vec<Event>>,
    chat: Mutex<Vec<(bool, String)>>,
    confirm: Mutex<Option<Vec<Line>>>,
    finished: Mutex<Option<Result<ResultView, String>>>,
    interview_done: Mutex<Option<Result<bool, String>>>,
    exported: Mutex<Option<Result<Exported, String>>>,
    to_interview: Mutex<Option<Sender<Reply>>>,
    /// What the interview is doing and its words so far (`working`,
    /// `partial`); `None` once it is waiting for you again.
    live: Mutex<Option<Option<(String, String)>>>,
}

enum Reply {
    Text(String),
    Confirm(bool),
}

fn take<T: Default>(m: &Mutex<T>) -> T {
    std::mem::take(&mut *m.lock().unwrap_or_else(|p| p.into_inner()))
}

/// A Send-able wake handle for worker threads.
#[derive(Clone)]
pub struct WakeHandle(pub Option<vieww_platform_winit::Waker>);

impl WakeHandle {
    pub fn wake(&self) {
        if let Some(w) = &self.0 {
            w.wake();
        }
    }
    pub fn current() -> WakeHandle {
        WakeHandle(super::APP_WAKER.with_borrow(|w| w.clone()))
    }
}

/// Interview I/O over channels: the worker blocks, the UI answers.
struct ChannelIo {
    earlier: Vec<(bool, String)>,
    /// What is on screen while it works: (doing, written so far).
    now: (String, String),
    first: Option<String>,
    rx: Receiver<Reply>,
    shared: Arc<Shared>,
    wake: WakeHandle,
}

impl ChannelIo {
    /// Change what the screen shows while you wait.
    fn live(&mut self, change: impl FnOnce(&mut (String, String))) {
        change(&mut self.now);
        *self.shared.live.lock().unwrap_or_else(|p| p.into_inner()) = Some(Some(self.now.clone()));
        self.wake.wake();
    }

    /// Nothing in progress: the interview is waiting for you.
    fn idle(&mut self) {
        self.now = Default::default();
        *self.shared.live.lock().unwrap_or_else(|p| p.into_inner()) = Some(None);
        self.wake.wake();
    }
}

impl InterviewIo for ChannelIo {
    fn say(&mut self, text: &str) {
        self.shared
            .chat
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push((false, text.to_string()));
        // The words are in the conversation now; nothing is half-written.
        self.live(|l| l.1.clear());
    }
    fn earlier(&mut self) -> Vec<(bool, String)> {
        std::mem::take(&mut self.earlier)
    }
    fn working(&mut self, what: &str) {
        let what = what.to_string();
        self.live(|l| l.0 = what);
    }
    fn partial(&mut self, so_far: &str) {
        let text = so_far.to_string();
        self.live(|l| l.1 = text);
    }
    fn ask(&mut self) -> Option<String> {
        if let Some(f) = self.first.take() {
            return Some(f);
        }
        self.idle();
        loop {
            match self.rx.recv() {
                Ok(Reply::Text(t)) => return Some(t),
                Ok(Reply::Confirm(_)) => continue,
                Err(_) => return None,
            }
        }
    }
    fn confirm(&mut self, lines: &[Line]) -> bool {
        *self
            .shared
            .confirm
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(lines.to_vec());
        self.idle();
        loop {
            match self.rx.recv() {
                Ok(Reply::Confirm(b)) => return b,
                Ok(Reply::Text(_)) => continue,
                Err(_) => return false,
            }
        }
    }
}

// ── the composer's state ─────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Voice {
    Idle,
    Listening,
    Transcribing,
    Failed(String),
}

/// Text, attachments, the `+` menu and the mic — one per place you can type.
#[derive(Clone)]
pub struct Composer {
    pub id: u64,
    pub prompt: Signal<TextEditingValue>,
    pub files: Signal<Vec<PathBuf>>,
    pub menu: Signal<bool>,
    pub voice: Signal<Voice>,
}

impl Composer {
    pub fn new(rt: &vieww::element::Runtime, id: u64) -> Composer {
        Composer {
            id,
            prompt: rt.signal(TextEditingValue::new("")),
            files: rt.signal(Vec::new()),
            menu: rt.signal(false),
            voice: rt.signal(Voice::Idle),
        }
    }

    pub fn attach(&self, paths: &[PathBuf]) {
        self.files.update(|f| {
            for p in paths {
                if !f.contains(p) {
                    f.push(p.clone());
                }
            }
        });
    }

    /// Put transcribed speech at the end of what is already typed.
    pub fn insert_text(&self, text: &str) {
        let cur = self.prompt.get().text;
        let joined = if cur.trim().is_empty() {
            text.to_string()
        } else {
            format!("{} {text}", cur.trim_end())
        };
        self.prompt.set(TextEditingValue::new(joined));
    }

    /// The message to send (with a note about attachments), clearing the box.
    /// Files are copied into `<project>/artifacts/` so agents can read them by
    /// name; folders are referenced where they are (copying a whole tree
    /// silently is not a favour).
    pub fn take_message(&self, project: Option<&Path>) -> Option<String> {
        let text = self.prompt.get().text.trim().to_string();
        let files = self.files.get();
        if text.is_empty() && files.is_empty() {
            return None;
        }
        let mut notes = Vec::new();
        for f in &files {
            let name = f
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if f.is_dir() {
                notes.push(format!("the folder {}", f.display()));
            } else if let Some(dir) = project {
                let dest = dir.join("artifacts");
                let _ = std::fs::create_dir_all(&dest);
                match std::fs::copy(f, dest.join(&name)) {
                    Ok(_) => notes.push(format!("artifacts/{name}")),
                    Err(e) => notes.push(format!("{} (could not copy: {e})", f.display())),
                }
            }
        }
        self.prompt.set(TextEditingValue::new(""));
        self.files.set(Vec::new());
        self.menu.set(false);
        Some(if notes.is_empty() {
            text
        } else {
            format!("{text}\n\nAttached: {}", notes.join(", "))
        })
    }
}

// ── a session ────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Session {
    pub project: Rc<PathBuf>,
    pub composer: Composer,
    pub chat: Signal<Vec<(bool, String)>>,
    pub confirm: Signal<Option<Vec<Line>>>,
    pub plan: Signal<Option<PlanView>>,
    pub events: Signal<Vec<Event>>,
    pub running: Signal<bool>,
    pub result: Signal<Option<ResultView>>,
    pub approvals: Signal<Vec<(RunId, u32, String, String)>>,
    pub notice: Signal<String>,
    /// Where the conversation is scrolled to.
    pub scroll: super::kit::ScrollState,
    /// How far the approval card is being dragged sideways.
    pub card_drag: Signal<f32>,
    /// The Export menu, the export in progress, and the latest results
    /// (newest first).
    pub export_menu: Signal<bool>,
    pub exporting: Signal<Option<Kind>>,
    pub exports: Signal<Vec<Result<Exported, String>>>,
    /// How many messages of `chat` drafted the plan (all of them, until
    /// there is a plan); the rest asked for changes.
    pub drafted: Signal<usize>,
    /// A reply on its way (see [`Pending`]); `None` when nothing is.
    pub pending: Signal<Option<Pending>>,
    /// The interview is running (drafting or changing the plan).
    pub interviewing: Signal<bool>,
    /// The last run's receipt, in verify's words; `None` until one is.
    pub receipt: Signal<Option<ReceiptView>>,
    /// The receipt overlay is open over this page.
    pub receipt_open: Signal<bool>,
    shared: Arc<Shared>,
    last_poll: Rc<Cell<Instant>>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Session({})", self.project.display())
    }
}

impl Session {
    pub fn open(rt: &vieww::element::Runtime, project: PathBuf, composer_id: u64) -> Session {
        let (conversation, drafted) = load_conversation(&project);
        let plan = load_plan(&project);
        let (events, result, receipt) = match last_run(&project) {
            Some(run) => (
                Store::open(&project)
                    .ok()
                    .and_then(|s| s.read_events(run).ok())
                    .map(|l| l.events().to_vec())
                    .unwrap_or_default(),
                load_result(&project, run),
                load_receipt(&project, run),
            ),
            None => (Vec::new(), None, None),
        };
        Session {
            project: Rc::new(project),
            composer: Composer::new(rt, composer_id),
            chat: rt.signal(conversation),
            confirm: rt.signal(None),
            plan: rt.signal(plan),
            events: rt.signal(events),
            running: rt.signal(false),
            result: rt.signal(result),
            approvals: rt.signal(Vec::new()),
            notice: rt.signal(String::new()),
            scroll: super::kit::ScrollState::new(rt),
            card_drag: rt.signal(0.0),
            export_menu: rt.signal(false),
            exporting: rt.signal(None),
            exports: rt.signal(Vec::new()),
            drafted: rt.signal(drafted),
            pending: rt.signal(None),
            interviewing: rt.signal(false),
            receipt: rt.signal(receipt),
            receipt_open: rt.signal(false),
            shared: Arc::new(Shared::default()),
            last_poll: Rc::new(Cell::new(Instant::now())),
        }
    }

    /// The name people see: the blueprint's, else the folder's.
    pub fn title(&self) -> String {
        match self.plan.get() {
            Some(p) if !p.name.is_empty() => p.name,
            _ => self
                .project
                .file_name()
                .map(|n| n.to_string_lossy().replace('-', " "))
                .unwrap_or_else(|| "New session".into()),
        }
    }

    /// One line for lists and the status bar, and how it should be coloured.
    pub fn status(&self) -> (String, Tone) {
        let waiting = self.approvals.get().len();
        if waiting > 0 {
            return (
                format!("Waiting for you · {waiting} to approve"),
                Tone::Wait,
            );
        }
        if self.running.get() {
            let progress = Progress::of(&self.events.get());
            let working: Vec<String> = progress
                .agents
                .iter()
                .filter(|(_, (s, _))| *s == State::Working)
                .map(|(n, _)| n.clone())
                .collect();
            return (
                if working.is_empty() {
                    "Running".into()
                } else {
                    format!("Running · {} working", working.join(", "))
                },
                Tone::Live,
            );
        }
        if self.interviewing.get() {
            return ("Drafting the plan with you".into(), Tone::Live);
        }
        match (self.result.get(), self.plan.get()) {
            (Some(r), _) if r.ok => (format!("Delivered · run {}", r.run), Tone::Good),
            (Some(r), _) => (format!("Stopped · run {}", r.run), Tone::Bad),
            (None, Some(_)) => ("Plan ready · not run yet".into(), Tone::Quiet),
            (None, None) => ("Describe what should exist".into(), Tone::Quiet),
        }
    }

    /// Share of agents settled in the current or last run, 0..1.
    pub fn done_share(&self) -> f32 {
        let Some(plan) = self.plan.get() else {
            return 0.0;
        };
        let p = Progress::of(&self.events.get());
        let total = plan.cards().count().max(1);
        plan.cards().filter(|c| p.state(&c.name).settled()).count() as f32 / total as f32
    }

    pub fn live_agents(&self) -> usize {
        Progress::of(&self.events.get())
            .agents
            .values()
            .filter(|(s, _)| matches!(s, State::Working | State::Waiting))
            .count()
    }

    /// Called before every frame: move worker results into signals.
    pub fn pump(&self) {
        let new_events = take(&self.shared.events);
        if !new_events.is_empty() {
            self.events.update(|v| v.extend(new_events));
        }
        let new_chat = take(&self.shared.chat);
        if !new_chat.is_empty() {
            keep_conversation(&self.project, &new_chat);
            self.chat.update(|c| c.extend(new_chat));
        }
        if let Some(live) = self
            .shared
            .live
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
        {
            let next = live.map(|(what, text)| {
                let since = self.pending.get().map_or_else(Instant::now, |p| p.since);
                Pending {
                    what: if what.is_empty() {
                        "Thinking".into()
                    } else {
                        what
                    },
                    text,
                    since,
                }
            });
            if next != self.pending.get() {
                self.pending.set(next);
            }
        }
        if let Some(lines) = self
            .shared
            .confirm
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
        {
            self.confirm.set(Some(lines));
        }
        if let Some(done) = self
            .shared
            .interview_done
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
        {
            self.interviewing.set(false);
            self.pending.set(None);
            match done {
                Ok(true) => {
                    if self.plan.get().is_none() {
                        mark_plan_written(&self.project);
                        self.drafted.set(self.chat.get().len());
                    }
                    self.plan.set(load_plan(&self.project));
                    self.notice.set(String::new());
                }
                Ok(false) => self
                    .notice
                    .set("Nothing was written. Tell me what to change.".into()),
                Err(e) => self.notice.set(format!("The interview stopped: {e}")),
            }
            self.confirm.set(None);
        }
        if let Some(fin) = self
            .shared
            .finished
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
        {
            self.running.set(false);
            match fin {
                Ok(r) => {
                    // The receipt loads with the result it proves — the
                    // window's evidence and the CLI's are one read.
                    self.receipt.set(load_receipt(&self.project, r.run));
                    self.result.set(Some(r));
                }
                Err(e) => self.notice.set(format!("The run did not start: {e}")),
            }
        }
        if let Some(r) = self
            .shared
            .exported
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
        {
            self.exporting.set(None);
            self.exports.update(|v| {
                v.insert(0, r);
                v.truncate(4);
            });
        }
        if self.running.get() && self.last_poll.get().elapsed() > Duration::from_millis(400) {
            self.last_poll.set(Instant::now());
            if let Ok(store) = Store::open(&self.project) {
                let pending = store.pending_approvals();
                if pending != self.approvals.get() {
                    self.card_drag.set(0.0);
                    self.approvals.set(pending);
                }
            }
        }
    }

    pub fn start_run(&self) {
        if self.running.get() {
            return;
        }
        self.running.set(true);
        self.result.set(None);
        self.events.set(Vec::new());
        self.notice.set(String::new());
        let project = (*self.project).clone();
        let shared = Arc::clone(&self.shared);
        let wake = WakeHandle::current();
        std::thread::spawn(move || {
            let (s2, w2) = (Arc::clone(&shared), wake.clone());
            let opts = FireOptions {
                trigger: Trigger::Manual,
                approver: match Store::open(&project) {
                    Ok(store) => Box::new(StoreApprover {
                        store,
                        timeout: Duration::from_secs(1800),
                    }),
                    Err(_) => Box::new(wright_engine::approve::AutoDeny),
                },
                listeners: vec![Box::new(move |e: &Event| {
                    s2.events
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .push(e.clone());
                    w2.wake();
                })],
                ..FireOptions::default()
            };
            let r = fire(&project, opts).map(|s| load_result(&project, s.run).unwrap_or_default());
            *shared.finished.lock().unwrap_or_else(|p| p.into_inner()) = Some(r);
            wake.wake();
        });
    }

    /// Send what is in the composer: to the interview (drafting or changing
    /// the plan). Returns false when there was nothing to send.
    pub fn send(&self) -> bool {
        let Some(text) = self.composer.take_message(Some(&self.project)) else {
            return false;
        };
        self.say(text);
        true
    }

    /// Send `text` as the user's message.
    pub fn say(&self, text: String) {
        self.chat.update(|c| c.push((true, text.clone())));
        keep_conversation(&self.project, &[(true, text.clone())]);
        // Something is happening from this moment, before any model answers.
        self.pending.set(Some(Pending {
            what: "Reading your message".into(),
            text: String::new(),
            since: Instant::now(),
        }));
        // An interview already waiting for an answer gets this one.
        if let Some(tx) = self
            .shared
            .to_interview
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
        {
            if tx.send(Reply::Text(text.clone())).is_ok() {
                return;
            }
        }
        let (tx, rx) = channel::<Reply>();
        *self
            .shared
            .to_interview
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(tx);
        self.interviewing.set(true);
        // A conversation from before (a reopened session, or one whose
        // interview stopped) carries on rather than starting over: what was
        // said since the plan was written, or all of it if there is none.
        let chat = self.chat.get();
        let from = if self.plan.get().is_some() {
            self.drafted.get().min(chat.len())
        } else {
            0
        };
        let mut earlier: Vec<(bool, String)> = chat[from..chat.len().saturating_sub(1)].to_vec();
        earlier.drain(..earlier.len().saturating_sub(40));
        let project = (*self.project).clone();
        let shared = Arc::clone(&self.shared);
        let wake = WakeHandle::current();
        std::thread::spawn(move || {
            let mut io = ChannelIo {
                earlier,
                now: Default::default(),
                first: Some(text),
                rx,
                shared: Arc::clone(&shared),
                wake: wake.clone(),
            };
            let r = Config::load()
                .and_then(|cfg| interview::interview(&project, &cfg, &mut io))
                .map(|bp| bp.is_some());
            *shared
                .to_interview
                .lock()
                .unwrap_or_else(|p| p.into_inner()) = None;
            *shared
                .interview_done
                .lock()
                .unwrap_or_else(|p| p.into_inner()) = Some(r);
            wake.wake();
        });
    }

    /// Export in the background, into `Exports/<project>` next to it.
    pub fn export(&self, kind: Kind) {
        if self.exporting.get().is_some() {
            return;
        }
        self.export_menu.set(false);
        self.exporting.set(Some(kind));
        let project = (*self.project).clone();
        let shared = Arc::clone(&self.shared);
        let wake = WakeHandle::current();
        std::thread::spawn(move || {
            let r = export::export(&project, kind, &export::default_out(&project));
            *shared.exported.lock().unwrap_or_else(|p| p.into_inner()) = Some(r);
            wake.wake();
        });
    }

    pub fn answer_confirm(&self, yes: bool) {
        self.confirm.set(None);
        self.pending.set(Some(Pending {
            what: if yes {
                "Writing the blueprint".into()
            } else {
                "Noting what to change".into()
            },
            text: String::new(),
            since: Instant::now(),
        }));
        if let Some(tx) = self
            .shared
            .to_interview
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
        {
            let _ = tx.send(Reply::Confirm(yes));
        }
    }

    pub fn answer_approval(&self, run: RunId, n: u32, yes: bool) {
        if let Ok(store) = Store::open(&self.project) {
            let _ = store.answer_approval(run, n, yes);
        }
        self.card_drag.set(0.0);
        self.approvals
            .update(|a| a.retain(|(r, k, _, _)| !(*r == run && *k == n)));
    }

    /// Spent today across runs (micro-USD), from the store's ledger.
    pub fn spent_today(&self) -> u64 {
        let day = wright_engine::firing::today_utc();
        Store::open(&self.project)
            .map(|s| s.spent_on(&day))
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Quiet,
    Live,
    Wait,
    Good,
    Bad,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the interview reports while it works reaches the screen: the
    /// status line, the words as they stream, and nothing once it waits.
    #[test]
    fn a_reply_shows_while_it_is_written() {
        let mut driver = vieww::render::FrameDriver::new(vieww::prelude::Size::new(800.0, 600.0));
        let dir = std::env::temp_dir().join(format!("wright-live-{}", std::process::id()));
        let s = Session::open(driver.elements().runtime(), dir, 1);
        let (_tx, rx) = channel::<Reply>();
        let mut io = ChannelIo {
            earlier: vec![(true, "before".into())],
            now: Default::default(),
            first: None,
            rx,
            shared: Arc::clone(&s.shared),
            wake: WakeHandle(None),
        };
        assert_eq!(io.earlier(), vec![(true, "before".to_string())]);
        assert!(io.earlier().is_empty(), "handed over once");

        io.working("Thinking");
        s.pump();
        assert_eq!(
            s.pending.get().map(|p| (p.what, p.text)),
            Some(("Thinking".into(), String::new()))
        );
        let since = s.pending.get().unwrap().since;
        io.partial("Which fol");
        s.pump();
        io.working("Drafting the plan");
        s.pump();
        let now = s.pending.get().unwrap();
        assert_eq!(
            (now.what.as_str(), now.text.as_str()),
            ("Drafting the plan", "Which fol")
        );
        assert_eq!(
            now.since, since,
            "the clock runs from the first sign of work"
        );
        io.say("Which folder?");
        s.pump();
        assert_eq!(
            s.chat.get().last(),
            Some(&(false, "Which folder?".to_string()))
        );
        assert_eq!(
            s.pending.get().map(|p| p.text),
            Some(String::new()),
            "said, so no longer half-written"
        );
        io.idle();
        s.pump();
        assert_eq!(s.pending.get(), None, "waiting for you");
    }
}
