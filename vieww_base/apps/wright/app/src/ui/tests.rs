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

/// The four product-page screens (`apps/wrightsite/assets/`), captured from
/// the real shell so the page cannot drift from the app: describe (Home with
/// the request typed), the confirm gate (the plan card, before anything
/// runs), the firing (the engine live, the gate up), and the receipt. Runs
/// against the mock provider, offline; writes to `WRIGHT_UI_SHOTS`.
#[test]
fn the_site_screens() {
    let home = temp("site-shots");
    std::env::set_var("WRIGHT_HOME", &home);
    let mock = wright_engine::mock::MockProvider::start(0).unwrap();
    std::fs::write(home.join("config.kdl"), format!("provider \"mock\" base=\"{}\" models=\"*\"\nvoice model=\"mock-whisper\"\n", mock.base_url())).unwrap();
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
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\nok");
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

    let out = std::env::var("WRIGHT_UI_SHOTS").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
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
    let ui = Ui::new(driver.elements().runtime(), None, std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)));
    driver.set_root(shell::root(&ui));
    ui.splash.set(false);
    ui.home.prompt.set(TextEditingValue::new(
        "Every morning at 8, fetch the top Hacker News stories, summarize them, and email me the digest.",
    ));
    settle(&mut driver, &ui, 8);
    let (png, _) = NativeRenderer::new().render_to_png(driver.scene(), 1280, 800, ui.pal().ground).unwrap();
    std::fs::write(out.join("1-describe.png"), png).unwrap();
    drop(driver);

    // ── a second launch, straight into the project: the plan, before
    //    anything has run ──
    let mut driver = FrameDriver::new(size);
    shell::register(&mut driver);
    let ui = Ui::new(driver.elements().runtime(), Some(dir.clone()), std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)));
    driver.set_root(shell::root(&ui));
    ui.splash.set(false);
    settle(&mut driver, &ui, 8);
    let (png, _) = NativeRenderer::new().render_to_png(driver.scene(), 1280, 800, ui.pal().ground).unwrap();
    std::fs::write(out.join("2-plan.png"), png).unwrap();

    let s = ui.current_session().expect("the project opened as a session");
    assert!(s.plan.get().is_some(), "the blueprint is on disk, so the plan is ready");

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
                assert_eq!(agent, "Mailer", "the before-send gate belongs to the Mailer");
                s.scroll.to_end();
                settle(&mut driver, &ui, 40);
                let (png, _) = NativeRenderer::new().render_to_png(driver.scene(), 1280, 800, ui.pal().ground).unwrap();
                std::fs::write(out.join("3-run.png"), png).unwrap();
                // The same moment with the engine open on the live graph.
                ui.set_engine(EngineView::Open);
                settle(&mut driver, &ui, 40);
                let (png, _) = NativeRenderer::new().render_to_png(driver.scene(), 1280, 800, ui.pal().ground).unwrap();
                std::fs::write(out.join("3-run-engine.png"), png).unwrap();
                ui.set_engine(EngineView::Hidden);
                settle(&mut driver, &ui, 40);
            }
            s.answer_approval(run, n, true);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(saw_card, "the approval request reached the UI");
    let r = s.result.get().expect("the run finished");
    assert!(r.ok, "{}", r.headline);

    // ── 4-result: the receipt, with the run card above it ──
    s.scroll.to_end();
    settle(&mut driver, &ui, 40);
    let (png, _) = NativeRenderer::new().render_to_png(driver.scene(), 1280, 800, ui.pal().ground).unwrap();
    std::fs::write(out.join("4-result.png"), png).unwrap();
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
    assert!(made.path.starts_with(dir.parent().unwrap().join("Exports")), "{}", made.path.display());
    assert!(made.path.exists() && s.exporting.get().is_none());

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

    // ── a conversation: something on screen at once, the reply as it comes,
    //    and all of it kept for when the session is reopened ──
    let fresh = temp("talk").canonicalize().unwrap();
    let talk = super::session::Session::open(&ui.rt, fresh.clone(), 9_001);
    talk.say("Every Monday, email me a summary of my sales folder".into());
    assert!(talk.pending.get().is_some(), "the moment you send, the screen says something is happening");
    assert!(talk.interviewing.get());
    let deadline = Instant::now() + Duration::from_secs(60);
    while talk.confirm.get().is_none() && Instant::now() < deadline {
        talk.pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(talk.confirm.get().is_some(), "the interview reached its summary");
    assert!(talk.pending.get().is_none(), "waiting for you is not 'working'");
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
    assert_eq!(again.chat.get(), said, "the conversation comes back with the session");
    assert_eq!(again.drafted.get(), said.len(), "and knows which part drafted the plan");
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
    let (png, _) = NativeRenderer::new().render_to_png(driver.scene(), 1280, 800, ui.pal().ground).unwrap();
    let out = std::env::var("WRIGHT_UI_SHOTS").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
    std::fs::write(out.join("home-saved.png"), png).unwrap();
    ui.saved.set(Vec::new());
    ui.go(1);

    // ── every screen renders ──
    let shots = temp("shots");
    let n = shell::snapshots(Some(&dir), &shots).unwrap();
    assert!(n >= 6);
    for f in ["1-splash", "2-home", "3-session", "4-engine", "5-overview", "6-settings", "6-settings-download", "6-settings-agents", "7-light", "8-export", "9-thinking"] {
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

/// Scrolling by a fraction of a pixel moves the page as one piece: the
/// frame at 100.6 is the frame at 100.0 moved up exactly one pixel, text and
/// boxes alike. (Before vieww snapped the shift, each line of text rounded
/// to the pixel grid on its own and scrolled text looked wavy.)
#[test]
fn scrolling_moves_the_page_as_one_piece() {
    let size = Size::new(1280.0, 800.0);
    let mut driver = FrameDriver::new(size);
    shell::register(&mut driver);
    let ui = Ui::new(driver.elements().runtime(), None, std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)));
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
        let (pixels, _) = NativeRenderer::new().render_to_pixels(driver.scene(), 1280, 800, ui.pal().ground).unwrap();
        pixels.data().to_vec()
    };
    let (a, b) = (frame_at(100.0), frame_at(100.6));
    // Inside the settings page's scrolling column.
    let row = |img: &[u8], y: usize| img[(y * 1280 + 360) * 4..(y * 1280 + 1100) * 4].to_vec();
    for y in 140..700 {
        assert!(row(&a, y + 1) == row(&b, y), "row {y} of the page did not move by exactly one pixel");
    }
}

