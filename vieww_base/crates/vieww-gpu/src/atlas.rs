//! A glyph atlas: many glyphs' coverage in one texture, so a whole screen of
//! text is one draw call.
//!
//! # Why an atlas at all
//!
//! A glyph is a small alpha bitmap. Uploading one texture per glyph would mean
//! a texture bind — and therefore a draw call — per glyph, and a line of code
//! in `viewwstudio`'s editor is a few hundred glyphs. The entire reason
//! [`ScenePlan`](crate::ScenePlan) batches is that nothing about the pipeline
//! changes between one shape and the next; a per-glyph texture would undo that
//! for the most common content on the screen.
//!
//! So every glyph goes into one texture, and the quad that draws it carries
//! the texture coordinates of its patch. Text then batches exactly like
//! everything else, and a screen of prose is one `draw_indexed`.
//!
//! # The white texel, and why solid fills sample it
//!
//! This atlas reserves texel `(0, 0)` and fills it with full coverage. Solid
//! geometry — every rectangle, path and stroke in the frame — is emitted with
//! all four of its vertices' UVs pointing at that texel.
//!
//! That is what lets **one** pipeline and **one** shader draw both. The
//! fragment shader is `color.a *= atlas(uv)` unconditionally: a glyph gets its
//! coverage, and a rectangle gets `1.0` and comes out exactly as it did before
//! this module existed. The alternative — two pipelines, or a branch on a
//! per-vertex "is this text" flag — costs a pipeline bind between a panel and
//! the label on it, which is to say between almost every pair of adjacent
//! draws in a real interface.
//!
//! A shelf allocator never reuses `(0, 0)` for a glyph because the first shelf
//! starts past it; see [`Atlas::new`].
//!
//! # Shelf packing, and why not something cleverer
//!
//! Glyphs are packed into horizontal shelves: a row of a fixed height, filled
//! left to right, and a new shelf opened above when the current one runs out.
//! It is the classic choice for glyph atlases (FreeType's own demo packer,
//! Skia's, and every text renderer that has published its packer) for a reason
//! that is specific to this content: glyphs from one run are nearly the same
//! height, so a shelf wastes very little, and the packer is O(1) per insert
//! with no bookkeeping to get wrong.
//!
//! Skyline or MaxRects packing wins on heterogeneous rectangles. Text is not
//! heterogeneous, and the cost of being wrong here is not wasted memory, it is
//! a subtle mispacking that puts one glyph's rim inside another's patch — a
//! bug that looks like a font rendering artefact and is not one.
//!
//! # Growth, eviction, and what happens when both stop
//!
//! The texture starts small and doubles when a glyph will not fit, up to
//! [`Atlas::MAX_SIDE`]. Doubling reflows every existing entry, which is why
//! [`Atlas::version`] exists: a backend re-uploads when the version changes
//! rather than trying to track individual texels.
//!
//! Past the maximum, [`Atlas::insert`] returns `None` and the planner records
//! the glyph as unsupported — the same honest gap every other unplannable
//! command gets. That frame is gone; the one after it is not. The next
//! [`Atlas::begin_frame`] sees the atlas is full and **compacts**: every glyph
//! used in the frame just ended is copied out and re-placed, everything else
//! is dropped, and the frame that only failed for lack of room now plans.
//! Evicting *mid-frame* is still forbidden — it would invalidate UVs already
//! written into this frame's vertex buffer, so a frame that drew nine of a
//! word's ten glyphs would get a tenth sampling stale coordinates, which is
//! worse than the CPU fallback it got instead. Between frames there is no
//! such hazard: last frame's vertices are gone, and this frame has written
//! none. A frame whose glyphs are *all* still live finds nothing to free and
//! keeps failing — correctly, because a working set that genuinely needs more
//! than 4096×4096 of distinct glyph coverage is a leak, not a cache miss.

use std::collections::HashMap;

/// Everything about a rasterised glyph that decides which texels it owns.
///
/// The same identity `vieww_paint`'s glyph raster cache keys on — font, id,
/// size, transform and sub-pixel phase — because two glyphs that share it are
/// the same bitmap and must share one patch, and two that do not are different
/// pictures and must not.
///
/// Carried as raw bits rather than floats so the key can be `Hash` and `Eq`,
/// and because bit equality is the equality actually wanted: two transforms
/// differing in the last bit can rasterise differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AtlasKey {
    pub font: u64,
    pub glyph: u16,
    pub size: u32,
    pub transform: [u32; 4],
    pub phase: (u32, u32),
}

