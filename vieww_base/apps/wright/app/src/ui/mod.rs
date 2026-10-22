//! The vieww shell.
//!
//! ```text
//!   splash: the W draws itself, grows, softens, and settles behind the app
//!   ┌───────────────────────────────────────────────────────────────┐
//!   │ wright      HN Daily Digest  ● ● ━ ○     Sessions Engine ⚙ ?  │  top bar
//!   │                                                               │
//!   │  [Home] [session] [session] [new]   <- a row of full pages;   │  stage
//!   │           two fingers sideways moves along it; pinch / O      │
//!   │           zooms out to all of them; Space peeks at the agents │
//!   │                                                               │
//!   │ ▌wright  HN Daily Digest  ● Running · Fetcher working   Dark ⚙│  status bar
//!   └───────────────────────────────────────────────────────────────┘
//! ```
//!
//! Files: `theme` (colours), `logo` (the mark), `kit` (building blocks),
//! `session` (one project's state and its worker bridge), `shell` (the
//! frame: bars, stage, overview, overlays), `thread` (a session's page),
//! `home` (Home and new-session pages), `engine` (the live agents),
//! `settings`, `capture` (microphone), `picker` (file dialog), `store`
//! (what the window remembers).
//!
//! Headless: `wright ui <project> --snapshot <dir>` renders the screens to
//! PNG through the same widget tree; `wright ui --icon <png>` renders the
//! app icon the installers ship.

mod capture;
mod engine;
mod home;
mod kit;
mod logo;
mod picker;
mod session;
mod settings;
mod shell;
mod store;
mod theme;
mod thread;

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use vieww::foundation::task::FrameWaker;
use vieww::foundation::{KeyEvent, LogicalKey, NamedKey, ScrollEvent, TargetPlatform};
use vieww::prelude::*;
use vieww::render::FrameDriver;

use session::{Composer, Session, Voice, WakeHandle};
use theme::{Mode, Pal};

thread_local! {
    static APP_WAKER: RefCell<Option<vieww_platform_winit::Waker>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineView {
    Hidden,
    /// While Space is held.
    Peek,
    Open,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub text: String,
    /// A closed session that Undo brings back.
    pub undo: Option<PathBuf>,
    pub until: Instant,
}

/// Results that come back from worker threads (transcripts, file picks),
/// addressed to a composer by id.
#[derive(Default)]
struct Inbox {
    voice: Mutex<Vec<(u64, Result<String, String>)>>,
    picked: Mutex<Vec<Picked>>,
}

/// A file-dialog answer for composer `.0`.
type Picked = (u64, Result<Vec<PathBuf>, String>);

#[derive(Debug, Clone, Copy, Default)]
struct Swipe {
    /// When the last sideways wheel event arrived.
    last: Option<Instant>,
    /// Ignore sideways scrolling until then (a trackpad keeps sending
    /// momentum after the fingers lift; one swipe must move one page).
    lock_until: Option<Instant>,
}

struct Inner {
    started: Instant,
    swipe: Cell<Swipe>,
    next_id: Cell<u64>,
    recording: RefCell<Option<(u64, capture::Recording)>>,
    inbox: Arc<Inbox>,
    live: Arc<AtomicBool>,
    /// Page widgets, rebuilt only when the list of sessions changes, so a
    /// swipe moves the same widgets instead of making new ones every frame.
    pages: RefCell<(Vec<PathBuf>, Vec<WidgetNode>)>,
    /// The focused object last frame (to notice focus moving).
    last_focus: Cell<Option<vieww::RenderId>>,
    /// A pinch in progress: how far the fingers have moved in total, when
    /// they last moved, and whether it has already done its one thing.
    pinch: Cell<(f32, Option<Instant>, bool)>,
}

/// Everything the shell reads. Signals: reading one in a build subscribes
/// that region; writing one rebuilds exactly the readers.
#[derive(Clone)]
pub struct Ui {
    pub rt: vieww::element::Runtime,
    pub mode: Signal<Mode>,
    pub accent: Signal<Color>,
    pub sessions: Signal<Vec<Session>>,
    pub saved: Signal<Vec<PathBuf>>,
    /// 0 = Home, 1..=n = sessions, n + 1 = a new session.
    pub page: Signal<usize>,
    /// Sideways finger offset while swiping, in pixels.
    pub drag: Signal<f32>,
    pub dragging: Signal<bool>,
    pub overview: Signal<bool>,
    pub engine: Signal<EngineView>,
    pub selected: Signal<Option<String>>,
    pub settings: Signal<bool>,
    pub help: Signal<bool>,
    pub toast: Signal<Option<Toast>>,
    /// Seconds since start, ticked only while something is live.
    pub clock: Signal<f32>,
    pub splash: Signal<bool>,
    pub mic_level: Signal<f32>,
    /// The placeholder of the text field that has keyboard focus, if any.
    /// A vieww field draws its caret unless told not to, and does not know
    /// whether it is focused; the shell does, so it tells each field.
    pub typing: Signal<Option<String>>,
    pub home: Composer,
    pub fresh: Composer,
    pub form: settings::Form,
    pub home_scroll: kit::ScrollState,
    pub engine_scroll: kit::ScrollState,
    pub size: Rc<Cell<Size>>,
    inner: Rc<Inner>,
}

impl std::fmt::Debug for Ui {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Ui")
    }
}

