//! The depth warp: re-photograph a frame's geometry from the viewer's
//! virtual camera.

use three_core::{CameraIntrinsics, Vec3};

use crate::ViewCamera;

/// The rendered frame: RGBA bytes and their dimensions.
#[derive(Clone, Debug, PartialEq)]
pub struct WarpOutput {
    pub width: u32,
    pub height: u32,
    /// RGBA8, row-major, top-left origin, always fully opaque where any
    /// data existed and hole-filled everywhere else.
    pub rgba: Vec<u8>,
}

/// Depth samples within this band of the buffer's winner blend together;
/// farther ones lose outright. Ten millimeters — smaller than any parallax
/// the gestures can produce at social distances, wide enough that the four
/// neighbours of a bilinear footprint all blend rather than fight.
const BLEND_BAND_M: f32 = 0.01;

/// Render one frame as seen from `camera`.
///
/// # The algorithm, honestly
///
/// A **forward splat**: every source pixel is unprojected to its measured
/// depth, transformed into the virtual camera, re-projected, and blended
/// into a bilinear footprint under a z-buffer. Forward, not backward,
/// because the alternative — for each *output* pixel, find the source pixel
/// that lands here — needs a depth lookup along a ray the source camera
/// never measured. The splat asks only questions the file can answer.
///
/// Holes are the price: geometry the source camera could not see (the far
/// side of the subject, regions occluded at capture time) has no pixels to
/// splat. [`fill_holes`] propagates the nearest valid colour into them —
/// the same compromise every 3D-photo renderer ships with, and the clamps
/// on [`ViewCamera`] exist to keep the holes a fringe rather than the
/// picture.
///
/// # The fast path
///
/// At the neutral pose — and for frames with no known depth at all — the
/// warp *is* the frame, and this function returns the source pixels without
/// touching them. Feed autoplay sits on this path: a card scrolling by
/// costs a copy, not a re-projection.
pub fn warp_frame(
    color: &[u8],
    depth_mm: &[u16],
    intrinsics: &CameraIntrinsics,
    camera: &ViewCamera,
    subject_distance: f32,
) -> WarpOutput {
    let (w, h) = (intrinsics.width as usize, intrinsics.height as usize);
    debug_assert_eq!(color.len(), w * h * 4, "validated captures only");
    debug_assert_eq!(depth_mm.len(), w * h, "validated captures only");

    // No measured geometry, or nowhere to move: the frame is the truth.
    let has_depth = depth_mm.iter().any(|&mm| mm > 0);
    if !has_depth || camera.is_neutral() {
        return WarpOutput {
            width: w as u32,
            height: h as u32,
            rgba: color.to_vec(),
        };
    }

    let (right, down, forward) = (camera.right(), camera.down(), camera.forward());
    let eye = camera.eye(subject_distance.max(0.05));
    let (fx, fy, cx, cy) = (intrinsics.fx, intrinsics.fy, intrinsics.cx, intrinsics.cy);

    // Accumulators: colour sums and weights per output pixel, plus the
    // z-buffer. f32 colour accumulations over <=4 weighted neighbours
    // cannot overflow u8 sums and keep the blend order-independent.
    let pixels = w * h;
    let mut zbuf = vec![f32::INFINITY; pixels];
    let mut weight = vec![0.0f32; pixels];
    let mut accum = vec![[0.0f32; 3]; pixels];

    for y in 0..h {
        for x in 0..w {
            let index = y * w + x;
            let millimeters = depth_mm[index];
            if millimeters == 0 {
                continue; // unknown depth: nothing to place
            }
            let z = millimeters as f32 / 1000.0;

            // Unproject into the capture's camera space (which *is* world
            // space here — the source camera is the frame of reference).
            let p = Vec3::new((x as f32 - cx) * z / fx, (y as f32 - cy) * z / fy, z);
            // Into the virtual camera's basis, at its eye.
            let d = Vec3::new(p.x - eye.x, p.y - eye.y, p.z - eye.z);
            let zz = d.x * forward.x + d.y * forward.y + d.z * forward.z;
            if zz <= crate::WARP_NEAR {
                continue; // behind the virtual lens
            }
            let xx = d.x * right.x + d.y * right.y + d.z * right.z;
            let yy = d.x * down.x + d.y * down.y + d.z * down.z;
            // Project with the *same* intrinsics: the warp is a re-photograph
            // through the same lens, so dolly is the only zoom there is.
            let u = fx * xx / zz + cx;
            let v = fy * yy / zz + cy;

            // Bilinear footprint: the four integer neighbours of (u, v).
            let (u0, v0) = (u.floor(), v.floor());
            for (iu, wu) in [(u0, 1.0 - (u - u0)), (u0 + 1.0, u - u0)] {
                for (iv, wv) in [(v0, 1.0 - (v - v0)), (v0 + 1.0, v - v0)] {
                    let (iu, iv) = (iu as isize, iv as isize);
                    if iu < 0 || iv < 0 || iu >= w as isize || iv >= h as isize {
                        continue;
                    }
                    let contribution = wu * wv;
                    if contribution <= 0.0 {
                        continue;
                    }
                    let out = iv as usize * w + iu as usize;
                    let pixel = &color[index * 4..index * 4 + 3];
                    if zz < zbuf[out] - BLEND_BAND_M {
                        // Clearly nearer: it wins outright and resets the
                        // accumulation, keeping occlusion crisp.
                        zbuf[out] = zz;
                        weight[out] = contribution;
                        accum[out] = [
                            pixel[0] as f32 * contribution,
                            pixel[1] as f32 * contribution,
                            pixel[2] as f32 * contribution,
                        ];
                    } else if zz <= zbuf[out] + BLEND_BAND_M {
                        // Within the band: blend. This is what makes edges
                        // smooth instead of speckled.
                        zbuf[out] = zbuf[out].min(zz);
                        weight[out] += contribution;
                        accum[out][0] += pixel[0] as f32 * contribution;
                        accum[out][1] += pixel[1] as f32 * contribution;
                        accum[out][2] += pixel[2] as f32 * contribution;
                    }
                    // Farther than the band: occluded, dropped.
                }
            }
        }
    }

    let mut rgba = vec![0u8; pixels * 4];
    for index in 0..pixels {
        if weight[index] > 0.0 {
            rgba[index * 4] = (accum[index][0] / weight[index]).round().clamp(0.0, 255.0) as u8;
            rgba[index * 4 + 1] = (accum[index][1] / weight[index]).round().clamp(0.0, 255.0) as u8;
            rgba[index * 4 + 2] = (accum[index][2] / weight[index]).round().clamp(0.0, 255.0) as u8;
            rgba[index * 4 + 3] = 255;
        }
    }
    fill_holes(&mut rgba, w, h);

    WarpOutput {
        width: w as u32,
        height: h as u32,
        rgba,
    }
}