/// Where one glyph lives in the atlas.
///
/// UVs are normalised `0..1` and name the patch's **edges**, not its texel
/// centres. The shader samples with nearest filtering and the quad is placed at
/// exactly the bitmap's device size, so each fragment lands on exactly one
/// texel and there is no filtering error to reason about — which is what makes
/// pixel-for-pixel parity with the CPU rasterizer achievable rather than
/// approximate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtlasSlot {
    /// Texel top-left inside the atlas. Stable for the atlas's lifetime.
    pub x: u32,
    pub y: u32,
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    /// Patch width in texels, which is also the quad's width in device pixels.
    pub width: u32,
    pub height: u32,
}

/// A growable single-channel coverage atlas.
pub struct Atlas {
    side: u32,
    texels: Vec<u8>,
    entries: HashMap<AtlasKey, Placement>,
    /// Where the next glyph goes on the current shelf.
    pen_x: u32,
    /// The current shelf's top row.
    shelf_y: u32,
    /// The tallest glyph on the current shelf — how far up the next shelf goes.
    shelf_height: u32,
    version: u64,
    /// Which frame `get` and `insert` are currently stamping. A frame is one
    /// `plan`; see [`begin_frame`](Self::begin_frame).
    frame: u32,
    /// Set when an insert failed for lack of room at
    /// [`MAX_SIDE`](Self::MAX_SIDE) — the compaction trigger, read at the
    /// *next* frame boundary rather than acted on now.
    full: bool,
}

/// A packed glyph's texel rectangle, kept so a resize can reflow it.
#[derive(Debug, Clone, Copy)]
struct Placement {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    /// The frame this glyph was last used in — the whole of the eviction
    /// decision. A glyph drawn every frame is never a candidate; one drawn
    /// once and scrolled away is.
    last_used: u32,
}

impl std::fmt::Debug for Atlas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Atlas")
            .field("side", &self.side)
            .field("glyphs", &self.entries.len())
            .field("version", &self.version)
            .finish()
    }
}

impl Default for Atlas {
    fn default() -> Self {
        Self::new()
    }
}

impl Atlas {
    /// The side the atlas starts at.
    ///
    /// 256x256 is 64 KiB and holds a few hundred small glyphs — a whole
    /// interface's chrome, at one size, without a single resize. Starting
    /// larger wastes upload bandwidth on the common case of an application
    /// that draws one font at two sizes.
    pub const INITIAL_SIDE: u32 = 256;

    /// The largest atlas this will grow to, per axis.
    ///
    /// 4096 is the `maxImageDimension2D` floor every Vulkan implementation
    /// must meet and the same floor D3D11 feature level 11 and Metal family 1
    /// guarantee, so an atlas that fits here fits everywhere this framework
    /// targets. A backend with a larger limit gains nothing: past this, the
    /// working set is not glyph coverage, it is a leak.
    pub const MAX_SIDE: u32 = 4096;

    /// Padding, in texels, between packed glyphs.
    ///
    /// One texel. With nearest sampling and an exact 1:1 quad this is not
    /// strictly required — no fragment can reach a neighbour's texels — but it
    /// costs almost nothing and it is the difference between "a mispacking is
    /// invisible until someone enables filtering" and "a mispacking is
    /// impossible". The white texel at the origin is protected by the same
    /// gap.
    const PAD: u32 = 1;

    #[must_use]
    pub fn new() -> Self {
        let side = Self::INITIAL_SIDE;
        let mut atlas = Self {
            side,
            texels: vec![0; (side * side) as usize],
            entries: HashMap::new(),
            pen_x: 0,
            shelf_y: 0,
            shelf_height: 0,
            version: crate::generation::next(),
            frame: 0,
            full: false,
        };
        atlas.write_white_texel();
        // The first shelf starts below the white texel's row, so no glyph can
        // ever be packed over it.
        atlas.shelf_y = 1 + Self::PAD;
        atlas
    }

