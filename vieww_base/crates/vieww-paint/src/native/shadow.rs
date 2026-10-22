//! `DrawShadow`: a blurred rounded-rect silhouette.
//!
//! Spec §8.1/§8.2: "the default blur is the 3-box Gaussian ... outer shadows
//! are an analytic SDF ... in a single draw" and inset shadows "keep the
//! complement trick (blurred rect punched through DestOut inside the
//! clip)". This build uses a rasterized-then-box-blurred mask rather than an
//! analytic SDF fragment shader (that is GPU-backend work — see
//! `docs/RENDERER-MIGRATION.md`), but keeps the same three-pass-box
//! approximation of a Gaussian the spec calls for, and the same DestOut
//! complement for the inset case, so a CPU-vs-GPU shadow stays comparable
//! once the SDF shader lands.

use vieww_foundation::{Offset, Rect, Shadow, Transform};

use super::geometry::fill::rasterize;
use super::rounded_rect::rounded_rect_polygon_transformed;

/// Shadows blurred, and their pixels, process-wide — both renderers blur
/// through [`shadow_alpha`]. What a report divides by frames to say whether
/// the caches are holding: a steady screen blurs nothing.
static BLURS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static BLURRED_PIXELS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many shadows have been blurred since the process started, and how
/// many pixels that covered — for a report, not for decisions.
#[must_use]
pub fn shadow_blurs() -> (u64, u64) {
    (
        BLURS.load(std::sync::atomic::Ordering::Relaxed),
        BLURRED_PIXELS.load(std::sync::atomic::Ordering::Relaxed),
    )
}

/// A shadow's final coverage before its colour: blurred and complemented
/// for an inset shadow. What the shadow cache keeps and tints as it draws, and what the GPU seam
/// hands the planner whole, so the GPU draws a shadow as one textured pass
/// instead of re-running the blur.
///
/// **Not cropped to `surface`**: it can reach past it by up to the blur's
/// reach (`shadow_masks` clamps there), and the caller cuts it to the surface
/// when it draws it. Cropped here, a card's patch would depend on where the
/// surface's edge falls across it, and a card scrolling under that edge
/// would need a new patch every frame.
#[derive(Debug, Clone)]
pub(crate) struct ShadowAlpha {
    pub(crate) x0: i32,
    pub(crate) y0: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Row-major, `width * height`, in `0..=1`.
    pub(crate) alpha: Vec<f32>,
}

/// See [`ShadowAlpha`].
pub(crate) fn shadow_alpha(
    rect: Rect,
    radius: f32,
    shadow: &Shadow,
    transform: Transform,
    surface: Rect,
) -> Option<ShadowAlpha> {
    let masks = shadow_masks(rect, radius, shadow, transform, surface)?;
    let (w, h) = (masks.width, masks.height);
    BLURS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    BLURRED_PIXELS.fetch_add(
        u64::from(w) * u64::from(h),
        std::sync::atomic::Ordering::Relaxed,
    );
    let mut alpha = masks.caster;

    if let Some(box_radius) = masks.box_radius {
        box_blur_3_radius(&mut alpha, w, h, box_radius);
    }

    if let Some(inner_alpha) = masks.inner {
        // Complement trick: invert the (unblurred-caster, blurred-edge)
        // mask and confine it back inside the original box via a second
        // rasterization used as a clip multiply.
        for i in 0..alpha.len() {
            alpha[i] = (1.0 - alpha[i]).clamp(0.0, 1.0) * inner_alpha[i];
        }
    }

    Some(ShadowAlpha {
        x0: masks.x0,
        y0: masks.y0,
        width: w,
        height: h,
        alpha,
    })
}

/// A surface's extent in whole pixels: outward to the pixels it touches.
pub(crate) fn pixel_bounds(surface: Rect) -> (i32, i32, i32, i32) {
    (
        surface.left.floor() as i32,
        surface.top.floor() as i32,
        surface.right.ceil() as i32,
        surface.bottom.ceil() as i32,
    )
}

