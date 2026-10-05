//! Pure depth-unit math shared by the capture backends.
//!
//! These functions have no platform imports on purpose: the iOS backend
//! calls them from its delegate callbacks, and the host test runner
//! executes the same code the device will — a conversion that only ever
//! ran on a phone is a conversion nobody has checked.

/// Depth in meters to millimetres with a plausibility filter.
///
/// A zero or absurd sample is data corruption rather than measurement, and
/// unknown (0) is the honest answer for it: the depth track's consumers
/// treat 0 as "no depth here", never as "the subject is touching the lens".
// The iOS backend is this module's only caller; everywhere else it is
// compiled so the host test runner can execute the same code the device
// will.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub fn depth_meters_to_mm(meters: &[f32]) -> Vec<u16> {
    meters
        .iter()
        .map(|&m| {
            if m.is_finite() && (0.1..=20.0).contains(&m) {
                (m * 1000.0).round().clamp(0.0, 8191.0) as u16
            } else {
                0
            }
        })
        .collect()
}

/// Nearest-neighbour resample of a depth map onto another grid.
///
/// Hardware depth arrives at its own resolution (a LiDAR 240x320 map next
/// to 640x480 color, say) and a frame's depth track must match its color
/// track, pixel for pixel, so the poll maps the coarser grid up before it
/// becomes millimetres. Source positions are floored, which keeps the
/// sample count exact and never reads out of bounds: for `y < th`,
/// `y * fh / th < fh` strictly.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub fn resample_depth(meters: &[f32], from: (u32, u32), to: (u32, u32)) -> Vec<f32> {
    let (fw, fh) = (from.0.max(1), from.1.max(1));
    let (tw, th) = (to.0.max(1), to.1.max(1));
    if fw == tw && fh == th {
        return meters.to_vec();
    }
    let mut out = Vec::with_capacity(tw as usize * th as usize);
    for y in 0..th {
        let sy = (y as u64 * fh as u64 / th as u64) as u32;
        for x in 0..tw {
            let sx = (x as u64 * fw as u64 / tw as u64) as u32;
            out.push(meters[(sy * fw + sx) as usize]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_meters_survive_the_conversion() {
        let mm = depth_meters_to_mm(&[1.0, 0.5, 3.75, 0.0, -1.0, f32::NAN, 25.0]);
        assert_eq!(mm[0], 1000);
        assert_eq!(mm[1], 500);
        assert_eq!(mm[2], 3750);
        assert_eq!(mm[3], 0, "zero meters is unknown, not zero depth");
        assert_eq!(mm[4], 0, "negative is corruption");
        assert_eq!(mm[5], 0, "NaN is corruption");
        assert_eq!(mm[6], 0, "beyond the 13-bit range is corruption");
    }

    #[test]
    fn same_size_resample_is_identity() {
        let meters: Vec<f32> = (0..12).map(|i| i as f32).collect();
        let out = resample_depth(&meters, (4, 3), (4, 3));
        assert_eq!(out, meters);
    }

    #[test]
    fn upsampling_repeats_the_nearest_source_pixel() {
        // 2x2 upscaled to 4x4: each source pixel owns a 2x2 block.
        let meters = vec![1.0, 2.0, 3.0, 4.0];
        let out = resample_depth(&meters, (2, 2), (4, 4));
        assert_eq!(
            out,
            vec![
                1.0, 1.0, 2.0, 2.0, //
                1.0, 1.0, 2.0, 2.0, //
                3.0, 3.0, 4.0, 4.0, //
                3.0, 3.0, 4.0, 4.0,
            ]
        );
    }

    #[test]
    fn anisotropic_resample_picks_floor_positions() {
        // 1x4 shrunk to 1x2: y=0 -> sy=0, y=1 -> sy=2.
        let meters = vec![10.0, 20.0, 30.0, 40.0];
        let out = resample_depth(&meters, (1, 4), (1, 2));
        assert_eq!(out, vec![10.0, 30.0]);
    }
}
