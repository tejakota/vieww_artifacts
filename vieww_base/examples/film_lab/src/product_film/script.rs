//! script — the session: the film's frame-indexed script, driving the
//! **actual viewwstudio** through the real input pipeline.
//!
//! No mock, no stand-in — `mount` builds the real app (`Shell` + `Studio`
//! + its element runtime + its compile pipeline) on the film's own
//! `FrameDriver`, and `apply` walks a frame-indexed script through it:
//! buffers are edited with the studio's own editor, taps go through
//! `handle_pointer` (the real gesture disambiguation), compiles are the
//! real `rustc` pipeline (say-codegen → counter.rs → cdylib → dlopen),
//! and the overlays the film toggles (`show_damage`) are the studio's
//! own inspectors.
//!
//! **Determinism.** Every action lands at a fixed film-time, and the
//! harness applies them in order, once each. Compiles are settled
//! off-clock: `Render` starts the real job, the film holds the real
//! `Compiling` state for its scripted frames, then `Settle` polls to
//! completion before the next frame is drawn. A re-run replays the same
//! actions at the same times and produces the same pixels.
//!
//! The tab indices, the tap coordinates and the workspace are all
//! properties of this checkout — the workspace is a committed fixture
//! (`product_film_workspace`, the studio's demo screens plus the film's
//! say program), and the tap constants were hit-tested against the real
//! tree by the keynote harness that shares the workspace's layout.

use std::path::PathBuf;
use std::time::Duration;

use vieww_foundation::{Offset, PointerEvent, PointerId, Size, TextEditingValue, TextSelection};
use vieww_render::FrameDriver;
use viewwstudio::compile::{Session, Toolchain};
use viewwstudio::state::{Platform, RightTab, Studio, View};
use viewwstudio::command::Command;
use viewwstudio::{Shell, Workspace};
use vieww_widget::prelude::*;
use vieww_widget::WidgetNode;

use super::{H, W};

/// The film's window — the studio runs full-bleed at logical master size.
/// (SCALE_FACTOR never changes this: the studio lays out in logical
/// points; only the raster multiplies.)
pub const WINDOW: Size = Size::new(W, H);

/// The workspace the film opens: the studio's own demo screens plus the
/// film's say program (`counter.say`, the grammar verbatim from the
/// studio's `docs/06-say.md`). Committed beside this source.
pub fn workspace_path() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/product_film_workspace"))
}

/// The target directory the toolchain compiles against: **the release
/// build the film binary itself links.** Host and guest are one
/// compilation — the one arrangement where the preview's `TypeId`s
/// match and the compiled screen actually mounts (see the studio's own
/// Cargo.toml note on why).
pub fn target_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/release"))
}

/// Mount the actual studio on the film's driver. Returns the studio
/// handle; the caller mounts [`shell_root`] as (part of) the root.
pub fn mount(driver: &mut FrameDriver) -> Studio {
    viewwstudio::install(driver);
    let runtime = driver.elements().runtime().clone();
    let workspace = Workspace::open(&workspace_path());
    let studio = Studio::with_workspace(&runtime, workspace)
        .with_toolchain(Toolchain::discover(&target_dir()), Session::new(0x51_2F1D).ok());
    studio.dark.set(true);
    studio.note_window_size(WINDOW);
    studio
}

/// A studio scene's root: the real Shell full-bleed, with the film's
/// overlay chrome riding on top.
pub fn shell_root(studio: &Studio, overlay: WidgetNode) -> WidgetNode {
    Stack::new()
        .push(Positioned::fill().child(Shell { studio: studio.clone() }))
        .push(Positioned::fill().child(overlay))
        .into()
}

// ── The tab indices — the workspace's own entry order ───────────────────────
//
// `Workspace::open` walks the film workspace breadth-first, sorted,
// `.rs` only — so counter.say never rides in with the entry points and
// the script opens it by path (the way a person would: the file tree).
// The open tabs: card_grid.rs (0) · landing_screen.rs (1) · live.rs (2)
// · settings_form.rs (3) · screen.rs (4) · …

pub const TAB_CARD_GRID: usize = 0;
pub const TAB_LIVE: usize = 2;