/// Where a shadow is drawn, separated from what it looks like — so the
/// same card at another place, or the same card one scroll step later,
/// reuses one blurred patch instead of blurring again.
///
/// # Why
///
/// Both renderers used to key a shadow by its absolute position. A list
/// that scrolls moves every card, so every shadow missed every frame: on
/// the stress screen (a feed of cards scrolling under a header) the GPU
/// path spent 308 ms of a 397 ms lavapipe frame re-blurring shadows the
/// previous frame had already blurred, a pixel higher up.
///
/// # How
///
/// A shadow is a function of its rectangle's *size*, its radius, the
/// shadow's parameters, the transform's linear part and the **sub-pixel
/// phase** of where the rectangle lands — not of which whole pixel that is.
/// [`shadow_frame`] moves the shadow to a canonical place (the rectangle at
/// the origin, translated only by its device origin's fractional part) and
/// says how many whole pixels to move the result back by.
///
/// That holds while nothing outside the shadow decides its pixels — while
/// the surface, grown by the blur's reach, holds the whole shadow, so
/// `shadow_masks`' clamp does not bite. A shadow crossing the screen's edge
/// is still free (it is cropped where it is drawn); one reaching further
/// past it than the blur does keeps its absolute key and its place.
pub(crate) struct ShadowFrame {
    /// The cache key: everything the patch's bytes depend on, colour
    /// included (a GPU caller that caches coverage alone can drop it — see
    /// [`ShadowFrame::coverage_key`]).
    pub(crate) key: ShadowKey,
    /// Where to render: the canonical rectangle and transform when the
    /// shadow is free of the surface, the original ones otherwise.
    pub(crate) rect: Rect,
    pub(crate) transform: Transform,
    pub(crate) surface: Rect,
    /// Whole pixels to add to the rendered patch's position.
    pub(crate) shift: (i32, i32),
}

impl ShadowFrame {
    /// The key without the colour: what the patch's *coverage* depends on.
    pub(crate) fn coverage_key(&self) -> [u32; 16] {
        let mut key = self.key.0;
        key[5] = 0;
        key
    }
}

/// See [`ShadowFrame`].
pub(crate) fn shadow_frame(
    rect: Rect,
    radius: f32,
    shadow: &Shadow,
    transform: Transform,
    surface: Rect,
) -> ShadowFrame {
    let origin = transform.apply(Offset::new(rect.left, rect.top));
    // The sub-pixel phase, **rounded to a quarter pixel**. Exact phases made
    // every frame of a scroll a miss on a phone: 6 points a frame at a
    // density of 3.5 is 21 device pixels and a fraction, and the fraction
    // was new every frame (Device Farm, S26 Ultra stress screen: shadows
    // 33.7 ms of an 82 ms CPU frame, 18.6 ms of GPU-path planning). A blurred
    // shadow moved by at most an eighth of a pixel is not a visible change;
    // a card now has at most sixteen phases, and a scroll reuses them.
    let snapped = (snap_phase(origin.dx), snap_phase(origin.dy));
    let whole = (snapped.0.floor(), snapped.1.floor());
    // The size, **snapped to 1/64 of a unit**: `bottom - top` of the same
    // card is 116.0 in one place and 115.99997 in another (float
    // subtraction at different magnitudes), and a key compared by bit
    // pattern took those for two shapes — every card a new patch, every
    // frame, until this rounding (5.2 blurs a frame on the stress screen at a
    // density of 2.625).
    let canon_rect = Rect::new(
        0.0,
        0.0,
        snap_size(rect.right - rect.left),
        snap_size(rect.bottom - rect.top),
    );
    let canon_transform = Transform::new(
        transform.a,
        transform.b,
        transform.c,
        transform.d,
        snapped.0 - whole.0,
        snapped.1 - whole.1,
    );
    let shift = (whole.0 as i32, whole.1 as i32);
    // Free whenever the shadow fits the surface (`fits`), wherever it is:
    // then `shadow_masks` never clamps it, and nothing about the surface
    // decides its bytes.
    let canon = canon_transform.apply_rect(shadow.bounds(canon_rect));
    let free = origin.dx.is_finite()
        && origin.dy.is_finite()
        && whole.0.abs() < 1.0e7
        && whole.1.abs() < 1.0e7
        && fits(canon, surface, reach(shadow, transform));
    if free {
        let canon_surface = surface.translate(Offset::new(-whole.0, -whole.1));
        ShadowFrame {
            // The surface does not decide a free patch's bytes, so it is
            // not in the key — a marker is, so a free key can never equal
            // the absolute key of a shadow that happens to sit at the
            // origin.
            key: ShadowKey::new(canon_rect, radius, shadow, canon_transform, FREE),
            rect: canon_rect,
            transform: canon_transform,
            surface: canon_surface,
            shift,
        }
    } else {
        ShadowFrame {
            key: ShadowKey::new(rect, radius, shadow, transform, surface),
            rect,
            transform,
            surface,
            shift: (0, 0),
        }
    }
}

