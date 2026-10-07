//! A grammar of graphics — Vega-Lite / Altair / Observable Plot's idea
//! (§1 Data-Driven Visualization, §Python Plotly/Altair): describe a chart
//! as **data + a mark + encodings**, and let the system choose scales,
//! axes and geometry.
//!
//! ```
//! use vieww_dataviz::spec::{Channel, Field, Mark, Spec, Value};
//!
//! let rows = vec![
//!     vec![("city", Value::from("A")), ("temp", Value::from(12.0))],
//!     vec![("city", Value::from("B")), ("temp", Value::from(19.0))],
//! ];
//! let chart = Spec::new(Mark::Bar)
//!     .encode(Channel::X, Field::nominal("city"))
//!     .encode(Channel::Y, Field::quantitative("temp"))
//!     .compile(&rows, 400.0, 300.0)
//!     .unwrap();
//! assert_eq!(chart.marks.len(), 2);
//! ```
//!
//! [`Spec::compile`] infers each encoding's scale from its field type
//! (quantitative → nice linear from zero for bars; nominal → band or
//! point; temporal → linear over time), maps colour to Tableau-10 (nominal)
//! or viridis (quantitative), size to a sqrt scale, and returns positioned
//! [`MarkGeom`]s plus axis ticks. Aggregations (`sum`, `mean`, `count`)
//! group rows by the nominal encodings first, as Vega-Lite's do.

use std::collections::BTreeMap;

use vieww_foundation::{Color, Offset, Path, Rect};

use crate::color::{viridis, TABLEAU10};
use crate::scale::{Band, Continuous};
use crate::shape::{line, Curve};

/// A cell value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Num(f64),
    Text(String),
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Self::Num(v)
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Self::Text(v.to_owned())
    }
}

impl Value {
    fn num(&self) -> Option<f64> {
        match self {
            Self::Num(v) => Some(*v),
            Self::Text(t) => t.parse().ok(),
        }
    }
    fn text(&self) -> String {
        match self {
            Self::Num(v) => format!("{v}"),
            Self::Text(t) => t.clone(),
        }
    }
}

/// A row: `(field name, value)` pairs.
pub type Row<'a> = Vec<(&'a str, Value)>;

/// The measurement type of a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Quantitative,
    Nominal,
    Ordinal,
    Temporal,
}

/// An aggregate over grouped rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aggregate {
    Sum,
    Mean,
    Count,
    Min,
    Max,
}

/// A field reference in an encoding.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub kind: Kind,
    pub aggregate: Option<Aggregate>,
}

impl Field {
    #[must_use]
    pub fn quantitative(name: &str) -> Self {
        Self { name: name.to_owned(), kind: Kind::Quantitative, aggregate: None }
    }
    #[must_use]
    pub fn nominal(name: &str) -> Self {
        Self { name: name.to_owned(), kind: Kind::Nominal, aggregate: None }
    }
    #[must_use]
    pub fn ordinal(name: &str) -> Self {
        Self { name: name.to_owned(), kind: Kind::Ordinal, aggregate: None }
    }
    #[must_use]
    pub fn temporal(name: &str) -> Self {
        Self { name: name.to_owned(), kind: Kind::Temporal, aggregate: None }
    }
    #[must_use]
    pub fn aggregate(mut self, a: Aggregate) -> Self {
        self.aggregate = Some(a);
        self
    }
}

/// Visual channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Channel {
    X,
    Y,
    Color,
    Size,
}

/// Mark types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Point,
    Bar,
    Line,
    Area,
    /// A heat-map cell (x and y both nominal/ordinal).
    Rect,
}

/// A chart specification.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    pub mark: Mark,
    pub encoding: BTreeMap<Channel, Field>,
}

/// One drawable.
#[derive(Debug, Clone, PartialEq)]
pub enum MarkGeom {
    Circle { center: Offset, radius: f32, color: Color },
    Rect { rect: Rect, color: Color },
    Path { path: Path, color: Color, filled: bool },
}

/// An axis: ticks as `(pixel position, label)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Axis {
    pub channel: Channel,
    pub title: String,
    pub ticks: Vec<(f32, String)>,
}

/// A compiled chart in pixel space.
#[derive(Debug, Clone, PartialEq)]
pub struct Chart {
    pub marks: Vec<MarkGeom>,
    pub axes: Vec<Axis>,
    /// Legend entries for a colour encoding.
    pub legend: Vec<(String, Color)>,
}

/// Why a spec did not compile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecError(pub String);

enum PosScale {
    Lin(Continuous),
    Band(Band),
}