impl Ui {
    pub fn new(rt: &vieww::element::Runtime, project: Option<PathBuf>, live: Arc<AtomicBool>) -> Ui {
        let (mode, accent) = theme::load();
        let mut open = store::read_list("ui-open");
        let mut saved = store::read_list("ui-saved");
        let mut page = 0;
        if let Some(p) = project {
            if let Some(i) = open.iter().position(|o| o == &p) {
                page = i + 1;
            } else {
                open.insert(0, p.clone());
                page = 1;
            }
            saved.retain(|s| s != &p);
        }
        saved.retain(|s| !open.contains(s));
        let next_id = Cell::new(10);
        let id = || {
            let v = next_id.get();
            next_id.set(v + 1);
            v
        };
        let sessions: Vec<Session> = open.iter().map(|p| Session::open(rt, p.clone(), id())).collect();
        let ui = Ui {
            rt: rt.clone(),
            mode: rt.signal(mode),
            accent: rt.signal(accent),
            sessions: rt.signal(sessions),
            saved: rt.signal(saved),
            page: rt.signal(page),
            drag: rt.signal(0.0),
            dragging: rt.signal(false),
            overview: rt.signal(false),
            engine: rt.signal(EngineView::Hidden),
            selected: rt.signal(None),
            settings: rt.signal(false),
            help: rt.signal(false),
            toast: rt.signal(None),
            clock: rt.signal(0.0),
            splash: rt.signal(true),
            mic_level: rt.signal(0.0),
            typing: rt.signal(None),
            home: Composer::new(rt, 1),
            fresh: Composer::new(rt, 2),
            form: settings::Form::new(rt),
            home_scroll: kit::ScrollState::new(rt),
            engine_scroll: kit::ScrollState::new(rt),
            size: Rc::new(Cell::new(Size::new(1280.0, 800.0))),
            inner: Rc::new(Inner {
                started: Instant::now(),
                swipe: Cell::new(Swipe::default()),
                next_id,
                recording: RefCell::new(None),
                inbox: Arc::new(Inbox::default()),
                live,
                pages: RefCell::new((Vec::new(), Vec::new())),
                last_focus: Cell::new(None),
                pinch: Cell::new((0.0, None, false)),
            }),
        };
        ui.persist();
        // Headless (snapshots, tests): a pasteboard only this process sees.
        // The window replaces it with the system one right after.
        kit::init_copy(rt, Rc::new(vieww::foundation::MemoryClipboard::new()));
        ui
    }

    /// Whether the focused text field is the one with this placeholder.
    pub fn is_typing_in(&self, placeholder: &str) -> bool {
        self.typing.get().as_deref() == Some(placeholder)
    }

    pub fn pal(&self) -> Pal {
        theme::palette(self.mode.get(), self.accent.get())
    }

    fn new_id(&self) -> u64 {
        let v = self.inner.next_id.get();
        self.inner.next_id.set(v + 1);
        v
    }

    pub fn page_count(&self) -> usize {
        self.sessions.get().len() + 2
    }

