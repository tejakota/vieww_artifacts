//! What each swapchain image holds, so a frame copies into it only what
//! changed since that image was last written — and the evidence, gathered on
//! the device itself, that doing so is safe there.
//!
//! # Why
//!
//! A GPU frame ends with one full-screen copy of the frame target `L[0]`
//! (RGBA16F) into the swapchain image (8-bit): on the LG Stylo 5 that copy
//! was 2.5 ms of an 11.7 ms GPU frame (Device Farm run 39594824376), the
//! second-largest thing the GPU did, for a frame whose damage was a moving
//! band and a counter. `L[0]` is already kept between frames and redrawn
//! only where damaged; the copy was the one step still paying for the whole
//! screen.
//!
//! # The catch, and how it is handled
//!
//! An image is written every second or third frame, not every frame, so a
//! partial copy into image *i* has to cover every change since *i* was last
//! written — EGL's "buffer age". [`PresentHistory`] keeps the frames'
//! changes and which frame last wrote each image, and unions what is
//! needed.
//!
//! And it relies on the image still holding what was last copied into it
//! when it comes back from the presentation engine. Real swapchains keep
//! their buffers (Android's are gralloc buffers on a `BufferQueue`, the
//! same ones EGL's buffer age is defined over), but that is the driver's
//! behaviour rather than a guarantee this code can lean on blind. So it is
//! **measured on the device before it is trusted**:
//!
//! 1. *Probing.* Every frame copies in full, and also reads the image back
//!    twice: before writing (what the image kept) and after (what it now
//!    holds). The first is hashed and compared with the second's hash from
//!    the last frame that wrote the same image.
//! 2. *Trusted* after [`PROBES`] such comparisons all matched: frames copy
//!    only the union of changes. Reads stop.
//! 3. *Refused* on any mismatch: full copies for the rest of the process,
//!    and the report says so.
//!
//! The swapchain is created for this (`clipped` off, so an image owns every
//! pixel even where another window covered it; `TRANSFER_SRC` usage, so it
//! can be read) and its generation changes whenever anything else writes an
//! image — a CPU frame, an abandoned image — which empties the history.
//!
//! With `VIEWW_PARTIAL_BLIT_CHECK=1` (the Device Farm passes) the reads
//! never stop, and each written image is also compared with the frame
//! itself, pixel by pixel: a trail left by a partial copy is a failed check
//! rather than something a person has to spot in a screenshot.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

use vieww_gpu::PixelRect;

/// Comparisons that must all match before partial copies are trusted: two
/// per image of a three-image swapchain.
pub(super) const PROBES: u32 = 6;

/// How many frames of changes are kept. An image is rewritten every
/// swapchain-length frames, so this only runs out after frames that wrote
/// no image at all (readback frames) — and then the copy is simply full.
const HISTORY: usize = 32;

/// Above this share of the frame, one full copy beats several partial ones.
const FULL_ABOVE: f64 = 0.6;

/// Where trust in partial copies stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Trust {
    Probing,
    Trusted,
    Refused,
}

/// What a frame does with the swapchain image it writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PresentPlan {
    /// `None`: copy the whole frame. `Some`: only these disjoint rectangles
    /// (possibly none, when nothing changed since the image was written).
    pub(super) region: Option<Vec<PixelRect>>,
    /// Read the image before writing it, to verify what it kept.
    pub(super) read_before: bool,
    /// Read the image after writing it, to know what it holds.
    pub(super) read_after: bool,
    /// Also compare the image with the frame (`VIEWW_PARTIAL_BLIT_CHECK`).
    pub(super) compare_frame: bool,
}

impl PresentPlan {
    pub(super) const FULL: Self = Self {
        region: None,
        read_before: false,
        read_after: false,
        compare_frame: false,
    };
}