    /// Full coverage at `(0, 0)` — see the module doc.
    fn write_white_texel(&mut self) {
        self.texels[0] = u8::MAX;
    }

    /// The UV of the reserved full-coverage texel, for solid geometry.
    ///
    /// Its centre, not its corner: with nearest filtering a coordinate exactly
    /// on a texel boundary is a tie, and which side of it the hardware picks is
    /// not something this code should be relying on.
    #[must_use]
    pub fn white_uv(&self) -> [f32; 2] {
        let half = 0.5 / self.side as f32;
        [half, half]
    }

    /// The atlas texture's side, in texels. Always square.
    #[must_use]
    pub const fn side(&self) -> u32 {
        self.side
    }

    /// The coverage texels, row-major, one byte each — an `R8_UNORM` upload.
    #[must_use]
    pub fn texels(&self) -> &[u8] {
        &self.texels
    }

    /// Bumped whenever a texel or the size changed.
    ///
    /// A backend keeps the version it last uploaded and re-uploads when this
    /// differs. Cheaper schemes exist (dirty rectangles, a ring of staging
    /// buffers) and none of them is worth anything until a profile says the
    /// upload is a cost: a full 4096x4096 atlas is 16 MB, and a *steady* frame
    /// re-uploads nothing at all because no new glyph was rasterised.
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// How many distinct glyphs are packed.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Where `key` lives, if it is already packed.
    ///
    /// Counts as a use in the current frame: a glyph still being drawn is
    /// never a compaction candidate. That stamp is a mutation of cache state,
    /// which is why this takes `&mut self` — a read that changes nothing is
    /// a read of a dead cache.
    pub fn get(&mut self, key: &AtlasKey) -> Option<AtlasSlot> {
        let frame = self.frame;
        let side = self.side;
        self.entries.get_mut(key).map(|p| {
            p.last_used = frame;
            slot_at(*p, side)
        })
    }

    /// Start a frame.
    ///
    /// Marks the boundary that makes eviction safe: everything planned
    /// *before* this call used the coordinates it was given, and nothing has
    /// been given any since. Bumps the frame counter — which is what `get`
    /// and `insert` stamp, and what compaction keeps — and, if the previous
    /// frame left the atlas full, compacts: re-place the glyphs used in that
    /// frame, drop the rest, clear the full flag. See the module doc for why
    /// eviction happens here and nowhere else.
    ///
    /// [`Planner::plan`](crate::Planner::plan) calls this, so an atlas held
    /// by a planner never needs it called by hand; the call is public because
    /// the atlas is and a hand-held one deserves the same safety.
    pub fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        if !self.full {
            return;
        }
        let previous = self.frame.wrapping_sub(1);

        // Copy out the live set — used in the frame just ended — before the
        // reset destroys the texels. Sorted by position, which is the order
        // they were originally packed in: shelf packing is order-deterministic,
        // so re-placing in this order lands the live set in a layout as tight
        // as (in fact tighter than) the one it came from, and the `place` in
        // the loop below cannot fail for a set that already fit.
        let mut keep: Vec<(AtlasKey, Placement, Vec<u8>)> = self
            .entries
            .iter()
            .filter(|(_, p)| p.last_used == previous)
            .map(|(k, p)| {
                let mut bytes = vec![0u8; (p.width * p.height) as usize];
                for row in 0..p.height {
                    let from = ((p.y + row) * self.side + p.x) as usize;
                    let to = (row * p.width) as usize;
                    let n = p.width as usize;
                    bytes[to..to + n].copy_from_slice(&self.texels[from..from + n]);
                }
                (*k, *p, bytes)
            })
            .collect();

        if keep.len() == self.entries.len() {
            // Everything is live. The working set genuinely exceeds the atlas:
            // compaction would free nothing, so the atlas stays full and the
            // honest `insert` failure stands.
            return;
        }
        keep.sort_unstable_by_key(|(_, p, _)| (p.y, p.x));

