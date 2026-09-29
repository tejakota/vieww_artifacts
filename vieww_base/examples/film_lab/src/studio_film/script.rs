//! The studio session — this film's script for the **actual**
//! `viewwstudio`, one `Vec<(f32, Action)>` in absolute film seconds.
//!
//! The times are the film's, not the product film's: this film is
//! 136 s and its studio act runs 56–98 s. The action vocabulary and the
//! applier are the product film's — [`Action`] and `pf::apply_list` —
//! so both films drive the app through one door, and the pointer-pair
//! atomicity rule (a `PointerDown` waits for its `PointerUp`, and both
//! land in one frame) holds here too, by construction rather than by
//! discipline.

use crate::product_film as pf;
use crate::product_film::script::{apply_list, Action, TAB_LIVE};
use vieww_foundation::Offset;
use viewwstudio::command::Command;
use viewwstudio::state::{Platform, Studio, View};

/// Where the studio session begins on this film's clock.
pub const STUDIO_OPEN: f32 = 126.0;

/// The counter's **Add one** button, hit-tested against this film's own
/// session state (panel open, counter.say rendered, iOS 393×852 at the
/// pane's 66% zoom).
pub const TAP_ADD_ONE: Offset = Offset::new(1593.0, 254.0);

/// The session — every action, in order, at absolute film seconds.
///
/// **The studio act is one scene long now.** In the v3 cut four scenes
/// rode the live application and the film annotated it; the review asked
/// for the product on the first frame and animation for the explanation,
/// so Z11 is the only scene the app renders in. The session is what that
/// one scene does — sixteen seconds of real work, no more scripted than
/// it has to be: a file opens, the panel breathes, live preview is
/// accepted, the damage overlay goes on, and two real edits land while it
/// is on.
///
/// Everything after Z11 — the compile pipeline, the fleet, the tokens —
/// is the film's own drawing, and says so.
pub fn session() -> Vec<(f32, Action)> {
    let t0 = STUDIO_OPEN;
    vec![
        (t0 + 0.4, Action::OpenPath("counter.say".into())),
        (t0 + 3.6, Action::PanelOpen(false)),
        (t0 + 6.2, Action::PanelOpen(true)),
        (t0 + 7.4, Action::ActiveTab(TAB_LIVE)),
        (t0 + 8.0, Action::Run(Command::LivePreview)),
        (t0 + 9.4, Action::AcceptLive),
        (t0 + 10.6, Action::ShowDamage(true)),
        (t0 + 11.8, Action::SetLiveTitle("My own inbox".into())),
        (t0 + 13.6, Action::SetLiveRow("Shipped the beta today".into())),
        (t0 + 15.2, Action::ShowDamage(false)),
    ]
}

/// Apply the session up to `abs`, resuming from `cursor`.
///
/// A thin wrapper over the product film's `apply_list` — one door, one
/// set of rules for driving the real app.
pub fn apply_session_up_to(
    driver: &mut vieww_render::FrameDriver,
    studio: &Studio,
    abs: f32,
    cursor: &mut usize,
) {
    apply_list(driver, studio, &session(), abs, cursor);
}
