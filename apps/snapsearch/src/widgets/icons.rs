//! Icons the framework's built-in set doesn't carry.
//!
//! `vieww_widget::icons` ships check/close/chevrons/add/remove — enough for
//! its own controls, but not a magnifier, which is the one glyph this app
//! genuinely needs. An earlier pass drew the FAB with a `🔍` emoji in a
//! `Text`, which renders at the mercy of whatever emoji font the device
//! happens to have and does not take the icon colour. This is real vector
//! geometry instead, on the same 24×24 grid the built-in set uses.

use vieww::foundation::{IconData, Offset, Path};

/// The circle-to-Bézier constant: the control-point distance, as a
/// fraction of the radius, that makes four cubics approximate a circle to
/// within about a thousandth of the radius.
const KAPPA: f32 = 0.552_284_75;

/// Append a circle to `path`.
///
/// Winding direction is the whole point: filling uses the non-zero rule, so
/// an outer ring wound one way and an inner ring wound the other cancel in
/// the overlap and leave a hole. Same-direction contours would just fill in
/// solid — which is how a magnifier turns into a lollipop.
fn circle(path: &mut Path, cx: f32, cy: f32, r: f32, clockwise: bool) {
    let k = KAPPA * r;
    let s = if clockwise { 1.0 } else { -1.0 };

    path.move_to(Offset::new(cx + r, cy));
    path.cubic_to(
        Offset::new(cx + r, cy + s * k),
        Offset::new(cx + k, cy + s * r),
        Offset::new(cx, cy + s * r),
    );
    path.cubic_to(
        Offset::new(cx - k, cy + s * r),
        Offset::new(cx - r, cy + s * k),
        Offset::new(cx - r, cy),
    );
    path.cubic_to(
        Offset::new(cx - r, cy - s * k),
        Offset::new(cx - k, cy - s * r),
        Offset::new(cx, cy - s * r),
    );
    path.cubic_to(
        Offset::new(cx + k, cy - s * r),
        Offset::new(cx + r, cy - s * k),
        Offset::new(cx + r, cy),
    );
    path.close();
}

/// A magnifying glass: a ring, and a handle off the lower right.
pub fn search() -> IconData {
    let mut path = Path::new();

    // The lens, as an annulus.
    circle(&mut path, 10.0, 10.0, 7.0, true);
    circle(&mut path, 10.0, 10.0, 5.0, false);

    // The handle, a bar along the 45° diagonal out of the lens.
    let (x0, y0) = (14.6, 14.6);
    let (x1, y1) = (20.5, 20.5);
    let half = 1.15; // half the bar's thickness, measured across the diagonal
    path.move_to(Offset::new(x0 - half, y0 + half));
    path.line_to(Offset::new(x1 - half, y1 + half));
    path.line_to(Offset::new(x1 + half, y1 - half));
    path.line_to(Offset::new(x0 + half, y0 - half));
    path.close();

    IconData::square24(path)
}

/// A short horizontal bar — the grab handle at the top of a sheet.
pub fn sheet_grip() -> IconData {
    let mut path = Path::new();
    path.move_to(Offset::new(6.0, 11.0));
    path.line_to(Offset::new(18.0, 11.0));
    path.line_to(Offset::new(18.0, 13.0));
    path.line_to(Offset::new(6.0, 13.0));
    path.close();
    IconData::square24(path)
}
