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

use super::color::Premul;
use super::geometry::fill::rasterize;
use super::rounded_rect::rounded_rect_polygon_transformed;
use super::target::Target;

/// Render `shadow` cast by a rounded rect (`rect`/`radius`, local space)
/// under `transform`, clamped to `surface`. Returns the tinted, blurred
/// patch and its top-left in device space, or `None` if it would be empty.
pub(crate) fn render_shadow(
    rect: Rect,
    radius: f32,
    shadow: &Shadow,
    transform: Transform,
    surface: Rect,
) -> Option<(Target, i32, i32)> {
    let ShadowAlpha {
        x0,
        y0,
        width: w,
        height: h,
        alpha,
    } = shadow_alpha(rect, radius, shadow, transform, surface)?;
    let tint = Premul::from_straight(shadow.color);
    let mut target = Target::new(w, h);
    for (i, a) in alpha.iter().enumerate() {
        target.pixels[i] = tint.scaled(*a);
    }
    // Written through `pixels` directly rather than through a compositing
    // method, so the ink `composite_layer` reads has to be declared here. The
    // patch is exactly the blurred silhouette's extent — it was sized to it —
    // so the whole of it counts.
    target.mark_all_ink();
    Some((target, x0, y0))
}

/// A shadow's final coverage before its colour: blurred and complemented
/// for an inset shadow. What [`render_shadow`] tints, and what the GPU seam
/// hands the planner whole, so the GPU draws a shadow as one textured pass
/// instead of re-running the blur.
///
/// **Not cropped to `surface`**: it can reach past it by up to the blur's
/// reach (`shadow_masks` clamps there), and the caller cuts it to the surface
/// when it draws it. Cropped here, a card's patch would depend on where the
/// surface's edge falls across it, and a card scrolling under that edge
/// would need a new patch every frame.
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
    let canon_rect = Rect::new(0.0, 0.0, rect.right - rect.left, rect.bottom - rect.top);
    let canon_transform = Transform::new(
        transform.a,
        transform.b,
        transform.c,
        transform.d,
        snapped.0 - whole.0,
        snapped.1 - whole.1,
    );
    let shift = (whole.0 as i32, whole.1 as i32);
    // Free when the whole unclamped patch, put back where it belongs, is
    // inside the surface grown by the blur's reach: then the clamp in
    // `shadow_masks` does not change it.
    let canon = canon_transform.apply_rect(shadow.bounds(canon_rect));
    let (sx0, sy0, sx1, sy1) = pixel_bounds(surface.inflate(reach(shadow, transform)));
    let free = origin.dx.is_finite()
        && origin.dy.is_finite()
        && whole.0.abs() < 1.0e7
        && whole.1.abs() < 1.0e7
        && canon.left.floor() as i32 + shift.0 >= sx0
        && canon.top.floor() as i32 + shift.1 >= sy0
        && canon.right.ceil() as i32 + shift.0 <= sx1
        && canon.bottom.ceil() as i32 + shift.1 <= sy1;
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
    // **Clamped to the surface grown by the blur's reach (`reach`), not to
    // the surface.** Three box passes of radius `r` read `3r` pixels each way,
    // so a pixel inside the surface depends on caster coverage up to `3r`
    // outside it. Clamped at the surface itself, the blur read transparent
    // there instead, and a card scrolled half off the top had its shadow
    // fade towards the screen's edge as if the card ended there. Grown by
    // the reach, every pixel inside the surface is the shadow's true value;
    // `shadow_alpha` crops the rest. Still bounded: a window-sized shadow
    // costs the window plus a margin, never the whole caster.
    let bounds_device = transform
        .apply_rect(bounds_local)
        .intersect(surface.inflate(reach(shadow, transform)));
    if bounds_device.is_empty() {
        return None;
    }
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

