# How Wright Wins Against DeepSeek Harness

*Positioning memo · October 2026 · dsh = DeepSeek Harness, 255K stars,
public preview, MIT, TypeScript, "everything is a plugin" (Cordis).*

---

## Corrections (v0.2 review, October 2026)

Two claims below were wrong or overstated when checked against current
public information. They are kept, struck through in spirit, so the
record shows what changed:

- **"No determinism, no replay."** dsh records every model-visible event
  in an append-only log that supports inspection, replay and forking.
  Wright's real difference is replaying and diffing a whole *project
  version* (blueprint commit + receipts + cache), not a single session.
- **"~20M tokens for a simple app; the second run costs the same."** One
  published benchmark measured dsh at ~89K runtime tokens per task with
  code mode and prompt caching. Our argument must rest on measured cost
  across REPEAT runs and targeted changes (the W-4 demo exists now; a
  real-model benchmark is v0.3 item 1), not on one bad review.
- Scheduling (with time zones), subagents, approval prompts and a desktop
  + web UI are standard across dsh, Kimi Work, Codex and Claude Code.
  Differentiation is the versioned blueprint, freshness-aware refire,
  code-generated Confirm Gate, checks-before-delivery, and model
  neutrality — not any one of those features.

## What dsh got right (respect first)

1. **The category is enormous.** 255K stars in months for an agent
   harness with a UI. The appetite for "harness as a product" is not a
   thesis anymore; it is a measurement.
2. **A real UI.** Desktop + local web, workspace metaphor, visible
   tool calls, file previews. The terminal was not the only door after
   all.
3. **Composability as identity.** Tools, skills, modes, models,
   providers, even *other harnesses* (Claude Code, Codex as sub-agent
   plugins) are pluggable. That flexibility earned the stars.
4. **Creator mode.** Build a plugin by chatting. It is their zero-code
   gesture, and it validated our direction.

## Where dsh is structurally exposed

**1. It is a workspace, not a factory.** The unit of work is a session.
There is no versioned project definition, no lifecycle, no definition
of done. Ask a dsh user "run it again next Tuesday, but with shorter
summaries, and show me what changed since last week" and you are asking
for features their architecture treats as plugins someone will write
someday. Wright's unit is the blueprint: a typed, versioned, diffable
project that fires on schedules and refires incrementally.

**2. "Everything is a plugin" pushes complexity onto the user.** Plugin
selection, YAML config, mode picking, permission scoping from read-only
to dangerous-skip-permissions. 255K stars arrived *despite* that tax.
Our bet is the opposite: we absorb complexity (chips, injected system
agents, one universal project convention). Simplicity is not a subset
of their roadmap — it is a different architecture.

**3. Token economics.** Hands-on reviewers measured ~20M tokens and 35
minutes for a simple ISS-tracker app, with 95-100% provider cache hits
*within* runs. But across runs there is no task cache, no structural
diff, no invalidation: the second run costs the same as the first.
Wright's refire engine makes the second run nearly free (§7 of
ARCHITECTURE.md; the demo lands as Spike W-4). We should publish this
comparison weekly once the demo exists — respectfully, with receipts.

**4. No determinism, no replay.** Sessions are logs you can inspect,
not event spines you can scrub, fork, and re-project. The time-travel
scrubber is unanswerable inside their model without a rewrite of their
execution layer.

**5. Developer preview chaos.** "There WILL be compatibility-breaking
changes" is in their README. Enterprises noticed. Our blueprint is
versioned (`meta.format`, `min-app`); recipes never break silently.

## The counter-positioning, in one table

| | dsh | Wright |
|---|---|---|
| Unit of work | Session (chat about files) | Blueprint (versioned project) |
| Setup burden | Plugin selection + YAML + modes | Chips; system agents inject the rest |
| Second run | Same cost | Cache hit — near zero |
| Debugging | Tool-call inspection | Time-travel scrubber, fork-from-here |
| Definition of done | None | Deliverable check, enforced |
| Architecture | Everything is a plugin | Everything is compiled |
| Runtime | Node/TypeScript | Rust, one binary, one dependency |

## What we take inspiration from (never their code — ADR-002)

- **Plugin marketplace energy** → recipes: signed, parameterized
  blueprints, one-click + mini-interview. ClawHub proved demand at 44K
  skills; we ship the whole project, not a skill.
- **Creator mode** → the Interviewer already builds by conversation;
  recipes make the output shareable and signed.
- **Wiring other harnesses as sub-agents** → ours is export-never-
  import (ADR-007): wrighted projects emit AGENTS.md/SKILL.md so Claude
  Code/Codex/Kimi/dsh can work ON our materializations. They become
  tools at the edge of our source of truth.

## Playbook (next 6 weeks)

1. **Week 1:** ship the W-4 cache-hit demo; the token-economics
   comparison becomes our recurring proof point.
2. **Weeks 2-3:** vieww UI shell + Interview → Confirm Gate end to end.
   The demo video is one sentence: "Describe it. It's wrought."
3. **Weeks 4-6:** gallery with 10 signed recipes; dsh's own greatest
   hits (daily digest, weekly report, file organizer) as the first
   three, each with the refire cost printed on the card.

## Honest risks

- **Velocity.** dsh iterates in public with a corporation behind it.
  We counter with focus: their roadmap must serve plugins; ours serves
  one job — the blueprint's life.
- **"Good enough."** For many users a session that can be re-prompted
  is fine. Our answer is the economics line: at scale, re-prompting is
  a subscription; refiring is a rounding error.
- **Network effects.** 30K forks of plugin examples. Recipes must be
  as easy to publish — and signed, which is a feature their ecosystem
  learned the hard way (9 CVEs in 4 days, elsewhere in the family).

## The one-liner for the war room

> dsh made the harness configurable. Wright makes the harness
> disappear.
