//! Layer filters: Gaussian (3-box) blur and the 5x4 colour matrix.
//!
//! Spec §8.1/§8.2: "the CPU kernels in vieww-effects are the definition of
//! correctness — the 3-box Gaussian blur_rgba, the premultiplied colour
//! matrix". Applied to a resolved layer [`super::target::Target`] in the
//! reference renderer's own layer-pop handling, in that order — blur first,
//! matrix after (spec
//! §7's `ImageFilter` docs: "blurring already-desaturated pixels and
//! desaturating already-blurred ones give the same answer for a linear
//! matrix", so the order is a convention rather than a constraint, and this
//! follows the one the doc states).
//!
//! # Why the blur results themselves are cached
//!
//! Blurred layers are the framework's most expensive per-pixel work — the
//! vertical pass alone was worth a 26% frame on a full-window blur before
//! its strip optimisation ([`vertical`]'s doc has the measurement) — and
//! they are disproportionately *repeated* work: a frosted panel over a
//! static background re-blurs byte-identical backdrop rows every frame,
//! because the backdrop snapshot (`reference.rs`'s `PushLayer` handling)
//! copies the same parent pixels into a fresh pooled buffer each time. The
//! six passes are pure: same input bytes, same `radius`, same direction →
//! same output bytes, bit for bit. That determinism is exactly what the
//! raster-identity gate's whole premise rests on, so a cache keyed on the
//! input *bits* — not on a scene identity that could alias — returns what
//! the un-cached kernel would have written, and `blur_cached`'s tests
//! assert that equality directly.
//!
//! The key hashes the band's pixel values (four `f32` bit patterns per
//! pixel, FNV-1a), plus the three kernel parameters the output actually
//! depends on: width, band height and `radius`. Note what is *not* in the
//! key: `sigma` itself. Two sigmas that round to the same `radius` produce
//! identical output by construction — the kernel never sees the difference —
//! so they are one entry, not two. A 64-bit hash collision would return one
//! band's blur for another's; at roughly 2^-64 per pair against an LRU
//! bounded at a few entries, that is far below the rate at which the
//! hardware this runs on flips a bit unasked, and the budgeted LRU
//! (`native/residency.rs`) bounds the memory the cache may hold — default
//! 8 MB, `VIEWW_BLUR_CACHE_MB` to move it, zero to turn the cache into a
//! pass-through. The blurred-shadow-patch cache (`native/shadow.rs`) is the
//! precedent this follows; layer blurs were the uncached sibling.

use vieww_foundation::filter::ColorMatrix;

use super::color::Premul;
use super::target::Target;

/// In-place 3-box approximation of a Gaussian blur, standard deviation
/// `sigma` device pixels, applied on premultiplied channels (blurring
/// premultiplied avoids a transparent neighbour tinting an opaque pixel,
/// which un-premultiplied blurring does not).
/// # Four channels at once, not four passes
///
/// This used to split the layer into four `f32` planes, blur each of them
/// separately, and interleave the result back — six full passes over the
/// buffer purely to move numbers around, and then twenty-four separable
/// passes over four separate allocations. It also read every sample through a
/// bounds-checked helper, including the overwhelming majority that are
/// nowhere near an edge, and its vertical pass walked *down columns* of a
/// row-major buffer, which is a cache miss per pixel.
///
/// A [`Premul`] is four floats side by side, so a running box sum over
/// `Premul` blurs all four channels together with the same number of
/// operations and a quarter of the memory traffic — no planes, no
/// deinterleave, one scratch buffer, and the vertical pass moving one whole
/// pixel at a time instead of one channel. The arithmetic is unchanged, so
/// the picture is too.
pub(crate) fn blur(target: &mut Target, sigma: f32, angle: Option<f32>) {
    match band_plan(target, sigma) {
        BandPlan::Skip => {}
        BandPlan::GrowOnly(reach) => target.grow_ink(reach),
        BandPlan::Blur { radius, top, bottom } => {
            let w = target.width;
            let band_h = bottom - top;
            let start = top as usize * w as usize;
            let rows = &mut target.pixels[start..start + band_h as usize * w as usize];
            blur_rows(rows, w, band_h, radius, angle);
            // **A blur moves ink outwards, and the composite has to know how
            // far.** Three box passes of half-width `radius` each reach
            // `3 * radius` in every direction; nothing beyond that can have
            // become non-transparent, because every sample feeding it was
            // transparent. Declaring exactly that keeps `Target::ink` an
            // honest upper bound on what a blurred layer contains —
            // under-reporting it here would clip the soft edge off every glow
            // and every shadow in the framework, which is the one way this
            // optimisation could be visible in a picture.
            target.grow_ink(3 * radius);
        }
    }
}

