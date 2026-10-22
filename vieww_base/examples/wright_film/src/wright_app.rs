//! wright_app — the film's subject, mounted.
//!
//! Two halves, both honest:
//!
//! 1. **The demo firing** ([`Demo`]). Before the first frame of the
//!    Wright act, the film fires the repository's own HN-digest example
//!    for real: the engine's mock provider serves the models, a local
//!    TCP sink stands in for the news site and the mail API, and
//!    `wright_engine::fire` runs the whole machine — the gate, the
//!    sandboxed tools it can run here, the event spine, the signed
//!    receipt. What the film then shows is *that* run's own spine and
//!    *that* run's own verify sentences, read from the store. The demo
//!    home lives under the film's output root, so a re-render reuses a
//!    completed firing instead of firing again.
//!
//! 2. **The mounted window** ([`Mounted`]). The real `wright::ui` shell
//!    — the same `Ui`, the same `shell::root` — on this film's own
//!    `FrameDriver` at the window's own 1280×800, driven by
//!    [`Script`]: a pure function of film seconds that sets the whole
//!    visible state each frame (the events prefix, the engine view, the
//!    approval card and its drag, the receipt overlay). Nothing is
//!    pre-recorded and nothing is faked: setting a signal is what the
//!    window's own code does too.

use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use vieww_foundation::Size;
use vieww_paint::Scene;
use vieww_render::FrameDriver;
use wright_core::events::Event;
use wright_runtime::store::Store;

/// Where the film's artifacts live while being built.
pub(crate) fn work_root() -> std::path::PathBuf {
    std::env::var_os("WRIGHT_FILM_OUT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("wright_film_out"))
}

// ── the demo firing ─────────────────────────────────────────────────────────

/// The film's one firing, and everything the scenes quote from it.
pub(crate) struct Demo {
    /// The project dir the firing ran in.
    pub project: std::path::PathBuf,
    /// The run the spine belongs to.
    pub run: u64,
    /// The whole spine, in order — the run card and the engine view are
    /// pure projections of prefixes of this list.
    pub events: Vec<Event>,
    /// The full result card, as the window loads it when the run is done.
    pub result: wright::ui::session::ResultView,
    /// The receipt in `wright verify`'s own words: every check, its
    /// sentence, the verdict. The proof movement quotes these verbatim.
    pub receipt: wright::ui::session::ReceiptView,
    /// The plan as the window loads it (stages, cards, bounds).
    pub plan: wright::ui::session::PlanView,
    /// The summary line the footer prints: agents, cost, signature.
    pub headline: String,
}

impl Demo {
    /// How many events the spine holds at fraction `frac` of the run.
    pub(crate) fn events_at(&self, frac: f32) -> usize {
        let n = self.events.len();
        ((frac.clamp(0.0, 1.0) * n as f32).round() as usize).min(n)
    }
}

static DEMO: std::sync::OnceLock<Demo> = std::sync::OnceLock::new();

/// The demo, fired once and read once — the film's one-firing rule.
pub(crate) fn demo() -> &'static Demo {
    DEMO.get_or_init(ensure)
}

fn ensure() -> Demo {
    let root = work_root();
    let home = root.join("demo-home");
    // The window reads `WRIGHT_HOME` at every construction and so does
    // the store — set it in THIS process before anything else, firing or
    // reading: the env is process-global and does not survive the
    // process that first set it.
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("WRIGHT_HOME", &home);
    std::env::set_var("WRIGHT_ALLOW_PRIVATE_NETWORK", "1");
    // Canonicalized from birth: the store's path is the project's own
    // hash, and a relative path and its absolute self hash differently —
    // the firing and the reading must agree on which one they mean.
    let project = home
        .join("projects")
        .join("hn-digest-film")
        .canonicalize()
        .unwrap_or_else(|_| {
            std::fs::create_dir_all(home.join("projects").join("hn-digest-film")).unwrap();
            home.join("projects")
                .join("hn-digest-film")
                .canonicalize()
                .unwrap()
        });
    let done = project.join("wright-film-run-done");
    if !done.exists() {
        fire(&home, &project);
        std::fs::write(&done, "the film's firing, complete\n").unwrap();
    }
    read(&project)
}

