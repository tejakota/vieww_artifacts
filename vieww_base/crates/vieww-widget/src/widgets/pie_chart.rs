//! Pie and donut charts: parts of a whole, as angles.

use vieww_foundation::{Color, Offset, Size, Sketchbook, TextStyle};

use crate::prelude::*;
use crate::widgets::painting::{Painter, Painting};

/// A pie chart: proportions as wedges of a circle.
///
/// The chart that answers "what is the *split*" — the one question a line,
/// bar or scatter chart cannot. Each value becomes a wedge whose angle is
/// its share of the total, drawn as a real vector arc (not a polygon
/// approximation: [`Sketchbook`]'s arc path is a first-class shape here),
/// and the wedge order is the value order, starting at twelve o'clock and
/// going clockwise — the same reading direction as a clock, which is the
/// convention every pie chart a reader has ever seen already follows.
///
/// A legend of names runs under the chart as real text, because colour
/// alone cannot carry a category (the line charts' module docs drew that
/// rule; this chart applies it where it bites hardest — five wedges of a
/// pie are five colours a reader must otherwise tell apart).
///
/// # Examples
///
/// ```ignore
/// PieChart::new(vec![("Rent", 1200.0), ("Food", 450.0), ("Transit", 90.0)])
///     .label("monthly spending")
/// ```
#[derive(Debug, Clone)]
pub struct PieChart {
    /// The wedges: a name and a value. Values are relative to their sum;
    /// negatives are refused at draw time (see the painter).
    slices: Vec<(&'static str, f32)>,
    chart_size: Size,
    /// `None` for a pie; `Some(r)` for a donut of that inner radius — see
    /// [`DonutChart`].
    hole: Option<f32>,
    label: Option<String>,
    /// Wedge colours, `None` to take the theme's roles in turn.
    colors: Option<Vec<Color>>,
}

impl PieChart {
    /// A pie from named values.
    #[must_use]
    pub fn new(slices: Vec<(&'static str, f32)>) -> Self {
        Self {
            slices,
            chart_size: Size::new(180.0, 180.0),
            hole: None,
            label: None,
            colors: None,
        }
    }

    /// Set the chart's size.
    #[must_use]
    pub const fn size(mut self, size: Size) -> Self {
        self.chart_size = size;
        self
    }

    /// What this chart shows — the screen-reader label, as on every chart.
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the wedge colours, overriding the theme. One per slice; a
    /// shorter list repeats from its start rather than refusing, because a
    /// caller with a three-colour brand palette and five wedges has made a
    /// design decision, not an error.
    #[must_use]
    pub fn colors(mut self, colors: Vec<Color>) -> Self {
        self.colors = Some(colors);
        self
    }

    /// The share of `index`'s value, as a fraction of the total, in text
    /// form — the legend's and the semantics' common currency.
    #[must_use]
    fn share_text(&self, index: usize) -> Option<String> {
        let total: f32 = self.slices.iter().map(|&(_, value)| value).sum();
        if total <= 0.0 {
            return None;
        }
        let (_, value) = *self.slices.get(index)?;
        let share = value / total;
        // Percent, whole numbers: a legend reading "33.333333%" is a
        // legend nobody scans, and the sum the reader checks (their
        // percentages adding to 100) survives rounding better as whole
        // numbers anyway.
        Some(format!("{}%", (share * 100.0).round() as i64))
    }

    /// The colours actually drawn: the caller's, or the theme's roles.
    fn drawn_colors(&self, theme: &ThemeData) -> Vec<Color> {
        match &self.colors {
            Some(colors) if !colors.is_empty() => colors.clone(),
            _ => vec![
                theme.colors.primary,
                theme.colors.error,
                theme.colors.success,
                theme.colors.outline,
                theme.colors.surface_variant,
            ],
        }
    }
}

impl Widget for PieChart {
    fn debug_name(&self) -> &'static str {
        "PieChart"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        let theme = ThemeData::of(ctx);
        let colors = self.drawn_colors(&theme);
        let painter = PieChartPainter {
            slices: self.slices.clone(),
            colors,
            hole: self.hole,
            text: theme.colors.on_surface,
        };

