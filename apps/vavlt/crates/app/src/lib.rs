//! The vavlt interface — every screen, in vieww, with no platform in it.
//!
//! This crate is the whole application above the operating system. It draws all
//! seven screens, holds all the state, and owns every rule about what the user
//! is shown and when. What it does *not* have is any idea where it is running:
//! see [`Platform`], which is four methods wide.
//!
//! # What this replaces
//!
//! Two Slint UIs — `crates/ui/ui/*.slint` for the desktop and
//! `crates/android/ui/*.slint` for the phone, about 3,200 lines between them,
//! with a shared colour file and nothing else. They had already diverged: the
//! desktop had a comparison viewer the phone did not, the phone had a consent
//! flow the desktop did not, and the two type scales were maintained by hand
//! against each other.
//!
//! There is now one tree. A phone, an iPhone and a desktop window run the same
//! widgets; the only thing that differs is [`FormFactor`](theme::FormFactor),
//! read from the surface width, which chooses between two sets of numbers.
//!
//! # Mounting it
//!
//! ```no_run
//! # use std::rc::Rc;
//! # use std::sync::Arc;
//! # fn demo(platform: Rc<dyn vavlt_app::Platform>, waker: Arc<dyn vieww::foundation::task::FrameWaker>) {
//! use vieww::prelude::*;
//! use vavlt_app::{Scrolls, VavltApp, VavltState};
//!
//! let mut driver = FrameDriver::new(vieww::foundation::Size::new(412.0, 915.0));
//! let runtime = driver.elements().runtime().clone();
//!
//! let state = VavltState::new(&runtime, platform, waker);
//! let scrolls = Rc::new(Scrolls::new(&runtime));
//! scrolls.attach(driver.tickers());
//!
//! driver.set_root(VavltApp::new(state, scrolls));
//! # }
//! ```
//!
//! The host then adds `state.pump()` as an `on_frame` hook, which is what turns
//! a worker thread's results into signal writes. `crates/desktop` is thirty
//! lines of exactly this and is the shortest thing to read next.

pub mod model;
pub mod platform;
pub mod root;
mod screens;
pub mod state;
pub mod theme;
pub mod ui;

pub use model::{AuditKind, AuditRow, Estimate, Photo, PhotoState, RunResult, Screen, Tab, Thumb};
pub use platform::Platform;
pub use root::{Scrolls, VavltApp};
pub use state::{Message, Pump, VavltState, MAX_PICK, RETENTION_DAYS};
pub use theme::{FormFactor, VavltColors, VavltMetrics, VavltTheme};
