# Wright

**Describe the outcome. Agents set up the project, build it, check it, and keep it understandable.**

Wright turns a person's intent into a working, repeatable project. You describe
what should exist; Wright drafts a typed **blueprint**, shows you — in plain
words generated from that blueprint, never by a model — exactly what will run,
what each step may touch, what it may cost and how "done" is checked; and only
after you approve does a workforce of agents build it inside a sandbox, test
it, have it reviewed, and deliver it. Change one requirement later and only the
affected work runs again.

> Working name. *Wright* (Old English: maker): you write it, agents wright it,
> the artifact is wrought. See `docs/DECISIONS.md` ADR-008.

## Status: v1.0.1 — trust before reach, recipes at the end — the receipt in the window

| Area | v0.2 | Notes |
|---|---|---|
| Blueprint: parse, validate, canonical form, diff, refire plan | ✅ | 84 core tests; every refire rule pinned |
| Compiler: stages, checks-before-delivery, tier-weighted budgets | ✅ | nothing touches the world before Testwright + Reviewer |
| Agent loop: streaming tool calls, gate, approvals, budget, compaction | ✅ | Anthropic + OpenAI-compatible (OpenAI, DeepSeek, GLM, Kimi, xAI, local) |
| Task cache with freshness rules | ✅ | live-data and delivery steps are never served from cache |
| Durable run store, resume after crash, receipts | ✅ | fsynced event log, BLAKE3 content-addressed objects |
| Tools: files, edit, apply-patch, git, run-tests, shell, http | ✅ | `browser` refuses loudly (not implemented) |
| Sandbox (Landlock + network namespace), Linux | ✅ | macOS/Windows: sandboxed tools refuse (W-5b) |
| TLS: OS OpenSSL via dlopen, Linux; HTTP CONNECT proxies | ✅ | macOS/Windows: through the system curl (ADR-023); native FFI is W-6b |
| Built-in models: Qwen3-1.7B (text, tools), Whisper base (voice), run in-process | ✅ | checked against llama.cpp / whisper.cpp; `docs/MODELS.md` |
| Schedules: cron + IANA time zones (TZif), background daemon | ✅ | systemd / launchd unit installer |
| Interview → Confirm Gate → blueprint | ✅ | the summary is code-generated |
| Universal project convention (python, node, react, rust, shell) | ✅ | every skeleton's own tests pass with no installs |
| vieww window: sessions, swipe, overview, live engine, settings, voice, files | ✅ | driven end to end headlessly; see "The window" |
| Installers: Windows, macOS, Linux | ✅ | `.github/workflows/wright-release.yml` (ADR-020) |
| Secrets: OS keychain via `security` / `secret-tool`, env for CI | ✅ | Windows Credential Manager: not yet |
| Rule of Two: typed legs, taint fixpoint, validator + gate enforcement | ✅ | ADR-024; 3-leg agents need a human approval, and the gate asks anyway |
| Signed receipts + `wright verify` (Ed25519, SHA-512 reference ports) | ✅ | ADR-025; five checks, inputs' hashes, acceptance verdicts |
| Workers: claude-code / codex / kimi-code / zcode steps, sandboxed copy, all-or-nothing | ✅ | ADR-026 |
| MCP behind permits: hash-pinned servers, allow-listed tools | ✅ | ADR-029; `wright mcp-hash` |
| Typed edges: out=/in=/schema= over five readable shapes | ✅ | ADR-027; checked at plan and fire time |
| Recipes gallery + signing, `wright recipe list/sign/verify/install` | ✅ | ADR-028; unsigned installs are refused |
| `wright explain`, `wright handoff`, `wright bench`, cost choice in `wright run` | ✅ | one-screen run story; PR folder; public benchmark |
| Server mode, teams | ❌ | not in the memo's scope for this release |

**380+ tests passing (plus the live-network opt-in set), clippy clean. Twelve failures on kernels before 5.13 are the sandbox failing closed (Landlock needs 5.13+) — the same tests pass on 5.13+; the new trust suite (`engine/tests/trust.rs`) runs everywhere.** Eight of them compare the built-in engine with llama.cpp and whisper.cpp, or run the real models; they need fixtures (`scripts/models/`) and skip without them — CI provides them (`docs/MODELS.md`). The headless binary has **zero**
external crates; the `ui` feature adds exactly one dependency — vieww, from
the repository Wright lives in, by path (ADR-002, ADR-011).

## Quick start (no key needed)

```bash
cargo build --release                       # headless CLI (no external crates)
cargo build --release --features ui         # + the vieww shell

./target/release/wright doctor              # what works on this machine
./target/release/wright model pull          # the built-in models (1.9 GB, once;
                                            # otherwise they download on first use)
./target/release/wright model ask "Plan a tiny CLI that counts words."
```

### The trust tour (all offline, all verifiable)

```bash
# What a plan may touch, in words — the Rule of Two included:
./target/release/wright plan myproject

# Fire it, choose what you pay (the estimate comes first):
./target/release/wright run myproject

# Prove what ran — five checks over the signed receipt:
./target/release/wright verify myproject

# The one-screen story of the run:
./target/release/wright explain myproject

# Hand the project to a developer as a PR folder:
./target/release/wright handoff myproject

# Time to a checked outcome, interventions per run:
./target/release/wright bench myproject --runs 3

# The recipe gallery — nothing installs unsigned:
./target/release/wright recipe list
```

With no key set up, projects run on the built-in `qwen3-1.7b` (and say
so). For a fully offline demo with scripted answers instead:

