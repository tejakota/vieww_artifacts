//! Image filters: what a layer's pixels go through on their way to the screen.
//!
//! # Why this is a layer property and not a widget property
//!
//! A blur is not a property of a shape, it is a property of a *group*. Blurring
//! a card means blurring the card, its border, its shadow and its text together
//! — blurring each primitive separately gives soft edges on sharp overlaps and
//! looks nothing like the thing anyone asked for. The same argument the
//! compositor already makes for group opacity, which is why the two travel in
//! the same command.
//!
//! So an [`ImageFilter`] rides on `PushLayer` beside `alpha` and `blend`,
//! rather than existing as a primitive of its own.
//!
//! # What "the CPU implementation is the reference" means here
//!
//! Both filters are defined by the pixel operation below, and the CPU backend
//! performs exactly that operation on the layer's rasterised contents. A GPU
//! backend is expected to match it, not to reinterpret it — see
//! `SceneReport::skipped_filters` for what happens on a backend that cannot yet
//! do either.

use crate::Color;

/// A 5×4 colour matrix, in row-major order.
///
/// The same layout as SVG's `feColorMatrix` and CSS's filter functions: four
/// rows of five, where each row produces one output channel from
/// `[r, g, b, a, 1]`. Twenty floats rather than a struct per effect, because
/// every one of the CSS filter functions *is* one of these and a chain of them
/// multiplies into one.
pub type ColorMatrix = [f32; 20];

/// What a layer's pixels go through before they are composited.
///
/// The two effect fields are optional and independent, so one layer can blur
/// *and* desaturate — which is the frosted-glass case, and the reason this is
/// a struct rather than an enum of one effect. [`Self::backdrop`] is a third,
/// orthogonal axis: not an effect of its own, but *which pixels* the two
/// effects above run on — see its own doc for what that means and why it is
/// a `bool` here rather than a fork into a second type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageFilter {
    /// Gaussian blur standard deviation in logical pixels.
    ///
    /// `0.0` is no blur. Per the W3C Filter Effects specification this is σ,
    /// not the visible radius — the blur reaches roughly `3σ`, so
    /// `backdrop-filter: blur(24px)` in CSS terms is about `sigma: 8.0` here.
    pub blur_sigma: f32,
    /// The direction a directional blur smears along, in radians — `None`
    /// for the default isotropic Gaussian (todo-upgrades U-14).
    ///
    /// A plain blur has no angle; a **motion** blur does. `Some(angle)` makes
    /// `blur_sigma` the extent *along that ray* rather than in every
    /// direction: the smear of a fast object, rather than the softness of a
    /// frosted one. `0.0` is horizontal, and the angle is clockwise in the
    /// Y-down space everything else here uses.
    ///
    /// This is a taste decision the film has not yet committed to — at 60 fps
    /// fast motion reads acceptably sharp — but the capability is here so the
    /// decision is not foreclosed.
    pub blur_angle: Option<f32>,
    /// A colour transform applied after the blur.
    ///
    /// After rather than before, because blurring already-desaturated pixels
    /// and desaturating already-blurred ones give the same answer for a linear
    /// matrix, and this order matches SVG's default primitive chaining.
    pub color_matrix: Option<ColorMatrix>,
    /// When `true`, this layer's starting content is not a blank buffer but a
    /// **copy of whatever is already painted beneath it** at this position in
    /// paint order — true backdrop sampling, CSS's `backdrop-filter`, as
    /// distinct from the plain case (this field `false`) which filters only
    /// the group's own subsequently-painted content.
    ///
    /// `blur_sigma`/`color_matrix` still describe what happens to that
    /// sampled backdrop, applied **once, before** the group's own content
    /// (a tint, a border, children) paints on top of it — not to the
    /// combined result. That ordering is what keeps foreground content sharp
    /// over a blurred background rather than blurring the foreground along
    /// with it, which is the entire point of a backdrop filter and the part
    /// a naive "filter the finished layer" implementation gets wrong. See
    /// `vieww-paint::native::reference`'s `Command::PushLayer` handling for
    /// exactly where that ordering is enforced.
    pub backdrop: bool,
}