    pub fn current_session(&self) -> Option<Session> {
        let page = self.page.get();
        let sessions = self.sessions.get();
        if page >= 1 && page <= sessions.len() {
            Some(sessions[page - 1].clone())
        } else {
            None
        }
    }

    /// The composer on the page in view (where dropped files go).
    pub fn current_composer(&self) -> Composer {
        let page = self.page.get();
        match self.current_session() {
            Some(s) => s.composer,
            None if page == 0 => self.home.clone(),
            None => self.fresh.clone(),
        }
    }

    fn composer_by_id(&self, id: u64) -> Option<Composer> {
        if self.home.id == id {
            return Some(self.home.clone());
        }
        if self.fresh.id == id {
            return Some(self.fresh.clone());
        }
        self.sessions.get().into_iter().map(|s| s.composer).find(|c| c.id == id)
    }

    /// The page widgets, one per page, rebuilt only when sessions change.
    pub fn pages(&self) -> Vec<WidgetNode> {
        let sessions = self.sessions.get();
        let key: Vec<PathBuf> = sessions.iter().map(|s| (*s.project).clone()).collect();
        let mut cache = self.inner.pages.borrow_mut();
        if cache.0 != key || cache.1.is_empty() {
            let mut nodes = vec![home::home(self)];
            nodes.extend(sessions.iter().map(|s| thread::page(self, s)));
            nodes.push(home::fresh(self));
            *cache = (key, nodes);
        }
        cache.1.clone()
    }

    // ── navigation ───────────────────────────────────────────────

    pub fn go(&self, page: usize) {
        let page = page.min(self.page_count() - 1);
        self.drag.set(0.0);
        self.dragging.set(false);
        self.selected.set(None);
        if page != self.page.get() {
            self.page.set(page);
        }
    }

    pub fn set_engine(&self, v: EngineView) {
        if v != EngineView::Hidden && self.overview.get() {
            self.overview.set(false);
        }
        if v == EngineView::Hidden {
            self.selected.set(None);
        }
        self.engine.set(v);
    }

    pub fn set_overview(&self, on: bool) {
        if on {
            self.engine.set(EngineView::Hidden);
        }
        self.overview.set(on);
    }

    pub fn set_look(&self, mode: Mode, accent: Color) {
        if mode != self.mode.get() {
            self.mode.set(mode);
        }
        if accent != self.accent.get() {
            self.accent.set(accent);
        }
        theme::save(mode, accent);
    }

    pub fn open_settings(&self) {
        self.form.reload(self.current_session().map(|s| (*s.project).clone()).as_deref());
        self.form.message.set(String::new());
        self.settings.set(true);
    }

    /// Settings, on the page that lists Wright's agents and tools.
    pub fn open_agents_and_tools(&self) {
        self.help.set(false);
        self.open_settings();
        self.form.tab.set(settings::Tab::AgentsTools);
        self.form.scroll.offset.set(0.0);
    }

    fn say(&self, text: impl Into<String>, undo: Option<PathBuf>) {
        self.toast.set(Some(Toast { text: text.into(), undo, until: Instant::now() + Duration::from_secs(6) }));
    }

    // ── sessions ─────────────────────────────────────────────────

    fn persist(&self) {
        let open: Vec<PathBuf> = self.sessions.get().iter().map(|s| (*s.project).clone()).collect();
        store::write_list("ui-open", &open);
        store::write_list("ui-saved", &self.saved.get());
    }

    /// Home's or the new-session page's composer: make a folder, open a
    /// session on it, and send the description.
    pub fn start_session_from(&self, c: &Composer) {
        let text = c.prompt.get().text;
        if text.trim().is_empty() && c.files.get().is_empty() {
            return;
        }
        let dir = match store::new_project_dir(&text) {
            Ok(d) => d,
            Err(e) => return self.say(e, None),
        };
        let Some(message) = c.take_message(Some(&dir)) else { return };
        let s = Session::open(&self.rt, dir, self.new_id());
        self.sessions.update(|v| v.push(s.clone()));
        self.persist();
        self.go(self.sessions.get().len());
        s.say(message);
    }