/// Where — and whether — a blur's six passes will run, decided before
/// anything is written.
///
/// Both [`blur`] and [`blur_cached`] must agree on this exactly: the
/// cache key's band is the band the passes would have run over, so a plan
/// computed one row wider or narrower would hand back a cached blur of a
/// region that was never asked about. One function, both callers — the
/// same single-source-of-truth discipline the kernels themselves follow.
enum BandPlan {
    /// Nothing to do at all (`sigma <= 0`, or a zero-sized buffer): the
    /// historical early-outs that did not even grow ink.
    Skip,
    /// No passable band — nothing was drawn, or the reach-grown band is
    /// empty — so the only work is declaring the blur's reach on the ink,
    /// exactly as those early-outs always did.
    GrowOnly(i32),
    /// The band of rows `[top, bottom)` the passes run over, at kernel
    /// radius `radius`.
    Blur { radius: i32, top: u32, bottom: u32 },
}

fn band_plan(target: &Target, sigma: f32) -> BandPlan {
    if sigma <= 0.0 {
        return BandPlan::Skip;
    }
    // See `shadow::box_blur_3`'s doc comment for why this is not `3 * sigma`.
    let diameter = (4.0 * sigma * sigma + 1.0).sqrt();
    let radius = (((diameter - 1.0) / 2.0).round() as i32).max(1);
    let (w, h) = (target.width, target.height);
    if w == 0 || h == 0 {
        return BandPlan::Skip;
    }

    // **Blur the band the layer actually inked, not the whole buffer.**
    //
    // A layer's buffer is sized to its declared bounds — for a sheet or an
    // overlay that is usually the whole window — while what was drawn into it
    // may be a card in the middle. Every pixel outside the ink is transparent,
    // and a box blur of transparency is transparency, so the rows above and
    // below the ink (plus the kernel's reach, which is where the falloff lives)
    // cannot change. Running the six separable passes over them anyway is the
    // dominant cost of every blurred layer in the framework.
    //
    // Rows only, not columns: the horizontal pass walks whole rows and the
    // vertical pass walks whole columns, so narrowing the column range would
    // mean strided access for one of them and a bookkeeping mistake waiting to
    // happen. Rows are the axis where the saving is, because the buffer is
    // row-major and a band of rows is a contiguous slice.
    let reach = 3 * radius;
    match target.ink() {
        Some((_, iy0, _, iy1)) => {
            let top = (iy0 - reach).max(0) as u32;
            let bottom = (iy1 + reach).min(h as i32).max(0) as u32;
            if bottom <= top {
                BandPlan::GrowOnly(reach)
            } else {
                BandPlan::Blur { radius, top, bottom }
            }
        }
        // Nothing was drawn: there is nothing to blur, and the whole buffer is
        // already transparent.
        None => BandPlan::GrowOnly(reach),
    }
}