impl PosScale {
    fn map(&self, v: &Value) -> f64 {
        match self {
            Self::Lin(s) => s.map(v.num().unwrap_or(0.0)),
            Self::Band(b) => b.map(&v.text()).unwrap_or(0.0) + b.bandwidth() * 0.5,
        }
    }
    fn width(&self) -> f64 {
        match self {
            Self::Lin(_) => 0.0,
            Self::Band(b) => b.bandwidth(),
        }
    }
}

impl Spec {
    #[must_use]
    pub fn new(mark: Mark) -> Self {
        Self { mark, encoding: BTreeMap::new() }
    }

    #[must_use]
    pub fn encode(mut self, c: Channel, f: Field) -> Self {
        self.encoding.insert(c, f);
        self
    }

    fn get<'r>(row: &'r Row<'_>, name: &str) -> Option<&'r Value> {
        row.iter().find(|(k, _)| *k == name).map(|(_, v)| v)
    }

    /// Group by nominal/ordinal encodings and aggregate the rest.
    fn aggregate(&self, rows: &[Row<'_>]) -> Vec<BTreeMap<String, Value>> {
        let any = self.encoding.values().any(|f| f.aggregate.is_some());
        let to_map = |r: &Row<'_>| -> BTreeMap<String, Value> {
            r.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect()
        };
        if !any {
            return rows.iter().map(to_map).collect();
        }
        let keys: Vec<&Field> = self
            .encoding
            .values()
            .filter(|f| f.aggregate.is_none() && matches!(f.kind, Kind::Nominal | Kind::Ordinal))
            .collect();
        let mut groups: BTreeMap<Vec<String>, Vec<&Row<'_>>> = BTreeMap::new();
        for r in rows {
            let k = keys.iter().map(|f| Self::get(r, &f.name).map(Value::text).unwrap_or_default()).collect();
            groups.entry(k).or_default().push(r);
        }
        groups
            .into_iter()
            .map(|(k, members)| {
                let mut m: BTreeMap<String, Value> = BTreeMap::new();
                for (f, v) in keys.iter().zip(k) {
                    m.insert(f.name.clone(), Value::Text(v));
                }
                for f in self.encoding.values().filter(|f| f.aggregate.is_some()) {
                    let vals: Vec<f64> = members.iter().filter_map(|r| Self::get(r, &f.name).and_then(Value::num)).collect();
                    #[allow(clippy::cast_precision_loss)]
                    let v = match f.aggregate.expect("filtered") {
                        Aggregate::Sum => vals.iter().sum(),
                        Aggregate::Mean => vals.iter().sum::<f64>() / vals.len().max(1) as f64,
                        Aggregate::Count => members.len() as f64,
                        Aggregate::Min => vals.iter().copied().fold(f64::INFINITY, f64::min),
                        Aggregate::Max => vals.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                    };
                    m.insert(f.name.clone(), Value::Num(v));
                }
                m
            })
            .collect()
    }

