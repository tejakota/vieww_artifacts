//! The icons this app needs that `vieww_widget::icons` does not ship.
//!
//! The framework is explicit that it is "not an icon library" — it carries the
//! six or so shapes its own controls need (a tick for a checkbox, a cross for a
//! chip, chevrons for a list row) and expects an application to bring the rest.
//! `IconData` being an open type is the mechanism for that.
//!
//! # Filled outlines, not strokes
//!
//! The paint layer fills paths and does not stroke them, so every shape here is
//! a closed outline rather than a centre-line with a width. A "2px stroked"
//! sparkle is drawn as the polygon its stroke would have covered. The framework
//! makes the same choice for the same reason and says so in `icons.rs`: at icon
//! sizes a stroke has to become an outline for rasterisation anyway, and doing
//! the conversion at design time means the shape is what was intended rather
//! than what a joins-and-caps algorithm made of it.
//!
//! Everything is on the Material 24x24 grid, so these sit correctly beside the
//! framework's own.

use vieww_foundation::{IconData, Offset, Path, Rect};

/// Close a polygon through `points` on the 24x24 grid.
fn polygon(points: &[(f32, f32)]) -> Path {
    let mut path = Path::new();
    let mut points = points.iter();
    if let Some(&(x, y)) = points.next() {
        path.move_to(Offset::new(x, y));
    }
    for &(x, y) in points {
        path.line_to(Offset::new(x, y));
    }
    path.close();
    path
}

/// A four-pointed star with concave sides, centred on `(cx, cy)`.
///
/// The concavity is what makes it read as a *sparkle* rather than as a diamond:
/// each edge is a cubic bowing in toward the centre, which is the shape a
/// specular highlight makes in a lens.
fn spark(path: &mut Path, cx: f32, cy: f32, radius: f32) {
    // How far the control points sit from the centre. Lower is more concave.
    let waist = radius * 0.30;
    path.move_to(Offset::new(cx, cy - radius));
    path.cubic_to(
        Offset::new(cx + waist * 0.4, cy - waist),
        Offset::new(cx + waist, cy - waist * 0.4),
        Offset::new(cx + radius, cy),
    );
    path.cubic_to(
        Offset::new(cx + waist, cy + waist * 0.4),
        Offset::new(cx + waist * 0.4, cy + waist),
        Offset::new(cx, cy + radius),
    );
    path.cubic_to(
        Offset::new(cx - waist * 0.4, cy + waist),
        Offset::new(cx - waist, cy + waist * 0.4),
        Offset::new(cx - radius, cy),
    );
    path.cubic_to(
        Offset::new(cx - waist, cy - waist * 0.4),
        Offset::new(cx - waist * 0.4, cy - waist),
        Offset::new(cx, cy - radius),
    );
    path.close();
}

/// Two sparkles — the Render button's icon, and the badge on an upscaled tile.
///
/// Two rather than one because a single star reads as "favourite"; a large one
/// with a small one beside it is the established "enhance" glyph.
#[must_use]
pub fn sparkles() -> IconData {
    let mut path = Path::new();
    spark(&mut path, 9.5, 10.0, 7.0);
    spark(&mut path, 18.0, 17.5, 4.0);
    IconData::square24(path)
}

/// A framed picture with a horizon and a sun: the empty state's icon.
#[must_use]
pub fn photo() -> IconData {
    // The frame, as a ring: outer rectangle then an inner one wound the other
    // way, so the non-zero fill rule leaves the middle empty.
    let mut path = Path::new();
    let outer = Rect::new(3.0, 4.0, 21.0, 20.0);
    path.extend(&Path::rounded_rect(outer, 2.5));
    let inner = Rect::new(5.0, 6.0, 19.0, 18.0);
    path.extend(&Path::rounded_rect(inner, 1.5).reversed());

    // A hill inside the frame, and a sun above it.
    path.extend(&polygon(&[
        (6.0, 17.0),
        (10.0, 11.5),
        (13.0, 15.0),
        (15.0, 13.0),
        (18.0, 17.0),
    ]));
    path.extend(&Path::arc(
        Offset::new(15.5, 9.5),
        1.6,
        0.0,
        std::f32::consts::TAU,
    ));
    IconData::square24(path)
}

/// A downward arrow into a tray: "Save to Photos".
#[must_use]
pub fn save() -> IconData {
    let mut path = Path::new();
    // The shaft and head, as one closed outline.
    path.extend(&polygon(&[
        (10.75, 3.0),
        (13.25, 3.0),
        (13.25, 11.5),
        (16.5, 11.5),
        (12.0, 16.5),
        (7.5, 11.5),
        (10.75, 11.5),
    ]));
    // The tray: a squared-off U.
    path.extend(&polygon(&[
        (4.0, 14.5),
        (6.5, 14.5),
        (6.5, 18.5),
        (17.5, 18.5),
        (17.5, 14.5),
        (20.0, 14.5),
        (20.0, 21.0),
        (4.0, 21.0),
    ]));
    IconData::square24(path)
}

/// Two arrows back to back — the compare sheet's drag handle.
#[must_use]
pub fn compare() -> IconData {
    let mut path = Path::new();
    path.extend(&polygon(&[(10.5, 6.0), (10.5, 18.0), (4.0, 12.0)]));
    path.extend(&polygon(&[(13.5, 6.0), (20.0, 12.0), (13.5, 18.0)]));
    IconData::square24(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every icon must produce geometry, and all of it must land inside the
    /// 24x24 grid — an icon whose path escapes its viewbox is scaled down by
    /// `IconData::fitted` and silently arrives smaller than its neighbours.
    #[test]
    fn icons_are_non_empty_and_in_grid() {
        for (name, icon) in [
            ("sparkles", sparkles()),
            ("photo", photo()),
            ("save", save()),
            ("compare", compare()),
        ] {
            assert!(!icon.path().is_empty(), "{name} produced no geometry");
            let bounds = icon.path().bounds();
            assert!(
                bounds.left >= -0.01
                    && bounds.top >= -0.01
                    && bounds.right <= 24.01
                    && bounds.bottom <= 24.01,
                "{name} escapes the 24x24 grid: {bounds:?}"
            );
        }
    }
}
