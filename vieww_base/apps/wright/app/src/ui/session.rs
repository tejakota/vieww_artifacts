//! One session = one project folder, and everything the screens read about it.
//!
//! Threads: vieww's tree is single-threaded (signals are `!Send`). The
//! engine and the interview run on worker threads and hand results back
//! through [`Shared`]; the window's `before_frame` hook calls
//! [`Session::pump`], which drains it into signals, and the app's waker
//! makes an idle window draw when something arrives.

use std::cell::{Cell, RefCell};
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
        matches!(self, State::Done | State::Cached | State::Skipped | State::Failed)
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
        let mut p = Progress { agents: BTreeMap::new(), times: BTreeMap::new(), spent: 0, tokens: (0, 0), started_ms: events.first().map(|e| e.at_ms).unwrap_or(0) };
        for e in events {
            if e.kind == EventKind::TokensRecorded {
                let d = e.data_value();
                let n = |k: &str| d.as_ref().and_then(|d| d.get(k).and_then(|c| c.as_u64())).unwrap_or(0);
                p.spent += n("cost_micro_usd");
                p.tokens.0 += n("in");
                p.tokens.1 += n("out");
            }
            let Some(a) = &e.agent else { continue };
            let st = match e.kind {
                EventKind::AgentStarted | EventKind::ApprovalResolved | EventKind::AgentRetrying => State::Working,
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
        self.times.get(agent).map(|(a, b)| (b.saturating_sub(*a)) as f64 / 1000.0).unwrap_or(0.0)
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
    if tools.iter().any(|t| t.contains("email") || t.contains("post") || t.contains("send")) {
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
                        model: t.model.as_ref().map(|m| m.primary.clone()).unwrap_or_default(),
                        tools: t.tools.clone(),
                        touch: touch_words(t.permissions, &t.tools),
                    }
                })
                .collect()
        })
        .collect();
    Some(PlanView {
        name: bp.meta.name.clone(),
        request: bp.intent.summary.clone(),
        lines,
        stages,
        per_run: bp.guardrails.per_firing_micro_usd as f64 / 1e6,
        per_day: bp.guardrails.daily_micro_usd as f64 / 1e6,
        commit: wright_core::hash::short(&wright_core::blueprint::canon::content_hash(&bp)).to_string(),
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
        cost: v.get("cost_micro_usd").and_then(|c| c.as_u64()).unwrap_or(0) as f64 / 1e6,
        ..ResultView::default()
    };
    for a in v.get("artifacts").and_then(|a| a.as_arr()).unwrap_or(&[]) {
        let name = a.str_of("name").unwrap_or("").to_string();
        let size = std::fs::metadata(project.join("artifacts").join(&name)).map(|m| m.len()).unwrap_or(0);
        r.artifacts.push((name, size));
    }
    if let Ok(log) = store.read_events(run) {
        for e in log.events() {
            if e.kind == EventKind::AgentFinished || e.kind == EventKind::CacheHit {
                let report = e.data_value().and_then(|d| d.str_of("report").map(String::from)).unwrap_or_default();
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

// ── the worker bridge ────────────────────────────────────────────

/// The worker-thread side of a session.
#[derive(Default)]
pub struct Shared {
    events: Mutex<Vec<Event>>,
    chat: Mutex<Vec<(bool, String)>>,
    confirm: Mutex<Option<Vec<Line>>>,
    finished: Mutex<Option<Result<ResultView, String>>>,
    interview_done: Mutex<Option<Result<bool, String>>>,
    to_interview: Mutex<Option<Sender<Reply>>>,
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
    first: Option<String>,
    rx: Receiver<Reply>,
    shared: Arc<Shared>,
    wake: WakeHandle,
}

impl InterviewIo for ChannelIo {
    fn say(&mut self, text: &str) {
        self.shared.chat.lock().unwrap_or_else(|p| p.into_inner()).push((false, text.to_string()));
        self.wake.wake();
    }
    fn ask(&mut self) -> Option<String> {
        if let Some(f) = self.first.take() {
            return Some(f);
        }
        loop {
            match self.rx.recv() {
                Ok(Reply::Text(t)) => return Some(t),
                Ok(Reply::Confirm(_)) => continue,
                Err(_) => return None,
            }
        }
    }
    fn confirm(&mut self, lines: &[Line]) -> bool {
        *self.shared.confirm.lock().unwrap_or_else(|p| p.into_inner()) = Some(lines.to_vec());
        self.wake.wake();
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
        Composer { id, prompt: rt.signal(TextEditingValue::new("")), files: rt.signal(Vec::new()), menu: rt.signal(false), voice: rt.signal(Voice::Idle) }
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
        let joined = if cur.trim().is_empty() { text.to_string() } else { format!("{} {text}", cur.trim_end()) };
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
            let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
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
        Some(if notes.is_empty() { text } else { format!("{text}\n\nAttached: {}", notes.join(", ")) })
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
    shared: Arc<Shared>,
    last_poll: Rc<Cell<Instant>>,
    interviewing: Rc<RefCell<bool>>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Session({})", self.project.display())
    }
}

impl Session {
    pub fn open(rt: &vieww::element::Runtime, project: PathBuf, composer_id: u64) -> Session {
        let plan = load_plan(&project);
        let (events, result) = match last_run(&project) {
            Some(run) => (
                Store::open(&project).ok().and_then(|s| s.read_events(run).ok()).map(|l| l.events().to_vec()).unwrap_or_default(),
                load_result(&project, run),
            ),
            None => (Vec::new(), None),
        };
        Session {
            project: Rc::new(project),
            composer: Composer::new(rt, composer_id),
            chat: rt.signal(Vec::new()),
            confirm: rt.signal(None),
            plan: rt.signal(plan),
            events: rt.signal(events),
            running: rt.signal(false),
            result: rt.signal(result),
            approvals: rt.signal(Vec::new()),
            notice: rt.signal(String::new()),
            scroll: super::kit::ScrollState::new(rt),
            card_drag: rt.signal(0.0),
            shared: Arc::new(Shared::default()),
            last_poll: Rc::new(Cell::new(Instant::now())),
            interviewing: Rc::new(RefCell::new(false)),
        }
    }

    /// The name people see: the blueprint's, else the folder's.
    pub fn title(&self) -> String {
        match self.plan.get() {
            Some(p) if !p.name.is_empty() => p.name,
            _ => self.project.file_name().map(|n| n.to_string_lossy().replace('-', " ")).unwrap_or_else(|| "New session".into()),
        }
    }

    /// One line for lists and the status bar, and how it should be coloured.
    pub fn status(&self) -> (String, Tone) {
        let waiting = self.approvals.get().len();
        if waiting > 0 {
            return (format!("Waiting for you · {waiting} to approve"), Tone::Wait);
        }
        if self.running.get() {
            let progress = Progress::of(&self.events.get());
            let working: Vec<String> = progress.agents.iter().filter(|(_, (s, _))| *s == State::Working).map(|(n, _)| n.clone()).collect();
            return (if working.is_empty() { "Running".into() } else { format!("Running · {} working", working.join(", ")) }, Tone::Live);
        }
        if *self.interviewing.borrow() {
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
        let Some(plan) = self.plan.get() else { return 0.0 };
        let p = Progress::of(&self.events.get());
        let total = plan.cards().count().max(1);
        plan.cards().filter(|c| p.state(&c.name).settled()).count() as f32 / total as f32
    }

    pub fn live_agents(&self) -> usize {
        Progress::of(&self.events.get()).agents.values().filter(|(s, _)| matches!(s, State::Working | State::Waiting)).count()
    }

    /// Called before every frame: move worker results into signals.
    pub fn pump(&self) {
        let new_events = take(&self.shared.events);
        if !new_events.is_empty() {
            self.events.update(|v| v.extend(new_events));
        }
        let new_chat = take(&self.shared.chat);
        if !new_chat.is_empty() {
            self.chat.update(|c| c.extend(new_chat));
        }
        if let Some(lines) = self.shared.confirm.lock().unwrap_or_else(|p| p.into_inner()).take() {
            self.confirm.set(Some(lines));
        }
        if let Some(done) = self.shared.interview_done.lock().unwrap_or_else(|p| p.into_inner()).take() {
            *self.interviewing.borrow_mut() = false;
            match done {
                Ok(true) => {
                    self.plan.set(load_plan(&self.project));
                    self.notice.set(String::new());
                }
                Ok(false) => self.notice.set("Nothing was written. Tell me what to change.".into()),
                Err(e) => self.notice.set(format!("The interview stopped: {e}")),
            }
            self.confirm.set(None);
        }
        if let Some(fin) = self.shared.finished.lock().unwrap_or_else(|p| p.into_inner()).take() {
            self.running.set(false);
            match fin {
                Ok(r) => self.result.set(Some(r)),
                Err(e) => self.notice.set(format!("The run did not start: {e}")),
            }
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
                    Ok(store) => Box::new(StoreApprover { store, timeout: Duration::from_secs(1800) }),
                    Err(_) => Box::new(wright_engine::approve::AutoDeny),
                },
                listeners: vec![Box::new(move |e: &Event| {
                    s2.events.lock().unwrap_or_else(|p| p.into_inner()).push(e.clone());
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
        let Some(text) = self.composer.take_message(Some(&self.project)) else { return false };
        self.say(text);
        true
    }

    /// Send `text` as the user's message.
    pub fn say(&self, text: String) {
        self.chat.update(|c| c.push((true, text.clone())));
        // An interview already waiting for an answer gets this one.
        if let Some(tx) = self.shared.to_interview.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            if tx.send(Reply::Text(text.clone())).is_ok() {
                return;
            }
        }
        let (tx, rx) = channel::<Reply>();
        *self.shared.to_interview.lock().unwrap_or_else(|p| p.into_inner()) = Some(tx);
        *self.interviewing.borrow_mut() = true;
        let project = (*self.project).clone();
        let shared = Arc::clone(&self.shared);
        let wake = WakeHandle::current();
        std::thread::spawn(move || {
            let mut io = ChannelIo { first: Some(text), rx, shared: Arc::clone(&shared), wake: wake.clone() };
            let r = Config::load().and_then(|cfg| interview::interview(&project, &cfg, &mut io)).map(|bp| bp.is_some());
            *shared.to_interview.lock().unwrap_or_else(|p| p.into_inner()) = None;
            *shared.interview_done.lock().unwrap_or_else(|p| p.into_inner()) = Some(r);
            wake.wake();
        });
    }

    pub fn answer_confirm(&self, yes: bool) {
        self.confirm.set(None);
        if let Some(tx) = self.shared.to_interview.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            let _ = tx.send(Reply::Confirm(yes));
        }
    }

    pub fn answer_approval(&self, run: RunId, n: u32, yes: bool) {
        if let Ok(store) = Store::open(&self.project) {
            let _ = store.answer_approval(run, n, yes);
        }
        self.card_drag.set(0.0);
        self.approvals.update(|a| a.retain(|(r, k, _, _)| !(*r == run && *k == n)));
    }

    /// Spent today across runs (micro-USD), from the store's ledger.
    pub fn spent_today(&self) -> u64 {
        let day = wright_engine::firing::today_utc();
        Store::open(&self.project).map(|s| s.spent_on(&day)).unwrap_or(0)
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