/// A shadow's size to the nearest 1/64 of a unit — see `shadow_frame`.
fn snap_size(v: f32) -> f32 {
    (v * 64.0).round() / 64.0
}

/// `v` to the nearest quarter: the sub-pixel phases a cached shadow keeps.
fn snap_phase(v: f32) -> f32 {
    (v * 4.0).round() / 4.0
}

/// The surface a free [`ShadowKey`] carries in place of a real one: no real
/// surface is NaN.
const FREE: Rect = Rect {
    left: f32::NAN,
    top: f32::NAN,
    right: f32::NAN,
    bottom: f32::NAN,
};

/// The inputs of a shadow before any blur: the caster silhouette's coverage,
/// the inset confinement mask, the patch placement and the box radius.
///
/// Split out of [`shadow_alpha`], which blurs, complements and crops it —
/// the coverage both renderers then tint (the GPU seam hands it to a backend
/// whole). One implementation of a shadow, blur included.
pub(crate) struct ShadowMasks {
    pub(crate) x0: i32,
    pub(crate) y0: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Unblurred caster coverage, row-major, `width * height`.
    pub(crate) caster: Vec<f32>,
    /// For an inset shadow, the coverage of the box the shadow is confined to.
    pub(crate) inner: Option<Vec<f32>>,
    /// The three-box radius, or `None` when the blur is too small to run.
    pub(crate) box_radius: Option<i32>,
}

pub(crate) fn shadow_masks(
    rect: Rect,
    radius: f32,
    shadow: &Shadow,
    transform: Transform,
    surface: Rect,
) -> Option<ShadowMasks> {
    if shadow.is_invisible() {
        return None;
    }
    let box_radius = shadow_box_radius(shadow, transform);

    let caster = rect.inflate(shadow.spread).translate(shadow.offset);
    let bounds_local = shadow.bounds(rect);
    // **The whole shadow, when it fits the surface; clamped to the surface
    // grown by the blur's reach when it does not.**
    //
    // A shadow no bigger than the surface is computed whole, wherever it is
    // — half off the screen included — so its bytes never depend on where
    // the surface's edge falls across it, and the cache keeps it as it
    // scrolls under that edge. Device Farm (2026-10-22, Galaxy A73, stress
    // screen) is why: clamped near the edge, the cards entering and leaving
    // were new patches every frame, 70.7 ms of a 180 ms CPU frame.
    //
    // A bigger one (a shadow under a whole scroll view's content) is clamped,
    // so it costs the surface plus a margin and never the whole caster. The
    // margin is the blur's reach: three box passes of radius `r` read `3r`
    // pixels each way, so every pixel inside the surface is still the
    // shadow's true value. (Clamped at the surface itself, as this once was,
    // the blur read transparent there and a shadow faded towards the edge.)
    let reach = reach(shadow, transform);
    let whole = transform.apply_rect(bounds_local);
    if whole.intersect(surface.inflate(reach)).is_empty() {
        return None;
    }
    let bounds_device = if fits(whole, surface, reach) {
        whole
    } else {
        whole.intersect(surface.inflate(reach))
    };
    let x0 = bounds_device.left.floor() as i32;
    let y0 = bounds_device.top.floor() as i32;
    let w = (bounds_device.right.ceil() as i32 - x0).max(0) as u32;
    let h = (bounds_device.bottom.ceil() as i32 - y0).max(0) as u32;
    if w == 0 || h == 0 {
        return None;
    }

    let clip = Rect::new(
        x0 as f32,
        y0 as f32,
        (x0 + w as i32) as f32,
        (y0 + h as i32) as f32,
    );
    let place = |polygon| {
        let mask = rasterize(&[polygon], clip);
        let mut alpha = vec![0.0f32; (w as usize) * (h as usize)];
        for dy in 0..mask.height {
            for dx in 0..mask.width {
                let c = mask.data[(dy * mask.width + dx) as usize];
                let px = (mask.x0 + dx as i32) - x0;
                let py = (mask.y0 + dy as i32) - y0;
                if px >= 0 && py >= 0 && (px as u32) < w && (py as u32) < h {
                    alpha[py as usize * w as usize + px as usize] = c;
                }
            }
        }
        alpha
    };
    // Rasterize the (offset+spread) caster into a local alpha buffer sized
    // to this patch.
    let caster_alpha = place(rounded_rect_polygon_transformed(caster, radius, transform));
    let inner = shadow
        .is_inset
        .then(|| place(rounded_rect_polygon_transformed(rect, radius, transform)));
    Some(ShadowMasks {
        x0,
        y0,
        width: w,
        height: h,
        caster: caster_alpha,
        inner,
        box_radius,
    })
}

