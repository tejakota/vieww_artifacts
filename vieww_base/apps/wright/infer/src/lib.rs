//! wright-infer: the built-in models, run in-house on the CPU.
//!
//! ```text
//!  gguf       the model file: metadata + tensors
//!  num        half floats, 8-bit blocks, dot products
//!  linear     matrix products across threads
//!  unicode    letter / number tables for the tokenizer
//!  tokenizer  Qwen's byte-level BPE
//!  qwen3      the text model's forward pass, with a key/value cache
//!  sample     picking the next token
//!  chat       requests to Qwen3's prompt format, output back to blocks
//!  llm        one completion, as the runtime asks for it
//!  audio      WAV decoding and the log-mel spectrogram
//!  whisper    the speech model
//!  backend    the runtime's LocalBackend: download on first use, load once
//! ```

pub mod audio;
pub mod backend;
pub mod chat;
pub mod gguf;
pub mod linear;
pub mod llm;
pub mod num;
pub mod qwen3;
pub mod sample;
pub mod tokenizer;
mod unicode;
pub mod whisper;

pub use backend::install;