    /// Close session `i` (0-based) into Saved. Nothing stops: its history,
    /// schedules and folder stay exactly as they were.
    pub fn close_session(&self, i: usize) {
        let sessions = self.sessions.get();
        let Some(s) = sessions.get(i) else { return };
        let path = (*s.project).clone();
        let title = s.title();
        self.sessions.update(|v| {
            v.remove(i);
        });
        self.saved.update(|v| {
            v.retain(|p| p != &path);
            v.insert(0, path.clone());
        });
        let page = self.page.get();
        if page > i + 1 || page > self.sessions.get().len() {
            self.page.set(page.saturating_sub(1).max(if self.sessions.get().is_empty() { 0 } else { 1 }));
        }
        self.persist();
        self.say(format!("{title} closed into Saved"), Some(path));
    }

    /// Bring a saved session back, at the end of the row.
    pub fn reopen(&self, path: &Path) {
        self.toast.set(None);
        let sessions = self.sessions.get();
        if let Some(i) = sessions.iter().position(|s| *s.project == path) {
            return self.go(i + 1);
        }
        self.saved.update(|v| v.retain(|p| p != path));
        if !path.is_dir() {
            self.persist();
            return self.say(format!("{} no longer exists", path.display()), None);
        }
        self.sessions.update(|v| v.push(Session::open(&self.rt, path.to_path_buf(), self.new_id())));
        self.persist();
        self.overview.set(false);
        self.go(self.sessions.get().len());
    }

    pub fn reopen_last(&self) {
        if let Some(p) = self.saved.get().first().cloned() {
            self.reopen(&p);
        }
    }

    // ── swiping ──────────────────────────────────────────────────

    /// Two fingers on a trackpad (or a sideways wheel): follow the fingers,
    /// then settle on a page.
    pub fn on_wheel(&self, e: ScrollEvent) {
        if self.overview.get() || self.settings.get() || self.help.get() || self.engine.get() != EngineView::Hidden {
            return;
        }
        let (dx, dy) = (e.delta.dx, e.delta.dy);
        if dx.abs() <= dy.abs() * 1.2 {
            return;
        }
        let now = Instant::now();
        let mut sw = self.inner.swipe.get();
        if let Some(until) = sw.lock_until {
            if now < until {
                sw.lock_until = Some(now + Duration::from_millis(220));
                self.inner.swipe.set(sw);
                return;
            }
        }
        let w = self.size.get().width.max(1.0);
        let (page, last) = (self.page.get(), self.page_count() - 1);
        let mut d = self.drag.get() + dx;
        if (page == 0 && d > 0.0) || (page == last && d < 0.0) {
            d = d.signum() * d.abs().min(200.0) * 0.5;
        }
        self.drag.set(d.clamp(-0.6 * w, 0.6 * w));
        if !self.dragging.get() {
            self.dragging.set(true);
        }
        sw.last = Some(now);
        self.inner.swipe.set(sw);
        if d.abs() > 0.3 * w {
            self.commit_swipe();
        }
    }

    /// A trackpad pinch: fingers together shows every session (the
    /// overview), apart again goes back to the one you were on. One pinch
    /// does one thing; a pause or lifting the fingers starts the next.
    pub fn on_pinch(&self, p: vieww_platform_winit::Pinch) -> bool {
        use vieww_platform_winit::PinchPhase;
        /// How far the fingers move before it counts: a deliberate pinch,
        /// not a palm resting on the pad.
        const ENOUGH: f32 = 0.12;
        if self.settings.get() || self.help.get() || self.splash.get() {
            return false;
        }
        let now = Instant::now();
        let (mut total, last, mut done) = self.inner.pinch.get();
        if p.phase == PinchPhase::Started || last.is_none_or(|t| now.duration_since(t) > Duration::from_millis(350)) {
            (total, done) = (0.0, false);
        }
        total += p.delta;
        let mut changed = false;
        if !done {
            if total <= -ENOUGH && !self.overview.get() {
                self.set_overview(true);
                (done, changed) = (true, true);
            } else if total >= ENOUGH && self.overview.get() {
                self.set_overview(false);
                (done, changed) = (true, true);
            }
        }
        self.inner.pinch.set(if p.phase == PinchPhase::Ended { (0.0, None, false) } else { (total, Some(now), done) });
        changed
    }