/// See the module docs.
#[derive(Debug)]
pub(super) struct PresentHistory {
    generation: u64,
    extent: (u32, u32),
    /// The number of the last frame drawn into `L[0]`.
    frame: u64,
    /// Changes to `L[0]`, oldest first: `None` for a frame drawn in full.
    changes: VecDeque<(u64, Option<Vec<PixelRect>>)>,
    /// Which frame each image index was last written by.
    written: HashMap<u32, u64>,
    /// What each image held after that write, when it was read.
    hashes: HashMap<u32, u64>,
    trust: Trust,
    verified: u32,
    check: bool,
}

impl PresentHistory {
    pub(super) fn new() -> Self {
        Self {
            generation: 0,
            extent: (0, 0),
            frame: 0,
            changes: VecDeque::new(),
            written: HashMap::new(),
            hashes: HashMap::new(),
            trust: Trust::Probing,
            verified: 0,
            check: std::env::var("VIEWW_PARTIAL_BLIT_CHECK").is_ok_and(|v| v == "1"),
        }
    }

    #[cfg(test)]
    fn with_check(check: bool) -> Self {
        Self {
            check,
            ..Self::new()
        }
    }

    pub(super) const fn trust(&self) -> Trust {
        self.trust
    }

    /// A frame was drawn into `L[0]`, changing `change` (`None`: all of
    /// it). Called for every frame, whether or not it reaches a swapchain
    /// image: a frame read back to the CPU changes `L[0]` all the same.
    pub(super) fn record_change(&mut self, change: Option<&[PixelRect]>) {
        self.frame += 1;
        self.changes
            .push_back((self.frame, change.map(<[PixelRect]>::to_vec)));
        while self.changes.len() > HISTORY {
            self.changes.pop_front();
        }
    }

    /// The plan for copying the frame just recorded into image `index`.
    ///
    /// `capable`: the swapchain was made for this. `one_to_one`: the copy
    /// is not a scaled one (a scaled copy filters across a rectangle's edge,
    /// so a partial one would not match).
    pub(super) fn plan(
        &mut self,
        index: u32,
        generation: u64,
        extent: (u32, u32),
        capable: bool,
        one_to_one: bool,
    ) -> PresentPlan {
        if generation != self.generation || extent != self.extent {
            // Something else wrote an image, or these are new images: what
            // any of them holds is unknown.
            self.generation = generation;
            self.extent = extent;
            self.written.clear();
            self.hashes.clear();
        }
        if !capable || !one_to_one || self.trust == Trust::Refused {
            self.written.remove(&index);
            self.hashes.remove(&index);
            return PresentPlan::FULL;
        }
        let last = self.written.insert(index, self.frame);
        let reading = self.trust == Trust::Probing || self.check;
        let read_before = reading && last.is_some() && self.hashes.contains_key(&index);
        if !reading {
            // The hash would go stale with this write.
            self.hashes.remove(&index);
        }
        let region = match (self.trust, last) {
            (Trust::Trusted, Some(last)) => self.changed_since(last, extent),
            _ => None,
        };
        PresentPlan {
            region,
            read_before,
            read_after: reading,
            compare_frame: self.check,
        }
    }

    /// Every change after frame `last` up to the current one, as disjoint
    /// rectangles inside `extent`; `None` when that is everything or more
    /// than [`FULL_ABOVE`] of it, or the history no longer reaches back.
    fn changed_since(&self, last: u64, extent: (u32, u32)) -> Option<Vec<PixelRect>> {
        let oldest = self.changes.front().map_or(u64::MAX, |(frame, _)| *frame);
        if last + 1 < oldest && last < self.frame {
            return None;
        }
        #[expect(clippy::cast_possible_wrap, reason = "frame extents")]
        let bounds = PixelRect {
            x0: 0,
            y0: 0,
            x1: extent.0 as i32,
            y1: extent.1 as i32,
        };
        let mut rects = Vec::new();
        for (frame, change) in &self.changes {
            if *frame <= last {
                continue;
            }
            rects.extend(
                change
                    .as_ref()?
                    .iter()
                    .map(|r| r.intersect(bounds))
                    .filter(|r| !r.is_empty()),
            );
        }
        let rects = super::scene::disjoint(rects);
        let area: u64 = rects
            .iter()
            .map(|r| u64::from(r.width()) * u64::from(r.height()))
            .sum();
        #[expect(clippy::cast_precision_loss, reason = "a ratio of pixel counts")]
        let share = area as f64 / (f64::from(extent.0) * f64::from(extent.1)).max(1.0);
        (share <= FULL_ABOVE).then_some(rects)
    }

