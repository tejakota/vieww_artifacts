//! The semantic encoder: OpenAI CLIP ViT-B/32, run locally through
//! [`candle`](candle_core).
//!
//! This is the backend that makes the app's actual premise work — finding a
//! photo by what it *depicts* rather than by what colour it is. Images and
//! text go through CLIP's two towers into the same 512-dimensional space,
//! which is precisely the joint space [`super::EmbeddingIndex`] wants.
//!
//! Everything runs on-device: no network call, no photo leaves the phone.
//! That is the whole reason for a local model rather than an embeddings API.
//!
//! # Getting the weights
//!
//! Not vendored — they are ~600MB. Fetch `openai/clip-vit-base-patch32`
//! from Hugging Face into one directory:
//!
//! ```console
//! # model.safetensors and tokenizer.json from
//! # https://huggingface.co/openai/clip-vit-base-patch32
//! export SNAPSEARCH_CLIP_DIR=/path/to/clip-vit-base-patch32
//! cargo run --features clip
//! ```
//!
//! On a phone the directory belongs in app storage, shipped as an asset or
//! downloaded once on first run.
//!
//! # Cost, honestly
//!
//! A ViT-B/32 forward pass is tens of milliseconds on a desktop CPU and
//! rather more on a phone. Indexing a real camera roll is therefore a
//! **background job**, not something to do on the frame thread — see the
//! note in `state.rs`. Encoding the *query* is one pass and is fine inline.

use std::path::{Path, PathBuf};

use candle_core::{DType, Device, Tensor};
use candle_nn::{VarBuilder, VarMap};
use tokenizers::models::bpe::BPE;
use candle_transformers::models::clip::{div_l2_norm, ClipConfig, ClipModel};
use tokenizers::Tokenizer;

use super::{normalize, Embedder};
use crate::photo::Image_;

/// CLIP ViT-B/32's projection dimension.
pub const DIM: usize = 512;

/// The side of the square CLIP expects.
const IMAGE_SIZE: u32 = 224;

/// CLIP's text context window.
const CONTEXT_LENGTH: usize = 77;

/// The channel statistics CLIP was trained against. These are not
/// ImageNet's, and using ImageNet's by mistake degrades similarity
/// silently — the model still returns plausible vectors, just worse ones.
const MEAN: [f32; 3] = [0.481_454_66, 0.457_827_5, 0.408_210_73];
const STD: [f32; 3] = [0.268_629_54, 0.261_302_58, 0.275_777_1];

/// Where the weights live, when not passed explicitly.
pub const MODEL_DIR_ENV: &str = "SNAPSEARCH_CLIP_DIR";

pub struct ClipEmbedder {
    model: ClipModel,
    tokenizer: Tokenizer,
    device: Device,
    pad_id: u32,
}

impl std::fmt::Debug for ClipEmbedder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClipEmbedder").field("dim", &DIM).finish()
    }
}

impl ClipEmbedder {
    /// Load from the directory named by `SNAPSEARCH_CLIP_DIR`.
    pub fn from_env() -> Result<Self, String> {
        let dir = std::env::var(MODEL_DIR_ENV)
            .map_err(|_| format!("{MODEL_DIR_ENV} is not set"))?;
        Self::load(PathBuf::from(dir))
    }

