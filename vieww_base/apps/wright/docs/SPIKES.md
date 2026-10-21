# Week-One Spike Plan (W-1 … W-8)

## Status after v0.2 (2026-10-20)

| Spike | Status | Evidence |
|---|---|---|
| W-1 vieww consumability | **done** | pinned git dependency resolves and builds outside vieww's workspace (`app` feature `ui`); ADR-011 records the "one repository" reading |
| W-2 capability inventory | **done (for the shell)** | the UI uses vieww's widgets, signals, `App::waker`, `before_frame`, and the native rasterizer for headless PNG snapshots |
| W-3 parser hardening | **done** | line numbers on every error, decimals, `project` section, round-trip test, reserved names, input/edge reachability |
| W-4 cache-hit demo | **done** | `engine/tests/e2e.rs`: second firing re-fetches live data, serves the unchanged Summarizer from cache, re-delivers; BLAKE3 replaced FNV |
| W-5 executor I/O | **bridge done** | thread-per-call `spawn_blocking` behind the same API (documented); a reactor is a server-mode concern (v0.3) |
| W-5b sandbox | **Linux done** | Landlock FS + TCP rules, user+net namespace, `no_new_privs`, process-group kill; macOS Seatbelt / Windows AppContainer: refuse until wired |
| W-6 OS-native TLS | **Linux done** | OpenSSL via `dlopen`, SNI, hostname check, TLS 1.2 floor, CONNECT proxies; tested live direct and through a proxy |
| W-6b TLS macOS/Windows | **bridged** | HTTPS through the system curl (ADR-023); native Security.framework / SChannel FFI still open |
| W-7 replay | **done** | `wright replay <run> --at N` (projection only, ADR-016); `wright resume` continues crashed runs |
| W-8 daemon | **done (Linux/macOS)** | `wright daemon` + `wright schedule install` (systemd user unit / launchd agent) |

### v0.3 candidates (in priority order)

1. A real-model benchmark: time-to-first-artifact, manual interventions,
   tokens on a targeted change, vs. a direct baseline harness.
2. Typed edge bindings (output → input, with schema) — Open question 6.
3. macOS/Windows TLS + sandbox (W-6b, W-5b), Windows keychain.
4. MCP client; other harnesses (Claude Code, Codex, Kimi Code) as workers.
5. Recipes with OS-API signatures (ADR-012); server mode with a reactor.

---


Each spike states: goal, why now, acceptance criteria, effort. Spikes
produce *working code in this repo*, not documents. Sizes: S (< half
day), M (~a day), L (~two days).

Order matters: W-1 and W-2 unblock everything vieww-shaped; W-4 is the
demo that wins arguments; W-3/W-5/W-6 make it real; W-7/W-8 make it a
product.

---

## W-1 · vieww consumability · L · *the wiring spike, do first*

**Goal.** An external app (this one) uses vieww outside its own
workspace: card shell + StudioTheme + input pipeline rendering
"Hello, blueprint".

**Why now.** The one dependency must actually resolve. The
vieww_artifacts repo carries its Rust workspace under `vieww_base/`;
we must determine whether a git dependency can find the crates (root
manifest vs. `[patch]` with a path into a vendored checkout) and pin a
rev.

**Acceptance.**
- [ ] `app/Cargo.toml` enables `vieww` and `cargo build` succeeds on
      CI (Linux; macOS check locally).
- [ ] A debug window opens with a vieww card shell, one card titled
      "Hello, blueprint".
- [ ] The pinned rev + wiring method is documented in this file and in
      an ADR addendum to DECISIONS.md.
- [ ] The transitive dependency tree is dumped to
      `docs/vieww-tree.txt` (the one-time audit artifact, ADR-002).

---

## W-2 · vieww capability inventory · M

**Goal.** Know what vieww already ships before building anything by
hand: serialization, process spawning, filesystem helpers, task/message
runtime, audio, anything reusable.

**Why now.** One dependency means mining it for all it is worth. This
spike prevents us rebuilding things vieww already has.

**Acceptance.**
- [ ] A table in this file: vieww crate → could serve Wright as →
      verdict (use / skip / later).
- [ ] Every "use" row has a follow-up issue with the owning spike.

---

## W-3 · Blueprint parser hardening · M

**Goal.** Take the working two-stage parser from "tested" to "hostile-
input proof".

**Why now.** The parser is the front door of the compiler; blueprints
arrive from recipes (strangers) and from the Interviewer (ours, but
young).

**Acceptance.**
- [ ] Fuzz-ish corpus: 30 malformed inputs (unterminated everything,
      unicode edge cases, 10k-deep nesting, duplicate sections, empty
      agents) → all produce line-numbered errors, no panics.
- [ ] `cargo test` grows a `parse_hostile_corpus` test.
- [ ] Error messages reviewed for plain language (they render in the
      UI verbatim).

---

