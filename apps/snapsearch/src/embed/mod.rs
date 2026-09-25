//! The embedding layer — what actually makes "find any photo using a photo
//! or text as reference" work.
//!
//! # The shape of it
//!
//! Search is a **joint embedding space**: images and text are both encoded
//! into vectors of the same dimension, so "how well does this text describe
//! this photo?" becomes cosine similarity between two vectors, and finding
//! matches becomes a nearest-neighbour lookup. That structure — [`Embedder`]
//! + [`EmbeddingIndex`] — is identical whichever encoder is behind it, which
//! is the whole point of putting a trait here.
//!
//! # Two encoders, and the honest difference between them
//!
//! | | [`local::LocalEmbedder`] (default) | [`clip::ClipEmbedder`] (`--features clip`) |
//! |---|---|---|
//! | what it encodes | **appearance** — colour, brightness, spatial layout | **meaning** — learned image/text semantics |
//! | "sunset over water" finds | warm-topped, blue-bottomed photos | photos that actually depict a sunset over water |
//! | "my dog on the porch" finds | little of use | photos that actually contain a dog |
//! | needs | nothing | ~600MB of CLIP ViT-B/32 weights on disk |
//! | runs | instantly, on device, offline | seconds per image on a phone CPU |
//!
//! `LocalEmbedder` is a real embedder doing real vector retrieval — it is
//! not a hash and not a placeholder scorer — but it understands *pictures*,
//! not *subjects*. Only CLIP does semantics. Both are wired end to end;
//! which one you get is a build flag, and the app labels which is live so
//! the difference is never invisible to the user.

pub mod local;

#[cfg(feature = "clip")]
pub mod clip;

use std::sync::Arc;

use crate::photo::Image_;

/// An encoder that puts images and text into one shared vector space.
///
/// Implementations must return vectors that are already **L2-normalised**,
/// so [`EmbeddingIndex`] can treat a dot product as a cosine similarity.
/// `Send + Sync` because embedding runs on a worker thread — see
/// [`crate::library::Loader`]. `Image_` is `Arc`-backed and crosses threads
/// happily; nothing here touches the widget tree.
pub trait Embedder: Send + Sync {
    /// How many dimensions this encoder's vectors have.
    fn dim(&self) -> usize;

    /// A short name for the backend, shown in the UI so it is always
    /// visible which encoder produced the results on screen.
    fn name(&self) -> &'static str;

    /// True when this encoder understands what a photo is *of*, rather than
    /// only what it looks like. Drives the UI's own honesty about results.
    fn is_semantic(&self) -> bool;

    fn embed_image(&self, image: &Image_) -> Vec<f32>;

    fn embed_text(&self, text: &str) -> Vec<f32>;

    /// A query as one or more vectors, scored by whichever matches best.
    ///
    /// Most encoders want exactly one: CLIP genuinely *composes* a phrase,
    /// so "sunset over water" is a single point meaning both things at once
    /// and averaging is the wrong picture of it.
    ///
    /// An encoder that cannot compose says so by returning several. The
    /// local one does: averaging a warm-orange vector with a deep-blue one
    /// lands on magenta, so "sunset over water" retrieved *pink flowers* —
    /// a blend of the two inputs that resembles neither. Scoring each
    /// concept separately and keeping the best turns that into "photos of
    /// either", which is a defensible reading of the query and a far more
    /// useful answer than the average of two colours.
    fn embed_query(&self, text: &str) -> Vec<Vec<f32>> {
        vec![self.embed_text(text)]
    }

    /// The cosine below which a result is not worth showing.
    ///
    /// Encoder-specific on purpose, because the two spaces are shaped
    /// completely differently and one shared constant cannot serve both.
    /// The local encoder's 32 colour features are highly correlated, so
    /// everything scores high and only the top of the range means anything
    /// — a low floor let a tan desert shot through as a "match" for a
    /// sunset. CLIP's 512 dimensions spread much wider, and an image/text
    /// pair that genuinely corresponds rarely passes ~0.35, so the same
    /// high floor there would reject every correct answer.
    fn min_similarity(&self) -> f32;
}

/// Scale a vector to unit length, in place.
///
/// A zero vector is left alone rather than producing `NaN`: it means "this
/// encoder found nothing to say about the input", which is a real outcome
/// for a text query of stopwords, and it must not poison every later
/// comparison.
pub fn normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > f32::EPSILON {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// Cosine similarity of two already-normalised vectors, in `-1.0..=1.0`.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>().clamp(-1.0, 1.0)
}

/// One photo's vector, alongside the id it belongs to.
#[derive(Debug, Clone)]
pub struct Entry {
    pub id: String,
    pub vector: Vec<f32>,
}

