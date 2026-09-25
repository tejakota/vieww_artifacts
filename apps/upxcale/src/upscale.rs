//! The thing the application is actually for: making a photo bigger without
//! making it softer.
//!
//! This is a real resampler, not a placeholder. The HTML prototype faked the
//! result with a CSS `filter: contrast() saturate() brightness()` over a
//! higher-resolution copy of the same file, which is the right move for a
//! prototype and the wrong one for an app — there, "upscaled" was a look; here
//! it has to be pixels that were not in the source.
//!
//! # Why Lanczos rather than bilinear
//!
//! Enlarging is reconstruction: the output asks for samples that lie between
//! the ones the sensor recorded, and the kernel is the guess about what was
//! there. Bilinear guesses with a triangle two pixels wide, which is cheap and
//! blurs, because a triangle in the spatial domain is a poor low-pass in the
//! frequency domain and throws away the high frequencies that read as detail.
//! Lanczos is a windowed `sinc` — the ideal reconstruction filter, truncated at
//! three lobes so it is finite — and it keeps those frequencies. It costs a
//! six-tap convolution per axis instead of two, which for the sizes here is
//! nothing next to the memory traffic.
//!
//! The cost of a windowed sinc is that its negative lobes overshoot at a hard
//! edge, which shows up as a faint light rim on the bright side and a dark one
//! on the dark side. That is the well-known trade and it is why `A` is 3 rather
//! than a larger, sharper window.
//!
//! # Two passes, not one
//!
//! The kernel is separable — `L(x, y) == L(x) * L(y)` — so a horizontal pass
//! followed by a vertical one is the same result as a 2D convolution, at
//! `2 * A * 2` taps per pixel instead of `(2 * A * 2)²`. For `A = 3` that is 12
//! multiplies rather than 144.
//!
//! # Alpha is premultiplied for the duration
//!
//! `vieww_foundation::Image` is *straight* alpha (`image.rs`), and resampling
//! straight alpha is wrong wherever alpha varies: a transparent pixel's colour
//! channels hold whatever was left in them, and averaging them into a
//! neighbouring opaque pixel drags that colour in — the classic dark halo
//! around a cut-out. So both passes run premultiplied and the result is
//! divided back out at the end. Photographs are opaque and would not notice;
//! the app does not get to assume its inputs are photographs.

use vieww_foundation::Image;

/// Lanczos window size, in lobes. Three is the usual choice for enlargement.
const A: f32 = 3.0;

/// How much of the high-pass to add back when sharpening.
///
/// 1.0 — the mask is added back at full strength. That is a strong setting for
/// a general-purpose sharpener and a reasonable one here, because the pass is
/// not decorating a photograph so much as returning acutance the enlargement
/// just took: the mask is built from the enlarged image, so what it finds *is*
/// what the resampling smeared.
///
/// Measured against the shipped photographs this lifts acutance about 13% over
/// a plain enlargement of the same size — see
/// `tests/upscale.rs::sharpening_measurably_improves_the_shipped_photographs`,
/// which is the test that decides whether this number is doing anything.
/// Pushing it past roughly 1.5 starts putting a visible light rim on the sky
/// side of a ridge line, which is the point where it stops being recovery.
const SHARPEN_AMOUNT: f32 = 1.0;

/// The unsharp mask's blur radius, **per unit of enlargement**.
///
/// This has to scale with the factor and it is not obvious why until you have
/// got it wrong: an edge that spanned one pixel in the source spans `factor`
/// pixels after enlargement, so a mask blurred at a fixed 0.9px finds almost no
/// difference to add back. The first version of this file used a constant here
/// and the sharpening pass measured as doing *nothing at all* on a real
/// photograph — the effect was there on a synthetic checkerboard, whose edges
/// are extreme enough to survive any threshold, which is exactly the kind of
/// test input that hides this.
///
/// Half the factor puts the mask's radius at the scale of the features the
/// enlargement actually smeared.
const SHARPEN_SIGMA_PER_FACTOR: f32 = 0.5;

/// Differences below this (0..=1 scale) are left alone, so film grain and
/// sensor noise are not sharpened into speckle along with the edges.
///
/// Deliberately small. After enlargement the local differences *are* small —
/// that is the whole problem being corrected — so a threshold set for
/// source-resolution noise suppresses the signal along with it.
const SHARPEN_THRESHOLD: f32 = 0.003;

