//! Deferring the UI thread's non-drawing chores to frames that have slack.
//!
//! # Why this exists (advisory Fix 14, 2026-10-19)
//!
//! The advisory asked for a "frame-budget guard": when a frame is already
//! expensive, push work that does not have to happen in it to a later one.
//! The type is easy; what was missing (PENDING §4.3) was a census of what
//! the UI thread does in a frame that is *not* the frame. In this crate it
//! is:
//!
//! | chore | when | cost | must it be in this frame? |
//! |---|---|---|---|
//! | pipeline-cache save (`save_pipeline_cache`) | after the first GPU frame | a driver blob fetch + a file write | no — but before the app is killed. **Guarded.** |
//! | renderer-choice calibration write | once per build per device, when a measurement ends | one sub-kilobyte write | no — left in place: it lives inside `Measuring::observe`, runs once in the app's lifetime, and moving it buys nothing measurable |
//! | planner shadow log | every frame, opt-in diagnostic only | a write + flush | it describes *this* frame; left in place |
//!
//! Image decode is not on the list: the framework never decodes inside a
//! frame (`vieww_image::codec::decode_async` is the app-facing half of
//! that). Glyph rasterisation is not either: it produces this frame's pixels
//! and deferring it would draw a placeholder, which the fidelity policy does
//! not allow.
//!
//! # The rule
//!
//! Queued work runs after a frame whose work fit in [`SLACK`] of the budget,
//! one item per frame, oldest first — so a chore never turns a fast frame
//! into a slow one. **It is never starved:** an item that has waited
//! [`MAX_WAIT`] runs after the next frame regardless. That bound is the
//! point on a phone like the LG Stylo 5, where *no* frame fits its budget
//! (Device Farm f91d07c: 240/240 over) and a pure "wait for slack" rule
//! would mean the pipeline cache is never written — a phone app is killed,
//! not dropped, so a chore postponed forever is a chore lost.
//!
//! Pixels cannot move: nothing on the list draws.

use std::collections::VecDeque;
use std::time::Duration;

/// A frame "has slack" when its work used at most this share of the budget.
pub(crate) const SLACK: f64 = 0.5;

/// The longest any chore waits for a frame with slack before it runs anyway.
pub(crate) const MAX_WAIT: Duration = Duration::from_millis(500);

/// The UI thread's deferred chores. Generic over what a chore *is* — the
/// caller's own enum — because a chore needs the caller's state (the
/// renderer, the window) to run, which a stored closure could not borrow.
/// The guard decides *when*; the caller runs what it is handed.
#[derive(Debug)]
pub(crate) struct FrameBudgetGuard<T> {
    queue: VecDeque<(T, Duration)>,
    ran_on_slack: u64,
    ran_on_deadline: u64,
}

impl<T> Default for FrameBudgetGuard<T> {
    fn default() -> Self {
        Self {
            queue: VecDeque::new(),
            ran_on_slack: 0,
            ran_on_deadline: 0,
        }
    }
}

impl<T> FrameBudgetGuard<T> {
    /// Queue `chore`; `now` is the app clock.
    pub(crate) fn defer(&mut self, chore: T, now: Duration) {
        self.queue.push_back((chore, now));
    }

    /// Called once after each presented frame with the work it took and its
    /// budget. Hands back at most one chore for the caller to run now.
    pub(crate) fn after_frame(
        &mut self,
        now: Duration,
        work: Duration,
        budget: Duration,
    ) -> Option<T> {
        let (_, queued_at) = self.queue.front()?;
        let has_slack = work.as_secs_f64() <= budget.as_secs_f64() * SLACK;
        let overdue = now.saturating_sub(*queued_at) >= MAX_WAIT;
        if !(has_slack || overdue) {
            return None;
        }
        if has_slack {
            self.ran_on_slack += 1;
        } else {
            self.ran_on_deadline += 1;
        }
        self.queue.pop_front().map(|(chore, _)| chore)
    }

    /// Everything still queued, oldest first: for suspend and shutdown,
    /// where there is no next frame to wait for.
    pub(crate) fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        self.queue.drain(..).map(|(chore, _)| chore)
    }

    #[cfg(test)]
    pub(crate) fn pending(&self) -> usize {
        self.queue.len()
    }

    /// `(ran in a frame with slack, ran at the deadline)`.
    #[cfg(test)]
    pub(crate) const fn counts(&self) -> (u64, u64) {
        (self.ran_on_slack, self.ran_on_deadline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    const BUDGET: Duration = Duration::from_micros(16_667);

    #[test]
    fn a_chore_waits_for_a_frame_with_slack() {
        let mut guard = FrameBudgetGuard::default();
        guard.defer("save", ms(0));
        assert_eq!(
            guard.after_frame(ms(16), ms(14), BUDGET),
            None,
            "a heavy frame runs nothing"
        );
        assert_eq!(guard.after_frame(ms(33), ms(5), BUDGET), Some("save"));
        assert_eq!(guard.counts(), (1, 0));
    }

    #[test]
    fn one_chore_per_frame_oldest_first() {
        let mut guard = FrameBudgetGuard::default();
        for name in ["a", "b", "c"] {
            guard.defer(name, ms(0));
        }
        let ran: Vec<_> = (1..=3)
            .filter_map(|t| guard.after_frame(ms(t * 16), ms(2), BUDGET))
            .collect();
        assert_eq!(ran, vec!["a", "b", "c"]);
        assert_eq!(guard.pending(), 0);
    }

    /// The LG Stylo 5 shape: every frame over budget. The chore still runs,
    /// at the deadline, rather than never.
    #[test]
    fn a_device_with_no_slack_still_runs_its_chores_at_the_deadline() {
        let mut guard = FrameBudgetGuard::default();
        guard.defer("pipeline cache", ms(100));
        let mut now = ms(100);
        loop {
            now += ms(40);
            assert!(
                now <= ms(100) + MAX_WAIT + ms(40),
                "waited past the deadline"
            );
            if guard.after_frame(now, ms(40), BUDGET).is_some() {
                break;
            }
        }
        assert_eq!(guard.counts(), (0, 1));
    }

    #[test]
    fn drain_hands_back_everything_in_order() {
        let mut guard = FrameBudgetGuard::default();
        for n in 0..3 {
            guard.defer(n, ms(0));
        }
        assert_eq!(guard.drain().collect::<Vec<_>>(), vec![0, 1, 2]);
        assert_eq!(guard.pending(), 0);
    }

    #[test]
    fn an_empty_guard_hands_back_nothing() {
        let mut guard: FrameBudgetGuard<()> = FrameBudgetGuard::default();
        assert_eq!(guard.after_frame(ms(16), ms(1), BUDGET), None);
    }
}
