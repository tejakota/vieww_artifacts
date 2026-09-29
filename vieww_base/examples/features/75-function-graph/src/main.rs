//! Two function graphs — the `FunctionGraph` widget, photographed.
//!
//! `sin(x)` over two turns with the origin axes crossing at zero, and
//! `x²/4 − 2` for a parabola on the same frame: the mathematics reading
//! of a graph (axes through the origin) beside the chart family's
//! gridlines and tick labels, both drawn by the same widget.

use vieww_foundation::{Color, Size};
use vieww_widget::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("75 — function graph", Size::new(640.0, 420.0), |d| {
        feature_harness::set_page(
            d,
            Container::new()
                .color(Color::WHITE)
                .padding(EdgeInsets::all(20.0))
                .child(
                    Flex::column()
                        .spacing(18.0)
                        .children(children![
                            FunctionGraph::new(|x| x.sin(), (-2.0 * std::f32::consts::PI, 2.0 * std::f32::consts::PI))
                                .color(Color::rgb(58, 122, 246))
                                .axes(ChartAxes::Value)
                                .label("f(x) = sin(x)")
                                .size(Size::new(560.0, 160.0)),
                            FunctionGraph::new(|x| x * x / 4.0 - 2.0, (-4.0, 4.0))
                                .color(Color::rgb(46, 145, 80))
                                .axes(ChartAxes::Value)
                                .label("g(x) = x²/4 − 2")
                                .size(Size::new(560.0, 160.0)),
                        ]),
                ),
        );
    })
}