impl ImageFilter {
    /// No filtering at all. What a plain group opacity layer carries.
    pub const NONE: Self = Self {
        blur_sigma: 0.0,
        blur_angle: None,
        color_matrix: None,
        backdrop: false,
    };

    /// A Gaussian blur of standard deviation `sigma`.
    #[must_use]
    pub const fn blur(sigma: f32) -> Self {
        Self {
            blur_sigma: sigma,
            blur_angle: None,
            color_matrix: None,
            backdrop: false,
        }
    }

    /// A directional (motion) blur: standard deviation `sigma` **along
    /// `angle`** rather than in every direction (todo-upgrades U-14).
    ///
    /// The smear of a shard travelling several hundred pixels a frame — the
    /// one thing that separates "photograph" from "render" in a shatter.
    /// `angle` is radians, `0.0` horizontal, clockwise in Y-down screen space;
    /// pass the direction of travel.
    #[must_use]
    pub const fn directional_blur(sigma: f32, angle: f32) -> Self {
        Self {
            blur_sigma: sigma,
            blur_angle: Some(angle),
            color_matrix: None,
            backdrop: false,
        }
    }

    /// A colour transform with no blur.
    #[must_use]
    pub const fn color(matrix: ColorMatrix) -> Self {
        Self {
            blur_sigma: 0.0,
            blur_angle: None,
            color_matrix: Some(matrix),
            backdrop: false,
        }
    }

    /// A real backdrop blur of standard deviation `sigma` — CSS's
    /// `backdrop-filter: blur(...)`. See [`Self::backdrop`] for exactly what
    /// distinguishes this from [`Self::blur`].
    #[must_use]
    pub const fn backdrop_blur(sigma: f32) -> Self {
        Self {
            blur_sigma: sigma,
            blur_angle: None,
            color_matrix: None,
            backdrop: true,
        }
    }

    /// Add a blur to an existing filter.
    #[must_use]
    pub const fn with_blur(mut self, sigma: f32) -> Self {
        self.blur_sigma = sigma;
        self
    }

    /// Point an existing blur along `angle` — see
    /// [`directional_blur`](Self::directional_blur).
    #[must_use]
    pub const fn with_blur_angle(mut self, angle: f32) -> Self {
        self.blur_angle = Some(angle);
        self
    }

    /// Add a colour transform to an existing filter.
    #[must_use]
    pub const fn with_color(mut self, matrix: ColorMatrix) -> Self {
        self.color_matrix = Some(matrix);
        self
    }

    /// Turn an existing filter into a real backdrop filter — see
    /// [`Self::backdrop`].
    #[must_use]
    pub const fn with_backdrop(mut self) -> Self {
        self.backdrop = true;
        self
    }

    /// `true` when this filter would change nothing.
    ///
    /// Checked by the backends before doing any offscreen work: a layer that
    /// carries `NONE` must cost exactly what a layer always cost, or every
    /// `Opacity` in the tree gets slower the day filters are added.
    #[must_use]
    pub fn is_noop(&self) -> bool {
        self.blur_sigma <= 0.0 && self.color_matrix.is_none()
    }

    /// How far beyond the layer's bounds this filter reaches, in pixels.
    ///
    /// A blur samples outward, so the offscreen buffer and the damage rect both
    /// have to be grown by this much or the edges of a blurred panel are cut
    /// off square — the classic "my blur has a hard edge" bug.
    ///
    /// `3σ` is where a Gaussian has fallen below 1% and is the same cut-off
    /// every other implementation uses. A *directional* blur reaches `3σ`
    /// along its ray only, so the honest expansion would be
    /// `3σ·|cos| × 3σ·|sin|` per axis — this returns the uniform `3σ` instead,
    /// which is a superset, because the caller is a scalar inflation and the
    /// alternative (per-axis damage for one filter) buys almost nothing.
    #[must_use]
    pub fn bounds_expansion(&self) -> f32 {
        if self.blur_sigma <= 0.0 {
            0.0
        } else {
            (self.blur_sigma * 3.0).ceil()
        }
    }
}

impl Default for ImageFilter {
    fn default() -> Self {
        Self::NONE
    }
}

