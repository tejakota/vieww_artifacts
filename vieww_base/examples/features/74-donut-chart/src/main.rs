//! A donut chart — a pie with a hole, and a centre label that says what
//! the whole is.
//!
//! Storage by category, with the total in the middle: the same arc-wedge
//! machinery as the pie, plus the hole and the overlay the type adds.
//! The centre label is the donut's whole point over a pie — the total is
//! readable without a second element.

use vieww_foundation::{Color, Size};
use vieww_widget::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("74 — donut chart", Size::new(640.0, 340.0), |d| {
        feature_harness::set_page(
            d,
            Container::new()
                .color(Color::WHITE)
                .padding(EdgeInsets::all(20.0))
                .child(
                    DonutChart::new(vec![
                        ("Media", 118.0),
                        ("Documents", 46.0),
                        ("Backups", 92.0),
                        ("System", 31.0),
                    ])
                    .label("storage by category, GiB")
                    .size(Size::new(300.0, 300.0)),
                ),
        );
    })
}
