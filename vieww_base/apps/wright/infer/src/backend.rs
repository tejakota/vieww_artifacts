//! The runtime's [`LocalBackend`], filled by this crate: the built-in
//! models, downloaded on first use, loaded once, kept warm.
//!
//! ```text
//!  complete("qwen3-1.7b", request)  -> file there? (else download, with Status lines)
//!                                   -> loaded? (else load ~2 GB, once)
//!                                   -> Llm::complete
//!  transcribe("whisper-base", wav)  -> the same, then Whisper::transcribe
//! ```

use std::sync::{Arc, Mutex};

use wright_runtime::local::{self, BuiltinModel, Kind, LocalBackend};
use wright_runtime::models::{CompletionRequest, CompletionResponse, Delta};

use crate::audio::wav_to_samples;
use crate::llm::Llm;
use crate::qwen3::Model;
use crate::sample::Sampler;
use crate::whisper::Whisper;

#[derive(Default)]
pub struct Engine {
    text: Mutex<Option<(String, Arc<Llm>)>>,
    speech: Mutex<Option<(String, Arc<Whisper>)>>,
}

/// Put the built-in engine in place for this process.
pub fn install() {
    local::install(Box::new(Engine::default()));
}

fn model(id: &str, kind: Kind) -> Result<&'static BuiltinModel, String> {
    let names = || {
        local::CATALOG
            .iter()
            .filter(|m| m.kind == kind)
            .map(|m| m.id)
            .collect::<Vec<_>>()
            .join(", ")
    };
    local::find(id).filter(|m| m.kind == kind).ok_or_else(|| {
        format!(
            "'{id}' is not a built-in {} model (built in: {})",
            if kind == Kind::Text { "text" } else { "speech" },
            names()
        )
    })
}

/// The file, downloading it first if need be; status lines go to `say`
/// (each 5% of the download, not every chunk).
fn ensure(m: &BuiltinModel, say: &mut dyn FnMut(String)) -> Result<std::path::PathBuf, String> {
    if m.installed() {
        return Ok(m.path());
    }
    let mut last = (local::Phase::Downloading, u32::MAX);
    local::pull(m, &mut |p| {
        let step = (p.phase, p.percent() / 5);
        if step != last {
            last = step;
            say(local::progress_line(m, p));
        }
    })
}

impl Engine {
    fn llm(&self, m: &BuiltinModel, say: &mut dyn FnMut(String)) -> Result<Arc<Llm>, String> {
        let mut slot = self.text.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((id, llm)) = slot.as_ref() {
            if id == m.id {
                return Ok(llm.clone());
            }
        }
        let path = ensure(m, say)?;
        say(format!(
            "Loading the built-in model {} ({})",
            m.id,
            m.size_text()
        ));
        let llm = Arc::new(Llm::new(Model::load(&path)?, m.id));
        *slot = Some((m.id.to_string(), llm.clone()));
        Ok(llm)
    }

    fn whisper(&self, m: &BuiltinModel) -> Result<Arc<Whisper>, String> {
        let mut slot = self.speech.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((id, w)) = slot.as_ref() {
            if id == m.id {
                return Ok(w.clone());
            }
        }
        let path = ensure(m, &mut |_| {})?;
        let w = Arc::new(Whisper::load(&path)?);
        *slot = Some((m.id.to_string(), w.clone()));
        Ok(w)
    }
}

impl LocalBackend for Engine {
    fn complete(
        &self,
        req: &CompletionRequest,
        on_delta: &mut dyn FnMut(&Delta),
    ) -> Result<CompletionResponse, String> {
        let m = model(&req.model, Kind::Text)?;
        let llm = self.llm(m, &mut |s| on_delta(&Delta::Status(s)))?;
        llm.complete(req, Sampler::qwen(), on_delta)
    }

    fn transcribe(&self, model_id: &str, wav: &[u8]) -> Result<String, String> {
        let m = model(model_id, Kind::Speech)?;
        let w = self.whisper(m)?;
        let samples = wav_to_samples(wav)?;
        Ok(w.transcribe(&samples, None)?.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_catalog_models_of_the_right_kind() {
        assert!(model("qwen3-1.7b", Kind::Text).is_ok());
        let e = model("whisper-base", Kind::Text).unwrap_err();
        assert!(e.contains("built in: qwen3-1.7b"), "{e}");
        assert!(model("gpt-9", Kind::Speech).is_err());
    }
}