    /// What the reads of a frame's image found, once the GPU finished it:
    /// `before`, the hash of what the image held before it was written;
    /// `after`, of what it held after.
    pub(super) fn verified(&mut self, index: u32, before: Option<u64>, after: Option<u64>) {
        if let Some(before) = before {
            if self.hashes.get(&index) == Some(&before) {
                self.verified += 1;
                STATS.probes_ok.fetch_add(1, Ordering::Relaxed);
            } else {
                STATS.probes_bad.fetch_add(1, Ordering::Relaxed);
                self.refuse();
            }
        }
        match after {
            Some(after) if self.trust != Trust::Refused => {
                self.hashes.insert(index, after);
            }
            _ => {
                self.hashes.remove(&index);
            }
        }
        if self.trust == Trust::Probing && self.verified >= PROBES {
            self.trust = Trust::Trusted;
        }
        STATS.set_trust(self.trust);
    }

    /// An image did not match the frame it was meant to show (the check
    /// mode's comparison): stop copying partially.
    pub(super) fn frame_mismatch(&mut self, pixels: u64) {
        STATS.check_bad_pixels.fetch_add(pixels, Ordering::Relaxed);
        if pixels > 0 {
            self.refuse();
            STATS.set_trust(self.trust);
        }
    }

    fn refuse(&mut self) {
        self.trust = Trust::Refused;
        self.hashes.clear();
        self.written.clear();
    }
}

/// A hash of an image's bytes: equal images, equal hashes; a stray pixel
/// changes it. Not cryptographic, and does not need to be.
pub(super) fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0x9E37_79B9_7F4A_7C15 ^ bytes.len() as u64;
    let (words, rest) = bytes.as_chunks::<8>();
    for word in words {
        let w = u64::from_le_bytes(*word);
        h = (h ^ w).wrapping_mul(0x0100_0000_01B3).rotate_left(29);
    }
    for &b in rest {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01B3);
    }
    h
}

/// What the copies into swapchain images have done, process-wide — for a
/// report (`vieww-platform-winit`'s device suite), not for decisions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartialBlitStats {
    /// Frames copied into a swapchain image only where they changed.
    pub partial_frames: u64,
    /// Frames copied in full.
    pub full_frames: u64,
    /// Pixels copied, over every frame.
    pub copied_pixels: u64,
    /// Pixels a full copy of every frame would have copied.
    pub frame_pixels: u64,
    /// Images that kept exactly what was last written to them.
    pub probes_ok: u64,
    /// Images that did not — after which partial copies stop.
    pub probes_bad: u64,
    /// Frames whose image was compared with the frame itself
    /// (`VIEWW_PARTIAL_BLIT_CHECK=1`).
    pub checked_frames: u64,
    /// Pixels, over those frames, that differed by more than rounding.
    pub check_bad_pixels: u64,
    /// `None` until a swapchain made for partial copies presented a frame;
    /// then whether they are being probed, trusted or refused.
    pub state: Option<&'static str>,
}

struct Stats {
    partial_frames: AtomicU64,
    full_frames: AtomicU64,
    copied_pixels: AtomicU64,
    frame_pixels: AtomicU64,
    probes_ok: AtomicU64,
    probes_bad: AtomicU64,
    checked_frames: AtomicU64,
    check_bad_pixels: AtomicU64,
    /// 0 unused, 1 probing, 2 trusted, 3 refused.
    state: AtomicU64,
}

impl Stats {
    fn set_trust(&self, trust: Trust) {
        let state = match trust {
            Trust::Probing => 1,
            Trust::Trusted => 2,
            Trust::Refused => 3,
        };
        self.state.store(state, Ordering::Relaxed);
    }
}

