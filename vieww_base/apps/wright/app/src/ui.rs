//! The vieww shell: five screens, one engine.
//!
//! ```text
//!  Describe  ->  Plan  ->  Run  ->  Result        (the path everyone walks)
//!                                     Inspect     (the engine room, for developers)
//! ```
//!
//! Design rules (from the v0.2 product review):
//! * **Plan before machinery.** The Plan screen is the Confirm Gate: what
//!   will exist, what each step may do, what it costs, how "done" is
//!   checked — all generated from the blueprint by code. Stages, not a
//!   mandatory graph canvas; built-in helpers are labelled, not configured.
//! * **The result is a screen, not a log line.** Artifacts, test evidence
//!   and the Reviewer's verdict; a model saying "done" is never the proof.
//! * **Business by default, engineering on demand.** Spend, status and
//!   approvals are always visible; tokens, events and hashes live in Inspect.
//!
//! Threads: vieww's tree is single-threaded (signals are `!Send`). The
//! engine and the interview run on worker threads and hand results back
//! through [`Shared`]; a `before_frame` hook drains it into signals, and
//! the App's waker makes an idle window draw when something arrives.
//!
//! Headless: `wright ui <project> --snapshot <dir>` renders every screen
//! to PNG through the same widget tree (vieww's native rasterizer).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use vieww::element::Signal;
use vieww::foundation::task::FrameWaker;
use vieww::prelude::*;
use vieww::render::FrameDriver;
use vieww::widget::BuildContext as Ctx;

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

// ── palette (StudioTheme::dark values, so Wright sits beside viewwstudio) ──

const WINDOW: Color = Color::hex(0x0E0E0E);
const CHROME_0: Color = Color::hex(0x181818);
const CHROME_2: Color = Color::hex(0x1F1F1F);
const CHROME_3: Color = Color::hex(0x262626);
const CHROME_4: Color = Color::hex(0x2F2F2F);
const LINE: Color = Color::hex(0x2B2B2B);
const INK: Color = Color::hex(0xE6E6E6);
const MUTED: Color = Color::hex(0x9AA0A6);
const ACCENT: Color = Color::hex(0x8B7CF6);
const ACCENT_WASH: Color = Color::hex(0x2A2540);
const OK: Color = Color::hex(0x4EC98A);
const CACHED: Color = Color::hex(0x4EC9B0);
const WARN: Color = Color::hex(0xE0B341);
const BAD: Color = Color::hex(0xF06A6A);

// ── screens and view models ──────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Describe,
    Plan,
    Run,
    Result,
    Inspect,
}

impl Screen {
    const ALL: [Screen; 5] = [Screen::Describe, Screen::Plan, Screen::Run, Screen::Result, Screen::Inspect];
    fn label(self) -> &'static str {
        match self {
            Screen::Describe => "Describe",
            Screen::Plan => "Plan",
            Screen::Run => "Run",
            Screen::Result => "Result",
            Screen::Inspect => "Inspect",
        }
    }
    fn hint(self) -> &'static str {
        match self {
            Screen::Describe => "what should exist",
            Screen::Plan => "review before anything runs",
            Screen::Run => "live progress and approvals",
            Screen::Result => "what was made, and the proof",
            Screen::Inspect => "events, history, blueprint",
        }
    }
}

#[derive(Debug, Clone)]
struct Card {
    name: String,
    role: &'static str,
    model: String,
}

#[derive(Debug, Clone)]
struct PlanView {
    name: String,
    lines: Vec<Line>,
    stages: Vec<Vec<Card>>,
    budget: String,
    commit: String,
}

#[derive(Debug, Clone, Default)]
struct ResultView {
    run: RunId,
    ok: bool,
    headline: String,
    verdict: String,
    artifacts: Vec<(String, String, u64)>,
    tests: String,
    review: String,
    cost: String,
    receipt: String,
}

/// Everything the screens read. All signals: reading one in `build`
/// subscribes that widget, writing one rebuilds exactly the readers.
#[derive(Clone)]
struct Model {
    project: Rc<PathBuf>,
    screen: Signal<Screen>,
    prompt: Signal<TextEditingValue>,
    chat: Signal<Vec<(bool, String)>>,
    confirm: Signal<Option<Vec<Line>>>,
    plan: Signal<Option<PlanView>>,
    events: Signal<Vec<Event>>,
    running: Signal<bool>,
    result: Signal<Option<ResultView>>,
    approvals: Signal<Vec<(RunId, u32, String, String)>>,
    notice: Signal<String>,
    shared: Arc<Shared>,
    waker: Rc<dyn Fn()>,
}

impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Model")
    }
}

/// The worker-thread side of the bridge.
#[derive(Default)]
struct Shared {
    events: Mutex<Vec<Event>>,
    chat: Mutex<Vec<(bool, String)>>,
    confirm: Mutex<Option<Vec<Line>>>,
    finished: Mutex<Option<Result<ResultView, String>>>,
    interview_done: Mutex<Option<Result<bool, String>>>,
    /// The UI's answers to the interview thread.
    to_interview: Mutex<Option<Sender<Reply>>>,
}

enum Reply {
    Text(String),
    Confirm(bool),
}

