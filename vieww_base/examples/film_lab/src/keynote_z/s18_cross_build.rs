//! S18 · CROSS BUILD — the toolchain, the export, the devices.
//! 2:38–2:49.
//!
//! The build story, in the product's own views: the Toolchain view (the
//! paths a person fills in before Android will build), then Export —
//! the build is fired — and the Devices tab, where the package lands.
//! The receipt the captions carry is the device suite's own: **59.3 fps
//! · 4.09 ms · a Redmi Note 7 Pro (2019)** — the repo's CI artifacts,
//! quoted with their source.
//!
//! Witness taps 7 (the build view, 161.3 s) and 8 (the devices land,
//! 166.8 s) fire here.

use super::{caption, clamp01, studio_chrome, tap_ring_at, Ctx, MINT, SYN_TYPE};
use vieww_widget::WidgetNode;

/// The script's moments, absolute film seconds.
const TOOLCHAIN_T: f32 = 159.0;
const EXPORT_T: f32 = 161.3;
const DEVICES_T: f32 = 166.8;

pub(crate) fn build(ctx: &Ctx) -> WidgetNode {
    let _t = ctx.t;
    let abs = ctx.abs;

    let mut stack = studio_chrome(ctx);

    // The device receipt — the CI artifacts, quoted: the chip row the
    // captions promise. Arrives with the devices tab.
    if abs >= DEVICES_T - 0.4 {
        let appear = clamp01((abs - DEVICES_T + 0.4) / 0.4);
        stack = stack.push(super::receipt_row(
            &[
                ("59.3 fps", MINT),
                ("4.09 ms", MINT),
                ("Redmi Note 7 Pro (2019)", SYN_TYPE),
            ],
            appear,
        ));
    }

    // The build's annotation — a tap ring at the Export view's moment
    // (the film's witness grammar, marking the build firing).
    if (EXPORT_T..EXPORT_T + 1.0).contains(&abs) {
        stack = stack.push(tap_ring_at(
            vieww_foundation::Offset::new(300.0, 300.0),
            (abs - EXPORT_T) * 0.9,
        ));
    }

    // The captions — the build's beats.
    stack = stack.push(caption(
        "the toolchain — the paths to fill in, once",
        1002.0,
        clamp01((abs - TOOLCHAIN_T) / 0.25),
    ));
    if abs >= EXPORT_T {
        stack = stack.push(caption(
            "export: the build is fired — apk or ipa, one click",
            966.0,
            clamp01((abs - EXPORT_T) / 0.15),
        ));
    }
    if abs >= DEVICES_T {
        stack = stack.push(caption(
            "and the devices tab — where the package lands. 59.3 fps on a 2019 phone",
            930.0,
            clamp01((abs - DEVICES_T) / 0.15),
        ));
    }

    stack.into()
}