/// The three-box radius a shadow blurs with under `transform`, or `None`
/// when the blur is too small to run.
fn shadow_box_radius(shadow: &Shadow, transform: Transform) -> Option<i32> {
    let scale = (transform.a * transform.d - transform.b * transform.c)
        .abs()
        .sqrt()
        .max(0.01);
    let sigma_device = (shadow.std_dev() * scale).max(0.0);
    (sigma_device > 0.1).then(|| box_radius_for_sigma(sigma_device))
}

/// How far past a pixel the blur reads, in device pixels: three box passes
/// of radius `r` read `3r`, plus one for the edge pixel's own coverage.
fn reach(shadow: &Shadow, transform: Transform) -> f32 {
    shadow_box_radius(shadow, transform).map_or(1.0, |r| (3 * r + 1) as f32)
}

/// Whether a shadow's device bounds fit `surface` grown by `reach` on each
/// side, in size — wherever they are. Such a shadow is computed whole and
/// cached by its shape (`shadow_frame`); a bigger one is clamped.
fn fits(bounds: Rect, surface: Rect, reach: f32) -> bool {
    bounds.width() <= surface.width() + 2.0 * reach
        && bounds.height() <= surface.height() + 2.0 * reach
}

/// The per-pass radius of the three-box approximation of a Gaussian of
/// standard deviation `sigma` — shared by shadows, layer blurs, and the GPU
/// seam, so all three agree on the kernel to the texel.
pub(crate) fn box_radius_for_sigma(sigma: f32) -> i32 {
    let diameter = (4.0 * sigma * sigma + 1.0).sqrt();
    (((diameter - 1.0) / 2.0).round() as i32).max(1)
}

/// Three passes of a box blur, radius derived from `sigma` (the standard
/// three-box approximation to a Gaussian — spec §8.1).
///
/// The per-pass radius is **not** `3 * sigma` — that would be the *reach* of
/// a single Gaussian tail (spec's `BLUR_REACH`, used for bounds, not for a
/// box width), and using it as every one of three box radii triples the
/// effective blur. Three box blurs of full width `d` approximate a Gaussian
/// of standard deviation `sigma` when `d = sqrt(4*sigma^2 + 1)` (Kovesi,
/// "Fast Almost-Gaussian Filtering") — this is that formula.
fn box_blur_3_radius(data: &mut [f32], w: u32, h: u32, radius: i32) {
    let mut tmp = vec![0.0f32; data.len()];
    for _ in 0..3 {
        box_blur_horizontal(data, &mut tmp, w, h, radius);
        box_blur_vertical(&tmp, data, w, h, radius);
    }
}

