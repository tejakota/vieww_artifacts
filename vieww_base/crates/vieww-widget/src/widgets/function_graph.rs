//! A function graph: `y = f(x)` drawn as a curve, with the axes a
//! mathematics reader expects — the Manim `FunctionGraph`/`Axes` capability
//! (and Matplotlib's `plot(xs, f(xs))`), which the comparison tables gave
//! to every scientific-plotting framework and to none of the UI ones.
//!
//! # Why a chart family member and not just a `Painting`
//!
//! A function graph is not a data chart. A [`LineChart`](crate::LineChart)
//! answers *"what did the measurements do"* — its axes are data ranges, its
//! gridlines are tick containers. This answers *"what does the function
//! do"* — its axes are the reference frame the curve lives in, and the
//! frame a mathematician reads is `x = 0` and `y = 0` crossing each other,
//! not a boxed plot area. Both belong beside each other in a dashboard, so
//! both follow the house chart conventions (theme colour, `label` for the
//! screen reader, the same tick formatting), but they draw different
//! things and say so.
//!
//! # The curve
//!
//! Sampled at a fixed count (160 points — enough that a smooth function
//! reads as smooth at any size this widget sensibly occupies, few enough
//! that a rebuild is not a re-integration) and drawn as line segments.
//! A discontinuity (`1/x`, `tan`) draws the near-vertical segments the
//! function actually produces across it, which is what a graphing
//! calculator shows too; the honest alternative — detecting and breaking
//! the line — is a heuristic, and a heuristic that guesses wrong about
//! `floor(x)` draws a staircase without its risers.

use std::rc::Rc;

use vieww_foundation::{Color, Offset, Size, TextStyle};

use crate::prelude::*;
use crate::widgets::custom_paint::{CustomPaint, CustomPainter, DrawInstruction};
use crate::widgets::line_chart::{format_tick, ChartAxes};

/// How many samples of the function one graph draws.
///
/// Published as a constant because it is the widget's resolution/price
/// trade-off, and a caller with a violently oscillating function deserves
/// to know which number to blame.
pub const SAMPLES: usize = 160;

/// A graph of `y = f(x)` over a domain, with origin axes.
///
/// # Examples
///
/// ```ignore
/// FunctionGraph::new(|x| x * x, (-2.0, 2.0))
///     .color(Color::rgb(58, 122, 246))
///     .axes(ChartAxes::Value)
///     .label("f(x) = x\u{b2}")
/// ```
#[derive(Clone)]
pub struct FunctionGraph {
    /// The function, in data units. `Rc` so the builder chains and the
    /// painter share it without a clone per rebuild.
    f: Rc<dyn Fn(f32) -> f32>,
    /// The x range to draw, in data units.
    domain: (f32, f32),
    /// `None` takes the theme's primary — the chart family's colour rule.
    color: Option<Color>,
    stroke: f32,
    graph_size: Size,
    axes: ChartAxes,
    label: Option<String>,
}

impl std::fmt::Debug for FunctionGraph {
    /// The function itself is unprintable (it is caller code, not data), so
    /// a debug dump names the type and the frame: the domain a tree dump
    /// needs to make sense of the painter's samples.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FunctionGraph")
            .field("domain", &self.domain)
            .field("color", &self.color)
            .field("stroke", &self.stroke)
            .field("graph_size", &self.graph_size)
            .field("axes", &self.axes)
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

impl FunctionGraph {
    /// A graph of `f` over `domain` (inclusive both ends).
    #[must_use]
    pub fn new(f: impl Fn(f32) -> f32 + 'static, domain: (f32, f32)) -> Self {
        Self {
            f: Rc::new(f),
            domain,
            color: None,
            stroke: 2.0,
            graph_size: Size::new(240.0, 140.0),
            axes: ChartAxes::None,
            label: None,
        }
    }

    /// Set the curve colour, overriding the theme.
    #[must_use]
    pub const fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Set the curve's stroke width in logical pixels.
    #[must_use]
    pub const fn stroke(mut self, stroke: f32) -> Self {
        self.stroke = stroke;
        self
    }

    /// Draw gridlines, tick labels and the origin axes. See [`ChartAxes`].
    #[must_use]
    pub const fn axes(mut self, axes: ChartAxes) -> Self {
        self.axes = axes;
        self
    }

