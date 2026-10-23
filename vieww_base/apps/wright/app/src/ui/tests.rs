//! Drives the real UI headlessly: the same signals, the same worker
//! bridge, the same widget tree — no window, no clicks.
//!
//! `WRIGHT_HOME` is process-global and cargo runs these tests on parallel
//! threads of one process, so every drive that touches the environment
//! holds `HOME` for its whole run: one owner of `WRIGHT_HOME` at a time.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use vieww::foundation::{KeyEvent, LogicalKey, NamedKey, ScrollEvent};
use vieww::paint::native::NativeRenderer;
use vieww::render::FrameDriver;

use super::session::State;
use super::*;

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("wright-ui-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn scroll(dx: f32, t: u64) -> ScrollEvent {
    ScrollEvent {
        position: Offset::new(600.0, 400.0),
        delta: Offset::new(dx, 0.0),
        timestamp: Duration::from_millis(t),
    }
}

/// The environment is process-global and the tests share it: whoever
/// holds `HOME` owns `WRIGHT_HOME` for the length of a drive. Without
/// it, whichever `set_var` landed last pointed every later
/// `wright_home()` read — the store lists, the config, the engine's
/// provider — at another test's home, and a finished test's teardown
/// dropped the variable out from under a running one.
static HOME: Mutex<()> = Mutex::new(());

fn home_lock() -> MutexGuard<'static, ()> {
    HOME.lock().unwrap_or_else(|p| p.into_inner())
}

