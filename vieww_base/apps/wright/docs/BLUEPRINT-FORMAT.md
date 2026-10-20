# The Blueprint Format

*The typed, versioned, diffable project definition. One file is the whole
project. The UI edits it through cards, chips and conversation; people may
edit it by hand too — `wright plan` checks every edit, and the canonical
form makes formatting irrelevant. Format version 1, Wright v0.2.*

---

## Design goals

1. **One file = the whole project** — inspectable, diffable, portable.
2. **Diffable by structure, not text** — a diff compiles to a refire plan:
   what re-runs, what the cache hands back, what is fresh every run anyway.
3. **Typed and validated at the gate** — nothing ambiguous survives the
   Confirm Gate, and the Confirm Gate's words are generated from this
   model by code (`core/src/confirm.rs`), never by a model.
4. **Round-trippable** — `parse(canonical(bp)) == bp` (pinned by a test),
   so the UI can edit the typed model and save the canonical text.

## Grammar (the Wright KDL dialect)

```text
doc       := statement*
statement := node
node      := IDENT arg? prop* block?
arg       := STRING | INT | DECIMAL | BOOL | IDENT
prop      := IDENT "=" (STRING | INT | DECIMAL | BOOL | IDENT)
block     := "{" statement* "}"
comment   := "//" to end of line
```

- Nodes take **at most one** positional argument (enforced loudly).
- Statements separate on newlines — or on the same line when unambiguous.
- Strings: `"..."` with `\" \\ \n \t \r` escapes. Booleans: `true/false/on/off`.
- `DECIMAL` (`0.3`) is kept as its text. The blueprint has no decimal
  fields, so an integer field given `1.5` is rejected; `config.kdl` uses
  decimals for prices.
- Every mapping error carries the line of the node it is about.

## Sections

| Section | Required | Contents |
|---|---|---|
| `meta` | yes | name, format, min-app, created, lineage |
| `intent` | yes | summary (the person's words), confirmed (`v3` = third confirmed version), evidence |
| `project` | no | language (`python \| node \| rust \| shell \| none`), test, run |
| `agent NAME` (repeatable) | ≥1 | the visible workforce |
| `edge from= to=` (repeatable) | no | dependency topology; the target receives the source's outputs |
| `chips` | no | scheduling (cron, tz), secrets, logging, tests, ci, webhooks |
| `guardrails` | no | budget per-firing / daily (micro-USD), wall-clock (s), max-turns (per firing) |
| `deliverable kind=` (repeatable) | ≥1 | the plain-language check that defines done |
| `models` | no | tier → prefer / fallback model ids |

### `project`

```kdl
project {
    language "python"                       // picks the skeleton (docs/PROJECT-CONVENTION.md)
    test "PYTHONPATH=src python3 -m unittest discover -s tests -t . -q"   // optional override
    run "PYTHONPATH=src python3 -m my_tool" // optional override
}
```

Omitted commands fall back to the convention's defaults for the language.
Changing `project` refires every agent (the skeleton changed).

### Agent node fields

| Field | Type | Notes |
|---|---|---|
| `goal` | string, one sentence | the whole job description the agent receives |
| `model tier=` | `cheap \| quality \| auto` | never a model name (ADR-010) |
| `tools` | repeatable string | from the palette; each tool's capabilities must be granted |
| `memory scope=` | `none \| firing \| project` | |
| `in` / `out` | repeatable string | artifact names (plain file names) in `artifacts/`; inputs must come from a connected producer |
| `approval` | `none \| before-send` | world-touching tools pause for a human |
| `retries` | 0..=255 | forced to 0 for delivery agents |
| `permissions` | bool props: files-read, files-write, network, shell, spawn | default OFF; part of provenance |

Agents with `out` lines automatically get the `write-output` tool, which
can write ONLY their declared artifact names. Names of system agents
(Testwright, Reviewer, …) are reserved.

### The tool palette and what it means for the cache

| Tool | Capabilities | Reads | Effect |
|---|---|---|---|
| write-output | (the blueprint is the grant) | nothing | workspace |
| files-read, files-list | files-read | workspace | — |
| files-write | files-write | nothing | workspace |
| edit, apply-patch | files-read, files-write | workspace | workspace |
| git (status/diff/log/add/commit) | files-read, files-write, spawn | workspace | workspace |
| run-tests | files-read, spawn | workspace | workspace |
| shell | shell, spawn | **outside** | workspace |
| http-read | network | **outside** | — |
| http-post | network | **outside** | **world** |
| browser (not implemented) | network | outside | world |

- An agent with any **outside**-reading tool is **volatile**: it runs fresh
  every firing, and so does everything downstream of it unless its output
  is byte-identical (content addressing gives the early cutoff for free).
- An agent with any **world**-effect tool is a **delivery** agent: it is
  scheduled after the Reviewer, never cached, never retried.
- An agent with a **workspace**-reading tool has the workspace tree hash in
  its cache key.

### Secrets

`secrets name="keychain:path"` — references only. The parser rejects any
other scheme. Values are resolved at the moment of use from the OS keychain
(or `WRIGHT_SECRET_<PATH>` on CI) and never stored, logged or evented.

## Canonical form and commits

`core/src/blueprint/canon.rs` emits ONE serialization: fixed section and
field order, agents sorted by name, edges sorted, strings escaped, no
comments. Formatting never changes the hash; **the BLAKE3 hash of the
canonical text IS the commit**. Every firing and every confirmation commits
the blueprint to the project store's history (`wright plan` diffs against
the last commit).

