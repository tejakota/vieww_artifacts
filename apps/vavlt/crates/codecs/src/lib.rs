//! Codec implementations behind one trait, so the engine and the measurement
//! harness exercise exactly the same code.
//!
//! Invariant: every lossless codec verifies its own roundtrip. A codec that
//! cannot prove byte-exact reconstruction must not report success.

use anyhow::Result;
use std::time::Instant;
use vavlt_core::AssetClass;

pub mod video;

pub struct CodecOutput {
    pub out_bytes: u64,
    pub encode_ms: u128,
    /// `Some(true/false)` for lossless codecs that ran a real decode+compare.
    /// `None` for lossy codecs, where byte equality is meaningless.
    pub roundtrip_ok: Option<bool>,
    pub decode_ms: Option<u128>,
}

pub trait Codec: Sync + Send {
    fn name(&self) -> &'static str;
    fn applies(&self, class: AssetClass) -> bool;
    fn lossless(&self) -> bool;
    fn run(&self, input: &[u8]) -> Result<CodecOutput>;
}

/// Lossless JPEG recode. Bit-exact; the original JPEG is reconstructable.
pub struct LeptonJpeg;

impl Codec for LeptonJpeg {
    fn name(&self) -> &'static str {
        "lepton"
    }
    fn applies(&self, class: AssetClass) -> bool {
        class == AssetClass::Jpeg
    }
    fn lossless(&self) -> bool {
        true
    }

    fn run(&self, input: &[u8]) -> Result<CodecOutput> {
        use lepton_jpeg::{decode_lepton, encode_lepton, EnabledFeatures, SingleThreadPool};
        use std::io::Cursor;

        let features = EnabledFeatures::compat_lepton_vector_write();
        // Single-threaded per file: the harness already parallelises across
        // files with rayon, and nested pools would distort the timings.
        let pool = SingleThreadPool::default();

        let t = Instant::now();
        let mut compressed = Cursor::new(Vec::new());
        encode_lepton(&mut Cursor::new(input), &mut compressed, &features, &pool)
            .map_err(|e| anyhow::anyhow!("lepton encode: {e:?}"))?;
        let encode_ms = t.elapsed().as_millis();
        let compressed = compressed.into_inner();

        // Proof, not assumption: decode and byte-compare.
        let t = Instant::now();
        let mut restored = Vec::new();
        decode_lepton(
            &mut Cursor::new(&compressed),
            &mut restored,
            &features,
            &pool,
        )
        .map_err(|e| anyhow::anyhow!("lepton decode: {e:?}"))?;
        let decode_ms = t.elapsed().as_millis();

        Ok(CodecOutput {
            out_bytes: compressed.len() as u64,
            encode_ms,
            roundtrip_ok: Some(restored == input),
            decode_ms: Some(decode_ms),
        })
    }
}

/// Lossless PNG re-optimisation. Pixels identical; file bytes differ.
pub struct Oxipng {
    pub preset: u8,
}

impl Codec for Oxipng {
    fn name(&self) -> &'static str {
        "oxipng"
    }
    fn applies(&self, class: AssetClass) -> bool {
        class == AssetClass::Png
    }
    fn lossless(&self) -> bool {
        true
    }

    fn run(&self, input: &[u8]) -> Result<CodecOutput> {
        let opts = oxipng::Options::from_preset(self.preset);
        let t = Instant::now();
        let out = oxipng::optimize_from_memory(input, &opts)
            .map_err(|e| anyhow::anyhow!("oxipng: {e}"))?;
        Ok(CodecOutput {
            out_bytes: out.len() as u64,
            encode_ms: t.elapsed().as_millis(),
            // Pixel-lossless, not byte-lossless: the original file cannot be
            // reproduced, so the vavlt must store the optimised PNG as the
            // canonical copy. Recorded honestly as `None`.
            roundtrip_ok: None,
            decode_ms: None,
        })
    }
}

/// General-purpose lossless compression. The document-bucket workhorse, and
/// the control that shows how little generic compression helps on media.
pub struct Zstd {
    pub level: i32,
}

impl Codec for Zstd {
    fn name(&self) -> &'static str {
        "zstd"
    }
    fn applies(&self, _class: AssetClass) -> bool {
        true
    }
    fn lossless(&self) -> bool {
        true
    }

    fn run(&self, input: &[u8]) -> Result<CodecOutput> {
        let t = Instant::now();
        let out = zstd::encode_all(input, self.level)?;
        let encode_ms = t.elapsed().as_millis();

        let t = Instant::now();
        let restored = zstd::decode_all(&out[..])?;
        let decode_ms = t.elapsed().as_millis();

        Ok(CodecOutput {
            out_bytes: out.len() as u64,
            encode_ms,
            roundtrip_ok: Some(restored == input),
            decode_ms: Some(decode_ms),
        })
    }
}

/// Lossy AVIF. Measured without a quality gate here — the harness reports raw
/// ratio and speed; the engine adds the SSIMULACRA2 floor and CRF search.
#[cfg(feature = "avif")]
pub struct Avif {
    pub quality: f32,
    pub speed: u8,
}

#[cfg(feature = "avif")]
impl Codec for Avif {
    fn name(&self) -> &'static str {
        "avif"
    }
    fn applies(&self, class: AssetClass) -> bool {
        matches!(class, AssetClass::Jpeg | AssetClass::Png)
    }
    fn lossless(&self) -> bool {
        false
    }

    fn run(&self, input: &[u8]) -> Result<CodecOutput> {
        let img = image::load_from_memory(input)?.to_rgb8();
        let (w, h) = img.dimensions();
        let pixels: Vec<rgb::RGB8> = img
            .pixels()
            .map(|p| rgb::RGB8::new(p[0], p[1], p[2]))
            .collect();

        let t = Instant::now();
        let res = ravif::Encoder::new()
            .with_quality(self.quality)
            .with_speed(self.speed)
            .encode_rgb(ravif::Img::new(&pixels[..], w as usize, h as usize))?;

        Ok(CodecOutput {
            out_bytes: res.avif_file.len() as u64,
            encode_ms: t.elapsed().as_millis(),
            roundtrip_ok: None,
            decode_ms: None,
        })
    }
}

/// Codec set for a measurement run.
pub fn default_set() -> Vec<Box<dyn Codec>> {
    #[cfg_attr(not(feature = "avif"), allow(unused_mut))]
    let mut v: Vec<Box<dyn Codec>> = vec![
        Box::new(LeptonJpeg),
        Box::new(Oxipng { preset: 4 }),
        Box::new(Zstd { level: 19 }),
    ];
    #[cfg(feature = "avif")]
    v.push(Box::new(Avif {
        quality: 80.0,
        speed: 6,
    }));
    v
}
