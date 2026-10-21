# Changelog

## 0.5.0 — 2026-10-21 — built-in models

- **Built-in models, run by Wright itself** (ADR-022, `docs/MODELS.md`):
  `qwen3-1.7b` (Qwen3-1.7B, 8-bit: text and tool calls) and `whisper-base`
  (speech to text, 99 languages). No key, no account; after a one-time
  download nothing leaves the machine.
  - Downloaded on first use into `~/.wright/models`, resumable, checked
    against pinned SHA-256s (a wrong file is deleted, never loaded).
    `WRIGHT_MODEL_MIRROR` for networks that cannot reach Hugging Face.
  - **Voice uses the built-in Whisper by default.**
  - **No key? The project still runs**: a model with no provider or key
    falls back to `qwen3-1.7b`, with a line in the event river saying so;
    it is also the last fallback if every cloud endpoint fails mid-run.
    `builtin fallback=false` (or Settings) turns this off.
  - `wright model list | pull | rm | ask | listen`; Settings → Models &
    keys → **Built-in models** (download with progress, remove, the
    fallback switch); `wright doctor` reports them.
- **`wright-infer`**, a new std-only crate: GGUF and whisper.cpp file
  readers, Qwen's tokenizer, the Qwen3 decoder with a reusable key/value
  cache, Qwen3's chat and tool-call format, WAV decoding, the log-mel
  spectrogram, Whisper's encoder and decoder. Checked against llama.cpp
  and whisper.cpp on every CI run (tokens, logits, language guesses) and
  on the real weights on Windows, macOS and Linux
  (`.github/workflows/wright-models.yml`).
- **Prompt safety:** text from outside (messages, tool results, fetched
  pages) is never encoded with control tokens, so it cannot open a turn.
- **HTTPS on macOS and Windows** through the system curl (ADR-023): cloud
  providers and model downloads now work there. The request, keys
  included, goes to curl on stdin, never on the command line.
  `wright doctor` shows which transport carries HTTPS.
- `core::sha256` hashes in pieces (`Sha256::update`), for gigabyte files.

## 0.4.0 — 2026-10-21 — export

- **`wright export`** and **Export** on the result card (ADR-021):
  - the source as a `.zip` — always; without logs, caches, installs or
    builds; the same project makes the same bytes;
  - a **Python wheel** (`py3-none-any`), written by Wright with no `build`
    package: RECORD hashes, METADATA from the blueprint and README, and a
    console command when `__main__.py` has `main()`. pip installs it;
  - a **Rust crate** via `cargo package` (into a scratch folder; the
    project's `target/` is untouched);
  - a **React web build**: `npm install`, `npm run build`, `dist/` zipped.
  Packages need the last run to have passed its checks and review. Exports
  go to `Exports/<project>` beside the project.
- **React projects** (`language "react"`): Vite + React 18, logic tested
  with no installs (`node --test`), JSX for the screen.
- `core::zip` (stored, reproducible) and `core::sha256` (FIPS 180-4),
  both std-only and checked against the published test values.
- Tests check exports with the tools that read them: Python's zipfile,
  pip (install, import, run), cargo, and a real Vite build with
  `WRIGHT_TEST_NPM=1`.

## 0.3.1 — 2026-10-21 — first-install fixes

- **The window opens centred and fits the screen.** It was placed by the
  OS default and, on a 1366x768 laptop, spilled off the bottom right
  (fixed in vieww-platform-winit for every vieww app).
- **The background W matches the logo.** The mark is one round-joined
  stroke; the watermark is that stroke blurred. Before, transparent
  segments overlapped at the joints and showed as darker discs.
- **Scroll bars.** Home, sessions, the engine and Settings scroll with a
  wheel, a trackpad, or a thin bar on the right you can drag. (Home and
  Settings did not scroll at all before.)
- **The caret shows only in the field you are typing in.** A vieww text
  field always draws its caret; the shell now tells each field whether it
  has focus.
- The mark and "wright" sit closer together in the top bar and status bar.

## 0.3.0 — 2026-10-20 — "the workspace"

A new window, built in vieww, and installers for three platforms.

### The window (ADR-019)
- **Opening:** the W draws itself, grows, softens, and settles as a pale
  mark behind every page.
- **Sessions in a row:** Home, one page per project, and a new-session page.
  Two fingers sideways move between them; swiping past the last starts a
  new one (its own folder under `~/Wright`, named from your first words).
- **Overview:** every session as a card; × closes it into **Saved**
  (Undo, the Saved shelf, Ctrl/Cmd+Shift+T reopen). Closing stops nothing.
- **The engine, on request:** Space peeks at the live agents and tools,
  E keeps them; steps run left to right, edges march while work flows,
  tool chips light up as they are called, and an inspector shows what an
  agent may touch and its live log.
- **Approvals in the conversation:** Allow / Don't allow, or drag the
  card right / left.
- **Composer:** `+` attaches files or a folder (the system's own dialog,
  or drop onto the window; files are copied into `artifacts/`), and the
  mic turns speech into text (any OpenAI-compatible speech model:
  `voice model="..."`).
- **Settings:** models and keys (keys go to the OS keychain), defaults,
  appearance (VS Code Dark/Light Modern, any accent colour), workspace
  (projects folder, background schedules), and the project's model tiers
  and spending limits (written to its blueprint, versioned).
- **Status bar** in the VS Code manner: where you are, what is running,
  today's spend, theme.

### Installers (ADR-020)
- `wright-release.yml`: Windows `.exe` installer, macOS universal `.dmg`,
  Linux `.deb` and `.tar.gz`, each smoke-tested on its own OS.
- `[profile.dist]` (LTO, stripped). LTO moved out of `release`, which
  failed on Linux under vieww_base's `prefer-dynamic`.
- `wright ui --icon <png>`: the app icon, rendered by the app.

### Runtime
- `wright_runtime::voice`: 16 kHz WAV and OpenAI-compatible transcription;
  the mock provider answers `/audio/transcriptions`.
- `Config::parse_file_only` / `to_kdl` / `save_file`: Settings writes
  `config.kdl` (keeping `config.kdl.bak`) without copying the built-ins.

### Tests
- The UI is driven end to end headlessly: a run with a dragged approval,
  swiping and its momentum lock, Space-peek, overview, close/Undo/reopen,
  attachments, voice through the mock, accent persistence, project limits.
- **Layout overflow is a failure:** `wright ui --snapshot` fails if any
  widget overflowed while rendering (vieww's overflow gate).

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
