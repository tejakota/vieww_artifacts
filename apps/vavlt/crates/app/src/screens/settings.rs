//! Tab 2 — settings, and the disclosures.
//!
//! This is where the app's own limits live. They were on the home screen
//! before, which made the first thing a new user saw a list of things the app
//! cannot do. Someone who wants to know comes here; the home screen keeps only
//! the caveat that changes how its own figure should be read.
//!
//! **The attribution section is no longer a licence obligation.** The Slint
//! build shipped under the royalty-free licence on condition it disclosed its
//! use of Slint, and SPEC §2 closed that decision on exactly that basis. vieww
//! is Apache-2.0, which asks for a notice rather than a visible credit — so the
//! line below is a credit this app chooses to give, and the licence blocker
//! SPEC §2 spent a paragraph resolving is simply gone.

use std::rc::Rc;

use vieww::element::ScrollController;
use vieww::prelude::*;

use crate::state::VavltState;
use crate::theme::VavltTheme;
use crate::ui;

pub fn settings(
    theme: &VavltTheme,
    state: &Rc<VavltState>,
    scroll: &ScrollController,
) -> WidgetNode {
    let m = theme.metrics;
    let mut body = children![];

    // --- appearance -------------------------------------------------------
    body.push(section(theme, "Appearance", {
        let toggle = state.clone();
        children![ui::switch_row(
            theme,
            "Dark",
            "Adwaita's own dark palette. Both are the real named colours, not a filter.",
            state.dark.get(),
            false,
            move |_| toggle.toggle_theme(),
        )]
    }));

    // --- what it cannot do ------------------------------------------------
    body.push(section(
        theme,
        "What this app cannot do",
        rows(
            theme,
            &[
                (
                    "No media permission",
                    "READ_MEDIA_IMAGES and its equivalents are not declared on any platform. \
                     Files arrive only from the system picker.",
                ),
                (
                    "No network permission",
                    "INTERNET is not declared, so no build of this app can send anything \
                     anywhere.",
                ),
                (
                    "No path access",
                    "The engine takes host-issued handles, never paths it chose. It cannot \
                     enumerate your storage.",
                ),
                (
                    "No model writes bytes",
                    "A model may pick a starting quality. Everything stored is produced by a \
                     deterministic codec.",
                ),
            ],
        ),
    ));

    // --- not built yet ----------------------------------------------------
    body.push(section(
        theme,
        "Not built yet",
        rows(
            theme,
            &[
                (
                    "The vavlt store",
                    "Nothing is written, moved or deleted. Runs encode, verify and report — \
                     that is all this build does.",
                ),
                (
                    "The perceptual gate",
                    "No SSIMULACRA2 or VMAF on device, so Deep Move falls back to lossless and \
                     says so on every photo it affects.",
                ),
                (
                    "Video",
                    "No hardware encoder is wired up, so video is counted and left alone.",
                ),
            ],
        ),
    ));

    // --- about ------------------------------------------------------------
    body.push(section(
        theme,
        "About",
        children![
            ui::row(
                theme,
                ui::Row::new("vavlt")
                    .subtitle("Local-first photo vault. Rust engine, no network.")
                    .trailing(env!("CARGO_PKG_VERSION")),
                None
            ),
            ui::divider(theme),
            ui::row(
                theme,
                ui::Row::new("Built with vieww").subtitle(
                    "One Rust UI, on Android, iOS and the desktop. Apache-2.0 — this credit \
                         is given, not required."
                ),
                None
            ),
            ui::divider(theme),
            ui::row(
                theme,
                ui::Row::new("Lepton · oxipng · zstd · BLAKE3").subtitle(
                    "The codecs and the hash every lossless claim in this app is proven with."
                ),
                None
            ),
        ],
    ));

    // The one action on this screen, and it ends a grant rather than giving
    // one — so it is flat and red-lettered rather than a filled button.
    let revoke = state.clone();
    if state.included_count() + state.excluded_count() > 0 {
        body.push(ui::gap(m.sp_1));
        body.push(ui::button(
            theme,
            "Give the files back",
            ui::ButtonKind::Flat,
            true,
            move || revoke.revoke_access(),
        ));
        body.push(ui::caption(
            theme,
            "Drops every handle the picker granted. Nothing is deleted; the app simply stops \
             being able to read them.",
        ));
    }

    ui::screen_body(theme, scroll, body)
}

/// A heading with its group under it. The heading is *not* inside the card:
/// Adwaita's boxed lists sit under their label, not in a box with it.
fn section(theme: &VavltTheme, title: &str, children: Vec<WidgetNode>) -> WidgetNode {
    let m = theme.metrics;
    Flex::column()
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
        .spacing(m.sp_1)
        .children(children![
            ui::group_heading(theme, title),
            ui::card(
                theme,
                Padding::new(EdgeInsets::symmetric(m.gutter, 0.0)).child(
                    Flex::column()
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .main_axis_size(MainAxisSize::Min)
                        .children(children)
                )
            ),
        ])
        .into()
}

/// Title/subtitle pairs, divided.
fn rows(theme: &VavltTheme, entries: &[(&str, &str)]) -> Vec<WidgetNode> {
    let mut out = Vec::with_capacity(entries.len() * 2);
    for (index, (title, subtitle)) in entries.iter().enumerate() {
        if index > 0 {
            out.push(ui::divider(theme));
        }
        out.push(ui::row(
            theme,
            ui::Row::new(*title).subtitle(*subtitle),
            None,
        ));
    }
    out
}
