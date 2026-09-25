//! Tests for the resampler — the part of this app that is not a widget.
//!
//! These assert on properties rather than on golden pixels. A golden-image test
//! of a resampler is brittle in the least useful way: it fails on a rounding
//! change that nobody can see and passes on a sign error that produces a
//! plausible-looking wrong picture. What is worth pinning down is that the
//! output is the right size, that flat areas stay flat, that the picture does
//! not drift, and that the thing the app claims to do — recover detail — is
//! measurably true.

use upxcale::upscale::{acutance, resample, sharpen, upscale};
use vieww_foundation::Image;

/// A solid colour.
fn flat(w: u32, h: u32, rgba: [u8; 4]) -> Image {
    Image::from_rgba8(rgba.repeat((w * h) as usize), w, h)
}

/// A checkerboard — the hardest thing for a resampler, and the easiest to see
/// ringing in.
fn checker(w: u32, h: u32, cell: u32) -> Image {
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let on = ((x / cell) + (y / cell)) % 2 == 0;
            let v = if on { 235 } else { 20 };
            px.extend_from_slice(&[v, v, v, 255]);
        }
    }
    Image::from_rgba8(px, w, h)
}

/// A single white pixel on **opaque** black, for checking the filter is centred.
///
/// The opacity is load-bearing and was got wrong the first time. With a
/// transparent background the surrounding pixels contribute alpha 0, so the
/// output is a blurred alpha ramp over a near-constant white — and
/// `unpremultiply` divides the colour back out by that same ramp, restoring
/// full white across the whole blurred region. Luma ignores alpha, so the
/// "brightest pixel" became a plateau and the search returned its top-left
/// corner: the test failed by eight pixels while the resampler was correct.
///
/// Opaque black everywhere makes alpha constant, so luma tracks the impulse
/// itself, which is what this test is actually about.
fn impulse(size: u32) -> Image {
    let mut px = vec![0_u8; (size * size * 4) as usize];
    for pixel in px.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
    let centre = ((size / 2 * size + size / 2) * 4) as usize;
    px[centre] = 255;
    px[centre + 1] = 255;
    px[centre + 2] = 255;
    Image::from_rgba8(px, size, size)
}

fn luma_at(image: &Image, x: u32, y: u32) -> f32 {
    let p = ((y * image.width() + x) * 4) as usize;
    let px = image.pixels();
    0.299 * f32::from(px[p]) + 0.587 * f32::from(px[p + 1]) + 0.114 * f32::from(px[p + 2])
}

#[test]
fn output_has_the_requested_dimensions() {
    let source = checker(32, 24, 4);
    let out = resample(&source, 97, 61);
    assert_eq!(out.width(), 97);
    assert_eq!(out.height(), 61);
    assert_eq!(out.pixels().len(), (97 * 61 * 4) as usize);
}

#[test]
fn upscale_multiplies_both_axes() {
    let source = checker(20, 30, 5);
    let out = upscale(&source, 4);
    assert_eq!(out.width(), 80);
    assert_eq!(out.height(), 120);
}

/// A flat field must come out flat. This is the normalisation check: if the
/// kernel weights do not sum to one, a solid colour develops ripple, and it is
/// the single most common way to get a resampler subtly wrong.
#[test]
fn flat_fields_stay_flat() {
    let source = flat(16, 16, [120, 64, 200, 255]);
    let out = resample(&source, 64, 64);
    for chunk in out.pixels().chunks_exact(4) {
        // One level of slack for the rounding to u8.
        assert!(
            (i32::from(chunk[0]) - 120).abs() <= 1
                && (i32::from(chunk[1]) - 64).abs() <= 1
                && (i32::from(chunk[2]) - 200).abs() <= 1
                && chunk[3] == 255,
            "flat field developed ripple: {chunk:?}"
        );
    }
}

/// Resampling to the same size must return essentially the same picture.
///
/// At scale 1 the Lanczos kernel samples exactly on the source grid, where
/// `sinc` is 1 at the centre tap and 0 at every other integer, so the identity
/// falls out of the maths. If it does not, the sample positions are off by half
/// a pixel — which is invisible on a photograph and catastrophic on a
/// checkerboard.
#[test]
fn identity_scale_is_the_identity() {
    let source = checker(24, 24, 3);
    let out = resample(&source, 24, 24);
    for (i, (a, b)) in source.pixels().iter().zip(out.pixels()).enumerate() {
        assert!(
            i32::from(*a).abs_diff(i32::from(*b)) <= 1,
            "byte {i} changed: {a} -> {b}"
        );
    }
}

/// An impulse must stay in the middle. A filter whose sample positions are off
/// by half a pixel drifts the whole image toward one corner — the classic
/// resampler bug, and one that hides completely on smooth content.
#[test]
fn the_image_does_not_drift() {
    let size = 33;
    let scale = 3;
    let out = resample(&impulse(size), size * scale, size * scale);

    // Find the brightest pixel and check it sits where the centre mapped to.
    let (mut best, mut best_at) = (-1.0_f32, (0_u32, 0_u32));
    for y in 0..out.height() {
        for x in 0..out.width() {
            let l = luma_at(&out, x, y);
            if l > best {
                best = l;
                best_at = (x, y);
            }
        }
    }
    let expected = (size / 2) * scale + scale / 2;
    let (dx, dy) = (
        best_at.0.abs_diff(expected) as i32,
        best_at.1.abs_diff(expected) as i32,
    );
    assert!(
        dx <= 1 && dy <= 1,
        "impulse landed at {best_at:?}, expected about ({expected}, {expected})"
    );
}