/// How far past the surface a shadow's masks are kept, in device pixels.
///
/// At least the blur's reach — three box passes of radius `r` read `3r`
/// pixels, plus one for the edge pixel's own coverage — so every pixel
/// inside the surface is exact. And [`SLACK`] beyond that, so a card keeps
/// its cached patch while it scrolls well past the edge rather than only
/// for its first few pixels: past the reach, the clamp would bite and make
/// the patch depend on where the edge falls (a miss every frame).
fn reach(shadow: &Shadow, transform: Transform) -> f32 {
    shadow_box_radius(shadow, transform).map_or(1.0, |r| (3 * r + 1) as f32) + SLACK
}

/// See [`reach`]. Bounded on purpose: the clamp is also what keeps a
/// shadow cast by a huge rectangle (a whole scroll view's content) from
/// costing a patch the size of the content; it costs the surface plus this,
/// on each side.
const SLACK: f32 = 128.0;

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
    entries: Vec<(ShadowKey, Target, i32, i32)>,
    pixels: usize,
    hits: usize,
    misses: usize,
}

/// Everything [`render_shadow`] reads. Compared by bit pattern rather than by
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
/// A patch is one [`Premul`] — sixteen bytes — per pixel, so this is a
/// budget of about thirty megabytes. Firmly bounded, which is the property
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
/// less than one and a half surfaces — a window-sized shadow's patch is the
/// window plus its blur margin, and the rest is room for the small ones beside
/// it — and still bounded by what is on screen.
const PIXEL_BUDGET: usize = 2_000_000;

/// The budget for a surface: [`PIXEL_BUDGET`], or one and a half times the
/// surface's pixel count when that is larger.
fn budget_for(surface: Rect) -> usize {
    let area = (surface.width().max(0.0) * surface.height().max(0.0)) as usize;
    PIXEL_BUDGET.max(area + area / 2)
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
    ) -> Option<(&Target, i32, i32)> {
        let frame = shadow_frame(rect, radius, shadow, transform, surface);
        let key = frame.key;
        if let Some(index) = self.entries.iter().position(|(k, ..)| *k == key) {
            self.hits += 1;
            let entry = self.entries.remove(index);
            self.entries.push(entry);
        } else {
            self.misses += 1;
            let (patch, px0, py0) =
                render_shadow(frame.rect, radius, shadow, frame.transform, frame.surface)?;
            let cost = patch.pixels.len();
            // Evict oldest-first until this one fits. A single patch larger
            // than the whole budget is still stored — one frame's correctness
            // is not worth trading for a memory bound — and evicted on the
            // next miss.
            let budget = budget_for(surface);
            while self.pixels + cost > budget && !self.entries.is_empty() {
                let (_, dropped, ..) = self.entries.remove(0);
                self.pixels -= dropped.pixels.len();
            }
            self.pixels += cost;
            self.entries.push((key, patch, px0, py0));
        }
        let (_, patch, px0, py0) = self.entries.last().expect("just pushed");
        Some((patch, *px0 + frame.shift.0, *py0 + frame.shift.1))
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

    /// A card crossing the surface's edge is still keyed by its shape: as it
    /// scrolls under the edge, its patch is reused.
    #[test]
    fn a_shadow_scrolling_under_the_edge_keeps_its_patch() {
        let surface = Rect::new(0.0, 0.0, 400.0, 800.0);
        let mut cache = ShadowCache::new();
        for top in [10.0, 4.0, -2.0, -8.0] {
            cache.patch(
                Rect::new(40.0, top, 360.0, top + 120.0),
                18.0,
                &card_shadow(),
                Transform::IDENTITY,
                surface,
            );
        }
        assert_eq!(cache.stats(), (3, 1));
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
        let (direct, dx, dy) =
            render_shadow(rect, 10.0, &card_shadow(), transform, surface).expect("visible");
        assert_eq!(
            (cx, cy, cached.width, cached.height),
            (dx, dy, direct.width, direct.height)
        );
        for (a, b) in cached.pixels.iter().zip(&direct.pixels) {
            assert!((a.a - b.a).abs() < 1e-4, "{a:?} against {b:?}");
        }
    }
}