    fn commit_swipe(&self) {
        let w = self.size.get().width.max(1.0);
        let (d, page) = (self.drag.get(), self.page.get());
        let target = if d < -0.12 * w {
            page + 1
        } else if d > 0.12 * w && page > 0 {
            page - 1
        } else {
            page
        };
        let mut sw = self.inner.swipe.get();
        sw.last = None;
        if target != page {
            sw.lock_until = Some(Instant::now() + Duration::from_millis(380));
        }
        self.inner.swipe.set(sw);
        self.go(target);
    }

    // ── voice and files ──────────────────────────────────────────

    pub fn start_voice(&self, c: &Composer) {
        if self.inner.recording.borrow().is_some() {
            return;
        }
        match capture::Recording::start() {
            Ok(r) => {
                *self.inner.recording.borrow_mut() = Some((c.id, r));
                c.voice.set(Voice::Listening);
            }
            Err(e) => c.voice.set(Voice::Failed(format!("The mic didn't start: {e}"))),
        }
    }

    pub fn stop_voice(&self, c: &Composer) {
        let Some((id, rec)) = self.inner.recording.borrow_mut().take() else { return };
        c.voice.set(Voice::Transcribing);
        self.mic_level.set(0.0);
        let speech = rec.finish();
        let inbox = Arc::clone(&self.inner.inbox);
        let wake = WakeHandle::current();
        std::thread::spawn(move || {
            let r = if speech.len() < (wright_runtime::voice::RATE / 4) as usize {
                Err("That was too short to hear anything — hold the mic a little longer.".to_string())
            } else {
                wright_runtime::config::Config::load().and_then(|cfg| wright_runtime::voice::transcribe(&cfg, &wright_runtime::voice::wav(&speech)))
            };
            inbox.voice.lock().unwrap_or_else(|p| p.into_inner()).push((id, r));
            wake.wake();
        });
    }

    pub fn pick(&self, c: &Composer, folder: bool) {
        c.menu.set(false);
        let (id, inbox, wake) = (c.id, Arc::clone(&self.inner.inbox), WakeHandle::current());
        std::thread::spawn(move || {
            let r = picker::pick(folder);
            inbox.picked.lock().unwrap_or_else(|p| p.into_inner()).push((id, r));
            wake.wake();
        });
    }

    // ── every frame ──────────────────────────────────────────────

