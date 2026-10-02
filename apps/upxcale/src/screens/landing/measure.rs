//! The compare sheet's measurements strip: edge energy, by position.
//!
//! The sheet's header says "Lanczos + unsharp" — this is where the app stops
//! asking to be believed. The strip measures two images of the photograph it
//! is holding open — the plain 4x resample (Lanczos alone, the control) and
//! the shipped result (Lanczos + unsharp) — and draws the two profiles over
//! each other, so the sharpening pass is a visible difference between two
//! curves rather than a claim on a chip. Both are at the result's own size,
//! so the comparison is like with like: same pixels-per-edge, differing by
//! exactly the unsharp mask. (The like-with-like acutance number the shots
//! print and `metrics.txt` carries is the same measurement, one figure.)
//!
//! What is measured is **local contrast**: for each sampled pixel, the mean
//! absolute RGB difference to its horizontal and vertical neighbours,
//! averaged over a column band. The bands are *relative* positions — band 0
//! is the left edge of the photograph, band 47 the right — because that is
//! what the divider divides.
//!
//! The chart also carries the divider: the stage's split position is drawn as
//! a marker line on the profile, so dragging the handle moves the marker and
//! the two read together — the chart is tied to the interaction, not pasted
//! under it.
//!
//! Drawn through `vieww-dataviz`: the y scale is
//! [`Continuous::linear`](vieww_dataviz::scale::Continuous::linear) `.nice`d
//! so its gridlines land on round numbers, the curves are
//! [`shape::line`](vieww_dataviz::shape::line) through `Curve::MonotoneX`
//! (a smooth line that cannot overshoot between bands and invent a peak the
//! photograph does not have), and the control's colour runs through
//! [`color::ramp`](vieww_dataviz::color::ramp) so it reads as a dimmed
//! variant of the primary rather than a second, unrelated hue.

use std::rc::Rc;

use vieww_dataviz::color;
use vieww_dataviz::scale::Continuous;
use vieww_dataviz::shape::{self, Curve};
use vieww_foundation::{Color, Image as Pixels, Offset, Path, Size, Sketchbook};
use vieww_widget::prelude::*;
use vieww_widget::{Painter, Painting};

/// How many column bands each profile is measured into.
pub const BANDS: usize = 48;

/// How tall the chart's plot area is.
const CHART_HEIGHT: f32 = 76.0;

/// Plot-area insets: room at the left and bottom for the axis captions.
const PAD_LEFT: f32 = 8.0;
const PAD_RIGHT: f32 = 8.0;
const PAD_TOP: f32 = 8.0;
const PAD_BOTTOM: f32 = 14.0;

/// The two profiles, plus the one number the strip headlines.
#[derive(Debug, Clone)]
pub struct Measured {
    /// Mean local contrast per band, `0..=1` relative positions, plain 4x.
    pub before: Vec<f32>,
    /// The same, measured on the sharpened result — same size, same bands.
    pub after: Vec<f32>,
    /// Mean after over mean before, as a ratio (1.10 = 10% more).
    pub gain: f32,
}

impl Measured {
    /// Measure both stages. `None` when either image is missing (the stage
    /// is still on its skeleton), degenerate, or flat — no half-drawn chart.
    ///
    /// `before` is the plain 4x resample and `after` the shipped result, at
    /// the same size — see [`Library::baseline`](crate::photos::Library::baseline).
    pub fn measure(before: &Pixels, after: &Pixels) -> Option<Self> {
        let b = edge_energy_profile(before, BANDS);
        let a = edge_energy_profile(after, BANDS);
        let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len().max(1) as f32;
        let (mb, ma) = (mean(&b), mean(&a));
        if b.is_empty() || a.is_empty() || mb <= f32::EPSILON || ma <= f32::EPSILON {
            return None;
        }
        Some(Self {
            before: b,
            after: a,
            gain: ma / mb,
        })
    }
}

