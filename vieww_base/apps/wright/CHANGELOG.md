# Changelog

## 1.0.0 — 2026-10-22 — trust before reach, agents as workers, recipes at the end

The release the strategy memo ordered, whole: the memo set out four
releases (trust, workers, everywhere, recipes), and every later step
leans on the gate rule and the signed receipts — so those came first,
and they carry the rest of the roadmap with them in one go. What this
machine cannot test honestly (the native sandbox and TLS legs for
macOS and Windows, a published gallery with real publisher keys) is
written down in `docs/AUDIT-2026-10.md`, not pretended.

- **The Rule of Two is enforced** (ADR-024). Every tool names the legs
  it puts an agent on — private data, untrusted input, an outbound
  channel — and the validator refuses a plan whose agent holds all
  three without `approval "before-send"`. Untrusted data is tracked as
  a taint: an artifact made from outside input carries the mark through
  the whole graph, and anything that reads it inherits it. The gate
  asks before every outbound step of a 3-leg agent, whatever the card
  says. The Confirm Gate says all of this in words.
- **Receipts are signed and verifiable** (ADR-025). Runs now write
  `wright.run-receipt/2`: every consumed input's hash, every
  acceptance sentence with its verdict, the Rule-of-Two agents, and an
  Ed25519 signature over the receipt's exact bytes. Ed25519 and SHA-512
  are reference ports in `wright-core` (no crates, no unsafe), pinned
  byte-for-byte to the RFC's vectors and to OpenSSL on this machine.
  `wright verify` runs five checks in words — signature, event log,
  artifacts, inputs, blueprint — and a tampered byte fails by name.
- **Acceptance in words** (the memo's item 1). A deliverable can carry
  `accept "..."` sentences: the person approves the promise, Testwright
  turns each into a check (`ACCEPT 1: PASS` lines, parsed strictly),
  and the Reviewer may not approve a run whose sentences fail — the
  engine enforces it, not the prompt.
- **A spending promise before the run** (item 3). `wright run` shows the
  estimate as a sentence ("about $0.04; the built-in model would be free
  but slower") and offers the built-in as a plain choice, not a config.
- **Other agents as workers** (ADR-026). A step can say `worker
  "claude-code"` (or codex, kimi-code, zcode — configured per machine):
  it runs in a sandboxed copy, its diff is applied whole or not at all,
  inside `worker-files` scope only, under the card's clock, through
  the permit gate, and its output goes through Wright's tests. No model
  cost on your plan.
- **MCP behind permits** (ADR-029). Hash-pinned MCP servers (config
  refuses an unpinned one; `wright mcp-hash` prints the pin), cards
  allow-list the exact tools, the server must offer them too, answers
  are treated as outside data, and the Rule of Two applies.
- **Typed edges** (ADR-027). An edge can bind `out=`/`in=` names with a
  `schema=` of five readable shapes; broken names fail at plan time,
  broken bytes before the consumer runs.
- **Recipes, curated and signed** (ADR-028). `wright recipe
  list|install|sign|verify` over a `recipes/` gallery; nothing installs
  unsigned — that is the lesson of the poisoned marketplaces.
- **Explain any run in one screen** (item 5). `wright explain` prints
  the story — plan, what ran, what it sent, what it made, the verdict,
  the numbers — from the store, never from a model.
- **Hand-off as a feature** (item 7). `wright handoff` writes a PR
  folder: HANDOFF.md (what this is, in the owner's words, promises
  listed), the exact committed blueprint, every receipt, the project's
  full diff against its first commit, and the checks.
- **A public benchmark** (ADR-030). `wright bench` measures time to a
  checked outcome and human interventions per run; the receipts stay
  for verification.
- **Voice first on Home** (item 6). The mic is now the main way to
  start: a big speak affordance sits above the composer, and a
  transcript starts the interview on its own (typing still needs send —
  a keyboard is a decision already made). Everything runs on this
  machine's built-in Whisper; nothing leaves.

## 0.5.2 — 2026-10-21 — a livelier conversation

- **You can see a reply being written.** The moment you send, the thread
  shows a spinner and what Wright is doing ("Reading your message",
  "Thinking", "Drafting the plan", "Checking the draft plan", or a model
  download line), with a seconds counter. The reply then appears word by
  word as the model streams it, and the page follows it down. Before this,
  the screen did not change until the whole answer had arrived.
- **Saved sessions keep their conversation.** Every message is written as it
  happens to `conversation.jsonl` in the project's store (beside its run
  history, outside the project folder). A session reopened from Saved, or
  after a restart, shows the whole conversation: the part that drafted the
  plan above the plan, and the changes you asked for below it. If you type
  in a session whose interview had ended, the interviewer starts from that
  conversation instead of from scratch. Saved cards on Home now show the
  plan's name, the last message and how many messages there are. The
  overview shows smaller chips.
- **Agents & tools page** (Settings → Agents & tools; also on Home, in Help
  and on the plan card). It lists Wright's nine built-in agents (Interviewer,
  Architect, Shipwright, Testwright, Logwright, Reviewer, Firewright,
  Operator, Explainer), with what each does and when it joins. It lists
  every tool a plan can grant, with what the tool may touch (only reads /
  changes the project / touches the world and asks first) and the
  permissions it needs. With a project open, it also lists that plan's own
  agents and the tools each was given.
- **Pinch to see every session.** On a trackpad, pinch with two fingers
  together to open the overview and apart to go back. This works on macOS
  (the native gesture) and on Windows (where a touchpad pinch arrives as
  Ctrl + wheel). vieww's desktop layer gained `App::on_pinch` for this. One
  pinch moves the view once, and a small twitch does nothing. The `O` key
  still works.

## 0.5.1 — 2026-10-21 — smooth scrolling

- **Scrolled text no longer looks wavy.** vieww's `Scrollable` now moves
  its content in whole device pixels. A fractional offset (from a trackpad
  or a drag) put each line of text at its own sub-pixel phase, and the
  renderer snaps each line to the pixel grid on its own, so lines stepped
  out of time with each other and with their boxes. A test now checks that
  a scroll of 0.6 px moves the page by exactly one pixel, every row alike.
- **Scrolling is much lighter on the CPU.** Measured scrolling Settings in
  a 1280x800 window (`scroll_frame_cost`, an opt-in benchmark):
  171 ms -> 14 ms a frame at a device ratio of 1, 926 ms -> 52 ms at 2.
  - vieww's CPU renderer repaints neighbouring damage as one region: it
    replayed the whole scene once per rectangle, and a scrolled panel
    reports one per card.
  - vieww's shadow cache is sized to the screen: on a high-density display
    a dialog's shadow was bigger than the cache and evicted every frame.
  - Wright's background W is drawn as stacked soft strokes instead of a
    blurred layer: it looks the same, and it no longer re-blurs under
    everything that moves in front of it.

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
