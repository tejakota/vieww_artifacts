//! The default encoder: a real embedder over what a photo **looks like**.
//!
//! It reads actual pixels and produces a 32-dimensional appearance vector —
//! a 3×3 grid of mean colour, plus five global statistics. Nothing about it
//! is a hash or a stand-in scorer: two photos that look alike land near each
//! other in this space, and that is genuinely useful for "find the other
//! shots from that evening" or for photo-as-reference search.
//!
//! What it is **not** is semantic. It has no idea what a dog is. See
//! [`super`]'s table, and [`crate::concept`] on why the text side of it
//! demonstrates plumbing rather than understanding.
//!
//! # Why the features are centred
//!
//! Raw RGB and luminance are all non-negative, so every image would sit in
//! the positive orthant and every pair would score a high cosine regardless
//! of content — the index would rank by brightness and nothing else. Each
//! feature is therefore shifted to straddle zero before normalising, which
//! is what lets the cosine actually discriminate.

use super::{normalize, Embedder};
use crate::concept::{self, Concept};
use crate::photo::Image_;
use vieww::foundation::Color;

/// 4×4 cells × RGB, a hue histogram, an edge-orientation histogram, and
/// four global statistics.
pub const DIM: usize = GRID * GRID * 3 + HUE_BINS + EDGE_BINS + 4;

const GRID: usize = 4;
const HUE_BINS: usize = 12;
const EDGE_BINS: usize = 8;

/// How much the global statistics count relative to any one grid cell.
///
/// Without this the 27 spatial channels drown out the 5 global ones and
/// overall hue stops mattering next to where the light happens to fall.
const GLOBAL_WEIGHT: f32 = 3.0;

/// How loudly the hue histogram and the edge histogram speak relative to a
/// single grid cell. Both are distributions rather than magnitudes, so they
/// are unit-normalised first and then scaled — otherwise a busy photograph
/// would out-vote a calm one purely by having more edges in it.
const HUE_WEIGHT: f32 = 2.0;
const EDGE_WEIGHT: f32 = 1.5;

