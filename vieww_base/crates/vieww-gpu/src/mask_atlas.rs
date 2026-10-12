//! Coverage masks for shaped clips and shadow silhouettes, packed into one
//! `R8` texture.
//!
//! # Why not the glyph atlas
//!
//! A glyph is small, lives for the whole session and is keyed by something
//! that repeats every frame. A clip mask is panel-sized and keyed by where the
//! panel *is* — a list that scrolls produces a new mask every frame. Putting
//! those into [`crate::Atlas`], which never evicts, would fill it in seconds
//! and turn every glyph after that into a coverage gap.
//!
//! So masks get their own texture with a frame-scoped lifetime:
//!
//! - [`MaskAtlas::begin_frame`] starts a frame. If the atlas is more than half
//!   full, it is **compacted**: rebuilt with only the entries the *previous*
//!   frame used. A steady screen keeps all of its masks and uploads nothing.
//! - Within a frame, entries are never moved — vertices already emitted hold
//!   their texel offsets — so a frame that runs out of room grows the texture
//!   (to [`MaskAtlas::MAX_SIDE`]) rather than evicting.
//! - A mask that cannot fit even then is a reported gap, never a dropped clip.
//!
//! # How a draw addresses a mask
//!
//! Masks are fetched with `textureLoad` at an integer texel, not sampled: a
//! fragment at device pixel `p` reads `p - offset`, where
//! `offset = device_origin - atlas_origin`. One fragment, one texel, no
//! filtering — the same exactness argument `crate::atlas` makes for glyphs.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

/// Where a mask was placed, and the device pixel its first texel covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaskSlot {
    /// Device-space top-left of the patch.
    pub device_x: i32,
    pub device_y: i32,
    /// Texel top-left inside the atlas.
    pub atlas_x: u32,
    pub atlas_y: u32,
    pub width: u32,
    pub height: u32,
}

impl MaskSlot {
    /// `device - atlas`: what a fragment subtracts from its pixel position to
    /// find its texel.
    #[must_use]
    #[expect(clippy::cast_precision_loss, reason = "texel coordinates < 2^24")]
    pub fn fetch_offset(&self) -> [f32; 2] {
        [
            (self.device_x - self.atlas_x as i32) as f32,
            (self.device_y - self.atlas_y as i32) as f32,
        ]
    }
}

/// A content key: a hash of everything that determines the mask's bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MaskKey(pub u64);

impl MaskKey {
    /// Hash anything hashable into a key.
    #[must_use]
    pub fn of(value: &impl Hash) -> Self {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        Self(hasher.finish())
    }
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    device_x: i32,
    device_y: i32,
    last_used: u64,
}

/// See the module doc.
pub struct MaskAtlas {
    side: u32,
    texels: Vec<u8>,
    entries: HashMap<MaskKey, Entry>,
    pen_x: u32,
    shelf_y: u32,
    shelf_height: u32,
    used_texels: u64,
    frame: u64,
    version: u64,
    /// Rectangles (`x, y, width, height`) blitted since [`Self::begin_frame`]
    /// — what a texture already holding `dirty_base` needs to catch up. See
    /// [`Self::changes_since`].
    dirty: Vec<[u32; 4]>,
    /// The version the texels had when `dirty` started recording.
    dirty_base: u64,
    /// Set when this frame rewrote the texture wholesale (a compaction or a
    /// grow): `dirty` no longer describes the difference.
    dirty_full: bool,
}

/// What a backend holding some earlier version of a [`MaskAtlas`] must upload
/// to match it — the answer to "which texels changed", so that a mask that is
/// new every frame costs its own bytes instead of the whole texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskChanges<'a> {
    /// Nothing: the texture is current.
    None,
    /// Only these rectangles (`x, y, width, height`, in texels) differ from
    /// the version the backend holds.
    Regions(&'a [[u32; 4]]),
    /// Upload everything — the texture was rebuilt, resized, or the backend's
    /// copy is from before this frame began.
    Full,
}

