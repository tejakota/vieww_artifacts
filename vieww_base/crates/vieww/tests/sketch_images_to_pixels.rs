//! `Sketch::Image` all the way to pixels: a plain image, an affinely warped
//! textured mesh, and a tiled pattern, through the native rasteriser.

#![cfg(feature = "native")]

use vieww::foundation::{Color, Image, Offset, Path, Rect, Size, Sketchbook};
use vieww::paint::native::NativeRenderer;
use vieww::prelude::*;

const SIDE: u32 = 128;

fn checker() -> Image {
    // 4×4: left half red, right half blue.
    let px: Vec<u8> = (0..16)
        .flat_map(|i| {
            if i % 4 < 2 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            }
        })
        .collect();
    Image::from_rgba8(px, 4, 4)
}

fn render(book: Sketchbook) -> vieww::paint::native::Pixels {
    let mut driver = FrameDriver::new(Size::square(SIDE as f32));
    driver.elements().set_root(Painting::sized(
        Size::square(SIDE as f32),
        PaintWith::new(move |b: &mut Sketchbook, _s: Size| {
            for item in book.items() {
                b.push(item.clone());
            }
        }),
    ));
    driver.draw_frame();
    NativeRenderer::new()
        .render_to_pixels(driver.scene(), SIDE, SIDE, Color::WHITE)
        .expect("render")
        .0
}

#[test]
fn an_image_is_stretched_over_its_rect() {
    let mut book = Sketchbook::new();
    book.image(Rect::new(0.0, 0.0, 128.0, 64.0), checker());
    let px = render(book);
    assert_eq!(px.pixel(16, 32), Color::rgba(255, 0, 0, 255));
    assert_eq!(px.pixel(112, 32), Color::rgba(0, 0, 255, 255));
    assert_eq!(px.pixel(64, 100), Color::WHITE, "nothing below the rect");
}

#[test]
fn a_textured_mesh_flips_the_image_by_its_uvs() {
    // A quad whose uvs are mirrored horizontally: blue must land on the left.
    let mut book = Sketchbook::new();
    book.textured_mesh(
        &checker(),
        &[
            Offset::new(0.0, 0.0),
            Offset::new(128.0, 0.0),
            Offset::new(128.0, 128.0),
            Offset::new(0.0, 128.0),
        ],
        &[
            Offset::new(1.0, 0.0),
            Offset::new(0.0, 0.0),
            Offset::new(0.0, 1.0),
            Offset::new(1.0, 1.0),
        ],
        &[0, 1, 2, 0, 2, 3],
    );
    let px = render(book);
    assert_eq!(px.pixel(16, 64), Color::rgba(0, 0, 255, 255));
    assert_eq!(px.pixel(112, 64), Color::rgba(255, 0, 0, 255));
    // No seam along the shared diagonal.
    for k in 8..120 {
        assert_ne!(px.pixel(k, k), Color::WHITE, "seam at {k}");
    }
}

#[test]
fn a_pattern_tiles_inside_its_area() {
    let mut book = Sketchbook::new();
    book.pattern(
        Path::rect(Rect::new(0.0, 0.0, 64.0, 64.0)),
        &checker(),
        Offset::ZERO,
        Size::new(16.0, 16.0),
    );
    let px = render(book);
    assert_eq!(px.pixel(4, 4), Color::rgba(255, 0, 0, 255));
    assert_eq!(px.pixel(12, 4), Color::rgba(0, 0, 255, 255));
    assert_eq!(
        px.pixel(20, 4),
        Color::rgba(255, 0, 0, 255),
        "the next tile repeats"
    );
    assert_eq!(px.pixel(100, 100), Color::WHITE, "clipped to the area");
}
