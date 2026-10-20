# Changelog

## 0.2.0 — 2026-10-20 — "the engine runs"

From a tested scaffold to a harness that builds, checks and delivers a
project end to end. The one-dependency doctrine holds: the headless binary
has zero external crates; the `ui` feature adds the vieww repository at a
pinned commit (ADR-011).

### Fixed (found in the v0.1 review; each has a regression test)
- **Cache freshness.** Agents that read the outside world or touch it were
  cached: a scheduled digest would replay yesterday's data and never send.
  Now volatile / delivery agents always run (ADR-015).
- **Checks after delivery.** Testwright ran after the Mailer had sent.
  Delivery now waits for the Reviewer; a rejection or test failure skips it.
- **Refire precision.** An edge reorder (same commit hash) refired every
  agent; adding one edge refired unrelated ones; system agents were missing
  from the plan. Now set-based, target-only, system-aware, and never calls
  live work "cached".
- **Permission gate holes.** The tool allowlist was never checked; the
  approval policy was a caller argument; tools could be called without the
  gate. Now the gate owns all four checks and tools require an unforgeable
  `Permit`.
- **Sandbox escape.** A symlink inside the project read files outside it.
  Path resolution now refuses links anywhere along a path; shell/git/tests
  run under Landlock.
- **Crashes from hostile input.** Deep JSON overflowed the stack; huge
  Content-Length / chunk sizes aborted on allocation; header values were
  not checked for CR/LF. All bounded and refused.
- **Executor.** Cancelled futures were never dropped; `TaskSet` orphaned
  tasks; a panic poisoned the executor; idle loops woke every 1 ms; the
  cancellation token could miss a wake-up and duplicated wakers.
- **Hash.** FNV-1a replaced by a BLAKE3 port pinned to official vectors.
- **MSRV** was declared 1.75; vieww needs 1.89.

### Added
- Agent loop with streaming tool calls (Anthropic; OpenAI-compatible for
  OpenAI, DeepSeek, GLM, Kimi, xAI, local gateways), retries with
  `retry-after`, fallback model, prompt caching, context compaction,
  per-firing budget and turn limits.
- Firing orchestration: parallel stages, built-in Shipwright / Operator,
  cache with early cutoff, resume after crash, receipts with the event
  log's hash, daily budget accounting, one-firing-per-project lock.
- Tools: write-output, files-read (paged), files-list, files-write, edit,
  apply-patch (all-or-nothing), git (allowlisted), run-tests, shell,
  http-read / http-post (private addresses refused by default).
- Linux sandbox: Landlock filesystem + TCP rules, user+network namespace,
  scrubbed env, scratch HOME, process-group timeouts. Fails closed.
- TLS via the system OpenSSL (`dlopen`), CONNECT proxies, NO_PROXY with CIDR.
- Interview with a `propose_blueprint` tool and a code-generated Confirm
  Gate (ADR-014).
- Universal project convention for python / node / rust / shell, each
  skeleton's own tests green with no installs.
- Cron + IANA time zones (TZif + POSIX rules, DST gaps and overlaps);
  `wright daemon`; systemd / launchd installers.
- Secrets via `security` / `secret-tool`, `WRIGHT_SECRET_*` for CI.
- Provider config (`~/.wright/config.kdl`) with prices; unknown models are
  priced high so budgets fail safe.
- CLI: new, plan, run, resume, runs, replay, approve, diff, schedule,
  daemon, secret, mock-provider, where, doctor, ui.
- The vieww shell: Describe · Plan · Run · Result · Inspect, live events
  and approvals from worker threads, headless `--snapshot`.
- An offline mock provider (a real HTTP/SSE server) that plays every role,
  used by the end-to-end tests and the quick start.

### Changed
- Wright's state moved out of projects to `~/.wright/projects/` (ADR-013).
- Project-agent prompts no longer include the project name or intent
  (editing display text must not silently change what agents are told);
  `PROMPT_VERSION` and inference settings are part of every cache key.

### Not yet
- macOS / Windows TLS and sandbox (they refuse, never downgrade), Windows
  keychain, `browser` tool, MCP, recipes and signing (ADR-012), server mode,
  typed edge bindings. See `docs/SPIKES.md`.