// ── data loading (pure functions of the project on disk) ──────────

fn load_plan(project: &Path) -> Option<PlanView> {
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
                    Card { name: t.name(), role, model: t.model.as_ref().map(|m| m.primary.clone()).unwrap_or_default() }
                })
                .collect()
        })
        .collect();
    Some(PlanView {
        name: bp.meta.name.clone(),
        lines,
        stages,
        budget: format!("${:.2} per run, ${:.2} per day", bp.guardrails.per_firing_micro_usd as f64 / 1e6, bp.guardrails.daily_micro_usd as f64 / 1e6),
        commit: wright_core::hash::short(&wright_core::blueprint::canon::content_hash(&bp)).to_string(),
    })
}

fn load_result(project: &Path, run: RunId) -> Option<ResultView> {
    let store = Store::open(project).ok()?;
    let text = std::fs::read_to_string(store.run_dir(run).join("receipt.json")).ok()?;
    let v = json::parse(&text).ok()?;
    let mut r = ResultView {
        run,
        ok: v.get("ok").and_then(|b| b.as_bool()).unwrap_or(false),
        headline: v.str_of("headline").unwrap_or("").to_string(),
        verdict: v.str_of("verdict").unwrap_or("no review").to_string(),
        cost: format!("${:.4}", v.get("cost_micro_usd").and_then(|c| c.as_u64()).unwrap_or(0) as f64 / 1e6),
        receipt: store.run_dir(run).join("receipt.json").display().to_string(),
        ..ResultView::default()
    };
    for a in v.get("artifacts").and_then(|a| a.as_arr()).unwrap_or(&[]) {
        let name = a.str_of("name").unwrap_or("").to_string();
        let size = std::fs::metadata(project.join("artifacts").join(&name)).map(|m| m.len()).unwrap_or(0);
        r.artifacts.push((name, a.str_of("blake3").unwrap_or("").to_string(), size));
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

// ── the bridge: workers -> signals ───────────────────────────────

impl Model {
    fn new(driver: &mut FrameDriver, project: PathBuf, waker: Rc<dyn Fn()>) -> Model {
        let rt = driver.elements().runtime().clone();
        let plan = load_plan(&project);
        let (events, result) = match last_run(&project) {
            Some(run) => (
                Store::open(&project).ok().and_then(|s| s.read_events(run).ok()).map(|l| l.events().to_vec()).unwrap_or_default(),
                load_result(&project, run),
            ),
            None => (Vec::new(), None),
        };
        let screen = if plan.is_none() { Screen::Describe } else if result.is_some() { Screen::Result } else { Screen::Plan };
        Model {
            project: Rc::new(project),
            screen: rt.signal(screen),
            prompt: rt.signal(TextEditingValue::new("")),
            chat: rt.signal(Vec::new()),
            confirm: rt.signal(None),
            plan: rt.signal(plan),
            events: rt.signal(events),
            running: rt.signal(false),
            result: rt.signal(result),
            approvals: rt.signal(Vec::new()),
            notice: rt.signal(String::new()),
            shared: Arc::new(Shared::default()),
            waker,
        }
    }

    /// Called before every frame: move worker results into signals.
    fn pump(&self, last_approval_poll: &mut Instant) {
        let new_events: Vec<Event> = std::mem::take(&mut *self.shared.events.lock().unwrap_or_else(|p| p.into_inner()));
        if !new_events.is_empty() {
            self.events.update(|v| v.extend(new_events));
        }
        let new_chat: Vec<(bool, String)> = std::mem::take(&mut *self.shared.chat.lock().unwrap_or_else(|p| p.into_inner()));
        if !new_chat.is_empty() {
            self.chat.update(|c| c.extend(new_chat));
        }
        if let Some(lines) = self.shared.confirm.lock().unwrap_or_else(|p| p.into_inner()).take() {
            self.confirm.set(Some(lines));
        }
        if let Some(done) = self.shared.interview_done.lock().unwrap_or_else(|p| p.into_inner()).take() {
            match done {
                Ok(true) => {
                    self.plan.set(load_plan(&self.project));
                    self.screen.set(Screen::Plan);
                    self.notice.set("Blueprint confirmed and saved as blueprint.kdl.".into());
                }
                Ok(false) => self.notice.set("Nothing was written.".into()),
                Err(e) => self.notice.set(format!("Interview stopped: {e}")),
            }
            self.confirm.set(None);
        }
        if let Some(fin) = self.shared.finished.lock().unwrap_or_else(|p| p.into_inner()).take() {
            self.running.set(false);
            match fin {
                Ok(r) => {
                    self.result.set(Some(r));
                    self.screen.set(Screen::Result);
                }
                Err(e) => self.notice.set(format!("The firing did not start: {e}")),
            }
        }
        if self.running.get() && last_approval_poll.elapsed() > Duration::from_millis(400) {
            *last_approval_poll = Instant::now();
            if let Ok(store) = Store::open(&self.project) {
                let pending = store.pending_approvals();
                if pending != self.approvals.get() {
                    self.approvals.set(pending);
                }
            }
        }
    }

    fn start_run(&self) {
        if self.running.get() {
            return;
        }
        self.running.set(true);
        self.events.set(Vec::new());
        self.screen.set(Screen::Run);
        let project = (*self.project).clone();
        let shared = Arc::clone(&self.shared);
        let wake = waker_handle(&self.waker);
        std::thread::spawn(move || {
            let s2 = Arc::clone(&shared);
            let w2 = wake.clone();
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

    fn send_prompt(&self) {
        let text = self.prompt.get().text.trim().to_string();
        if text.is_empty() {
            return;
        }
        self.prompt.set(TextEditingValue::new(""));
        self.chat.update(|c| c.push((true, text.clone())));
        // An interview already running gets the reply; otherwise start one.
        if let Some(tx) = self.shared.to_interview.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            if tx.send(Reply::Text(text.clone())).is_ok() {
                return;
            }
        }
        let (tx, rx) = channel::<Reply>();
        *self.shared.to_interview.lock().unwrap_or_else(|p| p.into_inner()) = Some(tx);
        let project = (*self.project).clone();
        let shared = Arc::clone(&self.shared);
        let wake = waker_handle(&self.waker);
        std::thread::spawn(move || {
            let mut io = ChannelIo { first: Some(text), rx, shared: Arc::clone(&shared), wake: wake.clone() };
            let r = Config::load().and_then(|cfg| interview::interview(&project, &cfg, &mut io)).map(|bp| bp.is_some());
            *shared.to_interview.lock().unwrap_or_else(|p| p.into_inner()) = None;
            *shared.interview_done.lock().unwrap_or_else(|p| p.into_inner()) = Some(r);
            wake.wake();
        });
    }

    fn answer_confirm(&self, yes: bool) {
        self.confirm.set(None);
        if let Some(tx) = self.shared.to_interview.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            let _ = tx.send(Reply::Confirm(yes));
        }
    }

    fn answer_approval(&self, run: RunId, n: u32, yes: bool) {
        if let Ok(store) = Store::open(&self.project) {
            let _ = store.answer_approval(run, n, yes);
        }
        self.approvals.update(|a| a.retain(|(r, k, _, _)| !(*r == run && *k == n)));
    }
}

/// A Send-able wake handle for worker threads.
#[derive(Clone)]
struct WakeHandle(Option<vieww_platform_winit::Waker>);
impl WakeHandle {
    fn wake(&self) {
        if let Some(w) = &self.0 {
            w.wake();
        }
    }
}

thread_local! {
    static APP_WAKER: RefCell<Option<vieww_platform_winit::Waker>> = const { RefCell::new(None) };
}

fn waker_handle(_local: &Rc<dyn Fn()>) -> WakeHandle {
    WakeHandle(APP_WAKER.with_borrow(|w| w.clone()))
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

// ── small building blocks ────────────────────────────────────────

fn t(s: impl Into<String>, size: f32, color: Color) -> WidgetNode {
    Text::new(s).size(size).color(color).into()
}

fn b(s: impl Into<String>, size: f32, color: Color) -> WidgetNode {
    Text::new(s).size(size).color(color).bold().into()
}

/// A text cell that takes the remaining width of its row and ends in
/// an ellipsis instead of overflowing it.
fn fill(s: impl Into<String>, size: f32, color: Color) -> WidgetNode {
    Flexible::expanded(1)
        .child(Text::new(s).size(size).color(color).max_lines(1).overflow(vieww::text::TextOverflow::Ellipsis))
        .into()
}

/// Width for each of `n` cards sharing `total` pixels with `gap` between.
fn card_width(n: usize, total: f32, gap: f32) -> f32 {
    let n = n.max(1) as f32;
    ((total - gap * (n - 1.0)) / n).clamp(96.0, 190.0)
}

/// Usable width of the main area (window minus rail, padding, panel padding).
const MAIN_WIDTH: f32 = 1280.0 - 220.0 - 48.0 - 34.0;

fn panel(child: impl Into<WidgetNode>) -> WidgetNode {
    Container::new().color(CHROME_2).radius(10.0).border(Border::new(LINE, 1.0)).padding(EdgeInsets::all(16.0)).child(child).into()
}

fn pill(label: &str, fg: Color, bg: Color) -> WidgetNode {
    Container::new().color(bg).radius(9.0).padding(EdgeInsets::symmetric(8.0, 3.0)).child(t(label, 11.0, fg)).into()
}

fn button(label: &str, primary: bool, on_tap: impl Fn() + 'static) -> WidgetNode {
    let label = label.to_string();
    Pressable::new(move |emphasis: f32| {
        let (bg, fg) = if primary { (ACCENT, Color::hex(0x0E0E0E)) } else { (CHROME_4, INK) };
        let bg = if emphasis > 0.0 { bg.with_alpha(220) } else { bg };
        Container::new().color(bg).radius(8.0).padding(EdgeInsets::symmetric(16.0, 9.0)).child(b(label.clone(), 13.0, fg)).into()
    })
    .on_tap(on_tap)
    .into()
}

fn column(spacing: f32, items: Vec<WidgetNode>) -> WidgetNode {
    Flex::column().cross_axis_alignment(CrossAxisAlignment::Start).spacing(spacing).children(items).into()
}

fn row(spacing: f32, items: Vec<WidgetNode>) -> WidgetNode {
    Flex::row().cross_axis_alignment(CrossAxisAlignment::Center).spacing(spacing).children(items).into()
}

fn grow(child: impl Into<WidgetNode>) -> WidgetNode {
    Flexible::expanded(1).child(child).into()
}

fn status_color(status: &str) -> Color {
    match status {
        "done" => OK,
        "cached" => CACHED,
        "working" => ACCENT,
        "waiting for you" => WARN,
        "failed" => BAD,
        "skipped" => MUTED,
        _ => CHROME_4,
    }
}

/// Card states projected from the event list (the same projection
/// `wright replay` prints).
fn project_cards(events: &[Event]) -> (std::collections::BTreeMap<String, (String, String)>, u64) {
    let mut cards: std::collections::BTreeMap<String, (String, String)> = std::collections::BTreeMap::new();
    let mut spent = 0u64;
    for e in events {
        if e.kind == EventKind::TokensRecorded {
            spent += e.data_value().and_then(|d| d.get("cost_micro_usd").and_then(|c| c.as_u64())).unwrap_or(0);
        }
        let Some(a) = &e.agent else { continue };
        let st = match e.kind {
            EventKind::AgentStarted => "working",
            EventKind::AgentFinished => "done",
            EventKind::AgentFailed => "failed",
            EventKind::CacheHit => "cached",
            EventKind::TaskSkipped => "skipped",
            EventKind::ApprovalRequested => "waiting for you",
            EventKind::ApprovalResolved => "working",
            EventKind::ToolCalled | EventKind::ModelText | EventKind::Verdict => {
                if let Some((s, _)) = cards.get(a) {
                    let s: String = s.clone();
                    cards.insert(a.clone(), (s, e.note.clone()));
                }
                continue;
            }
            _ => continue,
        };
        cards.insert(a.clone(), (st.to_string(), e.note.clone()));
    }
    (cards, spent)
}

fn clip_text(s: &str, n: usize) -> String {
    let one: String = s.chars().map(|c| if c == '\n' { ' ' } else { c }).collect();
    if one.chars().count() > n {
        format!("{}...", one.chars().take(n).collect::<String>())
    } else {
        one
    }
}

// ── the shell ────────────────────────────────────────────────────

#[derive(Debug)]
struct Shell {
    m: Model,
}

impl vieww::widget::Widget for Shell {
    fn debug_name(&self) -> &'static str {
        "WrightShell"
    }
    fn kind(&self) -> vieww::widget::WidgetKind<'_> {
        vieww::widget::WidgetKind::Composed
    }
    fn build(&self, _ctx: &Ctx) -> WidgetNode {
        let m = &self.m;
        let screen = m.screen.get();
        let body = match screen {
            Screen::Describe => describe(m),
            Screen::Plan => plan(m),
            Screen::Run => run(m),
            Screen::Result => result(m),
            Screen::Inspect => inspect(m),
        };
        Container::new()
            .color(WINDOW)
            .child(Flex::column().children(children![
                top_bar(m),
                grow(Flex::row().cross_axis_alignment(CrossAxisAlignment::Stretch).children(children![
                    rail(m, screen),
                    grow(Container::new().padding(EdgeInsets::all(24.0)).child(body)),
                ])),
            ]))
            .into()
    }
}
vieww::widget::widget_node_from!(Shell);

fn top_bar(m: &Model) -> WidgetNode {
    let (_, spent) = project_cards(&m.events.get());
    let name = m.plan.get().map(|p| p.name).unwrap_or_else(|| "New project".into());
    let status = if m.running.get() {
        pill("RUNNING", Color::hex(0x0E0E0E), ACCENT)
    } else {
        match m.result.get() {
            Some(r) if r.ok => pill("LAST RUN OK", Color::hex(0x0E0E0E), OK),
            Some(_) => pill("LAST RUN STOPPED", Color::hex(0x0E0E0E), BAD),
            None => pill("NOT RUN YET", INK, CHROME_4),
        }
    };
    let waiting = m.approvals.get().len();
    let mut right = vec![t(format!("spent ${:.4}", spent as f64 / 1e6), 12.0, MUTED)];
    if waiting > 0 {
        right.push(pill(&format!("{waiting} WAITING FOR YOU"), Color::hex(0x0E0E0E), WARN));
    }
    right.push(status);
    Container::new()
        .color(CHROME_0)
        .border(Border::new(LINE, 1.0))
        .padding(EdgeInsets::symmetric(18.0, 12.0))
        .child(Flex::row().cross_axis_alignment(CrossAxisAlignment::Center).spacing(14.0).children(children![
            b("wright", 17.0, ACCENT),
            t("|", 14.0, CHROME_4),
            b(name, 14.0, INK),
            t(m.project.display().to_string(), 11.0, MUTED),
            grow(SizedBox::new()),
            row(12.0, right),
        ]))
        .into()
}

fn rail(m: &Model, current: Screen) -> WidgetNode {
    let items: Vec<WidgetNode> = Screen::ALL
        .iter()
        .map(|&s| {
            let screen = m.screen.clone();
            let active = s == current;
            Pressable::new(move |_e: f32| {
                Container::new()
                    .color(if active { ACCENT_WASH } else { CHROME_0 })
                    .radius(8.0)
                    .padding(EdgeInsets::symmetric(12.0, 9.0))
                    .child(column(2.0, vec![b(s.label(), 14.0, if active { INK } else { MUTED }), t(s.hint(), 10.5, MUTED)]))
                    .into()
            })
            .on_tap(move || screen.set(s))
            .into()
        })
        .collect();
    Container::new()
        .color(CHROME_0)
        .padding(EdgeInsets::all(12.0))
        .child(SizedBox::width(196.0).child(column(6.0, items)))
        .into()
}

// ── Describe ─────────────────────────────────────────────────────

fn describe(m: &Model) -> WidgetNode {
    let mut items = vec![
        b("What should exist?", 26.0, INK),
        t("Describe the outcome in your own words. Wright drafts a plan; nothing runs until you approve it.", 13.0, MUTED),
    ];
    let prompt = m.prompt.clone();
    let field = TextField::new(m.prompt.get())
        .placeholder("e.g. Every morning at 8, fetch the top Hacker News stories, summarize them, and email me the digest")
        .size(15.0)
        .color(INK)
        .cursor(ACCENT, 2.0)
        .on_changed(Rc::new(move |v: TextEditingValue| prompt.set(v)));
    let mm = m.clone();
    items.push(
        Container::new()
            .color(CHROME_2)
            .radius(10.0)
            .border(Border::new(ACCENT, 1.0))
            .padding(EdgeInsets::all(14.0))
            .child(column(12.0, vec![SizedBox::height(64.0).child(field).into(), row(10.0, vec![grow(SizedBox::new()), button("Draft the plan", true, move || mm.send_prompt())])]))
            .into(),
    );
    let starters = [
        "Daily digest: fetch, summarize, email",
        "A Python CLI tool with tests and CI",
        "Weekly report from a folder of CSV files",
    ];
    let chips: Vec<WidgetNode> = starters
        .iter()
        .map(|s| {
            let prompt = m.prompt.clone();
            let s = s.to_string();
            Pressable::new({
                let s = s.clone();
                move |_e: f32| pill(&s, INK, CHROME_3)
            })
            .on_tap(move || prompt.set(TextEditingValue::new(s.clone())))
            .into()
        })
        .collect();
    items.push(row(8.0, std::iter::once(t("Start from:", 12.0, MUTED)).chain(chips).collect()));

    let chat = m.chat.get();
    if !chat.is_empty() {
        let lines: Vec<WidgetNode> = chat
            .iter()
            .rev()
            .take(8)
            .rev()
            .map(|(user, text)| {
                Flex::row()
                    .spacing(10.0)
                    .children(children![b(if *user { "you" } else { "wright" }, 12.0, if *user { MUTED } else { ACCENT }), fill(text.clone(), 13.0, INK)])
                    .into()
            })
            .collect();
        items.push(panel(column(8.0, lines)));
    }
    if let Some(lines) = m.confirm.get() {
        let (y, n) = (m.clone(), m.clone());
        let mut rows: Vec<WidgetNode> = vec![b("Confirm - this is exactly what will exist", 15.0, INK), t("Generated from the blueprint by Wright, not written by a model.", 11.5, MUTED)];
        rows.extend(lines.iter().take(14).map(confirm_line));
        rows.push(row(10.0, vec![button("Build this", true, move || y.answer_confirm(true)), button("Change something", false, move || n.answer_confirm(false))]));
        items.push(panel(column(6.0, rows)));
    }
    let notice = m.notice.get();
    if !notice.is_empty() {
        items.push(t(notice, 12.0, WARN));
    }
    column(16.0, items)
}

fn confirm_line(l: &Line) -> WidgetNode {
    Flex::row()
        .spacing(8.0)
        .children(children![
            t(if l.attention { "!" } else { "-" }, 13.0, if l.attention { WARN } else { MUTED }),
            fill(l.text.clone(), 13.0, if l.attention { INK } else { Color::hex(0xC8C8C8) }),
        ])
        .into()
}

// ── Plan ─────────────────────────────────────────────────────────

fn plan(m: &Model) -> WidgetNode {
    let Some(p) = m.plan.get() else {
        return column(12.0, vec![b("No blueprint yet", 22.0, INK), t("Describe the project first; the plan appears here for you to approve.", 13.0, MUTED)]);
    };
    let what: Vec<WidgetNode> = p.lines.iter().take(22).map(confirm_line).collect();
    let w = card_width(p.stages.len(), MAIN_WIDTH, 14.0);
    let stages: Vec<WidgetNode> = p
        .stages
        .iter()
        .enumerate()
        .map(|(i, stage)| {
            let cards: Vec<WidgetNode> = stage
                .iter()
                .map(|c| {
                    let accent = match c.role {
                        "delivers" => WARN,
                        "live data" => CACHED,
                        "built-in" => MUTED,
                        "helper" => Color::hex(0x6E8BD6),
                        _ => ACCENT,
                    };
                    SizedBox::width(w)
                        .child(
                            Container::new()
                                .color(CHROME_3)
                                .radius(8.0)
                                .border(Border::new(accent, 1.0))
                                .padding(EdgeInsets::all(10.0))
                                .child(column(3.0, vec![b(c.name.clone(), 13.0, INK), t(c.role, 10.5, accent), t(clip_text(if c.model.is_empty() { "no model" } else { &c.model }, ((w - 20.0) / 6.2) as usize), 10.0, MUTED)])),
                        )
                        .into()
                })
                .collect();
            column(6.0, std::iter::once(t(format!("STEP {}", i + 1), 10.0, MUTED)).chain(cards).collect())
        })
        .collect();
    let mm = m.clone();
    let running = m.running.get();
    column(
        16.0,
        vec![
            row(12.0, vec![b("Review the plan", 24.0, INK), pill(&format!("blueprint {}", p.commit), MUTED, CHROME_3), grow(SizedBox::new()), button(if running { "Running..." } else { "Approve & run" }, true, move || mm.start_run())]),
            t(format!("Budget: {}. Nothing runs until you press Approve & run.", p.budget), 12.5, MUTED),
            panel(column(6.0, std::iter::once(b("What will happen", 14.0, INK)).chain(what).collect())),
            panel(column(10.0, vec![b("Order of work (steps run left to right; cards in one step run at the same time)", 13.0, INK), row(14.0, stages)])),
        ],
    )
}

// ── Run ──────────────────────────────────────────────────────────

fn run(m: &Model) -> WidgetNode {
    let events = m.events.get();
    let (cards, spent) = project_cards(&events);
    let mut items = vec![row(12.0, vec![b(if m.running.get() { "Running" } else { "Last run" }, 24.0, INK), t(format!("spent ${:.4}", spent as f64 / 1e6), 13.0, MUTED)])];
    for (run, n, agent, reason) in m.approvals.get() {
        let (y, no) = (m.clone(), m.clone());
        items.push(
            Container::new()
                .color(Color::hex(0x2E2716))
                .radius(10.0)
                .border(Border::new(WARN, 1.0))
                .padding(EdgeInsets::all(14.0))
                .child(column(8.0, vec![
                    b(format!("{agent} is asking before it touches the world"), 14.0, WARN),
                    t(clip_text(&reason, 220), 12.5, INK),
                    row(10.0, vec![button("Allow", true, move || y.answer_approval(run, n, true)), button("Deny", false, move || no.answer_approval(run, n, false))]),
                ]))
                .into(),
        );
    }
    if let Some(p) = m.plan.get() {
        let w = card_width(p.stages.len(), MAIN_WIDTH, 12.0);
        let chars = ((w - 20.0) / 5.6) as usize;
        let cols: Vec<WidgetNode> = p
            .stages
            .iter()
            .map(|stage| {
                column(
                    8.0,
                    stage
                        .iter()
                        .map(|c| {
                            let (st, note) = cards.get(&c.name).cloned().unwrap_or_else(|| ("waiting".into(), String::new()));
                            let col = status_color(&st);
                            SizedBox::width(w)
                                .child(
                                    Container::new()
                                        .color(CHROME_3)
                                        .radius(8.0)
                                        .border(Border::new(col, if st == "working" { 2.0 } else { 1.0 }))
                                        .padding(EdgeInsets::all(10.0))
                                        .child(column(4.0, vec![b(c.name.clone(), 13.0, INK), t(st.to_uppercase(), 10.0, col), t(clip_text(&note, chars), 10.0, MUTED)])),
                                )
                                .into()
                        })
                        .collect(),
                )
            })
            .collect();
        items.push(panel(row(12.0, cols)));
    }
    let river: Vec<WidgetNode> = events
        .iter()
        .rev()
        .take(12)
        .rev()
        .map(|e| {
            let col = match e.kind {
                EventKind::AgentFailed | EventKind::RunFailed | EventKind::ToolDenied => BAD,
                EventKind::CacheHit => CACHED,
                EventKind::AgentFinished | EventKind::RunFinished => OK,
                EventKind::ApprovalRequested | EventKind::Verdict => WARN,
                _ => MUTED,
            };
            Flex::row()
                .spacing(10.0)
                .children(children![
                    SizedBox::width(34.0).child(t(e.seq.to_string(), 11.0, CHROME_4)),
                    SizedBox::width(96.0).child(b(e.agent.clone().unwrap_or_else(|| "wright".into()), 11.5, col)),
                    fill(e.note.clone(), 11.5, INK),
                ])
                .into()
        })
        .collect();
    items.push(panel(column(5.0, std::iter::once(b("Event river (newest last)", 13.0, INK)).chain(river).collect())));
    column(14.0, items)
}

// ── Result ───────────────────────────────────────────────────────

fn result(m: &Model) -> WidgetNode {
    let Some(r) = m.result.get() else {
        return column(12.0, vec![b("No result yet", 22.0, INK), t("Run the plan; what it made, and the proof that it works, appears here.", 13.0, MUTED)]);
    };
    let (fg, label) = if r.ok { (OK, "DELIVERED AND VERIFIED") } else { (BAD, "STOPPED - NOTHING UNVERIFIED WAS DELIVERED") };
    let arts: Vec<WidgetNode> = if r.artifacts.is_empty() {
        vec![t("No artifacts.", 12.5, MUTED)]
    } else {
        r.artifacts
            .iter()
            .map(|(n, h, size)| row(12.0, vec![SizedBox::width(200.0).child(b(n.clone(), 13.0, INK)).into(), SizedBox::width(90.0).child(t(format!("{size} bytes"), 12.0, MUTED)).into(), t(format!("blake3 {}", wright_core::hash::short(h)), 11.5, MUTED)]))
            .collect()
    };
    let evidence = |title: &str, text: &str| -> WidgetNode {
        let lines: Vec<WidgetNode> = text.lines().filter(|l| !l.trim().is_empty()).take(6).map(|l| t(clip_text(l, 120), 12.0, INK)).collect();
        panel(column(6.0, std::iter::once(b(title, 13.0, INK)).chain(if lines.is_empty() { vec![t("(not run)", 12.0, MUTED)] } else { lines }).collect()))
    };
    column(
        14.0,
        vec![
            row(12.0, vec![b(format!("Run {}", r.run), 24.0, INK), pill(label, Color::hex(0x0E0E0E), fg)]),
            t(clip_text(&r.headline, 160), 13.0, MUTED),
            panel(column(8.0, std::iter::once(b("What was made", 14.0, INK)).chain(arts).collect())),
            row(14.0, vec![grow(evidence("Test evidence (Testwright)", &r.tests)), grow(evidence("Review (Reviewer)", &r.review))]),
            t(format!("Verdict: {}   |   cost {}   |   receipt {}", r.verdict, r.cost, clip_text(&r.receipt, 70)), 11.5, MUTED),
        ],
    )
}

// ── Inspect ──────────────────────────────────────────────────────

fn inspect(m: &Model) -> WidgetNode {
    let events = m.events.get();
    let rows: Vec<WidgetNode> = events
        .iter()
        .rev()
        .take(22)
        .rev()
        .map(|e| {
            Flex::row()
                .spacing(10.0)
                .children(children![
                    SizedBox::width(34.0).child(t(e.seq.to_string(), 11.0, MUTED)),
                    SizedBox::width(130.0).child(t(e.kind.as_str(), 11.0, ACCENT)),
                    SizedBox::width(90.0).child(t(e.agent.clone().unwrap_or_default(), 11.0, INK)),
                    fill(e.note.clone(), 11.0, Color::hex(0xC8C8C8)),
                ])
                .into()
        })
        .collect();
    let history: Vec<WidgetNode> = Store::open(&m.project)
        .map(|s| s.blueprint_history())
        .unwrap_or_default()
        .iter()
        .rev()
        .take(6)
        .map(|c| t(format!("v{}  {}  {}", c.seq, wright_core::hash::short(&c.hash), clip_text(&c.message, 40)), 11.5, INK))
        .collect();
    let store_dir = Store::open(&m.project).map(|s| s.root().display().to_string()).unwrap_or_default();
    column(
        14.0,
        vec![
            row(12.0, vec![b("Inspect", 24.0, INK), t("the engine room: every event, every version, where it all lives", 12.5, MUTED)]),
            Flex::row().cross_axis_alignment(CrossAxisAlignment::Start).spacing(14.0).children(vec![
                grow(panel(column(4.0, std::iter::once(b(format!("Events ({} in this run, newest last)", events.len()), 13.0, INK)).chain(rows).collect()))),
                SizedBox::width(300.0)
                    .child(panel(column(6.0, std::iter::once(b("Blueprint versions", 13.0, INK)).chain(history).chain([t("State lives outside the project:", 11.0, MUTED), t(clip_text(&store_dir, 46), 10.5, MUTED)]).collect())))
                    .into(),
            ]).into(),
        ],
    )
}

// ── entry points ─────────────────────────────────────────────────

pub fn main(args: &[String]) -> i32 {
    let mut project: Option<PathBuf> = None;
    let mut snapshot: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--snapshot" {
            snapshot = args.get(i + 1).map(PathBuf::from);
            i += 2;
        } else {
            project = Some(crate::cmd::project_dir(Some(&args[i])));
            i += 1;
        }
    }
    let project = project.unwrap_or_else(|| PathBuf::from("."));
    let project = project.canonicalize().unwrap_or(project);
    match snapshot {
        Some(dir) => match snapshots(&project, &dir) {
            Ok(n) => {
                println!("wrote {n} screens to {}", dir.display());
                0
            }
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },
        None => window(project),
    }
}