// ── The tap coordinates — hit-tested against the real tree ──────────────────

/// The live demo's Home screen "Open settings" button — the first
/// tappable control the scan finds (the rows are text; the button acts).
/// Verified by `pfcal`: the scan's route-changing tap.
pub const TAP_LIVE_ROW: Offset = Offset::new(1576.0, 450.0);

/// The compiled counter's *Add one* button (in the preview pane).
/// Verified by `pfcal`: the pixel-hunted button centre, whose tap moved
/// the count 0 → 1 (the receipts in `calibrate/`).
pub const TAP_ADD_ONE: Offset = Offset::new(1594.0, 346.0);

// ── The action vocabulary ────────────────────────────────────────────────────

/// One scripted touch of the real studio.
#[derive(Clone)]
pub enum Action {
    /// Set the active editor tab (the buffers the workspace opened).
    ActiveTab(usize),
    /// Open a buffer by workspace-relative path — the file-tree click.
    OpenPath(String),
    /// Run a studio command through the real command bus.
    Run(Command),
    /// Accept the live-preview caution dialog.
    AcceptLive,
    /// Edit the live.rs buffer: replace the Home screen's title.
    SetLiveTitle(String),
    /// Edit the live.rs buffer: replace a row's text.
    SetLiveRow(String),
    /// Set the counter.say buffer to the first `n` characters of the
    /// film's say program (typing in multi-char bursts).
    TypeSay(usize),
    /// Fire the real Render: say-codegen → rustc → cdylib.
    Render,
    /// Poll the compile to completion, off-clock.
    Settle,
    /// A pointer press at a window position (the real pipeline).
    PointerDown(Offset),
    /// A pointer release (the tap fires here).
    PointerUp(Offset),
    /// Switch the sidebar view.
    View(View),
    /// Switch the right pane's tab.
    RightTab(RightTab),
    /// Simulate a preview platform.
    Platform(Platform),
    /// Set the studio accent by name (the token editor's live theming).
    Accent(String),
    /// Toggle the studio's own damage inspector.
    ShowDamage(bool),
    /// Fold the bottom panel away (the film's wide frame).
    PanelOpen(bool),
}

/// The script — every touch of the real studio, in film seconds. The
/// witness ladder's taps are exactly these times (see `taps()`).
pub fn script() -> Vec<(f32, Action)> {
    vec![
        // ── C01 · the studio opens (128–138) ────────────────────────────
        (128.4, Action::PanelOpen(false)),       // the wide frame
        (129.0, Action::ActiveTab(TAB_LIVE)),
        // ── C02 · first paint (138–150) ─────────────────────────────────
        (139.0, Action::Run(Command::LivePreview)), // the caution dialog — a real dialog
        (140.2, Action::AcceptLive),                 // the demo mounts           [tap 1]
        // ── C03 · live compose (150–162) ────────────────────────────────
        (151.0, Action::ShowDamage(true)),           // the real damage overlay wakes
        (154.0, Action::SetLiveTitle("My own inbox".into())), // the keystroke [tap 2]
        (157.5, Action::SetLiveRow("Release cut is ready".into())),
        (159.8, Action::ShowDamage(false)),
        // ── C04 · the tap (162–170) ─────────────────────────────────────
        (166.38, Action::PointerDown(TAP_LIVE_ROW)),
        (166.50, Action::PointerUp(TAP_LIVE_ROW)),   // the route changes         [tap 3]
        // ── C05 · say → rust (170–186) ──────────────────────────────────
        (171.0, Action::OpenPath("counter.say".into())),
        (171.4, Action::TypeSay(40)),
        (172.4, Action::TypeSay(96)),
        (173.4, Action::TypeSay(152)),
        (174.4, Action::TypeSay(208)),
        (175.4, Action::TypeSay(usize::MAX)),        // the whole program
        (176.2, Action::Render),                     // rustc runs                [tap 4]
        (177.0, Action::Settle),
        (178.5, Action::PointerDown(TAP_ADD_ONE)),
        (178.62, Action::PointerUp(TAP_ADD_ONE)),    // count 0 → 1               [tap 5]
        // ── C06 · state carries (186–198) ───────────────────────────────
        // One word of the say program changes (the heading), then Render
        // again — the `keep`ed count must survive the recompile.
        (187.8, Action::SetLiveTitle("::carry".into())),
        (188.1, Action::Render),
        (189.0, Action::Settle),                      // count is STILL 1
        // ── C07 · three frames (198–210) ────────────────────────────────
        (200.5, Action::Platform(Platform::Android)),
        (203.0, Action::Platform(Platform::Ios)),
        (205.5, Action::Platform(Platform::Desktop)),
        // ── C08 · the tokens (210–220) ──────────────────────────────────
        (211.0, Action::View(View::Tokens)),         // the token editor
        (214.5, Action::Accent("Teal".into())),      // live re-theming, one accent
        (216.2, Action::Accent("Purple".into())),    // …and back to the brand's
        // ── C09 · build ships (220–232) ─────────────────────────────────
        (221.0, Action::View(View::Toolchain)),
        (223.3, Action::View(View::Export)),         // the build view           [tap 6]
        (228.8, Action::RightTab(RightTab::Devices)), // the package lands        [tap 7]
    ]
}

