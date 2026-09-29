//! Web content, embedded — the gap-closure pass's `vieww-embed` crate,
//! photographed.
//!
//! Two `WebView`s: one by URL, one by inline `srcdoc` HTML. On the web
//! backend each of these becomes a real `<iframe>` (lazy, titled,
//! accessible); on every other backend — including this headless shot —
//! each renders the themed placeholder that says exactly what it is and
//! what would carry it. The honest placeholder *is* the design: a box that
//! pretends to be live content when it is not would be the bug.

use vieww_embed::{WebContent, WebView};
use vieww_foundation::{Color, Size};
use vieww_widget::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("71 — embed", Size::new(620.0, 360.0), |d| {
        feature_harness::set_page(
            d,
            Container::new()
                .color(Color::WHITE)
                .padding(EdgeInsets::all(20.0))
                .child(
                    Flex::column()
                        .spacing(16.0)
                        .children(children![
                            Text::new("WebView by URL").style(TextStyle {
                                size: 13.0,
                                color: Color::rgb(90, 100, 120),
                                ..TextStyle::default()
                            }),
                            WebView::new(WebContent::url("https://vieww.dev")),
                            Text::new("WebView by inline HTML (srcdoc)").style(TextStyle {
                                size: 13.0,
                                color: Color::rgb(90, 100, 120),
                                ..TextStyle::default()
                            }),
                            WebView::new(WebContent::html(
                                "<h1>Hello from the host page</h1>",
                            )),
                        ]),
                ),
        );
    })
}