/// The four product-page screens (`apps/wrightsite/assets/`), captured from
/// the real shell so the page cannot drift from the app: describe (Home with
/// the request typed), the confirm gate (the plan card, before anything
/// runs), the firing (the engine live, the gate up), and the receipt. Runs
/// against the mock provider, offline; writes to `WRIGHT_UI_SHOTS`.
#[test]
fn the_site_screens() {
    let _home = home_lock();
    let home = temp("site-shots");
    std::env::set_var("WRIGHT_HOME", &home);
    let mock = wright_engine::mock::MockProvider::start(0).unwrap();
    std::fs::write(
        home.join("config.kdl"),
        format!(
            "provider \"mock\" base=\"{}\" models=\"*\"\nvoice model=\"mock-whisper\"\n",
            mock.base_url()
        ),
    )
    .unwrap();
    std::env::set_var("WRIGHT_ALLOW_PRIVATE_NETWORK", "1");

    // A local sink the demo's Fetcher reads from and the Mailer delivers to,
    // so the firing exercises the network, the gate and the delivery.
    let sink = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = sink.local_addr().unwrap().port();
    std::thread::spawn(move || {
        use std::io::{Read as _, Write as _};
        for s in sink.incoming().flatten() {
            let mut s = s;
            let mut buf = [0u8; 65536];
            let _ = s.read(&mut buf);
            let _ = s.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\nok",
            );
        }
    });

    // The demo project the page's story tells: the repository's own
    // hn-digest blueprint, aimed at the sink so the gate is real.
    let dir = temp("hn-digest");
    let bp = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/hn-digest/blueprint.kdl"))
        .unwrap()
        .replace(
            "goal \"Fetch top 30 HN stories: id, title, url, points\"",
            &format!("goal \"Fetch the top stories from http://127.0.0.1:{port}/top.json: id, title, url, points\""),
        )
        .replace(
            "goal \"Compose and send the digest email\"",
            &format!("goal \"Compose and send the digest to http://127.0.0.1:{port}/send\""),
        );
    std::fs::write(dir.join("blueprint.kdl"), bp).unwrap();

    let out = std::env::var("WRIGHT_UI_SHOTS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    std::fs::create_dir_all(&out).unwrap();
    let size = Size::new(1280.0, 800.0);
    let mut clock = 0u64;
    // Frames until the stage's fades and glides have settled: the shell's
    // transitions run on the frame clock, so a capture taken too soon
    // catches them halfway.
    let mut settle = |driver: &mut FrameDriver, ui: &Ui, frames: usize| {
        for _ in 0..frames {
            clock += 16;
            ui.pump(driver);
            driver.draw_frame_at(Duration::from_millis(clock));
        }
    };

    // ── 1-describe: Home, the request typed, nothing sent yet ──
    let mut driver = FrameDriver::new(size);
    shell::register(&mut driver);
    let ui = Ui::new(
        driver.elements().runtime(),
        None,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    driver.set_root(shell::root(&ui));
    ui.splash.set(false);
    ui.home.prompt.set(TextEditingValue::new(
        "Every morning at 8, fetch the top Hacker News stories, summarize them, and email me the digest.",
    ));
    settle(&mut driver, &ui, 8);
    let (png, _) = NativeRenderer::new()
        .render_to_png(driver.scene(), 1280, 800, ui.pal().ground)
        .unwrap();
    std::fs::write(out.join("1-describe.png"), png).unwrap();
    drop(driver);

    // ── a second launch, straight into the project: the plan, before
    //    anything has run ──
    let mut driver = FrameDriver::new(size);
    shell::register(&mut driver);
    let ui = Ui::new(
        driver.elements().runtime(),
        Some(dir.clone()),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    driver.set_root(shell::root(&ui));
    ui.splash.set(false);
    settle(&mut driver, &ui, 8);
    let (png, _) = NativeRenderer::new()
        .render_to_png(driver.scene(), 1280, 800, ui.pal().ground)
        .unwrap();
    std::fs::write(out.join("2-plan.png"), png).unwrap();

    let s = ui
        .current_session()
        .expect("the project opened as a session");
    assert!(
        s.plan.get().is_some(),
        "the blueprint is on disk, so the plan is ready"
    );

    // ── 3-run: the firing. The thread scrolled to the gate: the run card's
    //    steps with their last notes, and the Mailer's approval card ──
    s.start_run();
    let mut saw_card = false;
    let deadline = Instant::now() + Duration::from_secs(120);
    while s.result.get().is_none() && Instant::now() < deadline {
        settle(&mut driver, &ui, 1);
        if let Some((run, n, agent, _)) = s.approvals.get().first().cloned() {
            if !saw_card {
                saw_card = true;
                assert_eq!(
                    agent, "Mailer",
                    "the before-send gate belongs to the Mailer"
                );
                s.scroll.to_end();
                settle(&mut driver, &ui, 40);
                let (png, _) = NativeRenderer::new()
                    .render_to_png(driver.scene(), 1280, 800, ui.pal().ground)
                    .unwrap();
                std::fs::write(out.join("3-run.png"), png).unwrap();
                // The same moment with the engine open on the live graph.
                ui.set_engine(EngineView::Open);
                settle(&mut driver, &ui, 40);
                let (png, _) = NativeRenderer::new()
                    .render_to_png(driver.scene(), 1280, 800, ui.pal().ground)
                    .unwrap();
                std::fs::write(out.join("3-run-engine.png"), png).unwrap();
                ui.set_engine(EngineView::Hidden);
                settle(&mut driver, &ui, 40);
            }
            s.answer_approval(run, n, true);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        saw_card,
        "the approval request reached the UI (result: {:?})",
        s.result.get()
    );
    let r = s.result.get().expect("the run finished");
    assert!(r.ok, "{}", r.headline);

    // ── 4-result: the receipt, with the run card above it ──
    s.scroll.to_end();
    settle(&mut driver, &ui, 40);
    let (png, _) = NativeRenderer::new()
        .render_to_png(driver.scene(), 1280, 800, ui.pal().ground)
        .unwrap();
    std::fs::write(out.join("4-result.png"), png).unwrap();
}

#[test]
fn open_folder_follows_the_latest_successful_export() {
    use wright_engine::export::{Exported, Kind};

    let project = PathBuf::from("projects/demo");
    let older = Exported {
        kind: Kind::Zip,
        path: PathBuf::from("Exports/demo/old.zip"),
        bytes: 12,
        sha256: "older".into(),
    };
    let latest = Exported {
        kind: Kind::Zip,
        path: PathBuf::from("Exports/demo/source.zip"),
        bytes: 24,
        sha256: "latest".into(),
    };
    let exports = vec![
        Err("one export attempt failed".into()),
        Ok(latest),
        Ok(older),
    ];

    assert_eq!(
        thread::folder_to_open(&project, &exports),
        PathBuf::from("Exports/demo")
    );
    assert_eq!(
        thread::folder_to_open(&project, &[]),
        project,
        "before the first export, the folder action still opens the project"
    );
}

#[test]
fn the_shell_end_to_end() {
    let _home = home_lock();
    let home = temp("home");
    std::env::set_var("WRIGHT_HOME", &home);
    let mock = wright_engine::mock::MockProvider::start(0).unwrap();
    std::fs::write(
        home.join("config.kdl"),
        format!(
            "provider \"mock\" base=\"{}\" models=\"*\"\nvoice model=\"mock-whisper\"\n",
            mock.base_url()
        ),
    )
    .unwrap();
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
    let dir = temp("proj");
    std::fs::write(
        dir.join("blueprint.kdl"),
        format!(
            "meta {{ name \"UI drive\" }}\nintent {{ summary \"send a note\" confirmed \"v1\" }}\n\
agent Notifier {{ goal \"Send a hello to http://127.0.0.1:{port}/hook\" tools \"http-post\" approval \"before-send\" permissions network=true }}\n\
deliverable kind=\"message\" {{ check \"the hook got a hello\" }}\nmodels {{ tier cheap prefer=\"mock-1\" fallback=\"\" tier quality prefer=\"mock-1\" fallback=\"\" }}\n"
        ),
    )
    .unwrap();
    let dir = dir.canonicalize().unwrap();

    let size = Size::new(1280.0, 800.0);
    let mut driver = FrameDriver::new(size);
    shell::register(&mut driver);
    let ui = Ui::new(
        driver.elements().runtime(),
        Some(dir.clone()),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    driver.set_root(shell::root(&ui));
    ui.splash.set(false);
    let mut frame = 0u64;
    let mut draw = |driver: &mut FrameDriver| {
        frame += 1;
        driver.draw_frame_at(Duration::from_millis(frame * 16));
    };
    draw(&mut driver);

    // Nothing is being typed into at start: no field draws a caret, and the
    // keyboard reaches the shortcuts (Space, E, O) instead of a text box.
    ui.pump(&mut driver);
    draw(&mut driver);
    ui.pump(&mut driver);
    assert_eq!(
        ui.typing.get(),
        None,
        "a fresh window has no text field focused"
    );

    // Opening a project puts it in the row, after Home, and shows it.
    assert_eq!(ui.page.get(), 1);
    let s = ui.current_session().expect("the project is a session");
    assert_eq!(s.title(), "UI drive");
    assert_eq!(
        super::store::read_list("ui-open"),
        vec![dir.clone()],
        "the row is remembered"
    );

    // ── the run: plan -> approval (dragged) -> result ──
    s.start_run();
    let mut saw_card = false;
    let deadline = Instant::now() + Duration::from_secs(60);
    while s.result.get().is_none() && Instant::now() < deadline {
        ui.pump(&mut driver);
        draw(&mut driver);
        if let Some((run, n, agent, _)) = s.approvals.get().first().cloned() {
            assert_eq!(agent, "Notifier");
            if !saw_card {
                saw_card = true;
                let (png, _) = NativeRenderer::new()
                    .render_to_png(driver.scene(), 1280, 800, ui.pal().ground)
                    .unwrap();
                let out = std::env::var("WRIGHT_UI_SHOTS")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| std::env::temp_dir());
                std::fs::write(out.join("run-approval.png"), png).unwrap();
                // While a run waits for you, the engine shows it.
                let progress = super::session::Progress::of(&s.events.get());
                assert_eq!(progress.state("Notifier"), State::Waiting);
            }
            s.answer_approval(run, n, true);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        saw_card,
        "the approval request reached the UI (result: {:?})",
        s.result.get()
    );
    let r = s.result.get().expect("the run finished");
    assert!(r.ok, "{}", r.headline);
    assert!(s.events.get().iter().any(|e| e.kind
        == wright_core::events::EventKind::ApprovalResolved
        && e.note.contains("approved")));

    // ── export: from the result card, in the background, next to the project ──
    s.export_menu.set(true);
    draw(&mut driver);
    s.export(wright_engine::export::Kind::Zip);
    assert!(!s.export_menu.get(), "choosing an export closes the menu");
    let deadline = Instant::now() + Duration::from_secs(30);
    while s.exports.get().is_empty() && Instant::now() < deadline {
        ui.pump(&mut driver);
        draw(&mut driver);
        std::thread::sleep(Duration::from_millis(20));
    }
    let made = s.exports.get()[0].clone().expect("the zip was made");
    assert!(
        made.path.starts_with(dir.parent().unwrap().join("Exports")),
        "{}",
        made.path.display()
    );
    assert!(made.path.exists() && s.exporting.get().is_none());

    // ── scrolling: the conversation reports its length; the bar can move it ──
    for _ in 0..3 {
        draw(&mut driver);
    }
    let (view, content) = s.scroll.extent.get();
    assert!(
        view > 0.0 && content > 0.0,
        "the conversation measured itself: {view} / {content}"
    );
    s.scroll.to_end();
    assert_eq!(
        s.scroll.at(),
        (content - view).max(0.0),
        "past the end is clamped to the end"
    );
    s.scroll.by(-1e6);
    assert_eq!(s.scroll.at(), 0.0);

    // ── swiping: sideways scroll follows the fingers, then settles ──
    ui.on_wheel(scroll(-500.0, 0));
    assert_eq!(
        ui.page.get(),
        2,
        "a long swipe left moves one page on (to the new-session page)"
    );
    ui.on_wheel(scroll(-500.0, 10));
    assert_eq!(
        ui.page.get(),
        2,
        "momentum right after a swipe does not move a second page"
    );
    std::thread::sleep(Duration::from_millis(400));
    ui.on_wheel(scroll(60.0, 500));
    assert!(
        ui.dragging.get() && ui.drag.get() > 0.0,
        "a short swipe follows the fingers"
    );
    std::thread::sleep(Duration::from_millis(200));
    ui.pump(&mut driver);
    assert_eq!(
        (ui.page.get(), ui.dragging.get()),
        (2, false),
        "a short swipe settles back where it was"
    );
    ui.go(1);

    // ── keys: Space peeks at the engine while held, Esc backs out ──
    let space = LogicalKey::Named(NamedKey::Space);
    assert!(ui.handle_key(&KeyEvent::down(space.clone(), Duration::ZERO)));
    assert_eq!(ui.engine.get(), EngineView::Peek);
    draw(&mut driver);
    assert!(ui.handle_key(&KeyEvent::up(space, Duration::ZERO)));
    assert_eq!(ui.engine.get(), EngineView::Hidden);
    assert!(ui.handle_key(&KeyEvent::down(
        LogicalKey::Character("o".into()),
        Duration::ZERO
    )));
    assert!(ui.overview.get());
    draw(&mut driver);
    assert!(ui.handle_key(&KeyEvent::down(
        LogicalKey::Named(NamedKey::Escape),
        Duration::ZERO
    )));
    assert!(!ui.overview.get());

    // ── closing into Saved, Undo, and reopening ──
    ui.close_session(0);
    assert!(ui.sessions.get().is_empty());
    assert_eq!(ui.saved.get(), vec![dir.clone()]);
    assert_eq!(
        super::store::read_list("ui-saved"),
        vec![dir.clone()],
        "Saved survives a restart"
    );
    assert!(
        ui.toast.get().and_then(|t| t.undo).is_some(),
        "closing offers Undo"
    );
    ui.reopen_last();
    assert_eq!(ui.sessions.get().len(), 1);
    assert!(ui.saved.get().is_empty());
    assert_eq!(ui.page.get(), 1);

    // ── files: dropping or picking attaches to the composer in view ──
    let note = dir.join("notes.txt");
    std::fs::write(&note, "hello").unwrap();
    let c = ui.current_composer();
    c.attach(std::slice::from_ref(&note));
    c.prompt.set(TextEditingValue::new("use this"));
    let msg = c.take_message(Some(&dir)).unwrap();
    assert!(msg.contains("artifacts/notes.txt"), "{msg}");
    assert!(
        dir.join("artifacts/notes.txt").exists(),
        "attachments are copied where agents read"
    );

    // ── voice: speech goes to the configured model, text comes back ──
    let speech = vec![0.1f32; 16_000];
    let text = wright_runtime::voice::transcribe(
        &wright_runtime::config::Config::load().unwrap(),
        &wright_runtime::voice::wav(&speech),
    )
    .unwrap();
    assert!(text.starts_with("mock transcript of"), "{text}");

    // ── appearance: any accent; the look is remembered ──
    ui.set_look(Mode::Light, Color::hex(0xD6336C));
    draw(&mut driver);
    assert_eq!(super::theme::load(), (Mode::Light, Color::hex(0xD6336C)));

    // ── settings: the project's limits go into its blueprint ──
    let commit = super::settings::save_project(
        &dir,
        [
            "mock-1".into(),
            String::new(),
            "mock-1".into(),
            String::new(),
        ],
        "0.40",
        "2",
    )
    .unwrap();
    let bp = wright_engine::load_blueprint(&dir).unwrap();
    assert_eq!(
        (
            bp.guardrails.per_firing_micro_usd,
            bp.guardrails.daily_micro_usd
        ),
        (400_000, 2_000_000)
    );
    assert!(!commit.is_empty());
    assert!(
        super::settings::save_project(
            &dir,
            [
                "mock-1".into(),
                String::new(),
                "mock-1".into(),
                String::new()
            ],
            "5",
            "1"
        )
        .is_err(),
        "a day cannot allow less than one run"
    );

    // ── a conversation: something on screen at once, the reply as it comes,
    //    and all of it kept for when the session is reopened ──
    let fresh = temp("talk").canonicalize().unwrap();
    let talk = super::session::Session::open(&ui.rt, fresh.clone(), 9_001);
    talk.say("Every Monday, email me a summary of my sales folder".into());
    assert!(
        talk.pending.get().is_some(),
        "the moment you send, the screen says something is happening"
    );
    assert!(talk.interviewing.get());
    let deadline = Instant::now() + Duration::from_secs(60);
    while talk.confirm.get().is_none() && Instant::now() < deadline {
        talk.pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        talk.confirm.get().is_some(),
        "the interview reached its summary"
    );
    assert!(
        talk.pending.get().is_none(),
        "waiting for you is not 'working'"
    );
    talk.answer_confirm(true);
    let deadline = Instant::now() + Duration::from_secs(30);
    while talk.plan.get().is_none() && Instant::now() < deadline {
        talk.pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    let plan = talk.plan.get().expect("the plan was written");
    assert!(!talk.interviewing.get() && talk.pending.get().is_none());
    let said = talk.chat.get();
    assert!(said.len() >= 2 && said[0].0 && !said[1].0, "{said:?}");
    let again = super::session::Session::open(&ui.rt, fresh.clone(), 9_002);
    assert_eq!(
        again.chat.get(),
        said,
        "the conversation comes back with the session"
    );
    assert_eq!(
        again.drafted.get(),
        said.len(),
        "and knows which part drafted the plan"
    );
    let card = super::session::saved_summary(&fresh);
    assert_eq!((card.title, card.messages), (plan.name.clone(), said.len()));
    assert!(card.last.is_some());
    // Its card on Home, among the saved.
    ui.saved.set(vec![fresh.clone()]);
    ui.go(0);
    for _ in 0..90 {
        ui.pump(&mut driver);
        draw(&mut driver);
    }
    let (png, _) = NativeRenderer::new()
        .render_to_png(driver.scene(), 1280, 800, ui.pal().ground)
        .unwrap();
    let out = std::env::var("WRIGHT_UI_SHOTS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    std::fs::write(out.join("home-saved.png"), png).unwrap();
    ui.saved.set(Vec::new());
    ui.go(1);

    // ── every screen renders ──
    let shots = temp("shots");
    let n = shell::snapshots(Some(&dir), &shots).unwrap();
    // The count is the shots that exist, named one by one — a lump sum
    // once said 9 while the directory held 10.
    let written = [
        "1-splash",
        "2-home",
        "3-session",
        "4-engine",
        "5-overview",
        "6-settings",
        "6-settings-download",
        "6-settings-agents",
        "6-settings-network",
        "6-settings-fleet",
        "6-settings-organization",
        "7-light",
        "8-export",
        "9-thinking",
        "10-stopped",
        "11-receipt",
        "12-light-session",
        "13-light-engine",
    ];
    for f in written {
        assert!(shots.join(format!("{f}.png")).exists(), "{f}");
    }
    assert_eq!(n, written.len(), "the count is the screens on disk");
    let icon = shots.join("icon.png");
    shell::render_icon(&icon, 256, true).unwrap();
    assert!(std::fs::metadata(&icon).unwrap().len() > 100);
}

/// The receipt, in the window: a finished run's evidence opens over the
/// session page as `wright verify` reads it — every check, its own
/// words, the verdict — and Esc puts it away. The CLI and the window
/// read the same store; this is what keeps them honest about it.
#[test]
fn the_receipt_opens_over_the_result() {
    let _home = home_lock();
    let home = temp("receipt");
    std::env::set_var("WRIGHT_HOME", &home);
    let mock = wright_engine::mock::MockProvider::start(0).unwrap();
    std::fs::write(
        home.join("config.kdl"),
        format!(
            "provider \"mock\" base=\"{}\" models=\"*\"\n",
            mock.base_url()
        ),
    )
    .unwrap();
    std::env::set_var("WRIGHT_ALLOW_PRIVATE_NETWORK", "1");

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
    let dir = temp("receipt-proj");
    std::fs::write(
        dir.join("blueprint.kdl"),
        format!(
            "meta {{ name \"Receipt drive\" }}\nintent {{ summary \"send a note\" confirmed \"v1\" }}\n\
agent Notifier {{ goal \"Send a hello to http://127.0.0.1:{port}/hook\" tools \"http-post\" approval \"before-send\" permissions network=true }}\n\
deliverable kind=\"message\" {{ check \"the hook got a hello\" }}\nmodels {{ tier cheap prefer=\"mock-1\" fallback=\"\" tier quality prefer=\"mock-1\" fallback=\"\" }}\n"
        ),
    )
    .unwrap();
    let dir = dir.canonicalize().unwrap();

    let size = Size::new(1280.0, 800.0);
    let mut driver = FrameDriver::new(size);
    shell::register(&mut driver);
    let ui = Ui::new(
        driver.elements().runtime(),
        Some(dir),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    driver.set_root(shell::root(&ui));
    ui.splash.set(false);
    let mut frame = 0u64;
    let mut draw = |driver: &mut FrameDriver| {
        frame += 1;
        driver.draw_frame_at(Duration::from_millis(frame * 16));
    };
    draw(&mut driver);
    let s = ui.current_session().expect("the project is a session");
    s.start_run();
    let deadline = Instant::now() + Duration::from_secs(60);
    while s.result.get().is_none() && Instant::now() < deadline {
        ui.pump(&mut driver);
        draw(&mut driver);
        if let Some((run, n, _, _)) = s.approvals.get().first().cloned() {
            s.answer_approval(run, n, true);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        s.result.get().expect("the run finished").ok,
        "the mock run delivers"
    );

    // The receipt loaded with the result: every check, and all of them ok.
    let r = s.receipt.get().expect("the receipt is in the window");
    assert!(r.ok, "a delivered run verifies: {}", r.report);
    let names: Vec<&str> = r.checks.iter().map(|(n, _, _)| n.as_str()).collect();
    for check in [
        "signature",
        "event log",
        "artifacts",
        "inputs",
        "blueprint",
        "identities",
    ] {
        assert!(
            names.contains(&check),
            "the {check} check is there: {names:?}"
        );
    }
    assert!(
        r.report.contains("verified"),
        "the report ends in a verdict"
    );

    // Over the page, it draws its own words — on the MOUNTED tree, not a
    // rebuild: the frame itself must change when the overlay rises
    // (a plain `if` at page level is read once at mount; the overlay is
    // its own Live region exactly so it is not).
    settle_frames(&mut driver, &ui, &mut frame);
    let before = frame_bytes(&mut driver);
    s.receipt_open.set(true);
    settle_frames(&mut driver, &ui, &mut frame);
    let after = frame_bytes(&mut driver);
    let drift: f64 = before
        .chunks(4)
        .zip(after.chunks(4))
        .map(|(a, b)| {
            (a[0] as i32 - b[0] as i32).abs() as f64
                + (a[1] as i32 - b[1] as i32).abs() as f64
                + (a[2] as i32 - b[2] as i32).abs() as f64
        })
        .sum::<f64>()
        / before.len() as f64;
    assert!(
        drift > 1.0,
        "the overlay mounted over the page (mean channel drift {drift:.2})"
    );
    let tree = vieww::debug_tree(super::thread::page(&ui, &s));
    assert!(tree.contains("The receipt"), "the overlay is up");
    assert!(tree.contains("signature"), "the checks are on screen");
    assert!(
        ui.handle_key(&KeyEvent::down(
            LogicalKey::Named(NamedKey::Escape),
            Duration::ZERO
        )),
        "Esc is handled"
    );
    assert!(
        !s.receipt_open.get(),
        "Esc closes the receipt, not the page"
    );
}

/// The frame as RGBA bytes, for before/after comparison.
fn frame_bytes(driver: &mut FrameDriver) -> Vec<u8> {
    let (pixels, _) = NativeRenderer::new()
        .render_to_pixels(driver.scene(), 1280, 800, Color::BLACK)
        .unwrap();
    pixels.data().to_vec()
}

/// Draw `n` frames with the clock and pump advancing together.
fn settle_frames(driver: &mut FrameDriver, ui: &Ui, frame: &mut u64) {
    for _ in 0..12 {
        *frame += 1;
        ui.pump(driver);
        driver.draw_frame_at(Duration::from_millis(*frame * 16));
    }
}

#[test]
fn the_palette_keeps_any_accent_readable() {
    for hex in [0x0078D4u32, 0xFFEB3B, 0x111111, 0xD6336C] {
        for mode in [Mode::Dark, Mode::Light] {
            let p = theme::palette(mode, Color::hex(hex));
            let lum = |c: Color| 0.2126 * c.r as f32 + 0.7152 * c.g as f32 + 0.0722 * c.b as f32;
            let (ink, ground) = (lum(p.accent_ink), lum(p.ground));
            assert!(
                (ink - ground).abs() > 45.0,
                "accent text {hex:06X} on {mode:?} ground is too close: {ink} vs {ground}"
            );
            assert!(
                (lum(p.on_accent) - lum(p.accent)).abs() > 60.0,
                "button label on {hex:06X} must contrast"
            );
        }
    }
}

/// The brand has one place at the top of the window: the title bar, the
/// strip that carries the minimise and close buttons — its title text and
/// its icon. The shell's own top bar, one strip below, starts with the page
/// title; a second "wright" directly under the first read as a stutter, and
/// this test is what keeps it from coming back.
#[test]
fn the_brand_is_on_the_title_bar_not_the_top_bar() {
    let size = Size::new(1280.0, 800.0);
    let mut driver = FrameDriver::new(size);
    shell::register(&mut driver);
    let ui = Ui::new(
        driver.elements().runtime(),
        None,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    ui.splash.set(false);
    let dump = vieww::debug_tree(shell::top_bar(&ui));
    assert!(
        !dump.contains("wright"),
        "the top bar repeats the name the title bar now carries:\n{dump}"
    );
    assert!(
        dump.contains("Home"),
        "the page title is still the bar's first label:\n{dump}"
    );
}

/// The session dots own the window's centre line: the pager sits centred
/// in the top bar, whatever the title on the left and the toggles on the
/// right weigh. (The dots began life tucked against the title box, visibly
/// left of centre.) The pill of the page you are on is read from both ends
/// of the pager — Home and the new-session page — because the pill moves:
/// the midpoint of the two pill centres is the pager's centre.
#[test]
fn the_dots_own_the_middle_of_the_top_bar() {
    let size = Size::new(1280.0, 800.0);
    let mut driver = FrameDriver::new(size);
    shell::register(&mut driver);
    let ui = Ui::new(
        driver.elements().runtime(),
        None,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    driver.set_root(shell::root(&ui));
    ui.splash.set(false);
    let accent = ui.pal().accent;
    // The active dot is the accent-filled pill; in the top strip it is the
    // only thing drawn in that colour, so its span says where it sits.
    let pill = |driver: &mut FrameDriver| -> (usize, usize) {
        ui.pump(driver);
        driver.draw_frame_at(Duration::from_millis(16));
        let (pixels, _) = NativeRenderer::new()
            .render_to_pixels(driver.scene(), 1280, 800, ui.pal().ground)
            .unwrap();
        let data = pixels.data();
        let (mut lo, mut hi) = (usize::MAX, 0);
        for y in 0..45 {
            for x in 0..1280 {
                let i = (y * 1280 + x) * 4;
                if (data[i], data[i + 1], data[i + 2], data[i + 3])
                    == (accent.r, accent.g, accent.b, 255)
                {
                    lo = lo.min(x);
                    hi = hi.max(x);
                }
            }
        }
        assert!(
            hi > lo,
            "the active dot's pill renders in the accent colour on the top strip"
        );
        (lo, hi)
    };
    let (lo, hi) = pill(&mut driver);
    let home_centre = (lo + hi) as f32 / 2.0;
    ui.go(ui.sessions.get().len() + 1);
    let (lo, hi) = pill(&mut driver);
    let new_centre = (lo + hi) as f32 / 2.0;
    let centre = (home_centre + new_centre) / 2.0;
    assert!(
        (centre - 640.0).abs() <= 1.0,
        "the pager centres on {}, not the window's middle (pills at {home_centre} and {new_centre})",
        centre
    );
}

/// The window icon is the installers' tile, rendered headlessly: the blue
/// ground, the white W somewhere on it, the corners rounded away so the
/// window's own background shows through them.
#[test]
fn the_window_icon_is_the_installers_tile() {
    let icon = shell::window_icon().expect("the window icon renders headlessly");
    assert_eq!((icon.width, icon.height), (32, 32));
    assert_eq!(icon.rgba.len(), 32 * 32 * 4);
    let at = |x: u32, y: u32| {
        let i = ((y * icon.width + x) as usize) * 4;
        (
            icon.rgba[i],
            icon.rgba[i + 1],
            icon.rgba[i + 2],
            icon.rgba[i + 3],
        )
    };
    let ground = logo::ICON_GROUND;
    let (r, g, b, a) = at(16, 30);
    assert_eq!(
        (r, g, b, a),
        (ground.r, ground.g, ground.b, 255),
        "the tile's blue ground, clear of the W"
    );
    assert!(
        icon.rgba.as_chunks::<4>().0.contains(&[255, 255, 255, 255]),
        "the white W is on the tile"
    );
    assert_eq!(at(1, 1).3, 0, "the corner is rounded away");
}

/// Scrolling by a fraction of a pixel moves the page as one piece: the
/// frame at 100.6 is the frame at 100.0 moved up exactly one pixel, text and
/// boxes alike. (Before vieww snapped the shift, each line of text rounded
/// to the pixel grid on its own and scrolled text looked wavy.)
#[test]
fn scrolling_moves_the_page_as_one_piece() {
    let size = Size::new(1280.0, 800.0);
    let mut driver = FrameDriver::new(size);
    shell::register(&mut driver);
    let ui = Ui::new(
        driver.elements().runtime(),
        None,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    driver.set_root(shell::root(&ui));
    ui.splash.set(false);
    ui.open_settings();
    let mut t = 0u64;
    let mut frame_at = |at: f32| {
        ui.form.scroll.offset.set(at);
        for _ in 0..3 {
            t += 1;
            driver.draw_frame_at(Duration::from_millis(t * 16));
        }
        let (pixels, _) = NativeRenderer::new()
            .render_to_pixels(driver.scene(), 1280, 800, ui.pal().ground)
            .unwrap();
        pixels.data().to_vec()
    };
    let (a, b) = (frame_at(100.0), frame_at(100.6));
    // Inside the settings page's scrolling column.
    let row = |img: &[u8], y: usize| img[(y * 1280 + 360) * 4..(y * 1280 + 1100) * 4].to_vec();
    for y in 140..700 {
        assert!(
            row(&a, y + 1) == row(&b, y),
            "row {y} of the page did not move by exactly one pixel"
        );
    }
}

/// Time a scroll step the way the window paints it (damage-limited, kept
/// pixels), at device ratios 1 and 2: `cargo test --release -p wright
/// --features ui scroll_frame_cost -- --ignored --nocapture`.
#[test]
#[ignore]
fn scroll_frame_cost() {
    let _home = home_lock();
    use vieww::paint::Damage;
    let home = temp("scrollbench");
    std::env::set_var("WRIGHT_HOME", &home);
    let size = Size::new(1280.0, 800.0);
    let mut driver = FrameDriver::new(size);
    shell::register(&mut driver);
    let ui = Ui::new(
        driver.elements().runtime(),
        None,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    driver.set_root(shell::root(&ui));
    ui.splash.set(false);
    ui.open_settings();
    let mut t = 0u64;
    for dpr in [1.0f32, 2.0] {
        let (w, h) = ((1280.0 * dpr) as u32, (800.0 * dpr) as u32);
        let mut r = NativeRenderer::new();
        ui.form.scroll.offset.set(0.0);
        for _ in 0..3 {
            t += 1;
            driver.draw_frame_at(Duration::from_millis(t * 16));
            r.render_in_place(&driver.scene().scaled(dpr), w, h, ui.pal().ground)
                .unwrap();
        }
        let (mut paint, mut area) = (Duration::ZERO, 0.0);
        for _ in 0..30 {
            ui.form.scroll.by(7.0);
            t += 1;
            driver.draw_frame_at(Duration::from_millis(t * 16));
            let mut damage =
                Damage::new(vieww::foundation::Rect::new(0.0, 0.0, w as f32, h as f32));
            for r0 in driver.damage().regions() {
                damage.add(vieww::foundation::Rect::new(
                    r0.left * dpr,
                    r0.top * dpr,
                    r0.right * dpr,
                    r0.bottom * dpr,
                ));
            }
            area += damage.covered_area() / (w as f32 * h as f32);
            let a = Instant::now();
            r.render_retained_in_place(&driver.scene().scaled(dpr), &damage, w, h, ui.pal().ground)
                .unwrap();
            paint += a.elapsed();
        }
        eprintln!(
            "dpr {dpr}: {:5.1} ms/frame repaint, {:3.0}% of the window damaged per scroll step",
            paint.as_secs_f64() * 1e3 / 30.0,
            area * 100.0 / 30.0
        );
    }
}

/// A two-finger pinch on a trackpad: fingers together shows every session,
/// apart goes back; a small twitch does nothing, and one pinch moves once.
#[test]
fn pinching_opens_and_closes_the_overview() {
    use vieww_platform_winit::{Pinch, PinchPhase};
    let mut driver = FrameDriver::new(Size::new(1280.0, 800.0));
    shell::register(&mut driver);
    let ui = Ui::new(
        driver.elements().runtime(),
        None,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    ui.splash.set(false);
    let pinch = |delta: f32, phase| ui.on_pinch(Pinch { delta, phase });
    assert!(!pinch(0.0, PinchPhase::Started));
    assert!(!pinch(-0.05, PinchPhase::Moved), "a twitch is not a pinch");
    assert!(!ui.overview.get());
    assert!(pinch(-0.1, PinchPhase::Moved));
    assert!(ui.overview.get(), "fingers together: the overview");
    assert!(
        !pinch(-0.3, PinchPhase::Moved) && !pinch(0.4, PinchPhase::Moved),
        "one pinch, one move"
    );
    assert!(ui.overview.get());
    pinch(0.0, PinchPhase::Ended);
    assert!(
        pinch(0.2, PinchPhase::Moved),
        "Ctrl + wheel (Windows) has no Started; a new pinch counts"
    );
    assert!(!ui.overview.get(), "fingers apart: back");
    ui.help.set(true);
    pinch(0.0, PinchPhase::Ended);
    assert!(!pinch(-0.5, PinchPhase::Moved), "not while a panel is open");
}

#[test]
fn speech_alone_starts_a_session_on_home() {
    let _home = home_lock();
    // Voice-first (the memo's item 6): the mic is the main way to start
    // on Home. Driving the same path the mic worker takes — a transcript
    // lands in the inbox, the shell pumps — the session must begin from
    // the spoken words alone, with the Confirm Gate still ahead of it.
    let home = temp("voice-first");
    std::env::set_var("WRIGHT_HOME", &home);
    let size = Size::new(1280.0, 800.0);
    let mut driver = FrameDriver::new(size);
    shell::register(&mut driver);
    let ui = Ui::new(
        driver.elements().runtime(),
        None,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    driver.set_root(shell::root(&ui));
    ui.splash.set(false);
    ui.pump(&mut driver);
    assert_eq!(ui.sessions.get().len(), 0, "Home starts with no sessions");

    // A transcript arrives from the voice worker for the home composer.
    ui.home.voice.set(super::session::Voice::Transcribing);
    ui.inner
        .inbox
        .voice
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .push((
            ui.home.id,
            Ok("A command line tool that counts words in files, with tests".to_string()),
        ));
    ui.pump(&mut driver);

    assert_eq!(
        ui.sessions.get().len(),
        1,
        "speech alone starts the session"
    );
    assert_eq!(
        ui.home.voice.get(),
        super::session::Voice::Idle,
        "the composer settles"
    );
    // The spoken words are the session's opening message: typing them
    // and pressing send does the same thing; this test pins that.
    let chat = ui.sessions.get()[0].chat.get();
    assert!(!chat.is_empty(), "the session opened with the spoken words");
    assert!(
        chat[0].1.contains("counts words in files"),
        "chat: {chat:?}"
    );
    assert!(chat[0].0, "the first message is the person's");
    std::env::remove_var("WRIGHT_HOME");
}

/// Copyable text: select it, copy it, and the words land on the window's
/// pasteboard — while letters typed at it still reach the shortcuts.
#[test]
fn reading_text_can_be_selected_and_copied() {
    use std::rc::Rc;
    use vieww::foundation::{
        Clipboard, MemoryClipboard, Modifiers, Services, SharedServices, TargetPlatform,
    };
    let mut driver = FrameDriver::new(Size::new(600.0, 200.0));
    let board = Rc::new(MemoryClipboard::new());
    kit::init_copy(
        driver.elements().runtime(),
        Rc::clone(&board) as Rc<dyn Clipboard>,
    );
    let mut registry = Services::new();
    registry.provide::<dyn Clipboard>(Rc::clone(&board) as Rc<dyn Clipboard>);
    let reply = "The plan reads the CSVs and emails a one-page summary.";
    driver.set_root(Inherited::new(
        SharedServices::new(registry),
        kit::copyable(reply, 15.0, Color::hex(0x222222)),
    ));
    driver.draw_frame();
    // Not a Tab stop (the first focus must stay with the composer)...
    assert!(!driver.focus_next(true), "reading text is not a Tab stop");
    // ...a press is what puts the keyboard there.
    let at = driver
        .semantics()
        .nodes()
        .iter()
        .find(|n| n.label.as_deref() == Some(reply))
        .expect("the text is in the tree")
        .bounds;
    let p = Offset::new(at.left + 4.0, at.top + at.height() / 2.0);
    driver.handle_pointer(&vieww::foundation::PointerEvent::down(
        vieww::foundation::PointerId(1),
        p,
        Duration::ZERO,
    ));
    driver.handle_pointer(&vieww::foundation::PointerEvent::up(
        vieww::foundation::PointerId(1),
        p,
        Duration::from_millis(40),
    ));
    driver.draw_frame();
    let shortcut = Modifiers::shortcut_for(TargetPlatform::current());
    for c in ["a", "c"] {
        assert!(driver.handle_key(
            &KeyEvent::down(LogicalKey::Character(c.into()), Duration::ZERO)
                .with_modifiers(shortcut)
        ));
        driver.draw_frame();
    }
    assert_eq!(board.read_text().unwrap().as_deref(), Some(reply));
    // A plain letter is not the text's: it goes on to the app (E, O, ?).
    assert!(!driver.handle_key(&KeyEvent::down(
        LogicalKey::Character("e".into()),
        Duration::ZERO
    )));
    // And the Copy buttons write to the same pasteboard.
    assert!(kit::copy_text("a whole plan"));
    assert_eq!(board.read_text().unwrap().as_deref(), Some("a whole plan"));
}

// ── the reply, rendered and interruptible ──────────────────────────────

/// A reply with markdown in it renders as what the model meant, not as
/// the asterisks it typed: the tree holds the framework's Markdown, and
/// a reply with none renders the same way it always did.
#[test]
fn a_reply_renders_its_markdown() {
    let _home = home_lock();
    let home = temp("markdown");
    std::env::set_var("WRIGHT_HOME", &home);
    let size = Size::new(1280.0, 800.0);
    let mut driver = FrameDriver::new(size);
    shell::register(&mut driver);
    let ui = Ui::new(
        driver.elements().runtime(),
        None,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    driver.set_root(shell::root(&ui));
    ui.splash.set(false);
    let dir = temp("markdown-proj").canonicalize().unwrap();
    let s = super::session::Session::open(driver.elements().runtime(), dir, 7_001);

    // The smaller models answer in markdown whether or not they were
    // asked to: a heading, a list, a code span.
    s.chat.set(vec![
        (true, "Make it weekly".into()),
        (
            false,
            "# Done\n\nThe plan now runs **weekly**:\n\n- fetch on Mondays\n- email by 9\n\nRun `wright plan` to read it back.".into(),
        ),
    ]);
    let tree = vieww::debug_tree(super::thread::page(&ui, &s));
    assert!(
        tree.contains("Markdown"),
        "the reply went through the renderer"
    );
    assert!(
        tree.contains("Copy"),
        "the whole reply is still one Copy away"
    );

    // Plain prose is unchanged by the renderer: same page, no markdown
    // in the text, the tree still builds (a paragraph is a Markdown
    // block — what changed is nothing the eye can see).
    s.chat.set(vec![
        (true, "Make it weekly".into()),
        (
            false,
            "Done. The plan now runs weekly and emails by nine.".into(),
        ),
    ]);
    let plain = vieww::debug_tree(super::thread::page(&ui, &s));
    assert!(plain.contains("Markdown"));
}

/// The chat takes a chat app's share of the window: the thread and the
/// composer under it widen with the pane (86% of it, up to a ceiling)
/// instead of floating in a fixed reading column's margins. The probe is
/// the first bubble — accent-tinted, right-aligned to the column's edge,
/// and the only thing on a fresh thread — so the rightmost strong pixel
/// in its band *is* the column's right edge. (The thread was a 760px
/// column centred in the window: at the default 1280 that left 260px of
/// dead margin each side, and more the wider the window grew.)
#[test]
fn the_chat_takes_a_chat_apps_share_of_the_window() {
    let _home = home_lock();
    let home = temp("chat-width");
    std::env::set_var("WRIGHT_HOME", &home);

    // The bubble's right edge at a given window size: the rightmost
    // pixel that is clearly not the ground, in the band a fresh thread's
    // one message occupies.
    let right_edge = |w: u32, h: u32, tag: &str, id: u64| -> usize {
        let size = Size::new(w as f32, h as f32);
        let mut driver = FrameDriver::new(size);
        shell::register(&mut driver);
        let ui = Ui::new(
            driver.elements().runtime(),
            None,
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        );
        let dir = temp(&format!("chat-width-{tag}")).canonicalize().unwrap();
        let s = super::session::Session::open(driver.elements().runtime(), dir, id);
        s.chat.set(vec![(
            true,
            "A message long enough to reach the bubble's own width cap, so its right edge is the \
             column's right edge, however wide the window it sits in."
                .into(),
        )]);
        driver.set_root(super::thread::page(&ui, &s));
        ui.pump(&mut driver);
        driver.draw_frame_at(Duration::from_millis(16));
        ui.pump(&mut driver);
        driver.draw_frame_at(Duration::from_millis(32));
        let (pixels, _) = NativeRenderer::new()
            .render_to_pixels(driver.scene(), w, h, ui.pal().ground)
            .unwrap();
        let data = pixels.data();
        let ground = ui.pal().ground;
        let (gr, gg, gb) = (ground.r as i32, ground.g as i32, ground.b as i32);
        let mut rightmost = 0usize;
        for y in 34..120usize {
            for x in (0..w as usize).rev() {
                let i = (y * w as usize + x) * 4;
                if data[i + 3] == 255
                    && (data[i] as i32 - gr).abs()
                        + (data[i + 1] as i32 - gg).abs()
                        + (data[i + 2] as i32 - gb).abs()
                        > 60
                {
                    rightmost = rightmost.max(x);
                    break;
                }
            }
        }
        assert!(
            rightmost > 0,
            "the bubble renders in the scanned band at {w}x{h}"
        );
        rightmost
    };

    // The default window: 86% of the pane past the gutters puts the
    // column's edge near 1170. The old fixed column stopped at 1020,
    // which is what this test is here to keep from coming back.
    let at_1280 = right_edge(1280, 800, "1280", 7_003);
    assert!(
        (1140..=1200).contains(&at_1280),
        "the chat's right edge at 1280 is {at_1280}, not a chat app's share of the pane"
    );

    // A wider window: the column stops at its ceiling (1160), so the
    // edge lands near 1380 — wider than the pane's share would be, and
    // narrower than filling the window outright would read.
    let at_1600 = right_edge(1600, 900, "1600", 7_004);
    assert!(
        (1350..=1410).contains(&at_1600),
        "the chat's right edge at 1600 is {at_1600}, not the column's ceiling"
    );
}

/// The send button is stop while a reply is on its way: the same circle,
/// the opposite deed, and a tap ends the wait and swallows the rest of
/// the stream — the worker's words are not kept, not shown, not written.
#[test]
fn the_send_button_becomes_stop_and_stops() {
    let _home = home_lock();
    let home = temp("stop");
    std::env::set_var("WRIGHT_HOME", &home);
    let size = Size::new(1280.0, 800.0);
    let mut driver = FrameDriver::new(size);
    shell::register(&mut driver);
    let ui = Ui::new(
        driver.elements().runtime(),
        None,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    driver.set_root(shell::root(&ui));
    ui.splash.set(false);
    let dir = temp("stop-proj").canonicalize().unwrap();
    let s = super::session::Session::open(driver.elements().runtime(), dir, 7_002);

    // Idle: the button says Send.
    let tree = vieww::debug_tree(super::thread::page(&ui, &s));
    assert!(tree.contains("Send"), "idle is send: {tree}");
    assert!(!tree.contains("Stop"));

    // A reply on its way: the same button says Stop.
    s.pending.set(Some(super::session::Pending {
        what: "Thinking".into(),
        text: String::new(),
        since: Instant::now(),
    }));
    let tree = vieww::debug_tree(super::thread::page(&ui, &s));
    assert!(tree.contains("Stop"), "busy is stop: {tree}");

    // The stop itself, driven through the worker bridge the window runs
    // on: the spinner goes now, and what the model says next lands
    // nowhere.
    use wright_engine::interview::InterviewIo as _;
    let (_tx, rx) = std::sync::mpsc::channel::<super::session::Reply>();
    let shared = std::sync::Arc::clone(&s.shared);
    let mut io = super::session::ChannelIo {
        earlier: Vec::new(),
        now: Default::default(),
        first: None,
        rx,
        shared,
        wake: super::session::WakeHandle(None),
    };
    io.working("Thinking");
    io.partial("Half a sentence");
    s.pump();
    assert!(s.pending.get().is_some());
    s.stop_reply();
    assert!(s.pending.get().is_none(), "the wait ends on the tap");
    assert!(!s.interviewing.get());
    // The model finishes anyway — the words are dropped.
    io.say("A full sentence that arrived too late");
    io.partial("More words streaming in");
    s.pump();
    assert!(
        s.pending.get().is_none(),
        "a stopped stream does not bring the spinner back"
    );
    assert!(
        !s.chat.get().iter().any(|(_, t)| t.contains("too late")),
        "the late words are not kept: {:?}",
        s.chat.get()
    );
    // And the interview's next question is the last: a stop ends it.
    assert_eq!(io.ask(), None);
    // When the interview then reports back, the notice is the stop's own
    // one line — not the machine's "nothing was written" verdict, which
    // describes the interview's choices rather than the person's.
    *s.shared
        .interview_done
        .lock()
        .unwrap_or_else(|p| p.into_inner()) = Some(Ok(false));
    s.pump();
    assert_eq!(s.notice.get(), "Stopped — nothing further was written.");
}

/// Esc is the keyboard's stop: a pending reply is the deepest thing open
/// on the page, and backing out of it is stopping it.
#[test]
fn esc_stops_a_reply_on_its_way() {
    let _home = home_lock();
    let home = temp("esc-stop");
    std::env::set_var("WRIGHT_HOME", &home);
    let size = Size::new(1280.0, 800.0);
    let mut driver = FrameDriver::new(size);
    shell::register(&mut driver);
    let ui = Ui::new(
        driver.elements().runtime(),
        None,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    driver.set_root(shell::root(&ui));
    ui.splash.set(false);
    let dir = temp("esc-stop-proj").canonicalize().unwrap();
    ui.sessions.set(vec![super::session::Session::open(
        driver.elements().runtime(),
        dir,
        7_003,
    )]);
    ui.go(1);
    let s = ui.current_session().expect("the project is the open page");
    s.pending.set(Some(super::session::Pending {
        what: "Thinking".into(),
        text: String::new(),
        since: Instant::now(),
    }));
    assert!(ui.handle_key(&KeyEvent::down(
        LogicalKey::Named(NamedKey::Escape),
        Duration::ZERO
    )));
    assert!(s.pending.get().is_none(), "Esc stopped the reply");
}

/// A thread scrolled far from its end grows a one-tap way back: the
/// jump-to-latest chip is there when the end is more than a screen away
/// and is not when it is not.
#[test]
fn a_thread_far_from_its_end_offers_the_jump_back() {
    let _home = home_lock();
    let home = temp("jump");
    std::env::set_var("WRIGHT_HOME", &home);
    let size = Size::new(1280.0, 800.0);
    let mut driver = FrameDriver::new(size);
    shell::register(&mut driver);
    let ui = Ui::new(
        driver.elements().runtime(),
        None,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    driver.set_root(shell::root(&ui));
    ui.splash.set(false);
    let dir = temp("jump-proj").canonicalize().unwrap();
    let s = super::session::Session::open(driver.elements().runtime(), dir, 7_004);

    // Nothing to scroll: no chip.
    let tree = vieww::debug_tree(super::thread::page(&ui, &s));
    assert!(!tree.contains("Jump to the latest"));

    // A long thread, scrolled to its top: the end is far, the chip is up.
    let talk: Vec<(bool, String)> = (0..80)
        .map(|i| {
            (
                i % 2 == 0,
                format!("line {i} of a conversation long enough to scroll"),
            )
        })
        .collect();
    s.chat.set(talk);
    s.scroll.extent.set((800.0, 6000.0));
    s.scroll.offset.set(0.0);
    let tree = vieww::debug_tree(super::thread::page(&ui, &s));
    assert!(
        tree.contains("Jump to the latest"),
        "the chip is up when the end is far: {tree}"
    );

    // Back at the end: gone.
    s.scroll.to_end();
    let tree = vieww::debug_tree(super::thread::page(&ui, &s));
    assert!(!tree.contains("Jump to the latest"));
}

/// The top bar's centre belongs to the pager alone: the dots stay on the
/// window's centre line while the session, engine, settings and help
/// controls hug the bar's right edge. (The toggles' row once filled its
/// flank but packed from the start, so the buttons crowded the pager and
/// the right quarter of the bar sat empty.)
#[test]
fn the_toggles_hug_the_right_edge_of_the_top_bar() {
    let size = Size::new(1280.0, 800.0);
    let mut driver = FrameDriver::new(size);
    shell::register(&mut driver);
    let ui = Ui::new(
        driver.elements().runtime(),
        None,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    driver.set_root(shell::root(&ui));
    ui.splash.set(false);
    ui.pump(&mut driver);
    driver.draw_frame_at(Duration::from_millis(16));
    let (pixels, _) = NativeRenderer::new()
        .render_to_pixels(driver.scene(), 1280, 800, ui.pal().ground)
        .unwrap();
    let data = pixels.data();
    // The strip's own background, read from the padding left of the title
    // on each scanned row, so the bar's paint can change without breaking
    // the test's meaning.
    let mut rightmost = 0usize;
    for y in 6..45usize {
        let i = (y * 1280 + 4) * 4;
        let (r0, g0, b0) = (data[i] as i32, data[i + 1] as i32, data[i + 2] as i32);
        for x in (0..1280usize).rev() {
            let i = (y * 1280 + x) * 4;
            if data[i + 3] == 255
                && (data[i] as i32 - r0).abs()
                    + (data[i + 1] as i32 - g0).abs()
                    + (data[i + 2] as i32 - b0).abs()
                    > 60
            {
                rightmost = rightmost.max(x);
                break;
            }
        }
    }
    // 1280 − 16 px of bar padding − the last icon's width leaves the
    // rightmost glyph near 1254; the broken layout stopped near 965.
    assert!(
        rightmost >= 1230,
        "the top bar's rightmost control ends at {rightmost}, not at the right edge"
    );
}
