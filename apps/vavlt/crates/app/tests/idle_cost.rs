//! What a settled screen costs, per frame, with a hundred photographs on it.
//!
//! ```console
//! cargo test -p vavlt-app --test idle_cost --release -- --nocapture
//! ```
//!
//! # Why this test exists
//!
//! The app was reported as laggy on a phone, and every explanation offered for
//! it so far has been an argument rather than a number. There are only three
//! ways a UI framework spends a frame it did not need to: it rebuilds widgets
//! that did not change, it lays out render objects that did not move, or it
//! re-uploads pixels the GPU already has. This file measures the first two, and
//! `vieww`'s own `an_image_drawn_across_frames_keeps_one_blob` measures the
//! third.
//!
//! The fourth possibility is worse than all of them and is checked first: a
//! frame that *asks for another frame*. A signal written during build or layout
//! marks its reader pending after the build that would have read it, so the
//! driver never goes idle and the display runs flat out forever — on a desktop
//! that is a warm fan, and on a phone it is exactly the symptom reported.

use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use vavlt_app::model::{Screen, Thumb};
use vavlt_app::{Message, Platform, Scrolls, VavltApp, VavltState};
use vavlt_engine::{Item, Source};
use vieww::foundation::task::FrameWaker;
use vieww::foundation::{Offset, ScrollEvent, Size};
use vieww::prelude::*;

const PHONE: Size = Size {
    width: 412.0,
    height: 915.0,
};

/// A hundred, because that is the picker's cap and therefore the worst case a
/// user can actually reach.
const PHOTOS: usize = 100;

/// A platform that does nothing, so the measurement is of the interface.
#[derive(Debug)]
struct NoPlatform(std::path::PathBuf);

impl Source for NoPlatform {
    fn read(&self, _handle: &str) -> anyhow::Result<Vec<u8>> {
        anyhow::bail!("no files here")
    }
}

impl Platform for NoPlatform {
    fn data_dir(&self) -> std::path::PathBuf {
        self.0.clone()
    }
    fn source(&self) -> Arc<dyn Source> {
        Arc::new(NoPlatform(self.0.clone()))
    }
    fn pick(&self, _max: usize, _reply: Sender<Message>, _waker: Arc<dyn FrameWaker>) {}
    fn spawn(&self, work: Box<dyn FnOnce() + Send + 'static>) {
        work();
    }
}

/// A waker that does nothing: the frames here are driven by hand.
#[derive(Debug)]
struct Still;

impl FrameWaker for Still {
    fn wake(&self) {}
}

/// A settled plan screen with `PHOTOS` decoded thumbnails on it.
fn settled_plan() -> FrameDriver {
    let mut driver = FrameDriver::new(PHONE);
    let runtime = driver.elements().runtime().clone();

    let dir = std::env::temp_dir().join("vavlt-idle-cost");
    std::fs::create_dir_all(&dir).ok();
    let state = VavltState::new(&runtime, Rc::new(NoPlatform(dir)), Arc::new(Still));
    state.dark.set(true);

    // Through `deliver`, not by writing signals: the manifest hash, the
    // estimate and the navigation are all produced by shipping code, so what is
    // measured is the screen the app actually puts up.
    let items: Vec<Item> = (0..PHOTOS)
        .map(|index| {
            Item::new(
                format!("h{index}"),
                format!("IMG_{:04}.jpg", 1200 + index),
                4_000_000 + (index as u64) * 17_000,
            )
        })
        .collect();
    state.deliver(Message::Picked(items));

    // Real decoded pixels at a real thumbnail size, so the frame draws images
    // rather than placeholders — the images are the whole point.
    for index in 0..PHOTOS {
        const EDGE: u32 = 288;
        state.deliver(Message::Thumb {
            index,
            thumb: Thumb::new(vec![(index % 255) as u8; (EDGE * EDGE * 4) as usize], EDGE),
        });
    }
    state.go(Screen::Plan);

    let scrolls = Rc::new(Scrolls::new(&runtime));
    scrolls.attach(driver.tickers());
    driver.set_root(VavltApp::new(state, scrolls));

    // Time is advanced by hand rather than by looping: the screen arrives with
    // a 320ms animation on it, and six frames drawn as fast as the machine can
    // draw them all land inside it. A test that then reports "the tree never
    // settles" is measuring its own impatience.
    //
    // One second, in 16ms steps: past the arrival, past the `LayoutBuilder`'s
    // correcting rebuild, and past anything either of them started.
    for step in 0..60 {
        driver.draw_frame_at(Duration::from_millis(step * 16));
    }
    driver
}