/// The identity matrix — every channel passes through unchanged.
#[must_use]
pub const fn identity_matrix() -> ColorMatrix {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 0.0, 1.0, 0.0,
    ]
}

/// Desaturate. `0.0` is fully grey, `1.0` unchanged.
///
/// The luminance coefficients are Rec. 709's, which is what CSS, SVG and every
/// display stack in use agree on. The naive `(r + g + b) / 3` is visibly wrong:
/// it makes greens too dark and blues too light.
#[must_use]
pub fn saturation_matrix(amount: f32) -> ColorMatrix {
    const LR: f32 = 0.213;
    const LG: f32 = 0.715;
    const LB: f32 = 0.072;
    let inv = 1.0 - amount;
    let (sr, sg, sb) = (inv * LR, inv * LG, inv * LB);

    [
        sr + amount,
        sg,
        sb,
        0.0,
        0.0, //
        sr,
        sg + amount,
        sb,
        0.0,
        0.0, //
        sr,
        sg,
        sb + amount,
        0.0,
        0.0, //
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
    ]
}

/// Fully desaturate.
#[must_use]
pub fn grayscale_matrix() -> ColorMatrix {
    saturation_matrix(0.0)
}

/// Scale every colour channel. `1.0` is unchanged, `0.0` is black.
#[must_use]
pub fn brightness_matrix(amount: f32) -> ColorMatrix {
    [
        amount, 0.0, 0.0, 0.0, 0.0, //
        0.0, amount, 0.0, 0.0, 0.0, //
        0.0, 0.0, amount, 0.0, 0.0, //
        0.0, 0.0, 0.0, 1.0, 0.0,
    ]
}

/// The sepia tone, with CSS's coefficients.
#[must_use]
pub const fn sepia_matrix() -> ColorMatrix {
    [
        0.393, 0.769, 0.189, 0.0, 0.0, //
        0.349, 0.686, 0.168, 0.0, 0.0, //
        0.272, 0.534, 0.131, 0.0, 0.0, //
        0.0, 0.0, 0.0, 1.0, 0.0,
    ]
}

/// Tint towards `color` by `amount`, keeping alpha.
///
/// What a "frosted glass" panel wants on top of its blur: a wash that takes the
/// blurred content towards white or towards the surface colour, without the
/// flat overlay that would hide the blur underneath it.
#[must_use]
pub fn tint_matrix(color: Color, amount: f32) -> ColorMatrix {
    let keep = 1.0 - amount;
    let scale = |channel: u8| f32::from(channel) / 255.0 * amount;

    [
        keep,
        0.0,
        0.0,
        0.0,
        scale(color.r), //
        0.0,
        keep,
        0.0,
        0.0,
        scale(color.g), //
        0.0,
        0.0,
        keep,
        0.0,
        scale(color.b), //
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
    ]
}

/// Multiply two colour matrices, so a chain of filters costs one pass.
///
/// `second * first` — the result applies `first` and then `second`, which is
/// the order a reader of `.filter(a).filter(b)` expects.
#[must_use]
pub fn compose_matrices(first: &ColorMatrix, second: &ColorMatrix) -> ColorMatrix {
    let mut out = [0.0f32; 20];
    for row in 0..4 {
        for col in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += second[row * 5 + k] * first[k * 5 + col];
            }
            out[row * 5 + col] = sum;
        }
        // The translation column also picks up `second`'s own translation.
        let mut offset = second[row * 5 + 4];
        for k in 0..4 {
            offset += second[row * 5 + k] * first[k * 5 + 4];
        }
        out[row * 5 + 4] = offset;
    }
    out
}

