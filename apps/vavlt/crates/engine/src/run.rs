//! Encode, prove, report.
//!
//! Nothing is written and nothing is deleted: the vavlt store is build-order
//! step 4 and does not exist yet, so this run measures on the real files and
//! leaves every original exactly where it was. The UI says so rather than
//! rounding it up to "reclaimed".

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use vavlt_codecs::{Codec, LeptonJpeg, Oxipng, Zstd};
use vavlt_core::{classify_bytes, AssetClass, Tier};

use crate::item::{Item, Outcome, Proof};
use crate::source::Source;

/// What a run tells whoever is watching it.
///
/// One `File` per item, in order, as it completes — not a batch at the end.
/// The working screen turns each of these into a tile losing its veil, which is
/// what makes the progress indicator the user's own photographs rather than an
/// abstract bar.
#[derive(Debug)]
pub enum Event {
    Started,
    File {
        index: usize,
        name: String,
        outcome: Outcome,
    },
    Finished {
        cancelled: bool,
    },
}

/// The compression matrix (SPEC §4), for the codecs that exist in this build.
///
/// Returning `None` is a policy statement, not a gap: RAW is never re-encoded
/// in any tier, and already-efficient formats are stored as they are rather
/// than being churned for a rounding error.
fn codec_for(class: AssetClass) -> Option<Box<dyn Codec>> {
    match class {
        AssetClass::Jpeg => Some(Box::new(LeptonJpeg)),
        AssetClass::Png => Some(Box::new(Oxipng { preset: 4 })),
        AssetClass::Document | AssetClass::Other => Some(Box::new(Zstd { level: 19 })),
        // Raw is protected. HEIC and video are stored as-is until the platform
        // encoders land (AMediaCodec — SPEC §5, pending).
        _ => None,
    }
}

fn skip_reason(class: AssetClass) -> &'static str {
    match class {
        AssetClass::Raw => "RAW is never re-encoded, in any tier",
        AssetClass::Heic => "already efficient — stored as-is",
        c if c.is_video() => "no hardware encoder in this build — stored as-is",
        _ => "no codec applies",
    }
}

/// Run `items` under `tier`, reporting each result as it lands.
///
/// `cancel` is checked between files rather than inside a codec: stopping
/// mid-encode would leave a partial result that has to be reasoned about, and
/// there is nothing to gain from it — the longest single file here is under a
/// second.
pub fn run(
    source: Arc<dyn Source>,
    items: Vec<Item>,
    tier: Tier,
    cancel: Arc<AtomicBool>,
    mut emit: impl FnMut(Event),
) {
    log::info!("run: {} items, tier {tier:?}", items.len());
    emit(Event::Started);

    for (index, item) in items.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            log::info!("run cancelled after {index} of {}", items.len());
            emit(Event::Finished { cancelled: true });
            return;
        }

        let outcome = process(source.as_ref(), item, tier);
        log::debug!(
            "{}: {} {} -> {} ({})",
            item.name,
            outcome.codec,
            outcome.before,
            outcome.after,
            outcome.proof.label()
        );
        emit(Event::File {
            index,
            name: item.name.clone(),
            outcome,
        });
    }

    log::info!("run finished: {} items", items.len());
    emit(Event::Finished { cancelled: false });
}

fn process(source: &dyn Source, item: &Item, tier: Tier) -> Outcome {
    let bytes = match source.read(&item.handle) {
        Ok(bytes) => bytes,
        Err(error) => {
            // A grant that lapsed, a file that moved. Costs this file and
            // nothing else, and is logged as loudly as a success.
            log::warn!("{}: unreadable: {error:#}", item.name);
            return Outcome {
                name: item.name.clone(),
                codec: "—".into(),
                before: item.bytes,
                after: item.bytes,
                proof: Proof::Failed,
                note: error.to_string(),
                fell_back: false,
            };
        }
    };

    // The header is authoritative; the extension was only ever a claim.
    let sniffed = classify_bytes(&bytes, Path::new(&item.name));
    let mismatch = sniffed != item.class;

    let Some(codec) = codec_for(sniffed) else {
        return Outcome {
            name: item.name.clone(),
            codec: "—".into(),
            before: bytes.len() as u64,
            after: bytes.len() as u64,
            proof: Proof::Skipped,
            note: skip_reason(sniffed).into(),
            fell_back: false,
        };
    };

    let mut note = if mismatch {
        format!(
            "header says {} — the name said {}",
            sniffed.label(),
            item.class.label()
        )
    } else {
        String::new()
    };

    match codec.run(&bytes) {
        Ok(out) => {
            let proof = match out.roundtrip_ok {
                Some(true) => Proof::Proven,
                Some(false) => Proof::Failed,
                // oxipng: pixel-lossless, not byte-lossless.
                None => Proof::PixelLossless,
            };

            // Deep Move promises a *smaller* result than Move, and it can only
            // deliver that with a scored lossy encode. There is no scorer here,
            // so say so rather than presenting the lossless number as if the
            // tier had been honoured.
            let fell_back = tier != Tier::Move && sniffed.is_photo();
            if fell_back {
                if !note.is_empty() {
                    note.push_str(" · ");
                }
                note.push_str(
                    "Deep Move fell back to lossless: no perceptual scorer in this build",
                );
            }

            Outcome {
                name: item.name.clone(),
                codec: codec.name().into(),
                before: bytes.len() as u64,
                after: out.out_bytes,
                proof,
                note,
                fell_back,
            }
        }
        Err(error) => {
            log::warn!("{}: {} failed: {error:#}", item.name, codec.name());
            Outcome {
                name: item.name.clone(),
                codec: codec.name().into(),
                before: bytes.len() as u64,
                after: bytes.len() as u64,
                proof: Proof::Failed,
                note: error.to_string(),
                fell_back: false,
            }
        }
    }
}
