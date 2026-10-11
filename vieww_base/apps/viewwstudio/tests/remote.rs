//! The out-of-process preview, end to end: real `rustc`, a real child
//! process, real pixels back over the pipe.
//!
//! The assertion that matters most is the one a test process can make by
//! still being alive: a guest that aborts — which no `catch_unwind` can catch,
//! on any platform — takes its child with it and leaves this process, and a
//! fresh child, working. Skips, loudly, without a built workspace, like
//! `pipeline.rs`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use vieww_foundation::{Color, Offset};
use viewwstudio::compile::{compile, Session, Toolchain};
use viewwstudio::remote::{Environment, Pointer, RemoteError, RemotePreview};
use viewwstudio::state::Platform;

fn child() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_viewwstudio-preview"))
}

fn target() -> PathBuf {
    let exe = std::env::current_exe().expect("a test binary");
    exe.parent()
        .and_then(Path::parent)
        .expect("target/debug")
        .to_path_buf()
}

const PROBE: &str = r#"
use vieww::prelude::*;
#[derive(Debug)]
pub struct Probe;
impl Widget for Probe {
    fn debug_name(&self) -> &'static str { "Probe" }
    fn kind(&self) -> WidgetKind<'_> { WidgetKind::Composed }
    fn build(&self, _ctx: &BuildContext) -> WidgetNode { Container::new().into() }
}
pub fn screen() -> impl Widget { Probe }
"#;

/// The toolchain with the `vieww` candidate the **child** links — not this
/// test binary, which is a different compilation with different features.
/// Probed through a child, advancing past ABI mismatches exactly as the
/// studio does.
fn toolchain() -> Option<Toolchain> {
    static RESOLVED: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    let base = match Toolchain::discover(&target()) {
        Ok(t) => t,
        Err(error) => {
            eprintln!("skipping: {error}");
            return None;
        }
    };
    let candidate = *RESOLVED.get_or_init(|| {
        let session = Session::new(0x7E70).ok()?;
        for candidate in 0.. {
            let toolchain = base.with_candidate(candidate)?;
            let Ok(compiled) = compile(&toolchain, &session, PROBE, "probe.rs") else {
                continue;
            };
            let library = compiled.library?;
            let mut remote = RemotePreview::spawn(&child()).ok()?;
            match remote.load(&library, toolchain.sysroot_lib.as_deref()) {
                Ok(()) => return Some(candidate),
                Err(RemoteError::AbiMismatch(_)) => continue,
                Err(error) => {
                    eprintln!("probe candidate {candidate}: {error}");
                    continue;
                }
            }
        }
        None
    });
    let Some(candidate) = candidate else {
        eprintln!("skipping: no `vieww` rlib matches the preview child's compilation");
        return None;
    };
    base.with_candidate(candidate)
}

fn compiled(toolchain: &Toolchain, seed: u64, source: &str) -> PathBuf {
    let session = Session::new(seed).expect("a temp directory");
    let compiled = compile(toolchain, &session, source, "screen.rs").expect("rustc ran");
    assert!(
        compiled
            .diagnostics
            .iter()
            .all(|d| d.severity != viewwstudio::state::Severity::Error),
        "{:?}",
        compiled.diagnostics
    );
    // The session directory must outlive the load; leak it for the test.
    std::mem::forget(session);
    compiled.library.expect("it compiles")
}

fn loaded(toolchain: &Toolchain, library: &Path) -> RemotePreview {
    let mut remote = RemotePreview::spawn(&child()).expect("the child starts");
    remote
        .load(library, toolchain.sysroot_lib.as_deref())
        .expect("the child loads it");
    remote
}

const TAPPABLE: &str = concat!(
    include_str!("remote/guest_prelude.rs"),
    r#"
#[derive(Debug)]
pub struct Screen;

impl Widget for Screen {
    fn debug_name(&self) -> &'static str { "Screen" }
    fn kind(&self) -> WidgetKind<'_> { WidgetKind::Composed }
    fn create_state(&self) -> Option<Box<dyn ElementState>> { Some(Box::new(Tapped::default())) }
    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        let color = if tapped(ctx) { Color::hex(0xFF_2244) } else { Color::hex(0x22_88FF) };
        GestureDetector::new().on_tap(on_tap(ctx)).child(Container::new().color(color)).into()
    }
}

pub fn screen() -> impl Widget { Screen }
"#
);