/// A running-sum box blur along each row.
///
/// # No bounds check per sample
///
/// The window is advanced by adding the sample entering it and subtracting
/// the one leaving, and both of those are only *sometimes* off the end of the
/// row. This used to ask a bounds-checked reader for every one of them —
/// twelve checked reads per pixel across the six passes of a three-box blur —
/// to get a zero on the handful that were genuinely outside. Reading the row
/// as a slice and testing the index instead keeps the same convention
/// (outside is transparent, which is what makes a shadow fade at its own
/// edge) with no work in the common case.
///
/// The arithmetic is written as `sum += entering - leaving`, one expression,
/// because splitting it into an add and a subtract changes the last bit of
/// the result and this kernel is a correctness reference.
fn box_blur_horizontal(src: &[f32], dst: &mut [f32], w: u32, h: u32, radius: i32) {
    let window = (2 * radius + 1) as f32;
    let width = w as i32;
    for y in 0..h as usize {
        let row = y * w as usize;
        let src_row = &src[row..row + w as usize];
        let dst_row = &mut dst[row..row + w as usize];

        let mut sum = 0.0f32;
        for x in 0..=radius.min(width - 1) {
            sum += src_row[x as usize];
        }
        for x in 0..width {
            dst_row[x as usize] = sum / window;
            let entering = x + radius + 1;
            let leaving = x - radius;
            let a = if entering < width {
                src_row[entering as usize]
            } else {
                0.0
            };
            let b = if leaving >= 0 {
                src_row[leaving as usize]
            } else {
                0.0
            };
            sum += a - b;
        }
    }
}

/// How many columns the vertical pass carries at once — see
/// `native/effects.rs`'s `STRIP`, which has the measurement and the trade.
/// Sixteen here rather than eight because a shadow mask is one `f32` per pixel,
/// not four, so sixteen of them is the same 64 bytes.
const STRIP: usize = 16;

/// The same, down each column — but [`STRIP`] columns at a time, for the reason
/// `native/effects.rs`'s `vertical` gives: a single-column walk over a
/// row-major buffer uses a quarter of every cache line it fetches. Each column
/// keeps its own running sum and they never interact, so the result is
/// bit-for-bit what the single-column loop produced.
fn box_blur_vertical(src: &[f32], dst: &mut [f32], w: u32, h: u32, radius: i32) {
    let window = (2 * radius + 1) as f32;
    let (width, height) = (w as usize, h as i32);

    let mut x0 = 0usize;
    while x0 < width {
        let strip = STRIP.min(width - x0);
        let mut sums = [0.0f32; STRIP];

        for y in 0..=radius.min(height - 1) {
            let row = y as usize * width + x0;
            for (i, sum) in sums.iter_mut().take(strip).enumerate() {
                *sum += src[row + i];
            }
        }

        for y in 0..height {
            let out_row = y as usize * width + x0;
            for (i, sum) in sums.iter_mut().take(strip).enumerate() {
                dst[out_row + i] = *sum / window;
            }
            let entering = y + radius + 1;
            let leaving = y - radius;
            for (i, sum) in sums.iter_mut().take(strip).enumerate() {
                let a = if entering < height {
                    src[entering as usize * width + x0 + i]
                } else {
                    0.0
                };
                let b = if leaving >= 0 {
                    src[leaving as usize * width + x0 + i]
                } else {
                    0.0
                };
                *sum += a - b;
            }
        }
        x0 += strip;
    }
}

/// Shadow patches already rendered, keyed by everything that decides one.
///
/// # Why a shadow is worth caching and a fill is not
///
/// A fill is rasterised in about as long as it takes to look up whether it
/// was cached. A shadow is a silhouette rasterised, copied into an alpha
/// buffer, run through six separable box-blur passes over the whole patch,
/// and tinted — and a panel's patch is the panel plus the blur's reach on
/// every side. Measured on the Studio shell: **13 shadows, 37.6 ms**, about
/// three milliseconds each.
///
/// None of that work depends on anything that changes between frames. A
/// panel's shadow is a pure function of its rectangle, its radius, the
/// shadow's own parameters and the transform — so a panel that has not moved
/// has the same shadow it had last frame, down to the bit.
///
/// This matters most exactly where it is least obvious. A damaged repaint
/// re-executes every command that overlaps the damaged region, and a card's
/// shadow overlaps everything inside the card — so without this, lighting one
/// row of a list re-blurs the whole card's shadow, and the incremental
/// repaint the damage pipeline exists to make cheap is dominated by an effect
/// that did not change.
#[derive(Debug, Default)]
pub(crate) struct ShadowCache {
    entries: Vec<(ShadowKey, ShadowAlpha)>,
    pixels: usize,
    hits: usize,
    misses: usize,
}