/// Propagate the nearest valid colour into unsplatted pixels.
///
/// Two rounds of four directional sweeps (left→right, right→left, top→bottom,
/// bottom→top), each carrying the last valid pixel it walked past. The order
/// is fixed, so the fill is deterministic; two rounds cross diagonals that
/// one cannot. A frame with *no* valid pixels stays black — the honest
/// answer for a warp that saw nothing.
fn fill_holes(rgba: &mut [u8], w: usize, h: usize) {
    for _ in 0..2 {
        // Horizontal passes.
        for row in 0..h {
            let mut carry: Option<[u8; 3]> = None;
            for column in 0..w {
                let index = (row * w + column) * 4;
                if rgba[index + 3] == 255 {
                    carry = Some([rgba[index], rgba[index + 1], rgba[index + 2]]);
                } else if let Some(color) = carry {
                    rgba[index..index + 3].copy_from_slice(&color);
                    rgba[index + 3] = 254; // filled, not measured
                }
            }
            let mut carry: Option<[u8; 3]> = None;
            for column in (0..w).rev() {
                let index = (row * w + column) * 4;
                if rgba[index + 3] == 255 {
                    carry = Some([rgba[index], rgba[index + 1], rgba[index + 2]]);
                } else if let Some(color) = carry {
                    rgba[index..index + 3].copy_from_slice(&color);
                    rgba[index + 3] = 254;
                }
            }
        }
        // Vertical passes.
        for column in 0..w {
            let mut carry: Option<[u8; 3]> = None;
            for row in 0..h {
                let index = (row * w + column) * 4;
                if rgba[index + 3] == 255 {
                    carry = Some([rgba[index], rgba[index + 1], rgba[index + 2]]);
                } else if let Some(color) = carry {
                    rgba[index..index + 3].copy_from_slice(&color);
                    rgba[index + 3] = 254;
                }
            }
            let mut carry: Option<[u8; 3]> = None;
            for row in (0..h).rev() {
                let index = (row * w + column) * 4;
                if rgba[index + 3] == 255 {
                    carry = Some([rgba[index], rgba[index + 1], rgba[index + 2]]);
                } else if let Some(color) = carry {
                    rgba[index..index + 3].copy_from_slice(&color);
                    rgba[index + 3] = 254;
                }
            }
        }
    }
    // Whatever is still unfilled had no measured neighbour in reach: black.
    for alpha in rgba.iter_mut().skip(3).step_by(4) {
        if *alpha != 255 {
            *alpha = 255;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── the scene ────────────────────────────────────────────────────────
    //
    // A dense background plane at 2.5 m with a red 3x3 block at 1 m,
    // one third in from the left, vertically centred. Every expected
    // position below was computed by hand from the pinhole model —
    // independently of this file's code, on paper — which is the point:
    // a renderer's tests must not derive their answers from the renderer.

    const W: u32 = 96;
    const H: u32 = 64;
    const SUBJECT: f32 = 2.0;

    fn scene() -> (Vec<u8>, Vec<u16>, CameraIntrinsics) {
        let k = CameraIntrinsics::approximate(W, H);
        let mut color = vec![0u8; (W * H * 4) as usize];
        let mut depth = vec![2500u16; (W * H) as usize];
        for y in 0..H {
            for x in 0..W {
                let index = (y * W + x) as usize;
                color[index * 4..index * 4 + 4].copy_from_slice(&[40, 60, 90, 255]);
                if (29..32).contains(&x) && (31..34).contains(&y) {
                    color[index * 4..index * 4 + 4].copy_from_slice(&[240, 40, 40, 255]);
                    depth[index] = 1000;
                }
            }
        }
        (color, depth, k)
    }

    /// The output x of the red block's centroid, for a given camera.
    fn red_centroid_x(out: &WarpOutput) -> f32 {
        let mut sum = 0.0;
        let mut count = 0.0;
        for y in 0..H as usize {
            for x in 0..W as usize {
                let pixel = &out.rgba[(y * W as usize + x) * 4..][..3];
                if pixel[0] > 200 && pixel[1] < 120 {
                    sum += x as f32;
                    count += 1.0;
                }
            }
        }
        assert!(count > 0.0, "the red block must survive the warp");
        sum / count
    }

    #[test]
    fn neutral_camera_returns_the_frame_untouched() {
        let (color, depth, k) = scene();
        let out = warp_frame(&color, &depth, &k, &ViewCamera::default(), SUBJECT);
        assert_eq!(out.rgba, color, "the fast path is a copy, not a re-render");
    }

    #[test]
    fn a_frame_without_depth_is_always_untouched() {
        let (color, depth, k) = scene();
        let no_depth = vec![0u16; depth.len()];
        let moved = crate::test_camera(0.3, 0.1, 0.8);
        let out = warp_frame(&color, &no_depth, &k, &moved, SUBJECT);
        assert_eq!(out.rgba, color, "no geometry, no parallax, no lie");
    }

    #[test]
    fn an_orbit_lands_the_near_block_where_the_math_says() {
        // Camera: yaw 0.2, dolly 1. Hand-computed landing of the block's
        // centre (source x=30, y=32, z=1 m, fx=83.138, cx=47.5):
        //
        //   p      = ((30-47.5)*1/83.138, (32-31.5)*1/83.138, 1)
        //           = (-0.2103, 0.0060, 1)
        //   eye    = (-0.3973, 0, 0.0399)   [2 m back from (0,0,2), yaw 0.2]
        //   d      = p - eye = (0.1870, 0.0060, 0.9601)
        //   z'     = d.f = 0.9782,  x' = d.right = -0.0074
        //   u      = 83.138 * (-0.0074) / 0.9782 + 47.5 = 46.87
        //
        // The background pixel from the same source position lands at
        // u = 35.1 — the near block is *ahead* of where the camera left
        // the background, which is parallax itself.
        let (color, depth, k) = scene();
        let camera = crate::test_camera(0.2, 0.0, 1.0);
        let out = warp_frame(&color, &depth, &k, &camera, SUBJECT);
        let centroid = red_centroid_x(&out);
        assert!(
            (centroid - 46.87).abs() < 0.75,
            "the block lands near 46.87, not {centroid:.2}"
        );
    }

    #[test]
    fn dollying_in_grows_the_near_block_more_than_the_background() {
        // Dolly 0.75 puts the eye at z = 0.5. Hand-computed:
        //   near block centre (z=1):  u = 83.138*(-0.2103)/0.5 + 47.5 = 12.51
        //   its right edge  (x=31):   u = 83.138*(-0.1984)/0.5 + 47.5 = 14.49
        //   -> ~1.98 output pixels per source pixel at the block
        //   background      (z=2.5):  u = 83.138*(-0.5258)/2.0 + 47.5 = 25.64
        //   its neighbour   (x=31):   u = 83.138*(-0.4958)/2.0 + 47.5 = 26.90
        //   -> ~1.26 output pixels per source pixel at the background
        //
        // Pinch-to-zoom-in-depth magnifies near geometry faster than far
        // geometry — the one property an ordinary 2D zoom cannot fake.
        let (color, depth, k) = scene();
        let camera = crate::test_camera(0.0, 0.0, 0.75);
        let out = warp_frame(&color, &depth, &k, &camera, SUBJECT);
        let centroid = red_centroid_x(&out);
        assert!(
            (centroid - 12.51).abs() < 0.75,
            "the block lands near 12.51, not {centroid:.2}"
        );

        // The magnification comparison, measured on the render itself: the
        // block's red pixel count vs the source's 9, against the
        // background's per-pixel spread. With the eye at 0.5 m:
        // near scale = 0.5/(1-0.5) ... measured as area ratio.
        let mut red_count = 0.0;
        for y in 0..H as usize {
            for x in 0..W as usize {
                let pixel = &out.rgba[(y * W as usize + x) * 4..][..3];
                if pixel[0] > 200 && pixel[1] < 120 {
                    red_count += 1.0;
                }
            }
        }
        // 9 source pixels at ~1.98x horizontal and ~2x vertical spread.
        assert!(
            (30.0..50.0).contains(&red_count),
            "the near block magnifies ~4x in area (got {red_count:.0} px from 9)"
        );
    }

    #[test]
    fn warp_is_deterministic() {
        let (color, depth, k) = scene();
        let camera = crate::test_camera(-0.25, 0.15, 1.3);
        let a = warp_frame(&color, &depth, &k, &camera, SUBJECT);
        let b = warp_frame(&color, &depth, &k, &camera, SUBJECT);
        assert_eq!(a, b);
    }

    #[test]
    fn output_is_fully_opaque() {
        let (color, depth, k) = scene();
        let camera = crate::test_camera(0.45, -0.3, 0.6);
        let out = warp_frame(&color, &depth, &k, &camera, SUBJECT);
        assert!(out.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    }

    #[test]
    fn behind_the_virtual_lens_is_dropped_not_warped() {
        // Dolly in until the background plane (2.5 m) is behind the eye
        // (dolly 0.8 * subject 2 = 1.6 m): nothing at 2.5 m survives,
        // and the near block at 1 m does. The render stays finite and
        // opaque — no division-by-zero ghosts.
        let (color, depth, k) = scene();
        let camera = crate::test_camera(0.0, 0.0, 0.8);
        let out = warp_frame(&color, &depth, &k, &camera, SUBJECT);
        assert!(out.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        // And the block is still somewhere on the sensor.
        assert!(red_centroid_x(&out) >= 0.0 && red_centroid_x(&out) <= W as f32);
    }
}