// ── Applying the script ──────────────────────────────────────────────────────

/// The film's say program, read from the workspace at mount (the same
/// file the studio will compile — the film cannot type anything the
/// buffer does not contain).
pub fn say_source() -> String {
    std::fs::read_to_string(workspace_path().join("counter.say")).unwrap_or_default()
}

/// Build a `TextEditingValue` for a whole-text set with the caret at
/// the end — how a paste looks to the editor.
fn set_text(text: String) -> TextEditingValue {
    let end = text.chars().count();
    TextEditingValue {
        selection: TextSelection::collapsed(end),
        text,
        ..Default::default()
    }
}

/// Apply every action whose time has come (at or before `abs`), in
/// order, once each. `cursor` is how far the walk has already applied;
/// the caller owns it across frames so the census, the master and any
/// replay all agree.
///
/// **Pointer pairs never straddle a `set_root`.** The harness sets a
/// fresh root every frame, and `FrameDriver::set_root` clears all live
/// pointers (a pointer's route into a tree being thrown away is
/// meaningless afterwards) — so a `PointerDown` whose `PointerUp` lands
/// in a later frame would arrive at an arena with no memory of the
/// press, and the tap would silently die. A pair is therefore applied
/// **atomically, in the frame where its release is due**: the down is
/// deferred to the up's frame and both dispatch back-to-back against
/// the same drawn tree. The tap lands at its release time — which is
/// when the witness ladder counts it anyway.
pub fn apply_up_to(driver: &mut FrameDriver, studio: &Studio, abs: f32, cursor: &mut usize) {
    apply_list(driver, studio, &script(), abs, cursor);
}

/// Apply **any** session list — this film's own, or a guest film's —
/// up to `abs`, resuming from `cursor`.
///
/// `studio_film` drives the same app through this door: one vocabulary,
/// one set of rules. The pointer-pair rule is the list's, not the
/// caller's — a `PointerDown` whose release is not yet due is deferred,
/// and both halves land in one frame.
pub fn apply_list(
    driver: &mut FrameDriver,
    studio: &Studio,
    script: &[(f32, Action)],
    abs: f32,
    cursor: &mut usize,
) {
    let clock = Duration::from_secs_f64(abs.max(0.0) as f64);
    while *cursor < script.len() {
        let (at, action) = &script[*cursor];
        if *at > abs {
            break;
        }
        // A press whose release is not yet due is deferred — it will
        // dispatch together with the release, inside one frame.
        if matches!(action, Action::PointerDown(_)) {
            if let Some((up_at, Action::PointerUp(_))) = script.get(*cursor + 1) {
                if *up_at > abs {
                    break;
                }
            }
        }
        apply_one(driver, studio, action.clone(), clock);
        *cursor += 1;
    }
}

