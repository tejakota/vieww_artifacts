//! Desktop entry point.
//!
//! ```console
//! cargo run --release
//! ```
//!
//! **Run it in release.** A debug build of the native rasteriser is roughly an
//! order of magnitude slower, and a frame-rate impression from one is an
//! impression of `rustc -O0` rather than of this app. vieww's own README says
//! the same thing about its examples, for the same reason.
//!
//! The Android entry point (`android_main`) lives in [`upxcale::entry`] rather
//! than here, because cargo-apk builds the crate's `cdylib` target and the
//! `#[no_mangle]` symbol must be exported from the library. See
//! `src/entry.rs` for the shared boot logic.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use upxcale::entry::{boot, Pump};
use upxcale::SURFACE;

use vieww_foundation::task::FrameWaker;
use vieww_platform_winit::App;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The frame hook has to be installed on the `App` *builder*, but the state
    // it pumps cannot exist until `run` hands over a `FrameDriver` to take a
    // `Runtime` from. This cell is the join: the hook reads it every frame and
    // does nothing until `boot` has filled it in, which happens on the first.
    let pump: Rc<RefCell<Option<Pump>>> = Rc::new(RefCell::new(None));
    let hook_pump = Rc::clone(&pump);

    let app = App::new()
        .title("upxcale")
        .size(SURFACE)
        .background(upxcale::theme::scheme().surface)
        .before_frame(move |_driver| {
            if let Some(pump) = hook_pump.borrow_mut().as_mut() {
                pump.tick();
            }
        });

    // Taken before `run` consumes the app. This is the handle the render worker
    // uses to say "there is a new frame's worth of progress"; without it a batch
    // that finishes while the screen is idle leaves the bar frozen at whatever
    // it last painted.
    let waker: Arc<dyn FrameWaker> = Arc::new(app.waker());

    let report = app.run(move |driver| {
        let state = boot(driver, waker);
        *pump.borrow_mut() = Some(Pump::new(state));
    })?;

    println!("{report}");
    Ok(())
}
