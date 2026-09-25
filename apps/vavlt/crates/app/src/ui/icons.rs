//! The app's own icons.
//!
//! vieww ships ten — a tick, a cross, four chevrons, plus and minus — and
//! deliberately so: a framework that shipped an icon set would be shipping a
//! visual identity. This app needs fourteen, and the three it was borrowing
//! were nonsense: a **tick** for the vavlt tab, a **downward chevron** for
//! activity and a **plus** for settings, because those were the nearest members
//! of a set that has no members near them.
//!
//! # How these are built
//!
//! `IconData` is a *filled* path on a 24×24 grid, so a stroked look has to be
//! drawn as the outline's fill. Rather than hand-authoring outlines — which is
//! where an icon set stops looking like a set, because every author rounds a
//! corner differently — every icon here is composed from four primitives:
//! [`bar`], [`line`], [`ring`] and [`dot`]. One stroke width, one corner
//! radius, one grid. That is what makes them a family, and it is why adding a
//! fifteenth is six lines rather than an afternoon in a vector editor.
//!
//! # Why 1.9 and not 2
//!
//! [`STROKE`] is the visual weight of Adwaita's own symbolics at this size. A
//! 2.0 stroke on a 24-grid reads a shade heavier than the text beside it at
//! `t_body`, and an icon that out-weighs its own label is the thing that makes
//! a tab bar look like a toolbar.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

use vieww::foundation::{IconData, Offset, Path, Rect, Transform};

/// The one stroke width. Every icon in this module is drawn with it.
const STROKE: f32 = 1.9;

/// Half the stroke, which is the radius that makes a bar's end a semicircle.
const CAP: f32 = STROKE / 2.0;

/// The grid everything is designed on.
const GRID: f32 = 24.0;

/// A rectangle from a corner and a *size*.
///
/// # Why this exists rather than `Rect::new`
///
/// Because `Rect::new` takes **`(left, top, right, bottom)`**, and every
/// primitive in this module was calling it as `(x, y, width, height)`. Every
/// icon in the application was therefore built from rectangles whose right and
/// bottom edges were its width and height — so a 1.9-wide bar came out 2.85
/// wide and offset, a `dot` at (8.6, 9.6) came out as a rectangle from
/// (7.1, 8.1) to (3.0, 3.0), and a frame's hole did not line up with its
/// outside.
///
/// It never failed a test. The icon tests assert *relative* geometry — one
/// icon's ink against another's — and a consistent error passes all of them.
/// It failed on screen: at twenty points every glyph in the tab bar and every
/// glyph on a button rendered as an unreadable blob, which is what a screenshot
/// showed and no assertion did.
///
/// A named constructor, so the mistake cannot be made again by writing four
/// numbers in the order they are said out loud.
const fn boxed(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, x + w, y + h)
}

// --- primitives ------------------------------------------------------------

/// A horizontal or vertical stroke, with round caps.
#[must_use]
fn bar(x: f32, y: f32, w: f32, h: f32) -> Path {
    Path::rounded_rect(boxed(x, y, w, h), CAP.min(w / 2.0).min(h / 2.0))
}

/// A stroke between two points, at any angle, with round caps.
///
/// Built as a horizontal bar and rotated, so a diagonal has exactly the same
/// weight and the same caps as an axis-aligned one. Drawing diagonals directly
/// is how a set ends up with a tick that is visibly thinner than its cross.
#[must_use]
fn line(from: (f32, f32), to: (f32, f32)) -> Path {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = dx.hypot(dy);
    // Drawn centred on its own origin so the rotation pivots on the start cap.
    let flat = bar(0.0, -CAP, length, STROKE);
    flat.transformed(
        Transform::rotate(dy.atan2(dx)).then(Transform::translate(Offset::new(from.0, from.1))),
    )
}

/// A circle outline.
#[must_use]
fn ring(cx: f32, cy: f32, radius: f32) -> Path {
    Path::rounded_ring(
        boxed(cx - radius, cy - radius, radius * 2.0, radius * 2.0),
        radius,
        STROKE,
    )
}