/// One action, executed against the real studio.
fn apply_one(driver: &mut FrameDriver, studio: &Studio, action: Action, clock: Duration) {
    match action {
        Action::ActiveTab(i) => {
            studio.active_buffer.set(i);
        }
        Action::OpenPath(name) => {
            studio.open_path(workspace_path().join(name));
        }
        Action::Run(command) => {
            studio.run(command);
        }
        Action::AcceptLive => {
            studio.accept_live_preview();
        }
        Action::SetLiveTitle(title) => {
            // The carry marker: "::carry" is the state-carry edit's cue —
            // it edits the *say* buffer's heading, not live.rs.
            if title == "::carry" {
                edit_say_heading(studio);
            } else {
                edit_live(studio, |text| {
                    text.replace("title \"My own inbox\"", &format!("title \"{title}\""))
                        .replace("title \"Inbox\"", &format!("title \"{title}\""))
                });
            }
        }
        Action::SetLiveRow(row) => {
            edit_live(studio, |text| {
                text.replace("Draft for v0.3 is ready to review", &row)
            });
        }
        Action::TypeSay(n) => {
            type_say(studio, n);
        }
        Action::Render => {
            studio.render();
        }
        Action::Settle => {
            settle(studio);
        }
        Action::PointerDown(at) => {
            driver.handle_pointer(&PointerEvent::down(PointerId(1), at, clock));
        }
        Action::PointerUp(at) => {
            driver.handle_pointer(&PointerEvent::up(PointerId(1), at, clock));
        }
        Action::View(view) => {
            studio.view.set(view);
        }
        Action::RightTab(tab) => {
            studio.right_tab.set(tab);
        }
        Action::Platform(platform) => {
            studio.platform.set(platform);
        }
        Action::Accent(name) => {
            studio.accent.set(name);
        }
        Action::ShowDamage(on) => {
            studio.show_damage.set(on);
        }
        Action::PanelOpen(on) => {
            studio.panel_open.set(on);
        }
    }
}

/// Edit the active buffer with a whole-text transform (the editor sees
/// a paste; its comforts do not fire).
fn edit_with(studio: &Studio, f: impl FnOnce(String) -> String) {
    let Some(buffer) = studio.active() else { return };
    let mut value = buffer.value.clone();
    value.text = f(value.text.clone());
    value.selection = TextSelection::collapsed(value.text.chars().count());
    value.composing = None;
    studio.edit(value);
}

/// Edit the live.rs buffer (whatever tab is active is not trusted — the
/// film names its target).
fn edit_live(studio: &Studio, f: impl FnOnce(String) -> String) {
    let index = find_tab(studio, "live.rs");
    let Some(i) = index else { return };
    studio.active_buffer.set(i);
    edit_with(studio, f);
}

/// Type the say program: set the counter.say buffer to its first `n`
/// characters (the whole program when `n` is `usize::MAX`).
fn type_say(studio: &Studio, n: usize) {
    let source = say_source();
    let prefix: String = if n == usize::MAX {
        source.clone()
    } else {
        source.chars().take(n).collect()
    };
    let Some(i) = find_tab(studio, "counter.say") else { return };
    studio.active_buffer.set(i);
    let Some(buffer) = studio.active() else { return };
    // Only edit when the text actually changes — the undo history stays
    // honest (one entry per burst, none for re-sets).
    if buffer.value.text == prefix {
        return;
    }
    studio.edit(set_text(prefix));
}

/// The state-carry edit: one word of the say program changes (the
/// heading), so the recompile mounts a *different* screen — and the
/// `keep`ed count must ride across it.
fn edit_say_heading(studio: &Studio) {
    let Some(i) = find_tab(studio, "counter.say") else { return };
    studio.active_buffer.set(i);
    edit_with(studio, |text| text.replace("a heading \"Counter\"", "a heading \"Counted\""));
}

/// Which tab holds `name`, if the workspace opened it.
fn find_tab(studio: &Studio, name: &str) -> Option<usize> {
    let buffers = studio.buffers.get();
    buffers
        .iter()
        .position(|b| b.name == name)
}

/// Poll the compile to completion — the wall-clock wait the film takes
/// off-clock so its frames stay deterministic.
fn settle(studio: &Studio) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
    while studio.is_compiling() && std::time::Instant::now() < deadline {
        if !studio.poll_compile() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

/// Is a compile in flight (for the harness's probes)?
pub fn is_compiling(studio: &Studio) -> bool {
    studio.is_compiling()
}