impl std::fmt::Debug for MaskAtlas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MaskAtlas")
            .field("side", &self.side)
            .field("masks", &self.entries.len())
            .field("version", &self.version)
            .finish()
    }
}

impl Default for MaskAtlas {
    fn default() -> Self {
        Self::new()
    }
}

impl MaskAtlas {
    pub const INITIAL_SIDE: u32 = 256;
    pub const MAX_SIDE: u32 = 4096;
    const PAD: u32 = 1;

    #[must_use]
    pub fn new() -> Self {
        Self {
            side: Self::INITIAL_SIDE,
            texels: vec![0; (Self::INITIAL_SIDE * Self::INITIAL_SIDE) as usize],
            entries: HashMap::new(),
            pen_x: 0,
            shelf_y: 0,
            shelf_height: 0,
            used_texels: 0,
            frame: 1,
            version: 0,
            dirty: Vec::new(),
            dirty_base: 0,
            dirty_full: true,
        }
        .with_fresh_version()
    }

    fn with_fresh_version(mut self) -> Self {
        self.version = crate::generation::next();
        self.dirty_base = self.version;
        self
    }

    /// What a backend whose texture holds `held` (the [`Self::version`] it
    /// last uploaded, `None` if it has never uploaded) must send to match.
    ///
    /// # Why this is exact
    ///
    /// Texels change in exactly three places: a new mask's blit, a
    /// compaction, and a grow. `begin_frame` starts recording with the
    /// version the texels had at that moment; every blit after it appends its
    /// rectangle; a compaction or grow marks the frame as a full rewrite. So a
    /// texture that holds precisely the start-of-frame version differs from
    /// the current texels in the recorded rectangles and nowhere else.
    /// Anything else — a backend that skipped a frame, a first upload, a
    /// rebuilt texture — is [`MaskChanges::Full`], which is always correct.
    #[must_use]
    pub fn changes_since(&self, held: Option<u64>) -> MaskChanges<'_> {
        match held {
            Some(v) if v == self.version => MaskChanges::None,
            Some(v) if v == self.dirty_base && !self.dirty_full => {
                MaskChanges::Regions(&self.dirty)
            }
            _ => MaskChanges::Full,
        }
    }

    #[must_use]
    pub const fn side(&self) -> u32 {
        self.side
    }
    #[must_use]
    pub fn texels(&self) -> &[u8] {
        &self.texels
    }
    /// Changes whenever the texels do; a backend re-uploads on a change.
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Start a frame; compacts if more than half the texture is in use. See
    /// the module doc.
    pub fn begin_frame(&mut self) {
        let previous = self.frame;
        self.frame += 1;
        self.dirty.clear();
        self.dirty_base = self.version;
        self.dirty_full = false;
        // **Compact when packing has advanced far *and* most of it is dead.**
        //
        // Shelf packing never reuses a dead mask's space, so what runs a
        // churning atlas out of room is the packing frontier, not the texels
        // still in use. This used to test only `used_texels * 2 > capacity`,
        // and an animated shape — a new mask every frame — never got there:
        // the frontier hit the bottom first, the atlas *grew* (keeping every
        // dead mask), and kept growing to `MAX_SIDE` with a few dozen masks in
        // it, each grow a full-texture upload. `examples/anim_paths` measured
        // 4096^2 for three moving shapes.
        //
        // The frontier alone is not enough either: right after a compaction
        // the live set can keep it past half on its own, and compacting every
        // frame is a full upload every frame. So the second condition is that
        // at least half of what has been packed belongs to masks the previous
        // frame did not use — a compaction then always frees at least half.
        //
        // Three quarters rather than half for the frontier: a compaction is a
        // whole-texture upload (`changes_since` reports `Full`), and waiting
        // longer halves how often one happens. `anim_paths`, 120 frames: one
        // rotating panel went from 39 full uploads to 20 and 1.62 MB a frame
        // to 1.02 MB on average; three moving shapes 30 -> 21, 2.50 -> 1.91 MB.
        let frontier = self.shelf_y + self.shelf_height;
        if frontier * 4 <= self.side * 3 {
            return;
        }
        let live: u64 = self
            .entries
            .values()
            .filter(|e| e.last_used >= previous)
            .map(|e| u64::from(e.width) * u64::from(e.height))
            .sum();
        if live * 2 > self.used_texels {
            return;
        }
        let keep: Vec<(MaskKey, Entry, Vec<u8>)> = self
            .entries
            .iter()
            .filter(|(_, e)| e.last_used >= previous)
            .map(|(k, e)| (*k, *e, self.copy_out(e)))
            .collect();
        if keep.len() == self.entries.len() {
            // Everything is live: compaction would free nothing.
            return;
        }
        self.reset(self.side);
        self.dirty_full = true;
        for (key, entry, bytes) in keep {
            if let Some(placed) = self.place(entry.width, entry.height, &bytes) {
                self.entries.insert(
                    key,
                    Entry {
                        device_x: entry.device_x,
                        device_y: entry.device_y,
                        last_used: entry.last_used,
                        ..placed
                    },
                );
            }
        }
        self.version = crate::generation::next();
    }

    /// The mask for `key` if it is resident, marked used this frame —
    /// [`Self::get_or_insert`] without the build, for callers whose build
    /// needs to read this atlas.
    pub fn get(&mut self, key: MaskKey) -> Option<MaskSlot> {
        let frame = self.frame;
        let entry = self.entries.get_mut(&key)?;
        entry.last_used = frame;
        Some(slot(entry))
    }

    /// The mask for `key`, building it with `build` on a miss.
    ///
    /// `build` returns `(device_x, device_y, width, height, alpha)`. `None`
    /// when the mask is empty or cannot fit.
    pub fn get_or_insert(
        &mut self,
        key: MaskKey,
        build: impl FnOnce() -> Option<(i32, i32, u32, u32, Vec<u8>)>,
    ) -> Option<MaskSlot> {
        let frame = self.frame;
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_used = frame;
            return Some(slot(entry));
        }
        let (device_x, device_y, width, height, alpha) = build()?;
        if width == 0 || height == 0 {
            return None;
        }
        debug_assert_eq!(alpha.len(), width as usize * height as usize);
        let placed = loop {
            if let Some(placed) = self.place(width, height, &alpha) {
                break placed;
            }
            if self.side >= Self::MAX_SIDE {
                return None;
            }
            self.grow();
        };
        let entry = Entry {
            device_x,
            device_y,
            last_used: frame,
            ..placed
        };
        self.entries.insert(key, entry);
        self.version = crate::generation::next();
        Some(slot(&entry))
    }

    fn copy_out(&self, e: &Entry) -> Vec<u8> {
        let mut out = Vec::with_capacity(e.width as usize * e.height as usize);
        for row in 0..e.height {
            let from = ((e.y + row) * self.side + e.x) as usize;
            out.extend_from_slice(&self.texels[from..from + e.width as usize]);
        }
        out
    }

    fn reset(&mut self, side: u32) {
        self.side = side;
        self.texels = vec![0; (side * side) as usize];
        self.entries.clear();
        self.pen_x = 0;
        self.shelf_y = 0;
        self.shelf_height = 0;
        self.used_texels = 0;
    }

    fn grow(&mut self) {
        let old: Vec<(MaskKey, Entry, Vec<u8>)> = self
            .entries
            .iter()
            .map(|(k, e)| (*k, *e, self.copy_out(e)))
            .collect();
        let side = (self.side * 2).min(Self::MAX_SIDE);
        self.reset(side);
        self.dirty_full = true;
        // Slots handed out earlier this frame are already baked into vertices
        // as texel offsets, so a grow must not move them: every old entry is
        // copied to *its own* texel position in the larger texture, and new
        // packing continues below the lowest of them.
        let mut old = old;
        old.sort_by_key(|(_, e, _)| (e.y, e.x));
        for (key, entry, bytes) in old {
            let placed = self.place_at(entry.x, entry.y, entry.width, entry.height, &bytes);
            self.entries.insert(
                key,
                Entry {
                    device_x: entry.device_x,
                    device_y: entry.device_y,
                    last_used: entry.last_used,
                    ..placed
                },
            );
        }
        self.version = crate::generation::next();
    }

    /// Put a patch at an exact position (used by `grow` to keep slots stable)
    /// and continue packing after the furthest shelf.
    fn place_at(&mut self, x: u32, y: u32, width: u32, height: u32, alpha: &[u8]) -> Entry {
        self.blit(x, y, width, height, alpha);
        // Continue new allocations on a fresh shelf below everything placed.
        self.shelf_y = self.shelf_y.max(y + height + Self::PAD);
        self.pen_x = 0;
        self.shelf_height = 0;
        self.used_texels += u64::from(width) * u64::from(height);
        Entry {
            x,
            y,
            width,
            height,
            device_x: 0,
            device_y: 0,
            last_used: 0,
        }
    }

    fn place(&mut self, width: u32, height: u32, alpha: &[u8]) -> Option<Entry> {
        if width + Self::PAD > self.side || height + Self::PAD > self.side {
            return None;
        }
        if self.pen_x + width + Self::PAD > self.side {
            self.shelf_y += self.shelf_height + Self::PAD;
            self.pen_x = 0;
            self.shelf_height = 0;
        }
        if self.shelf_y + height + Self::PAD > self.side {
            return None;
        }
        let (x, y) = (self.pen_x, self.shelf_y);
        self.pen_x += width + Self::PAD;
        self.shelf_height = self.shelf_height.max(height);
        self.blit(x, y, width, height, alpha);
        self.used_texels += u64::from(width) * u64::from(height);
        Some(Entry {
            x,
            y,
            width,
            height,
            device_x: 0,
            device_y: 0,
            last_used: 0,
        })
    }

    fn blit(&mut self, x: u32, y: u32, width: u32, height: u32, alpha: &[u8]) {
        if !self.dirty_full {
            self.dirty.push([x, y, width, height]);
        }
        for row in 0..height {
            let from = (row * width) as usize;
            let to = ((y + row) * self.side + x) as usize;
            self.texels[to..to + width as usize]
                .copy_from_slice(&alpha[from..from + width as usize]);
        }
    }
}