    /// What this chart shows — the same screen-reader contract as
    /// [`LineChart::label`](crate::LineChart::label).
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the graph's size.
    #[must_use]
    pub const fn size(mut self, size: Size) -> Self {
        self.graph_size = size;
        self
    }
}

impl Widget for FunctionGraph {
    fn debug_name(&self) -> &'static str {
        "FunctionGraph"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        let theme = ThemeData::of(ctx);
        // The samples are taken once per build — the function is caller
        // code, and running it in the painter's `should_repaint` would run
        // it twice per comparison and four times per frame.
        let samples = sample_function(self.f.as_ref(), self.domain, SAMPLES);
        let y_range = range_of_samples(&samples).unwrap_or((-1.0, 1.0));
        let painter = FunctionGraphPainter {
            samples,
            y_range,
            color: self.color.unwrap_or(theme.colors.primary),
            stroke: self.stroke,
            grid: theme.colors.outline,
            axes: self.axes,
            domain: self.domain,
        };

        let described: WidgetNode = match self.label.as_deref() {
            Some(name) => {
                let summary = format!(
                    "{name}, x {min} to {max}",
                    min = format_tick(self.domain.0),
                    max = format_tick(self.domain.1)
                );
                Semantics::new()
                    .label(summary)
                    .child(CustomPaint::sized(self.graph_size, painter))
                    .into()
            }
            None => CustomPaint::sized(self.graph_size, painter).into(),
        };

        if self.axes == ChartAxes::None {
            return described;
        }
        // The y-axis labels wrap the painted field, the same frame the
        // other charts use.
        let (min, max) = y_range;
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
                SizedBox::from_size(Size::new(0.0, self.graph_size.height)).child(labels),
                SizedBox::width(6.0),
                Flexible::expanded(1).child(described),
            ])
            .into()
    }
}

crate::widget_node_from!(FunctionGraph);

/// Evaluate `f` at `count` evenly spaced points across `domain`.
fn sample_function(f: &dyn Fn(f32) -> f32, domain: (f32, f32), count: usize) -> Vec<(f32, f32)> {
    let span = domain.1 - domain.0;
    if count < 2 || span.abs() < f32::EPSILON {
        return vec![(domain.0, f(domain.0))];
    }
    (0..count)
        .map(|i| {
            let t = i as f32 / (count - 1) as f32;
            let x = domain.0 + span * t;
            let y = f(x);
            // NaN poisons every comparison downstream (a min of NaN is
            // NaN, and the graph disappears); a function that returns NaN
            // at a point is a function that is undefined there, and
            // undefined draws as a gap — 0.0 keeps the frame and drops
            // the segment, which the segment filter below handles.
            (x, if y.is_finite() { y } else { f32::NAN })
        })
        .collect()
}

/// The y range of the samples, ignoring NaNs; `None` if there is nothing
/// finite to show.
fn range_of_samples(samples: &[(f32, f32)]) -> Option<(f32, f32)> {
    let finite = samples.iter().filter(|&(_, y)| y.is_finite());
    let min = finite.clone().map(|&(_, y)| y).reduce(f32::min)?;
    let max = finite.map(|&(_, y)| y).reduce(f32::max)?;
    Some((min, max))
}

/// The painter that draws the curve and the frame.
struct FunctionGraphPainter {
    samples: Vec<(f32, f32)>,
    y_range: (f32, f32),
    color: Color,
    stroke: f32,
    grid: Color,
    axes: ChartAxes,
    domain: (f32, f32),
}