/// Alpha is premultiplied for the duration, so a transparent region must not
/// bleed its colour channels into an opaque neighbour.
#[test]
fn transparent_pixels_do_not_bleed() {
    // Left half opaque white, right half fully transparent *red* — the red is
    // the trap: with straight-alpha resampling it drags a pink fringe across
    // the seam.
    let (w, h) = (16, 8);
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..h {
        for x in 0..w {
            if x < w / 2 {
                px.extend_from_slice(&[255, 255, 255, 255]);
            } else {
                px.extend_from_slice(&[255, 0, 0, 0]);
            }
        }
    }
    let out = resample(&Image::from_rgba8(px, w, h), w * 4, h * 4);

    // Sample well inside the opaque half; it must still be neutral.
    for y in 0..out.height() {
        let x = out.width() / 4;
        let p = ((y * out.width() + x) * 4) as usize;
        let bytes = &out.pixels()[p..p + 4];
        assert!(
            bytes[0] == bytes[1] && bytes[1] == bytes[2],
            "opaque region picked up a colour cast from the transparent side: {bytes:?}"
        );
    }
}

/// The claim the app is built on: an upscaled photograph carries more fine
/// detail than the same enlargement without the sharpening pass.
#[test]
fn sharpening_recovers_detail_a_plain_enlargement_loses() {
    let source = checker(24, 24, 4);
    let plain = resample(&source, 96, 96);
    let finished = upscale(&source, 4);

    let plain_detail = acutance(&plain);
    let finished_detail = acutance(&finished);

    assert!(
        finished_detail > plain_detail * 1.05,
        "sharpening added nothing: {plain_detail:.3} -> {finished_detail:.3}"
    );
}

/// The same claim, on the photographs the app actually ships.
///
/// This test exists because the checkerboard version above passed while the
/// sharpener was doing **nothing at all** to a real photograph. A synthetic
/// checkerboard is 20-vs-235 at every edge, so it clears any threshold and
/// survives any blur radius; a photograph's detail after a 4x enlargement is
/// two orders of magnitude smaller than that. Tuning the mask against the
/// checkerboard alone produced parameters that measured beautifully and did
/// nothing.
///
/// So: real assets, and a real margin.
#[test]
fn sharpening_measurably_improves_the_shipped_photographs() {
    let bundle = vieww_asset::DirectoryBundle::at(concat!(env!("CARGO_MANIFEST_DIR"), "/assets"));

    let mut checked = 0;
    for photo in upxcale::photos::CATALOGUE {
        let bytes = vieww_asset::AssetBundle::open(&bundle, &photo.asset_path())
            .unwrap_or_else(|e| panic!("{}: {e}", photo.asset_path()));
        let (source, _) = vieww_asset::decode(&bytes).expect("decoding");

        let factor = upxcale::photos::UPSCALE_FACTOR;
        let plain = resample(&source, source.width() * factor, source.height() * factor);
        let finished = upscale(&source, factor);

        let plain_detail = acutance(&plain);
        let finished_detail = acutance(&finished);

        // 8%, not a round 10%. The bar is set below the *weakest* photograph in
        // the set (`summer-meadow`, which measures about 9.8%) rather than at a
        // number the strongest ones clear comfortably: this test exists to
        // catch the sharpener doing nothing, and a bar tuned to the best case
        // would fail on a future photograph that is simply smoother.
        assert!(
            finished_detail > plain_detail * 1.08,
            "{}: sharpening added under 8%: {plain_detail:.4} -> {finished_detail:.4}",
            photo.slug
        );
        checked += 1;
    }
    assert_eq!(checked, upxcale::photos::CATALOGUE.len());
}

/// The threshold must actually suppress something, or it is not doing its job:
/// sharpening with a threshold above the image's entire dynamic range must be a
/// no-op.
#[test]
fn the_threshold_suppresses_noise() {
    let source = checker(16, 16, 2);
    let untouched = sharpen(&source, 0.6, 0.9, 10.0);
    assert_eq!(
        untouched.pixels(),
        source.pixels(),
        "a threshold above full scale should leave every pixel alone"
    );
}

/// Degenerate inputs must not panic — a zero-sized image is reachable from a
/// truncated file, and a resampler that unwinds there takes the frame with it.
#[test]
fn degenerate_sizes_are_survivable() {
    let empty = Image::from_rgba8(Vec::new(), 0, 0);
    let out = resample(&empty, 8, 8);
    assert_eq!(out.width(), 8);
    assert_eq!(out.height(), 8);

    let single = flat(1, 1, [10, 20, 30, 255]);
    let grown = upscale(&single, 4);
    assert_eq!(grown.width(), 4);
    assert_eq!(grown.height(), 4);

    // Factor zero is clamped to one rather than producing an empty image.
    let same = upscale(&single, 0);
    assert_eq!(same.width(), 1);
}

/// Downscaling is not what the app does, but the planner handles it and a
/// future thumbnail path would use it. Check it does not alias into noise.
#[test]
fn downscaling_averages_rather_than_samples() {
    // A 1px checkerboard shrunk 8x should approach the mean grey, not pick one
    // phase of the pattern.
    let source = checker(64, 64, 1);
    let out = resample(&source, 8, 8);
    for y in 0..out.height() {
        for x in 0..out.width() {
            let l = luma_at(&out, x, y);
            assert!(
                (40.0..=215.0).contains(&l),
                "pixel ({x}, {y}) aliased to {l}, not an average"
            );
        }
    }
}
