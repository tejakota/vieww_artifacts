//! Does `Filtered` survive a repaint boundary inside it?
//!
//! Wrapping the library in `Filtered::blur` blurred the header and made the
//! photo grid disappear entirely. The grid is inside a `Scrollable`, and
//! `RenderViewport::is_repaint_boundary()` is `true` — so the suspicion is
//! that a nested boundary records into its own layer, which the filter's
//! offscreen pass never sees.
//!
//! Stock widgets only, so whatever this finds is reportable as-is.

use std::time::Duration;

use vieww::foundation::{Color, Size};
use vieww::paint::native::NativeRenderer;
use vieww::prelude::*;
use vieww::render::FrameDriver;

const W: f32 = 300.0;
const H: f32 = 200.0;

/// Three stripes, tall enough to overflow a scrollable.
fn stripes() -> WidgetNode {
    Flex::column()
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .spacing(6.0)
        .children(children![
            Container::new().height(60.0).color(Color::rgb(0xff, 0x6b, 0x81)),
            Container::new().height(60.0).color(Color::rgb(0x7c, 0x3a, 0xed)),
            Container::new().height(60.0).color(Color::rgb(0xff, 0xb5, 0x45)),
        ])
        .into()
}

fn main() {
    let out = std::path::PathBuf::from(
        std::env::args().nth(1).unwrap_or_else(|| "shots".into()),
    );
    std::fs::create_dir_all(&out).expect("creating the output directory");

    let cases: Vec<(&str, WidgetNode)> = vec![
        ("A-plain", stripes()),
        ("B-blurred", Filtered::blur(6.0).child(stripes()).into()),
        (
            "C-scrollable-unblurred",
            Scrollable::vertical(0.0).child(stripes()).into(),
        ),
        (
            "D-blur-outside-scrollable",
            Filtered::blur(6.0)
                .child(Scrollable::vertical(0.0).child(stripes()))
                .into(),
        ),
        (
            "E-blur-inside-scrollable",
            Scrollable::vertical(0.0)
                .child(Filtered::blur(6.0).child(stripes()))
                .into(),
        ),
        // The app's actual shape: a plain header AND a scrollable, both
        // inside one filter.
        (
            "F-blur-header-and-scrollable",
            Filtered::blur(6.0)
                .child(
                    Flex::column()
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .children(children![
                            Container::new().height(40.0).color(Color::rgb(0x35, 0xd6, 0xa9)),
                            Flexible::expanded(1)
                                .child(Scrollable::vertical(0.0).child(stripes())),
                        ]),
                )
                .into(),
        ),
    ];

    for (name, root) in cases {
        let mut driver = FrameDriver::new(Size::new(W, H));
        let mut renderer = NativeRenderer::new();
        driver.set_root(
            Theme::new(ThemeData::dark()).child(
                Container::new()
                    .color(Color::rgb(0x12, 0x10, 0x20))
                    .child(root),
            ),
        );
        driver.draw_frame_at(Duration::from_millis(16));

        let (png, report) = renderer
            .render_to_png(driver.scene(), W as u32, H as u32, Color::BLACK)
            .expect("rasterising");
        std::fs::write(out.join(format!("{name}.png")), png).expect("writing the PNG");
        println!(
            "{name:28} shapes {:2}  layers {:2}  filtered {:2}",
            report.shapes, report.layers, report.filtered_layers
        );
    }
}
