//! Mathematical notation — the `MathText` widget, photographed.
//!
//! Five formulas, one screen: the TeX-subset's every feature — scripts,
//! fractions, a radical with an index, Greek, the operators — rendered
//! from the strings a caller actually writes. Each line is also the
//! parser's test corpus in miniature, which is why the Gauss sum is here.

use vieww_foundation::Color;
use vieww_widget::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    feature_harness::launch("76 — math text", Size::new(640.0, 400.0), |d| {
        feature_harness::set_page(
            d,
            Container::new()
                .color(Color::WHITE)
                .padding(EdgeInsets::all(24.0))
                .child(
                    Flex::column()
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .spacing(26.0)
                        .children(children![
                            MathText::new("x^2 + y^2 = z^2").size(18.0),
                            MathText::new("\\sum_{i=0}^{n} i = \\frac{n(n+1)}{2}").size(18.0),
                            MathText::new(
                                "E = mc^2 \\quad \\to \\quad \\sqrt[3]{\\frac{E}{c^2}} = m"
                            )
                            .size(18.0),
                            MathText::new("\\frac{\\partial u}{\\partial t} = \\alpha \\nabla^2 u")
                                .size(18.0),
                            MathText::new("\\int_{-\\infty}^{\\infty} e^{-x^2} dx = \\sqrt{\\pi}")
                                .size(18.0),
                            MathText::new(
                                "\\alpha \\cdot \\beta \\le \\gamma \\ne \\omega \\pm \\epsilon"
                            )
                            .size(18.0),
                        ]),
                ),
        );
    })
}