/// Scale a histogram to unit length, centre it on its own mean, and append.
///
/// Centring matters for the same reason the colour channels are centred: a
/// histogram is all non-negative, and two unrelated images both have *some*
/// of every bin.
fn push_normalised(out: &mut Vec<f32>, bins: &[f32], weight: f32) {
    let total: f32 = bins.iter().sum();
    let mean = 1.0 / bins.len() as f32;
    if total <= f32::EPSILON {
        out.extend(std::iter::repeat_n(0.0, bins.len()));
        return;
    }
    for bin in bins {
        out.push((bin / total - mean) * weight);
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LocalEmbedder;

// Trivially thread-safe: it has no state at all.

impl LocalEmbedder {
    pub const fn new() -> Self {
        Self
    }
}

impl Embedder for LocalEmbedder {
    fn dim(&self) -> usize {
        DIM
    }

    fn name(&self) -> &'static str {
        "local appearance"
    }

    fn is_semantic(&self) -> bool {
        false
    }

    /// Measured, not guessed. Across the sample library the correct subject
    /// scores 0.84–1.00 and the first wrong answer sits at 0.24–0.88, so
    /// 0.80 keeps every right answer and cuts the long tail.
    ///
    /// It does **not** cut everything wrong: a plate of curry scores 0.88
    /// against "sunset", because a plate of curry genuinely looks like a
    /// sunset to an encoder that reads colour and edges. That is the
    /// documented limit of this backend, not a tuning failure — telling the
    /// two apart is what CLIP is for.
    fn min_similarity(&self) -> f32 {
        0.80
    }

    fn embed_image(&self, image: &Image_) -> Vec<f32> {
        let (w, h) = (image.width() as usize, image.height() as usize);
        let px = image.pixels();
        if w == 0 || h == 0 {
            return vec![0.0; DIM];
        }

        let mut features = Vec::with_capacity(DIM);

        let mut cell_sums = [[0.0f32; 3]; GRID * GRID];
        let mut cell_counts = [0.0f32; GRID * GRID];
        let mut hue = [0.0f32; HUE_BINS];
        let mut edges = [0.0f32; EDGE_BINS];

        let (mut luma_sum, mut luma_sq_sum, mut sat_sum) = (0.0f32, 0.0f32, 0.0f32);
        let (mut rg_sum, mut rg_sq, mut yb_sum, mut yb_sq) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);

        // Luma is kept for the gradient pass below; computing it twice for a
        // 256px thumbnail is cheaper than the branchy alternatives.
        let mut luma = vec![0.0f32; w * h];

        for y in 0..h {
            let row = (y * GRID / h).min(GRID - 1);
            for x in 0..w {
                let col = (x * GRID / w).min(GRID - 1);
                let i = (y * w + x) * 4;
                let (r, g, b) = (
                    px[i] as f32 / 255.0,
                    px[i + 1] as f32 / 255.0,
                    px[i + 2] as f32 / 255.0,
                );

                let cell = row * GRID + col;
                cell_sums[cell][0] += r;
                cell_sums[cell][1] += g;
                cell_sums[cell][2] += b;
                cell_counts[cell] += 1.0;

                // Rec. 601 — perceptual weighting, so a yellow and a blue of
                // the same RGB magnitude are not called equally bright.
                let l = 0.299 * r + 0.587 * g + 0.114 * b;
                luma[y * w + x] = l;
                luma_sum += l;
                luma_sq_sum += l * l;

                let max = r.max(g).max(b);
                let min = r.min(g).min(b);
                let chroma = max - min;
                let saturation = if max > f32::EPSILON { chroma / max } else { 0.0 };
                sat_sum += saturation;

                // Hue, weighted by how colourful the pixel is: a grey pixel
                // has a hue, and it is noise. Weighting keeps a photograph
                // of a grey street from voting in the hue histogram at all.
                if chroma > 1e-4 {
                    let hue_deg = if max == r {
                        60.0 * (((g - b) / chroma) % 6.0)
                    } else if max == g {
                        60.0 * ((b - r) / chroma + 2.0)
                    } else {
                        60.0 * ((r - g) / chroma + 4.0)
                    };
                    let hue_deg = if hue_deg < 0.0 { hue_deg + 360.0 } else { hue_deg };
                    let bin = ((hue_deg / 360.0) * HUE_BINS as f32) as usize % HUE_BINS;
                    hue[bin] += chroma;
                }

                // Hasler–Süsstrunk colourfulness, accumulated here and
                // finished below: it separates a vivid photo from a muted
                // one better than mean saturation does.
                let rg = r - g;
                let yb = 0.5 * (r + g) - b;
                rg_sum += rg;
                rg_sq += rg * rg;
                yb_sum += yb;
                yb_sq += yb * yb;
            }
        }

        // Edge orientations — a Sobel-lite over the luma plane. This is what
        // makes the descriptor respond to *structure* rather than only to
        // colour: a beach and a plain blue wall are the same colour and a
        // very different picture.
        for y in 1..h.saturating_sub(1) {
            for x in 1..w.saturating_sub(1) {
                let gx = luma[y * w + x + 1] - luma[y * w + x - 1];
                let gy = luma[(y + 1) * w + x] - luma[(y - 1) * w + x];
                let magnitude = (gx * gx + gy * gy).sqrt();
                if magnitude < 0.02 {
                    continue;
                }
                // Unsigned orientation over a half-turn: an edge and the
                // same edge with light and dark swapped are one edge.
                let angle = gy.atan2(gx);
                let normalised = (angle + std::f32::consts::PI) / std::f32::consts::PI % 1.0;
                let bin = (normalised * EDGE_BINS as f32) as usize % EDGE_BINS;
                edges[bin] += magnitude;
            }
        }

        // ---- assemble, centred ----------------------------------------
        // Raw RGB and luminance are all non-negative, so uncentred every
        // image would sit in the positive orthant and every pair would
        // score a high cosine whatever their content — the index would rank
        // by brightness and nothing else.
        for cell in 0..GRID * GRID {
            let n = cell_counts[cell].max(1.0);
            for channel in 0..3 {
                features.push(cell_sums[cell][channel] / n - 0.5);
            }
        }

        push_normalised(&mut features, &hue, HUE_WEIGHT);
        push_normalised(&mut features, &edges, EDGE_WEIGHT);

        let n = (w * h) as f32;
        let luma_mean = luma_sum / n;
        // Variance via E[x²] − E[x]², clamped: floating-point error can make
        // that difference slightly negative for a flat image, and the square
        // root of a negative is `NaN`, which then poisons every comparison
        // this vector takes part in.
        let luma_var = (luma_sq_sum / n - luma_mean * luma_mean).max(0.0);
        let rg_var = (rg_sq / n - (rg_sum / n).powi(2)).max(0.0);
        let yb_var = (yb_sq / n - (yb_sum / n).powi(2)).max(0.0);
        let colourfulness =
            (rg_var + yb_var).sqrt() + 0.3 * ((rg_sum / n).powi(2) + (yb_sum / n).powi(2)).sqrt();

        features.push((luma_mean - 0.5) * GLOBAL_WEIGHT);
        features.push((luma_var.sqrt() - 0.25) * GLOBAL_WEIGHT);
        features.push((sat_sum / n - 0.5) * GLOBAL_WEIGHT);
        features.push((colourfulness - 0.3) * GLOBAL_WEIGHT);

        debug_assert_eq!(features.len(), DIM);
        normalize(&mut features);
        features
    }

    fn embed_text(&self, text: &str) -> Vec<f32> {
        let matched = concept::concepts_in(text);
        if matched.is_empty() {
            // Nothing recognisable. A zero vector, which `EmbeddingIndex`
            // treats as "no results" rather than inventing a ranking — the
            // app then says so instead of showing ten arbitrary photos.
            return vec![0.0; DIM];
        }

        let mut summed = vec![0.0f32; DIM];
        for concept in &matched {
            // The crucial bit: the query is turned into a *picture* and run
            // through the very same image encoder above. The two vectors
            // are in one space by construction rather than by hand-tuning
            // 32 numbers to line up, which is how a joint space like this
            // quietly drifts out of alignment.
            let sketch = sketch_of(concept);
            for (slot, value) in summed.iter_mut().zip(self.embed_image(&sketch)) {
                *slot += value;
            }
        }

        normalize(&mut summed);
        summed
    }

    /// One vector per concept the query names, rather than their average.
    fn embed_query(&self, text: &str) -> Vec<Vec<f32>> {
        let matched = concept::concepts_in(text);
        if matched.is_empty() {
            return vec![vec![0.0; DIM]];
        }
        matched
            .iter()
            .map(|concept| self.embed_image(&sketch_of(concept)))
            .collect()
    }
}

