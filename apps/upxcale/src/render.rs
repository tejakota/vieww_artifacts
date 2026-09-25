//! Running the upscaler without stopping the frame.
//!
//! A 300x300 source at 4x is a 1200x1200 Lanczos resample and then an unsharp
//! pass over 1.44 million pixels. That is tens of milliseconds on a desktop and
//! rather more on a phone — many times the 16.67ms budget — so doing it inside
//! `build` would freeze the very overlay that exists to say it is working.
//!
//! # vieww owns no executor, and neither does this
//!
//! `vieww_foundation::task::Spawn` is `Box<dyn FnOnce() + Send>` and nothing
//! more, so an application that already runs tokio or rayon is never made to
//! start a second runtime. This module takes a `&dyn Spawn` and a
//! `FrameWaker` and has no opinion about which. `main.rs` passes
//! [`Threads`](vieww_foundation::task::Threads); a test passes `Inline` and the
//! whole thing runs synchronously with no other change.
//!
//! # Why a channel and not a `Task`
//!
//! `Task<T, E>` resolves once, which would mean one progress step for a batch
//! of eight photographs. The overlay is a *determinate* bar — the prototype was
//! specific about that — and a determinate bar needs a number that moves. So
//! the worker reports each stage as it finishes and the tree drains those
//! reports once per frame. The progress shown is therefore work actually
//! completed, never a timer pretending to be one.

use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

use vieww_foundation::task::{FrameWaker, Spawn};
use vieww_foundation::Image;

use crate::photos::UPSCALE_FACTOR;
use crate::upscale;

/// What the worker sends back as it goes.
#[derive(Debug)]
pub enum Progress {
    /// A stage of one photograph finished. `index` is its position in the
    /// batch, `stage` how many of that photo's stages are done.
    Stage { index: usize, stage: u8 },
    /// A photograph is finished, with its pixels.
    Done { id: String, image: Image },
    /// A photograph could not be rendered. The batch continues.
    Failed { id: String, error: String },
}

/// How many reportable stages one photograph passes through.
///
/// Two: the resample, then the sharpen. Naming it here keeps the worker and the
/// bar's denominator from drifting apart.
pub const STAGES_PER_PHOTO: u8 = 2;

/// A batch of photographs being upscaled on some other thread.
///
/// Held by [`AppState`](crate::state::AppState) for as long as the progress
/// route is up, and dropped when it is dismissed — dropping the receiver is
/// also how a cancelled render stops mattering: the worker finishes the photo
/// it is on, discovers nobody is listening, and exits.
#[derive(Debug)]
pub struct RenderJob {
    receiver: Receiver<Progress>,
    /// Number of photographs in this batch.
    pub total: usize,
}

impl RenderJob {
    /// Spawn the batch.
    ///
    /// `work` is `(id, source pixels)` pairs. The pixels are passed rather than
    /// the [`Photo`](crate::photos::Photo) so the worker never touches the
    /// asset bundle or the `RefCell`s in
    /// [`Library`](crate::photos::Library) — neither is `Send`, and keeping the
    /// thread's inputs to plain data is what makes that a compile-time fact
    /// rather than a rule to remember.
    #[must_use]
    pub fn start(
        spawner: &dyn Spawn,
        waker: Arc<dyn FrameWaker>,
        work: Vec<(String, Image)>,
    ) -> Self {
        let total = work.len();
        let (sender, receiver) = mpsc::channel();

        spawner.spawn(Box::new(move || {
            for (index, (id, source)) in work.into_iter().enumerate() {
                let enlarged = upscale::resample(
                    &source,
                    source.width().saturating_mul(UPSCALE_FACTOR),
                    source.height().saturating_mul(UPSCALE_FACTOR),
                );
                // `send` fails only when the receiver is gone, which means the
                // route was dismissed. Stop rather than finish work nobody
                // asked for any more.
                if sender.send(Progress::Stage { index, stage: 1 }).is_err() {
                    return;
                }
                waker.wake();

                let sharpened = upscale::sharpen_for_factor(&enlarged, UPSCALE_FACTOR);
                if sender.send(Progress::Stage { index, stage: 2 }).is_err() {
                    return;
                }
                if sender
                    .send(Progress::Done {
                        id,
                        image: sharpened,
                    })
                    .is_err()
                {
                    return;
                }
                waker.wake();
            }
        }));

        Self { receiver, total }
    }

    /// Take everything the worker has reported since the last frame.
    ///
    /// Non-blocking by construction: this runs in the frame loop, and a frame
    /// that waits on a worker is a frame that was dropped.
    #[must_use]
    pub fn drain(&self) -> Vec<Progress> {
        // `try_recv` returns `Disconnected` once the worker has finished and
        // dropped its sender — but only *after* the queue has been drained, so
        // stopping on either error is correct: both mean "nothing more right
        // now", and the caller learns the batch is over from the completion
        // count rather than from this.
        let mut batch = Vec::new();
        while let Ok(message) = self.receiver.try_recv() {
            batch.push(message);
        }
        batch
    }

    /// Total reportable stages in this batch — the progress bar's denominator.
    #[must_use]
    pub const fn stages(&self) -> usize {
        self.total * STAGES_PER_PHOTO as usize
    }
}
