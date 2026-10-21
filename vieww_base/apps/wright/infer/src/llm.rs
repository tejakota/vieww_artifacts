//! A completion from the built-in text model: the same request and
//! response every cloud provider takes and gives.
//!
//! ```text
//!  CompletionRequest -> chat::prompt -> tokens -> prefill (reusing the cache)
//!    -> sample, run, sample, run ... until <|im_end|> or max_tokens
//!    -> chat::parse -> CompletionResponse (text + tool uses)
//! ```

use std::sync::Mutex;

use wright_runtime::models::{Block, CompletionRequest, CompletionResponse, Delta, StopReason, Usage};

use crate::chat;
use crate::qwen3::{Cache, Model};
use crate::sample::Sampler;

/// Context the built-in model keeps (tokens). Bigger costs memory
/// (~110 KB a token for Qwen3-1.7B) and time; agent turns fit easily.
pub const CONTEXT: usize = 16_384;

pub struct Llm {
    pub model: Model,
    pub name: String,
    cache: Mutex<Cache>,
    stops: Vec<u32>,
}

impl Llm {
    pub fn new(model: Model, name: impl Into<String>) -> Llm {
        let cache = Mutex::new(model.cache(CONTEXT));
        let stops = ["<|im_end|>", "<|endoftext|>"].iter().filter_map(|s| model.tokenizer.id(s)).chain([model.tokenizer.eos]).collect();
        Llm { model, name: name.into(), cache, stops }
    }

    /// Generate an answer. `on_delta` sees text as it is written.
    pub fn complete(&self, req: &CompletionRequest, sampler: Sampler, on_delta: &mut dyn FnMut(&Delta)) -> Result<CompletionResponse, String> {
        // Template markup may hold control tokens; content never does.
        let ids: Vec<u32> = chat::prompt(req).parts.iter().flat_map(|(text, control)| self.model.tokenizer.encode(text, *control)).collect();
        let (out, n_out, stop) = self.generate(&ids, req.max_tokens as usize, sampler, on_delta)?;
        let blocks = chat::parse(&out);
        let stop = if blocks.iter().any(|b| matches!(b, Block::ToolUse { .. })) { StopReason::ToolUse } else { stop };
        Ok(CompletionResponse { blocks, usage: Usage { input_tokens: ids.len() as u64, output_tokens: n_out, cached_input_tokens: 0 }, stop, model: self.name.clone() })
    }

    /// Token-level generation: the text written, how many tokens, and why
    /// it stopped.
    pub fn generate(&self, prompt: &[u32], max_tokens: usize, mut sampler: Sampler, on_delta: &mut dyn FnMut(&Delta)) -> Result<(String, u64, StopReason), String> {
        let mut cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
        let mut logits = self.model.prefill(&mut cache, prompt)?;
        let mut bytes = Vec::new();
        let mut shown = 0usize;
        let mut announced = false;
        let mut n = 0u64;
        let room = CONTEXT.saturating_sub(cache.len());
        let limit = max_tokens.max(1).min(room);
        let mut stop = StopReason::MaxTokens;
        for _ in 0..limit {
            let tok = sampler.sample(&logits);
            if self.stops.contains(&tok) {
                stop = StopReason::EndTurn;
                // Keep the end marker in the cache: the next turn's prompt
                // has it right here, so the whole turn is reused.
                let _ = self.model.forward(&mut cache, &[tok]);
                break;
            }
            n += 1;
            bytes.extend(self.model.tokenizer.token_bytes(tok));
            // Stream whole characters, never a tag still being written.
            let text = valid_prefix(&bytes);
            let visible = chat::visible_prefix(text).trim_start();
            let visible_len = visible.len();
            if visible_len > shown {
                on_delta(&Delta::Text(visible[shown..].to_string()));
                shown = visible_len;
            }
            if !announced {
                if let Some(name) = tool_name_so_far(text) {
                    on_delta(&Delta::ToolStarted(name));
                    announced = true;
                }
            }
            if n as usize == limit {
                break;
            }
            logits = self.model.forward(&mut cache, &[tok])?;
        }
        Ok((String::from_utf8_lossy(&bytes).into_owned(), n, stop))
    }
}

/// The longest prefix of `bytes` that is whole UTF-8.
fn valid_prefix(bytes: &[u8]) -> &str {
    match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => std::str::from_utf8(&bytes[..e.valid_up_to()]).unwrap_or(""),
    }
}

/// `{"name": "files-read"` seen inside a tool call: the tool's name.
fn tool_name_so_far(text: &str) -> Option<String> {
    let body = &text[text.find("<tool_call>")?..];
    let after = &body[body.find("\"name\"")? + 6..];
    let after = after.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names_are_spotted_early() {
        assert_eq!(tool_name_so_far("ok <tool_call>\n{\"name\": \"files-read\", \"argu"), Some("files-read".into()));
        assert_eq!(tool_name_so_far("<tool_call>\n{\"name\": \"files-r"), None);
        assert_eq!(valid_prefix(&[b'a', 0xe2, 0x82]), "a");
    }
}