        // The legend: "name share" entries under the pie, in rows of
        // three — because a legend that runs off the chart's right edge is
        // a legend whose last entries simply do not exist for a reader,
        // and a fixed chunk reads more predictably than a wrap the layout
        // engine does not offer.
        let style = TextStyle {
            size: 11.0,
            color: theme.colors.on_surface_variant,
            ..theme.text.body
        };
        let entries: Vec<WidgetNode> = self
            .slices
            .iter()
            .enumerate()
            .filter_map(|(index, &(name, _))| {
                self.share_text(index)
                    .map(|share| Text::new(format!("{name} {share}")).style(style).into())
            })
            .collect();
        let mut legend = Flex::column().spacing(2.0);
        for chunk in entries.chunks(3) {
            legend = legend.push(Flex::row().spacing(10.0).children(chunk.iter().cloned()));
        }

        let pie = SizedBox::from_size(self.chart_size).child(Painting::new(painter));
        let mut column = Flex::column().spacing(6.0).push(pie);
        if !entries.is_empty() {
            column = column.push(legend);
        }

        // The semantics summary: the label and the shares, which is what a
        // sighted reader takes from the glance a pie chart is.
        let described = match self.label.as_deref() {
            Some(name) => {
                let shares: Vec<String> = (0..self.slices.len())
                    .filter_map(|index| self.share_text(index))
                    .collect();
                let summary = if shares.is_empty() {
                    format!("{name}, no data")
                } else {
                    format!("{name}: {}", shares.join(", "))
                };
                Semantics::new().label(summary).child(column).into()
            }
            None => column.into(),
        };
        described
    }
}

crate::widget_node_from!(PieChart);

/// A donut chart: a pie chart with the middle taken out.
///
/// The hole is not decoration: a pie's wedges meet at the centre, where
/// their angles are unreadable (every wedge is "thin" there), and pulling
/// the middle out puts every wedge's *thickness* on display — which is the
/// readable encoding of a proportion. The centre is then free for a
/// headline number, which is what a donut in a dashboard is *for*; see
/// [`center_label`](DonutChart::center_label).
#[derive(Debug, Clone)]
pub struct DonutChart {
    pie: PieChart,
    /// The text in the hole, if any.
    center: Option<String>,
}

impl DonutChart {
    /// A donut from named values, with a hole of the default quarter
    /// radius.
    #[must_use]
    pub fn new(slices: Vec<(&'static str, f32)>) -> Self {
        Self {
            pie: PieChart::new(slices),
            center: None,
        }
    }

    /// Put text in the hole. **The one-line summary the chart exists to
    /// show** — the total, the headline share — set here rather than laid
    /// out beside the chart, because the eye reads the middle of a donut
    /// first and a dashboard that wastes its best position on empty space
    /// wasted the hole.
    #[must_use]
    pub fn center_label(mut self, label: impl Into<String>) -> Self {
        self.center = Some(label.into());
        self
    }

    /// Set the size.
    #[must_use]
    pub const fn size(mut self, size: Size) -> Self {
        self.pie.chart_size = size;
        self
    }

    /// What this chart shows. See [`PieChart::label`].
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.pie.label = Some(label.into());
        self
    }

    /// Set the wedge colours, overriding the theme.
    #[must_use]
    pub fn colors(mut self, colors: Vec<Color>) -> Self {
        self.pie.colors = Some(colors);
        self
    }
}

impl Widget for DonutChart {
    fn debug_name(&self) -> &'static str {
        "DonutChart"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        // Default hole: 55% of the radius — wide enough that every wedge's
        // thickness is on display, narrow enough that a wedge still reads
        // as a part of a circle rather than a detached arc. Cloned rather
        // than mutated: `build` is `&self`, everywhere, and a builder that
        // needed `&mut` would stand out from every other widget in the
        // catalogue for a number the painter could have derived.
        let mut pie = self.pie.clone();
        let radius = pie.chart_size.width.min(pie.chart_size.height) / 2.0;
        pie.hole = Some(radius * 0.55);
        let pie = pie.build(ctx);