/// Where the clock is left after [`settled_plan`].
const SETTLED_AT: Duration = Duration::from_millis(60 * 16);

/// One more frame, `step` frames after the tree settled.
fn frame(driver: &mut FrameDriver, step: u64) {
    driver.draw_frame_at(SETTLED_AT + Duration::from_millis(step * 16));
}

#[test]
fn a_settled_screen_does_not_ask_for_another_frame() {
    let mut driver = settled_plan();

    assert!(
        !driver.needs_frame(),
        "the plan screen never goes idle: something writes a signal from build \
         or layout, so every frame schedules the next one and the phone renders \
         flat out until the battery is gone"
    );

    // And it stays idle. A condition that clears on one extra frame and comes
    // back on the frame after is the spin wearing a disguise.
    for step in 1..=30 {
        frame(&mut driver, step);
        assert!(
            !driver.needs_frame(),
            "settled, then asked for a frame again"
        );
    }
}

#[test]
fn a_settled_frame_rebuilds_nothing_and_re_syncs_nothing() {
    let mut driver = settled_plan();

    driver.elements().mark_builds();
    frame(&mut driver, 1);

    assert_eq!(
        driver.elements().builds_since_mark(),
        0,
        "an idle frame rebuilt widgets. With {PHOTOS} photographs on screen \
         that is {PHOTOS} tiles' worth of `Photo::clone` and six `String` \
         allocations each, sixty times a second."
    );
    assert_eq!(
        driver.owner().visited_last_sync(),
        0,
        "an idle frame walked the element tree to sync the render tree. \
         `Element::subtree_revision` exists so that walk can stop at the top of \
         anything that did not change."
    );
}

#[test]
fn scrolling_the_wall_does_not_rebuild_the_wall() {
    let mut driver = settled_plan();

    driver.elements().mark_builds();
    // A fling's worth of scroll deltas, the way a finger delivers them.
    for step in 1..=20 {
        driver.handle_scroll(&ScrollEvent::new(
            Offset::new(206.0, 500.0),
            Offset::new(0.0, -18.0 - step as f32),
            SETTLED_AT + Duration::from_millis(16 * step),
        ));
        frame(&mut driver, step);
    }

    // Three per step: the `Scrolling` widget that reads the offset, the
    // `Scrollable` it builds, and the viewport underneath. Nothing below the
    // viewport, which is the whole point.
    //
    // **6,720 before this was fixed**, for the same twenty steps — a hundred
    // tiles, each cloning a `Photo` and allocating six `String`s, to draw them
    // exactly where they already were. Two changes closed it: the scroll offset
    // is now read inside `ui::Scrolling` rather than in the screen's own build
    // (`crates/app/src/ui/mod.rs`), and `vieww` republishes an inherited value
    // into a provision the element already holds rather than minting a new
    // scope for the whole subtree (`vieww_widget::Provision`).
    let rebuilds = driver.elements().builds_since_mark();
    assert!(
        rebuilds <= 20 * 4,
        "twenty scroll steps rebuilt {rebuilds} elements, against a budget of \
         four per step. A scroll moves a transform; it must not rebuild the \
         {PHOTOS} tiles underneath it."
    );
}