static STATS: Stats = Stats {
    partial_frames: AtomicU64::new(0),
    full_frames: AtomicU64::new(0),
    copied_pixels: AtomicU64::new(0),
    frame_pixels: AtomicU64::new(0),
    probes_ok: AtomicU64::new(0),
    probes_bad: AtomicU64::new(0),
    checked_frames: AtomicU64::new(0),
    check_bad_pixels: AtomicU64::new(0),
    state: AtomicU64::new(0),
};

/// Count a frame's copy into a swapchain image.
pub(super) fn count_copy(plan: &PresentPlan, extent: (u32, u32), trust: Trust, capable: bool) {
    let frame = u64::from(extent.0) * u64::from(extent.1);
    STATS.frame_pixels.fetch_add(frame, Ordering::Relaxed);
    match &plan.region {
        Some(rects) => {
            STATS.partial_frames.fetch_add(1, Ordering::Relaxed);
            let copied: u64 = rects
                .iter()
                .map(|r| u64::from(r.width()) * u64::from(r.height()))
                .sum();
            STATS.copied_pixels.fetch_add(copied, Ordering::Relaxed);
        }
        None => {
            STATS.full_frames.fetch_add(1, Ordering::Relaxed);
            STATS.copied_pixels.fetch_add(frame, Ordering::Relaxed);
        }
    }
    if plan.compare_frame {
        STATS.checked_frames.fetch_add(1, Ordering::Relaxed);
    }
    if capable {
        STATS.set_trust(trust);
    }
}

