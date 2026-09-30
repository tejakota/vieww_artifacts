//! A scatter chart: points as points, not as a line through them.

use vieww_foundation::{Color, Offset, Size, TextStyle};

use crate::prelude::*;
use crate::widgets::custom_paint::{CustomPaint, CustomPainter, DrawInstruction};
use crate::widgets::line_chart::{format_tick, range_of, ChartAxes};

/// A scatter chart: (x, y) pairs, drawn as dots on a normalised field.
///
/// The difference between this and a [`LineChart`](crate::LineChart) is the
/// difference between two questions: *what is the trend* (a line) and
/// *where are the things* (a scatter). A line chart with `show_dots` still
/// asserts a connection — the segments between the points are drawn — and
/// for stock versus rainfall, class height versus reading score, latency
/// versus request size, that assertion is a lie the chart tells with a
/// straight face. Points only, in the plane they were measured in.
///
/// # Examples
///
/// ```ignore
/// ScatterChart::new(vec![(10.0, 8.0), (25.0, 12.0), (15.0, 4.0)])
///     .color(Color::rgb(58, 122, 246))
///     .dot_radius(3.0)
///     .show_dots(true)
/// ```
#[derive(Debug)]
pub struct ScatterChart {
    /// The points, in data units.
    points: Vec<(f32, f32)>,
    /// `None` takes the theme's primary — see the line charts' module docs,
    /// which drew the colour rule this chart follows too.
    color: Option<Color>,
    dot_radius: f32,
    chart_size: Size,
    axes: ChartAxes,
    label: Option<String>,
}

impl ScatterChart {
    /// A chart from `(x, y)` pairs.
    #[must_use]
    pub fn new(points: Vec<(f32, f32)>) -> Self {
        Self {
            points,
            color: None,
            dot_radius: 3.0,
            chart_size: Size::new(240.0, 120.0),
            axes: ChartAxes::None,
            label: None,
        }
    }

    /// Set the dot colour, overriding the theme.
    #[must_use]
    pub const fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Set the dot radius in logical pixels.
    #[must_use]
    pub const fn dot_radius(mut self, radius: f32) -> Self {
        self.dot_radius = radius;
        self
    }

    /// Draw a baseline, gridlines and value labels. See [`ChartAxes`].
    #[must_use]
    pub const fn axes(mut self, axes: ChartAxes) -> Self {
        self.axes = axes;
        self
    }

    /// What this chart shows. The same contract as [`LineChart::label`]:
    /// the one thing a screen reader can be told.
    ///
    /// [`LineChart::label`]: crate::LineChart::label
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the chart's size.
    #[must_use]
    pub const fn size(mut self, size: Size) -> Self {
        self.chart_size = size;
        self
    }
}

impl Widget for ScatterChart {
    fn debug_name(&self) -> &'static str {
        "ScatterChart"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        let theme = ThemeData::of(ctx);
        let painter = ScatterChartPainter {
            points: self.points.clone(),
            color: self.color.unwrap_or(theme.colors.primary),
            dot_radius: self.dot_radius,
            grid: theme.colors.outline,
            axes: self.axes,
        };

        // The y-axis labels wrap the painted field, the same frame the line
        // and bar charts use — one layout, so three charts' axes agree.
        let described: WidgetNode = match self.label.as_deref() {
            Some(name) => {
                let summary = match scatter_range(&self.points) {
                    Some((min_y, max_y)) => format!(
                        "{name}, {} points, y {min} to {max}",
                        self.points.len(),
                        min = format_tick(min_y),
                        max = format_tick(max_y)
                    ),
                    None => format!("{name}, no data"),
                };
                Semantics::new()
                    .label(summary)
                    .child(CustomPaint::sized(self.chart_size, painter))
                    .into()
            }
            None => CustomPaint::sized(self.chart_size, painter).into(),
        };

        if self.axes == ChartAxes::None {
            return described;
        }
        let ys: Vec<f32> = self.points.iter().map(|&(_, y)| y).collect();
        let Some((min, max)) = range_of(&ys) else {
            return described;
        };
        let style = TextStyle {
            size: 10.0,
            color: theme.colors.on_surface_variant,
            ..theme.text.body
        };
        let mut labels = Flex::column()
            .main_axis_alignment(MainAxisAlignment::SpaceBetween)
            .cross_axis_alignment(CrossAxisAlignment::End);
        for step in 0..=4 {
            let t = 1.0 - step as f32 / 4.0;
            labels = labels.push(Text::new(format_tick(min + (max - min) * t)).style(style));
        }

        Flex::row()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .children(children![
                SizedBox::from_size(Size::new(0.0, self.chart_size.height)).child(labels),
                SizedBox::width(6.0),
                Flexible::expanded(1).child(described),
            ])
            .into()
    }
}

crate::widget_node_from!(ScatterChart);

/// The y range of a scatter's points, or `None` for no data.
fn scatter_range(points: &[(f32, f32)]) -> Option<(f32, f32)> {
    let min = points.iter().map(|&(_, y)| y).reduce(f32::min)?;
    let max = points.iter().map(|&(_, y)| y).reduce(f32::max)?;
    Some((min, max))
}

/// The painter that draws the scatter.
struct ScatterChartPainter {
    points: Vec<(f32, f32)>,
    color: Color,
    dot_radius: f32,
    grid: Color,
    axes: ChartAxes,
}