/// The six separable passes themselves, over exactly the band
/// [`band_plan`] chose — the whole of what a blur writes, and the unit
/// [`blur_cached`] caches. The caller owns the ink accounting
/// (`blur`'s `grow_ink`); this only transforms rows.
fn blur_rows(rows: &mut [Premul], w: u32, band_h: u32, radius: i32, angle: Option<f32>) {
    // **The blur runs at full resolution, and a reduced-resolution version was
    // tried and removed.**
    //
    // Computing a wide blur on a half- or quarter-size copy and scaling the
    // result back up is what Core Animation, Skia and Chrome all do, and the
    // arithmetic is sound: past a certain width the output has no detail a
    // coarser grid cannot carry. It was implemented here in full — box
    // downsample, variance-corrected sigma, bilinear upsample — and then
    // deleted, because measuring it first would have saved the effort:
    //
    // * A callgrind profile of `examples/fixtures`' `23-editor-glass`, the
    //   most blur-heavy screen in the gallery, puts **7.5%** of the frame in
    //   this function. The other 92% is compositing and the output conversion.
    //   Sixteen-fold on 7.5% is worth 7%, and it changes what is drawn.
    // * Measured end to end, `00-blurs` moved from 8.15 ms to 7.39 ms and
    //   `23-editor-glass` from 32.21 ms to 32.46 ms — inside this machine's
    //   run-to-run spread, in both directions.
    // * The bilinear upsample called `f32::floor` twice per pixel, and on the
    //   baseline `x86-64` target that is a **call into `compiler-builtins`**
    //   rather than an instruction, which put 2.9% of the frame into `floorf`
    //   alone. The optimisation was paying most of its own winnings back.
    //
    // So: an approximation that changes output, for no measurable gain, on the
    // strength of an argument that is correct about everything except where the
    // time was going. Recorded because it is the obvious idea and someone will
    // have it again; the honest next step for blur-heavy frames is the
    // compositing loop, or a GPU. (And, since this cache exists, the frame
    // after this one.)
    let mut scratch = vec![Premul::TRANSPARENT; rows.len()];
    match angle {
        None => {
            for _ in 0..3 {
                horizontal(rows, &mut scratch, w, band_h, radius);
                vertical(&scratch, rows, w, band_h, radius);
            }
        }
        // todo-upgrades U-14: a motion blur shares the band selection and
        // the ink accounting with the isotropic one — only the kernel changes
        // direction. Three box passes along the ray, same radii, same
        // edge-clamped sampling U-15 fixed.
        Some(angle) => {
            let (mut dx, mut dy) = (angle.cos(), angle.sin());
            // Rays pointing left read the same as their mirror — a smear is
            // not a directed gradient.
            if dx < 0.0 {
                dx = -dx;
                dy = -dy;
            }
            for _ in 0..3 {
                directed(rows, &mut scratch, w, band_h, radius, dx, dy);
                directed(&scratch, rows, w, band_h, radius, dx, dy);
            }
        }
    }
}

/// The default memory the blur-result cache may hold — one full-window
/// 1366x679 band would not fit in it (14.8 MB), which is the point: the
/// cache is for the *repeated* bands of cards, panels and badges, and a
/// single band that size re-derives in about the time the frame spends
/// compositing anyway (the 7.5% figure in `blur_rows`' comment). The
/// memory targets this serves are in PENDING §4.2; `VIEWW_BLUR_CACHE_MB`
/// moves the number, `=0` disables the cache outright.
pub(crate) const DEFAULT_BLUR_CACHE_BUDGET_BYTES: usize = 8 * 1024 * 1024;

/// What one cached blur depends on, exactly.
///
/// The output of the six passes is a pure function of the input band's
/// bits, `w`, `band_h`, `radius` and the direction — so the key carries
/// those five and nothing else. Two fields deserve their "why":
///
/// * **The hash, not a `&[Premul]` compare.** The band is tens of
///   thousands of pixels; comparing it entry-by-entry would cost what the
///   cache exists to save. FNV-1a over the four `f32` bit patterns per
///   pixel is one sequential pass (cheaper than *one* box pass, let alone
///   six) and exact: the rasterizer is deterministic — the raster-identity
///   gate's own premise — so identical frames produce identical bits, and
///   `to_bits()` keeps that promise through the hash where `PartialEq`
///   on floats would not (-0.0 and 0.0 are "equal" but blur differently
///   through `add`/`sub` only in the last bit; hashing bits treats them
///   as the different inputs they are).
/// * **`radius`, not `sigma`.** Two sigmas rounding to the same radius
///   produce identical output — the kernel is never told the sigma — so
///   `5.0` and `5.2` are one entry. Keying on sigma bits would cache
///   duplicates the kernel cannot distinguish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct BlurKey {
    width: u32,
    band_h: u32,
    radius: i32,
    /// `angle.to_bits()` — `None` for the isotropic blur, which keys
    /// separately from any directed one even at identical content.
    angle: Option<u32>,
    hash: u64,
}

/// The blurred-band cache's type: budgeted, LRU-evicting, stats-carrying —
/// the same `ResidencyCache` the glyph outlines and the shadow patches use,
/// because the reasoning there (a derived, deterministic, read-only value
/// re-derivable at any time) applies to a blur word for word.
pub(crate) type BlurCache = super::residency::ResidencyCache<BlurKey, Vec<Premul>>;

