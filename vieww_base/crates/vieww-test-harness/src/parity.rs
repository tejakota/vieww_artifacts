//! The two pixel-parity contracts, named and enforceable.
//!
//! The external architecture review's observation, and the design tension it
//! named: ViewW's commitment to exact rendering parity can conflict with
//! aggressive platform-specific optimization — a CPU-generated coverage mask
//! and a native GPU path may both be visually correct while their
//! antialiased edge pixels differ, and a framework that insists on identical
//! pixels everywhere "may discard optimizations that Flutter or Qt would be
//! willing to use." Its recommendation, adopted here: **maintain two explicit
//! contracts** — a deterministic reference mode for tests and export, and a
//! performance mode with documented perceptual tolerances — so the strongest
//! engineering property (byte-determinism where it matters) stops costing
//! every runtime path the same tax.
//!
//! These are not aspirations; each contract is a number a comparison can
//! fail against:
//!
//! * [`ParityContract::Deterministic`] — the reference contract. The CPU
//!   oracle's bytes, exactly: what a golden fixture, an export, or a
//!   regression test asserts, and what the two-window-same-machine claim
//!   (`assert_partial_repaint_is_complete`) is made under. Zero tolerance,
//!   zero interior exceptions — a single unit of drift on a single edge
//!   pixel is a failure, because a reference that tolerates drift is not a
//!   reference.
//! * [`ParityContract::Performance`] — the runtime contract. Interiors must
//!   agree within a documented per-channel tolerance (the same per-channel
//!   `2` every hardware parity suite in this workspace already uses), and
//!   any disagreement must *hug an edge of the reference itself* — the rule
//!   `vieww-hal`'s Vulkan suite states at length: two rasterizers disagree
//!   by design at exactly one place, the fractional pixels along a shape's
//!   boundary, and nowhere else. A regression that misplaces a shape, drops
//!   it, or paints it the wrong colour moves interior pixels and fails.
//!
//! A backend choosing to run under the performance contract is choosing to
//! be *perceptually* indistinguishable from the oracle rather than
//! byte-identical to it — a decision this type makes explicit, in the call,
//! where it can be reviewed, rather than implicit in a tolerance somebody
//! typed into a comparison once.

use crate::visual::Frame;

/// The per-channel tolerance the performance contract's interiors allow —
/// `2`, the number the Vulkan parity suite, the 3D lit-pass suite and the
/// damage suite all already assert with. Kept here so the contract and the
/// suites it came from cannot drift apart silently.
pub const PERFORMANCE_INTERIOR_TOLERANCE: u8 = 2;

/// How large a step between neighbouring reference pixels counts as "an
/// edge" for the performance contract's edge-hugging rule — the same `8`
/// the hardware suites' `reference_edge` closures use. A disagreement on a
/// gradient this steep is an antialiasing disagreement; one on anything
/// flatter is a moved or miscolored shape.
pub const PERFORMANCE_EDGE_GRADIENT: u8 = 8;

/// Which pixel-parity contract a comparison is made under — see the
/// [module docs](self) for why there are two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParityContract {
    /// The reference contract: byte-identical to the CPU oracle. For golden
    /// fixtures, exports, and determinism tests.
    Deterministic,
    /// The performance contract: interiors within
    /// [`PERFORMANCE_INTERIOR_TOLERANCE`] per channel, disagreements only
    /// where the reference itself has an edge. For runtime backends whose
    /// antialiasing is allowed to be its own computation.
    Performance {
        /// The per-channel interior tolerance. [`Self::performance()`]
        /// fills in this workspace's documented default.
        per_channel_tolerance: u8,
    },
}

impl ParityContract {
    /// The deterministic reference contract, tolerance zero.
    #[must_use]
    pub const fn reference() -> Self {
        Self::Deterministic
    }

    /// The performance contract with this workspace's documented tolerance
    /// ([`PERFORMANCE_INTERIOR_TOLERANCE`]) — the number the hardware
    /// parity suites already enforce.
    #[must_use]
    pub const fn performance() -> Self {
        Self::Performance {
            per_channel_tolerance: PERFORMANCE_INTERIOR_TOLERANCE,
        }
    }

    /// The per-channel tolerance this comparison allows in a frame's
    /// interiors. Zero for the reference contract.
    #[must_use]
    pub const fn tolerance(&self) -> u8 {
        match *self {
            Self::Deterministic => 0,
            Self::Performance { per_channel_tolerance } => per_channel_tolerance,
        }
    }

    /// Assert `actual` satisfies this contract against `reference`, naming
    /// `what` in the failure message.
    ///
    /// # Panics
    ///
    /// Under [`Self::Deterministic`], if any byte differs. Under
    /// [`Self::Performance`], if any pixel differs by more than the
    /// tolerance *and* is not on an edge of the reference (the
    /// edge-hugging rule: a disagreement where the reference is flat is a
    /// moved or recolored shape, not an antialiasing difference), or if the
    /// frames are not the same size.
    pub fn assert_matches(&self, actual: &Frame, reference: &Frame, what: &str) {
        match *self {
            Self::Deterministic => {
                let diffs = reference.differences(actual, 0);
                assert!(
                    diffs.is_empty(),
                    "{what}: the deterministic reference contract allows no drift, \
                     {} pixel(s) differ (first at ({}, {}): {:?} vs {:?})",
                    diffs.len(),
                    diffs.first().map_or(0, |d| d.x),
                    diffs.first().map_or(0, |d| d.y),
                    diffs.first().map_or((0, 0, 0, 0), |d| d.left),
                    diffs.first().map_or((0, 0, 0, 0), |d| d.right),
                );
            }
            Self::Performance { per_channel_tolerance } => {
                let offenders = interior_mismatches(actual, reference, per_channel_tolerance);
                assert!(
                    offenders.is_empty(),
                    "{what}: {offender_count} interior pixel(s) differ by more than \
                     {per_channel_tolerance} per channel under the performance contract \
                     (first at ({first_x}, {first_y}))",
                    offender_count = offenders.len(),
                    first_x = offenders.first().map_or(0, |p| p.0),
                    first_y = offenders.first().map_or(0, |p| p.1),
                );
            }
        }
    }
}