/// A brute-force nearest-neighbour index over normalised vectors.
///
/// Brute force is the right call at this size and is not a shortcut: a
/// personal library is thousands of photos, and 512 floats × a few thousand
/// is a few million multiply-adds — well under a frame. An approximate
/// index (HNSW/IVF) only starts paying for itself in the millions, and it
/// would trade exact results for a tuning problem this app does not have.
#[derive(Debug, Default, Clone)]
pub struct EmbeddingIndex {
    entries: Vec<Entry>,
}

impl EmbeddingIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Index every photo in `photos`, in order.
    ///
    /// Synchronous, and only used for the generated sample library, which is
    /// tens of tiny bitmaps. Real photos go through
    /// [`crate::library::Loader`] and arrive one at a time via
    /// [`Self::insert`] — a thousand JPEGs decoded and embedded on the frame
    /// thread is a minute of frozen application.
    pub fn build(embedder: &dyn Embedder, photos: &[crate::photo::Photo]) -> Self {
        let entries = photos
            .iter()
            .map(|photo| Entry {
                id: photo.id.clone(),
                vector: embedder.embed_image(&photo.image),
            })
            .collect();
        Self { entries }
    }

    /// Drop everything — a rescan replacing one library with another.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn insert(&mut self, id: impl Into<String>, vector: Vec<f32>) {
        self.entries.push(Entry {
            id: id.into(),
            vector,
        });
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The `k` closest entries to `query`, best first, as
    /// `(index_into_the_indexed_order, similarity)`.
    ///
    /// `min_score` drops weak matches entirely rather than padding the grid
    /// with near-random photos — "nothing here looks like that" is a more
    /// useful answer than ten arbitrary pictures, and it is what makes the
    /// empty state reachable.
    pub fn search(&self, query: &[f32], k: usize, min_score: f32) -> Vec<(usize, f32)> {
        self.search_any(std::slice::from_ref(&query.to_vec()), k, min_score)
    }

    /// The same, against several query vectors at once, scoring each entry
    /// by its **best** match among them. See [`Embedder::embed_query`].
    pub fn search_any(
        &self,
        queries: &[Vec<f32>],
        k: usize,
        min_score: f32,
    ) -> Vec<(usize, f32)> {
        let live: Vec<&Vec<f32>> = queries
            .iter()
            .filter(|q| q.iter().any(|x| x.abs() > f32::EPSILON))
            .collect();
        if live.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(usize, f32)> = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let best = live
                    .iter()
                    .map(|q| cosine(q, &entry.vector))
                    .fold(f32::NEG_INFINITY, f32::max);
                (i, best)
            })
            .filter(|(_, score)| *score >= min_score)
            .collect();
        // `total_cmp`, not `partial_cmp().unwrap()` — a NaN slipping in from
        // a degenerate vector would panic the sort rather than sink to the
        // bottom, and this runs on every keystroke-driven search.
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.truncate(k);
        scored
    }
}

/// The encoder this build actually uses, chosen at compile time.
///
/// With `--features clip` and a readable model directory this is real CLIP;
/// otherwise — and if the CLIP weights fail to load — it is the local
/// appearance encoder, which always works. The fallback is deliberate: an
/// app whose search silently does nothing because a model file moved is
/// worse than one that says which encoder it is running.
pub fn default_embedder() -> Arc<dyn Embedder> {
    #[cfg(feature = "clip")]
    {
        match clip::ClipEmbedder::from_env() {
            Ok(embedder) => return Arc::new(embedder),
            Err(err) => {
                eprintln!(
                    "snapsearch: CLIP weights unavailable ({err}); \
                     falling back to the local appearance encoder"
                );
            }
        }
    }
    Arc::new(local::LocalEmbedder::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalising_leaves_a_zero_vector_alone_rather_than_producing_nan() {
        let mut zero = vec![0.0; 8];
        normalize(&mut zero);
        assert!(zero.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn cosine_of_a_vector_with_itself_is_one() {
        let mut v = vec![0.3, -0.7, 0.1, 0.9];
        normalize(&mut v);
        assert!((cosine(&v, &v) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_index_ranks_the_nearest_vector_first() {
        let mut index = EmbeddingIndex::new();
        index.insert("far", vec![0.0, 1.0]);
        index.insert("near", vec![1.0, 0.0]);
        let hits = index.search(&[0.99, 0.14], 2, -1.0);
        assert_eq!(hits[0].0, 1, "the near vector wins");
    }

    #[test]
    fn a_min_score_drops_weak_matches_instead_of_padding_the_results() {
        let mut index = EmbeddingIndex::new();
        index.insert("orthogonal", vec![0.0, 1.0]);
        let hits = index.search(&[1.0, 0.0], 10, 0.2);
        assert!(hits.is_empty(), "nothing similar enough to return");
    }
}
