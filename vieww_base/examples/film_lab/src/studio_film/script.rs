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
pub const STUDIO_OPEN: f32 = 56.0;

/// The counter's **Add one** button, hit-tested against this film's
/// own session state (panel open, counter.say rendered, iOS 393×852
/// at the pane's 66% zoom). The product film's constant was calibrated
/// against its session, whose layout at tap time differed; this film
/// re-derives its own from the rendered preview.
pub const TAP_ADD_ONE: Offset = Offset::new(1593.0, 254.0);

/// The session — every action, in order, at absolute film seconds.
///
/// Read it as the studio act's stage directions:
///
/// * **Z07 · studio_opens** (56–66) — the say file opens, the panel
///   breathes closed and open; the shell is shown, not narrated.
/// * **Z08 · live_compose** (66–77) — live preview accepted, damage
///   overlay on, two edits land while it is on: the audience sees
///   exactly what an edit costs.
/// * **Z09 · say_to_rust** (77–89) — the `.say` program types itself,
///   renders through real rustc (settled off-clock), and the counter
///   takes two real taps.
/// * **Z10 · ships_everywhere** (89–98) — the same tree re-frames
///   itself android → ios → desktop, then the token editor flips the
///   accent teal → purple, live.
pub fn session() -> Vec<(f32, Action)> {
    vec![
        // Z07 — the shell, first contact.
        (56.4, Action::OpenPath("counter.say".into())),
        (60.0, Action::PanelOpen(false)),
        (64.0, Action::PanelOpen(true)),
        // Z08 — edit a line, see the picture change.
        (66.6, Action::ActiveTab(TAB_LIVE)),
        (67.2, Action::Run(Command::LivePreview)),
        (69.2, Action::AcceptLive),
        (71.0, Action::ShowDamage(true)),
        (73.0, Action::SetLiveTitle("My own inbox".into())),
        (75.6, Action::SetLiveRow("Shipped the beta today".into())),
        (76.8, Action::ShowDamage(false)),
        // Z09 — .say → rust → cdylib → pixels.
        (77.4, Action::OpenPath("counter.say".into())),
        (78.0, Action::TypeSay(40)),
        (79.0, Action::TypeSay(96)),
        (80.0, Action::TypeSay(152)),
        (81.0, Action::TypeSay(208)),
        (82.0, Action::TypeSay(usize::MAX)),
        (83.2, Action::Render),
        (83.4, Action::Settle),
        (85.6, Action::PointerDown(TAP_ADD_ONE)),
        (85.72, Action::PointerUp(TAP_ADD_ONE)),
        (87.2, Action::PointerDown(TAP_ADD_ONE)),
        (87.32, Action::PointerUp(TAP_ADD_ONE)),
        // Z10 — one tree, every device; tokens, not forks.
        (89.6, Action::Platform(Platform::Android)),
        (91.2, Action::Platform(Platform::Ios)),
        (92.8, Action::Platform(Platform::Desktop)),
        (94.4, Action::View(View::Tokens)),
        (95.6, Action::Accent("Teal".into())),
        (96.8, Action::Accent("Purple".into())),
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