## W-4 · The cache-hit demo · L · *the anti-dsh showpiece*

**Goal.** End-to-end firing with a fake (deterministic) provider:
fire a blueprint, then edit one agent's goal, refire, and print the
cost delta — Fetcher is a cache hit, Summarizer+Mailer refire.

**Why now.** It is the moat, demonstrated. It is also the artifact
store's forcing function (content-addressed blobs, `~/.wright/objects`)
and the record/replay cassette's forcing function (rig's cassette idea,
ours by port).

**Acceptance.**
- [ ] `wright fire examples/hn-digest/blueprint.kdl` (headless, fake
      provider, no network) completes and writes run receipts.
- [ ] Editing Summarizer's goal and refiring prints:
      `refired: Summarizer, Mailer (fake $X) · cached: Fetcher ($0.00)`.
- [ ] Changing ONLY guardrails prints `all agents cached, $0.00`.
- [ ] BLAKE3 (public-domain reference port) replaces FNV-1a in
      `core/src/hash.rs`; all hashes change once, deliberately.

---

## W-5 · Executor I/O backends · L (research + design, impl follows)

**Goal.** The reactor story: readiness-driven I/O behind the existing
`Executor` API (epoll / kqueue / IOCP), plus keep-alive pooling for
`wire::http`.

**Why now.** The executor is zero-unsafe and correct for computational
tasks; networked tools need to await real sockets without parking a
thread each. HTTP/2 stays punted (every LLM REST API speaks 1.1).

**Acceptance.**
- [ ] A design note in this file: backend per platform, wake-up path
      from socket readiness → task queue, thread topology (1 reactor
      thread + N workers vs. inline).
- [ ] A working prototype: two concurrent HTTPS requests via the
      executor on Linux (epoll path), no parked threads (assert via
      timing).
- [ ] `shell` tool design sketched (process spawn + sandbox tiers).

---

## W-6 · OS-native TLS · L

**Goal.** The three backends: Security.framework (macOS), SChannel
(Windows), system OpenSSL via `dlopen` (Linux), behind
`runtime::tls::Transport`.

**Why now.** Doctrine line one. Every networked tool and provider
client waits on this; the loud-refusal placeholder keeps us honest
until then.

**Acceptance.**
- [ ] `https://api.<provider>` round-trips on all three platforms
      (Linux in CI; macOS/Windows manually, results recorded here).
- [ ] Certificate verification ON by default; no downgrade path, ever.
- [ ] Zero hand-written crypto; a comment trail to each OS entry point.

---

## W-7 · Replay in anger · M

**Goal.** A long firing (200+ events), scrubbed and forked: snapshot at
any seq, fork at any seq, both byte-identical on re-projection.

**Why now.** The time-travel scrubber is a headline demo and the
debugging story; the event spine API exists and needs stress.

**Acceptance.**
- [ ] A generated 500-event log: snapshot_at(k) re-projects identically
      for every k; fork_at(k) carries provenance for every k.
- [ ] Determinism test: same events → same projected state hash.

---

## W-8 · Tray daemon lifecycle · M

**Goal.** The app parks in the tray, a cron fires while the window is
closed, single-instance lock, auto-start registration.

**Why now.** "The 8am digest needs something alive at 8am" — the tray
trick is the answer and it must be true on all three platforms.

**Acceptance.**
- [ ] Close window → process lives in tray; open window → restores.
- [ ] A scheduled firing fires with the window closed (fake provider).
- [ ] Second instance launches → focuses the first, exits.
- [ ] Clean uninstall removes everything but user projects.

---

## Week-one sequencing

```
Day 1-2   W-1 (wiring)          ──┐
Day 2     W-2 (inventory)        ├─ vieww truth established
Day 3     W-3 (parser hardening) │
Day 3-4   W-4 (cache-hit demo)  ─┴─ the moat, demonstrated
Day 4-5   W-6 (TLS, Linux first) → W-5 design note
Day 5     W-7 + W-8 (replay & daemon)
```

**Definition of done for week one:** `wright fire` works headless with
the fake provider; the refire economics print; the vieww window shows
one card; the spike table above is all checkboxes.

## Risk ledger (honest)

| Risk | Mitigation |
|---|---|
| vieww not consumable externally (W-1 fails) | Vendor + `[patch]` fallback; worst case: fork the needed crates under ADR-002 exception review |
| dsh ships project-level blueprints first | Speed. W-4 demo within the week; their architecture (session-centric, plugin-everything) makes this a rewrite, not a feature |
| FNV-1a collisions in dev artifacts | W-4 replaces with BLAKE3 before any real artifact store ships |
| Executor I/O design stalls | Keep-alive + thread-per-request is an acceptable 0.1 bridge; the reactor is a perf feature, not a correctness one |
| Scope creep in the palette | ADR-003 applies to vocabulary too: new tools argue their way in |