/// A blur cache at the default budget, or the `VIEWW_BLUR_CACHE_MB` one
/// when that is set and parses — read here rather than at the call sites
/// so every renderer in the process gets the same trade without each of
/// them knowing the env var exists.
pub(crate) fn default_blur_cache() -> BlurCache {
    let budget = std::env::var("VIEWW_BLUR_CACHE_MB")
        .ok()
        .as_deref()
        .and_then(super::pool::mb_env_bytes)
        .unwrap_or(DEFAULT_BLUR_CACHE_BUDGET_BYTES);
    super::residency::ResidencyCache::new(budget)
}

/// FNV-1a 64, byte-wise over each channel's IEEE bit pattern. Byte-wise
/// rather than word-wise so a change in any of the 32 bits spreads; the
/// constants are the standard ones, and the loop is over `Premul`s the
/// band walk already has in cache.
fn hash_band(rows: &[Premul]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for pixel in rows {
        for channel in [pixel.r, pixel.g, pixel.b, pixel.a] {
            for byte in channel.to_bits().to_le_bytes() {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(0x100_0000_01b3);
            }
        }
    }
    hash
}

/// [`blur`], with its result remembered: a layer whose band matches one
/// already blurred at the same radius and direction gets the finished
/// band copied back instead of six passes recomputing it.
///
/// The contract is strict because the renderer's output is: **the cached
/// path writes the same bytes the uncached path would have.** On a miss
/// the passes run exactly as [`blur`] runs them ([`blur_rows`], fed by
/// the same [`band_plan`]) and the finished band is stored; on a hit the
/// stored band is copied over the input. Both paths end with the same
/// `grow_ink`, so the ink accounting — the part a cached blur could most
/// plausibly get wrong, and the one way a wrong cache would become
/// visible — is identical by construction. The tests in this module run
/// both paths over the same content and diff them.
pub(crate) fn blur_cached(
    target: &mut Target,
    sigma: f32,
    angle: Option<f32>,
    cache: &mut BlurCache,
) {
    if sigma <= 0.0 {
        return;
    }
    match band_plan(target, sigma) {
        BandPlan::Skip => {}
        BandPlan::GrowOnly(reach) => target.grow_ink(reach),
        BandPlan::Blur { radius, top, bottom } => {
            let w = target.width;
            let band_h = bottom - top;
            let start = top as usize * w as usize;
            let rows = &mut target.pixels[start..start + band_h as usize * w as usize];
            // Key on the *input* bits: the value stored under it is the
            // band after the passes, so identical input can only have
            // identical output (the kernel is pure), and a collision would
            // need two 2^-64-class events in the same process to matter.
            let key = BlurKey {
                width: w,
                band_h,
                radius,
                angle: angle.map(f32::to_bits),
                hash: hash_band(rows),
            };
            match cache.lookup(&key) {
                Some(cached) => rows.copy_from_slice(cached),
                None => {
                    blur_rows(rows, w, band_h, radius, angle);
                    let band = rows.to_vec();
                    cache.insert(key, band, |band| {
                        band.len() * std::mem::size_of::<Premul>()
                    });
                }
            }
            // The ink accounting, shared with [`blur`] — the reach a
            // composite must respect does not depend on where the pixels
            // came from.
            target.grow_ink(3 * radius);
        }
    }
}