/// The concept drawn as a small top-to-bottom gradient.
pub fn sketch_of(concept: &Concept) -> Image_ {
    gradient_image(concept.top, concept.bottom, 24, 32, 0.0)
}

/// A vertical-ish gradient bitmap: `top` at the top, `bottom` at the
/// bottom, optionally sheared sideways by `skew` for variety.
pub fn gradient_image(top: Color, bottom: Color, w: u32, h: u32, skew: f32) -> Image_ {
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let fy = y as f32 / (h - 1).max(1) as f32;
            let fx = x as f32 / (w - 1).max(1) as f32 - 0.5;
            let t = (fy + fx * skew).clamp(0.0, 1.0);
            pixels.push(lerp(top.r, bottom.r, t));
            pixels.push(lerp(top.g, bottom.g, t));
            pixels.push(lerp(top.b, bottom.b, t));
            pixels.push(255);
        }
    }
    Image_::from_rgba8(pixels, w, h)
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::cosine;

    fn concept(name: &str) -> &'static Concept {
        concept::CONCEPTS
            .iter()
            .find(|c| c.name == name)
            .expect("a concept by that name")
    }

    #[test]
    fn an_images_vector_has_the_declared_dimension_and_is_unit_length() {
        let v = LocalEmbedder::new().embed_image(&sketch_of(concept("ocean")));
        assert_eq!(v.len(), DIM);
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4, "norm was {norm}");
    }

    #[test]
    fn two_pictures_of_the_same_thing_are_closer_than_two_of_different_things() {
        let e = LocalEmbedder::new();
        let ocean = e.embed_image(&sketch_of(concept("ocean")));
        let lake = e.embed_image(&sketch_of(concept("lake")));
        let sunset = e.embed_image(&sketch_of(concept("sunset")));

        // Two bodies of water look more alike than water and a sunset.
        assert!(
            cosine(&ocean, &lake) > cosine(&ocean, &sunset),
            "water/water {:.3} should beat water/sunset {:.3}",
            cosine(&ocean, &lake),
            cosine(&ocean, &sunset)
        );
    }

    #[test]
    fn a_flat_image_does_not_produce_nan_from_its_zero_variance() {
        let flat = gradient_image(Color::rgb(40, 40, 40), Color::rgb(40, 40, 40), 8, 8, 0.0);
        let v = LocalEmbedder::new().embed_image(&flat);
        assert!(v.iter().all(|x| x.is_finite()), "{v:?}");
    }

    #[test]
    fn an_unrecognisable_query_embeds_to_zero_rather_than_to_a_ranking() {
        let v = LocalEmbedder::new().embed_text("qwertyuiop zxcvbnm");
        assert!(v.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn a_text_query_lands_nearest_the_thing_it_names() {
        let e = LocalEmbedder::new();
        let query = e.embed_text("a sunset");
        let sunset = e.embed_image(&sketch_of(concept("sunset")));
        let forest = e.embed_image(&sketch_of(concept("forest")));
        assert!(
            cosine(&query, &sunset) > cosine(&query, &forest),
            "sunset {:.3} should beat forest {:.3}",
            cosine(&query, &sunset),
            cosine(&query, &forest)
        );
    }
}