/// Apply a colour matrix in place to tightly packed **premultiplied** RGBA8.
///
/// # Why premultiplied needs un-premultiplying first
///
/// A colour matrix is defined over straight (un-premultiplied) colour. Applying
/// one directly to premultiplied pixels scales the colour by alpha twice and
/// darkens every edge — which shows up as a grey halo around anti-aliased text
/// and is exactly the sort of bug that survives review because it looks like
/// the font is thin.
///
/// So each pixel is divided by its alpha, transformed, and multiplied back.
/// Fully transparent pixels are skipped: their colour carries no information
/// and dividing by zero would invent some.
pub fn apply_color_matrix_premultiplied(pixels: &mut [u8], matrix: &ColorMatrix) {
    for pixel in pixels.chunks_exact_mut(4) {
        let a = f32::from(pixel[3]) / 255.0;
        if a <= 0.0 {
            continue;
        }

        let r = f32::from(pixel[0]) / 255.0 / a;
        let g = f32::from(pixel[1]) / 255.0 / a;
        let b = f32::from(pixel[2]) / 255.0 / a;

        let out_r = matrix[0] * r + matrix[1] * g + matrix[2] * b + matrix[3] * a + matrix[4];
        let out_g = matrix[5] * r + matrix[6] * g + matrix[7] * b + matrix[8] * a + matrix[9];
        let out_b = matrix[10] * r + matrix[11] * g + matrix[12] * b + matrix[13] * a + matrix[14];
        let out_a = matrix[15] * r + matrix[16] * g + matrix[17] * b + matrix[18] * a + matrix[19];

        let out_a = out_a.clamp(0.0, 1.0);
        let encode = |value: f32| (value.clamp(0.0, 1.0) * out_a * 255.0).round() as u8;

        pixel[0] = encode(out_r);
        pixel[1] = encode(out_g);
        pixel[2] = encode(out_b);
        pixel[3] = (out_a * 255.0).round() as u8;
    }
}

/// Gaussian-blur tightly packed RGBA8 in place.
///
/// # Three box blurs rather than a Gaussian kernel
///
/// A true Gaussian is O(n·r) per axis. Three successive box blurs approximate
/// one to within about 3% — well below what an eye resolves on a blurred
/// backdrop — and each box blur is O(n) with a sliding sum, independent of
/// radius. That is the difference between a blur that costs the same at σ=2 and
/// σ=40 and one that does not, which matters because the σ a designer wants for
/// frosted glass is large.
///
/// The box widths come from the standard derivation (Kovesi), which picks the
/// integer widths whose combined variance is closest to the requested σ.
///
/// Operates on premultiplied pixels directly, which is correct: a weighted sum
/// of premultiplied colours *is* the premultiplied weighted sum, so unlike the
/// colour matrix this needs no round trip.
pub fn blur_rgba(pixels: &mut [u8], width: usize, height: usize, sigma: f32) {
    if sigma <= 0.0 || width == 0 || height == 0 {
        return;
    }

    let mut scratch = vec![0u8; pixels.len()];
    for radius in box_radii_for_gaussian(sigma) {
        if radius == 0 {
            continue;
        }
        box_blur_horizontal(pixels, &mut scratch, width, height, radius);
        box_blur_vertical(&scratch, pixels, width, height, radius);
    }
}

/// A directional (motion) blur — `sigma` is the extent **along the ray** at
/// `angle`, not in every direction (todo-upgrades U-14).
///
/// The isotropic [`blur_rgba`] approximates a Gaussian with three box passes
/// run as a separable horizontal/vertical pair. A direction breaks that
/// separability, so each box pass here runs **along the ray**: the same three
/// radii, each a sliding window over samples offset by `t · (cos θ, sin θ)`
/// with the edge-clamped sampling U-15 fixed — a motion blur that darkened
/// at its own frame edges would be a worse bug than the softness it replaces.
///
/// Samples are rounded to the nearest pixel rather than interpolated: a
/// bilinear tap would make each pass a two-pixel-wide smear and halve the
/// effective length. Cost is `O(w · h · span)` per pass where the isotropic
/// blur's sliding sums are `O(w · h)` — the price of the option, paid only
/// when an angle is asked for.
///
/// `0.0` radians smears horizontally; the angle is clockwise in the Y-down
/// space the rest of this crate uses, matching `ImageFilter::blur_angle`.
pub fn directional_blur_rgba(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    sigma: f32,
    angle: f32,
) {
    if sigma <= 0.0 || width == 0 || height == 0 {
        return;
    }

    // Keep the ray pointing right so an angle and its opposite produce the
    // same smear — a motion blur is not a directed gradient.
    let (mut dx, mut dy) = (angle.cos(), angle.sin());
    if dx < 0.0 {
        dx = -dx;
        dy = -dy;
    }

    let mut scratch = vec![0u8; pixels.len()];
    for radius in box_radii_for_gaussian(sigma) {
        if radius == 0 {
            continue;
        }
        box_blur_directed(pixels, &mut scratch, width, height, radius, dx, dy);
        box_blur_directed(&scratch, pixels, width, height, radius, dx, dy);
    }
}