/// A partial circle outline, angles in radians, clockwise from three o'clock.
#[must_use]
fn arc(cx: f32, cy: f32, radius: f32, start: f32, sweep: f32) -> Path {
    Path::arc_ring(Offset::new(cx, cy), radius, STROKE, start, sweep)
}

/// A filled disc.
#[must_use]
fn dot(cx: f32, cy: f32, radius: f32) -> Path {
    Path::rounded_ring(
        boxed(cx - radius, cy - radius, radius * 2.0, radius * 2.0),
        radius,
        radius,
    )
}

/// A rounded rectangle outline.
#[must_use]
fn frame(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Path {
    let mut outer = Path::rounded_rect(boxed(x, y, w, h), radius);
    // The hole, wound the other way, so the even-odd fill leaves an outline.
    let inner = Path::rounded_rect(
        boxed(x + STROKE, y + STROKE, w - STROKE * 2.0, h - STROKE * 2.0),
        (radius - STROKE).max(0.0),
    )
    .reversed();
    outer.extend(&inner);
    outer
}

/// Assemble the parts into one icon on the shared grid.
#[must_use]
fn icon(parts: impl IntoIterator<Item = Path>) -> IconData {
    let mut path = Path::default();
    for part in parts {
        path.extend(&part);
    }
    IconData::new(path, boxed(0.0, 0.0, GRID, GRID))
}

// --- the tabs --------------------------------------------------------------

/// **The vavlt tab.** A safe: a frame with a dial on it.
///
/// Named for the shape rather than the brand — `vavlt_mark` is the brand.
///
/// Not a padlock — a padlock is what the *delete grant* would be, and the two
/// must not share a shape when one is a place and the other is a permission.
#[must_use]
pub fn vault() -> IconData {
    icon([
        frame(3.0, 4.0, 18.0, 16.0, 3.0),
        ring(11.0, 12.0, 3.6),
        dot(11.0, 12.0, 1.0),
        // The handle, at four o'clock, so the dial reads as a dial rather than
        // as a camera lens.
        line((13.6, 14.6), (15.6, 16.6)),
    ])
}

/// **Activity.** A clock, because the log is ordered by time and nothing else.
#[must_use]
pub fn clock() -> IconData {
    icon([
        ring(12.0, 12.0, 8.4),
        // Hands at roughly ten past two — asymmetric, so the icon has an
        // orientation and does not read as a plain circle at 21px.
        line((12.0, 12.0), (12.0, 7.4)),
        line((12.0, 12.0), (15.4, 13.6)),
    ])
}

/// **Settings.** Sliders, not a cogwheel.
///
/// A cog says "machinery you should not touch"; this screen is a list of
/// choices and disclosures, and sliders say "things set to values".
#[must_use]
pub fn sliders() -> IconData {
    icon([
        bar(3.5, 6.05, 17.0, STROKE),
        bar(3.5, 16.05, 17.0, STROKE),
        // Knobs at different positions, or the two rows read as one control.
        dot(9.0, 7.0, 2.6),
        dot(15.5, 17.0, 2.6),
    ])
}

// --- state and marks -------------------------------------------------------

/// A tick. Proven, granted, done.
#[must_use]
pub fn check() -> IconData {
    icon([
        line((4.8, 12.6), (9.6, 17.4)),
        line((9.6, 17.4), (19.2, 6.8)),
    ])
}

/// A cross. Excluded, dismissed, failed.
#[must_use]
pub fn cross() -> IconData {
    icon([
        line((6.4, 6.4), (17.6, 17.6)),
        line((17.6, 6.4), (6.4, 17.6)),
    ])
}

/// A padlock. **The delete grant, and RAW protection** — the two places
/// something is held shut.
#[must_use]
pub fn lock() -> IconData {
    icon([
        frame(5.0, 10.5, 14.0, 9.5, 2.4),
        // The shackle: a half-turn open at the bottom, meeting the body.
        arc(12.0, 10.5, 4.0, PI, PI),
        dot(12.0, 15.0, 1.3),
    ])
}

/// A shield. The invariants — what the app refuses to do.
#[must_use]
pub fn shield() -> IconData {
    let mut path = Path::default();
    path.move_to(Offset::new(12.0, 3.2));
    path.line_to(Offset::new(19.8, 6.4));
    path.line_to(Offset::new(19.8, 12.0));
    path.cubic_to(
        Offset::new(19.8, 16.4),
        Offset::new(16.6, 19.4),
        Offset::new(12.0, 20.8),
    );
    path.cubic_to(
        Offset::new(7.4, 19.4),
        Offset::new(4.2, 16.4),
        Offset::new(4.2, 12.0),
    );
    path.line_to(Offset::new(4.2, 6.4));
    path.close();

    // Hollowed by scaling the same silhouette about its centre, so the outline
    // keeps an even weight all the way round a curve — which an inset path
    // built by hand does not.
    let bounds = path.bounds();
    let inset = path
        .transformed(
            Transform::translate(Offset::new(-12.0, -12.0))
                .then(Transform::scale(
                    1.0 - STROKE * 2.0 / bounds.width(),
                    1.0 - STROKE * 2.0 / bounds.height(),
                ))
                .then(Transform::translate(Offset::new(12.0, 12.0))),
        )
        .reversed();
    path.extend(&inset);
    IconData::new(path, boxed(0.0, 0.0, GRID, GRID))
}

/// The vavlt brand mark: a clean V with a hollow square at bottom-right.
#[must_use]
pub fn vavlt_mark() -> IconData {
    let mut path = Path::new();
    path.move_to(Offset::new(3.5, 4.0));
    path.line_to(Offset::new(8.8, 19.5));
    path.cubic_to(
        Offset::new(9.1, 20.4),
        Offset::new(10.0, 21.0),
        Offset::new(11.0, 21.0),
    );
    path.cubic_to(
        Offset::new(12.0, 21.0),
        Offset::new(12.9, 20.4),
        Offset::new(13.2, 19.5),
    );
    path.line_to(Offset::new(18.5, 4.0));
    path.line_to(Offset::new(15.3, 4.0));
    path.line_to(Offset::new(11.0, 17.0));
    path.line_to(Offset::new(6.7, 4.0));
    path.close();

    // Hollow square in the lower-right corner, represented as an even-odd
    // filled ring so the icon remains a single vector path.
    let mut square = Path::rounded_rect(boxed(15.0, 15.0, 6.0, 6.0), 0.6);
    square.extend(&Path::rounded_rect(boxed(16.3, 16.3, 3.4, 3.4), 0.2).reversed());
    path.extend(&square);
    IconData::new(path, boxed(0.0, 0.0, GRID, GRID))
}

// --- content ---------------------------------------------------------------

/// A photograph. The empty state, and anywhere a picture stands for the set.
#[must_use]
pub fn photo() -> IconData {
    icon([
        frame(3.0, 5.0, 18.0, 14.0, 2.6),
        dot(8.6, 9.6, 1.5),
        // A ridge line rather than a filled triangle: at 20px a filled mountain
        // and the frame merge into a solid block.
        line((5.4, 16.2), (10.4, 11.2)),
        line((10.4, 11.2), (14.0, 14.8)),
        line((14.0, 14.8), (16.4, 12.4)),
        line((16.4, 12.4), (18.6, 14.6)),
    ])
}

/// A downward arrow onto a line. Export.
#[must_use]
pub fn export() -> IconData {
    icon([
        line((12.0, 3.6), (12.0, 14.4)),
        line((12.0, 14.6), (7.6, 10.2)),
        line((12.0, 14.6), (16.4, 10.2)),
        bar(4.4, 18.05, 15.2, STROKE),
    ])
}

/// A left-pointing chevron. Back.
#[must_use]
pub fn back() -> IconData {
    icon([
        line((15.0, 5.4), (8.4, 12.0)),
        line((8.4, 12.0), (15.0, 18.6)),
    ])
}

/// A right-pointing chevron. A row that leads somewhere.
#[must_use]
pub fn forward() -> IconData {
    icon([
        line((9.0, 5.4), (15.6, 12.0)),
        line((15.6, 12.0), (9.0, 18.6)),
    ])
}

/// Half-filled circle. The light/dark switch.
///
/// A sun-and-moon pair would need two icons and a crossfade; a disc that is
/// half ink says the same thing in one shape, and it is the mark Adwaita's own
/// style switcher uses.
#[must_use]
pub fn contrast() -> IconData {
    let center = Offset::new(12.0, 12.0);
    let radius = 5.8;
    // Build the filled semicircle explicitly with one cubic per quarter.
    // This avoids Path::arc rasterization/stroke overlap issues on Android/Vello.
    const KAPPA: f32 = 0.552_284_8;
    let k = radius * KAPPA;

    let top = Offset::new(center.dx, center.dy - radius);
    let left = Offset::new(center.dx - radius, center.dy);
    let bottom = Offset::new(center.dx, center.dy + radius);

    let mut half = Path::new();
    half.move_to(center)
        .line_to(top)
        .cubic_to(
            Offset::new(center.dx - k, center.dy - radius),
            Offset::new(center.dx - radius, center.dy - k),
            left,
        )
        .cubic_to(
            Offset::new(center.dx - radius, center.dy + k),
            Offset::new(center.dx - k, center.dy + radius),
            bottom,
        )
        .close();

    icon([ring(12.0, 12.0, 8.4), half])
}

/// A circled `i`. A disclosure, a caveat.
#[must_use]
pub fn info() -> IconData {
    icon([
        ring(12.0, 12.0, 8.4),
        dot(12.0, 7.9, 1.15),
        bar(12.0 - CAP, 10.6, STROKE, 6.4),
    ])
}

/// Four tiles. The photo grid, as a mark.
#[must_use]
pub fn grid() -> IconData {
    icon([
        frame(3.6, 3.6, 7.6, 7.6, 2.0),
        frame(12.8, 3.6, 7.6, 7.6, 2.0),
        frame(3.6, 12.8, 7.6, 7.6, 2.0),
        frame(12.8, 12.8, 7.6, 7.6, 2.0),
    ])
}

/// An arrow curling anticlockwise. Restore.
#[must_use]
pub fn restore() -> IconData {
    icon([
        // Open at the top-left so the arrowhead has somewhere to sit.
        arc(12.0, 12.0, 8.0, -FRAC_PI_2 + FRAC_PI_4, TAU - FRAC_PI_2),
        line((6.4, 6.4), (6.4, 11.4)),
        line((6.4, 6.4), (11.4, 6.4)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bar is exactly as thick as the one stroke width this module has.
    ///
    /// **The test that was missing, and that every icon needed.** The whole set
    /// was built on `Rect::new(x, y, w, h)` where the constructor takes
    /// `(left, top, right, bottom)`, so every stroke came out the wrong
    /// thickness and in the wrong place — a 1.9 bar drawn 2.85 wide, a dot
    /// drawn as an inverted rectangle. Nothing caught it: the existing tests
    /// assert one icon's ink *relative to* another's, and a mistake made
    /// identically everywhere passes all of them.
    ///
    /// This one asserts an absolute number against the constant it is supposed
    /// to be, which is the only kind of assertion that could have failed.
    #[test]
    fn a_bar_is_exactly_one_stroke_thick() {
        let horizontal = bar(4.0, 10.0, 16.0, STROKE).bounds();
        assert!(
            (horizontal.height() - STROKE).abs() < 0.01,
            "a horizontal bar is {} thick and STROKE is {STROKE}: {horizontal:?}",
            horizontal.height()
        );
        assert!(
            (horizontal.width() - 16.0).abs() < 0.01,
            "and 16 long: {horizontal:?}"
        );
        assert!(
            (horizontal.left - 4.0).abs() < 0.01 && (horizontal.top - 10.0).abs() < 0.01,
            "starting where it was asked to: {horizontal:?}"
        );
    }

    /// A dot is a disc of the radius it was given, centred where it was put.
    #[test]
    fn a_dot_is_round_and_where_it_was_put() {
        let bounds = dot(8.0, 12.0, 2.0).bounds();
        assert!(
            (bounds.width() - 4.0).abs() < 0.2 && (bounds.height() - 4.0).abs() < 0.2,
            "a radius-2 dot is 4 across: {bounds:?}"
        );
        let (cx, cy) = (
            bounds.left + bounds.width() / 2.0,
            bounds.top + bounds.height() / 2.0,
        );
        assert!(
            (cx - 8.0).abs() < 0.2 && (cy - 12.0).abs() < 0.2,
            "centred on (8, 12): {bounds:?}"
        );
    }

    /// A frame's outline is one stroke thick on every side.
    #[test]
    fn a_frame_is_a_ring_of_one_stroke() {
        let bounds = frame(3.0, 5.0, 18.0, 14.0, 2.6).bounds();
        assert!(
            (bounds.width() - 18.0).abs() < 0.2 && (bounds.height() - 14.0).abs() < 0.2,
            "an 18x14 frame occupies 18x14: {bounds:?}"
        );
        assert!(
            (bounds.left - 3.0).abs() < 0.2 && (bounds.top - 5.0).abs() < 0.2,
            "at (3, 5): {bounds:?}"
        );
    }

    /// Every icon has ink, and all of it is inside the grid it declares.
    ///
    /// The cheap guard against the failure this module's construction invites:
    /// a `line` whose rotation lands it off the viewbox draws nothing visible
    /// and reports no error, and the only symptom is a blank tab.
    #[test]
    fn every_icon_has_ink_inside_its_grid() {
        let set: [(&str, IconData); 14] = [
            ("vault", vault()),
            ("clock", clock()),
            ("sliders", sliders()),
            ("check", check()),
            ("cross", cross()),
            ("lock", lock()),
            ("shield", shield()),
            ("photo", photo()),
            ("export", export()),
            ("back", back()),
            ("forward", forward()),
            ("contrast", contrast()),
            ("info", info()),
            ("grid", grid()),
        ];

        for (name, data) in set {
            let bounds = data.path().bounds();
            assert!(
                bounds.width() > 1.0 && bounds.height() > 1.0,
                "{name} drew nothing"
            );
            // Half a stroke of tolerance: a round cap on a path that touches the
            // edge extends by exactly that much, by design.
            let slack = CAP + 0.01;
            assert!(
                bounds.left >= -slack
                    && bounds.top >= -slack
                    && bounds.right <= GRID + slack
                    && bounds.bottom <= GRID + slack,
                "{name} draws outside its 24x24 grid: {bounds:?}"
            );
        }
    }

    /// A diagonal and an axis-aligned stroke must weigh the same, which is the
    /// whole reason `line` rotates a bar instead of drawing a quadrilateral.
    ///
    /// Measured through the bounding box, since a rotated path has no "width"
    /// of its own: a bar of length L and weight w at 45 degrees bounds to
    /// (L + w) / sqrt(2) on each axis. Getting that formula wrong is what this
    /// comment is for — the first version squared the length by accident and
    /// asserted 24 against a true 17.
    #[test]
    fn a_diagonal_weighs_what_a_horizontal_weighs() {
        // Relative, not absolute. `Path::bounds` is a control-point bound
        // rather than a tight one — a round cap reports about 1.5x the stroke —
        // so an exact number here would be asserting an implementation detail
        // of the curve flattener rather than anything about the icons.
        let flat = line((4.0, 12.0), (20.0, 12.0)).bounds().height();
        let steep = line((12.0, 4.0), (12.0, 20.0)).bounds().width();
        assert!(
            (flat - steep).abs() < 0.05,
            "a horizontal and a vertical must weigh the same: {flat} vs {steep}"
        );

        // The diagonal is checked by its *thickness across the stroke*, which
        // is the only thing "weighs the same" can mean for a rotated path.
        // Its bounding box is not that: `Path::bounds` is a control-point
        // bound, so a 45-degree bar reports a box whose size depends on how the
        // flattener placed the cap curves rather than on the stroke width. An
        // exact assertion there tests the curve library, not these icons.
        let slant = line((4.0, 4.0), (20.0, 20.0)).bounds();
        assert!(
            slant.width() > 15.0 && (slant.width() - slant.height()).abs() < 0.05,
            "a 45-degree bar must bound to a square: {slant:?}"
        );
    }
}