/// Time a scroll step the way the window paints it (damage-limited, kept
/// pixels), at device ratios 1 and 2: `cargo test --release -p wright
/// --features ui scroll_frame_cost -- --ignored --nocapture`.
#[test]
#[ignore]
fn scroll_frame_cost() {
    use vieww::paint::Damage;
    let home = temp("scrollbench");
    std::env::set_var("WRIGHT_HOME", &home);
    let size = Size::new(1280.0, 800.0);
    let mut driver = FrameDriver::new(size);
    shell::register(&mut driver);
    let ui = Ui::new(driver.elements().runtime(), None, std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)));
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
            r.render_in_place(&driver.scene().scaled(dpr), w, h, ui.pal().ground).unwrap();
        }
        let (mut paint, mut area) = (Duration::ZERO, 0.0);
        for _ in 0..30 {
            ui.form.scroll.by(7.0);
            t += 1;
            driver.draw_frame_at(Duration::from_millis(t * 16));
            let mut damage = Damage::new(vieww::foundation::Rect::new(0.0, 0.0, w as f32, h as f32));
            for r0 in driver.damage().regions() {
                damage.add(vieww::foundation::Rect::new(r0.left * dpr, r0.top * dpr, r0.right * dpr, r0.bottom * dpr));
            }
            area += damage.covered_area() / (w as f32 * h as f32);
            let a = Instant::now();
            r.render_retained_in_place(&driver.scene().scaled(dpr), &damage, w, h, ui.pal().ground).unwrap();
            paint += a.elapsed();
        }
        eprintln!("dpr {dpr}: {:5.1} ms/frame repaint, {:3.0}% of the window damaged per scroll step", paint.as_secs_f64() * 1e3 / 30.0, area * 100.0 / 30.0);
    }
}

/// A two-finger pinch on a trackpad: fingers together shows every session,
/// apart goes back; a small twitch does nothing, and one pinch moves once.
#[test]
fn pinching_opens_and_closes_the_overview() {
    use vieww_platform_winit::{Pinch, PinchPhase};
    let mut driver = FrameDriver::new(Size::new(1280.0, 800.0));
    shell::register(&mut driver);
    let ui = Ui::new(driver.elements().runtime(), None, std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)));
    ui.splash.set(false);
    let pinch = |delta: f32, phase| ui.on_pinch(Pinch { delta, phase });
    assert!(!pinch(0.0, PinchPhase::Started));
    assert!(!pinch(-0.05, PinchPhase::Moved), "a twitch is not a pinch");
    assert!(!ui.overview.get());
    assert!(pinch(-0.1, PinchPhase::Moved));
    assert!(ui.overview.get(), "fingers together: the overview");
    assert!(!pinch(-0.3, PinchPhase::Moved) && !pinch(0.4, PinchPhase::Moved), "one pinch, one move");
    assert!(ui.overview.get());
    pinch(0.0, PinchPhase::Ended);
    assert!(pinch(0.2, PinchPhase::Moved), "Ctrl + wheel (Windows) has no Started; a new pinch counts");
    assert!(!ui.overview.get(), "fingers apart: back");
    ui.help.set(true);
    pinch(0.0, PinchPhase::Ended);
    assert!(!pinch(-0.5, PinchPhase::Moved), "not while a panel is open");
}

#[test]
fn speech_alone_starts_a_session_on_home() {
    // Voice-first (the memo's item 6): the mic is the main way to start
    // on Home. Driving the same path the mic worker takes — a transcript
    // lands in the inbox, the shell pumps — the session must begin from
    // the spoken words alone, with the Confirm Gate still ahead of it.
    let home = temp("voice-first");
    std::env::set_var("WRIGHT_HOME", &home);
    let size = Size::new(1280.0, 800.0);
    let mut driver = FrameDriver::new(size);
    shell::register(&mut driver);
    let ui = Ui::new(driver.elements().runtime(), None, std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)));
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
        .push((ui.home.id, Ok("A command line tool that counts words in files, with tests".to_string())));
    ui.pump(&mut driver);

    assert_eq!(ui.sessions.get().len(), 1, "speech alone starts the session");
    assert_eq!(ui.home.voice.get(), super::session::Voice::Idle, "the composer settles");
    // The spoken words are the session's opening message: typing them
    // and pressing send does the same thing; this test pins that.
    let chat = ui.sessions.get()[0].chat.get();
    assert!(!chat.is_empty(), "the session opened with the spoken words");
    assert!(chat[0].1.contains("counts words in files"), "chat: {chat:?}");
    assert!(chat[0].0, "the first message is the person's");
    std::env::remove_var("WRIGHT_HOME");
}