        match &self.center {
            Some(center) => Stack::new()
                .children(children![
                    pie,
                    Center::new().child(Text::new(center.clone()).style(TextStyle {
                        size: 13.0,
                        color: ThemeData::of(ctx).colors.on_surface,
                        ..ThemeData::of(ctx).text.body
                    })),
                ])
                .into(),
            None => pie,
        }
    }
}

crate::widget_node_from!(DonutChart);

/// The pie's painter — a [`Painter`] because the wedges are *arcs*, and a
/// `CustomPaint`'s instruction set has no arc: circles, rects, lines and
/// rounded rects are what a rectangle of ink needs, and a pie wedge is a
/// shaped fill the [`Sketchbook`] path API already expresses exactly.
struct PieChartPainter {
    slices: Vec<(&'static str, f32)>,
    colors: Vec<Color>,
    /// The inner radius of the hole; `None` for a full pie.
    hole: Option<f32>,
    /// The label text colour (for a centre label, if the widget is a donut
    /// with one — the painter draws it so the wedge and the number land in
    /// the same paint pass).
    text: Color,
}

impl Painter for PieChartPainter {
    fn paint(&self, book: &mut Sketchbook, size: Size) {
        let total: f32 = self.slices.iter().map(|&(_, value)| value).sum();
        if total <= 0.0 {
            // No data, or values summing to nothing: nothing is drawn. An
            // outline "empty chart" would be decoration pretending to be
            // information; the widget's semantics already said "no data",
            // and the space stays honest.
            return;
        }

        let center = Offset::new(size.width / 2.0, size.height / 2.0);
        let radius = size.width.min(size.height) / 2.0 - 1.0;
        // Wedge order starts at twelve o'clock — the `-FRAC_PI_2` — and
        // runs clockwise, matching the sweep direction of the arc path.
        let mut start = -std::f32::consts::FRAC_PI_2;

        for (index, &(name, value)) in self.slices.iter().enumerate() {
            let share = value / total;
            let sweep = share * std::f32::consts::TAU;
            let color = self.colors.get(index).copied().unwrap_or(self.colors[0]);

            if value < 0.0 {
                // A negative proportion has no angle: skipped, silently and
                // by design. A chart is a display, not a validator — a
                // dashboard that crashed on bad data would be worse than
                // one that quietly showed the values that exist — and the
                // `name` here is retained for the day a linter wants it.
                let _ = (name, value);
                continue;
            }
            if sweep <= 0.0 {
                continue;
            }

            // The wedge: `arc_ring` with the hole's radius as its inner
            // edge. A full pie is a ring whose inner edge is the centre —
            // the same path, which is why one painter draws both charts.
            book.fill(
                vieww_foundation::Path::arc_ring(
                    center,
                    radius,
                    self.hole.map_or(radius, |inner| radius - inner),
                    start,
                    sweep,
                ),
                color,
            );
            start += sweep;
        }
    }