const SIZE: (f32, f32) = (1280.0, 800.0);

fn window(project: PathBuf) -> i32 {
    let app = vieww_platform_winit::App::new().title("Wright").size(Size::new(SIZE.0, SIZE.1));
    APP_WAKER.with_borrow_mut(|w| *w = Some(app.waker()));
    let model: Rc<RefCell<Option<Model>>> = Rc::new(RefCell::new(None));
    let for_hook = Rc::clone(&model);
    let mut last_poll = Instant::now();
    let result = app
        .before_frame(move |_driver| {
            if let Some(m) = for_hook.borrow().as_ref() {
                m.pump(&mut last_poll);
            }
        })
        .run(move |driver| {
            let m = Model::new(driver, project, Rc::new(|| {}));
            *model.borrow_mut() = Some(m.clone());
            driver.set_root(Shell { m });
        });
    match result {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("error: could not open a window ({e}). Use the CLI (`wright help`), or `wright ui --snapshot <dir>` for pictures.");
            1
        }
    }
}

/// Render every screen of `project` to PNGs, headless.
fn snapshots(project: &Path, dir: &Path) -> Result<usize, String> {
    use vieww::paint::native::NativeRenderer;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let size = Size::new(SIZE.0, SIZE.1);
    let mut driver = FrameDriver::new(size);
    let m = Model::new(&mut driver, project.to_path_buf(), Rc::new(|| {}));
    if m.chat.get().is_empty() {
        let intent = wright_engine::load_blueprint(project).map(|b| b.intent.summary).unwrap_or_default();
        m.chat.set(vec![(true, intent), (false, "I have drafted the blueprint. Wright will now show you exactly what it will do.".into())]);
    }
    driver.set_root(Shell { m: m.clone() });
    let mut renderer = NativeRenderer::new();
    let mut n = 0;
    for (i, s) in Screen::ALL.iter().enumerate() {
        m.screen.set(*s);
        driver.draw_frame_at(Duration::from_millis(16 * (i as u64 + 1)));
        driver.draw_frame_at(Duration::from_millis(16 * (i as u64 + 1) + 8));
        let (png, _report) = renderer
            .render_to_png(driver.scene(), SIZE.0 as u32, SIZE.1 as u32, WINDOW)
            .map_err(|e| format!("render: {e}"))?;
        let path = dir.join(format!("{}-{}.png", i + 1, s.label().to_lowercase()));
        std::fs::write(&path, png).map_err(|e| e.to_string())?;
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    //! Drives the real UI model headlessly: the same signals, the same
    //! worker bridge, the same widget tree — no window, no clicks.
    use super::*;
    use vieww::paint::native::NativeRenderer;

    #[test]
    fn plan_to_approval_to_result_through_the_ui_model() {
        let home = std::env::temp_dir().join(format!("wright-ui-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("WRIGHT_HOME", &home);
        let mock = wright_engine::mock::MockProvider::start(0).unwrap();
        std::fs::write(home.join("config.kdl"), format!("provider \"mock\" base=\"{}\" models=\"*\"\n", mock.base_url())).unwrap();
        std::env::set_var("WRIGHT_ALLOW_PRIVATE_NETWORK", "1");

        // A one-agent project that sends (needs approval) to a local sink.
        let sink = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = sink.local_addr().unwrap().port();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            for s in sink.incoming().flatten() {
                let mut s = s;
                let mut buf = [0u8; 65536];
                let _ = s.read(&mut buf);
                let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
            }
        });
        let dir = std::env::temp_dir().join(format!("wright-ui-proj-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("blueprint.kdl"),
            format!(
                "meta {{ name \"UI drive\" }}\nintent {{ summary \"send a note\" confirmed \"v1\" }}\n\
agent Notifier {{ goal \"Send a hello to http://127.0.0.1:{port}/hook\" tools \"http-post\" approval \"before-send\" permissions network=true }}\n\
deliverable kind=\"message\" {{ check \"the hook got a hello\" }}\nmodels {{ tier cheap prefer=\"mock-1\" fallback=\"\" tier quality prefer=\"mock-1\" fallback=\"\" }}\n"
            ),
        )
        .unwrap();

        let mut driver = FrameDriver::new(Size::new(SIZE.0, SIZE.1));
        let m = Model::new(&mut driver, dir.canonicalize().unwrap(), Rc::new(|| {}));
        driver.set_root(Shell { m: m.clone() });
        assert_eq!(m.screen.get(), Screen::Plan, "a project with a blueprint opens on its plan");

        m.start_run();
        assert_eq!(m.screen.get(), Screen::Run);
        let mut poll = Instant::now() - Duration::from_secs(1);
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut saw_banner = false;
        let mut frame = 0u64;
        while m.result.get().is_none() && Instant::now() < deadline {
            m.pump(&mut poll);
            frame += 1;
            driver.draw_frame_at(Duration::from_millis(frame * 16));
            if let Some((run, n, agent, _)) = m.approvals.get().first().cloned() {
                assert_eq!(agent, "Notifier");
                if !saw_banner {
                    saw_banner = true;
                    // The approval banner, rendered.
                    let (png, _) = NativeRenderer::new().render_to_png(driver.scene(), SIZE.0 as u32, SIZE.1 as u32, WINDOW).unwrap();
                    let out = std::env::var("WRIGHT_UI_SHOTS").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
                    std::fs::write(out.join("run-approval.png"), png).unwrap();
                }
                m.answer_approval(run, n, true);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(saw_banner, "the approval request reached the UI");
        let r = m.result.get().expect("the firing finished");
        assert!(r.ok, "{}", r.headline);
        assert_eq!(m.screen.get(), Screen::Result, "a finished firing lands on its result");
        assert!(m.events.get().iter().any(|e| e.kind == EventKind::ApprovalResolved && e.note.contains("approved")));
    }
}