/// `sinc(x) = sin(pi x) / (pi x)`, with the removable singularity at 0 filled in.
fn sinc(x: f32) -> f32 {
    if x.abs() < 1.0e-6 {
        1.0
    } else {
        let pi_x = std::f32::consts::PI * x;
        pi_x.sin() / pi_x
    }
}

/// The Lanczos kernel: a `sinc` windowed by a wider, stretched `sinc`.
fn lanczos(x: f32) -> f32 {
    let x = x.abs();
    if x >= A {
        0.0
    } else {
        sinc(x) * sinc(x / A)
    }
}

/// The taps that produce one output sample: the first source index, and the
/// weights to apply from there.
struct Taps {
    first: isize,
    weights: Vec<f32>,
}

/// Precompute the taps for every output position along one axis.
///
/// Doing this once per axis rather than once per pixel is the whole reason a
/// separable filter is affordable: an output row of width `w` reuses the same
/// `w` weight sets for every one of its rows.
///
/// Note the `scale >= 1.0` clamp on the filter's footprint. When *downscaling*
/// the kernel has to be stretched to the output pitch or it aliases; when
/// enlarging it must not be, or the result is a blur. This function handles
/// both so it stays correct if it is ever asked to shrink.
fn plan(src_len: u32, dst_len: u32) -> Vec<Taps> {
    let scale = dst_len as f32 / src_len as f32;
    // Enlarging: sample the kernel at its natural width. Shrinking: widen it so
    // each output sample averages every source pixel that lands in it.
    let filter_scale = if scale < 1.0 { 1.0 / scale } else { 1.0 };
    let support = A * filter_scale;

    (0..dst_len)
        .map(|i| {
            // Map the output pixel's *centre* to source coordinates. The half
            // pixel on each side is what keeps the image from drifting by half
            // a pixel toward the origin — the single most common off-by-one in
            // a resampler.
            let center = (i as f32 + 0.5) / scale - 0.5;
            let first = (center - support).ceil() as isize;
            let last = (center + support).floor() as isize;

            let mut weights = Vec::with_capacity((last - first + 1).max(0) as usize);
            let mut total = 0.0;
            for s in first..=last {
                let w = lanczos((s as f32 - center) / filter_scale);
                weights.push(w);
                total += w;
            }
            // Normalise so flat areas keep their exact value. Without this the
            // kernel's own sum (which is only approximately 1, and is clipped
            // further at the edges) shows up as brightness ripple.
            if total.abs() > 1.0e-6 {
                for w in &mut weights {
                    *w /= total;
                }
            }
            Taps { first, weights }
        })
        .collect()
}

/// Clamp a source index to the image, which extends the edge pixel outward.
///
/// Edge handling has to be *some* choice; extending is the one that does not
/// invent a dark border (zero-padding) or wrap the far side of the picture into
/// this one (tiling).
fn clamp_index(i: isize, len: u32) -> usize {
    i.clamp(0, len as isize - 1) as usize
}

