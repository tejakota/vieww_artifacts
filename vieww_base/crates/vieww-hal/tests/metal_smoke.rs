//! The Metal backend against the claims the Vulkan smoke suites make:
//! an exact clear, a mesh in the right place (not mirrored), an empty mesh
//! that draws nothing, and several frames on one device.
//!
//! `#[ignore]`d like the Vulkan suites, because it needs the hardware:
//!
//! ```console
//! cargo test -p vieww-hal --features metal -- --ignored --test-threads=1
//! ```
//!
//! With `VIEWW_REQUIRE_HARDWARE` containing `gpu` (the macOS CI leg sets it),
//! a missing Metal device is a failure instead of a skip — a suite that can
//! quietly skip on the one runner meant to run it is not evidence.

#![cfg(all(any(target_os = "macos", target_os = "ios"), feature = "metal"))]

use vieww_foundation::{Color, Offset, Path};
use vieww_gpu::tessellate_fill;
use vieww_hal::metal::MetalDevice;
use vieww_hal::Device;

fn device() -> Option<MetalDevice> {
    match MetalDevice::new() {
        Ok(device) => Some(device),
        Err(error) => {
            let required = std::env::var("VIEWW_REQUIRE_HARDWARE")
                .is_ok_and(|v| v.split(',').any(|w| w.trim() == "gpu"));
            assert!(!required, "VIEWW_REQUIRE_HARDWARE=gpu, but: {error}");
            eprintln!("skipping: {error}");
            None
        }
    }
}

const WHY: &str =
    "needs a Metal device; run with `cargo test -p vieww-hal --features metal -- --ignored`";

#[test]
#[ignore = "needs a Metal device; run with `cargo test -p vieww-hal --features metal -- --ignored`"]
fn reports_an_adapter() {
    let Some(device) = device() else { return };
    let info = device.info();
    eprintln!("Metal adapter: {info:?} ({WHY})");
    assert_eq!(info.backend, "metal");
    assert!(!info.name.is_empty());
}

#[test]
#[ignore = "needs a Metal device; run with `cargo test -p vieww-hal --features metal -- --ignored`"]
fn a_clear_reads_back_as_the_requested_colour() {
    let Some(device) = device() else { return };
    for color in [
        Color::rgba(255, 0, 0, 255),
        Color::rgba(0, 255, 0, 255),
        Color::rgba(20, 130, 220, 200),
        Color::rgba(0, 0, 0, 0),
    ] {
        let pixels = device.render_clear_to_pixels(64, 48, color).unwrap();
        assert_eq!(pixels.len(), 64 * 48 * 4);
        for px in pixels.chunks_exact(4) {
            let close = |a: u8, b: u8| (i32::from(a) - i32::from(b)).abs() <= 1;
            assert!(
                close(px[0], color.r)
                    && close(px[1], color.g)
                    && close(px[2], color.b)
                    && close(px[3], color.a),
                "{px:?} != {color:?}"
            );
        }
    }
}

#[test]
#[ignore = "needs a Metal device; run with `cargo test -p vieww-hal --features metal -- --ignored`"]
fn a_mesh_lands_where_it_was_drawn_and_nowhere_else() {
    let Some(device) = device() else { return };
    // Off-centre on both axes, so a flip in either moves it (see
    // `vulkan_mesh_smoke.rs` for the bug this shape caught there).
    let mut path = Path::new();
    path.move_to(Offset::new(10.0, 10.0));
    path.line_to(Offset::new(60.0, 10.0));
    path.line_to(Offset::new(60.0, 35.0));
    path.line_to(Offset::new(10.0, 35.0));
    path.close();
    let mesh = tessellate_fill(&path, 0.1);
    let pixels = device
        .render_mesh_to_pixels(
            100,
            100,
            &mesh.positions,
            &mesh.indices,
            Color::rgba(255, 0, 0, 255),
        )
        .unwrap();
    let at = |x: usize, y: usize| {
        let i = (y * 100 + x) * 4;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    };
    assert_eq!(at(35, 22), [255, 0, 0, 255]);
    for (x, y) in [(5, 5), (90, 90), (35, 78), (85, 22)] {
        assert_eq!(at(x, y)[3], 0, "({x}, {y}) should be empty");
    }
}

#[test]
#[ignore = "needs a Metal device; run with `cargo test -p vieww-hal --features metal -- --ignored`"]
fn the_mesh_path_agrees_with_vulkans_pixel_rule() {
    // The sampling rule every GPU API shares (pixel centres, top-left fill
    // convention): a rect spanning 10..60 covers columns 10 through 59 and
    // nothing past them, so one mesh rasterises to the same pixels on Metal
    // as on Vulkan and D3D12.
    let Some(device) = device() else { return };
    let mut path = Path::new();
    path.move_to(Offset::new(10.0, 10.0));
    path.line_to(Offset::new(60.0, 10.0));
    path.line_to(Offset::new(60.0, 35.0));
    path.line_to(Offset::new(10.0, 35.0));
    path.close();
    let mesh = tessellate_fill(&path, 0.1);
    let pixels = device
        .render_mesh_to_pixels(
            100,
            100,
            &mesh.positions,
            &mesh.indices,
            Color::rgba(0, 0, 255, 255),
        )
        .unwrap();
    let alpha = |x: usize, y: usize| pixels[(y * 100 + x) * 4 + 3];
    assert_eq!(alpha(10, 10), 255);
    assert_eq!(alpha(59, 34), 255);
    assert_eq!(alpha(60, 20), 0);
    assert_eq!(alpha(20, 35), 0);
    assert_eq!(alpha(9, 20), 0);
}

#[test]
#[ignore = "needs a Metal device; run with `cargo test -p vieww-hal --features metal -- --ignored`"]
fn an_empty_mesh_draws_a_transparent_frame() {
    let Some(device) = device() else { return };
    let pixels = device
        .render_mesh_to_pixels(16, 16, &[], &[], Color::rgba(0, 255, 0, 255))
        .unwrap();
    assert!(pixels.iter().all(|&b| b == 0));
}

#[test]
#[ignore = "needs a Metal device; run with `cargo test -p vieww-hal --features metal -- --ignored`"]
fn a_bad_index_is_refused_not_drawn() {
    let Some(device) = device() else { return };
    let err = device
        .render_mesh_to_pixels(
            16,
            16,
            &[[0.0, 0.0]; 3],
            &[0, 1, 7],
            Color::rgba(0, 0, 0, 255),
        )
        .unwrap_err();
    assert!(err.to_string().contains("index 7"), "{err}");
}

#[test]
#[ignore = "needs a Metal device; run with `cargo test -p vieww-hal --features metal -- --ignored`"]
fn several_frames_on_one_device() {
    let Some(device) = device() else { return };
    for i in 0..5 {
        let pixels = device
            .render_clear_to_pixels(32, 32, Color::rgba(10, 20, 30, 255))
            .unwrap_or_else(|e| panic!("frame {i}: {e}"));
        assert_eq!(&pixels[..4], &[10, 20, 30, 255]);
    }
}