/// Everything [`shadow_alpha`] reads, and the tint. Compared by bit pattern rather than by
/// `PartialEq` on floats, so that two keys are equal exactly when the
/// arithmetic below them would be.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ShadowKey(pub(crate) [u32; 16]);

impl ShadowKey {
    fn new(rect: Rect, radius: f32, shadow: &Shadow, transform: Transform, surface: Rect) -> Self {
        Self([
            rect.left.to_bits(),
            rect.top.to_bits(),
            rect.right.to_bits(),
            rect.bottom.to_bits(),
            radius.to_bits(),
            u32::from_le_bytes([
                shadow.color.r,
                shadow.color.g,
                shadow.color.b,
                shadow.color.a,
            ]),
            shadow.offset.dx.to_bits(),
            shadow.offset.dy.to_bits(),
            shadow.blur.to_bits(),
            shadow.spread.to_bits(),
            u32::from(shadow.is_inset),
            transform.a.to_bits() ^ transform.b.to_bits().rotate_left(8),
            transform.c.to_bits() ^ transform.d.to_bits().rotate_left(8),
            transform.tx.to_bits(),
            transform.ty.to_bits(),
            surface.left.to_bits()
                ^ surface.top.to_bits().rotate_left(8)
                ^ surface.right.to_bits().rotate_left(16)
                ^ surface.bottom.to_bits().rotate_left(24),
        ])
    }
}

/// How many patch pixels the cache may hold at once, at least.
///
/// A patch is its coverage, one `f32` — four bytes — per pixel, so this is
/// a budget of about thirty megabytes: the same memory as when patches were
/// stored tinted at sixteen bytes a pixel and a quarter of the pixels, which
/// a list of cards overran (two card heights at four sub-pixel phases beside
/// an animated shadow, 466k pixels each at a density of 2.625 — every
/// shadow missed every frame). Firmly bounded, which is the property
/// that matters: a scene that animates a shadow's blur every frame must
/// degrade to "no reuse", not to "no memory".
///
/// # At least, because a fixed number thrashes on a high-density screen
///
/// A panel's shadow covers the panel plus its blur, and at a device ratio of
/// 2 a dialog over most of a 1280x800 window is a patch of about three million
/// pixels — more than this whole budget. It was still stored ("one frame's
/// correctness is not worth a memory bound") and then evicted by the next,
/// smaller shadow of the same frame, which it evicted in turn on the next
/// frame: every shadow missed, every frame. Measured on Wright's settings
/// dialog while scrolling at ratio 2: 209 ms a frame with the thrash, and the
/// shadow's share of it gone with room for it. So the budget is also never
/// less than three surfaces — a window-sized shadow's patch is the window
/// plus its blur margin, and the rest is room for the small ones beside it —
/// and still bounded by what is on screen.
const PIXEL_BUDGET: usize = 8_000_000;

/// The budget for a surface: [`PIXEL_BUDGET`], or three times the surface's
/// pixel count when that is larger.
fn budget_for(surface: Rect) -> usize {
    let area = (surface.width().max(0.0) * surface.height().max(0.0)) as usize;
    PIXEL_BUDGET.max(3 * area)
}

impl ShadowCache {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Hits and misses over this cache's lifetime — reported through
    /// [`NativeRenderer::shadow_cache_stats`](super::NativeRenderer::shadow_cache_stats).
    pub(crate) fn stats(&self) -> (usize, usize) {
        (self.hits, self.misses)
    }