/// Mean local contrast per column band, sampled in both axes.
///
/// Rows and columns are strided so the cost is bounded by the band count and
/// a row budget, not the photograph's megapixels: the profile is a shape, not
/// a census. The stride divides the sampling evenly, and both stages get the
/// same treatment, so the comparison between them stays fair.
fn edge_energy_profile(image: &Pixels, bands: usize) -> Vec<f32> {
    let width = image.width() as usize;
    let height = image.height() as usize;
    let px = image.pixels();
    let stride = width * 4;

    let mut sums = vec![0f64; bands];
    let mut counts = vec![0u32; bands];
    // ~64 sampled rows and ~256 sampled columns, whatever the resolution.
    let row_step = (height / 64).max(1);
    let col_step = ((width / 256).max(1)) * 4; // in bytes, RGBA
    let band_width = (width as f64 / bands as f64).max(1.0);

    let mut y = 1;
    while y + 1 < height {
        let row = y * stride;
        let up = row - stride;
        let down = row + stride;
        let mut offset = row + 4; // x = 1
        let mut x = 1;
        while x + 1 < width {
            // Band by relative position.
            let band = ((x as f64 / band_width) as usize).min(bands - 1);
            let xo = x * 4; // x's byte offset within any row
            // Horizontal neighbour difference (offset is x*4 within the row).
            let gx = (px[offset + 4] as f64 - px[offset - 4] as f64).abs()
                + (px[offset + 5] as f64 - px[offset - 3] as f64).abs()
                + (px[offset + 6] as f64 - px[offset - 2] as f64).abs();
            // Vertical neighbour difference.
            let gy = (px[down + xo] as f64 - px[up + xo] as f64).abs()
                + (px[down + xo + 1] as f64 - px[up + xo + 1] as f64).abs()
                + (px[down + xo + 2] as f64 - px[up + xo + 2] as f64).abs();
            sums[band] += gx + gy;
            counts[band] += 1;
            offset += col_step;
            x += col_step / 4;
        }
        y += row_step;
    }

    sums.iter()
        .zip(&counts)
        .map(|(sum, count)| {
            if *count == 0 {
                0.0
            } else {
                (sum / f64::from(*count)) as f32
            }
        })
        .collect()
}

/// What the chart draws with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartPalette {
    pub grid: Color,
    pub before: Color,
    pub after: Color,
    pub marker: Color,
}

impl ChartPalette {
    fn from_theme(theme_data: &ThemeData) -> Self {
        let primary = theme_data.colors.primary;
        Self {
            grid: theme_data.colors.outline,
            // "Before" is the primary washed toward the surface: present, but
            // clearly the stage being measured *against*.
            before: color::ramp(
                &[primary.lerp(theme_data.colors.surface, 0.62), primary],
                0.35,
            ),
            after: primary,
            marker: theme_data.colors.on_surface,
        }
    }
}

/// The chart: two profile curves, gridlines, the divider marker.
///
/// A `Painter` rather than a composition of boxes because the curves have no
/// widget spelling — the shapes are computed paths, and this is exactly what
/// the paint layer is for.
pub struct MeasureChart {
    measured: Rc<Measured>,
    palette: ChartPalette,
    /// The stage's divider position, 0..=1, drawn as a marker line.
    divider: f32,
}

impl std::fmt::Debug for MeasureChart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MeasureChart")
            .field("bands", &self.measured.before.len())
            .field("gain", &self.measured.gain)
            .field("divider", &self.divider)
            .finish()
    }
}