## The diff engine and invalidation semantics

`core/src/blueprint/diff.rs`:

| Change | Refires | Cached |
|---|---|---|
| Agent definition | that agent + downstream | upstream |
| Agent removed | its former targets + downstream | rest |
| Edge added / removed | the edge's TARGET + downstream | everything else |
| Edges merely reordered | nothing (same commit) | everything |
| Chip toggled | that chip's system agents only | all project agents |
| Guardrails / schedule / tz | nothing | everything |
| Deliverable check | Testwright + Reviewer + delivery agents | other project agents |
| Model tier preference | agents on that tier + downstream | others |
| Intent summary | nothing (agents are not told it) | everything |
| Project language / commands | every agent | nothing |

Separately, volatile and delivery agents are listed as **fresh every run
anyway** — the plan never calls them "cached". Every row is pinned by a test.

## Cache key

```
key = BLAKE3( "wright-cache-key-v2"
            ∥ definition       project agent: canonical card + prompt-template version + max-tokens
                               system agent:  prompt contract + template version + project + chips + checks
            ∥ resolved model   primary, fallback, tier
            ∥ tools            name@version, each
            ∥ inputs           content hashes of `in` artifacts
                               (checkers: every artifact; the Reviewer also the Testwright's report)
            ∥ permission bits  provenance
            ∥ workspace hash   only for workspace readers
            ∥ app version )
```

Absent by design: guardrails, schedules, intent text, other agents'
definitions (their artifacts enter by hash).

## Where things live

```
<project>/                     the project — yours, git-friendly
  blueprint.kdl  README.md  AGENTS.md  src/  tests/  logs/  artifacts/

~/.wright/                     Wright's — never visible to sandboxed agents
  config.kdl                   providers, prices, interview model
  schedules.json               the daemon's registry
  projects/<name>-<id>/
    runs/<n>/events.jsonl      the event spine (fsynced)
    runs/<n>/receipt.json      the run receipt
    objects/<ab>/<blake3>      content-addressed artifacts, verified on read
    cache/<key>.json           task cache entries
    blueprints/<hash>.kdl, log every committed version
    approvals/                 cross-process approval requests/answers
    spend.jsonl                daily budget accounting
```

## Run receipts (`wright.run-receipt/1`)

JSON: run, project, blueprint commit, trigger, start/finish, ok, headline,
cost, saved, per-task status, artifacts with BLAKE3 hashes, the Reviewer's
verdict, and `events_blake3` — the hash of the event log, so a receipt and
its log can be checked against each other by machine.

## Recipes (v0.3, not implemented)

A recipe is a parameterized blueprint (`param` nodes generate the install
mini-interview). Signing needs Ed25519, which the doctrine routes through OS
APIs (ADR-012); until that lands, recipes are not distributed.