    /// The patch for this shadow, rendering it only if it is not already held.
    ///
    /// Returns `None` for a shadow that produces nothing at all — invisible,
    /// or entirely off the surface.
    pub(crate) fn patch(
        &mut self,
        rect: Rect,
        radius: f32,
        shadow: &Shadow,
        transform: Transform,
        surface: Rect,
    ) -> Option<(&ShadowAlpha, i32, i32)> {
        let frame = shadow_frame(rect, radius, shadow, transform, surface);
        let key = frame.key;
        if let Some(index) = self.entries.iter().position(|(k, ..)| *k == key) {
            self.hits += 1;
            let entry = self.entries.remove(index);
            self.entries.push(entry);
        } else {
            self.misses += 1;
            let alpha = shadow_alpha(frame.rect, radius, shadow, frame.transform, frame.surface)?;
            let cost = alpha.alpha.len();
            // Evict oldest-first until this one fits. A single patch larger
            // than the whole budget is still stored — one frame's correctness
            // is not worth trading for a memory bound — and evicted on the
            // next miss.
            let budget = budget_for(surface);
            while self.pixels + cost > budget && !self.entries.is_empty() {
                let (_, dropped) = self.entries.remove(0);
                self.pixels -= dropped.alpha.len();
            }
            self.pixels += cost;
            self.entries.push((key, alpha));
        }
        let (_, patch) = self.entries.last().expect("just pushed");
        Some((patch, patch.x0 + frame.shift.0, patch.y0 + frame.shift.1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vieww_foundation::{Color, Offset};

    fn card_shadow() -> Shadow {
        Shadow::new(Color::rgba(0, 0, 0, 200), Offset::new(0.0, 6.0), 16.0)
    }

    /// A card half off the top of the surface keeps its true shadow inside
    /// the surface: the same values a surface large enough to hold the
    /// whole shadow produces there. Clamped at the surface itself, the blur
    /// read transparent past it and the shadow faded towards the edge.
    #[test]
    fn a_shadow_crossing_the_surface_edge_does_not_fade_at_it() {
        let rect = Rect::new(40.0, -30.0, 200.0, 60.0);
        let small = Rect::new(0.0, 0.0, 240.0, 200.0);
        let large = Rect::new(-400.0, -400.0, 640.0, 600.0);
        let near =
            shadow_alpha(rect, 12.0, &card_shadow(), Transform::IDENTITY, small).expect("visible");
        let whole =
            shadow_alpha(rect, 12.0, &card_shadow(), Transform::IDENTITY, large).expect("visible");
        let at = |s: &ShadowAlpha, x: i32, y: i32| {
            s.alpha[((y - s.y0) as u32 * s.width + (x - s.x0) as u32) as usize]
        };
        let mut checked = 0;
        for y in 0..200 {
            for x in 0..240 {
                let inside = |s: &ShadowAlpha| {
                    x >= s.x0
                        && y >= s.y0
                        && x < s.x0 + s.width as i32
                        && y < s.y0 + s.height as i32
                };
                if inside(&whole) {
                    assert!(
                        inside(&near),
                        "({x},{y}) is in the shadow but not the patch"
                    );
                    let (a, b) = (at(&near, x, y), at(&whole, x, y));
                    assert!(
                        (a - b).abs() < 1e-5,
                        "({x},{y}): {a} at the edge, {b} in truth"
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 1000, "{checked}");
    }

    /// A shadow bigger than the surface is clamped to it (grown by the blur's
    /// reach), so its patch costs the surface and not the whole caster.
    #[test]
    fn a_shadow_bigger_than_the_surface_is_clamped_to_it() {
        let surface = Rect::new(0.0, 0.0, 400.0, 300.0);
        let content = Rect::new(0.0, -5000.0, 400.0, 5000.0);
        let alpha = shadow_alpha(content, 0.0, &card_shadow(), Transform::IDENTITY, surface)
            .expect("visible");
        assert!(
            alpha.height < 600,
            "{} rows for a 300-row surface",
            alpha.height
        );
    }

    /// A card wholly off the surface, beyond the blur's reach, casts nothing
    /// on it, and is not blurred.
    #[test]
    fn a_shadow_beyond_the_surface_is_not_computed() {
        let surface = Rect::new(0.0, 0.0, 400.0, 300.0);
        let far = Rect::new(40.0, 900.0, 360.0, 1000.0);
        assert!(shadow_alpha(far, 8.0, &card_shadow(), Transform::IDENTITY, surface).is_none());
    }

    /// A card crossing the surface's edge is still keyed by its shape: as it
    /// scrolls under the edge, its patch is reused.
    #[test]
    fn a_shadow_scrolling_under_the_edge_keeps_its_patch() {
        let surface = Rect::new(0.0, 0.0, 400.0, 800.0);
        let mut cache = ShadowCache::new();
        // Far past the edge too: the card is computed whole, wherever it is.
        for top in [10.0, 4.0, -2.0, -8.0, -60.0, -100.0] {
            cache.patch(
                Rect::new(40.0, top, 360.0, top + 120.0),
                18.0,
                &card_shadow(),
                Transform::IDENTITY,
                surface,
            );
        }
        assert_eq!(cache.stats(), (5, 1));
    }

    /// The same card at another whole-pixel position — the next one down a
    /// list, or this one a scroll step later — is a cache hit, drawn at its
    /// own place; a sub-pixel move is a different phase, and a miss.
    #[test]
    fn a_whole_pixel_move_is_a_cache_hit_at_the_new_place() {
        let surface = Rect::new(0.0, 0.0, 400.0, 2000.0);
        let mut cache = ShadowCache::new();
        let at = |top: f32| Rect::new(40.0, top, 360.0, top + 120.0);
        let (_, x0, y0) = cache
            .patch(
                at(200.0),
                18.0,
                &card_shadow(),
                Transform::IDENTITY,
                surface,
            )
            .map(|(p, x, y)| (p.width, x, y))
            .expect("visible");
        let (_, x1, y1) = cache
            .patch(
                at(206.0),
                18.0,
                &card_shadow(),
                Transform::IDENTITY,
                surface,
            )
            .map(|(p, x, y)| (p.width, x, y))
            .expect("visible");
        assert_eq!(cache.stats(), (1, 1), "one miss, then a hit");
        assert_eq!((x1, y1), (x0, y0 + 6), "drawn where the card now is");
        // The scroll could equally be the transform's.
        let moved = Transform::translate(Offset::new(0.0, 12.0));
        cache.patch(at(200.0), 18.0, &card_shadow(), moved, surface);
        assert_eq!(cache.stats(), (2, 1));
        cache.patch(
            at(200.5),
            18.0,
            &card_shadow(),
            Transform::IDENTITY,
            surface,
        );
        assert_eq!(cache.stats(), (2, 2), "half a pixel is another phase");
        // A scroll step with a fraction in it — a phone's 6 points at a
        // density of 3.5 — lands on one of four phases a pixel, not a new
        // one every frame.
        for step in 0..32 {
            #[expect(clippy::cast_precision_loss, reason = "a small count")]
            let top = 200.0 + step as f32 * 21.03;
            cache.patch(at(top), 18.0, &card_shadow(), Transform::IDENTITY, surface);
        }
        let (hits, misses) = cache.stats();
        assert!(
            misses <= 2 + 4,
            "{misses} misses over 32 fractional steps ({hits} hits)"
        );
    }

    /// Caching by shape draws exactly what drawing in place draws.
    #[test]
    fn a_shadow_from_the_cache_is_the_shadow_drawn_in_place() {
        let surface = Rect::new(0.0, 0.0, 400.0, 800.0);
        let rect = Rect::new(37.25, 211.75, 301.0, 330.0);
        let transform = Transform::translate(Offset::new(3.0, 140.0));
        let mut cache = ShadowCache::new();
        let (cached, cx, cy) = cache
            .patch(rect, 10.0, &card_shadow(), transform, surface)
            .map(|(p, x, y)| (p.clone(), x, y))
            .expect("visible");
        let direct = shadow_alpha(rect, 10.0, &card_shadow(), transform, surface).expect("visible");
        assert_eq!(
            (cx, cy, cached.width, cached.height),
            (direct.x0, direct.y0, direct.width, direct.height)
        );
        for (a, b) in cached.alpha.iter().zip(&direct.alpha) {
            assert!((a - b).abs() < 1e-4, "{a} against {b}");
        }
    }
}
