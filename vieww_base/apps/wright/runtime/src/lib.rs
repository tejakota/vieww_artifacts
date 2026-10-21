//! # wright-runtime
//!
//! The engine room. Everything here touches the outside world — sockets,
//! files, processes, the clock — so `wright-core` can stay pure.
//!
//! * [`executor`] — a small structured-concurrency executor (spawn, join
//!   sets, cancellation, `spawn_blocking`), the tokio API shape, ours.
//! * [`gate`] — the permission gate and the unforgeable [`gate::Permit`]
//!   every tool demands.
//! * [`cache`] — Bazel-style task cache keys, with freshness rules for
//!   tasks that read or touch the world.
//! * [`store`] — the project's durable memory under `.wright/`: fsynced
//!   event logs, content-addressed objects, cache, blueprint history,
//!   cross-process approvals, spend.
//! * [`net`] / [`tls`] / [`curl`] — HTTP over TCP and OS-native TLS, with
//!   proxies; the OS's curl carries HTTPS where TLS is not wired natively.
//! * [`models`] — Anthropic + OpenAI-compatible providers: streaming,
//!   tool calling, retries, fallback.
//! * [`config`] — which provider serves which model, and what it costs.
//! * [`local`] — the built-in models: catalog, verified downloads, and the
//!   engine slot `wright-infer` fills.
//! * [`secrets`] — `keychain:` references resolved at the last moment.
//! * [`tz`] — the OS zone database, for schedules.
//!
//! The runtime never talks to the UI. It writes events; the UI reads them.

pub mod cache;
pub mod config;
pub mod curl;
pub mod executor;
pub mod gate;
pub mod local;
pub mod models;
pub mod net;
pub mod secrets;
pub mod store;
pub mod tls;
pub mod tz;
pub mod voice;
