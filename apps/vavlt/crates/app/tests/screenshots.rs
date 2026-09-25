//! Every screen, rendered to a PNG through the real GPU backend.
//!
//! ```console
//! cargo test -p vavlt-app --test screenshots --release -- --nocapture
//! ```
//!
//! # Why this exists rather than a layout assertion
//!
//! A layout test can prove a box is 340 wide. It cannot prove the caption under
//! the heading is legible against the card it sits on, that the tier control's
//! two figures line up, or that the light theme's dividers are visible at all —
//! and those are the failures a UI actually has. Every screen in this app was
//! checked by looking at the file this writes.
//!
//! Each screen is rendered four ways: phone and desktop, dark and light. The
//! same widget tree in all four, which is the claim the whole port is built on
//! and therefore the one worth photographing.
//!
//! # It skips rather than fails without a GPU
//!
//! `GpuRenderer::headless` needs an adapter. On a machine with none — a CI box
//! with no `lavapipe`, say — every test here returns early rather than going
//! red, the same way `vieww`'s own pixel tests do. A screenshot suite that
//! fails on the absence of a screen is a suite people delete.
//!
//! Output lands in `target/screenshots/`.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use vavlt_app::model::{Screen, Tab, Thumb};
use vavlt_app::{Message, Platform, Scrolls, VavltApp, VavltState};
use vavlt_core::Tier;
use vavlt_engine::{Item, Outcome, Proof, Source};
use vieww::foundation::task::FrameWaker;
use vieww::foundation::{Color, Size};
use vieww::paint::gpu::{GpuError, GpuRenderer};
use vieww::prelude::*;

/// A phone. Pixel-ish logical size, the surface the design was drawn for.
const PHONE: Size = Size {
    width: 412.0,
    height: 915.0,
};

/// A desktop window, above the 640 breakpoint so the other metrics apply.
const DESKTOP: Size = Size {
    width: 1024.0,
    height: 768.0,
};

// --- the harness -----------------------------------------------------------

/// The process-wide renderer, or `None` on a machine with no usable adapter.
///
/// One device for the whole binary, never dropped, behind a mutex — the same
/// arrangement `vieww`'s own GPU tests use, and for the same reason: creating a
/// device per test and dropping them at exit segfaults in driver teardown.
fn renderer() -> Option<MutexGuard<'static, GpuRenderer>> {
    static GPU: OnceLock<Option<Mutex<GpuRenderer>>> = OnceLock::new();

    let slot = GPU.get_or_init(|| match GpuRenderer::headless() {
        Ok(renderer) => Some(Mutex::new(renderer)),
        Err(GpuError::NoAdapter) => {
            eprintln!("skipping screenshots: no graphics adapter");
            None
        }
        Err(error) => panic!("initialising the GPU backend: {error}"),
    });

    slot.as_ref().map(|gpu| gpu.lock().expect("the GPU mutex"))
}

/// A host that opens nothing.
///
/// Every screen this suite photographs is driven from state written directly,
/// so nothing here is ever called — which is itself worth having, because a
/// `Platform` that panics proves the screens do not reach for one while
/// drawing.
#[derive(Debug)]
struct NoPlatform(PathBuf);

impl Source for NoPlatform {
    fn read(&self, _handle: &str) -> anyhow::Result<Vec<u8>> {
        anyhow::bail!("the screenshot harness opens no files")
    }
}

impl Platform for NoPlatform {
    fn data_dir(&self) -> PathBuf {
        self.0.clone()
    }

    fn source(&self) -> Arc<dyn Source> {
        Arc::new(NoPlatform(self.0.clone()))
    }

    fn pick(&self, _max: usize, _reply: Sender<Message>, _waker: Arc<dyn FrameWaker>) {
        unreachable!("the screenshot harness never picks");
    }

    /// Runs the work inline.
    ///
    /// Not `unreachable!`: the fixtures deliver a real `Picked` message, which
    /// makes the app start its thumbnail loader — and a host that panicked
    /// there would mean the suite could only photograph states reached by
    /// writing signals behind the app's back. Inline is exactly right for a
    /// test: it resolves before the next line rather than at some point later.
    fn spawn(&self, work: Box<dyn FnOnce() + Send + 'static>) {
        work();
    }
}

/// A waker that counts nothing. No frames are scheduled here; they are drawn on
/// demand.
#[derive(Debug)]
struct Still;

impl FrameWaker for Still {
    fn wake(&self) {}
}

/// One rendering job: a name, a surface, a theme, and the state to draw.
struct Shot {
    name: &'static str,
    size: Size,
    dark: bool,
}

