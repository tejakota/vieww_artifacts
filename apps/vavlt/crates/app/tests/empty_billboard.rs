//! The empty vault's copy must not be drawn on top of its own ghost wall.
//!
//! ```console
//! cargo test -p vavlt-app --test empty_billboard
//! ```
//!
//! Reported by eye from `target/screenshots/01-vavlt-empty-phone-*.png`: the
//! eyebrow "LOCAL · NO NETWORK" and the headline "Get space back / without
//! losing anything" were drawn over the outlined tile grid, so "NO NETWORK" sat
//! on a tile border and the word "space" sat inside a lit tile. With a real
//! photograph behind it that overlay is the whole design; with the placeholder
//! wall behind it, it reads as a rendering fault.
//!
//! # Why this is asserted on geometry rather than on a screenshot
//!
//! A screenshot is what *found* it and a screenshot cannot keep it fixed —
//! nothing in a PNG fails a build. The failure is two boxes overlapping, and
//! two boxes overlapping is a number, so the assertion is the number: the top
//! of the highest glyph run in the billboard against the bottom of the lowest
//! tile outline.
//!
//! The suite already had forty screenshots and every one of them was green
//! through this defect, which is the same lesson `docs/AIMS.md` draws about
//! performance: a count of what is tested is not a reading of what is on the
//! screen.

use vieww::foundation::{Rect, Size};
use vieww::paint::Command;
use vieww::prelude::*;
use vieww::FrameDriver;

const PHONE: Size = Size {
    width: 412.0,
    height: 915.0,
};

/// The empty billboard exactly as `vault.rs` builds it, and nothing else.
///
/// Built here rather than driven through `VavltApp` so the assertion is about
/// this one component: a failure means the billboard changed, not that some
/// other screen grew a row above it.
fn drive(dark: bool) -> FrameDriver {
    let theme = vavlt_app::theme::VavltTheme::new(dark, vavlt_app::FormFactor::Phone);
    let colors = theme.colors;
    let height = vavlt_app::ui::Height::Home.for_form(theme.form);

    let mut driver = FrameDriver::new(PHONE);
    driver.set_root(
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min)
            .children(children![vavlt_app::ui::billboard(
                &theme,
                height,
                // The state under test: no photograph, so the ghost wall.
                None,
                false,
                children![
                    vavlt_app::ui::eyebrow(&theme, "local · no network", None),
                    vavlt_app::ui::figure(&theme, "Get space back", 38.0, colors.fg),
                    vavlt_app::ui::figure(
                        &theme,
                        "without losing anything",
                        38.0,
                        colors.fg.with_alpha(128)
                    ),
                    vavlt_app::ui::gap(10.0),
                    vavlt_app::ui::lede(
                        &theme,
                        "Hand vavlt some photos. It shows you exactly what it can save on those \
                         files, proves every claim, and does nothing until you say so."
                    ),
                ],
            )]),
    );

    // `LayoutBuilder` seeds itself with the surface and rebuilds against the
    // measured width, so the first frame is not the laid-out one.
    for _ in 0..8 {
        driver.draw_frame();
    }
    driver
}

/// Every glyph run's bounding box, in surface coordinates.
fn text_bounds(driver: &FrameDriver) -> Vec<Rect> {
    driver
        .scene()
        .commands()
        .iter()
        .filter_map(|command| match command {
            Command::DrawGlyphs {
                run,
                transform,
                clip,
            } => Some(intersect(transform.apply_rect(run.bounds()), clip.bounds())),
            _ => None,
        })
        .collect()
}