```bash
# Scripted answers (the mock provider), for demos and CI:
./target/release/wright mock-provider --port 7878 &
mkdir -p ~/.wright && echo 'provider "mock" base="http://127.0.0.1:7878/v1" models="*"
interview model="mock-1"' > ~/.wright/config.kdl

mkdir demo && cd demo
../target/release/wright new .              # describe it; confirm the summary
../target/release/wright plan .             # what will happen, in plain words
../target/release/wright run . --yes        # fire it (watch the event river)
../target/release/wright run . --yes        # again: unchanged work is cached
../target/release/wright ui .               # the same project, in the vieww shell
../target/release/wright ui . --snapshot shots/   # every screen as PNG, headless
```

With cloud models: `wright secret set keychain:wright/anthropic` (or
`deepseek`, `openai`, `zai`, `moonshot`, `xai`) and remove the mock provider
from the config. On CI and servers, set `WRIGHT_SECRET_WRIGHT_ANTHROPIC`
instead (`wright secret env <ref>` prints the variable name).

## The window

```
 wright        HN Daily Digest  ● ━ ○        All sessions  Engine  ⚙  ?
 ┌─────────────────────────────────────────────────────────────────────┐
 │  [Home] [HN Daily Digest] [Word Count CLI] [new]  <- two fingers    │
 │                                                                     │
 │   your request                                         (bubble)     │
 │   Run 3 · running      ✓ Collect  ◌ Summarize  ○ Check  ○ Send      │
 │   Mailer is asking before it touches the world   [Allow] [Don't]    │
 │   + │ Ask for a change…                                  mic  send  │
 └─────────────────────────────────────────────────────────────────────┘
 ▌wright  HN Daily Digest  ● Running · Summarizer working   $0.004  Dark
```

| Do | How |
|---|---|
| Next / previous session | two fingers sideways (or ← →) |
| New session | swipe past the last one, or type on Home |
| Every session at once | **All sessions** (or `O`); × closes into Saved |
| Reopen a closed session | Undo, the Saved shelf, or Ctrl/Cmd+Shift+T |
| See the agents and tools, live | hold **Space** to peek; `E` keeps them |
| Approve | the buttons, or drag the card right (allow) / left (refuse) |
| Attach files | `+` in the composer, or drop them on the window |
| Speak | the mic — transcribed on this machine by the built-in Whisper (or any speech model you set) |
| Settings | ⚙, or Ctrl/Cmd+, — models & keys, defaults, appearance, workspace, project |
| Export | **Export** on the result: the source as a .zip, or the package (wheel, crate, web build) once a run has passed its checks |
| Prove it | **See the receipt** on the result: `wright verify`'s six checks in their own words, the verdict, the report to copy — the same store the CLI reads |

## The commands

```
wright new [dir]            describe a project; confirm; get a blueprint
wright plan [dir]           what will happen, in plain words, and what an edit refires
wright run [dir]            fire it   (--yes  --headless  --no-cache  --quiet)
wright resume <run> [dir]   continue a run that crashed or was stopped
wright runs [dir]           run history
wright replay <run> [dir]   reconstruct a past run from its log (--at N) — no re-execution
wright approve [dir]        list / answer pending approvals (any terminal, any time)
wright diff [old] [new]     what an edit refires, and why
wright export [dir]         the project as a .zip, plus its package: a Python wheel,
                            a Rust crate or a web build (--zip --wheel --crate --web)
wright schedule ...         list | remove [dir] | install (login service)
wright daemon               fire schedules in the background
wright secret set <ref>     put a key in the OS keychain
wright model ...            the built-in models: list | pull [id] | rm <id> | ask | listen
wright where [dir]          where the project's history lives
wright doctor               what works on this machine
wright ui [dir]             the window (--snapshot <dir>: all 18 screens as PNG, receipt and light mode included;
                            --icon <png> [--size N] [--padded]: the app icon)
```

## What a firing looks like

```
stage 1  Shipwright (built-in)   lays the universal skeleton — no model, milliseconds
stage 2  Logwright               logging, inside that skeleton            (chip: logging)
stage 3  Fetcher                 live data: always fresh                  (your agent)
stage 4  Summarizer              cached if its input did not change       (your agent)
stage 5  Testwright              writes + runs checks                     (chip: tests)
stage 6  Reviewer                APPROVE or REJECT — rejection stops delivery
stage 7  Mailer                  touches the world: asks first, never cached, never retried
stage 8  Operator (built-in)     registers the schedule; the receipt is written
```

Edit a requirement and `wright diff` says, before anything runs:

```
This change refires Summarizer + Mailer + Testwright + Reviewer.
Fetcher is fresh every run anyway (live data or delivery).
```

## Repo map

```
wright/
├── core/      blueprint, compiler, confirm summary, convention, schedule, events,
│              wire (JSON / HTTP / SSE), BLAKE3 — pure, std-only
├── runtime/   executor, permission gate + permits, cache keys, durable store,
│              net + TLS (+ system curl) + proxies, model providers, built-in
│              model downloads, config, secrets, time zones
├── tools/     tool implementations, path guard, Landlock sandbox, patch engine
├── agents/    the system workforce: prompts as contracts, verdict parsing
├── engine/    agent loop, firing, interview, scheduler, approvals, mock provider
├── infer/     the built-in models: GGUF, tokenizer, Qwen3, Whisper — std-only
├── app/       the binary: CLI + (feature `ui`) the vieww shell
├── examples/  the HN Daily Digest blueprint
└── docs/      ARCHITECTURE, BLUEPRINT-FORMAT, PROJECT-CONVENTION, DECISIONS, SPIKES, MODELS
```

## House rules

See `AGENTS.md`. Short version: one dependency (the vieww repo, UI only),
std-only core, unsafe only where the OS must be called (TLS, sandbox), every
module readable by a sleepy engineer at 3AM, every prompt a contract, every
bug fix ships with the test that would have caught it, secrets are references.