/// Resample `src` to exactly `dst_w` x `dst_h`.
///
/// Returns straight-alpha RGBA8, the same convention it was given.
#[must_use]
pub fn resample(src: &Image, dst_w: u32, dst_h: u32) -> Image {
    let (sw, sh) = (src.width(), src.height());
    if sw == 0 || sh == 0 || dst_w == 0 || dst_h == 0 {
        return Image::from_rgba8(vec![0; (dst_w * dst_h * 4) as usize], dst_w, dst_h);
    }

    let pixels = src.pixels();

    // ── premultiply, into f32 ────────────────────────────────────────────────
    // f32 for the whole pipeline: two rounds of rounding to u8 between the
    // passes would quantise the intermediate, and the horizontal pass's output
    // is exactly where the precision is needed.
    let mut linear = vec![0.0_f32; (sw * sh * 4) as usize];
    for (i, chunk) in pixels.chunks_exact(4).enumerate() {
        let a = f32::from(chunk[3]) / 255.0;
        linear[i * 4] = f32::from(chunk[0]) / 255.0 * a;
        linear[i * 4 + 1] = f32::from(chunk[1]) / 255.0 * a;
        linear[i * 4 + 2] = f32::from(chunk[2]) / 255.0 * a;
        linear[i * 4 + 3] = a;
    }

    // ── horizontal pass: (sw x sh) -> (dst_w x sh) ──────────────────────────
    let plan_x = plan(sw, dst_w);
    let mut mid = vec![0.0_f32; (dst_w * sh * 4) as usize];
    for y in 0..sh as usize {
        let row = y * sw as usize * 4;
        let out_row = y * dst_w as usize * 4;
        for (x, taps) in plan_x.iter().enumerate() {
            let mut acc = [0.0_f32; 4];
            for (k, &w) in taps.weights.iter().enumerate() {
                let sx = clamp_index(taps.first + k as isize, sw);
                let p = row + sx * 4;
                acc[0] += linear[p] * w;
                acc[1] += linear[p + 1] * w;
                acc[2] += linear[p + 2] * w;
                acc[3] += linear[p + 3] * w;
            }
            let o = out_row + x * 4;
            mid[o..o + 4].copy_from_slice(&acc);
        }
    }

    // ── vertical pass: (dst_w x sh) -> (dst_w x dst_h) ──────────────────────
    let plan_y = plan(sh, dst_h);
    let mut out = vec![0.0_f32; (dst_w * dst_h * 4) as usize];
    for (y, taps) in plan_y.iter().enumerate() {
        let out_row = y * dst_w as usize * 4;
        for x in 0..dst_w as usize {
            let mut acc = [0.0_f32; 4];
            for (k, &w) in taps.weights.iter().enumerate() {
                let sy = clamp_index(taps.first + k as isize, sh);
                let p = (sy * dst_w as usize + x) * 4;
                acc[0] += mid[p] * w;
                acc[1] += mid[p + 1] * w;
                acc[2] += mid[p + 2] * w;
                acc[3] += mid[p + 3] * w;
            }
            let o = out_row + x * 4;
            out[o..o + 4].copy_from_slice(&acc);
        }
    }

    Image::from_rgba8(unpremultiply(&out), dst_w, dst_h)
}

/// Divide the colour channels back out by alpha and quantise to `u8`.
fn unpremultiply(linear: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(linear.len());
    for px in linear.chunks_exact(4) {
        // The negative lobes can push a channel below zero or past one; clamp
        // before the divide so a tiny alpha cannot amplify an out-of-range
        // colour into a bright speck.
        let a = px[3].clamp(0.0, 1.0);
        if a <= 1.0e-6 {
            bytes.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            for c in &px[..3] {
                bytes.push(to_u8(c.clamp(0.0, a) / a));
            }
            bytes.push(to_u8(a));
        }
    }
    bytes
}

