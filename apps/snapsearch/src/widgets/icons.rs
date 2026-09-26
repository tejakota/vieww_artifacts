//! Icons the framework's built-in set doesn't carry.
//!
//! `vieww_widget::icons` ships check/close/chevrons/add/remove — enough for
//! its own controls, but not a magnifier, which is the one glyph this app
//! genuinely needs. An earlier pass drew the FAB with a `🔍` emoji in a
//! `Text`, which renders at the mercy of whatever emoji font the device
//! happens to have and does not take the icon colour. This is real vector
//! geometry instead, on the same 24×24 grid the built-in set uses.

use vieww::foundation::{IconData, Offset, Path, Rect};

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

/// The four-point sparkle — the "found it by meaning" mark.
///
/// Not a star polygon: the edge from each outer point to the next is a cubic
/// whose control points are pulled 72% of the way toward the centre, which is
/// what makes the sides bow in. Straight edges would read as a compass rose;
/// the concavity is what reads as a *glint*.
pub fn sparkle() -> IconData {
    let mut path = Path::new();

    // The big glint, centred at (11, 13) with an 11-unit reach.
    sparkle_contour(
        &mut path,
        Offset::new(11.0, 13.0),
        11.0,
    );
    // A small companion up and to the right — the second glint the "sparkles"
    // glyph carries. Same construction, a quarter of the size.
    sparkle_contour(
        &mut path,
        Offset::new(20.0, 4.0),
        3.6,
    );

    IconData::square24(path)
}

/// One four-point glint: four cubic arcs, outer point to outer point, each
/// with both control points `0.72 * radius` in from the tip it starts or ends
/// at. Wound once around, closed — one contour, filled by the same nonzero
/// rule as everything else.
fn sparkle_contour(path: &mut Path, center: Offset, r: f32) {
    let pull = 0.72 * r;
    let (cx, cy) = (center.dx, center.dy);
    // N, E, S, W tips.
    let n = Offset::new(cx, cy - r);
    let e = Offset::new(cx + r, cy);
    let s = Offset::new(cx, cy + r);
    let w = Offset::new(cx - r, cy);

    path.move_to(n);
    // N → E, bowing toward the centre.
    path.cubic_to(
        Offset::new(cx, cy - r + pull),
        Offset::new(cx + r - pull, cy),
        e,
    );
    // E → S.
    path.cubic_to(
        Offset::new(cx + r - pull, cy),
        Offset::new(cx, cy + r - pull),
        s,
    );
    // S → W.
    path.cubic_to(
        Offset::new(cx, cy + r - pull),
        Offset::new(cx - r + pull, cy),
        w,
    );
    // W → N.
    path.cubic_to(
        Offset::new(cx - r + pull, cy),
        Offset::new(cx, cy - r + pull),
        n,
    );
    path.close();
}

/// A photograph glyph — the frame, a sun, and one mountain ridge.
///
/// Three contours in one fill: the frame is a rounded ring (outer wound one
/// way, inner the other, so the nonzero rule leaves the middle open), the sun
/// a small annulus, and the ridge two straight lines off the inner floor.
/// All one colour, as an icon is.
pub fn image() -> IconData {
    let mut path = Path::new();

    // The frame: `rounded_ring` already winds its two contours against each
    // other — that is the documented contract of a ring under the nonzero
    // fill rule.
    let frame = Path::rounded_ring(
        Rect::new(2.5, 4.5, 21.5, 19.5),
        3.0,
        1.9,
    );
    path.extend(&frame);

    // The sun, a tiny annulus in the frame's sky.
    let sun = Path::rounded_ring(
        Rect::new(15.2, 7.6, 18.4, 10.8),
        1.6,
        1.3,
    );
    path.extend(&sun);

    // The ridge: up from the inner floor to a peak, down to a lower peak,
    // back to the floor. Filled as a contour of its own — the frame's inner
    // edge and the ridge's base share a y, so the two read as one silhouette.
    path.move_to(Offset::new(4.4, 17.6));
    path.line_to(Offset::new(9.2, 11.2));
    path.line_to(Offset::new(12.6, 15.0));
    path.line_to(Offset::new(15.4, 12.4));
    path.line_to(Offset::new(19.6, 17.6));
    path.close();

    IconData::square24(path)
}
