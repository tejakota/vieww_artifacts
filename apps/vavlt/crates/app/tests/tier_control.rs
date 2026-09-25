//! The tier control's selection must land inside its own track.
//!
//! ```console
//! cargo test -p vavlt-app --test tier_control
//! ```
//!
//! Reported from a phone as "the toggle stays on Move and only the colour
//! changes". What it was actually doing was sliding the selected lozenge *past*
//! the right-hand edge of the track and off the screen, so the part still
//! visible sat over the wrong label.
//!
//! Asserted on the painted rectangle rather than on a screenshot, because the
//! failure is arithmetic and a number says which way it is wrong.

use vavlt_core::Tier;
use vieww::foundation::{Rect, Size};
use vieww::paint::Command;
use vieww::prelude::*;
use vieww::FrameDriver;

const WIDTH: f32 = 412.0;
const GUTTER: f32 = 20.0;

/// The control, inset by a gutter the way every screen insets it.
fn drive(tier: Tier) -> FrameDriver {
    let theme = vavlt_app::theme::VavltTheme::new(true, vavlt_app::FormFactor::Phone);
    let mut driver = FrameDriver::new(Size::new(WIDTH, 900.0));
    driver.set_root(
        Padding::new(vieww::foundation::EdgeInsets::symmetric(GUTTER, 0.0)).child(
            Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_size(MainAxisSize::Min)
                .children(children![vavlt_app::ui::tier_control(
                    &theme,
                    tier,
                    "32.9 MB",
                    "58.7 MB",
                    |_| {},
                )]),
        ),
    );
    // Enough frames for the LayoutBuilder to hear its real width and the
    // selection animation to reach its end.
    for _ in 0..40 {
        driver.draw_frame();
    }
    driver
}

/// Every filled area in the frame, in paint order, in surface coordinates.
///
/// Both variants: a plain box records a `FillRect`, and anything with a corner
/// radius — which the track and the lozenge both have — records a `FillPath`.
/// A test that looked only at `FillRect` found nothing at all and failed on the
/// wrong assertion, which is worth more comment than it looks.
fn fills(driver: &FrameDriver) -> Vec<Rect> {
    driver
        .scene()
        .commands()
        .iter()
        .filter_map(|command| match command {
            Command::FillRect {
                rect, transform, ..
            } => Some(transform.apply_rect(*rect)),
            Command::FillPath {
                path, transform, ..
            } => Some(transform.apply_rect(path.bounds())),
            _ => None,
        })
        .collect()
}

/// The track: the widest fill in the frame.
fn track(rects: &[Rect]) -> Rect {
    *rects
        .iter()
        .max_by(|a, b| a.width().total_cmp(&b.width()))
        .expect("the control drew something")
}

#[test]
fn the_selection_stays_inside_the_track_on_both_sides() {
    for (name, tier) in [("Move", Tier::Move), ("Deep Move", Tier::DeepMove)] {
        let driver = drive(tier);
        let rects = fills(&driver);
        let track = track(&rects);

        // The lozenge: the widest fill that is not the track itself.
        let thumb = rects
            .iter()
            .filter(|rect| rect.width() < track.width() - 1.0)
            .max_by(|a, b| a.width().total_cmp(&b.width()))
            .copied()
            .expect("the selection drew something");

        assert!(
            thumb.left >= track.left - 0.5 && thumb.right <= track.right + 0.5,
            "{name}: the selection is at {thumb:?} and the track is at \
             {track:?} — it is {}pt past the right edge",
            (thumb.right - track.right)
                .max(track.left - thumb.left)
                .round()
        );
    }
}

#[test]
fn the_selection_is_on_the_half_that_was_chosen() {
    for (name, tier, want_left_half) in [
        ("Move", Tier::Move, true),
        ("Deep Move", Tier::DeepMove, false),
    ] {
        let driver = drive(tier);
        let rects = fills(&driver);
        let track = track(&rects);
        let thumb = rects
            .iter()
            .filter(|rect| rect.width() < track.width() - 1.0)
            .max_by(|a, b| a.width().total_cmp(&b.width()))
            .copied()
            .expect("the selection drew something");

        let middle = track.left + track.width() / 2.0;
        let thumb_middle = thumb.left + thumb.width() / 2.0;
        assert_eq!(
            thumb_middle < middle,
            want_left_half,
            "{name}: the selection's centre is at {thumb_middle} and the \
             track's is at {middle}"
        );
    }
}