/// One box pass along `(dx, dy)`, samples rounded to whole pixels and clamped
/// at the buffer's edges (see [`directional_blur_rgba`]).
fn box_blur_directed(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    height: usize,
    radius: usize,
    dx: f32,
    dy: f32,
) {
    let span = radius as u32 * 2 + 1;
    let max_x = (width - 1) as f32;
    let max_y = (height - 1) as f32;
    for y in 0..height {
        for x in 0..width {
            for channel in 0..4 {
                let mut sum: u32 = 0;
                for t in -(radius as i32)..=(radius as i32) {
                    let step = t as f32;
                    let sx = (x as f32 + step * dx).round().clamp(0.0, max_x) as usize;
                    let sy = (y as f32 + step * dy).round().clamp(0.0, max_y) as usize;
                    sum += u32::from(src[(sy * width + sx) * 4 + channel]);
                }
                dst[(y * width + x) * 4 + channel] = (sum / span) as u8;
            }
        }
    }
}

/// The three box-blur radii whose combined variance approximates `sigma`.
#[must_use]
pub fn box_radii_for_gaussian(sigma: f32) -> [usize; 3] {
    // Kovesi's derivation: the ideal averaging filter width for a given sigma
    // across n passes, split into the two nearest odd integers.
    let n = 3.0f32;
    let ideal = ((12.0 * sigma * sigma / n) + 1.0).sqrt();
    let mut lower = ideal.floor() as i32;
    if lower % 2 == 0 {
        lower -= 1;
    }
    let upper = lower + 2;
    let m = ((12.0 * sigma * sigma
        - (n * lower as f32 * lower as f32)
        - (4.0 * n * lower as f32)
        - (3.0 * n))
        / (-4.0 * lower as f32 - 4.0))
        .round();

    let mut radii = [0usize; 3];
    for (index, radius) in radii.iter_mut().enumerate() {
        let width = if (index as f32) < m { lower } else { upper };
        *radius = ((width - 1) / 2).max(0) as usize;
    }
    radii
}

/// One horizontal box blur pass, with a sliding sum so cost is radius-independent.
fn box_blur_horizontal(src: &[u8], dst: &mut [u8], width: usize, height: usize, radius: usize) {
    let span = radius * 2 + 1;
    for y in 0..height {
        let row = y * width * 4;
        for channel in 0..4 {
            let at = |x: usize| u32::from(src[row + x * 4 + channel]);
            // Seed the window, clamping at the edge rather than wrapping: a
            // wrapped blur bleeds the right edge onto the left.
            let mut sum: u32 = at(0) * (radius as u32 + 1);
            for x in 1..=radius.min(width - 1) {
                sum += at(x);
            }
            if width <= radius {
                sum += at(width - 1) * (radius as u32 + 1 - width as u32);
            }

            for x in 0..width {
                dst[row + x * 4 + channel] = (sum / span as u32) as u8;
                let leaving = at(x.saturating_sub(radius));
                let entering = at((x + radius + 1).min(width - 1));
                sum = sum + entering - leaving;
            }
        }
    }
}