        // Reset at the *current* side, not the initial one: the live set has
        // already demanded this much texture once, and growing back to it a
        // frame later would re-upload four times to arrive at the same place.
        let side = self.side;
        self.texels = vec![0; (side * side) as usize];
        self.write_white_texel();
        self.entries.clear();
        self.entries.reserve(keep.len());
        self.pen_x = 0;
        self.shelf_y = 1 + Self::PAD;
        self.shelf_height = 0;
        for (key, placement, bytes) in keep {
            let Some(replaced) = self.try_allocate(placement.width, placement.height) else {
                // Unreachable for a set that fit before — see the comment on
                // the sort — but a cache must never *depend* on an invariant
                // it cannot check: a glyph that cannot be re-placed is
                // dropped, which is a miss, not a corruption.
                continue;
            };
            self.blit(replaced, &bytes);
            self.entries.insert(
                key,
                Placement {
                    last_used: placement.last_used,
                    ..replaced
                },
            );
        }
        self.full = false;
        self.version = crate::generation::next();
    }

    /// Pack `alpha` — `width * height` coverage bytes, row-major — under
    /// `key`, or return the slot it already occupies.
    ///
    /// `None` when the glyph cannot be packed even after growing to
    /// [`MAX_SIDE`](Self::MAX_SIDE); see the module doc for why that is
    /// reported to this frame rather than resolved by eviction — and why the
    /// frame after it gets a compacted atlas instead of the same failure.
    pub fn insert(
        &mut self,
        key: AtlasKey,
        width: u32,
        height: u32,
        alpha: &[u8],
    ) -> Option<AtlasSlot> {
        if let Some(existing) = self.entries.get_mut(&key) {
            existing.last_used = self.frame;
            let placed = *existing;
            let side = self.side;
            return Some(slot_at(placed, side));
        }
        if width == 0 || height == 0 {
            return None;
        }
        debug_assert_eq!(
            alpha.len(),
            (width as usize) * (height as usize),
            "coverage buffer must be exactly width * height"
        );

        let mut placement = self.allocate(width, height)?;
        placement.last_used = self.frame;
        self.blit(placement, alpha);
        self.entries.insert(key, placement);
        self.version = crate::generation::next();
        Some(self.slot(placement))
    }

    /// Find room for `width` x `height`, growing if the current side cannot
    /// hold it.
    fn allocate(&mut self, width: u32, height: u32) -> Option<Placement> {
        loop {
            if let Some(placement) = self.try_allocate(width, height) {
                return Some(placement);
            }
            // A single glyph larger than the maximum atlas is not a packing
            // failure that growing can fix, and the loop must not spin on it.
            if width > Self::MAX_SIDE || height > Self::MAX_SIDE {
                return None;
            }
            if self.side >= Self::MAX_SIDE {
                // Out of room at the ceiling. This frame's insert fails
                // honestly; the flag is what makes the *next* frame's
                // `begin_frame` compact rather than fail the same way.
                self.full = true;
                return None;
            }
            self.grow();
        }
    }

    /// One shelf-packing attempt at the current size.
    fn try_allocate(&mut self, width: u32, height: u32) -> Option<Placement> {
        if width > self.side {
            return None;
        }
        // Does it fit on the current shelf?
        if self.pen_x + width > self.side {
            // No — open a new one above.
            let next_y = self.shelf_y + self.shelf_height + Self::PAD;
            if next_y + height > self.side {
                return None;
            }
            self.shelf_y = next_y;
            self.pen_x = 0;
            self.shelf_height = 0;
        }
        if self.shelf_y + height > self.side {
            return None;
        }
        let placement = Placement {
            x: self.pen_x,
            y: self.shelf_y,
            width,
            height,
            last_used: 0,
        };
        self.pen_x += width + Self::PAD;
        self.shelf_height = self.shelf_height.max(height);
        Some(placement)
    }

    /// Double the side, keeping every glyph at the texel it already had.
    ///
    /// # Why not repack
    ///
    /// This used to repack every glyph into the larger texture, which uses the
    /// new width better. It also moved glyphs that vertices earlier in the
    /// *same frame* already addressed: a frame whose text grew the atlas drew
    /// its first lines sampling whatever landed at their old positions.
    /// A grow happens at most a handful of times in a session; a stable
    /// address is worth the right half of a few old shelves. The shelf being
    /// filled simply continues into the wider row.
    fn grow(&mut self) {
        let old_side = self.side;
        let old_texels = std::mem::take(&mut self.texels);
        self.side = (old_side * 2).min(Self::MAX_SIDE);
        self.texels = vec![0; (self.side * self.side) as usize];
        for row in 0..old_side {
            let from = (row * old_side) as usize;
            let to = (row * self.side) as usize;
            self.texels[to..to + old_side as usize]
                .copy_from_slice(&old_texels[from..from + old_side as usize]);
        }
        self.version = crate::generation::next();
    }

    fn blit(&mut self, at: Placement, alpha: &[u8]) {
        for row in 0..at.height {
            let from = (row * at.width) as usize;
            let to = ((at.y + row) * self.side + at.x) as usize;
            let n = at.width as usize;
            self.texels[to..to + n].copy_from_slice(&alpha[from..from + n]);
        }
    }

    fn slot(&self, at: Placement) -> AtlasSlot {
        slot_at(at, self.side)
    }
}