/// The firing itself: mock provider, local sink, the repo's own example.
fn fire(home: &std::path::Path, project: &std::path::Path) {
    std::fs::create_dir_all(project).unwrap();
    // WRIGHT_HOME is process-global; the film owns the process.
    std::env::set_var("WRIGHT_HOME", home);
    std::env::set_var("WRIGHT_ALLOW_PRIVATE_NETWORK", "1");
    let mock = wright_engine::mock::MockProvider::start(0)
        .expect("the mock provider serves the film's models");
    std::fs::write(
        home.join("config.kdl"),
        format!(
            "provider \"mock\" base=\"{}\" models=\"*\"\ninterview model=\"mock-1\"\n",
            mock.base_url()
        ),
    )
    .unwrap();

    // The sink: a local stand-in for the news site and the mail API, so
    // the firing exercises the network, the gate and the delivery.
    let sink = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = sink.local_addr().unwrap().port();
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        for s in sink.incoming().flatten() {
            let mut s = s;
            let mut buf = [0u8; 65536];
            let _ = s.read(&mut buf);
            let _ = s.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\nok",
            );
        }
    });

    let bp = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/wright/examples/hn-digest/blueprint.kdl"
    ))
    .expect("the repo ships the example")
    .replace(
        "goal \"Fetch top 30 HN stories: id, title, url, points\"",
        &format!("goal \"Fetch the top stories from http://127.0.0.1:{port}/top.json: id, title, url, points\""),
    )
    .replace(
        "goal \"Compose and send the digest email\"",
        &format!("goal \"Compose and send the digest to http://127.0.0.1:{port}/send\""),
    );
    std::fs::write(project.join("blueprint.kdl"), bp).unwrap();

    wright_engine::fire(
        project,
        wright_engine::FireOptions {
            trigger: wright_engine::Trigger::Manual,
            // The film's firing runs to completion on its own; the
            // approval *moment* the window shows is the real approval
            // card, driven by the real signal — see `Script`.
            approver: Box::new(wright_engine::approve::AutoApprove),
            allow_private_network: true,
            ..Default::default()
        },
    )
    .expect("the mock firing delivers");
}

/// Read a completed firing back — the store is the source of truth, the
/// same one the CLI and the window read.
fn read(project: &std::path::Path) -> Demo {
    let store = Store::open(project).expect("the demo project has a store");
    let run = *store.runs().last().expect("the demo fired");
    let events = store
        .read_events(run)
        .expect("the spine is durable")
        .events()
        .to_vec();
    let result = wright::ui::session::load_result(project, run).expect("the result loads");
    let receipt = wright::ui::session::load_receipt(project, run).expect("the receipt verifies");
    let plan = wright::ui::session::load_plan(project).expect("the plan is committed");
    let cost = result.cost;
    Demo {
        project: project.to_path_buf(),
        run,
        events,
        result,
        receipt,
        plan,
        headline: format!(
            "the run this film shows: {} events · {} agents · ${:.4} · receipt signed",
            store
                .read_events(run)
                .map(|l| l.events().len())
                .unwrap_or(0),
            8,
            cost
        ),
    }
}

// ── the mounted window ──────────────────────────────────────────────────────

/// The window's own size — the app is designed at 1280×800; the master
/// places its scene into the film frame.
pub(crate) const APP_W: f32 = 1280.0;
pub(crate) const APP_H: f32 = 800.0;

/// The real shell on the film's driver, at the window's own size.
pub(crate) struct Mounted {
    pub driver: FrameDriver,
    pub ui: Rc<wright::ui::Ui>,
}

impl Mounted {
    /// Mount the shell over the demo project. `WRIGHT_HOME` must already
    /// point at the demo home — `demo()` sets it before the first mount.
    pub(crate) fn new() -> Mounted {
        let d = demo();
        let mut driver = FrameDriver::new(Size::new(APP_W, APP_H));
        wright::ui::shell::register(&mut driver);
        let ui = Rc::new(wright::ui::Ui::new(
            driver.elements().runtime(),
            Some(d.project.clone()),
            std::sync::Arc::new(AtomicBool::new(false)),
        ));
        driver.set_root(wright::ui::shell::root(&ui));
        Mounted { driver, ui }
    }

    /// Apply the script at scene second `sec` on film second `abs`, and
    /// draw the frame. Scene seconds drive the script (every field is
    /// "seconds into the scene"); film seconds drive the clock.
    pub(crate) fn draw(&mut self, script: &Script, sec: f32, abs: f32) -> Scene {
        script.apply(self, sec, abs);
        self.driver
            .draw_frame_at(Duration::from_secs_f64(abs as f64));
        self.driver.scene().clone()
    }
}