/// One vertical box blur pass.
fn box_blur_vertical(src: &[u8], dst: &mut [u8], width: usize, height: usize, radius: usize) {
    let span = radius * 2 + 1;
    for x in 0..width {
        for channel in 0..4 {
            let at = |y: usize| u32::from(src[(y * width + x) * 4 + channel]);
            let mut sum: u32 = at(0) * (radius as u32 + 1);
            for y in 1..=radius.min(height - 1) {
                sum += at(y);
            }
            if height <= radius {
                sum += at(height - 1) * (radius as u32 + 1 - height as u32);
            }

            for y in 0..height {
                dst[(y * width + x) * 4 + channel] = (sum / span as u32) as u8;
                let leaving = at(y.saturating_sub(radius));
                let entering = at((y + radius + 1).min(height - 1));
                sum = sum + entering - leaving;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_noop_filter_is_recognised_as_one() {
        assert!(ImageFilter::NONE.is_noop());
        assert!(ImageFilter::blur(0.0).is_noop());
        assert!(!ImageFilter::blur(4.0).is_noop());
        assert!(!ImageFilter::color(grayscale_matrix()).is_noop());
    }

    #[test]
    fn a_blur_reaches_three_sigma_and_no_filter_reaches_nothing() {
        assert_eq!(ImageFilter::NONE.bounds_expansion(), 0.0);
        assert_eq!(ImageFilter::blur(8.0).bounds_expansion(), 24.0);
    }

    #[test]
    fn the_identity_matrix_leaves_pixels_alone() {
        let mut pixels = vec![10, 120, 250, 255, 0, 0, 0, 0];
        let before = pixels.clone();
        apply_color_matrix_premultiplied(&mut pixels, &identity_matrix());
        for (a, b) in before.iter().zip(&pixels) {
            assert!(a.abs_diff(*b) <= 1, "{before:?} became {pixels:?}");
        }
    }

    /// The bug the un-premultiply exists for. A half-transparent white pixel is
    /// stored as `(128,128,128,128)`; a matrix applied to those numbers
    /// directly treats it as mid-grey and darkens it.
    #[test]
    fn a_matrix_on_premultiplied_pixels_does_not_darken_translucent_ones() {
        // Half-alpha white, premultiplied.
        let mut pixels = vec![128, 128, 128, 128];
        apply_color_matrix_premultiplied(&mut pixels, &identity_matrix());
        assert_eq!(pixels[3], 128, "alpha must survive");
        assert!(
            pixels[0].abs_diff(128) <= 1,
            "a half-transparent white must stay half-transparent white, not {}",
            pixels[0]
        );
    }

    #[test]
    fn grayscale_makes_every_channel_equal() {
        let mut pixels = vec![200, 40, 90, 255];
        apply_color_matrix_premultiplied(&mut pixels, &grayscale_matrix());
        assert!(
            pixels[0].abs_diff(pixels[1]) <= 1 && pixels[1].abs_diff(pixels[2]) <= 1,
            "not grey: {pixels:?}"
        );
        // And it must use luminance weights rather than a flat mean — the mean
        // of (200,40,90) is 110, the Rec.709 luminance is about 78.
        assert!(
            pixels[0] < 100,
            "flat-mean grey rather than luminance: {pixels:?}"
        );
    }

    #[test]
    fn composing_two_matrices_matches_applying_them_in_turn() {
        let a = brightness_matrix(0.5);
        let b = saturation_matrix(0.25);

        let mut stepwise = vec![210, 90, 40, 255];
        apply_color_matrix_premultiplied(&mut stepwise, &a);
        apply_color_matrix_premultiplied(&mut stepwise, &b);

        let mut composed = vec![210, 90, 40, 255];
        apply_color_matrix_premultiplied(&mut composed, &compose_matrices(&a, &b));

        for (x, y) in stepwise.iter().zip(&composed) {
            assert!(x.abs_diff(*y) <= 2, "{stepwise:?} vs {composed:?}");
        }
    }

    #[test]
    fn a_blur_spreads_a_single_bright_pixel_into_its_neighbours() {
        const W: usize = 17;
        let mut pixels = vec![0u8; W * W * 4];
        let centre = ((W / 2) * W + W / 2) * 4;
        pixels[centre..centre + 4].copy_from_slice(&[255, 255, 255, 255]);

        blur_rgba(&mut pixels, W, W, 2.0);

        let at = |x: usize, y: usize| pixels[(y * W + x) * 4];
        assert!(at(W / 2, W / 2) < 255, "the centre must have spread out");
        assert!(at(W / 2 + 1, W / 2) > 0, "the neighbour must have lit up");
        assert!(
            at(W / 2, W / 2) > at(W / 2 + 2, W / 2),
            "a blur must still fall off with distance"
        );
    }

    /// Total light is conserved: a blur redistributes energy, it does not
    /// create or destroy it. Off by more than rounding and the image visibly
    /// darkens or blooms every time the filter runs.
    #[test]
    fn a_blur_conserves_roughly_the_total_it_started_with() {
        const W: usize = 33;
        let mut pixels = vec![0u8; W * W * 4];
        for y in 12..21 {
            for x in 12..21 {
                let at = (y * W + x) * 4;
                pixels[at..at + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        let before: u64 = pixels.iter().map(|v| u64::from(*v)).sum();

        blur_rgba(&mut pixels, W, W, 3.0);

        let after: u64 = pixels.iter().map(|v| u64::from(*v)).sum();
        let drift = (before as f64 - after as f64).abs() / before as f64;
        assert!(drift < 0.05, "{before} became {after} — {drift:.3} drift");
    }

    #[test]
    fn a_zero_sigma_blur_changes_nothing() {
        let mut pixels = vec![1, 2, 3, 255, 4, 5, 6, 255];
        let before = pixels.clone();
        blur_rgba(&mut pixels, 2, 1, 0.0);
        assert_eq!(pixels, before);
    }

    /// The edge case a sliding-window blur gets wrong first: a buffer narrower
    /// than the radius. It must clamp rather than index out of bounds.
    #[test]
    fn a_buffer_smaller_than_the_radius_does_not_panic() {
        let mut pixels = vec![255u8; 2 * 2 * 4];
        blur_rgba(&mut pixels, 2, 2, 12.0);
    }

    /// todo-upgrades U-15, the entry that named this defect "highest severity
    /// in the file": the blur used to read outside its buffer as transparent
    /// black, so a full-bleed uniform field came back with edges at 100/255 and
    /// corners at 39/255 — every frosted bar darkened along its own rim, and
    /// the application could not fix it because it never sees the intermediate
    /// buffer. Edge-clamped sampling means a uniform field is a fixed point of
    /// the blur, exactly, at every sigma.
    #[test]
    fn a_blur_of_a_uniform_field_stays_uniform_to_the_corner() {
        const W: usize = 96;
        for sigma in [4.0, 8.0, 24.0] {
            let mut pixels = vec![255u8; W * W * 4];
            blur_rgba(&mut pixels, W, W, sigma);
            let corner = &pixels[0..4];
            let edge = &pixels[(W / 2 * W) * 4..(W / 2 * W) * 4 + 4];
            let centre = &pixels[((W / 2) * W + W / 2) * 4..((W / 2) * W + W / 2) * 4 + 4];
            assert_eq!(centre, &[255, 255, 255, 255], "sigma {sigma}: centre");
            assert_eq!(edge, &[255, 255, 255, 255], "sigma {sigma}: edge");
            assert_eq!(corner, &[255, 255, 255, 255], "sigma {sigma}: corner");
        }
    }

    /// The directional variant inherits the same property (todo-upgrades U-14
    /// + U-15): a motion blur of a full-bleed field is still the field.
    #[test]
    fn a_directional_blur_of_a_uniform_field_stays_uniform_to_the_corner() {
        const W: usize = 64;
        let mut pixels = vec![255u8; W * W * 4];
        directional_blur_rgba(&mut pixels, W, W, 8.0, 0.6);
        assert!(pixels.iter().all(|&v| v == 255), "a corner or edge darkened");
    }

    /// A motion blur smears **along** its ray and only along it: a single
    /// bright pixel at `angle = 0` lights its horizontal neighbours and leaves
    /// the vertical ones dark. This is the property that separates it from an
    /// isotropic blur, and the one a shatter plate's shards depend on.
    #[test]
    fn a_directional_blur_spreads_along_its_ray_and_not_across_it() {
        const W: usize = 33;
        let mut pixels = vec![0u8; W * W * 4];
        let centre = ((W / 2) * W + W / 2) * 4;
        pixels[centre..centre + 4].copy_from_slice(&[255, 255, 255, 255]);

        directional_blur_rgba(&mut pixels, W, W, 3.0, 0.0);

        let at = |x: usize, y: usize| pixels[(y * W + x) * 4];
        assert!(at(W / 2 + 3, W / 2) > 0, "along the ray must light up");
        assert!(at(W / 2, W / 2 + 3) == 0, "across the ray must stay dark");
    }
}
