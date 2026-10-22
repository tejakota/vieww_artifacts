# Decision Register (ADRs)

Every load-bearing decision, written down so it survives us. Each ADR
states the decision, the reasoning, and what it would take to reverse.
Statuses: Accepted (binding), Proposed (leaning), Open (honest doubt).

---

## ADR-001 — The user never writes code · Accepted

**Decision.** Wright's interface is intent (conversation, cards, chips,
sliders, toggles). Code is the product, never the interface.

**Why.** Every competitor moved the setup hurdle rather than removing it:
node-wiring (Dify/n8n), config files (dsh, Claude Code), terminal flags
(Codex, Kimi). The empty quadrant is UI-first + zero-user-code +
project-producing. Removing the hurdle — not automating it — is the product.

**Reversal.** Product death. If users demand an escape hatch to raw code,
Inspector Mode already provides read-only visibility; the fix is better
REFINE, not user code.

**v0.2 note.** A reframing is proposed in ADR-017 ("no code required, code
allowed"); until it is accepted, this ADR stands as written.

---

## ADR-002 — One dependency (vieww), everything else in-house · Accepted

**Decision.** Exactly one direct external dependency: vieww. All other
capability is built from scratch, mimicking proven designs:
tokio's API shape (executor), rig's unified provider surface (models),
reqwest's minimal client + HTTP/1.1 (wire), serde_json's value model
(json), git's object store (artifacts, W-4), Bazel's action cache
(task cache), KDL's grammar (blueprint dialect).

**Why.** Supply-chain trust after OpenClaw's 9 CVEs in 4 days is a
feature, not asceticism. "Direct dependencies: 1. Lines of ours: all of
them." vieww's transitive tree is pinned and audited once (W-1), then
frozen.

**Exception (the only one).** TLS: OS-native backends
(Security.framework / SChannel / system OpenSSL via `dlopen`). Binding
the platform we already ship on is not a dependency; hand-rolled crypto
is an incident report wearing a feature costume.

**Reversal.** An ADR arguing a specific crate replaces a specific
in-house module, with the security math shown.

---

## ADR-003 — An app, not a crate set · Accepted

**Decision.** Wright ships as one installable binary (.dmg / .msi /
.AppImage), new repo, own version train. The internal layout is five
crates: `app`, `core`, `runtime`, `agents`, `tools`. New crates must
argue their way into existence against the 3AM rule.

**Why.** The promise is "install one thing and describe what you want."
An app is a product; crates are parts. vieww becomes a dependency, not
a host — and gets its proof-of-framework story for free.

**v0.2 addendum.** A sixth crate, `engine` (agent loop, firing, interview,
scheduler), argued its way in: `tools` must depend on `runtime` (tools
demand the gate's `Permit` type) and the agent loop must call tools, so the
loop cannot live in `runtime` without a cycle.

**Reversal.** Unthinkable before 1.0.

---

## ADR-004 — Blueprint: KDL-dialect, one file, canonical form · Accepted

**Decision.** Projects are one `blueprint.kdl` file: typed, validated at
the Confirm Gate, canonically serialized (formatting changes do not
change the hash; the hash IS the commit). Structural diffs compile to
refire plans. Recipes are signed, parameterized blueprints.

**Why.** The industry converged on config files as the interface
(AGENTS.md, SKILL.md, subagent YAML) — but untyped, unversioned,
undiffable prose. We make the same idea a typed database with a UI on
top and a workforce underneath.

**Reversal.** Format version `meta.format` exists for that purpose.

---

## ADR-005 — Guardrails never invalidate work · Accepted

**Decision.** Compile-time values (budgets, wall clock, max turns,
schedules, timezone) are excluded from cache keys and refire plans.
Changing them must cost $0.00 in re-runs.

**Why.** The moat is incremental refire. A budget slider that triggers a
$0.50 re-run is a bug in the moat. Pinned by tests in
`core/src/blueprint/diff.rs`.

**Reversal.** Never.

---

## ADR-006 — Permissions are provenance (cache keys include them) · Accepted

**Decision.** An agent's permission set is part of its task cache key.

**Why.** An artifact produced under broader powers is a different
artifact. Without this, the cache would launder provenance — an
OpenClaw-class security bug in ours. Pinned by
`permission_change_invalidates_provenance`.

**Reversal.** Never.

---

## ADR-007 — Interop: export, never import · Accepted

**Decision.** Wrighted projects emit AGENTS.md and SKILL.md so Claude
Code, Codex, Kimi Code, and dsh can work ON wrighted projects. MCP is a
tool source (protocol implementation, not a dependency). We never accept
prose config as truth.

**Why.** Wright is the source of truth; other harnesses are tools at the
edge. Importing prose would import ambiguity.

**Reversal.** An importer may exist someday, but it goes through the
Interviewer (i.e., it becomes a blueprint via confirmation), never the
file system.

---

## ADR-008 — Name: "Wright" (working) · Proposed

**Decision.** Wright — the write/wright/wrought pun encodes the zero-code
promise in the name. System agents are wrights of trades (Shipwright,
Testwright, Logwright, Firewright). Tagline: "Describe it. It's wrought."

**Why.** A name that generates vocabulary (Kubernetes/pods) beats a label.
Kiln is the strong backup (a run is a "firing", a re-run a "refire" —
already the words we use); Heph is the wildcard.

**Outstanding.** Domain + trademark class-9 pass. Research shows clear
water in the agent-tooling category; verify formally before committing.

---

## ADR-009 — Local-first, single user, v1 · Accepted

**Decision.** v1 is a local app with a background daemon. No accounts, no
cloud, no telemetry by default. The store is under `~/.wright` (per project:
`~/.wright/projects/<name>-<id>/`, see ADR-013).

**Why.** "Extremely fast" and "simplest setup" are both lies if the
first screen is a sign-up. Trust is won by not phoning home.

**Reversal.** Multi-user tenancy is a v2+ question (see Open Questions).

---

## ADR-010 — Model tiers, not model names, in blueprints · Accepted

**Decision.** Agents reference tiers (`cheap` / `quality` / `auto`);
the tier table maps to preferred + fallback model ids. Swap vendors by
editing strings.

**Why.** Models are commodities that rename quarterly. Blueprints are
forever artifacts; they must not rot when a model deprecates. Tier
changes invalidate the right agents (and only those) via the diff
engine.

**Reversal.** Advanced users may pin exact models in Inspector Mode
(v2 idea); tiers remain the default surface.

---

## ADR-011 — "One dependency" means one repository at one pinned commit · Accepted

**Decision.** The one dependency is the vieww repository
(`github.com/tejakota/vieww_artifacts`) at a single pinned `rev`. Wright may
use any crate *of that repository*; in v0.2 it uses two: `vieww` (the
umbrella, with its `native` renderer) and `vieww-platform-winit` (the
window). Only the `app` crate's `ui` feature pulls it in; the headless
binary has zero external crates.

**Why.** Spike W-1 found that the umbrella crate deliberately contains no
window — the platform layer is a separate crate of the same workspace. Read
literally as "one crate", the doctrine would have been false the day the UI
landed. One repository at one commit is the reviewable unit (one audit, one
upgrade decision), which is what the doctrine was protecting.

**Consequences.** MSRV is vieww's: 1.89. vieww's own transitive crates
(cosmic-text, winit, accesskit, image…) come with it; the audit artifact is
`cargo tree -e normal -p wright --features ui` at the pinned rev. The git
dependency clones a large repository once (films and plates live there);
splitting `vieww_base/` into its own repository would make this cheap.

**Reversal.** Changing the pinned rev is a reviewed commit; adding a crate
from outside that repository needs its own ADR.

---

## ADR-012 — The crypto boundary · Accepted

**Decision.** Crypto is never written by hand. TLS: the OS library
(OpenSSL via `dlopen` on Linux — done; Security.framework and SChannel —
W-6b). Hashing: the public-domain BLAKE3 reference port, pinned to the
official test vectors. Signatures and MACs (recipe signing with Ed25519,
SSO tokens, webhook HMAC): **only through OS APIs**, per platform, and the
feature that needs one does not ship on a platform until its API is wired.

**Why.** Rule 3 forbids hand-rolled crypto, and several planned features
need more than TLS. Writing that down now prevents someone "just porting"
an Ed25519 implementation in v0.3.

**Reversal.** An ADR naming a specific reviewed reference implementation
for a specific primitive, BLAKE3-style, with vectors.

*(Exercised, 2026-10-22: ADR-025 names Ed25519 — RFC 8032 reference port,
`core/src/ed25519.rs` — and SHA-512 — FIPS 180-4, `core/src/sha512.rs` — for
receipt and recipe signing, pinned to the RFC's and NIST's vectors plus
OpenSSL cross-vectors computed on the build machine.)*

---

## ADR-013 — Wright's state lives outside the project · Accepted

**Decision.** Run logs, cache, objects, blueprint history and approvals live
in `~/.wright/projects/<folder-name>-<hash of path>/`, not in
`<project>/.wright/`.

**Why.** Agents may run sandboxed shell commands that can write anywhere
inside the project, and Landlock cannot make a sub-tree of a writable tree
read-only. Inside the project, an agent could forge a cache entry or
rewrite the event log. Outside, the sandbox makes it invisible. Objects are
additionally verified against their hash on every read.

---

## ADR-014 — The Confirm Gate is written by code · Accepted

**Decision.** The plain-language summary a person approves is generated by
`core/src/confirm.rs` from the typed blueprint. The Interviewer proposes a
blueprint through a tool; Wright validates it; the model never describes
permissions, costs or schedules in its own words.

**Why.** The one text that authorizes everything must be true by
construction. A model that writes "the Mailer won't send anything without
asking" could be wrong; a line generated from the `approval` field cannot.

---

## ADR-015 — Freshness and delivery semantics · Accepted

**Decision.** Every tool declares what it reads (`nothing | workspace |
outside`) and what it changes (`none | workspace | world`). Outside-readers
make an agent volatile (never cached); world-effects make it a delivery
agent (never cached, never retried, scheduled after the Reviewer, skipped
on rejection or test failure). An approval from the cache never outlives a
new test failure.

**Why.** v0.1 cached everything with an unchanged key, so a scheduled
digest would have replayed yesterday's stories and never sent the email —
and ran its tests after the email was sent. Bazel's rule is hermetic
actions; agents are not hermetic, so the palette must say which are not.

---

## ADR-016 — Replay is not re-execution · Accepted

**Decision.** Two guarantees, named separately. **Replay** (`wright
replay`, the UI's scrubber) reconstructs a past run's visible state from
its event log; it executes nothing and sends nothing. **Re-execution**
(`wright run`, `wright resume`) runs work again under recorded or changed
conditions; Wright never promises a model will produce the same output
twice.

**Why.** "Deterministic replay" was being read as "deterministic
re-execution", which LLMs cannot offer. The event spine makes replay exact;
the cache makes re-execution cheap; neither makes models deterministic.

---

## ADR-017 — "No code required, code allowed" · Proposed

**Proposal.** Reframe ADR-001: people never *have to* write code or harness
configuration to get started, but code and `blueprint.kdl` stay
inspectable, editable and exportable. The CLI is a first-class door for
developers, not an escape hatch.

**Why.** The audience runs from core developers to businesses. Developers
will edit the blueprint in their editor and diff it in git — the canonical
form and `wright plan` already make that safe. Business users never see it.

**Status.** Proposed; v0.2 already behaves this way (hand edits are
validated, not forbidden).

---

## ADR-018 — The UI has three levels, plan before machinery · Accepted

**Decision.** Five screens on one engine: Describe → Plan → Run → Result,
and Inspect. The Plan screen is the Confirm Gate with stages, not a
mandatory graph canvas; built-in helpers are labelled, not configured;
spend, status and approvals are always visible; tokens, events and hashes
live in Inspect. The Result screen shows artifacts, test evidence and the
verdict — a model saying "done" is never the proof.

**Why.** From the v0.2 product review: a graph of nine system agents is a
great explanation for an engineer and an obstacle for everyone else.

## ADR-019 — One window, many sessions, moved with gestures · Accepted

Supersedes the five-screen rail of ADR-018 (its rules stand: plan before
machinery, proof on the result, engineering on demand).

**Decision.** The window is a row of full pages: Home, one page per
session (a session IS a project folder), and a new-session page at the
end. Two fingers sideways move along the row; swiping past the last page
starts a new session; the overview (button or `O`) shows every session as
a card with × to close it into **Saved** (closing never stops schedules or
deletes anything; Undo, the Saved shelf and Ctrl/Cmd+Shift+T bring it
back). The agents and tools are a layer *under* the conversation: hidden,
peeked with Space, kept with E. Settings holds models and keys (keys to
the OS keychain), defaults, appearance (any accent colour; every accent
role is derived from it), the workspace, and the project's own limits
(written to its blueprint, so they are versioned like any edit). The
composer takes files (`+`, or drop on the window) and voice (the mic,
transcribed by an OpenAI-compatible `voice model`).

**Why.** The v0.3 brief: "flowy, gesturey, live", for people who never
want to see a graph unless they ask.

**Two vieww facts this design works around** (reported upstream, not
patched here):
1. *Pinch is not delivered.* winit's touchpad pinch is not forwarded by
   `vieww-platform-winit`, and `ScrollEvent` has no modifiers (so
   Ctrl+wheel cannot stand in). The overview is a button and a key until
   it is.
2. *Layers ignore a parent's scale.* A scroll view is a repaint boundary
   with its own layer, and a `Transformed` ancestor's scale is not applied
   to it — a shrunk page draws its scrolling part full size. The overview
   therefore draws miniature cards instead of scaled live pages.

## ADR-020 — Installers are built by CI from scripts anyone can run · Accepted

**Decision.** `.github/workflows/wright-release.yml` builds Windows (Inno
Setup, per-user), macOS (universal `.app` in a `.dmg`, ad-hoc signed) and
Linux (`.deb` + `.tar.gz`) from `packaging/`, smoke-tests every binary on
its own OS (version, plan, headless screens), and publishes a release
only when asked. The app icon is rendered by the app (`wright ui --icon`)
from the same painter as the in-app mark. The shipped binary is built with
`--profile dist` (LTO, stripped) and explicit RUSTFLAGS, which replace
vieww_base's `prefer-dynamic` development setting.

**Not yet.** Code signing and notarization (they need certificates held as
secrets), and TLS/sandbox on macOS and Windows (W-5b, W-6b) — the release
notes say so in a table, per platform.

## ADR-021 — Export: a copy always, a package only with proof · Accepted

**Decision.** `wright export` (and Export on the result card) makes the
project's source as a zip, plus the package its language makes: a Python
wheel, a Rust crate, or a built React app. Packages require the last run
to have passed its checks and the Reviewer's approval; the source zip does
not (a copy is not a claim). Exports go to `Exports/<project>` beside the
project, never inside it, so exporting changes no cache key and the next
export does not include the last. The zip and the wheel are written by
Wright (std only: `core::zip`, `core::sha256`) with fixed timestamps, so
the same project exports the same bytes; the crate and the web build run
`cargo package` / `npm run build` on the user's machine, outside the
sandbox, because a person asked for them.

**React** joins the convention with rule 1 intact: logic in plain
modules tested by `node --test` with nothing installed; JSX and Vite only
for running and building.

## ADR-022 — Built-in models, run by Wright itself · Accepted

**Decision.** Wright runs two small open models in-process — Qwen3-1.7B
(8-bit) for text and tool calls, Whisper base for speech — with its own
std-only engine (`infer/`), not a bundled C++ runtime or a side server.
The files are not in the installer: they download on first use into
`~/.wright/models`, resumably, and are refused unless they match the
SHA-256 pinned in `runtime/src/local.rs`. In config they are an ordinary
provider of kind `builtin`, last in the list; voice uses Whisper by
default, and a model with no provider or no key falls back to Qwen3 with
a line in the event river (`builtin fallback=false` turns that off).

**Why.** The first run should not need an account, a key or a credit
card, and voice should not leave the machine. In-process keeps the
one-dependency rule (ADR-002) and the zero-external-crates headless build;
a 1.8 GB runtime download beats a 1.8 GB installer. Qwen3-1.7B is the
smallest open model that calls tools reliably in its own chat format;
Whisper base is the smallest multilingual Whisper that transcribes
instructions well.

**How we know it is right.** The engine is checked against llama.cpp and
whisper.cpp — tokens, logits, language guesses — on random models in the
real formats in every CI run, and on the real weights on all three
platforms (`docs/MODELS.md`).

**Boundaries.** The runtime does not depend on `infer`: it defines a
`LocalBackend` trait and the app installs the engine at startup. Content
is never encoded with control tokens (a tool result cannot open a turn).
Kernels are safe Rust; hand-written SIMD would need `unsafe` and is an
open question below.

## ADR-023 — HTTPS on macOS and Windows through the system curl · Accepted

**Decision.** Where Wright has no native TLS binding yet (macOS, Windows),
`net.rs` hands `https://` requests to the operating system's own curl
(`/usr/bin/curl`, `%SystemRoot%\System32\curl.exe`), which is built on the
platform's TLS stack and trust store. The whole request, keys included,
goes in as a config file on **stdin** — the command line is only
`-q --config -` — and the raw response (`--include --raw`) comes back
through the same head parser and body reader as a direct connection.
Wright decides proxying (its `NO_PROXY` rules) and passes it explicitly;
`~/.curlrc` and curl's own proxy environment are ignored. Linux keeps the
OpenSSL binding; `WRIGHT_TLS=curl|native` forces either.

**Why.** Cloud providers and model downloads on two of three platforms,
now, without hand-rolled crypto (rule 3), new `unsafe` (rule 2), or a
crate (rule 1). The Security.framework / SChannel FFI (W-6b) remains the
plan; curl is the bridge, not the destination.

---

## Open questions (honest ledger)

1. **Tenancy.** Shared blueprints in v1.5 (LAN/URL share) or hold until
   v2 cloud? Lean: hold — local-first is the trust story.
2. **Recipe governance.** ClawHub's 44K skills in 3 months proves the
   market and the risk. Curated+signed at launch; unsigned community
   recipes with warning badges after. Exact curation bar: Open.
3. **Recursive agents.** May project agents spawn sub-agents? v1 says
   no — the blueprint is the only place structure is declared. Revisit
   with budget caps once the gate proves itself.
6. **Typed edge bindings.** Today an edge orders work and the validator
   proves every `in` has a connected producer. Binding a named output to a
   named input with a schema (and failing early on mismatch) is the
   leading v0.3 format change (raised in the v0.2 review).
4. **Multi-deliverable blueprints.** Multiple `deliverable` nodes are
   legal in the format; the UI for several definitions of done is
   undesigned.
5. **Agent-authored tools.** v1 palette is closed. Tool-packs as signed
   artifacts referenced by hash is the leading v2 design.
7. **SIMD for the built-in engine.** Hand-written AVX2/AVX-512/NEON
   kernels would make prompts ~3x faster (llama.cpp's margin today) but
   need `unsafe` loads. Allow one small, test-pinned `infer/src/simd.rs`,
   or wait for safe SIMD in stable Rust? (`docs/MODELS.md`, "Speed".)

## ADR-024 — The Rule of Two, enforced by the validator and the gate · Accepted

Date: 2026-10-22 · Status: Accepted · Supersedes: none

The memo's bet #1, made checkable. An agent may not hold private data,
untrusted input and an outbound channel at once unless a human approved
that combination. Every tool on the palette carries its *legs*:
`PrivateData` (it can read the person's own files), `UntrustedInput`
(its output depends on the outside world), `OutboundChannel` (it can
send something out).

- **Legs are typed on the palette**, not inferred from prose:
  `files-read` stands on the private leg, `http-read` on untrusted,
  `http-post` on outbound, `shell` on private+untrusted, and the new
  `worker` and `mcp-call` tools say theirs (ADR-026/029).
- **Taint is a fixpoint** (the memo's bet #4): an artifact is untrusted
  when any producer on the untrusted leg made it; anything that reads
  it inherits the leg, so the mark follows the data through the graph,
  not the tool that touched it last. `core/src/rule_of_two.rs` iterates
  to a fixed point (bounded; validation refuses cycles first).
- **The validator refuses** a 3-leg agent without `approval
  "before-send"` on its card, in one sentence a person can read.
- **The gate asks again** before every outbound step of a 3-leg agent,
  even when the card promised approval — the promise names the
  combination, the ask names the actual send. Belt and braces, because
  a blueprint is compiled, not trusted.
- Artifacts an untrusted-touched task writes say so in the event spine.

The hn-digest's split (fetch, then send) never trips: the Fetcher holds
untrusted, the Mailer outbound + inherited-untrusted — two legs each,
and the Mailer already promised to ask before sending.

## ADR-025 — Signed receipts, acceptance sentences, and `wright verify` · Accepted

Date: 2026-10-22 · Status: Accepted · Supersedes: none

The memo's moves #1 and #2. A receipt is only worth something if a
third party can check it, so receipts are now `wright.run-receipt/2`:

- **Every input the plan consumed is recorded with its BLAKE3** (stored
  in the object store), so a verifier can name what the work was based on.
- **Every numbered acceptance sentence is recorded with its verdict** —
  the promises a person approved, kept or broken, in the receipt.
- **The Rule of Two agents are named**, so the risky ones are visible.
- **The whole receipt is signed with Ed25519** over its exact JSON bytes
  (the writer is insertion-ordered, so bytes reproduce), with a
  reference port of RFC 8032 in `core/src/ed25519.rs` — no crates, no
  unsafe, pinned byte-for-byte to the RFC's vectors and to three
  OpenSSL-generated pairs computed on this machine. The signing key
  lives in the OS keychain (`keychain:wright/receipt-signing`), with an
  env override for CI and an honest local-generated fallback whose
  provenance the receipt states. SHA-512 (FIPS 180-4) is ported the
  same way, constants computed from their definitions and pinned to
  OpenSSL's digests.
- **`wright verify` runs five checks** and prints them in words:
  signature, event log hash, artifacts vs the store and the folder,
  inputs, blueprint commit. v1 receipts are reported as pre-signature,
  never silently passed.

Acceptance sentences: a deliverable may carry `accept "..."` lines —
promises the person reads in the Confirm Gate, Testwright must check
(`ACCEPT <n>: PASS|FAIL` per numbered promise, parsed strictly by
`wright_agents::parse_accepts`), and the Reviewer may not approve past
a broken one — enforced in the engine, not in the prompt.

## ADR-026 — Other agents as workers · Accepted

Date: 2026-10-22 · Status: Accepted · Supersedes: none

The memo's move #3 and bet #2. A blueprint hands a step to an external
coding agent — Claude Code, Codex, Kimi Code, ZCode, anything the
machine configures — with Wright as the manager:

```kdl
agent Coder { goal "write the tool" worker "claude-code"
             worker-files "src/" worker-timeout 600 out "done.txt" }
```

- **Workers are config, not blueprints**: `worker "claude-code"
  command="claude" args="--print" timeout=900` in config.kdl; the four
  the memo names ship as defaults.
- **A copy, never the real project**: the tool clones the workspace to
  a throwaway temp dir, writes a `WORKER.md` brief, runs the agent with
  the prompt as its last argument, and diffs the copy back.
- **All or nothing**: when the diff steps outside `worker-files`,
  NOTHING is applied; partial takeovers are refused in words.
- **No keys**: the run gets a scrubbed environment; on Linux with
  Landlock it also gets the kernel sandbox `shell` gets, and where the
  kernel has no Landlock the run says so and proceeds with the copy +
  scrubbed env + the card's clock.
- **No model budget**: the engine runs the step through the gate
  (a `worker` tool call, permit and all), never calls a model; the
  external agent's own account pays for its tokens.
- The worker's output goes through the same Testwright and Reviewer as
  everything else, and worker tasks are never cached — an external
  agent's answer is not a function of the workspace bytes.

## ADR-027 — Typed edges · Accepted

Date: 2026-10-22 · Status: Accepted · Supersedes: none

The memo's bet #6. An edge may name its artifact and promise a shape:

```kdl
edge from="Fetcher" to="Summarizer" out="stories.json" in="stories.json" schema="json-array"
```

Five shapes, deliberately readable: `any`, `text`, `json-object`,
`json-array`, `csv` — checked over the actual bytes in one pass. Names
are validated at plan time (the binding must match the cards); bytes
are checked at fire time, before the consumer runs, and a broken
promise fails the plan with both sides named. One file, one shape;
unions and regexes would be the start of a type system, and this is a
contract you can say out loud.

## ADR-028 — Recipes are curated and signed, and nothing installs unsigned · Accepted

Date: 2026-10-22 · Status: Accepted · Supersedes: none

The memo's move #4. A recipe is one KDL file: `recipe "id" { version,
publisher, description, blueprint, publisher-key }` plus a `signature`
line. The signature (Ed25519, the same reference port as receipts)
covers the recipe's canonical bytes — the meaning, not the layout — so
a publisher can reformat the file without re-signing while an edited
blueprint fails verification. `wright recipe sign` signs with this
machine's publishing key; `install` verifies FIRST and refuses an
unsigned or tampered recipe with the sentence that quotes this ADR; the
gallery ships unsigned *sources* by design, because the gallery's own
rule is that nothing installs without a signature. The README in
`recipes/` says exactly that.

## ADR-029 — MCP behind permits · Accepted

Date: 2026-10-22 · Status: Accepted · Supersedes: none

The memo's bet #3. MCP servers are other people's programs, so they run
inside the same law as everything else:

```kdl
# config.kdl — the pin is BLAKE3 of the executable's bytes
mcp "weather" command="uvx" args="mcp-server-weather,--local" hash="blake3:..." timeout=60
```

```kdl
# the card allow-lists tools; anything else does not exist
agent Looker { goal "weather" mcp server="weather" allow="get_forecast"
               approval "before-send" out "w.txt" }
```

- **Pinned by hash**: before the first spawn, the executable is hashed
  and compared with the pin; a server that changed since the pin was
  approved is refused, loudly. `wright mcp-hash <command>` prints the
  pin to paste. Config refuses an unpinned server at parse time.
- **The card is the law**: the server name must match, the tool must be
  allow-listed, and the server's own tools/list must offer it — both
  sides of the contract must agree.
- **The wire is std-only**: JSON-RPC 2.0, one message per line, over the
  server's stdio (MCP's stdio transport, protocol 2024-11-05). A fresh
  process per call; a watchdog thread owns the clock; teardown kills.
- **Answers are outside data**: the palette marks `mcp-call` untrusted
  and outbound, so taint tracking and the Rule of Two apply — an MCP
  agent reading your files without a human approval is refused at plan
  time (ADR-024).

## ADR-030 — A public benchmark: time to a checked outcome · Accepted

Date: 2026-10-22 · Status: Accepted · Supersedes: none

The memo's bet #8. `wright bench [dir] [--runs N]` fires the project
through the real engine and measures the two numbers that matter to a
person who does not write code:

- **Time to a checked outcome**: from `RunStarted` to a receipt whose
  verdict is `approve` — the OUTCOME CHECKED, not "the model answered".
- **Human interventions**: every `ApprovalRequested` the run raised;
  zero is the number an unattended run is allowed to have.

Two rules keep it honest: the reported median is the pessimistic (lower)
one, and every measured run's receipt stays in the store for
`wright verify` — a benchmark number nobody can audit is marketing.
The release gate is the same numbers over the recipe gallery, recorded
in docs/BENCHMARK.md on a machine that can run the sandbox fully.
