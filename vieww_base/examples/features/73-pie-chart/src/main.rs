//! A pie chart — real arc wedges, not a stacked bar pretending.
//!
//! Share of session time by activity: vector-arc wedges through the path
//! API, a chunked text legend, whole-percent shares in the screen-reader
//! summary. The negative slice in the data is skipped silently, which is
//! the chart's own documented contract — this example shows the contract
//! working rather than hiding it.

use vieww_foundation::{Color, Size};
use vieww_widget::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("73 — pie chart", Size::new(640.0, 320.0), |d| {
        feature_harness::set_page(
            d,
            Container::new()
                .color(Color::WHITE)
                .padding(EdgeInsets::all(20.0))
                .child(
                    PieChart::new(vec![
                        ("Coding", 42.0),
                        ("Reviewing", 18.0),
                        ("Meetings", 12.0),
                        ("Reading", 9.0),
                        ("-abandoned sessions", -3.0), // skipped: the documented rule
                    ])
                    .label("share of session time")
                    .size(Size::new(280.0, 280.0)),
                ),
        );
    })
}
