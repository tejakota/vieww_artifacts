//! Split-frame execution: one frame, two executors, one merge — the
//! `Mixed` recommendation's path from a planner opinion to pixels on a
//! screen.
//!
//! # The shape, and why bands
//!
//! A `Mixed` plan says some of a frame's work belongs on each side. The
//! executable form of that, on the live path, is a **row band split**: both
//! executors render the *same* frame, and a horizontal band decides whose
//! pixels each row uses — the CPU's for the top `cpu_rows`, the GPU's for
//! the rest. Bands are the one split that needs no new compositor: both
//! sides already produce a correct whole frame, so partitioning the rows
//! partitions the truth with it. Per-pass dispatch — whole passes to
//! whichever side the planner favoured, with a real merge compositor —
//! remains the deeper architecture, and this module is the first executable
//! half of it: `Mixed` stops being unactionable, the split's fraction comes
//! from the plan's own placements
//! ([`ShadowPlan::mixed_cpu_fraction`](vieww_render_planner::ShadowPlan::mixed_cpu_fraction)),
//! and every split frame is recorded in the shadow log as the executor it
//! was.
//!
//! # What each side draws
//!
//! The CPU rasterises the frame as it always does — `NativeRenderer` has no
//! band-scoped render, so the split's CPU cost is the full raster and the
//! band only decides whose pixels show. The GPU draws only its band,
//! through the scene renderer's own damage path: the band is the damage,
//! the renderer's retained target keeps everything else, and the merge
//! reads back only the band's rows. When the caller supplies real frame
//! damage, a split frame's GPU half costs its band and not the frame; the
//! CPU half is the CPU's full-frame cost until paint grows a clipped
//! raster, which is the follow-up this module's doc exists to keep honest.

use vieww_gpu::PixelRect;

/// How a frame's rows divide between the CPU and the GPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Bands {
    /// Rows `[0, cpu_rows)` come from the CPU; the rest from the GPU.
    pub cpu_rows: u32,
}

impl Bands {
    /// The band split for a frame `height` tall, with the CPU's share
    /// `cpu_fraction` of the predicted work — from
    /// [`ShadowPlan::mixed_cpu_fraction`](vieww_render_planner::ShadowPlan::mixed_cpu_fraction),
    /// which never returns a fraction outside `[0.05, 0.95]`. The clamp
    /// here is the backstop, not the policy: a band of zero rows on either
    /// side is a whole-frame executor wearing a split's name.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a rounded fraction of a frame height, clamped to the frame"
    )]
    pub(crate) fn from_fraction(height: u32, cpu_fraction: f32) -> Self {
        let fraction = if cpu_fraction.is_finite() {
            cpu_fraction.clamp(0.0, 1.0)
        } else {
            0.5
        };
        let rows = (f64::from(height) * f64::from(fraction)).round() as u32;
        Self {
            cpu_rows: rows.min(height),
        }
    }

    /// The GPU's band, as a damage rectangle — `None` when the GPU's band
    /// is empty (an all-CPU split, which the fraction's floor makes a
    /// degenerate plan, but a caller may still ask).
    #[must_use]
    #[expect(clippy::cast_possible_wrap, reason = "frame coordinates")]
    pub(crate) fn gpu_band(&self, width: u32, height: u32) -> Option<PixelRect> {
        if self.cpu_rows >= height {
            return None;
        }
        Some(PixelRect {
            x0: 0,
            y0: self.cpu_rows as i32,
            x1: width as i32,
            y1: height as i32,
        })
    }
}

/// One frame out of two: the CPU's rows `[0, bands.cpu_rows)` and the GPU's
/// rows below them, into a fresh straight-alpha RGBA8 buffer.
///
/// Both inputs are whole frames at `width x height` — the CPU's full
/// raster, the GPU's readback (whose non-band rows are the renderer's
/// retained previous frame and are simply not used).
#[must_use]
pub(crate) fn merge(cpu: &[u8], gpu: &[u8], bands: &Bands, width: u32, height: u32) -> Vec<u8> {
    let row = width as usize * 4;
    debug_assert_eq!(cpu.len(), row * height as usize, "the CPU frame is whole");
    debug_assert_eq!(gpu.len(), row * height as usize, "the GPU frame is whole");
    let mut out = vec![0u8; row * height as usize];
    let split = (bands.cpu_rows as usize).min(height as usize) * row;
    out[..split].copy_from_slice(&cpu[..split]);
    out[split..].copy_from_slice(&gpu[split..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_half_frame_splits_half_its_rows() {
        let bands = Bands::from_fraction(100, 0.5);
        assert_eq!(bands.cpu_rows, 50);
        assert_eq!(
            bands.gpu_band(80, 100),
            Some(PixelRect {
                x0: 0,
                y0: 50,
                x1: 80,
                y1: 100
            })
        );
    }

    #[test]
    fn the_fraction_never_empties_a_band_it_cannot_vouch_for() {
        // The planner's own fraction is clamped to [0.05, 0.95]; this is the
        // backstop for a hand-rolled number.
        assert_eq!(Bands::from_fraction(100, 0.0).cpu_rows, 0);
        assert_eq!(Bands::from_fraction(100, 1.0).cpu_rows, 100);
        assert_eq!(Bands::from_fraction(100, f32::NAN).cpu_rows, 50);
        // The floor and ceiling survive a frame too short to split evenly.
        assert_eq!(Bands::from_fraction(1, 0.05).cpu_rows, 0);
        assert_eq!(Bands::from_fraction(3, 0.95).cpu_rows, 3);
    }

    #[test]
    fn an_empty_gpu_band_is_no_damage_not_a_zero_height_rect() {
        assert_eq!(Bands::from_fraction(100, 1.0).gpu_band(80, 100), None);
        // A zero-height damage rectangle would be a full redraw wearing a
        // band's name; `None` is the caller's "redraw nothing extra".
        assert_eq!(Bands::from_fraction(0, 0.5).gpu_band(80, 0), None);
    }

    #[test]
    fn the_merge_takes_each_side_s_rows_and_only_those() {
        let (width, height) = (4u32, 6u32);
        // Every CPU pixel encodes its row; every GPU pixel its row plus 100.
        let cpu: Vec<u8> = (0..height * width)
            .flat_map(|i| {
                let row = (i / width) as u8;
                [row, row, row, 255]
            })
            .collect();
        let gpu: Vec<u8> = (0..height * width)
            .flat_map(|i| {
                let v = (i / width) as u8 + 100;
                [v, v, v, 255]
            })
            .collect();
        let bands = Bands::from_fraction(height, 0.5);
        let merged = merge(&cpu, &gpu, &bands, width, height);

        // Rows 0..3 from the CPU, rows 3..6 from the GPU — and the join is
        // byte-exact on both sides of it.
        for y in 0..height as usize {
            for x in 0..width as usize {
                let index = (y * width as usize + x) * 4;
                let expect = if y < bands.cpu_rows as usize {
                    (y as u8, y as u8)
                } else {
                    ((y as u8) + 100, (y as u8) + 100)
                };
                assert_eq!(
                    &merged[index..index + 4],
                    &[expect.0, expect.1, expect.0, 255],
                    "pixel ({x}, {y})"
                );
            }
        }
    }
}