/// The pixels that violate the performance contract: differing by more than
/// `tolerance` in any channel *while sitting on flat reference ground* —
/// every neighbour within one pixel, including diagonals, is within
/// [`PERFORMANCE_EDGE_GRADIENT`] of the reference's own pixel there. This is
/// the hardware suites' `interior_mismatches`/`reference_edge` rule, lifted
/// into the harness so the contract and its suites read the same.
fn interior_mismatches(actual: &Frame, reference: &Frame, tolerance: u8) -> Vec<(u32, u32)> {
    let (w, h) = (reference.width(), reference.height());
    if actual.width() != w || actual.height() != h {
        return vec![(0, 0)];
    }
    let apart = |a: (u8, u8, u8, u8), b: (u8, u8, u8, u8)| {
        a.0.abs_diff(b.0) > tolerance
            || a.1.abs_diff(b.1) > tolerance
            || a.2.abs_diff(b.2) > tolerance
            || a.3.abs_diff(b.3) > tolerance
    };
    let mut out = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let here = actual.at(x, y);
            if !apart(here, reference.at(x, y)) {
                continue;
            }
            let on_reference_edge = (-1_i64..=1).any(|dy| {
                (-1_i64..=1).any(|dx| {
                    let (nx, ny) = (i64::from(x) + dx, i64::from(y) + dy);
                    if nx < 0 || ny < 0 || nx >= i64::from(w) || ny >= i64::from(h) {
                        return false;
                    }
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "bounds-checked coordinates"
                    )]
                    let n = reference.at(nx as u32, ny as u32);
                    let c = reference.at(x, y);
                    c.0.abs_diff(n.0) > PERFORMANCE_EDGE_GRADIENT
                        || c.1.abs_diff(n.1) > PERFORMANCE_EDGE_GRADIENT
                        || c.2.abs_diff(n.2) > PERFORMANCE_EDGE_GRADIENT
                        || c.3.abs_diff(n.3) > PERFORMANCE_EDGE_GRADIENT
                })
            });
            if !on_reference_edge {
                out.push((x, y));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame from raw pixels, the tests' own constructor.
    fn frame(w: u32, h: u32, paint: impl Fn(u32, u32) -> [u8; 4]) -> Frame {
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&paint(x, y));
            }
        }
        Frame::from_rgba(w, h, data)
    }

    #[test]
    fn the_reference_contract_passes_identical_frames_and_nothing_else() {
        let reference = frame(4, 4, |x, _| [if x < 2 { 0 } else { 255 }; 4]);
        ParityContract::reference().assert_matches(&reference, &reference, "identical");
        // One unit of drift on one antialiased edge pixel: the exact
        // difference two rasterizers of identical geometry can produce, and
        // exactly what a reference must not learn to tolerate.
        let mut drifted = reference.data().to_vec();
        drifted[4] = 1; // pixel (1, 0), the edge column, blue +1
        let drifted = Frame::from_rgba(4, 4, drifted);
        let diffs = reference.differences(&drifted, 0);
        assert_eq!(diffs.len(), 1);
        assert!(std::panic::catch_unwind(|| {
            ParityContract::reference().assert_matches(&drifted, &reference, "drifted")
        })
        .is_err());
    }

    #[test]
    fn the_performance_contract_allows_edge_disagreement_and_nothing_else() {
        // A hard vertical edge at x == 2: the reference's own gradient there
        // is 255, an edge by any threshold.
        let reference = frame(8, 8, |x, _| [if x < 2 { 0 } else { 255 }; 4]);
        // A unit of drift on the edge passes; so does three units — the
        // kind of coverage difference a GPU's own antialiasing produces.
        for drift in [1_u8, 3, 40] {
            let mut actual = reference.data().to_vec();
            // Pixel (2, 3): the edge column.
            actual[(3 * 8 + 2) * 4] = 255 - drift;
            let actual = Frame::from_rgba(8, 8, actual);
            ParityContract::performance().assert_matches(&actual, &reference, "edge drift");
        }
        // The same drift one column inside the flat region is a moved
        // shape, and fails: the reference is flat there (gradient 0).
        let mut actual = reference.data().to_vec();
        actual[(3 * 8 + 4) * 4] = 200;
        let actual = Frame::from_rgba(8, 8, actual);
        assert!(std::panic::catch_unwind(|| {
            ParityContract::performance().assert_matches(&actual, &reference, "interior drift")
        })
        .is_err());
    }

    #[test]
    fn the_performance_tolerance_is_the_number_the_suites_use() {
        assert_eq!(ParityContract::performance().tolerance(), 2);
        assert_eq!(
            ParityContract::Performance { per_channel_tolerance: 5 }.tolerance(),
            5
        );
        assert_eq!(ParityContract::reference().tolerance(), 0);
    }

    #[test]
    fn mismatched_sizes_fail_both_contracts() {
        let small = frame(2, 2, |_, _| [0, 0, 0, 255]);
        let big = frame(4, 4, |_, _| [0, 0, 0, 255]);
        for contract in [ParityContract::reference(), ParityContract::performance()] {
            let result = std::panic::catch_unwind(|| contract.assert_matches(&small, &big, "sizes"));
            assert!(result.is_err());
        }
    }
}