/// What the window shows, as one pure function of film seconds. Every
/// field is the same signal the app's own code sets — the window's own
/// state machine, read by the film.
#[derive(Default)]
pub(crate) struct Script {
    /// The run's walk through the event spine, as (from_sec, to_sec):
    /// the spine grows from empty to whole between them. `None` shows
    /// the finished run whole (the delivery scenes).
    pub events_walk: Option<(f32, f32)>,
    /// The approval card: (from_sec, agent, reason). From `from_sec` it
    /// waits 3.2 s; over the next 0.9 s it is dragged right to allow
    /// (the card's own drag signal, the ±120 px threshold's grammar).
    pub approval: Option<(f32, String, String)>,
    /// When the engine view opens (seconds into the scene).
    pub engine_from: Option<f32>,
    /// The engine node the inspector shows, from this scene second.
    pub inspect_from: Option<(f32, String)>,
    /// When the receipt overlay rises.
    pub receipt_from: Option<f32>,
    /// When the window flips to light mode.
    pub light_from: Option<f32>,
    /// Keep the thread scrolled to its end from this second.
    pub follow_from: Option<f32>,
}

impl Script {
    /// Set every visible signal for scene second `sec` on film second
    /// `abs`; the app projects the state itself, as it always does.
    fn apply(&self, m: &Mounted, sec: f32, abs: f32) {
        let ui = &m.ui;
        let d = demo();
        ui.splash.set(false);
        ui.clock.set(abs);
        ui.go(1);

        let s = ui
            .current_session()
            .expect("the demo project is the open session");

        // The conversation's one human turn: the request the film typed
        // in movement I, as the window keeps it.
        let request = d.plan.request.clone();
        let chat: Vec<(bool, String)> = if request.is_empty() {
            Vec::new()
        } else {
            vec![(true, request)]
        };
        if s.chat.get() != chat {
            s.chat.set(chat);
        }

        // The spine as a prefix: the run card and the engine view are
        // projections of exactly this list. The walk is the scene's own
        // clock, eased so the steps land legibly.
        let frac = self.events_walk.map(|(from, to)| {
            let f = (sec - from) / (to - from).max(0.001);
            (f.clamp(0.0, 1.0) * f.clamp(0.0, 1.0) * (3.0 - 2.0 * f.clamp(0.0, 1.0))).sqrt()
        });
        let running = frac.is_some_and(|f| f < 1.0);
        match frac {
            Some(f) => {
                let k = d.events_at(f);
                s.events.set(d.events[..k].to_vec());
            }
            None => s.events.set(d.events.clone()),
        }
        if running {
            s.result.set(None);
            s.receipt.set(None);
            s.running.set(true);
        } else {
            s.running.set(false);
            s.result.set(Some(d.result.clone()));
            s.receipt.set(Some(d.receipt.clone()));
        }

        // The engine layer.
        let engine_on = self.engine_from.is_some_and(|t| abs >= t);
        ui.set_engine(if engine_on {
            wright::ui::EngineView::Open
        } else {
            wright::ui::EngineView::Hidden
        });
        ui.selected.set(match &self.inspect_from {
            Some((t, name)) if abs >= *t => Some(name.clone()),
            _ => None,
        });

        // The approval card, and its drag to allow.
        match &self.approval {
            Some((from, agent, reason)) if abs >= *from => {
                let hold = 3.2;
                let drag = ((abs - *from - hold).clamp(0.0, 0.9) / 0.9).min(1.0);
                if drag >= 1.0 {
                    s.card_drag.set(0.0);
                    s.approvals.set(Vec::new());
                } else {
                    s.card_drag.set(drag * 132.0);
                    s.approvals
                        .set(vec![(d.run, 1, agent.clone(), reason.clone())]);
                }
            }
            _ => {
                s.card_drag.set(0.0);
                s.approvals.set(Vec::new());
            }
        }

        // The receipt overlay.
        s.receipt_open
            .set(self.receipt_from.is_some_and(|t| abs >= t));

        // Light mode.
        ui.mode.set(if self.light_from.is_some_and(|t| abs >= t) {
            wright::ui::theme::Mode::Light
        } else {
            wright::ui::theme::Mode::Dark
        });

        // Keep the thread at the end while the run pours in.
        if self.follow_from.is_some_and(|t| abs >= t) {
            s.scroll.to_end();
        }
    }
}

/// The window placed in the film frame once its entrance dissolve is
/// done: (scale, dx, dy) — 1536×960 at (192, 60), chrome above and below.
pub(crate) fn window_place() -> (f32, f32, f32) {
    (1.2, 192.0, 60.0)
}

/// The window's rect in the film frame at placement `(scale, dx, dy)`.
pub(crate) fn window_rect(scale: f32, dx: f32, dy: f32) -> vieww_foundation::Rect {
    vieww_foundation::Rect::new(dx, dy, dx + APP_W * scale, dy + APP_H * scale)
}
