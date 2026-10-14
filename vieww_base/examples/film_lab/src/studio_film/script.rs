//! The studio session — this film's script for the **actual**
//! `viewwstudio`, one list of `(film seconds, action)`.
//!
//! The studio act is seven scenes long and every one of them rides the
//! real application: Z11 shows it whole, Z12–Z17 quote it — as plates of
//! its own draw list, magnified, taken apart, or captured in a state the
//! scene needs (a platform, a device, the dark theme). The session is what
//! the application actually does across those scenes.
//!
//! The vocabulary is the product film's [`Action`] plus a few of this
//! film's own ([`Sf`]): an edit that puts the caret where the edit
//! happened (so the editor does not scroll away from it), the preview's
//! dark switch, and the Devices tab's presets.

use crate::product_film as pf;
use crate::product_film::script::{apply_list, Action, TAB_LIVE};
use vieww_foundation::{TextEditingValue, TextSelection};
use viewwstudio::command::Command;
use viewwstudio::state::{Device, Platform, RightTab, Studio};

/// One touch of the real studio.
#[derive(Clone)]
pub(crate) enum Sf {
    /// The product film's vocabulary.
    Pf(Action),
    /// Replace `from` with `to` in live.rs, caret left after the edit.
    EditLive(&'static str, &'static str),
    /// Make the named buffer the active tab.
    Tab(&'static str),
    /// The preview's Dark switch.
    PreviewDark(bool),
    /// A Devices-tab preset by index into `Device::ALL`, or the platform
    /// default.
    Device(Option<usize>),
    /// What the command palette's input holds — the real filter runs on
    /// it (`>` is the command registry).
    PaletteQuery(&'static str),
    /// Close the command palette.
    PaletteClose,
    /// The studio's semantics overlay — what a screen reader is told.
    Semantics(bool),
}

/// The start of scene `id` on the film's clock.
fn at(id: &str) -> f32 {
    let list = super::scenes();
    let i = list.iter().position(|s| s.id == id).expect("scene id");
    super::scene_start(i)
}

/// The session — every action, in order, at absolute film seconds.
pub(crate) fn session() -> Vec<(f32, Sf)> {
    let z11 = at("Z11");
    let z13 = at("Z13");
    let z14 = at("Z14");
    let z15 = at("Z15");
    let z16 = at("Z16");
    let z17 = at("Z17");
    let z12b = at("Z12B");
    let z13b = at("Z13B");
    let mut list = vec![
        // Z11 — the studio opens: a file, the live preview's own dialog,
        // and the sketch running on a simulated phone.
        (z11 + 1.6, Sf::Pf(Action::ActiveTab(TAB_LIVE))),
        (z11 + 3.6, Sf::Pf(Action::Run(Command::LivePreview))),
        (z11 + 6.4, Sf::Pf(Action::AcceptLive)),
        // Z13 — two real edits with the studio's damage overlay on.
        (z13 + 1.2, Sf::Pf(Action::ShowDamage(true))),
        (
            z13 + 3.4,
            Sf::EditLive("title \"Inbox\"", "title \"My own inbox\""),
        ),
        (
            z13 + 7.6,
            Sf::EditLive(
                "Draft for v0.3 is ready to review",
                "Shipped the beta today",
            ),
        ),
        (z13 + 13.4, Sf::Pf(Action::ShowDamage(false))),
        // Z14 — the say program opens in the real editor.
        (z14 + 0.3, Sf::Pf(Action::OpenPath("counter.say".into()))),
        // Z15 — back to the sketch (before the cut, so Z15's snapshots
        // see it); the platforms are snapshots.
        (z15 - 0.4, Sf::Tab("live.rs")),
        // Z16 — the Devices tab, walked through its presets.
        (z16 + 0.6, Sf::Pf(Action::RightTab(RightTab::Devices))),
        (z16 + 2.4, Sf::Device(Some(0))),
        (z16 + 5.0, Sf::Device(Some(3))),
        (z16 + 7.6, Sf::Device(Some(6))),
        (z16 + 10.2, Sf::Device(Some(4))),
        // Back to the preview, platform default — **after** Z16 has
        // left the screen. Z16's list plate is live, so a switch made
        // before the cut landed in its final frame: the Devices list
        // turned into the preview pane (a phone in the left box) and
        // that frame then ghosted through Z17's dissolve. Z17's
        // snapshots do not need the switch early — each snapshot sets
        // the Preview tab and the default device itself.
        (z17 + 0.05, Sf::Device(None)),
        (z17 + 0.05, Sf::Pf(Action::RightTab(RightTab::Preview))),
        // Z12B — make it yours: the real command palette, the real filter
        // typed a key at a time, then the accent walked round the wheel —
        // the whole studio re-themed live — and home to the brand's.
        (z12b + 0.6, Sf::Pf(Action::Run(Command::CommandPalette))),
        (z12b + 1.0, Sf::PaletteQuery(">")),
        (z12b + 2.6, Sf::PaletteQuery(">t")),
        (z12b + 2.75, Sf::PaletteQuery(">th")),
        (z12b + 2.9, Sf::PaletteQuery(">the")),
        (z12b + 3.05, Sf::PaletteQuery(">them")),
        (z12b + 3.2, Sf::PaletteQuery(">theme")),
        (z12b + 4.4, Sf::PaletteClose),
        (z12b + 4.6, Sf::Pf(Action::Accent("Teal".into()))),
        (z12b + 5.9, Sf::Pf(Action::Accent("Amber".into()))),
        (z12b + 7.2, Sf::Pf(Action::Accent("Rose".into()))),
        (z12b + 8.5, Sf::Pf(Action::Accent("Green".into()))),
        (z12b + 9.8, Sf::Pf(Action::Accent("Purple".into()))),
        // Z13B — the inspector: the previewed screen's render tree, then
        // the semantics overlay over the whole window; back to the
        // preview before the say program opens.
        (z13b + 0.4, Sf::Pf(Action::RightTab(RightTab::Inspector))),
        (z13b + 5.6, Sf::Semantics(true)),
        (z13b + 11.3, Sf::Semantics(false)),
        (z13b + 11.3, Sf::Pf(Action::RightTab(RightTab::Preview))),
    ];
    // One clock: the scenes' own order decides the session's.
    list.sort_by(|a, b| a.0.total_cmp(&b.0));
    list
}

/// The Devices presets Z16 steps through, in order, with when.
pub(crate) fn z16_devices() -> [(f32, Option<usize>); 5] {
    [
        (0.0, None),
        (2.4, Some(0)),
        (5.0, Some(3)),
        (7.6, Some(6)),
        (10.2, Some(4)),
    ]
}

/// Apply one action to the real studio.
pub(crate) fn apply_one(
    driver: &mut vieww_render::FrameDriver,
    studio: &Studio,
    action: &Sf,
    abs: f32,
) {
    match action {
        Sf::Pf(a) => {
            let mut c = 0usize;
            apply_list(driver, studio, &[(abs, a.clone())], abs, &mut c);
        }
        Sf::EditLive(from, to) => {
            let buffers = studio.buffers.get();
            let Some(i) = buffers.iter().position(|b| b.name == "live.rs") else {
                return;
            };
            studio.active_buffer.set(i);
            let Some(buffer) = studio.active() else {
                return;
            };
            let text = buffer.value.text.clone();
            let Some(pos) = text.find(from) else { return };
            let new_text = format!("{}{}{}", &text[..pos], to, &text[pos + from.len()..]);
            let caret = new_text[..pos + to.len()].chars().count();
            studio.edit(TextEditingValue {
                text: new_text,
                selection: TextSelection::collapsed(caret),
                ..Default::default()
            });
        }
        Sf::Tab(name) => {
            let buffers = studio.buffers.get();
            if let Some(i) = buffers.iter().position(|b| b.name == *name) {
                studio.active_buffer.set(i);
            }
        }
        Sf::PreviewDark(on) => studio.preview_dark.set(*on),
        Sf::Device(Some(i)) => studio.choose_device(Device::ALL[*i]),
        Sf::Device(None) => studio.clear_device(),
        Sf::PaletteQuery(q) => {
            studio.palette_query.set((*q).to_string());
            studio.palette_index.set(0);
        }
        Sf::PaletteClose => studio.palette_open.set(false),
        Sf::Semantics(on) => studio.show_semantics.set(*on),
    }
}

/// Apply the session up to `abs`, resuming from `cursor`.
pub(crate) fn apply_session_up_to(
    driver: &mut vieww_render::FrameDriver,
    studio: &Studio,
    abs: f32,
    cursor: &mut usize,
) {
    let list = session();
    while *cursor < list.len() && list[*cursor].0 <= abs {
        apply_one(driver, studio, &list[*cursor].1, list[*cursor].0);
        *cursor += 1;
    }
}

/// A named snapshot of the studio: the actions that put it in the state
/// the snapshot quotes, and the actions that put the live session back.
pub(crate) struct Snap {
    pub key: &'static str,
    pub apply: Vec<Sf>,
    pub restore: Vec<Sf>,
}

/// The snapshots a studio scene quotes, taken once when it is entered.
pub(crate) fn snapshots(id: &str) -> Vec<Snap> {
    let plat = |key: &'static str, p: Platform, dark: bool| Snap {
        key,
        apply: vec![
            Sf::Device(None),
            Sf::Pf(Action::Platform(p)),
            Sf::PreviewDark(dark),
        ],
        restore: vec![
            Sf::Pf(Action::Platform(Platform::Ios)),
            Sf::PreviewDark(false),
        ],
    };
    match id {
        "Z15" | "Z17" => vec![
            plat("ios", Platform::Ios, false),
            plat("android", Platform::Android, false),
            plat("desktop", Platform::Desktop, false),
            plat("ios_dark", Platform::Ios, true),
            plat("android_dark", Platform::Android, true),
            plat("desktop_dark", Platform::Desktop, true),
        ],
        "Z16" => z16_devices()
            .iter()
            .map(|(_, d)| Snap {
                key: device_key(*d),
                apply: vec![Sf::Device(*d)],
                restore: vec![Sf::Device(None)],
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The snapshot key for a Devices preset.
pub(crate) fn device_key(d: Option<usize>) -> &'static str {
    match d {
        None => "dev_default",
        Some(0) => "dev_0",
        Some(1) => "dev_1",
        Some(2) => "dev_2",
        Some(3) => "dev_3",
        Some(4) => "dev_4",
        Some(5) => "dev_5",
        Some(6) => "dev_6",
        _ => "dev_7",
    }
}

/// Silence the lint for the product-film re-export this module keeps in
/// scope for the scenes.
#[allow(unused)]
fn _reserved() {
    let _ = pf::W;
}
