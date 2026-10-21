//! Drives the real UI headlessly: the same signals, the same worker
//! bridge, the same widget tree — no window, no clicks.
//!
//! One test owns `WRIGHT_HOME` (the process environment is shared), so
//! everything that needs a home runs inside it, in order.

use std::path::PathBuf;
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
    ScrollEvent { position: Offset::new(600.0, 400.0), delta: Offset::new(dx, 0.0), timestamp: Duration::from_millis(t) }
}

#[test]
fn the_shell_end_to_end() {
    let home = temp("home");
    std::env::set_var("WRIGHT_HOME", &home);
    let mock = wright_engine::mock::MockProvider::start(0).unwrap();
    std::fs::write(home.join("config.kdl"), format!("provider \"mock\" base=\"{}\" models=\"*\"\nvoice model=\"mock-whisper\"\n", mock.base_url())).unwrap();
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
    let ui = Ui::new(driver.elements().runtime(), Some(dir.clone()), std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)));
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
    assert_eq!(ui.typing.get(), None, "a fresh window has no text field focused");

    // Opening a project puts it in the row, after Home, and shows it.
    assert_eq!(ui.page.get(), 1);
    let s = ui.current_session().expect("the project is a session");
    assert_eq!(s.title(), "UI drive");
    assert_eq!(super::store::read_list("ui-open"), vec![dir.clone()], "the row is remembered");

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
                let (png, _) = NativeRenderer::new().render_to_png(driver.scene(), 1280, 800, ui.pal().ground).unwrap();
                let out = std::env::var("WRIGHT_UI_SHOTS").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
                std::fs::write(out.join("run-approval.png"), png).unwrap();
                // While a run waits for you, the engine shows it.
                let progress = super::session::Progress::of(&s.events.get());
                assert_eq!(progress.state("Notifier"), State::Waiting);
            }
            s.answer_approval(run, n, true);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(saw_card, "the approval request reached the UI");
    let r = s.result.get().expect("the run finished");
    assert!(r.ok, "{}", r.headline);
    assert!(s.events.get().iter().any(|e| e.kind == wright_core::events::EventKind::ApprovalResolved && e.note.contains("approved")));

    // ── scrolling: the conversation reports its length; the bar can move it ──
    for _ in 0..3 {
        draw(&mut driver);
    }
    let (view, content) = s.scroll.extent.get();
    assert!(view > 0.0 && content > 0.0, "the conversation measured itself: {view} / {content}");
    s.scroll.to_end();
    assert_eq!(s.scroll.at(), (content - view).max(0.0), "past the end is clamped to the end");
    s.scroll.by(-1e6);
    assert_eq!(s.scroll.at(), 0.0);

    // ── swiping: sideways scroll follows the fingers, then settles ──
    ui.on_wheel(scroll(-500.0, 0));
    assert_eq!(ui.page.get(), 2, "a long swipe left moves one page on (to the new-session page)");
    ui.on_wheel(scroll(-500.0, 10));
    assert_eq!(ui.page.get(), 2, "momentum right after a swipe does not move a second page");
    std::thread::sleep(Duration::from_millis(400));
    ui.on_wheel(scroll(60.0, 500));
    assert!(ui.dragging.get() && ui.drag.get() > 0.0, "a short swipe follows the fingers");
    std::thread::sleep(Duration::from_millis(200));
    ui.pump(&mut driver);
    assert_eq!((ui.page.get(), ui.dragging.get()), (2, false), "a short swipe settles back where it was");
    ui.go(1);

    // ── keys: Space peeks at the engine while held, Esc backs out ──
    let space = LogicalKey::Named(NamedKey::Space);
    assert!(ui.handle_key(&KeyEvent::down(space.clone(), Duration::ZERO)));
    assert_eq!(ui.engine.get(), EngineView::Peek);
    draw(&mut driver);
    assert!(ui.handle_key(&KeyEvent::up(space, Duration::ZERO)));
    assert_eq!(ui.engine.get(), EngineView::Hidden);
    assert!(ui.handle_key(&KeyEvent::down(LogicalKey::Character("o".into()), Duration::ZERO)));
    assert!(ui.overview.get());
    draw(&mut driver);
    assert!(ui.handle_key(&KeyEvent::down(LogicalKey::Named(NamedKey::Escape), Duration::ZERO)));
    assert!(!ui.overview.get());

    // ── closing into Saved, Undo, and reopening ──
    ui.close_session(0);
    assert!(ui.sessions.get().is_empty());
    assert_eq!(ui.saved.get(), vec![dir.clone()]);
    assert_eq!(super::store::read_list("ui-saved"), vec![dir.clone()], "Saved survives a restart");
    assert!(ui.toast.get().and_then(|t| t.undo).is_some(), "closing offers Undo");
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
    assert!(dir.join("artifacts/notes.txt").exists(), "attachments are copied where agents read");

    // ── voice: speech goes to the configured model, text comes back ──
    let speech = vec![0.1f32; 16_000];
    let text = wright_runtime::voice::transcribe(&wright_runtime::config::Config::load().unwrap(), &wright_runtime::voice::wav(&speech)).unwrap();
    assert!(text.starts_with("mock transcript of"), "{text}");

    // ── appearance: any accent; the look is remembered ──
    ui.set_look(Mode::Light, Color::hex(0xD6336C));
    draw(&mut driver);
    assert_eq!(super::theme::load(), (Mode::Light, Color::hex(0xD6336C)));

    // ── settings: the project's limits go into its blueprint ──
    let commit = super::settings::save_project(&dir, ["mock-1".into(), String::new(), "mock-1".into(), String::new()], "0.40", "2").unwrap();
    let bp = wright_engine::load_blueprint(&dir).unwrap();
    assert_eq!((bp.guardrails.per_firing_micro_usd, bp.guardrails.daily_micro_usd), (400_000, 2_000_000));
    assert!(!commit.is_empty());
    assert!(super::settings::save_project(&dir, ["mock-1".into(), String::new(), "mock-1".into(), String::new()], "5", "1").is_err(), "a day cannot allow less than one run");

    // ── every screen renders ──
    let shots = temp("shots");
    let n = shell::snapshots(Some(&dir), &shots).unwrap();
    assert!(n >= 6);
    for f in ["1-splash", "2-home", "3-session", "4-engine", "5-overview", "6-settings", "7-light"] {
        assert!(shots.join(format!("{f}.png")).exists(), "{f}");
    }
    let icon = shots.join("icon.png");
    shell::render_icon(&icon, 256, true).unwrap();
    assert!(std::fs::metadata(&icon).unwrap().len() > 100);
}

#[test]
fn the_palette_keeps_any_accent_readable() {
    for hex in [0x0078D4u32, 0xFFEB3B, 0x111111, 0xD6336C] {
        for mode in [Mode::Dark, Mode::Light] {
            let p = theme::palette(mode, Color::hex(hex));
            let lum = |c: Color| 0.2126 * c.r as f32 + 0.7152 * c.g as f32 + 0.0722 * c.b as f32;
            let (ink, ground) = (lum(p.accent_ink), lum(p.ground));
            assert!((ink - ground).abs() > 45.0, "accent text {hex:06X} on {mode:?} ground is too close: {ink} vs {ground}");
            assert!((lum(p.on_accent) - lum(p.accent)).abs() > 60.0, "button label on {hex:06X} must contrast");
        }
    }
}