/// A running-sum box blur along each row.
///
/// **Samples past a row's ends repeat the end pixel** (todo-upgrades U-15):
/// the convention CSS `filter: blur()` specifies, and the same one
/// `vieww_foundation::blur_rgba` now uses — the two kernels are compared as
/// oracles and must agree at the edges as well as the middle. This used to
/// count them as transparent, which faded a full-bleed layer's rim toward
/// nothing at its own buffer edge — every frosted surface darkening along
/// its own edges, invisible on a single panel and unmistakable across a
/// window. For content that stops inside the blur-reach-grown buffer the
/// two conventions agree, because the pixels being repeated are transparent
/// either way. The window is advanced by adding the entering sample and
/// subtracting the leaving one, so the cost is per pixel rather than per
/// pixel per radius.
fn horizontal(src: &[Premul], dst: &mut [Premul], w: u32, h: u32, radius: i32) {
    // Divided, not multiplied by a precomputed reciprocal: `x / w` and
    // `x * (1/w)` differ in the last bit, and this kernel is a correctness
    // oracle before it is a fast path. The fixture gallery caught the
    // difference as six changed pixels in a blurred panel.
    let window = (2 * radius + 1) as f32;
    let width = w as i32;
    for y in 0..h as usize {
        let row = y * w as usize;
        let src_row = &src[row..row + w as usize];
        let dst_row = &mut dst[row..row + w as usize];

        // Seed: the window at x = 0 is positions -radius..=radius with the
        // left out-of-range half clamped onto column 0, which therefore
        // counts radius + 1 times (todo-upgrades U-15).
        let mut sum = src_row[0];
        for _ in 0..radius {
            sum = add(sum, src_row[0]);
        }
        for x in 1..=radius.min(width - 1) {
            sum = add(sum, src_row[x as usize]);
        }
        if width <= radius {
            for _ in 0..(radius + 1 - width) {
                sum = add(sum, src_row[(width - 1) as usize]);
            }
        }
        // `sum += entering - leaving`, as one expression per channel rather
        // than an add followed by a subtract. Floating-point addition is not
        // associative, so the two spellings can differ in the last bit — and
        // this renderer is the oracle every other backend is diffed against,
        // so "the same picture, one bit different" is a worse answer than it
        // sounds. Measured on the fixture gallery: splitting them moved 446
        // pixels of a blurred panel by exactly 1/255.
        //
        // **Three spans, one arithmetic.** Only the first `radius` columns
        // clamp the leaving sample and only the last `radius + 1` clamp the
        // entering one; in between both are plain offsets. The middle span is
        // walked as three zipped slices, so it carries no clamps and no
        // bounds checks — those were most of this loop's instructions (a
        // callgrind profile of `blur_bench`: ~25 per pixel per pass) — while
        // every pixel still gets exactly the same divide, subtract and add in
        // the same order. The output is unchanged to the bit.
        let step = |sum: &mut Premul, dst: &mut Premul, x: i32| {
            *dst = divide(*sum, window);
            let entering = (x + radius + 1).min(width - 1);
            let leaving = (x - radius).max(0);
            *sum = add(
                *sum,
                sub(src_row[entering as usize], src_row[leaving as usize]),
            );
        };
        // Middle: x - radius >= 0 and x + radius + 1 <= width - 1.
        let mid_start = radius.min(width);
        let mid_end = (width - radius - 1).max(mid_start);
        for x in 0..mid_start {
            step(&mut sum, &mut dst_row[x as usize], x);
        }
        let (r, ms, me) = (radius as usize, mid_start as usize, mid_end as usize);
        if ms < me {
            for ((out, entering), leaving) in dst_row[ms..me]
                .iter_mut()
                .zip(&src_row[ms + r + 1..me + r + 1])
                .zip(&src_row[ms - r..me - r])
            {
                *out = divide(sum, window);
                sum = add(sum, sub(*entering, *leaving));
            }
        }
        for x in mid_end..width {
            step(&mut sum, &mut dst_row[x as usize], x);
        }
    }
}

/// How many columns the vertical pass carries at once — see [`vertical`].
///
/// Four `Premul`s are 64 bytes, which is one cache line on every target this
/// runs on. Eight is two lines per row per strip: enough that the loads are
/// whole lines rather than quarters, small enough that eight running sums plus
/// their addresses stay in registers.
const STRIP: usize = 8;