    fn pos_scale(&self, ch: Channel, data: &[BTreeMap<String, Value>], range: (f64, f64)) -> Result<(PosScale, Axis), SpecError> {
        let f = self.encoding.get(&ch).ok_or_else(|| SpecError(format!("no {ch:?} encoding")))?;
        let vals: Vec<&Value> = data.iter().filter_map(|r| r.get(&f.name)).collect();
        if vals.is_empty() {
            return Err(SpecError(format!("field '{}' not in the data", f.name)));
        }
        let title = match f.aggregate {
            Some(a) => format!("{a:?}({})", f.name).to_lowercase(),
            None => f.name.clone(),
        };
        Ok(match f.kind {
            Kind::Nominal | Kind::Ordinal => {
                let mut domain: Vec<String> = Vec::new();
                for v in &vals {
                    let t = v.text();
                    if !domain.contains(&t) {
                        domain.push(t);
                    }
                }
                if f.kind == Kind::Ordinal {
                    domain.sort();
                }
                let mut b = Band::new(domain.clone(), range);
                b.padding_inner = 0.15;
                b.padding_outer = 0.1;
                let ticks = domain
                    .iter()
                    .map(|d| {
                        #[allow(clippy::cast_possible_truncation)]
                        ((b.map(d).unwrap_or(0.0) + b.bandwidth() * 0.5) as f32, d.clone())
                    })
                    .collect();
                (PosScale::Band(b), Axis { channel: ch, title, ticks })
            }
            Kind::Quantitative | Kind::Temporal => {
                let nums: Vec<f64> = vals.iter().filter_map(|v| v.num()).collect();
                let (mut lo, hi) = nums.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, &v| (a.0.min(v), a.1.max(v)));
                if matches!(self.mark, Mark::Bar | Mark::Area) && f.kind == Kind::Quantitative {
                    lo = lo.min(0.0);
                }
                let hi = if hi <= lo { lo + 1.0 } else { hi };
                let s = Continuous::linear((lo, hi), range).nice(8);
                let step = s.ticks(8).windows(2).next().map_or(1.0, |w| w[1] - w[0]);
                let ticks = s
                    .ticks(8)
                    .into_iter()
                    .map(|t| {
                        #[allow(clippy::cast_possible_truncation)]
                        (s.map(t) as f32, crate::scale::format_tick(t, step))
                    })
                    .collect();
                (PosScale::Lin(s), Axis { channel: ch, title, ticks })
            }
        })
    }

    /// Compile against `rows` into a `width × height` plot area.
    ///
    /// # Errors
    /// A missing x/y encoding, or a field absent from the data.
    #[allow(clippy::too_many_lines)]
    pub fn compile(&self, rows: &[Row<'_>], width: f32, height: f32) -> Result<Chart, SpecError> {
        let data = self.aggregate(rows);
        let (xs, xa) = self.pos_scale(Channel::X, &data, (0.0, f64::from(width)))?;
        let (ys, ya) = self.pos_scale(Channel::Y, &data, (f64::from(height), 0.0))?;
        // Colour.
        let mut legend: Vec<(String, Color)> = Vec::new();
        let color_of: Box<dyn Fn(&BTreeMap<String, Value>) -> Color> = match self.encoding.get(&Channel::Color) {
            None => Box::new(|_| TABLEAU10[0]),
            Some(f) if matches!(f.kind, Kind::Nominal | Kind::Ordinal) => {
                let mut domain: Vec<String> = Vec::new();
                for r in &data {
                    if let Some(v) = r.get(&f.name) {
                        if !domain.contains(&v.text()) {
                            domain.push(v.text());
                        }
                    }
                }
                legend = domain.iter().enumerate().map(|(i, d)| (d.clone(), TABLEAU10[i % 10])).collect();
                let name = f.name.clone();
                let lg = legend.clone();
                Box::new(move |r| {
                    let t = r.get(&name).map(Value::text).unwrap_or_default();
                    lg.iter().find(|(k, _)| *k == t).map_or(TABLEAU10[0], |(_, c)| *c)
                })
            }
            Some(f) => {
                let nums: Vec<f64> = data.iter().filter_map(|r| r.get(&f.name).and_then(Value::num)).collect();
                let (lo, hi) = nums.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, &v| (a.0.min(v), a.1.max(v)));
                let s = Continuous::linear((lo, if hi > lo { hi } else { lo + 1.0 }), (0.0, 1.0));
                let name = f.name.clone();
                #[allow(clippy::cast_possible_truncation)]
                Box::new(move |r| viridis(s.map(r.get(&name).and_then(Value::num).unwrap_or(lo)) as f32))
            }
        };
        let size_of = |r: &BTreeMap<String, Value>| -> f32 {
            match self.encoding.get(&Channel::Size) {
                None => 4.0,
                Some(f) => {
                    let nums: Vec<f64> = data.iter().filter_map(|q| q.get(&f.name).and_then(Value::num)).collect();
                    let hi = nums.iter().copied().fold(0.0f64, f64::max).max(1e-9);
                    let s = Continuous::sqrt((0.0, hi), (2.0, 14.0));
                    #[allow(clippy::cast_possible_truncation)]
                    let v = s.map(r.get(&f.name).and_then(Value::num).unwrap_or(0.0)) as f32;
                    v
                }
            }
        };
        let xf = &self.encoding[&Channel::X].name;
        let yf = &self.encoding[&Channel::Y].name;
        let null = Value::Num(0.0);
        let px = |r: &BTreeMap<String, Value>| xs.map(r.get(xf).unwrap_or(&null));
        let py = |r: &BTreeMap<String, Value>| ys.map(r.get(yf).unwrap_or(&null));
        #[allow(clippy::cast_possible_truncation)]
        let f = |v: f64| v as f32;
        let mut marks = Vec::new();
        match self.mark {
            Mark::Point => {
                for r in &data {
                    marks.push(MarkGeom::Circle { center: Offset::new(f(px(r)), f(py(r))), radius: size_of(r), color: color_of(r) });
                }
            }
            Mark::Bar => {
                let base = match &ys {
                    PosScale::Lin(s) => s.map(0.0f64.clamp(s.domain.0.min(s.domain.1), s.domain.0.max(s.domain.1))),
                    PosScale::Band(..) => f64::from(height),
                };
                for r in &data {
                    let (x, y, w) = (px(r), py(r), xs.width().max(2.0));
                    marks.push(MarkGeom::Rect {
                        rect: Rect::new(f(x - w * 0.5), f(y.min(base)), f(x + w * 0.5), f(y.max(base))),
                        color: color_of(r),
                    });
                }
            }
            Mark::Rect => {
                let (w, h) = (xs.width().max(1.0), ys.width().max(1.0));
                for r in &data {
                    let (x, y) = (px(r), py(r));
                    marks.push(MarkGeom::Rect {
                        rect: Rect::new(f(x - w * 0.5), f(y - h * 0.5), f(x + w * 0.5), f(y + h * 0.5)),
                        color: color_of(r),
                    });
                }
            }
            Mark::Line | Mark::Area => {
                // One series per colour value.
                let mut series: BTreeMap<String, Vec<&BTreeMap<String, Value>>> = BTreeMap::new();
                let ckey = self.encoding.get(&Channel::Color).map(|f| f.name.clone());
                for r in &data {
                    let k = ckey.as_ref().and_then(|c| r.get(c)).map(Value::text).unwrap_or_default();
                    series.entry(k).or_default().push(r);
                }
                for (_, mut pts) in series {
                    pts.sort_by(|a, b| px(a).total_cmp(&px(b)));
                    let ps: Vec<Offset> = pts.iter().map(|r| Offset::new(f(px(r)), f(py(r)))).collect();
                    let color = color_of(pts[0]);
                    if self.mark == Mark::Line {
                        marks.push(MarkGeom::Path { path: line(&ps, Curve::MonotoneX), color, filled: false });
                    } else {
                        let base: Vec<Offset> = ps.iter().map(|p| Offset::new(p.dx, height)).collect();
                        marks.push(MarkGeom::Path { path: crate::shape::area(&ps, &base, Curve::MonotoneX), color, filled: true });
                    }
                }
            }
        }
        Ok(Chart { marks, axes: vec![xa, ya], legend })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<Row<'static>> {
        let mut v = Vec::new();
        for (i, (c, s)) in [("A", "x"), ("B", "x"), ("C", "y"), ("A", "y"), ("B", "y")].iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            v.push(vec![("cat", Value::from(*c)), ("series", Value::from(*s)), ("v", Value::from(i as f64 * 3.0 + 1.0)), ("t", Value::from(i as f64))]);
        }
        v
    }

    #[test]
    fn bars_from_zero_with_band_x() {
        let c = Spec::new(Mark::Bar)
            .encode(Channel::X, Field::nominal("cat"))
            .encode(Channel::Y, Field::quantitative("v").aggregate(Aggregate::Sum))
            .compile(&rows(), 300.0, 200.0)
            .unwrap();
        assert_eq!(c.marks.len(), 3, "grouped by category");
        for m in &c.marks {
            let MarkGeom::Rect { rect, .. } = m else { panic!() };
            assert!((rect.bottom - 200.0).abs() < 1e-3, "bars stand on zero");
        }
        assert_eq!(c.axes[0].ticks.len(), 3);
        assert!(c.axes[1].title.contains("sum"));
    }

    #[test]
    fn points_get_colour_legend_and_size() {
        let c = Spec::new(Mark::Point)
            .encode(Channel::X, Field::quantitative("t"))
            .encode(Channel::Y, Field::quantitative("v"))
            .encode(Channel::Color, Field::nominal("series"))
            .encode(Channel::Size, Field::quantitative("v"))
            .compile(&rows(), 300.0, 200.0)
            .unwrap();
        assert_eq!(c.marks.len(), 5);
        assert_eq!(c.legend.len(), 2);
        let r = |i: usize| match &c.marks[i] {
            MarkGeom::Circle { radius, .. } => *radius,
            _ => 0.0,
        };
        assert!(r(4) > r(0));
    }

    #[test]
    fn lines_split_by_colour_and_errors_name_fields() {
        let c = Spec::new(Mark::Line)
            .encode(Channel::X, Field::quantitative("t"))
            .encode(Channel::Y, Field::quantitative("v"))
            .encode(Channel::Color, Field::nominal("series"))
            .compile(&rows(), 300.0, 200.0)
            .unwrap();
        assert_eq!(c.marks.len(), 2);
        let e = Spec::new(Mark::Point)
            .encode(Channel::X, Field::quantitative("nope"))
            .encode(Channel::Y, Field::quantitative("v"))
            .compile(&rows(), 10.0, 10.0)
            .unwrap_err();
        assert!(e.0.contains("nope"));
    }
}
