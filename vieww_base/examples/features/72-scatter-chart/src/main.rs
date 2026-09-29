//! A scatter chart — the gap-closure pass's chart round-out, photographed.
//!
//! Study hours versus quiz score: points in the plane, each axis
//! normalised on its own range, dots inset so extremes keep their edges.
//! The point of a scatter (versus a line) is that the *cloud* is the
//! message, and a line through this data would claim a connection the
//! measurements do not.

use vieww_foundation::{Color, Size};
use vieww_widget::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("72 — scatter chart", Size::new(640.0, 300.0), |d| {
        let points = vec![
            (1.0, 42.0),
            (1.5, 45.0),
            (2.0, 51.0),
            (2.5, 48.0),
            (3.0, 58.0),
            (3.5, 61.0),
            (4.0, 57.0),
            (4.5, 66.0),
            (5.0, 70.0),
            (5.5, 68.0),
            (6.0, 74.0),
            (6.5, 71.0),
        ];
        feature_harness::set_page(
            d,
            Container::new()
                .color(Color::WHITE)
                .padding(EdgeInsets::all(20.0))
                .child(
                    ScatterChart::new(points)
                        .color(Color::rgb(58, 122, 246))
                        .dot_radius(4.0)
                        .axes(ChartAxes::Value)
                        .label("study hours versus quiz score")
                        .size(Size::new(560.0, 220.0)),
                ),
        );
    })
}