    /// Load a model directory.
    ///
    /// Expects `model.safetensors`, plus a tokenizer in either of the two
    /// layouts CLIP is distributed in — `tokenizer.json`, or the original
    /// `vocab.json` + `merges.txt` pair. Accepting both matters: the same
    /// model is published in both shapes, and refusing one of them looks
    /// like a broken download rather than an unsupported layout.
    pub fn load(dir: impl AsRef<Path>) -> Result<Self, String> {
        let dir = dir.as_ref();
        let weights = dir.join("model.safetensors");
        if !weights.exists() {
            return Err(format!("{} is missing", weights.display()));
        }

        // CPU deliberately: this has to behave the same on a phone as on a
        // laptop, and a CUDA path that only ever runs on one developer's
        // machine is how a model quietly stops being tested.
        let device = Device::Cpu;

        // SAFETY: memory-mapping trusts the file not to change underneath
        // us. These are model weights in app storage, written once.
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[weights], DType::F32, &device)
                .map_err(|e| format!("reading the weights: {e}"))?
        };

        let config = ClipConfig::vit_base_patch32();
        let model =
            ClipModel::new(vb, &config).map_err(|e| format!("building the model: {e}"))?;

        Self::assemble(model, load_tokenizer(dir)?, device)
    }

    /// The same architecture, **randomly initialised**.
    ///
    /// Exists so the whole pipeline — preprocessing, tokenisation, both
    /// towers, the projections, the L2 normalisation, and the shape and
    /// finiteness of what comes out — can be exercised without a 600MB
    /// download. It answers "is this wired correctly", never "is this a
    /// good match".
    ///
    /// Randomly initialised rather than zeroed: with zeros, every
    /// `LayerNorm` divides by a zero variance and the whole tower returns
    /// `NaN` — which is what the first version of this did, and it proves
    /// nothing except that zeros are not weights. `VarMap` lets each layer
    /// apply its own initialiser, so the towers behave numerically the way
    /// trained ones do.
    pub fn untrained(tokenizer: Tokenizer) -> Result<Self, String> {
        Self::untrained_with(tokenizer).map(|(embedder, _)| embedder)
    }

    /// The same, handing back the `VarMap` so its weights can be written to
    /// a real `model.safetensors`.
    ///
    /// That is what lets the *file-loading* path be tested: `untrained`
    /// alone bypasses `from_mmaped_safetensors`, so it proves the towers
    /// work and proves nothing about whether this app can read a model off
    /// disk. Saving these weights and loading them back exercises exactly
    /// the code a real download goes through — the tensor names, the
    /// shapes, the dtype and the memory map — with only the values
    /// differing.
    pub fn untrained_with(tokenizer: Tokenizer) -> Result<(Self, VarMap), String> {
        let device = Device::Cpu;
        let varmap = VarMap::new();
        let vb = VarBuilder::from_varmap(&varmap, DType::F32, &device);
        let model = ClipModel::new(vb, &ClipConfig::vit_base_patch32())
            .map_err(|e| format!("building the model: {e}"))?;
        Ok((Self::assemble(model, tokenizer, device)?, varmap))
    }

    fn assemble(model: ClipModel, tokenizer: Tokenizer, device: Device) -> Result<Self, String> {
        let pad_id = tokenizer.token_to_id("<|endoftext|>").unwrap_or(0);
        Ok(Self {
            model,
            tokenizer,
            device,
            pad_id,
        })
    }

    /// RGBA8 → the `[1, 3, 224, 224]` normalised tensor CLIP expects.
    fn pixel_values(&self, image: &Image_) -> Result<Tensor, String> {
        let (w, h) = (image.width(), image.height());
        if w == 0 || h == 0 {
            return Err("an image with no pixels".into());
        }

        // Drop alpha, then resize with a real filter — nearest-neighbour
        // here visibly changes the embedding of anything with fine detail.
        let rgb: Vec<u8> = image
            .pixels()
            .chunks_exact(4)
            .flat_map(|px| [px[0], px[1], px[2]])
            .collect();
        let buffer = image::RgbImage::from_raw(w, h, rgb)
            .ok_or_else(|| "the pixel buffer did not match its dimensions".to_string())?;
        let resized = image::imageops::resize(
            &buffer,
            IMAGE_SIZE,
            IMAGE_SIZE,
            image::imageops::FilterType::CatmullRom,
        );

        // CHW, not HWC — the layout the conv stem expects.
        let mut data = vec![0f32; 3 * (IMAGE_SIZE * IMAGE_SIZE) as usize];
        let plane = (IMAGE_SIZE * IMAGE_SIZE) as usize;
        for (i, px) in resized.pixels().enumerate() {
            for c in 0..3 {
                data[c * plane + i] = (px[c] as f32 / 255.0 - MEAN[c]) / STD[c];
            }
        }

        Tensor::from_vec(data, (1, 3, IMAGE_SIZE as usize, IMAGE_SIZE as usize), &self.device)
            .map_err(|e| format!("building the image tensor: {e}"))
    }

    fn token_ids(&self, text: &str) -> Result<Tensor, String> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| format!("tokenising: {e}"))?;
        let mut ids = encoding.get_ids().to_vec();
        ids.truncate(CONTEXT_LENGTH);
        ids.resize(CONTEXT_LENGTH, self.pad_id);

        Tensor::new(ids.as_slice(), &self.device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| format!("building the token tensor: {e}"))
    }

    fn to_unit_vec(tensor: &Tensor) -> Result<Vec<f32>, String> {
        let normalised =
            div_l2_norm(tensor).map_err(|e| format!("normalising: {e}"))?;
        let mut v: Vec<f32> = normalised
            .flatten_all()
            .and_then(|t| t.to_vec1())
            .map_err(|e| format!("reading the vector: {e}"))?;
        // Belt and braces: `div_l2_norm` already did this, but the index's
        // contract is unit vectors and a silent violation there shows up as
        // subtly wrong ranking rather than as an error.
        normalize(&mut v);
        Ok(v)
    }
}

