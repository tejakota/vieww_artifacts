//! Unit tests for the `MeshPainter` — what gets recorded, in what order, and
//! when a repaint is honestly needed.
//!
//! These assert on the `Sketchbook` a `Painting` records rather than on
//! pixels: the recording is the contract between the widget and the
//! renderer, and it is exact where pixels are fuzzy.

use std::rc::Rc;

use vieww::prelude::*;
use vieww::foundation::{Brush, Sketch};
use vieww::widget::Painter;

use three_runtime::demo_capture;
use three_vieww::{MeshShading, OrbitCamera, Perspective};

use vieww_integration::{MeshPainter, ViewPalette, painter::shade};

const SIZE: Size = Size { width: 400.0, height: 300.0 };

/// The shared demo capture, framed from a fixed camera.
fn painter_over(capture: &Rc<three_core::Capture3D>, wireframe: bool, show_points: bool) -> MeshPainter {
    // A camera that frames the demo from a fixed angle, so the counts below
    // are stable. The demo's bounds center the fit; the yaw/pitch match the
    // viewer's initial viewing angle.
    let mut camera = OrbitCamera {
        yaw: 0.6,
        pitch: 0.35,
        distance: 5.0,
        target: three_core::Vec3::new(0.0, 0.0, 0.0),
    };
    if let Some(bounds) = capture.bounds() {
        camera.fit(bounds, Perspective::default().fov_y, 1.15);
    }
    MeshPainter {
        capture: Rc::clone(capture),
        mesh_index: Some(0),
        points_index: Some(0),
        camera,
        wireframe,
        show_points,
        palette: ViewPalette::from_theme(&ThemeData::dark()),
        shading: MeshShading::default(),
        lens: Perspective::default(),
    }
}

fn demo_painter(wireframe: bool, show_points: bool) -> MeshPainter {
    painter_over(&Rc::new(demo_capture()), wireframe, show_points)
}

/// What a painter records at `SIZE`, by sketch kind.
fn counts(painter: MeshPainter) -> (usize, usize, usize) {
    let book = Painting::new(painter).record(SIZE);

    let mut fills = 0;
    let mut strokes = 0;
    let mut other = 0;
    for sketch in book.items() {
        match sketch {
            Sketch::Fill { .. } => fills += 1,
            Sketch::Stroke { .. } => strokes += 1,
            _ => other += 1,
        }
    }
    (fills, strokes, other)
}

#[test]
fn the_demo_camera_sees_every_triangle() {
    // The demo's frame 0 is an 8x8-cell grid: 128 triangles, plus a point
    // frame of 32 motes, plus the chamber background.
    let (fills, strokes, other) = counts(demo_painter(false, true));
    assert_eq!((fills, strokes, other), (161, 0, 0), "1 chamber + 128 triangles + 32 motes");
}

#[test]
fn hiding_the_motes_removes_their_fills() {
    let shown = counts(demo_painter(false, true));
    let hidden = counts(demo_painter(false, false));
    assert_eq!(hidden.0, shown.0 - 32, "32 motes stop being drawn");
}

#[test]
fn wireframe_records_strokes_not_fills() {
    let wire = counts(demo_painter(true, true));
    // The chamber stays a fill; every mesh edge becomes a stroke; the motes
    // stay fills.
    assert_eq!(wire.0, 33, "chamber + 32 motes");
    // A 9x9-vertex grid has 9*8 vertical + 8*9 horizontal edges plus one
    // diagonal per cell (8*8): 208 unique edges.
    assert_eq!(wire.1, 208, "the grid's unique edges");
}

#[test]
fn the_first_recorded_item_is_the_chamber() {
    let painter = demo_painter(false, false);
    let book = Painting::new(painter).record(SIZE);
    let Sketch::Fill { path, .. } = &book.items()[0] else {
        panic!("the chamber is a fill");
    };
    let bounds = path.bounds();
    assert_eq!(bounds.left, 0.0);
    assert_eq!(bounds.top, 0.0);
    assert_eq!(bounds.width(), SIZE.width);
    assert_eq!(bounds.height(), SIZE.height);
}

#[test]
fn triangle_fills_are_shaded_below_full_brightness() {
    let painter = demo_painter(false, false);
    let book = Painting::new(painter).record(SIZE);
    let Sketch::Fill { brush, .. } = &book.items()[1] else {
        panic!("after the chamber comes a triangle");
    };
    let Brush::Solid(color) = brush else {
        panic!("a triangle is filled with a solid colour");
    };

    // The wave surface is rarely square to the light, so at least one of the
    // early (far, floor-like) triangles must come out darker than the pure
    // theme primary — that is what makes the surface read as 3D.
    let base = ViewPalette::from_theme(&ThemeData::dark()).mesh;
    assert!(
        color.r < base.r || color.g < base.g || color.b < base.b,
        "shading must darken the base colour: {color:?} vs {base:?}"
    );
}

#[test]
fn a_degenerate_box_records_nothing() {
    let book = Painting::new(demo_painter(false, true)).record(Size::new(0.0, 0.0));
    assert!(book.is_empty(), "no geometry fits in no space");
}

#[test]
fn should_repaint_sees_every_difference_that_matters() {
    let capture = Rc::new(demo_capture());
    let a = painter_over(&capture, false, true);

    // Identical values over the same capture: no repaint.
    let same = painter_over(&capture, false, true);
    assert!(!a.should_repaint(&same), "an identical painter repaints nothing");

    // A moved camera.
    let mut moved = painter_over(&capture, false, true);
    moved.camera.yaw += 0.5;
    assert!(a.should_repaint(&moved));

    // A later frame.
    let mut later = painter_over(&capture, false, true);
    later.mesh_index = Some(7);
    assert!(a.should_repaint(&later));

    // A hidden layer, and a different mode.
    let mut motes_off = painter_over(&capture, false, true);
    motes_off.show_points = false;
    assert!(a.should_repaint(&motes_off));
    let wire = painter_over(&capture, true, true);
    assert!(a.should_repaint(&wire));

    // A different capture entirely: same-shaped content, but not the same
    // data behind it.
    let other = painter_over(&Rc::new(demo_capture()), false, true);
    assert!(a.should_repaint(&other), "a different capture is a different drawing");
}

#[test]
fn shade_darkens_each_channel() {
    let base = Color::rgb(200, 100, 50);
    assert_eq!(shade(base, 1.0), base);
    assert_eq!(shade(base, 0.5), Color::rgb(100, 50, 25));
    // Out-of-range brightness clamps rather than overflowing.
    assert_eq!(shade(base, 7.0), base);
    assert_eq!(shade(base, -1.0), Color::rgb(0, 0, 0));
    // Alpha rides along untouched.
    let translucent = Color::rgba(200, 100, 50, 128);
    assert_eq!(shade(translucent, 0.5).a, 128);
}