impl Painter for MeasureChart {
    fn paint(&self, book: &mut Sketchbook, size: Size) {
        let total_height = CHART_HEIGHT + PAD_TOP + PAD_BOTTOM;
        if size.width < PAD_LEFT + PAD_RIGHT + 16.0 || size.height < total_height {
            return;
        }

        // The plot area, as edges and plain numbers: every use below is a
        // coordinate, and spelling them once avoids re-deriving them at each
        // call site through `Rect`'s accessors.
        let left = PAD_LEFT;
        let top = PAD_TOP;
        let right = size.width - PAD_RIGHT;
        let bottom = PAD_TOP + CHART_HEIGHT;
        let plot_width = right - left;

        // The y scale over both profiles, niced so the gridlines land on
        // round numbers — a gridline at 0.73 reads as noise.
        let max = self
            .measured
            .before
            .iter()
            .chain(self.measured.after.iter())
            .copied()
            .fold(0.0f32, f32::max)
            .max(f32::EPSILON);
        let y = Continuous::linear(
            (0.0, f64::from(max)),
            (f64::from(bottom), f64::from(top)),
        )
        .nice(4);

        // Gridlines at the scale's own ticks: the interior ones, at most four.
        // Zero is the baseline, drawn separately and heavier.
        for tick in y.ticks(4).iter().skip(1).take(4) {
            let gy = y.map(*tick) as f32;
            book.line(
                Offset::new(left, gy),
                Offset::new(right, gy),
                self.palette.grid,
                1.0,
            );
        }

        // Baseline: the axis the profiles sit on.
        book.line(
            Offset::new(left, bottom),
            Offset::new(right, bottom),
            self.palette.grid,
            1.5,
        );

        // The two profiles, each mapped through the scale.
        let points = |profile: &[f32]| -> Vec<Offset> {
            profile
                .iter()
                .enumerate()
                .map(|(band, value)| {
                    let denom = (profile.len().max(2) - 1) as f32;
                    let t = band as f32 / denom;
                    Offset::new(left + plot_width * t, y.map(f64::from(*value)) as f32)
                })
                .collect()
        };

        // Before: thinner, dimmer — the reference.
        let before = shape::line(&points(&self.measured.before), Curve::MonotoneX);
        book.stroke(before, self.palette.before, 1.5);

        // After: the app's own work, in the primary, heavier.
        let after = shape::line(&points(&self.measured.after), Curve::MonotoneX);
        book.stroke(after, self.palette.after, 2.5);

        // The divider marker: where the stage is split, a hairline plus a
        // small triangle foot so it reads as a position, not a third curve.
        let mx = left + plot_width * self.divider.clamp(0.0, 1.0);
        book.line(
            Offset::new(mx, top),
            Offset::new(mx, bottom),
            self.palette.marker,
            1.0,
        );
        let mut foot = Path::new();
        foot.move_to(Offset::new(mx - 4.0, bottom));
        foot.line_to(Offset::new(mx + 4.0, bottom));
        foot.line_to(Offset::new(mx, bottom - 6.0));
        foot.close();
        book.fill(foot, self.palette.marker);
    }