/// Read whichever tokenizer layout the directory has.
fn load_tokenizer(dir: &Path) -> Result<Tokenizer, String> {
    let combined = dir.join("tokenizer.json");
    if combined.exists() {
        return Tokenizer::from_file(&combined)
            .map_err(|e| format!("reading {}: {e}", combined.display()));
    }

    let vocab = dir.join("vocab.json");
    let merges = dir.join("merges.txt");
    if vocab.exists() && merges.exists() {
        let bpe = BPE::from_file(
            vocab.to_str().ok_or("vocab.json path is not UTF-8")?,
            merges.to_str().ok_or("merges.txt path is not UTF-8")?,
        )
        .end_of_word_suffix("</w>".to_string())
        .build()
        .map_err(|e| format!("building the BPE tokenizer: {e}"))?;
        return Ok(Tokenizer::new(bpe));
    }

    Err(format!(
        "no tokenizer in {} — expected tokenizer.json, or vocab.json + merges.txt",
        dir.display()
    ))
}

impl Embedder for ClipEmbedder {
    fn dim(&self) -> usize {
        DIM
    }

    fn name(&self) -> &'static str {
        "CLIP ViT-B/32"
    }

    fn is_semantic(&self) -> bool {
        true
    }

    /// CLIP image/text cosines live in a narrow band far below 1.0; ~0.2
    /// is the usual "this really is what you asked for" line.
    fn min_similarity(&self) -> f32 {
        0.20
    }

    fn embed_image(&self, image: &Image_) -> Vec<f32> {
        // A failed encode returns a zero vector rather than panicking: the
        // index reads that as "no opinion", so one unreadable photo drops
        // out of the results instead of taking the app down.
        self.pixel_values(image)
            .and_then(|pixels| {
                self.model
                    .get_image_features(&pixels)
                    .map_err(|e| format!("the vision tower: {e}"))
            })
            .and_then(|features| Self::to_unit_vec(&features))
            .unwrap_or_else(|err| {
                eprintln!("snapsearch: could not embed an image: {err}");
                vec![0.0; DIM]
            })
    }

    fn embed_text(&self, text: &str) -> Vec<f32> {
        if text.trim().is_empty() {
            return vec![0.0; DIM];
        }
        // CLIP was trained on captions, so a bare noun does measurably
        // better wrapped the way its training data reads.
        let prompt = format!("a photo of {}", text.trim());
        self.token_ids(&prompt)
            .and_then(|ids| {
                self.model
                    .get_text_features(&ids)
                    .map_err(|e| format!("the text tower: {e}"))
            })
            .and_then(|features| Self::to_unit_vec(&features))
            .unwrap_or_else(|err| {
                eprintln!("snapsearch: could not embed a query: {err}");
                vec![0.0; DIM]
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::local::gradient_image;
    use vieww::foundation::Color;

    /// A CLIP-shaped BPE tokenizer built from a tiny vocabulary.
    ///
    /// Enough to exercise the tokenisation path — encode, truncate, pad to
    /// the 77-token context, and hand candle a `[1, 77]` tensor. The real
    /// vocabulary is 49,408 entries and changes the *ids*, not the shape.
    fn toy_tokenizer() -> Tokenizer {
        use tokenizers::models::bpe::Vocab;
        let mut vocab: Vocab = Vocab::new();
        for (i, token) in [
            "<|startoftext|>", "<|endoftext|>", "a", "photo", "of", "sunset", "dog", "the",
        ]
        .iter()
        .enumerate()
        {
            vocab.insert((*token).to_string(), i as u32);
        }
        let bpe = BPE::builder()
            .vocab_and_merges(vocab, vec![])
            .unk_token("<|endoftext|>".to_string())
            .build()
            .expect("building the toy BPE");
        Tokenizer::new(bpe)
    }

    fn untrained() -> ClipEmbedder {
        ClipEmbedder::untrained(toy_tokenizer()).expect("building an untrained CLIP")
    }

    /// The point of this test: a **real forward pass through the real
    /// architecture**. It runs `decode → resize → CLIP's own channel
    /// statistics → [1,3,224,224] → the vision tower → the visual
    /// projection → L2 norm`, and the same for the text tower. Only the
    /// trained values are missing.
    #[test]
    fn an_image_goes_all_the_way_through_the_vision_tower() {
        let embedder = untrained();
        let image = gradient_image(
            Color::rgb(0xff, 0x9a, 0x3c),
            Color::rgb(0xd6, 0x2f, 0x6d),
            64,
            48,
            0.0,
        );

        let v = embedder.embed_image(&image);

        assert_eq!(v.len(), DIM, "CLIP ViT-B/32 projects to 512");
        assert!(v.iter().all(|x| x.is_finite()), "no NaNs out of the tower");
    }

    #[test]
    fn a_query_goes_all_the_way_through_the_text_tower() {
        let embedder = untrained();
        let v = embedder.embed_text("a photo of a dog");
        assert_eq!(v.len(), DIM);
        assert!(v.iter().all(|x| x.is_finite()));
    }

    /// Preprocessing is where a CLIP integration usually goes quietly wrong
    /// — the wrong normalisation still produces plausible vectors, just
    /// worse ones. This pins the tensor the model actually receives.
    #[test]
    fn preprocessing_produces_the_tensor_shape_the_model_expects() {
        let embedder = untrained();
        let image = gradient_image(Color::WHITE, Color::BLACK, 10, 200, 0.0);
        let tensor = embedder.pixel_values(&image).expect("preprocessing");

        assert_eq!(
            tensor.dims(),
            &[1, 3, IMAGE_SIZE as usize, IMAGE_SIZE as usize],
            "batch, CHW, square — a non-square source must be resized, not cropped"
        );
    }

    #[test]
    fn a_query_is_padded_to_clips_context_length() {
        let embedder = untrained();
        let ids = embedder.token_ids("sunset").expect("tokenising");
        assert_eq!(ids.dims(), &[1, CONTEXT_LENGTH]);
    }

    /// An empty query must not be handed to the model at all — it would
    /// return a vector, and a vector is a ranking.
    #[test]
    fn an_empty_query_embeds_to_zero_rather_than_to_a_ranking() {
        let embedder = untrained();
        assert!(embedder.embed_text("   ").iter().all(|x| *x == 0.0));
    }

    /// The pipeline is genuinely sensitive to its input: two different
    /// pictures come out as two different vectors, and each is a unit
    /// vector. That is as far as an untrained model can be verified — the
    /// numbers are meaningless, but they are *responding*, which is what
    /// rules out a preprocessing path that quietly discards the image.
    #[test]
    fn different_pictures_produce_different_unit_vectors() {
        let embedder = untrained();
        let a = embedder.embed_image(&gradient_image(Color::WHITE, Color::BLACK, 32, 32, 0.0));
        let b = embedder.embed_image(&gradient_image(
            Color::rgb(0, 128, 255),
            Color::rgb(255, 0, 0),
            32,
            32,
            0.0,
        ));

        assert_ne!(a, b, "the tower must actually see the pixels");
        for v in [&a, &b] {
            let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!((norm - 1.0).abs() < 1e-3, "not unit length: {norm}");
        }
    }

    /// The last unverified link in the chain: reading a model **off disk**.
    ///
    /// Everything above builds the towers in memory. This writes a real
    /// `model.safetensors` with CLIP ViT-B/32's exact tensor names and
    /// shapes, writes a real `tokenizer.json` beside it, and then loads the
    /// directory through the same `ClipEmbedder::load` a downloaded model
    /// goes through — memory map, `VarBuilder::from_mmaped_safetensors`,
    /// `ClipModel::new`, tokenizer discovery, the lot.
    ///
    /// After this, the only thing separating the app from real CLIP is the
    /// *values* in that file.
    #[test]
    fn a_model_directory_on_disk_loads_and_embeds() {
        let dir = std::env::temp_dir().join("snapsearch-clip-load-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let (_, varmap) = ClipEmbedder::untrained_with(toy_tokenizer())
            .expect("building an untrained CLIP");
        varmap
            .save(dir.join("model.safetensors"))
            .expect("writing real safetensors");
        toy_tokenizer()
            .save(dir.join("tokenizer.json"), false)
            .expect("writing the tokenizer");

        let loaded = ClipEmbedder::load(&dir).expect("loading the model directory");
        assert_eq!(loaded.name(), "CLIP ViT-B/32");

        let v = loaded.embed_image(&gradient_image(Color::WHITE, Color::BLACK, 32, 32, 0.0));
        assert_eq!(v.len(), DIM);
        assert!(v.iter().all(|x| x.is_finite()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A directory missing its tokenizer is refused with a message naming
    /// the thing that is missing, not a panic three frames deep in candle.
    #[test]
    fn a_model_directory_without_a_tokenizer_says_so() {
        let dir = std::env::temp_dir().join("snapsearch-clip-notokenizer-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (_, varmap) = ClipEmbedder::untrained_with(toy_tokenizer()).unwrap();
        varmap.save(dir.join("model.safetensors")).unwrap();

        let error = ClipEmbedder::load(&dir).unwrap_err();
        assert!(error.contains("tokenizer"), "unhelpful error: {error}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// And the limit, asserted so the tests above are not read as more than
    /// they are: an untrained model ranks nothing. Only real weights make
    /// the similarity mean anything.
    #[test]
    fn an_untrained_model_is_not_a_search_engine() {
        let embedder = untrained();
        let sunset = embedder.embed_image(&gradient_image(
            Color::rgb(0xff, 0x9a, 0x3c),
            Color::rgb(0xd6, 0x2f, 0x6d),
            32,
            32,
            0.0,
        ));
        let query = embedder.embed_text("a photo of sunset");
        let similarity = crate::embed::cosine(&query, &sunset);
        assert!(
            similarity.is_finite(),
            "it produces a number, and that number means nothing yet"
        );
    }
}