fn slot(e: &Entry) -> MaskSlot {
    MaskSlot {
        device_x: e.device_x,
        device_y: e.device_y,
        atlas_x: e.x,
        atlas_y: e.y,
        width: e.width,
        height: e.height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert(atlas: &mut MaskAtlas, id: u64, w: u32, h: u32, value: u8) -> Option<MaskSlot> {
        atlas.get_or_insert(MaskKey(id), || {
            Some((10, 20, w, h, vec![value; (w * h) as usize]))
        })
    }

    fn texel(atlas: &MaskAtlas, slot: MaskSlot, dx: u32, dy: u32) -> u8 {
        atlas.texels()[((slot.atlas_y + dy) * atlas.side() + slot.atlas_x + dx) as usize]
    }

    /// Replays `changes_since` onto a "texture" the way a backend would and
    /// checks it ends up equal to the atlas — over new masks, hits, frames
    /// that compact, a grow, and a backend that skips a frame.
    #[test]
    fn changes_since_brings_a_texture_exactly_up_to_date() {
        let mut atlas = MaskAtlas::new();
        let mut texture: Vec<u8> = Vec::new();
        let mut held: Option<u64> = None;
        let mut full_uploads = 0;
        let mut region_bytes = 0u64;
        let mut sync = |atlas: &MaskAtlas, texture: &mut Vec<u8>, held: &mut Option<u64>| {
            match atlas.changes_since(*held) {
                MaskChanges::None => {}
                MaskChanges::Full => {
                    *texture = atlas.texels().to_vec();
                    full_uploads += 1;
                }
                MaskChanges::Regions(rects) => {
                    assert_eq!(
                        texture.len(),
                        atlas.texels().len(),
                        "a region upload never resizes"
                    );
                    for &[x, y, w, h] in rects {
                        for row in y..y + h {
                            let i = (row * atlas.side() + x) as usize;
                            texture[i..i + w as usize]
                                .copy_from_slice(&atlas.texels()[i..i + w as usize]);
                            region_bytes += u64::from(w);
                        }
                    }
                }
            }
            *held = Some(atlas.version());
        };
        let mut id = 0;
        let mut compactions = 0;
        let mut last_side = atlas.side();
        for frame in 0..400u32 {
            let before = atlas.version();
            atlas.begin_frame();
            if atlas.version() != before && atlas.side() == last_side {
                compactions += 1;
            }
            last_side = atlas.side();
            // A steady mask every frame, plus one or two new ones (an animated
            // shape), sized so the atlas compacts and grows along the way.
            insert(&mut atlas, 1_000_000, 40, 30, 200).unwrap();
            for _ in 0..1 + frame % 2 {
                id += 1;
                let side = 20 + (id % 7) as u32 * 15;
                insert(&mut atlas, id, side, side + 3, (id % 251) as u8 + 1).unwrap();
            }
            if frame % 37 == 5 {
                continue; // the backend misses a frame
            }
            sync(&atlas, &mut texture, &mut held);
            assert!(
                texture == atlas.texels(),
                "frame {frame}: texture differs from atlas"
            );
        }
        assert!(
            compactions >= 3,
            "the test must cross compactions: {compactions}"
        );
        assert!(
            full_uploads < 80,
            "full uploads should be the exception: {full_uploads}"
        );
        assert!(region_bytes > 0, "region uploads should be the rule");
    }

    #[test]
    fn a_hit_does_not_change_the_version() {
        let mut atlas = MaskAtlas::new();
        let a = insert(&mut atlas, 1, 8, 8, 7).unwrap();
        let v = atlas.version();
        let b = insert(&mut atlas, 1, 8, 8, 7).unwrap();
        assert_eq!(a, b);
        assert_eq!(atlas.version(), v);
    }

    #[test]
    fn the_fetch_offset_maps_device_pixels_to_texels() {
        let mut atlas = MaskAtlas::new();
        insert(&mut atlas, 1, 3, 3, 1).unwrap();
        let slot = insert(&mut atlas, 2, 4, 4, 9).unwrap();
        let [ox, oy] = slot.fetch_offset();
        // Device (10, 20) is the patch's first texel.
        assert_eq!((10.0 - ox) as u32, slot.atlas_x);
        assert_eq!((20.0 - oy) as u32, slot.atlas_y);
    }

    #[test]
    fn growing_keeps_every_slot_in_place() {
        let mut atlas = MaskAtlas::new();
        let mut slots = Vec::new();
        for id in 0..200u64 {
            let value = (id % 250 + 1) as u8;
            let slot = insert(&mut atlas, id, 30, 20, value).unwrap();
            slots.push((slot, value));
        }
        assert!(atlas.side() > MaskAtlas::INITIAL_SIDE, "the test must grow");
        for (id, (slot, value)) in slots.iter().enumerate() {
            let again = atlas.get_or_insert(MaskKey(id as u64), || None).unwrap();
            assert_eq!(*slot, again, "slot {id} moved during a grow");
            assert_eq!(texel(&atlas, *slot, 29, 19), *value);
        }
    }

    #[test]
    fn compaction_keeps_last_frames_masks_and_drops_the_rest() {
        let mut atlas = MaskAtlas::new();
        for id in 0..60u64 {
            insert(&mut atlas, id, 30, 30, 5).unwrap();
        }
        atlas.begin_frame();
        // Only mask 3 is used in this frame.
        let kept = insert(&mut atlas, 3, 30, 30, 5).unwrap();
        atlas.begin_frame();
        assert_eq!(atlas.len(), 1, "only the mask used last frame survives");
        let again = atlas.get_or_insert(MaskKey(3), || None).unwrap();
        assert_eq!(texel(&atlas, again, 0, 0), 5);
        let _ = kept;
    }
}