    /// Before each frame: drain workers into signals, settle a swipe whose
    /// fingers lifted, tick the clock while anything is live.
    pub fn pump(&self, driver: &mut FrameDriver) {
        for s in self.sessions.get() {
            s.pump();
        }
        self.form.pump();
        let voice = std::mem::take(&mut *self.inner.inbox.voice.lock().unwrap_or_else(|p| p.into_inner()));
        for (id, r) in voice {
            if let Some(c) = self.composer_by_id(id) {
                match r {
                    Ok(text) if !text.is_empty() => {
                        c.insert_text(&text);
                        c.voice.set(Voice::Idle);
                        // Voice-first (the memo's item 6): on Home — and
                        // on the new-session page — speech alone STARTS
                        // the session. The person spoke their project;
                        // the interview begins from their words, and
                        // the Confirm Gate still guards everything that
                        // runs. Nothing else changes: typing still needs
                        // the send button, because a keyboard is a
                        // decision already made.
                        if id == self.home.id || id == self.fresh.id {
                            self.start_session_from(&c);
                        }
                    }
                    Ok(_) => c.voice.set(Voice::Failed("No words came through — try again a little closer to the mic.".into())),
                    Err(e) => c.voice.set(Voice::Failed(e)),
                }
            }
        }
        let picked = std::mem::take(&mut *self.inner.inbox.picked.lock().unwrap_or_else(|p| p.into_inner()));
        for (id, r) in picked {
            match (self.composer_by_id(id), r) {
                (Some(c), Ok(paths)) => c.attach(&paths),
                (_, Err(e)) => self.say(e, None),
                _ => {}
            }
        }
        let dropped = driver.take_dropped_files();
        if !dropped.is_empty() {
            self.current_composer().attach(dropped.paths());
        }
        if driver.focused().is_none() {
            driver.focus_next(true);
        }
        let focused = driver.focused();
        if focused != self.inner.last_focus.get() {
            self.inner.last_focus.set(focused);
            // Only text fields have a caret; their placeholder is their label.
            let typing = focused.and_then(|id| {
                let tree = driver.semantics();
                tree.node(id).filter(|n| n.role == vieww::Role::TextField).map(|n| n.label.clone().unwrap_or_default())
            });
            if typing != self.typing.get() {
                self.typing.set(typing);
            }
        }

        let now = Instant::now();
        if self.dragging.get() {
            if let Some(last) = self.inner.swipe.get().last {
                if now.duration_since(last) > Duration::from_millis(150) {
                    self.commit_swipe();
                }
            }
        }
        if self.splash.get() && self.inner.started.elapsed() > Duration::from_millis(3300) {
            self.splash.set(false);
        }
        if let Some(t) = self.toast.get() {
            if now > t.until {
                self.toast.set(None);
            }
        }
        if let Some((_, rec)) = self.inner.recording.borrow().as_ref() {
            self.mic_level.set(rec.level());
        }
        let busy = self.sessions.get().iter().any(|s| s.running.get() || s.exporting.get().is_some() || s.pending.get().is_some() || s.status().1 == session::Tone::Live);
        let live = busy
            || self.splash.get()
            || self.dragging.get()
            || self.engine.get() != EngineView::Hidden
            || self.inner.recording.borrow().is_some()
            || self.toast.get().is_some();
        self.inner.live.store(live, Ordering::Relaxed);
        if live {
            self.clock.set(self.inner.started.elapsed().as_secs_f32());
        }
    }

    // ── keys ─────────────────────────────────────────────────────

    /// Shortcuts. A text field takes typing first; only keys it did not
    /// want arrive here.
    pub fn handle_key(&self, e: &KeyEvent) -> bool {
        let space = matches!(&e.key, LogicalKey::Named(NamedKey::Space)) || matches!(&e.key, LogicalKey::Character(c) if c == " ");
        if !e.is_down() {
            if space && self.engine.get() == EngineView::Peek {
                self.set_engine(EngineView::Hidden);
                return true;
            }
            return false;
        }
        let shortcut = e.modifiers.is_shortcut(TargetPlatform::current());
        if space {
            if !e.repeat && self.engine.get() == EngineView::Hidden && !self.settings.get() {
                self.set_engine(EngineView::Peek);
            }
            return true;
        }
        match &e.key {
            LogicalKey::Named(NamedKey::Escape) => {
                if self.help.get() {
                    self.help.set(false);
                } else if self.settings.get() {
                    self.settings.set(false);
                } else if self.selected.get().is_some() {
                    self.selected.set(None);
                } else if self.engine.get() != EngineView::Hidden {
                    self.set_engine(EngineView::Hidden);
                } else if self.overview.get() {
                    self.set_overview(false);
                } else {
                    return false;
                }
                true
            }
            LogicalKey::Named(NamedKey::ArrowRight) if e.modifiers.is_empty() => {
                self.go(self.page.get() + 1);
                true
            }
            LogicalKey::Named(NamedKey::ArrowLeft) if e.modifiers.is_empty() => {
                self.go(self.page.get().saturating_sub(1));
                true
            }
            LogicalKey::Named(NamedKey::Enter) if self.overview.get() => {
                self.set_overview(false);
                true
            }
            LogicalKey::Character(c) => {
                let c = c.to_lowercase();
                match (shortcut, e.modifiers.shift(), c.as_str()) {
                    (true, _, ",") => self.open_settings(),
                    (true, true, "t") => self.reopen_last(),
                    (true, false, "w") => {
                        if let Some(i) = self.page.get().checked_sub(1).filter(|i| *i < self.sessions.get().len()) {
                            self.close_session(i);
                        }
                    }
                    (false, _, "e") => self.set_engine(match self.engine.get() {
                        EngineView::Open => EngineView::Hidden,
                        _ => EngineView::Open,
                    }),
                    (false, _, "o") => self.set_overview(!self.overview.get()),
                    (false, _, "?") => self.help.set(!self.help.get()),
                    _ => return false,
                }
                true
            }
            _ => false,
        }
    }
}