/// The same, down each column — but eight columns at a time.
///
/// # Why a strip, and why this is not an approximation
///
/// Column-major access over a row-major buffer is the one genuinely
/// cache-hostile step in a separable blur. Walking a single column touches one
/// `Premul` — 16 bytes — out of every 64-byte line the hardware fetches, so
/// three quarters of the memory traffic of the vertical pass was fetching
/// neighbours it was about to throw away and then come back for. A blurred
/// full-window layer runs this three times.
///
/// Carrying `STRIP` adjacent columns down the buffer together uses the whole
/// line while it is resident. Each column keeps its own running sum, exactly as
/// before, and the sums never interact — so this is the same arithmetic in the
/// same order *per column*, only interleaved across columns. The output is
/// bit-for-bit what the single-column loop produced, which matters here more
/// than usual: this kernel is a correctness reference, and `horizontal`'s own
/// comment records a previous change to it that moved 446 pixels by 1/255.
///
/// # What it is worth, and a warning about how it was nearly measured
///
/// `examples/blur_bench.rs`, one full-window blurred layer at 1366x768, best of
/// twelve renders per run, six runs: **43.3 ms with the strip against 58.6 ms
/// without** — about 26%.
///
/// Those six runs are not ceremony. A single run of each said 55.4 ms for the
/// strip and 53.8 ms without it, which is the *opposite* conclusion, and it was
/// nearly acted on: the change was reverted on that number before a repeat run
/// showed the two distributions barely overlap. This machine's spread on the
/// same binary is wider than the effect being measured, so a before/after taken
/// one sample each is not evidence of anything, in either direction.
fn vertical(src: &[Premul], dst: &mut [Premul], w: u32, h: u32, radius: i32) {
    // Divided rather than multiplied by a reciprocal — see `horizontal`.
    let window = (2 * radius + 1) as f32;
    let (width, height) = (w as usize, h as i32);

    let mut x0 = 0usize;
    while x0 < width {
        let strip = STRIP.min(width - x0);
        let mut sums = [Premul::TRANSPARENT; STRIP];

        // Edge-clamped seed (todo-upgrades U-15), same arithmetic as
        // `horizontal`'s: row 0 counts radius + 1 times, rows past the
        // bottom repeat the last row.
        for (i, sum) in sums.iter_mut().take(strip).enumerate() {
            *sum = src[x0 + i];
        }
        for _ in 0..radius {
            for (i, sum) in sums.iter_mut().take(strip).enumerate() {
                *sum = add(*sum, src[x0 + i]);
            }
        }
        for y in 1..=radius.min(height - 1) {
            let row = y as usize * width + x0;
            for (i, sum) in sums.iter_mut().take(strip).enumerate() {
                *sum = add(*sum, src[row + i]);
            }
        }
        if height <= radius {
            let row = (height - 1) as usize * width + x0;
            for _ in 0..(radius + 1 - height) {
                for (i, sum) in sums.iter_mut().take(strip).enumerate() {
                    *sum = add(*sum, src[row + i]);
                }
            }
        }

        for y in 0..height {
            let out_row = y as usize * width + x0;
            // One expression per column, for the reason `horizontal` gives.
            let entering = (y + radius + 1).min(height - 1) as usize * width + x0;
            let leaving = (y - radius).max(0) as usize * width + x0;
            // Zipped slices rather than `src[row + i]` per column: the same
            // operations in the same order, without a bounds check per
            // sample (see `horizontal`).
            for (((sum, out), entering), leaving) in sums[..strip]
                .iter_mut()
                .zip(&mut dst[out_row..out_row + strip])
                .zip(&src[entering..entering + strip])
                .zip(&src[leaving..leaving + strip])
            {
                *out = divide(*sum, window);
                *sum = add(*sum, sub(*entering, *leaving));
            }
        }
        x0 += strip;
    }
}

/// One box pass along `(dx, dy)` — the directional sibling of the separable
/// pair (todo-upgrades U-14).
///
/// A ray is not axis-aligned, so neither the running-sum trick nor the
/// column strip applies: every tap is a gather, rounded to a whole pixel
/// (a bilinear tap would smear two pixels wide and halve the reach) and
/// clamped at the band's edges exactly as `horizontal` and `vertical` now
/// clamp at theirs (U-15). Cost is per pixel per tap, paid only when a
/// caller asks for a direction.
fn directed(src: &[Premul], dst: &mut [Premul], w: u32, h: u32, radius: i32, dx: f32, dy: f32) {
    // Divided rather than multiplied by a reciprocal — see `horizontal`.
    let window = (2 * radius + 1) as f32;
    let (width, height) = (w as usize, h as usize);
    let max_x = (width - 1) as f32;
    let max_y = (height - 1) as f32;
    for y in 0..height {
        for x in 0..width {
            let mut sum = Premul::TRANSPARENT;
            for t in -radius..=radius {
                let step = t as f32;
                let sx = (x as f32 + step * dx).round().clamp(0.0, max_x) as usize;
                let sy = (y as f32 + step * dy).round().clamp(0.0, max_y) as usize;
                sum = add(sum, src[sy * width + sx]);
            }
            dst[y * width + x] = divide(sum, window);
        }
    }
}