/// Every ghost tile's bounding box.
///
/// A tile is a `Container` with a border, and a border is recorded as a
/// **`FillPath` of a rounded ring** rather than a stroke — so filtering on
/// `StrokePath` finds nothing at all, and a test that did would fail on the
/// wrong assertion. (It did, on the first run.) They are picked out by their
/// declared height instead, which is a constant of the design and the one
/// property that distinguishes them from the two full-band gradients drawn in
/// the same layer.
fn tile_bounds(driver: &FrameDriver) -> Vec<Rect> {
    /// `ghost_tiles` builds every cell at exactly this height.
    const TILE: f32 = 66.0;

    driver
        .scene()
        .commands()
        .iter()
        .filter_map(|command| match command {
            Command::FillPath {
                path,
                transform,
                clip,
                ..
            } => {
                let drawn = transform.apply_rect(path.bounds());
                // **Intersected with the clip, and this is the whole point.**
                // A recorded path keeps its full geometry whatever clips it —
                // clipping is a field on the command, not a change to the
                // shape. So the tile that the fix cuts in half still *records*
                // its full 66 points, and a test that read the raw bounds
                // would report the wall reaching exactly as far as it did
                // before the fix. It did, on the second run: y=290.0 either
                // way. What actually changed, and what has to be measured, is
                // how much of it reaches the surface.
                Some(intersect(drawn, clip.bounds()))
            }
            _ => None,
        })
        .filter(|rect| rect.bottom > rect.top)
        // Clipped tiles are shorter than a full one and never taller, so the
        // upper bound is exact and the lower bound only has to exclude the
        // band-sized gradients.
        .filter(|rect| (rect.bottom - rect.top) <= TILE + 0.5)
        .collect()
}

/// `drawn` cut down to the clip that was in force when it was recorded.
fn intersect(drawn: Rect, clip: Option<Rect>) -> Rect {
    match clip {
        Some(clip) => Rect::new(
            drawn.left.max(clip.left),
            drawn.top.max(clip.top),
            drawn.right.min(clip.right),
            drawn.bottom.min(clip.bottom),
        ),
        None => drawn,
    }
}

fn lowest_edge(boxes: &[Rect]) -> f32 {
    boxes
        .iter()
        .fold(f32::NEG_INFINITY, |low, r| low.max(r.bottom))
}

fn highest_edge(boxes: &[Rect]) -> f32 {
    boxes.iter().fold(f32::INFINITY, |high, r| high.min(r.top))
}

#[test]
fn the_copy_starts_below_every_ghost_tile() {
    for dark in [true, false] {
        let driver = drive(dark);

        let tiles = tile_bounds(&driver);
        assert!(
            tiles.len() >= 5,
            "dark={dark}: expected the ghost wall to draw outlined tiles; found \
             {} tile-sized fills, so this test is measuring the wrong thing",
            tiles.len()
        );

        let text = text_bounds(&driver);
        assert!(
            text.len() >= 3,
            "dark={dark}: expected the eyebrow, two headline lines and the lede; \
             found {} glyph runs",
            text.len()
        );

        let wall_bottom = lowest_edge(&tiles);
        let copy_top = highest_edge(&text);

        assert!(
            copy_top >= wall_bottom,
            "dark={dark}: the copy starts at y={copy_top:.1} and the ghost wall \
             reaches y={wall_bottom:.1}, so {:.1} points of headline are drawn \
             over the placeholder tiles",
            wall_bottom - copy_top
        );
    }
}

/// The band is still exactly as tall as it declares.
///
/// The fix restructured the billboard from one stack into a column, and the
/// obvious way to get that wrong is to let the copy push the band taller than
/// `Height::Home` — which would move every rail below it down the screen and
/// nothing else in the suite asserts the total.
#[test]
fn the_band_is_still_its_declared_height() {
    let theme = vavlt_app::theme::VavltTheme::new(true, vavlt_app::FormFactor::Phone);
    let expected = vavlt_app::ui::Height::Home.for_form(theme.form);

    let driver = drive(true);
    let everything: Vec<Rect> = text_bounds(&driver)
        .into_iter()
        .chain(tile_bounds(&driver))
        .collect();

    let bottom = lowest_edge(&everything);
    assert!(
        bottom <= expected + 0.5,
        "the billboard declares {expected} points and something is drawn at \
         y={bottom:.1}, so the band grew to fit its copy instead of the copy \
         fitting the band"
    );
}