// ── entry points ─────────────────────────────────────────────────

pub fn main(args: &[String]) -> i32 {
    let mut project: Option<PathBuf> = None;
    let mut snapshot: Option<PathBuf> = None;
    let mut icon: Option<PathBuf> = None;
    let mut icon_size = 1024u32;
    let mut padded = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--snapshot" => {
                snapshot = args.get(i + 1).map(PathBuf::from);
                i += 2;
            }
            "--icon" => {
                icon = args.get(i + 1).map(PathBuf::from);
                i += 2;
            }
            "--size" => {
                icon_size = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(1024);
                i += 2;
            }
            "--padded" => {
                padded = true;
                i += 1;
            }
            other => {
                let dir = crate::cmd::project_dir(Some(other));
                project = Some(dir.canonicalize().unwrap_or(dir));
                i += 1;
            }
        }
    }
    let done = |r: Result<String, String>| match r {
        Ok(msg) => {
            println!("{msg}");
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    };
    if let Some(path) = icon {
        return done(shell::render_icon(&path, icon_size, padded).map(|_| format!("wrote {}", path.display())));
    }
    match snapshot {
        Some(dir) => done(shell::snapshots(project.as_deref(), &dir).map(|n| format!("wrote {n} screens to {}", dir.display()))),
        None => window(project),
    }
}

/// A double-clicked app on Windows starts with a console window of its
/// own; let it go (a console shared with a terminal is kept, so
/// `wright ui` from a shell still prints).
#[cfg(target_os = "windows")]
fn release_private_console() {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
        fn FreeConsole() -> i32;
    }
    let mut ids = [0u32; 4];
    // SAFETY: plain Win32 calls with a valid buffer of the stated length.
    unsafe {
        if GetConsoleProcessList(ids.as_mut_ptr(), ids.len() as u32) <= 1 {
            FreeConsole();
        }
    }
}

fn window(project: Option<PathBuf>) -> i32 {
    #[cfg(target_os = "windows")]
    release_private_console();
    let app = vieww_platform_winit::App::new().title("Wright").size(Size::new(1280.0, 800.0));
    let waker = app.waker();
    APP_WAKER.with_borrow_mut(|w| *w = Some(waker.clone()));
    let live = Arc::new(AtomicBool::new(true));
    {
        // The frame clock for live things (spinners, marching edges, the
        // mic meter): ~30 wakes a second, and none while nothing is live.
        let live = Arc::clone(&live);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(33));
            if live.load(Ordering::Relaxed) {
                waker.wake();
            }
        });
    }
    // The platform's services above the root: the system pasteboard is
    // what makes cut, copy and paste work in every text field (and the
    // Copy buttons). Without this the fields had nowhere to send Ctrl+C.
    let registry = vieww_platform_winit::services::platform();
    let clipboard: Option<Rc<dyn vieww::foundation::Clipboard>> = registry.get::<dyn vieww::foundation::Clipboard>();
    let services = vieww::foundation::SharedServices::new(registry);
    let ui: Rc<RefCell<Option<Ui>>> = Rc::new(RefCell::new(None));
    let for_hook = Rc::clone(&ui);
    let for_pinch = Rc::clone(&ui);
    let result = app
        .on_pinch(move |p| for_pinch.borrow().as_ref().is_some_and(|u| u.on_pinch(p)))
        .before_frame(move |driver| {
            if let Some(u) = for_hook.borrow().as_ref() {
                u.pump(driver);
            }
        })
        .run(move |driver| {
            shell::register(driver);
            let u = Ui::new(driver.elements().runtime(), project, live);
            if let Some(c) = clipboard.clone() {
                kit::init_copy(driver.elements().runtime(), c);
            }
            *ui.borrow_mut() = Some(u.clone());
            driver.set_root(Inherited::new(services.clone(), shell::root(&u)));
        });
    match result {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("error: could not open a window ({e}). Use the CLI (`wright help`), or `wright ui --snapshot <dir>` for pictures.");
            1
        }
    }
}

#[cfg(test)]
mod tests;
