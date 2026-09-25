//! What a navigation actually looks like part-way through.
//!
//! ```console
//! cargo test -p vavlt-app --test transition --release -- --nocapture
//! ```
//!
//! # Why this exists
//!
//! Every other test in this crate photographs a screen *at rest*. A transition
//! is the one place where being correct and looking correct are genuinely
//! different: a stack that ends on the right screen passes every assertion and
//! can still flash, jump or overlay on the way there. This drives the animation
//! to a series of fractions and photographs each one, so the middle of a
//! navigation is inspectable rather than a thing somebody reports afterwards.
//!
//! It was written for a specific report — "between switching the screen the
//! text overlaid for a fraction of a second" — and the frames it produces are
//! how that was turned from a description into a defect.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use vavlt_app::model::Screen;
use vavlt_app::{Message, Platform, Scrolls, VavltApp, VavltState};
use vavlt_engine::{Item, Source};
use vieww::foundation::task::FrameWaker;
use vieww::foundation::{Color, Size};
use vieww::paint::gpu::{GpuError, GpuRenderer};
use vieww::prelude::*;

const PHONE: Size = Size {
    width: 412.0,
    height: 915.0,
};

/// The navigation duration in `theme::motion::NAV`. Sampled either side of it
/// so the resting frames bracket the moving ones.
const SAMPLES: [u64; 6] = [0, 60, 120, 200, 300, 420];

fn renderer() -> Option<MutexGuard<'static, GpuRenderer>> {
    static GPU: OnceLock<Option<Mutex<GpuRenderer>>> = OnceLock::new();
    let slot = GPU.get_or_init(|| match GpuRenderer::headless() {
        Ok(r) => Some(Mutex::new(r)),
        Err(GpuError::NoAdapter) => {
            eprintln!("skipping: no graphics adapter");
            None
        }
        Err(e) => panic!("gpu: {e}"),
    });
    slot.as_ref().map(|g| g.lock().expect("gpu mutex"))
}

#[derive(Debug)]
struct NoPlatform(PathBuf);

impl Source for NoPlatform {
    fn read(&self, _h: &str) -> anyhow::Result<Vec<u8>> {
        anyhow::bail!("no files here")
    }
}

impl Platform for NoPlatform {
    fn data_dir(&self) -> PathBuf {
        self.0.clone()
    }
    fn source(&self) -> Arc<dyn Source> {
        Arc::new(NoPlatform(self.0.clone()))
    }
    fn pick(&self, _m: usize, _r: Sender<Message>, _w: Arc<dyn FrameWaker>) {}
    fn spawn(&self, work: Box<dyn FnOnce() + Send + 'static>) {
        work();
    }
}

#[derive(Debug)]
struct Still;
impl FrameWaker for Still {
    fn wake(&self) {}
}

fn items() -> Vec<Item> {
    (0..9)
        .map(|i| {
            Item::new(
                format!("h{i}"),
                format!("IMG_{:04}.jpg", 1200 + i),
                4_000_000,
            )
        })
        .collect()
}

/// Walk one navigation and write a PNG per sample.
///
/// Time is driven explicitly with `draw_frame_at` rather than by sleeping: a
/// wall-clock test of an animation is a test of how loaded the machine is.
fn walk(name: &str, from: Screen, to: Screen) {
    let Some(mut gpu) = renderer() else {
        return;
    };

    let dir = std::env::temp_dir().join(format!("vavlt-t-{name}"));
    std::fs::create_dir_all(&dir).ok();

    let mut driver = FrameDriver::new(PHONE);
    let runtime = driver.elements().runtime().clone();
    let state = VavltState::new(&runtime, Rc::new(NoPlatform(dir.clone())), Arc::new(Still));
    state.deliver(Message::Picked(items()));
    state.go(from);

    let scrolls = Rc::new(Scrolls::new(&runtime));
    scrolls.attach(driver.tickers());
    driver.set_root(VavltApp::new(state.clone(), scrolls));

    // Settle at the starting screen before the clock starts.
    for _ in 0..3 {
        driver.draw_frame_at(Duration::ZERO);
    }

    state.go(to);

    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/transitions");
    std::fs::create_dir_all(&out).ok();

    let mut runs: Vec<(u64, usize)> = Vec::new();
    for ms in SAMPLES {
        driver.draw_frame_at(Duration::from_millis(ms));
        let (pixels, report) = gpu
            .render_to_pixels(
                driver.scene(),
                PHONE.width as u32,
                PHONE.height as u32,
                Color::hex(0x24_2424),
            )
            .expect("render");

        let path = out.join(format!("{name}-{ms:03}ms.png"));
        image::save_buffer(
            &path,
            pixels.data(),
            pixels.width(),
            pixels.height(),
            image::ColorType::Rgba8,
        )
        .expect("png");
        println!(
            "{name} @{ms:>3}ms  {} shapes  {} glyph runs",
            report.shapes, report.glyph_runs
        );
        runs.push((ms, report.glyph_runs));
    }

    // **The assertion the PNGs were being read by eye for.**
    //
    // The reported defect was "the text overlaid for a fraction of a second",
    // and superimposed text has one unmistakable signature: part-way through
    // the navigation the frame carries roughly the sum of both screens' runs.
    // The last sample is the destination at rest, so no sample may exceed it by
    // any meaningful margin.
    let (_, settled) = *runs.last().expect("samples were taken");
    for &(ms, count) in &runs {
        assert!(
            count <= settled + 2,
            "{name} at {ms}ms drew {count} runs of text against {settled} on \
             the settled screen — two screens are in the tree at once and \
             their text is on top of each other"
        );
    }

    // And it does not go blank on the way, which is the failure a nervous fix
    // for the above produces — including on the very first frame, which is the
    // one an `AnimationController` draws at its own starting value. See
    // `root::ARRIVAL_FLOOR`.
    let most = runs.iter().map(|&(_, count)| count).max().unwrap_or(0);
    for &(ms, count) in &runs {
        assert!(
            count * 2 > most,
            "{name} at {ms}ms drew {count} runs of text against {most} at its \
             fullest — the arriving screen is invisible, which is a blank \
             flash rather than a transition"
        );
    }

    std::fs::remove_dir_all(&dir).ok();
}

/// Root → Plan. The one the report was about, and the one where the tab bar
/// leaves the layout.
#[test]
fn root_to_plan() {
    walk("01-root-to-plan", Screen::Root, Screen::Plan);
}

/// Plan → Working, with no chrome change either side. If this one is clean and
/// `root_to_plan` is not, the defect is the chrome rather than the slide.
#[test]
fn plan_to_working() {
    walk("02-plan-to-working", Screen::Plan, Screen::Working);
}

/// Back out, which is the same slide in the other direction.
#[test]
fn outcome_to_root() {
    walk("03-outcome-to-root", Screen::Outcome, Screen::Root);
}