fn add(a: Premul, b: Premul) -> Premul {
    Premul {
        r: a.r + b.r,
        g: a.g + b.g,
        b: a.b + b.b,
        a: a.a + b.a,
    }
}

fn sub(a: Premul, b: Premul) -> Premul {
    Premul {
        r: a.r - b.r,
        g: a.g - b.g,
        b: a.b - b.b,
        a: a.a - b.a,
    }
}

fn divide(p: Premul, k: f32) -> Premul {
    Premul {
        r: p.r / k,
        g: p.g / k,
        b: p.b / k,
        a: p.a / k,
    }
}

/// Apply a 5x4 colour matrix to every pixel, in **straight** alpha (spec
/// §8.2: "applies to straight-alpha colours per the reference kernel's
/// premultiplied-to-straight round trip").
pub(crate) fn color_matrix(target: &mut Target, matrix: &ColorMatrix) {
    // A matrix has offsets in its fifth column, so a fully transparent pixel
    // can come out visible. Every pixel is therefore potentially ink.
    target.mark_all_ink();
    for p in &mut target.pixels {
        let a = p.a.max(1e-6);
        let (r, g, b, alpha) = (p.r / a, p.g / a, p.b / a, p.a);
        let apply = |row: usize| -> f32 {
            matrix[row * 5] * r
                + matrix[row * 5 + 1] * g
                + matrix[row * 5 + 2] * b
                + matrix[row * 5 + 3] * alpha
                + matrix[row * 5 + 4]
        };
        let nr = apply(0).clamp(0.0, 1.0);
        let ng = apply(1).clamp(0.0, 1.0);
        let nb = apply(2).clamp(0.0, 1.0);
        let na = apply(3).clamp(0.0, 1.0);
        *p = Premul {
            r: nr * na,
            g: ng * na,
            b: nb * na,
            a: na,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::residency::ResidencyCache;

    /// Deterministic test content: a hard diagonal ramp with two solid
    /// blocks, so every row differs from every other and any off-by-one or
    /// re-ordering in the key's hashing shows up as a false hit or a wrong
    /// copy rather than as a pass. The ink stops short of the buffer's
    /// edges, so the band logic (rows outside the ink) is exercised too.
    fn inked_target(w: u32, h: u32) -> Target {
        let mut target = Target::new(w, h);
        for y in 2..h as i32 - 2 {
            for x in 2..w as i32 - 2 {
                let v = ((x * 7 + y * 13) % 255) as f32 / 255.0;
                target.set(x, y, Premul { r: v, g: v * 0.5, b: 0.25, a: v });
            }
        }
        target
    }

    fn fresh_cache() -> BlurCache {
        ResidencyCache::new(usize::MAX)
    }

    #[test]
    fn the_cached_and_uncached_paths_write_the_same_bytes() {
        let mut plain = inked_target(24, 20);
        let mut cached = inked_target(24, 20);
        let mut cache = fresh_cache();
        blur(&mut plain, 2.5, None);
        blur_cached(&mut cached, 2.5, None, &mut cache);
        assert_eq!(plain.pixels, cached.pixels, "pixels, bit for bit");
        assert_eq!(plain.ink(), cached.ink(), "the ink accounting too");
        assert_eq!(cache.stats().misses, 1);
        assert_eq!(cache.stats().hits, 0);
    }

    #[test]
    fn a_hit_returns_the_derived_band_without_rederiving() {
        let mut cache = fresh_cache();
        let mut first = inked_target(24, 20);
        blur_cached(&mut first, 2.5, None, &mut cache);
        let reference = first.pixels.clone();
        let mut second = inked_target(24, 20);
        blur_cached(&mut second, 2.5, None, &mut cache);
        assert_eq!(second.pixels, reference);
        assert_eq!(cache.stats().hits, 1, "one derive, one reuse");
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn a_single_changed_pixel_is_a_different_key() {
        let mut cache = fresh_cache();
        let mut a = inked_target(24, 20);
        blur_cached(&mut a, 2.5, None, &mut cache);
        let mut b = inked_target(24, 20);
        b.set(10, 10, Premul { r: 1.0, g: 1.0, b: 1.0, a: 1.0 });
        blur_cached(&mut b, 2.5, None, &mut cache);
        assert_eq!(cache.stats().hits, 0, "changed content must not hit");
        assert_eq!(cache.stats().misses, 2);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn sigmas_that_round_to_the_same_radius_share_an_entry() {
        // radius = round((sqrt(4*sigma^2 + 1) - 1) / 2): 5.0 gives 5, and so
        // does 5.2. The kernel never sees the sigma, only the radius, so the
        // two are the same blur and the key must say so.
        let mut cache = fresh_cache();
        let mut a = inked_target(24, 20);
        blur_cached(&mut a, 5.0, None, &mut cache);
        let mut b = inked_target(24, 20);
        blur_cached(&mut b, 5.2, None, &mut cache);
        assert_eq!(cache.len(), 1, "same radius, same content, one entry");
        assert_eq!(cache.stats().hits, 1);
        assert_eq!(a.pixels, b.pixels);
    }

    #[test]
    fn a_directed_blur_keys_separately_from_the_isotropic_one() {
        let mut cache = fresh_cache();
        let mut a = inked_target(24, 20);
        blur_cached(&mut a, 2.5, None, &mut cache);
        let mut b = inked_target(24, 20);
        blur_cached(&mut b, 2.5, Some(0.7), &mut cache);
        assert_eq!(cache.len(), 2, "same content, different kernel direction");
        assert_eq!(cache.stats().hits, 0);
    }

    #[test]
    fn an_inkless_layer_grows_ink_without_touching_the_cache() {
        // `band_plan`'s GrowOnly path: nothing was drawn, the whole buffer
        // is transparent, and there is nothing to cache — the historical
        // early-out must stay out of the key space entirely.
        let mut cache = fresh_cache();
        let mut target = Target::new(16, 16);
        blur_cached(&mut target, 2.5, None, &mut cache);
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.stats().misses, 0);
        assert_eq!(cache.stats().hits, 0);
    }

    #[test]
    fn a_budgeted_cache_keeps_the_most_recent_band_working() {
        // One band of this content is 24*20 pixels; a budget of exactly
        // that band evicts the first entry when the second derives, and the
        // third identical render re-derives (miss) and still produces the
        // same bytes — eviction may cost time, never correctness.
        let band_bytes = 24 * 20 * std::mem::size_of::<Premul>();
        let mut cache = ResidencyCache::new(band_bytes);
        let mut a = inked_target(24, 20);
        blur_cached(&mut a, 2.5, None, &mut cache);
        let mut b = inked_target(24, 20);
        b.set(5, 5, Premul { r: 0.0, g: 0.0, b: 0.0, a: 1.0 });
        blur_cached(&mut b, 2.5, None, &mut cache);
        assert_eq!(cache.stats().evictions, 1);
        assert_eq!(cache.len(), 1);
        let mut again = inked_target(24, 20);
        blur_cached(&mut again, 2.5, None, &mut cache);
        assert_eq!(
            again.pixels,
            a.pixels,
            "an evicted entry re-derives to the same bytes"
        );
    }

    #[test]
    fn the_cached_directed_blur_matches_the_uncached_kernel_too() {
        // The directed kernel shares `band_plan` but not the separable
        // pair; cache it through its own keying and diff against `blur`.
        let mut plain = inked_target(20, 24);
        let mut cached = inked_target(20, 24);
        let mut cache = fresh_cache();
        blur(&mut plain, 3.0, Some(0.9));
        blur_cached(&mut cached, 3.0, Some(0.9), &mut cache);
        assert_eq!(plain.pixels, cached.pixels);
    }

    #[test]
    fn a_negative_sigma_is_a_no_op_on_both_paths() {
        let mut cache = fresh_cache();
        let mut a = inked_target(24, 20);
        let reference = a.pixels.clone();
        blur(&mut a, -1.0, None);
        blur_cached(&mut a, 0.0, None, &mut cache);
        assert_eq!(a.pixels, reference);
        assert_eq!(cache.len(), 0);
    }
}