    fn should_repaint(&self, previous: &dyn Painter) -> bool {
        match previous.as_any().downcast_ref::<Self>() {
            Some(previous) => {
                !Rc::ptr_eq(&self.measured, &previous.measured)
                    || self.palette != previous.palette
                    || (self.divider - previous.divider).abs() > 0.002
            }
            None => true,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// The whole strip: legend row, chart, and the position axis.
///
/// Returns `None` until both stages are decoded — a chart of one curve is a
/// chart that looks finished and is not.
pub fn measurements_row(
    theme_data: &ThemeData,
    before: &Option<Pixels>,
    after: &Option<Pixels>,
    divider: f32,
) -> Option<WidgetNode> {
    let (before, after) = (before.as_ref()?, after.as_ref()?);
    let measured = Rc::new(Measured::measure(before, after)?);
    let palette = ChartPalette::from_theme(theme_data);
    let gap = theme_data.metrics.gap;

    // The headline: the ratio of mean local contrast, as a percentage. It is
    // measured on this sheet's own two images, in this build — not a constant
    // that happens to look right. Like with like: the plain 4x control
    // against the sharpened result, both at the result's size.
    let gain_pct = (measured.gain - 1.0) * 100.0;
    let headline = if gain_pct >= 0.0 {
        format!("MEASURED · +{gain_pct:.0}% edge energy (plain 4x vs sharpened)")
    } else {
        format!("MEASURED · {gain_pct:.0}% edge energy (plain 4x vs sharpened)")
    };

    let legend = Flex::row()
        .spacing(gap)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .push(legend_swatch(palette.before, "plain 4x", theme_data))
        .push(legend_swatch(palette.after, "sharpened", theme_data));

    let title_row = Flex::row()
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .push(
            Flexible::expanded(1).child(
                Text::new(headline)
                    .style(theme_data.text.label)
                    .color(theme_data.colors.primary)
                    .size(11.0)
                    .bold(),
            ),
        )
        .push(legend);

    // The chart stretches to the sheet's width at a fixed height: the profile
    // is a property of the photograph, not of the space it is given.
    let chart = SizedBox::height(CHART_HEIGHT + PAD_TOP + PAD_BOTTOM).child(Painting::new(
        MeasureChart {
            measured: Rc::clone(&measured),
            palette,
            divider,
        },
    ));

    // The position axis: the one caption the chart needs — what x means.
    let axis = Text::new("left · centre · right")
        .style(theme_data.text.label)
        .color(theme_data.colors.on_surface_variant)
        .size(10.0);

    Some(
        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .spacing(4.0)
            .push(title_row)
            .push(chart)
            .push(axis)
            .into(),
    )
}

/// One "colour dot + label" legend entry.
fn legend_swatch(dot: Color, label: &str, theme_data: &ThemeData) -> WidgetNode {
    Flex::row()
        .spacing(5.0)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .push(SizedBox::square(8.0).child(ColoredBox::new(dot)))
        .push(
            Text::new(label.to_string())
                .style(theme_data.text.label)
                .color(theme_data.colors.on_surface_variant)
                .size(11.0),
        )
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small RGBA test image: a vertical light/dark stripe pattern.
    fn striped(width: u32, height: u32, contrast: u8, period: u32) -> Pixels {
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let on = (x / period.max(1)).is_multiple_of(2);
                let level = if on { 128 + contrast / 2 } else { 128 - contrast / 2 };
                pixels.extend([
                    level,
                    level,
                    level,
                    u8::from(y == 0 && x == 0),
                ]);
            }
        }
        Pixels::from_rgba8(pixels, width, height)
    }

    #[test]
    fn a_flat_image_has_no_edge_energy_and_measures_to_none() {
        let flat = striped(64, 64, 0, 8);
        let m = Measured::measure(&flat, &flat);
        assert!(m.is_none(), "a flat pair has no profile worth drawing");
    }

    #[test]
    fn a_sharper_image_measures_more_edge_energy_than_a_soft_one() {
        // Same size, same pattern; the sharper one has steeper edges. This is
        // the unsharp mask's effect in miniature: no new detail, steeper
        // transitions.
        let soft = striped(64, 64, 40, 8);
        let sharp = striped(64, 64, 160, 8);
        let m = Measured::measure(&soft, &sharp)
            .expect("both images have structure");
        assert!(
            m.gain > 2.0,
            "a 4x steeper edge must measure clearly more energy, got {}",
            m.gain
        );
    }

    #[test]
    fn the_profile_tracks_where_the_contrast_is() {
        // Contrast only on the left half: the profile's left-half mean must
        // exceed its right-half mean by a wide margin.
        let mut pixels = vec![128u8; 64 * 64 * 4];
        for y in 0..64 {
            for x in 0..32 {
                let on = x % 4 < 2;
                let level = if on { 200 } else { 56 };
                let i = ((y * 64 + x) * 4) as usize;
                pixels[i] = level;
                pixels[i + 1] = level;
                pixels[i + 2] = level;
                pixels[i + 3] = 255;
            }
        }
        let half_half = Pixels::from_rgba8(pixels, 64, 64);
        let profile = edge_energy_profile(&half_half, BANDS);
        let left: f32 = profile[..BANDS / 2].iter().sum();
        let right: f32 = profile[BANDS / 2..].iter().sum();
        assert!(
            left > right * 4.0,
            "left half mean {left} must dominate flat right half mean {right}"
        );
    }
}