fn pixel(frame: &viewwstudio::remote::Frame, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * frame.width + x) * 4) as usize;
    [
        frame.rgba[i],
        frame.rgba[i + 1],
        frame.rgba[i + 2],
        frame.rgba[i + 3],
    ]
}

#[test]
fn a_screen_renders_in_the_child_and_a_tap_reaches_it() {
    let Some(toolchain) = toolchain() else { return };
    let library = compiled(&toolchain, 0x7E71, TAPPABLE);
    let mut remote = loaded(&toolchain, &library);
    let env = Environment::of(Platform::Android);

    let first = remote
        .frame(&env, 1.0, Duration::ZERO, Color::BLACK, &[])
        .expect("a frame");
    assert_eq!(
        (first.width, first.height),
        (412, 915),
        "the device's logical size at 1x"
    );
    let (cx, cy) = (first.width / 2, first.height / 2);
    assert_eq!(
        pixel(&first, cx, cy),
        [0x22, 0x88, 0xFF, 0xFF],
        "the guest's colour, drawn by the child"
    );

    let centre = Offset::new(206.0, 457.0);
    let tapped = remote
        .frame(
            &env,
            1.0,
            Duration::from_millis(16),
            Color::BLACK,
            &[Pointer::Down(centre), Pointer::Up(centre)],
        )
        .expect("a frame after the tap");
    assert_eq!(
        pixel(&tapped, cx, cy),
        [0xFF, 0x22, 0x44, 0xFF],
        "the tap reached the guest and rebuilt it"
    );

    // Scale: physical pixels for a 2x window.
    let crisp = remote
        .frame(&env, 2.0, Duration::from_millis(32), Color::BLACK, &[])
        .expect("a 2x frame");
    assert_eq!((crisp.width, crisp.height), (824, 1830));
    // And the environment changed under it without losing the tap's state.
    let dark = Environment { dark: true, ..env };
    let after = remote
        .frame(&dark, 1.0, Duration::from_millis(48), Color::BLACK, &[])
        .expect("a frame in dark mode");
    assert_eq!(
        pixel(&after, cx, cy),
        [0xFF, 0x22, 0x44, 0xFF],
        "guest state survives an environment change"
    );
}

#[test]
fn a_guest_that_aborts_takes_its_child_and_not_the_studio() {
    let Some(toolchain) = toolchain() else { return };
    // `abort` on the build after a tap. No unwinder anywhere catches this.
    let library = compiled(
        &toolchain,
        0x7E72,
        &format!(
            "{}{}",
            include_str!("remote/guest_prelude.rs"),
            r#"
#[derive(Debug)]
pub struct Screen;
impl Widget for Screen {
    fn debug_name(&self) -> &'static str { "Screen" }
    fn kind(&self) -> WidgetKind<'_> { WidgetKind::Composed }
    fn create_state(&self) -> Option<Box<dyn ElementState>> { Some(Box::new(Tapped::default())) }
    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        if tapped(ctx) {
            eprintln!("the guest is about to abort");
            std::process::abort();
        }
        GestureDetector::new().on_tap(on_tap(ctx)).child(Container::new().color(Color::hex(0x22_88FF))).into()
    }
}
pub fn screen() -> impl Widget { Screen }
"#
        ),
    );
    let mut remote = loaded(&toolchain, &library);
    let env = Environment::of(Platform::Ios);
    remote
        .frame(&env, 1.0, Duration::ZERO, Color::BLACK, &[])
        .expect("the first frame draws");
    let c = Offset::new(196.0, 426.0);
    let error = remote
        .frame(
            &env,
            1.0,
            Duration::from_millis(16),
            Color::BLACK,
            &[Pointer::Down(c), Pointer::Up(c)],
        )
        .expect_err("the child died");
    let RemoteError::Crashed { stderr, .. } = &error else {
        panic!("expected a crash, got {error:?}");
    };
    assert!(
        stderr.contains("the guest is about to abort"),
        "the guest's last words come back: {stderr:?}"
    );
    // A dead preview stays dead, and says the same thing each time.
    assert_eq!(
        remote
            .frame(&env, 1.0, Duration::ZERO, Color::BLACK, &[])
            .unwrap_err(),
        error
    );

    // This process is alive to read all that — and a fresh child works.
    let good = compiled(&toolchain, 0x7E73, TAPPABLE);
    let mut fresh = loaded(&toolchain, &good);
    let frame = fresh
        .frame(&env, 1.0, Duration::ZERO, Color::BLACK, &[])
        .expect("a new child draws");
    assert_eq!(
        pixel(&frame, frame.width / 2, frame.height / 2),
        [0x22, 0x88, 0xFF, 0xFF]
    );
}