impl CustomPainter for FunctionGraphPainter {
    fn paint(&self, size: Size) -> Vec<DrawInstruction> {
        if self.samples.len() < 2 {
            return Vec::new();
        }
        let (y_min, y_max) = self.y_range;
        let y_span = (y_max - y_min).max(f32::EPSILON);
        let x_span = (self.domain.1 - self.domain.0).max(f32::EPSILON);

        let to_px = |x: f32| ((x - self.domain.0) / x_span) * size.width;
        let to_py = |y: f32| (1.0 - (y - y_min) / y_span) * size.height;

        let mut instructions = Vec::new();

        if self.axes == ChartAxes::Value {
            // The data-chart gridlines, then the two origin axes on top of
            // them: a mathematician's frame and a chart's ticks, both
            // visible, neither pretending to be the other.
            for step in 0..=4 {
                #[expect(clippy::cast_precision_loss, reason = "four gridlines, small constants")]
                let y = size.height * (step as f32 / 4.0);
                instructions.push(DrawInstruction::DrawLine {
                    from: Offset::new(0.0, y),
                    to: Offset::new(size.width, y),
                    color: self.grid.with_alpha(0x40),
                    width: 0.5,
                });
            }
            // x = 0, if the domain crosses zero.
            if self.domain.0 <= 0.0 && self.domain.1 >= 0.0 {
                let x = to_px(0.0);
                instructions.push(DrawInstruction::DrawLine {
                    from: Offset::new(x, 0.0),
                    to: Offset::new(x, size.height),
                    color: self.grid,
                    width: 1.0,
                });
            }
            // y = 0, if the sampled range crosses zero.
            if y_min <= 0.0 && y_max >= 0.0 {
                let y = to_py(0.0);
                instructions.push(DrawInstruction::DrawLine {
                    from: Offset::new(0.0, y),
                    to: Offset::new(size.width, y),
                    color: self.grid,
                    width: 1.0,
                });
            }
        }

        // The curve: consecutive finite samples joined by segments. A NaN
        // between two points breaks the line rather than drawing a segment
        // to a point that does not exist — a graph of 1/x has no bridge at
        // zero, and drawing one is a claim about the function that is false.
        let mut previous: Option<(f32, f32)> = None;
        for &(x, y) in &self.samples {
            if y.is_finite() {
                if let Some((px, py)) = previous {
                    instructions.push(DrawInstruction::DrawLine {
                        from: Offset::new(to_px(px), to_py(py)),
                        to: Offset::new(to_px(x), to_py(y)),
                        color: self.color,
                        width: self.stroke,
                    });
                }
                previous = Some((x, y));
            } else {
                previous = None;
            }
        }

        instructions
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn should_repaint(&self, previous: &dyn CustomPainter) -> bool {
        match previous.as_any().downcast_ref::<Self>() {
            Some(prev) => {
                self.samples != prev.samples
                    || self.y_range != prev.y_range
                    || self.color != prev.color
                    || self.stroke != prev.stroke
                    || self.axes != prev.axes
                    || self.grid != prev.grid
                    || self.domain != prev.domain
            }
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_cover_the_domain_inclusively() {
        let samples = sample_function(&|x| x, (-2.0, 2.0), 5);
        let xs: Vec<f32> = samples.iter().map(|&(x, _)| x).collect();
        assert_eq!(xs, vec![-2.0, -1.0, 0.0, 1.0, 2.0]);
    }

    #[test]
    fn sample_count_is_respected() {
        assert_eq!(sample_function(&|x| x, (0.0, 1.0), SAMPLES).len(), SAMPLES);
    }

    #[test]
    fn the_range_covers_the_function() {
        let samples = sample_function(&|x| x * x, (-3.0, 3.0), 33);
        let (min, max) = range_of_samples(&samples).expect("finite samples");
        assert!((0.0 - 1e-3..=0.0 + 1e-3).contains(&min), "f(0) = 0 is the min: {min}");
        assert!((max - 9.0).abs() < 1e-3, "f(±3) = 9 is the max: {max}");
    }

    #[test]
    fn non_finite_samples_are_kept_as_gaps_not_zeros() {
        // The painter breaks the line at a NaN; the sampling must therefore
        // *emit* the NaN rather than quietly substituting a value, and this
        // test is the contract between the two halves.
        let samples = sample_function(&|x| 1.0 / x, (-1.0, 1.0), 41);
        assert!(samples.iter().any(|&(_, y)| !y.is_finite()), "1/x has a pole");
        // And the range survives it.
        assert!(range_of_samples(&samples).is_some());
        let (min, max) = range_of_samples(&samples).expect("range");
        assert!(min < -10.0 && max > 10.0, "the pole dominates the range: {min}..{max}");
    }

    #[test]
    fn a_degenerate_domain_draws_one_point() {
        let samples = sample_function(&|x| x + 1.0, (5.0, 5.0), 100);
        assert_eq!(samples, vec![(5.0, 6.0)]);
    }

    #[test]
    fn one_sample_has_no_range() {
        assert_eq!(range_of_samples(&[(1.0, f32::NAN)]), None);
    }
}
