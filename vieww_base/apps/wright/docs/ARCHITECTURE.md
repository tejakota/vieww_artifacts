# Wright — Architecture & Positioning

*Working draft, updated for v0.2 · October 2026 · this file is the
source of truth (the PDF is a v0.1 snapshot).*

> **v0.2 in one paragraph.** The machine described here now runs end to
> end on Linux: interview → code-generated Confirm Gate → blueprint →
> stages (checks before delivery) → agent loop with streaming tool calls
> through the permission gate → Landlock-sandboxed tools → fsynced event
> spine → content-addressed cache with freshness rules → receipt; plus
> resume, replay, schedules and the vieww shell. Decisions that changed
> on the way are ADR-011 … ADR-018. Where this document and the code
> disagree, the code and `docs/BLUEPRINT-FORMAT.md` win.

---

> **Platform note (October 2026 follow-up):** Linux commands use Landlock and
> network namespaces. Windows POSIX shell/Git/test commands can use WSL 2 +
> Bubblewrap when its namespace probe passes; otherwise they fail closed.
> Windows-native worker CLIs run only against a throwaway workspace copy and
> are reported as not OS-sandboxed. macOS remains fail-closed until a native
> sandbox is implemented. See the Windows setup section in `README.md`.

## 1. The thesis

Wright is an agent harness where the user never writes code. The UI is
not a workflow editor (where you still assemble like a programmer) — it
is a factory floor. You say what you want. Agents build it. You watch,
steer, and approve. Code exists as the product, never the interface.

Three layers, and the user only ever touches the first two:

| Layer | Who touches it | What it is |
|---|---|---|
| What you SAY | User | Intent — plain language, forms, toggles |
| What you SEE | User | Blueprint, live runs, artifacts — visual truth |
| What agents DO | Agents | All code, all files, all wiring — invisible by default, inspectable on demand |

The folder structure, logging setup, test framework, CI, scheduling —
the "project setup hurdle" — is not automated. It is **hidden**, the way
a compiler's register allocation is hidden: an implementation detail.

The whole system is one idea taken seriously: **a compiler where the
source language is conversation and the target is a running project.**

```
   INTENT          BLUEPRINT         TASK GRAPH        RUN            ARTIFACTS
 (conversation)  (the IR, versioned)  (the DAG)     (the execution)  (the project)
      │                │                 │                │               │
  Interviewer  →   The Blueprint  →   The Compiler  →   The Runtime  →   The Store
                    (source of truth)  (planner)      (agents work)    (deliverable)
```

Every arrow is inspectable in the UI and reversible: refine = edit
upstream, recompile downstream only. That last property — incremental
rebuild — is the moat (§7).

## 2. The landscape (October 2026)

Five categories of competitor; none occupies our quadrant.

**Code-first frameworks** (LangGraph + DeepAgents, Microsoft Agent
Framework, CrewAI, OpenAI Agents SDK, Google ADK 2.0, AWS Strands,
Pydantic AI, Mastra). State machines, actor graphs, typed pipelines —
powerful, Python/TS-bound, and used by only ~18% of orgs running LLM
apps because teams experience them as overhead. None ships a real UI.

**Visual builders** (n8n ~207K stars, Dify ~158K, Langflow; **Flowise
archived August 2026**). Node-wiring for integrations and chatbots.
None scaffolds a real software project; the lane "UI for project setup"
is empty, and Flowise's death left a hole.

**Coding-agent harnesses** (Claude Code, Codex CLI, Kimi Code, OpenCode
~208K stars, OpenClaw ~346-389K stars). Terminal-first, session-shaped.
OpenClaw's 40,214 exposed instances and 9 CVEs in 4 days set the trust
bar we must clear; its 44K-skill ClawHub in three months set the
ecosystem bar we must match with recipes.

**DeepSeek Harness (dsh, 255K stars)** — the closest competitor and the
category validator. Desktop + web UI, "everything is a plugin" (Cordis),
Creator mode (build plugins by chat), scheduled tasks, can even wire
Claude Code and Codex as sub-agents. What it does not have: a versioned
project definition (it is a workspace of sessions, not a factory of
projects), deterministic replay, structural diffs, or token economics —
reviewers measured ~20M tokens and 35 minutes for a simple app, and its
second run costs the same as its first. See `MEMO-DSH-RESPONSE.md`.