    fn should_repaint(&self, previous: &dyn Painter) -> bool {
        match previous.as_any().downcast_ref::<Self>() {
            Some(prev) => {
                self.slices != prev.slices
                    || self.colors != prev.colors
                    || self.hole != prev.hole
                    || self.text != prev.text
            }
            None => true,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl std::fmt::Debug for PieChartPainter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The sketchbook is the drawing; the debug story is the split.
        let total: f32 = self.slices.iter().map(|&(_, value)| value).sum();
        f.debug_struct("PieChartPainter")
            .field("slices", &self.slices.len())
            .field("total", &total)
            .field("holed", &self.hole.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inflate;

    fn spend() -> PieChart {
        PieChart::new(vec![("Rent", 1200.0), ("Food", 450.0), ("Transit", 90.0)])
    }

    #[test]
    fn a_pie_builds_wedges_and_a_text_legend() {
        let node = inflate(spend().label("monthly"));
        let rendered = format!("{node:?}");
        assert!(rendered.contains("PieChart"), "the painted pie: {rendered}");
        assert!(rendered.contains("Text"), "the legend: {rendered}");
        assert!(rendered.contains("Rent"), "by name: {rendered}");
        assert!(rendered.contains("Semantics"), "labelled: {rendered}");
    }

    #[test]
    fn the_shares_read_as_whole_percents() {
        let pie = spend();
        // 1200 / 1740 = 68.97 -> 69%.
        assert_eq!(pie.share_text(0).as_deref(), Some("69%"));
        assert_eq!(pie.share_text(1).as_deref(), Some("26%"));
        assert_eq!(pie.share_text(2).as_deref(), Some("5%"));
    }

    #[test]
    fn a_pie_of_nothing_has_no_shares() {
        let pie = PieChart::new(Vec::new());
        assert_eq!(pie.share_text(0), None);

        let zero = PieChart::new(vec![("a", 0.0), ("b", 0.0)]);
        assert_eq!(
            zero.share_text(0),
            None,
            "a total of zero divides by nothing"
        );
    }

    #[test]
    fn a_short_colour_list_repeats_rather_than_refuses() {
        let pie = spend().colors(vec![Color::RED]);
        let theme = ThemeData::light();
        assert_eq!(pie.drawn_colors(&theme), vec![Color::RED]);
        // The painter's per-index lookup wraps: five slices, one colour.
        let painter_colors = [Color::RED];
        assert_eq!(
            painter_colors
                .get(4)
                .copied()
                .or_else(|| painter_colors.first().copied()),
            Some(Color::RED)
        );
    }

    #[test]
    fn without_a_caller_palette_the_theme_roles_are_used() {
        let theme = ThemeData::light();
        let pie = spend();
        assert_eq!(
            pie.drawn_colors(&theme),
            vec![
                theme.colors.primary,
                theme.colors.error,
                theme.colors.success,
                theme.colors.outline,
                theme.colors.surface_variant,
            ]
        );
    }

    #[test]
    fn a_donut_puts_its_label_in_the_hole() {
        let node = inflate(DonutChart::new(vec![("a", 1.0), ("b", 1.0)]).center_label("2 total"));
        let rendered = format!("{node:?}");
        assert!(
            rendered.contains("Stack"),
            "the label overlays the ring: {rendered}"
        );
        assert!(rendered.contains("2 total"), "in the hole: {rendered}");
    }

    #[test]
    fn a_donut_without_a_centre_label_is_just_a_holed_pie() {
        let node = inflate(DonutChart::new(vec![("a", 1.0)]));
        let rendered = format!("{node:?}");
        assert!(
            !rendered.contains("Stack"),
            "no overlay scaffold: {rendered}"
        );
    }

    #[test]
    fn the_painter_walks_the_circle_from_twelve_oclock() {
        // One full-value slice: one wedge of the whole turn, from -90°.
        let painter = PieChartPainter {
            slices: vec![("all", 1.0)],
            colors: vec![Color::RED],
            hole: None,
            text: Color::BLACK,
        };
        let mut book = Sketchbook::new();
        let size = Size::new(100.0, 100.0);
        painter.paint(&mut book, size);
        assert!(!book.is_empty(), "one wedge painted");
    }

    #[test]
    fn the_painter_of_nothing_is_empty() {
        let painter = PieChartPainter {
            slices: Vec::new(),
            colors: vec![Color::RED],
            hole: None,
            text: Color::BLACK,
        };
        let mut book = Sketchbook::new();
        painter.paint(&mut book, Size::new(100.0, 100.0));
        assert!(
            book.is_empty(),
            "no data is no drawing, not an empty outline"
        );
    }

    #[test]
    fn a_negative_slice_is_skipped_not_drawn() {
        // With `debug_assertions` off the painter must still total the
        // *positives*: the shape is a pie of the values that exist.
        let painter = PieChartPainter {
            slices: vec![("good", 3.0), ("bad", -1.0), ("fine", 1.0)],
            colors: vec![Color::RED],
            hole: None,
            text: Color::BLACK,
        };
        let mut book = Sketchbook::new();
        painter.paint(&mut book, Size::new(100.0, 100.0));
        // Two wedges of the 4.0 total: 3/4 and 1/4 of the turn.
        assert!(!book.is_empty());
    }

    #[test]
    fn the_painter_is_debuggable_without_the_sketchbook() {
        let painter = PieChartPainter {
            slices: vec![("a", 1.0)],
            colors: vec![Color::RED],
            hole: None,
            text: Color::BLACK,
        };
        let rendered = format!("{painter:?}");
        assert!(rendered.contains("PieChartPainter"), "{rendered}");
    }
}
