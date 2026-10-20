//! Wire protocols, written from scratch per the one-dependency
//! doctrine. API shapes mimic the tools we studied; the code is ours.
//!
//! * [`json`] — a JSON value model + parser + serializer (the
//!   serde_json API shape, ~1/50th the surface, zero deps).
//! * [`http`] — HTTP/1.1 request encoding and response decoding over
//!   an abstract transport. Every LLM REST API speaks 1.1.
//! * [`sse`] — server-sent-events line parser for streaming.
//!
//! Nothing here performs I/O; the runtime supplies streams. That keeps
//! this crate pure and these modules testable with plain byte slices.

pub mod http;
pub mod json;
pub mod sse;