/// [`Atlas::slot`](Atlas::slot)'s arithmetic, free of `self` so a borrow of
/// `entries` can outlive the call that reads `side`.
fn slot_at(at: Placement, side: u32) -> AtlasSlot {
    let side = side as f32;
    AtlasSlot {
        x: at.x,
        y: at.y,
        u0: at.x as f32 / side,
        v0: at.y as f32 / side,
        u1: (at.x + at.width) as f32 / side,
        v1: (at.y + at.height) as f32 / side,
        width: at.width,
        height: at.height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(glyph: u16) -> AtlasKey {
        AtlasKey {
            font: 1,
            glyph,
            size: 13f32.to_bits(),
            transform: [0; 4],
            phase: (0, 0),
        }
    }

    #[test]
    fn the_white_texel_is_full_coverage_and_nothing_is_packed_over_it() {
        let mut atlas = Atlas::new();
        assert_eq!(atlas.texels()[0], 255);
        for g in 0..200u16 {
            atlas.insert(key(g), 6, 9, &[128; 54]);
        }
        assert_eq!(
            atlas.texels()[0],
            255,
            "the reserved texel survived {} glyphs",
            atlas.len()
        );
    }

    /// The property the whole design rests on: sampling the white texel returns
    /// full coverage, so a solid fill routed through the text shader is
    /// unchanged.
    #[test]
    fn the_white_uv_lands_inside_the_reserved_texel() {
        let atlas = Atlas::new();
        let [u, v] = atlas.white_uv();
        let side = atlas.side() as f32;
        assert!(
            u * side > 0.0 && u * side < 1.0,
            "u lands in texel column 0"
        );
        assert!(v * side > 0.0 && v * side < 1.0, "v lands in texel row 0");
    }

    #[test]
    fn the_same_key_is_packed_once() {
        let mut atlas = Atlas::new();
        let first = atlas.insert(key(7), 4, 4, &[9; 16]).unwrap();
        let version = atlas.version();
        let second = atlas.insert(key(7), 4, 4, &[9; 16]).unwrap();
        assert_eq!(first, second);
        assert_eq!(atlas.len(), 1);
        assert_eq!(
            atlas.version(),
            version,
            "re-inserting an existing key changes nothing, so it must not \
             force a re-upload"
        );
    }

    /// Packed glyphs must not overlap. Asserted by painting each glyph a
    /// distinct value and checking every texel is claimed by at most one — the
    /// failure this catches is one glyph's rim appearing inside another's
    /// patch, which reads as a font bug rather than a packer bug.
    #[test]
    fn packed_glyphs_never_share_a_texel() {
        let mut atlas = Atlas::new();
        let mut slots = Vec::new();
        for g in 1..60u16 {
            let w = 3 + u32::from(g % 7);
            let h = 4 + u32::from(g % 5);
            let value = u8::try_from(g).unwrap();
            let slot = atlas
                .insert(key(g), w, h, &vec![value; (w * h) as usize])
                .unwrap();
            slots.push((value, slot));
        }
        let side = atlas.side();
        for (value, slot) in slots {
            let x0 = (slot.u0 * side as f32).round() as u32;
            let y0 = (slot.v0 * side as f32).round() as u32;
            for row in 0..slot.height {
                for col in 0..slot.width {
                    let texel = atlas.texels()[((y0 + row) * side + x0 + col) as usize];
                    assert_eq!(
                        texel,
                        value,
                        "texel ({}, {}) belongs to glyph {value} and holds {texel}",
                        x0 + col,
                        y0 + row
                    );
                }
            }
        }
    }

    /// Growth must keep every glyph's bytes *and* keep its slot pointing at
    /// them. A resize that reflows without re-blitting is the bug this exists
    /// for, and it shows up as the whole screen's text turning into other
    /// letters.
    #[test]
    fn growing_preserves_every_glyphs_coverage() {
        let mut atlas = Atlas::new();
        let mut written = Vec::new();
        // Enough 30x30 glyphs to overflow a 256x256 atlas several times.
        for g in 1..400u16 {
            let value = u8::try_from(g % 251 + 1).unwrap();
            let Some(_) = atlas.insert(key(g), 30, 30, &vec![value; 900]) else {
                break;
            };
            written.push((key(g), value));
        }
        assert!(
            atlas.side() > Atlas::INITIAL_SIDE,
            "the test needs to have forced at least one growth"
        );
        let side = atlas.side();
        for (k, value) in written {
            let slot = atlas.get(&k).expect("still packed after growth");
            let x0 = (slot.u0 * side as f32).round() as u32;
            let y0 = (slot.v0 * side as f32).round() as u32;
            for row in 0..slot.height {
                for col in 0..slot.width {
                    assert_eq!(
                        atlas.texels()[((y0 + row) * side + x0 + col) as usize],
                        value,
                        "glyph {} lost its coverage across a resize",
                        k.glyph
                    );
                }
            }
        }
    }

    #[test]
    fn a_glyph_larger_than_the_biggest_atlas_is_refused_rather_than_looping() {
        let mut atlas = Atlas::new();
        let side = Atlas::MAX_SIDE + 1;
        assert!(atlas
            .insert(key(1), side, 4, &vec![0; (side * 4) as usize])
            .is_none());
    }

    #[test]
    fn a_zero_sized_glyph_is_not_packed() {
        let mut atlas = Atlas::new();
        assert!(atlas.insert(key(1), 0, 5, &[]).is_none());
        assert!(atlas.is_empty());
    }

    /// UVs must address the patch that was written, at every size the atlas
    /// passes through — an off-by-one in `slot` is text drawn one texel to the
    /// left, which is subtle enough to ship.
    #[test]
    fn a_slots_uv_rectangle_is_exactly_its_texels() {
        let mut atlas = Atlas::new();
        let slot = atlas.insert(key(3), 5, 7, &[200; 35]).unwrap();
        let side = atlas.side() as f32;
        assert!((slot.u1 - slot.u0 - 5.0 / side).abs() < 1e-6);
        assert!((slot.v1 - slot.v0 - 7.0 / side).abs() < 1e-6);
        assert_eq!(slot.width, 5);
        assert_eq!(slot.height, 7);
    }

    // ------------------------------------------------------------- eviction

    /// Enough 128x128 glyphs to fill a 4096x4096 atlas. Growth keeps every
    /// shelf at the width it was opened at — a shelf opened while the atlas
    /// was 512 wide stays 512 wide forever — so the ~930 a fresh 4096 packer
    /// would take becomes about 680, and a thousand inserts is comfortably
    /// past full either way. Each glyph carries a value unique enough that a
    /// texel surviving in the wrong patch is detectable.
    fn fill(atlas: &mut Atlas) -> usize {
        let mut packed = 0;
        for g in 0..1_000u16 {
            let value = u8::try_from(g % 251 + 1).unwrap();
            if atlas
                .insert(key(g), 128, 128, &vec![value; 128 * 128])
                .is_none()
            {
                break;
            }
            packed += 1;
        }
        packed
    }

    /// A deliberately filled atlas: the last insert fails, the side is at the
    /// ceiling, and — the property the whole design rests on — nothing is
    /// evicted *during* the frame, so every glyph planned so far still
    /// resolves.
    #[test]
    fn a_full_atlas_fails_the_insert_but_evicts_nothing_mid_frame() {
        let mut atlas = Atlas::new();
        let packed = fill(&mut atlas);

        assert!(
            packed > 600,
            "the test must fill the atlas, packed {packed}"
        );
        assert_eq!(atlas.side(), Atlas::MAX_SIDE);
        assert!(atlas
            .insert(key(65_535), 128, 128, &[7; 128 * 128])
            .is_none());
        // Mid-frame, the atlas is still a valid cache of everything packed.
        assert!(atlas.get(&key(1)).is_some());
        assert_eq!(atlas.len(), packed);
    }

    /// The frame *after* a full one compacts: glyphs still in use are kept
    /// with their coverage intact, everything else is dropped, and a glyph
    /// that could not be packed now can be. This is the test the worklist
    /// asked for when the atlas could not evict — one that fills the atlas
    /// deliberately rather than waiting for a real application to do it by
    /// accident. The item is closed; `TRACKER.md`'s 2026-10-09 entry has
    /// the evidence, and this test is most of it.
    #[test]
    fn the_frame_after_a_full_one_compacts_away_the_unused() {
        let mut atlas = Atlas::new();
        let packed = fill(&mut atlas);
        let version_before = atlas.version();

        // Frame 1 opens: everything from the filling frame is live, so the
        // compaction has nothing to free and the atlas stays full.
        atlas.begin_frame();
        assert_eq!(atlas.len(), packed, "all-live compaction frees nothing");
        assert!(atlas
            .insert(key(65_535), 128, 128, &[7; 128 * 128])
            .is_none());

        // Frame 1 draws only the glyphs with ids 0..50 — the "user scrolled
        // away from the rest" of the story.
        for g in 0..50u16 {
            atlas.get(&key(g));
        }
        // Frame 2 opens: the stale 600-odd glyphs go, the 50 stay.
        atlas.begin_frame();
        assert_eq!(atlas.len(), 50);
        assert!(atlas.version() > version_before);

        // The glyph that failed twice now packs — the frame that only failed
        // for lack of room gets its room.
        assert!(atlas
            .insert(key(65_535), 128, 128, &[7; 128 * 128])
            .is_some());
        assert_eq!(atlas.len(), 51);

        // Coverage survived the compaction: each kept glyph's patch still
        // holds its own value, in the patch its (new) slot points at.
        let side = atlas.side();
        for g in 0..50u16 {
            let value = u8::try_from(g % 251 + 1).unwrap();
            let slot = atlas.get(&key(g)).expect("kept glyph still packed");
            let x0 = (slot.u0 * side as f32).round() as u32;
            let y0 = (slot.v0 * side as f32).round() as u32;
            for row in 0..slot.height {
                for col in 0..slot.width {
                    assert_eq!(
                        atlas.texels()[((y0 + row) * side + x0 + col) as usize],
                        value,
                        "glyph {g} lost its coverage across the compaction"
                    );
                }
            }
        }

        // And the reserved texel survived the reset.
        assert_eq!(atlas.texels()[0], 255);
    }

    /// A working set that genuinely fills the atlas *and stays live* is not a
    /// cache-miss pattern — it is a leak — and compaction must say so by
    /// leaving the atlas full rather than thrashing it every frame.
    #[test]
    fn an_all_live_full_atlas_stays_full_rather_than_thrashing() {
        let mut atlas = Atlas::new();
        let packed = fill(&mut atlas);

        // Two frames in which every glyph is drawn.
        atlas.begin_frame();
        for g in 0..packed as u16 {
            atlas.get(&key(g));
        }
        atlas.begin_frame();
        for g in 0..packed as u16 {
            atlas.get(&key(g));
        }
        atlas.begin_frame();

        assert_eq!(atlas.len(), packed, "nothing was evictable");
        assert!(atlas
            .insert(key(65_535), 128, 128, &[7; 128 * 128])
            .is_none());
    }

    /// `begin_frame` on a non-full atlas is a counter bump and nothing else:
    /// no version change, no re-upload, no churn — a steady frame must cost
    /// nothing, which is the property that makes proactive compaction the
    /// wrong default here (unlike the mask atlas, whose entries are cheap to
    /// re-derive).
    #[test]
    fn a_begin_frame_on_a_roomy_atlas_changes_nothing() {
        let mut atlas = Atlas::new();
        atlas.insert(key(1), 6, 9, &[54; 54]);
        let version = atlas.version();
        let len = atlas.len();

        atlas.begin_frame();
        assert_eq!(atlas.version(), version);
        assert_eq!(atlas.len(), len);
        assert!(atlas.get(&key(1)).is_some());
    }
}