/// See [`PartialBlitStats`].
#[must_use]
pub fn partial_blit_stats() -> PartialBlitStats {
    let load = |a: &AtomicU64| a.load(Ordering::Relaxed);
    PartialBlitStats {
        partial_frames: load(&STATS.partial_frames),
        full_frames: load(&STATS.full_frames),
        copied_pixels: load(&STATS.copied_pixels),
        frame_pixels: load(&STATS.frame_pixels),
        probes_ok: load(&STATS.probes_ok),
        probes_bad: load(&STATS.probes_bad),
        checked_frames: load(&STATS.checked_frames),
        check_bad_pixels: load(&STATS.check_bad_pixels),
        state: match load(&STATS.state) {
            1 => Some("probing"),
            2 => Some("trusted"),
            3 => Some("refused"),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXTENT: (u32, u32) = (100, 200);

    fn rect(x0: i32, y0: i32, x1: i32, y1: i32) -> PixelRect {
        PixelRect { x0, y0, x1, y1 }
    }

    /// Draw a frame changing `change` and present it to `index`, with the
    /// image keeping whatever it was given (`hash` stands for its content).
    fn frame(h: &mut PresentHistory, index: u32, change: Option<&[PixelRect]>) -> PresentPlan {
        h.record_change(change);
        h.plan(index, 1, EXTENT, true, true)
    }

    /// Probe until trusted, over three images that keep their contents.
    fn trusted() -> PresentHistory {
        let mut h = PresentHistory::with_check(false);
        let mut content = HashMap::new();
        for n in 0..12u64 {
            let index = (n % 3) as u32;
            let probing = h.trust() == Trust::Probing;
            let plan = frame(&mut h, index, Some(&[rect(0, 0, 10, 10)]));
            if probing {
                assert_eq!(plan.region, None, "full copies while probing");
            }
            let before = plan.read_before.then(|| content[&index]);
            content.insert(index, n);
            h.verified(index, before, plan.read_after.then_some(n));
        }
        assert_eq!(h.trust(), Trust::Trusted);
        h
    }

    #[test]
    fn nothing_is_copied_partially_until_the_device_has_shown_it_keeps_images() {
        let mut h = PresentHistory::with_check(false);
        let first = frame(&mut h, 0, Some(&[rect(0, 0, 10, 10)]));
        assert_eq!(
            first,
            PresentPlan {
                region: None,
                read_before: false,
                read_after: true,
                compare_frame: false
            }
        );
        assert_eq!(h.trust(), Trust::Probing);
        let _ = trusted();
    }

    #[test]
    fn an_image_copies_every_change_since_it_was_last_written() {
        let mut h = trusted();
        // Image 0 was last written three frames ago (frame 12 wrote image 2).
        let a = frame(&mut h, 0, Some(&[rect(0, 0, 10, 10)]));
        assert!(!(a.read_before || a.read_after), "reads stop once trusted");
        // Frames 10, 11, 12, 13 changed (0,0,10,10) each: one rectangle.
        assert_eq!(a.region, Some(vec![rect(0, 0, 10, 10)]));
        let b = frame(&mut h, 1, Some(&[rect(50, 50, 60, 60)]));
        assert_eq!(
            b.region.map(|r| r.len()),
            Some(2),
            "image 1 missed frame 13's change as well as carrying frame 14's"
        );
    }

    #[test]
    fn a_full_frame_in_between_makes_the_copy_full() {
        let mut h = trusted();
        h.record_change(None);
        let plan = frame(&mut h, 0, Some(&[rect(0, 0, 10, 10)]));
        assert_eq!(plan.region, None);
    }

    #[test]
    fn a_large_change_is_one_full_copy() {
        let mut h = trusted();
        let plan = frame(&mut h, 0, Some(&[rect(0, 0, 100, 150)]));
        assert_eq!(plan.region, None, "75% of the frame");
    }

    #[test]
    fn something_else_writing_an_image_forgets_them_all() {
        let mut h = trusted();
        h.record_change(Some(&[rect(0, 0, 10, 10)]));
        // A CPU frame was presented: the generation moved on.
        let plan = h.plan(0, 2, EXTENT, true, true);
        assert_eq!(plan.region, None);
        // The next write of the same image can be partial again.
        h.record_change(Some(&[rect(0, 0, 10, 10)]));
        assert_eq!(
            h.plan(0, 2, EXTENT, true, true).region,
            Some(vec![rect(0, 0, 10, 10)])
        );
    }

    #[test]
    fn an_image_that_lost_its_contents_refuses_partial_copies_for_good() {
        let mut h = PresentHistory::with_check(false);
        let plan = frame(&mut h, 0, None);
        h.verified(0, None, plan.read_after.then_some(7));
        let plan = frame(&mut h, 0, None);
        assert!(plan.read_before);
        h.verified(0, Some(8), Some(9));
        assert_eq!(h.trust(), Trust::Refused);
        for _ in 0..10 {
            assert_eq!(
                frame(&mut h, 0, Some(&[rect(0, 0, 1, 1)])),
                PresentPlan::FULL
            );
        }
    }

    #[test]
    fn check_mode_keeps_reading_and_comparing_while_copying_partially() {
        let mut h = PresentHistory::with_check(true);
        let mut content = HashMap::new();
        for n in 0..12u64 {
            let index = (n % 3) as u32;
            let plan = frame(&mut h, index, Some(&[rect(0, 0, 10, 10)]));
            let before = plan.read_before.then(|| content[&index]);
            content.insert(index, n);
            h.verified(index, before, plan.read_after.then_some(n));
        }
        let plan = frame(&mut h, 0, Some(&[rect(0, 0, 10, 10)]));
        assert!(plan.region.is_some() && plan.read_before && plan.read_after && plan.compare_frame);
        h.frame_mismatch(3);
        assert_eq!(h.trust(), Trust::Refused);
    }

    #[test]
    fn a_scaled_copy_or_an_incapable_swapchain_is_always_full() {
        let mut h = trusted();
        h.record_change(Some(&[rect(0, 0, 1, 1)]));
        assert_eq!(h.plan(0, 1, EXTENT, true, false), PresentPlan::FULL);
        h.record_change(Some(&[rect(0, 0, 1, 1)]));
        assert_eq!(h.plan(0, 1, EXTENT, false, true), PresentPlan::FULL);
    }

    #[test]
    fn the_hash_sees_one_changed_byte() {
        let mut bytes = vec![7u8; 4096 + 3];
        let a = hash_bytes(&bytes);
        bytes[2049] = 8;
        assert_ne!(a, hash_bytes(&bytes));
        bytes[2049] = 7;
        assert_eq!(a, hash_bytes(&bytes));
    }
}