**Rust-native frameworks** (Rig 8K stars/3.6M downloads, AutoAgents,
AxonerAI). Rust's advantages are validated (5x memory reduction,
13-43x throughput vs Python, ms cold starts) but nobody put a real UI on
one — Rig's own coding agent is a TUI.

**The convergence.** Every major harness standardized on six things:
AGENTS.md project memory, the Agent Skills `SKILL.md` standard, MCP
tools, subagents-as-markdown, permission levels, scheduled tasks. They
made config files the interface — untyped, unversioned, undiffable
prose. Wright's answer: the same ideas as a typed, versioned, diffable
blueprint with a UI on top and an agent workforce underneath.

**The empty quadrant:** UI-first + zero-user-code + Rust-fast +
project-producing. Nobody holds two of the four. We hold all four by
construction.

## 3. Product definition — the five verbs

The user's entire relationship with Wright is five verbs. Every UI
surface maps to one.

**DECLARE — the interview.** A conversation with the Interviewer
agent, structured like a sharp consultant: what should exist, what goes
in, what comes out, what failure looks like, when it runs. The output
is not a chat log — it is a blueprint condensing live beside the
conversation (ghost cards drifting into place, capability chips popping
in as suggested pills). The **Confirm Gate**: nothing fires until the
user reads back a plain-language summary of what will exist and agrees.
Every line of the summary clicks through to the exchange that produced
it.

