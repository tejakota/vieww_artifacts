//! layout — each scene's content box, in the scene's own coordinates.
//!
//! The master fits this box uniformly into [`super::frame::BODY`], so the
//! box decides how large a scene is drawn: a tight box is a big picture.
//! The numbers are the ink the scene actually lays down over its whole
//! length (`film_lab sfmeasure` prints them), padded by a hair so glows
//! and shadows breathe instead of touching the padding.

use vieww_foundation::Rect;

/// The whole application, for the scenes that quote it.
pub const APP: Rect = Rect { left: 0.0, top: 0.0, right: 1920.0, bottom: 1080.0 };

/// The largest a fit may magnify — past this, a small scene reads as a
/// close-up rather than as a composition.
pub const MAX_SCALE: f32 = 1.45;

pub fn content_box(id: &str) -> Rect {
    match id {
        "Z11" | "Z12" | "Z13" | "Z14" | "Z15" | "Z16" | "Z17" | "Z20" => APP,
        "Z21" | "Z22" => r(300.0, 240.0, 1620.0, 950.0),
        "Z18" => r(190.0, 290.0, 1730.0, 950.0),
        "Z19" => r(80.0, 270.0, 1870.0, 850.0),
        "Z01" => r(130.0, 268.0, 1790.0, 950.0),
        "Z02" => r(80.0, 305.0, 1790.0, 850.0),
        "Z03" => r(70.0, 290.0, 1900.0, 830.0),
        "Z04" => r(70.0, 290.0, 1820.0, 880.0),
        "Z05" => super::frame::BODY,
        "Z00" => super::frame::SAFE,
        "Z06" => r(165.0, 340.0, 1770.0, 870.0),
        "Z07" => r(185.0, 180.0, 1500.0, 1062.0),
        "Z08" => r(135.0, 305.0, 1775.0, 960.0),
        "Z09" => r(80.0, 275.0, 1860.0, 930.0),
        "Z10" => r(70.0, 300.0, 1850.0, 940.0),
        "Z10B" => r(96.0, 292.0, 1824.0, 874.0),
        _ => r(84.0, 290.0, 1836.0, 950.0),
    }
}

const fn r(left: f32, top: f32, right: f32, bottom: f32) -> Rect {
    Rect { left, top, right, bottom }
}