fn to_u8(v: f32) -> u8 {
    // `+ 0.5` then truncate rounds to nearest; the clamp keeps the cast in
    // range so the `as` conversion cannot wrap.
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// A one-dimensional Gaussian kernel, normalised, truncated at three sigma.
fn gaussian_kernel(sigma: f32) -> Vec<f32> {
    let radius = (sigma * 3.0).ceil().max(1.0) as isize;
    let two_sigma_sq = 2.0 * sigma * sigma;
    let mut kernel: Vec<f32> = (-radius..=radius)
        .map(|i| (-(i * i) as f32 / two_sigma_sq).exp())
        .collect();
    let total: f32 = kernel.iter().sum();
    for k in &mut kernel {
        *k /= total;
    }
    kernel
}

/// Unsharp mask: add back a scaled copy of what a blur removed.
///
/// The name is the darkroom one — the mask *is* the unsharp (blurred) copy, and
/// subtracting it from the original leaves only what the blur destroyed, which
/// is the high-frequency detail. Adding a fraction of that back exaggerates the
/// local contrast at edges, which the eye reads as sharpness.
///
/// Resampling is a low-pass whatever the kernel, so some of this is recovering
/// what the enlargement cost rather than inventing acutance the source never
/// had. `SHARPEN_THRESHOLD` is what keeps it from also amplifying the noise.
#[must_use]
pub fn sharpen(image: &Image, amount: f32, sigma: f32, threshold: f32) -> Image {
    let (w, h) = (image.width(), image.height());
    if w == 0 || h == 0 || amount <= 0.0 {
        return image.clone();
    }

    let src = image.pixels();
    let kernel = gaussian_kernel(sigma);
    let radius = (kernel.len() / 2) as isize;

    // Only the three colour channels are blurred and sharpened; alpha is
    // carried through untouched. Sharpening alpha would put a halo on the
    // *shape* of a cut-out, which is never what is wanted.
    let mut blurred_h = vec![0.0_f32; (w * h * 3) as usize];
    for y in 0..h as usize {
        for x in 0..w as usize {
            let mut acc = [0.0_f32; 3];
            for (k, &weight) in kernel.iter().enumerate() {
                let sx = clamp_index(x as isize + k as isize - radius, w);
                let p = (y * w as usize + sx) * 4;
                for c in 0..3 {
                    acc[c] += f32::from(src[p + c]) / 255.0 * weight;
                }
            }
            let o = (y * w as usize + x) * 3;
            blurred_h[o..o + 3].copy_from_slice(&acc);
        }
    }

    let mut out = Vec::with_capacity(src.len());
    for y in 0..h as usize {
        for x in 0..w as usize {
            let mut blurred = [0.0_f32; 3];
            for (k, &weight) in kernel.iter().enumerate() {
                let sy = clamp_index(y as isize + k as isize - radius, h);
                let p = (sy * w as usize + x) * 3;
                for c in 0..3 {
                    blurred[c] += blurred_h[p + c] * weight;
                }
            }
            let p = (y * w as usize + x) * 4;
            for c in 0..3 {
                let original = f32::from(src[p + c]) / 255.0;
                let detail = original - blurred[c];
                // Below the threshold this is noise, not an edge: leave it.
                let boosted = if detail.abs() < threshold {
                    original
                } else {
                    original + detail * amount
                };
                out.push(to_u8(boosted));
            }
            out.push(src[p + 3]);
        }
    }

    Image::from_rgba8(out, w, h)
}

/// Sharpen an image that has just been enlarged by `factor`.
///
/// The second half of [`upscale`], exposed on its own because
/// [`RenderJob`](crate::render::RenderJob) runs the two stages separately so it
/// can report progress between them. Both paths call *this*, so the parameters
/// cannot drift apart — an earlier version had the worker passing its own
/// literals, which is how one of them ends up tuned and the other forgotten.
#[must_use]
pub fn sharpen_for_factor(enlarged: &Image, factor: u32) -> Image {
    let sigma = SHARPEN_SIGMA_PER_FACTOR * factor.max(1) as f32;
    sharpen(enlarged, SHARPEN_AMOUNT, sigma, SHARPEN_THRESHOLD)
}

/// Enlarge by an integer factor and restore the acutance the enlargement cost.
///
/// This is what the Upscale button runs. `factor` is the app's "4x".
#[must_use]
pub fn upscale(source: &Image, factor: u32) -> Image {
    let factor = factor.max(1);
    let enlarged = resample(
        source,
        source.width().saturating_mul(factor),
        source.height().saturating_mul(factor),
    );
    sharpen_for_factor(&enlarged, factor)
}

/// A cheap measure of how much fine detail an image carries.
///
/// The mean absolute Laplacian: for every pixel, how far its luminance sits
/// from the average of its four neighbours. Flat areas contribute nothing and
/// edges contribute a lot, so the number rises with acutance and is the
/// straightforward way to check that [`upscale`] is doing something a plain
/// enlargement would not. `tests/upscale.rs` asserts on exactly that.
#[must_use]
pub fn acutance(image: &Image) -> f32 {
    let (w, h) = (image.width(), image.height());
    if w < 3 || h < 3 {
        return 0.0;
    }
    let px = image.pixels();
    let luma = |x: usize, y: usize| -> f32 {
        let p = (y * w as usize + x) * 4;
        // Rec. 601 luma. The exact coefficients matter less than using the same
        // ones on both sides of a comparison.
        0.299 * f32::from(px[p]) + 0.587 * f32::from(px[p + 1]) + 0.114 * f32::from(px[p + 2])
    };

    let mut total = 0.0;
    for y in 1..h as usize - 1 {
        for x in 1..w as usize - 1 {
            let neighbours = luma(x - 1, y) + luma(x + 1, y) + luma(x, y - 1) + luma(x, y + 1);
            total += (4.0 * luma(x, y) - neighbours).abs();
        }
    }
    total / ((w - 2) * (h - 2)) as f32
}