#[test]
fn a_panic_on_a_later_rebuild_is_survived_on_every_platform() {
    let Some(toolchain) = toolchain() else { return };
    let library = compiled(
        &toolchain,
        0x7E74,
        &format!(
            "{}{}",
            include_str!("remote/guest_prelude.rs"),
            r#"
#[derive(Debug)]
pub struct Screen;
impl Widget for Screen {
    fn debug_name(&self) -> &'static str { "Screen" }
    fn kind(&self) -> WidgetKind<'_> { WidgetKind::Composed }
    fn create_state(&self) -> Option<Box<dyn ElementState>> { Some(Box::new(Tapped::default())) }
    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        if tapped(ctx) { panic!("a panic on a later rebuild"); }
        GestureDetector::new().on_tap(on_tap(ctx)).child(Container::new().color(Color::hex(0x22_88FF))).into()
    }
}
pub fn screen() -> impl Widget { Screen }
"#
        ),
    );
    let mut remote = loaded(&toolchain, &library);
    let env = Environment::of(Platform::Ios);
    remote
        .frame(&env, 1.0, Duration::ZERO, Color::BLACK, &[])
        .expect("first frame");
    let c = Offset::new(196.0, 426.0);
    let result = remote.frame(
        &env,
        1.0,
        Duration::from_millis(16),
        Color::BLACK,
        &[Pointer::Down(c), Pointer::Up(c)],
    );
    // With one shared libstd (Linux, macOS) the child's element tree catches
    // it and draws the placeholder; without (MSVC) the child dies. Either is
    // survival — the studio is this process, and it is still here.
    match result {
        Ok(frame) => assert_ne!(
            pixel(&frame, 196, 426),
            [0x22, 0x88, 0xFF, 0xFF],
            "the placeholder replaced the screen"
        ),
        Err(RemoteError::Crashed { .. }) => {
            if !cfg!(windows) {
                panic!("only MSVC lacks the shared libstd, so only Windows may crash here");
            }
        }
        Err(other) => panic!("{other:?}"),
    }
}

/// The studio's own Render, with isolation on: the scratch buffer compiles,
/// loads in a child, and the preview pane draws the child's frame.
#[test]
fn the_studio_renders_into_a_child_process_when_isolation_is_on() {
    use viewwstudio::state::{PreviewState, Studio};
    let Some(toolchain) = toolchain() else { return };
    let mut driver = vieww_render::FrameDriver::new(vieww_foundation::Size::new(1400.0, 900.0));
    viewwstudio::install(&mut driver);
    let runtime = driver.elements().runtime().clone();
    let studio = Studio::new(&runtime).with_toolchain(Ok(toolchain), Session::new(0x7E75).ok());
    studio.isolated_preview.set(true);
    studio.render();
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    // `poll_compile` finishes a Render — and on an ABI mismatch starts the
    // next candidate's, which is the studio's own walk — so poll until the
    // state settles.
    loop {
        studio.poll_compile();
        if studio.preview.get() != PreviewState::Compiling {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "the Render finished");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        studio.preview.get(),
        PreviewState::Rendered,
        "{:?}",
        studio.output.get()
    );
    assert!(
        studio.preview_screen.get().is_none(),
        "nothing was dlopened into the studio"
    );
    let remote = studio
        .preview_remote
        .get()
        .expect("the preview runs in a child");
    assert!(remote.borrow_mut().crashed().is_none());

    // The pane draws it: mount the shell, and the RemoteScreen inside it asks
    // the child for a frame during the build.
    driver.set_root(viewwstudio::Shell {
        studio: studio.clone(),
    });
    driver.draw_frame();
    assert!(
        remote.borrow_mut().crashed().is_none(),
        "the child answered the pane"
    );
    assert!(
        remote.borrow_mut().frames() >= 1,
        "the pane's RemoteScreen asked the child for a frame"
    );
    let frame = remote
        .borrow_mut()
        .frame(
            &Environment::of(Platform::Ios),
            1.0,
            Duration::ZERO,
            Color::BLACK,
            &[],
        )
        .expect("the child is still drawing");
    assert!(
        frame.rgba.chunks_exact(4).any(|p| p != [0, 0, 0, 255]),
        "the scratch screen drew something"
    );
}