/// Build a tree, draw one frame, write a PNG.
///
/// Two frames, not one. The first mounts the tree, which is when `Animated`
/// elements are created — and they are created *at* their target, so a second
/// frame is not needed for the slide. It is needed for layout: `LayoutBuilder`
/// publishes its constraints during layout and rebuilds its subtree from them,
/// so the first frame draws the pre-measurement tree. Photographing that would
/// mean photographing a grid that has not been told how wide it is.
fn shoot(shot: &Shot, prepare: impl FnOnce(&Rc<VavltState>)) {
    let Some(mut gpu) = renderer() else {
        return;
    };

    let dir = std::env::temp_dir().join(format!("vavlt-shot-{}", shot.name));
    std::fs::create_dir_all(&dir).expect("scratch dir");

    let mut driver = FrameDriver::new(shot.size);
    let runtime = driver.elements().runtime().clone();

    let platform = Rc::new(NoPlatform(dir.clone()));
    let state = VavltState::new(&runtime, platform, Arc::new(Still));
    state.dark.set(shot.dark);
    prepare(&state);

    let scrolls = Rc::new(Scrolls::new(&runtime));
    scrolls.attach(driver.tickers());
    driver.set_root(VavltApp::new(state, scrolls));

    // A second of simulated time, in 16ms steps.
    //
    // Two things need it, and neither is satisfied by drawing three frames as
    // fast as the machine can. `LayoutBuilder` seeds itself with the *surface*
    // until layout has reported the constraints its subtree actually got, so
    // the first frame draws a photo grid that has been told it is as wide as
    // the window, and the second rebuilds against the measured width. And every
    // screen now arrives through `root::arrival`, which fades it up over
    // `motion::NAV` — three frames drawn back to back all land inside that, and
    // the PNG comes out very nearly blank. It did: `06-outcome-phone-dark drew
    // only 3 shapes`.
    //
    // Simulated rather than slept, so the picture does not depend on how loaded
    // the machine is.
    for step in 0..60 {
        driver.draw_frame_at(std::time::Duration::from_millis(step * 16));
    }

    let width = shot.size.width as u32;
    let height = shot.size.height as u32;
    let background = if shot.dark {
        Color::hex(0x24_2424)
    } else {
        Color::hex(0xFA_FAFA)
    };

    let (pixels, report) = gpu
        .render_to_pixels(driver.scene(), width, height, background)
        .expect("rendering the frame");

    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/screenshots")
        .join(format!("{}.png", shot.name));
    std::fs::create_dir_all(out.parent().expect("a parent")).expect("output dir");
    image::save_buffer(
        &out,
        pixels.data(),
        pixels.width(),
        pixels.height(),
        image::ColorType::Rgba8,
    )
    .expect("writing the PNG");

    // The shape count is the cheap guard that catches a blank screen without
    // anybody looking: a screen that drew nothing is a screen that drew nothing,
    // whatever the PNG viewer says about a nice flat colour.
    assert!(
        report.shapes > 5,
        "{} drew only {} shapes — that is a blank screen, not a frame",
        shot.name,
        report.shapes
    );

    println!(
        "{} -> {} ({} shapes)",
        shot.name,
        out.display(),
        report.shapes
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// Both themes at both sizes, from one description.
fn shoot_all(base: &'static str, prepare: impl Fn(&Rc<VavltState>) + Copy) {
    for (suffix, size) in [("phone", PHONE), ("desktop", DESKTOP)] {
        for (tone, dark) in [("dark", true), ("light", false)] {
            let name: &'static str = Box::leak(format!("{base}-{suffix}-{tone}").into_boxed_str());
            shoot(&Shot { name, size, dark }, prepare);
        }
    }
}

// --- fixtures --------------------------------------------------------------

/// A recognisable tile: four quadrants, so `BoxFit::Cover` cropping and the
/// excluded-tile opacity are both visible rather than being flat colour.
fn swatch(seed: u8) -> Thumb {
    const EDGE: u32 = 64;
    let mut pixels = Vec::with_capacity((EDGE * EDGE * 4) as usize);
    for y in 0..EDGE {
        for x in 0..EDGE {
            let quadrant = u32::from(x > EDGE / 2) + u32::from(y > EDGE / 2) * 2;
            let base = seed.wrapping_add((quadrant as u8) * 40);
            pixels.extend_from_slice(&[base, base.wrapping_add(70), base.wrapping_add(130), 255]);
        }
    }
    Thumb::new(pixels, EDGE)
}

/// Eleven files, as a picker would report them.
///
/// Sizes are real camera-photo sizes, so the estimate the app computes from
/// them — and therefore every figure in these screenshots — is the number the
/// shipping arithmetic produces rather than a caption somebody typed.
fn picked_items() -> Vec<Item> {
    (0..11)
        .map(|index| {
            let name = if index % 7 == 3 {
                format!("IMG_{:04}.dng", 1200 + index)
            } else {
                format!("IMG_{:04}.jpg", 1200 + index)
            };
            Item::new(
                format!("content://media/picker/{index}"),
                name,
                4_000_000 + (index as u64) * 1_700_000,
            )
        })
        .collect()
}

/// A picked selection, priced by the app itself, with no run against it yet.
///
/// Goes through `deliver` rather than writing signals: the manifest hash, the
/// estimate, the audit entry and the navigation to the plan screen are all
/// produced by the code that ships, so a screenshot that looks right is
/// evidence the flow is right.
fn with_selection(state: &Rc<VavltState>) {
    state.deliver(Message::Picked(picked_items()));
    for index in 0..11usize {
        state.deliver(Message::Thumb {
            index,
            thumb: swatch((index as u8).wrapping_mul(31)),
        });
    }
    // One left out, so the excluded mark and the "1 left out" line are both in
    // frame — and so the manifest hash in the screenshot is the one that
    // exclusion produced.
    state.toggle_photo(6);
    state.go(Screen::Root);
}

/// A run, one outcome at a time, exactly as the engine reports them.
fn run_to_completion(state: &Rc<VavltState>, upto: usize) {
    with_selection(state);
    state.go(Screen::Plan);
    state.start_run();

    let items = picked_items();
    let mut delivered = 0;
    for (index, item) in items.iter().enumerate() {
        if index == 6 {
            continue; // excluded, so the engine never sees it
        }
        if delivered >= upto {
            break;
        }
        delivered += 1;

        let proof = match index {
            3 => Proof::Skipped,
            5 => Proof::Failed,
            9 => Proof::Unqualified,
            7 => Proof::PixelLossless,
            _ => Proof::Proven,
        };
        state.deliver(Message::RunFile {
            index,
            outcome: Outcome {
                name: item.name.clone(),
                codec: "lepton".into(),
                before: item.bytes,
                after: item.bytes * 4 / 5,
                proof,
                note: if proof == Proof::Failed {
                    "lepton: unsupported progressive scan".into()
                } else {
                    String::new()
                },
                fell_back: false,
            },
        });
    }
}

// --- the screens -----------------------------------------------------------

#[test]
fn vault_empty() {
    shoot_all("01-vavlt-empty", |_state| {});
}

#[test]
fn vault_after_a_run() {
    shoot_all("02-vavlt-after-run", |state| {
        run_to_completion(state, 11);
        state.deliver(Message::RunFinished { cancelled: false });
        state.go(Screen::Root);
    });
}

#[test]
fn plan_move() {
    shoot_all("03-plan-move", |state| {
        with_selection(state);
        state.go(Screen::Plan);
    });
}

/// The one that has to be checked by eye: Deep Move turns the hero figure and
/// the commit button red, the selection slides to the right-hand tier, and the
/// delete grant is on — so the only red switch in the app is on screen too.
#[test]
fn plan_deep_move() {
    shoot_all("04-plan-deep", |state| {
        with_selection(state);
        state.set_tier(Tier::DeepMove);
        state.set_grant_delete(true);
        state.go(Screen::Plan);
    });
}

#[test]
fn working_mid_run() {
    shoot_all("05-working", |state| {
        run_to_completion(state, 5);
        state.go(Screen::Working);
    });
}

#[test]
fn outcome() {
    shoot_all("06-outcome", |state| {
        run_to_completion(state, 11);
        state.deliver(Message::RunFinished { cancelled: false });
    });
}

/// Opened on the file that failed, because that is the state the screen exists
/// for — "proven" is easy to draw and "failed, and here is why" is the claim.
#[test]
fn photo_proof() {
    shoot_all("07-photo", |state| {
        run_to_completion(state, 11);
        state.deliver(Message::RunFinished { cancelled: false });
        state.open_photo(5);
    });
}

#[test]
fn activity_empty() {
    shoot_all("08-activity-empty", |state| {
        state.set_tab(Tab::Activity);
        state.audit_rows.set(Vec::new());
    });
}

/// The log the app wrote for itself while the fixtures ran, rather than three
/// entries somebody typed — so the hash chain in the screenshot is a real one.
#[test]
fn activity_with_entries() {
    shoot_all("09-activity", |state| {
        run_to_completion(state, 11);
        state.deliver(Message::RunFinished { cancelled: false });
        state.go(Screen::Root);
        state.set_tab(Tab::Activity);
    });
}

#[test]
fn settings() {
    shoot_all("10-settings", |state| {
        with_selection(state);
        state.go(Screen::Root);
        state.set_tab(Tab::Settings);
    });
}