**COMPOSE — the blueprint canvas.** Agent cards on a physics-settled
DAG (vieww's rigid3d does the layout). The core gesture is semantics:
*drag a card onto another = dependent; drop apart = independent*.
Cards carry role, goal, tools (chips), model tier, memory dial. The
chips rail — `Scheduling` `Secrets` `Logging` `Tests` `CI` `Webhooks` —
is the only configuration surface; toggling a chip injects the matching
system agents. The recipes drawer shows community blueprints with live
animated mini-DAG previews; one click inserts, a mini-interview
customizes.

**CONSTRAIN — guardrails as UI.** Per-agent permission toggles
(Files Read / Files Write / Network / Shell / Spawn), default OFF.
Budget sliders (per firing, per day, wall clock, retries) with live
estimates from past firings. Approval rules ("ask me before sending
email / spending > $X / deleting anything") become pause points in the
theater. Fully-sandboxed cards wear a green shield — least-privilege
as a visible, satisfying state.

**UI mapping (v0.2, ADR-018).** The five verbs are the internal
framework; the shell presents them as five screens: **Describe**
(DECLARE), **Plan** (COMPOSE + CONSTRAIN — the Confirm Gate with stages),
**Run** (OBSERVE), **Result** (the deliverable and its proof), and
**Inspect** (REFINE's evidence: events, versions, where state lives).

**OBSERVE — the run theater.** The same canvas, alive: cards glow as
they work (vieww-effects halation and light rays), stream intensity
equals token throughput, the spend odometer runs bottom-right. The
**Event River** at the bottom is the event spine as a scrubbable
timeline — scrubbing time-travels the entire UI (determinism makes
replay exact). Failures freeze-frame with a "What happened?" plain-
language explanation and three buttons: Retry / Fix by talking / Fork
from here. Approval pauses pulse the requesting card for one-tap
decisions. On success, the deliverable card materializes — the "it
exists" moment.

**REFINE — the conversation that edits.** Chat about any part; the
blueprint updates as diffs with an affected-subgraph highlight, and the
moat shows up as a sentence a human enjoys reading: *"This change
refires Summarizer + Testwright (est. $0.03, 45s). Fetcher is cached —
free."* The version strip is the commit log; scrubbing it time-travels
the whole app to that blueprint version.

**UI constitution (five rules):** the user never writes code; everything
is inspectable to arbitrary depth; everything is time-travelable; trust
is always one glance away (permissions and spend visible without
clicking); every dead end has a "talk about this" button.

**App anatomy.** One binary. The UI shell and the runtime live in one
process; closing the window parks it in the menu bar as the daemon that
fires the 8am digest (the Operator registers auto-start on first
schedule). `--headless` exists for CI and the three nerds who want it.
First run opens directly into the interview with one question: *"What
should exist?"*

## 4. System architecture

Five layers, one spine.

**Layer 0 — the shell** (vieww). Card shell, activity views, StudioTheme,
input pipeline, Vulkan rendering for the Run Theater. Zero business
logic: the UI is a pure projection of the event stream. It renders; it
emits commands; it never decides.

**Layer 1 — the event spine.** Append-only, deterministically ordered,
one log per run, fsynced before the action it records counts as done
(`wright resume` reads it after a crash). Replay projects it; it never
re-executes (ADR-016). Every agent start, tool call, token spent, file
written, decision made. One log, three superpowers: live UI (project
forward), time-travel replay (project backward), debugging (fork at any
event). This is vieww's film-pipeline determinism applied to agent runs.

**Layer 2 — the blueprint store.** Versioned like git: every Confirm
Gate pass and approved REFINE is a commit; the canonical serialization's
hash IS the commit; diffs are first-class UI objects. The unit of
sharing: recipes are signed, parameterized blueprints (ClawHub proved
the market; we ship the whole project definition, not just a skill).

**Layer 3 — the compiler.** Blueprint in, task graph out: resolves
dependencies (independent agents become parallel lanes), injects the
system workforce from chips, routes models per tier (cost/quality
decisions logged to the Why-ledger), splits budgets, and emits a
permission-annotated DAG. Implemented and tested today in
`wright-core::compiler`.

**Layer 4 — the runtime.** Tokio-shaped structured concurrency (our own
zero-unsafe executor on `std::task::Wake`: spawn, join sets,
cooperative cancellation). Sandboxed tool execution behind the
permission gate: every tool call checks capabilities, budget, and
approval policy — the anti-OpenClaw layer. The task cache is Bazel's
action graph in miniature: task hash = (agent definition ∥ model id ∥
input artifact hashes ∥ tool versions ∥ permission set ∥ app version).

**Layer 5 — the artifact store.** Content-addressed with BLAKE3 (git's
object model, ours by port), verified on every read. On disk,
local-first, under `~/.wright/projects/<name>-<id>/` — outside the
project, so sandboxed agents cannot reach it (ADR-013).

### The two-tier workforce

| Tier | Who | Injected by |
|---|---|---|
| **System agents** (we ship them — they ARE the harness) | Interviewer, Architect, Shipwright, Testwright, Logwright, Reviewer, Firewright, Operator, Explainer | The compiler, from chips |
| **Project agents** (the user's blueprint) | Fetcher, Summarizer, Mailer, ... whatever the use case needs | The blueprint |

The user composes the visible tier; the compiler silently wraps it in
the system tier. Since users cannot impose structure, WE impose one
universal project convention — the same skeleton shape for every
project Wright has ever built. Uniformity is what makes agent-written
code navigable by other agents and by a sleepy engineer at 3AM.
Rails' "convention over configuration," applied to agent output. All
nine prompts live in `wright-agents` as contracts: role, inputs/outputs,
definition of done.

### vieww mapping

| vieww asset | Becomes |
|---|---|
| Card shell, activity views, StudioTheme, input pipeline | The five-verb UI shell |
| Vulkan real-time rendering | Run Theater (glowing cards, streams at 1080p60) |
| rigid3d physics | DAG layout physics (cards settle into place) |
| vieww-effects (halation, light rays, bloom) | Active-card glow, stream intensity, the understated completion celebration |
| Deterministic film pipeline (as pattern) | Event spine → replay → time-travel scrubber |
| Benchmark receipts (as pattern) | Run records — machine-verifiable receipts |
| Headless renderer + ffmpeg (as pattern) | Headless daemon mode |
| vieww-audio | The muted-by-default completion chime, synthesized |

New crates in the app (all five exist in the scaffold, compiling,
tested): `core` (blueprint/compiler/events/wire), `runtime` (executor/
gate/cache/models/tls), `agents` (workforce), `tools` (implementations),
`app` (the binary).

## 5. The blueprint format

One file is the whole project. A KDL-inspired dialect (`meta.format`
versions it), parseable by our ~700-line two-stage parser, canonical
form deterministic (formatting changes never change the hash). Full
spec: `BLUEPRINT-FORMAT.md`. Shape:

```kdl
agent Fetcher {
    goal "Fetch top 30 HN stories: id, title, url, points"
    model tier="cheap"
    tools "http-read"
    out "stories.json"
    permissions network=true
}
edge from="Fetcher" to="Summarizer"
chips { scheduling cron="0 8 * * *" tz="Asia/Kolkata" logging on tests on }
guardrails { budget per-firing=500000 daily=3000000 wall-clock 300 max-turns 40 }
deliverable kind="email" { check "Mail received 08:00-08:05 local" }
models { tier cheap prefer="deepseek-v4-flash" fallback="glm-5.3-flash" }
```

The agent node is a typed superset of the industry's richest definition
(Claude Code subagent frontmatter): goal, tier (never a model name),
tool allowlist, memory scope, approval policy, retries with no-retry
semantics for side effects, typed input/output contracts, and — what
nobody else has — a permission set and a budget, both enforced and both
part of provenance.

## 6. Security and trust

Invariants (each is enforced in code, most are pinned by tests):

1. **Secrets are references, never values.** `keychain:` scheme only;
   the parser rejects anything else. The Operator brokers from the OS
   keychain at use-time and drops after; the event spine records that a
   secret was used, never which value.
2. **The permission gate is the only path to a tool.** Budget →
   capabilities → approval, in that order. Denials are free; asks are
   free; allows charge the estimate. Unknown agents have zero grants.
3. **Permissions are provenance** (ADR-006): the cache cannot launder
   an artifact made under broader powers.
4. **Least privilege by default.** Every capability toggles OFF; the
   Confirm Gate shows exactly what the blueprint grants.
5. **TLS is OS-native, never hand-rolled** — and until the backend
   lands (W-6), HTTPS fails loudly rather than downgrading to plaintext.
6. **Run receipts** (vieww benchmark-receipt pattern): every firing
   writes a machine-verifiable record — blueprint commit, trigger,
   cost, events, artifacts.
7. **Signed recipes.** Gallery recipes carry ed25519 signatures;
   unsigned community recipes install with a warning badge and a chip-
   permission review. (OpenClaw's CVE firestorm is the counterexample
   we design against.)
8. **Local-first.** No accounts, no cloud, no telemetry by default.
   Trust is won by not phoning home.

## 7. The moat — incremental refire

Every competitor re-runs everything. Wright diffs the blueprint and
refires only what changed, because every artifact carries its cache key:

| Change | Refires | Cached |
|---|---|---|
| Agent goal/model/tools/permissions | that agent + downstream | upstream |
| Edge rewired | affected subgraph | rest |
| Chip toggled | system subgraph only | all project agents |
| Guardrails / schedule changed | nothing | everything |
| Model tier preference swapped | tier agents + downstream | others |
| Deliverable check changed | Operator + Reviewer | project agents |

The proof it matters is dsh's measured ~20M tokens for a simple app.
Their second run costs the same. Ours is a cache hit: *"make summaries
punchier" → Summarizer + Testwright refire (est. $0.03, 45s), Fetcher
cached — free.* Implemented today: `wright-core::blueprint::diff`
compiles the table above, and its tests pin each row.

## 8. The one-dependency doctrine

Direct dependencies in `Cargo.toml`: exactly one — vieww. Everything
else is ours, mimicking proven designs (see ADR-002 for the full mimic
map). The scaffold is the receipt: JSON, HTTP/1.1, SSE, a structured-
concurrency executor, content hashing — all from scratch, all tested,
zero unsafe, std-only. The one exception is doctrine line one: crypto
reaches the OS and is never hand-rolled.

## 9. Interop — export, never import

Wrighted projects emit `AGENTS.md` and `SKILL.md` so Claude Code, Codex,
Kimi Code, and dsh can work ON the materialization. MCP is a tool
source (a protocol implementation we write, not a dependency). Wright
remains the source of truth; other harnesses are tools at the edge. We
never import prose as truth — anything entering the system goes through
the Interviewer and the Confirm Gate like everything else.

## 10. Roadmap

Week one is `SPIKES.md`: W-1 vieww consumability (the wiring spike),
W-2 capability inventory, W-3 parser hardening, W-4 the cache-hit demo
(the anti-dsh showpiece), W-5 executor I/O backends, W-6 OS-native TLS,
W-7 replay in anger, W-8 tray daemon. Milestone 0.1: the Interview runs
headless, one recipe fires end-to-end on a schedule, and the refire
economics are demonstrable in a terminal.

Then the UI (W-1 unlocks the shell), then the gallery, then the world.

## 11. Open questions

Honest ledger, mirrored in `DECISIONS.md`: multi-user tenancy (hold),
recipe curation bar (open), recursive sub-agents (v1: no), the UI for
multiple deliverables (undesigned), agent-authored tool-packs (v2,
signed artifacts referenced by hash).