impl CustomPainter for ScatterChartPainter {
    fn paint(&self, size: Size) -> Vec<DrawInstruction> {
        if self.points.is_empty() {
            return Vec::new();
        }

        // Both axes normalise on their own ranges: the *shape* of the cloud
        // is the point of a scatter, and pinning either axis to a data range
        // the caller did not state would silently change that shape.
        let x_min = self.points.iter().map(|&(x, _)| x).reduce(f32::min).unwrap_or(0.0);
        let x_max = self.points.iter().map(|&(x, _)| x).reduce(f32::max).unwrap_or(1.0);
        let (y_min, y_max) = scatter_range(&self.points).unwrap_or((0.0, 1.0));
        let x_range = (x_max - x_min).max(f32::EPSILON);
        let y_range = (y_max - y_min).max(f32::EPSILON);

        // A half-dot inset so points on the extremes are not clipped by the
        // chart's own bounds — a dot missing its edge looks like a data
        // point missing its value.
        let inset = self.dot_radius;
        let usable = Size::new(
            (size.width - inset * 2.0).max(1.0),
            (size.height - inset * 2.0).max(1.0),
        );

        let mut instructions = Vec::new();
        if self.axes == ChartAxes::Value {
            // The same hairline grid the line chart draws, first, so the
            // dots sit on the reference rather than under it.
            for step in 0..=4 {
                #[expect(clippy::cast_precision_loss, reason = "four gridlines, small constants")]
                let y = size.height * (step as f32 / 4.0);
                let baseline = step == 4;
                instructions.push(DrawInstruction::DrawLine {
                    from: Offset::new(0.0, y),
                    to: Offset::new(size.width, y),
                    color: if baseline {
                        self.grid
                    } else {
                        self.grid.with_alpha(0x40)
                    },
                    width: if baseline { 1.0 } else { 0.5 },
                });
            }
        }

        for &(x, y) in &self.points {
            let px = inset + ((x - x_min) / x_range) * usable.width;
            let py = inset + (1.0 - (y - y_min) / y_range) * usable.height;
            instructions.push(DrawInstruction::FillCircle {
                center: Offset::new(px, py),
                radius: self.dot_radius,
                color: self.color,
            });
        }

        instructions
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn should_repaint(&self, previous: &dyn CustomPainter) -> bool {
        match previous.as_any().downcast_ref::<Self>() {
            Some(prev) => {
                self.points != prev.points
                    || self.color != prev.color
                    || self.dot_radius != prev.dot_radius
                    || self.axes != prev.axes
                    || self.grid != prev.grid
            }
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inflate;

    fn scatter() -> ScatterChart {
        ScatterChart::new(vec![(0.0, 0.0), (1.0, 1.0), (2.0, 0.5)])
    }

    #[test]
    fn a_scatter_builds_a_painted_field_with_semantics() {
        let node = inflate(scatter().label("latency vs size"));
        let rendered = format!("{node:?}");
        assert!(rendered.contains("Semantics"), "labelled: {rendered}");
        assert!(rendered.contains("CustomPaint"), "painted: {rendered}");
    }

    #[test]
    fn a_scatter_without_a_label_has_no_semantics_wrapper() {
        let node = inflate(scatter());
        let rendered = format!("{node:?}");
        assert!(!rendered.contains("Semantics"), "no label, no wrapper: {rendered}");
    }

    #[test]
    fn the_painter_places_points_in_the_plane() {
        let painter = ScatterChartPainter {
            points: vec![(0.0, 0.0), (1.0, 1.0), (2.0, 0.5)],
            color: Color::BLUE,
            dot_radius: 3.0,
            grid: Color::BLACK,
            axes: ChartAxes::None,
        };
        let size = Size::new(206.0, 114.0); // 240x120 minus the two 3-pt insets
        let instructions = painter.paint(size);

        assert_eq!(instructions.len(), 3, "one dot per point, no grid");
        let Some(DrawInstruction::FillCircle { center, radius, .. }) = instructions.first() else {
            panic!("a scatter paints circles");
        };
        assert_eq!(*radius, 3.0);
        // The first point (min x, min y) sits bottom-left, inside the inset.
        assert!((center.dx - 3.0).abs() < 1.0, "x at the left inset: {center:?}");
        assert!((center.dy - 111.0).abs() < 1.0, "y at the bottom: {center:?}");
    }

    #[test]
    fn an_empty_scatter_paints_nothing() {
        let painter = ScatterChartPainter {
            points: Vec::new(),
            color: Color::BLUE,
            dot_radius: 3.0,
            grid: Color::BLACK,
            axes: ChartAxes::Value,
        };
        assert!(painter.paint(Size::new(100.0, 100.0)).is_empty());
    }

    #[test]
    fn a_single_point_fills_the_field_without_dividing_by_zero() {
        let painter = ScatterChartPainter {
            points: vec![(5.0, 5.0)],
            color: Color::BLUE,
            dot_radius: 2.0,
            grid: Color::BLACK,
            axes: ChartAxes::None,
        };
        let instructions = painter.paint(Size::new(100.0, 100.0));
        assert_eq!(instructions.len(), 1, "degenerate ranges are handled, not divided by");
    }

    #[test]
    fn axes_add_the_value_labels_column() {
        let node = inflate(scatter().axes(ChartAxes::Value));
        let rendered = format!("{node:?}");
        assert!(rendered.contains("Flex"), "the axis frame wraps the field: {rendered}");
        assert!(rendered.contains("Text"), "with tick labels: {rendered}");
    }

    #[test]
    fn repaint_compares_what_drives_the_drawing() {
        let make = |points: Vec<(f32, f32)>| ScatterChartPainter {
            points,
            color: Color::BLUE,
            dot_radius: 3.0,
            grid: Color::BLACK,
            axes: ChartAxes::None,
        };
        let base = make(vec![(0.0, 0.0)]);
        let same = make(vec![(0.0, 0.0)]);
        let moved = make(vec![(1.0, 1.0)]);
        assert!(!base.should_repaint(&same));
        assert!(base.should_repaint(&moved));
    }
}
