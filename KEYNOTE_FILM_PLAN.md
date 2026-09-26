# vieww keynote film — the production blueprint

**Version:** plan-2.0 · supersedes the story layer of scene graph v0.7 (structure re-architected; systems inherited — see §7.1)
**For:** the viewwstudio product release keynote
**Runtime:** 3:00 locked · 60fps master · 10,800 frames
**Story engine:** kishōtenketsu (起承転結) — the four-act methodology, made explicit
**Composed from:** 90 proven film_lab plates + 8 new bespoke builds

---

## 0 · How to read this document

This is a build program, not a wish list. Every scene names its tier, its
effects, the props it reuses, and the numbers it is allowed to show; every
effect names the primitive that renders it; every build names what it blocks.
House style inherited from the handoff and the lab:

- **Node ids carry the plan.** `S07` is a scene, `E-16` an effect, `N-03` a new
  build, `T-06` a task, `F4` a reveal frame, `K2` a twist. Changes are proposed
  as node edits, not prose.
- **Durations are free while the sums still close.** 40 + 85 + 30 + 25 = 180s.
  The reveal inside Act III closes 3+1+2+3+2+3 = 14s; Act IV closes 4+13+8 =
  25s. Any retime that keeps the acts summing to 180 is a free edit.
- **Receipts over claims.** No number in this film is typed by a human. Every
  figure on screen is emitted by the pipeline it describes — the probe, the
  element tree, FrameStats, CI artifacts, or the renderer's own account.
- **The cut works muted.** Sound is the author's, later; every beat is
  captioned by construction.

---

## 1 · The mission and the method

### 1.1 The film, and the meta-move

A **3:00 keynote film** for the viewwstudio product release, carrying one
meta-move that defines everything else: **the film's motion graphics layer is
rendered by vieww itself** — headless `FrameDriver`, fixed clock, raw frames
out, re-renderable to the byte on a given bench. The closing sting is the
contract: *"this film was rendered with vieww."*

What the film is allowed to fake: **nothing in a product shot.** Tier C camera
footage may be color-graded and carries no claims. Everything else is the
product rendering itself at capture. The tiers:

| tier | meaning | determinism |
|---|---|---|
| **A** | headless render through `NativeRenderer` | byte-reproducible per bench |
| **B** | scripted replay through the real input pipeline | deterministic ≠ fake |
| **C** | camera capture (warmth, no claims) | gradeable |

The strategy's one-line thesis, inherited verbatim from the keynote strategy
note: **the film's grammar becomes the product's proof.** The absence of cuts
around waiting is evidence; the 60fps cadence is the product's own; the
manifest on the end card is the renderer's account of itself. Nothing in the
film argues — everything demonstrates.

### 1.2 The four acts — kishōtenketsu, made explicit

The keynote strategy note named the theft: *"Kishōtenketsu for the joints. The
four-act Japanese structure has no villain; its engine is the twist that
recontextualizes what you've seen."* This plan promotes that engine from the
joints to the whole skeleton. The 3:00 is re-architected as **four explicit
movements**, replacing the inherited three-act frame:

| act | movement | name | time | job |
|---|---|---|---|---|
| I | **Ki** (起 · introduce) | THE WORLD OF THE WAIT | 0:00–0:40 · 40s | establish the world as it is — the wait — without naming an enemy |
| II | **Shō** (承 · develop) | THE SESSION | 0:40–2:05 · 85s | deepen one session, one artifact, across every surface; comprehension builds |
| III | **Ten** (転 · twist) | THE TWISTS | 2:05–2:35 · 30s | the cascade that recontextualizes everything watched |
| IV | **Ketsu** (結 · reconcile) | THE RECONCILIATION | 2:35–3:00 · 25s | the new world settles; the circle closes; the sting |

Kishōtenketsu's engine is not conflict but **recontextualization**: the twist
does not defeat an antagonist, it changes the meaning of what you already saw.
That is exactly this film's shape — its best beats were always reveals, and
Act III stacks them; Act IV then reconciles by returning to the opening image
transformed (§4, S16). No villain appears anywhere: the wait is a condition,
not a competitor; it is never even granted a product name.

### 1.3 The twist ladder

The K-twists, restaged for the four movements. Each is a reveal that
recontextualizes a specific earlier image:

| id | time | twist | what it recontextualizes |
|---|---|---|---|
| **K1** | 0:13 | the hard cut out of the wait: into silence **and** 60fps — the wait is obsolete | the judder and grain of S01 |
| **K0** | 0:36 | a Rust framework whose first shown code is `say`, not Rust | "UI in Rust" as you assumed it |
| **K2** | 2:03 | tap on the phone; the desktop felt it — **7** on every surface | "a desktop tool" |
| **K3** | 2:05 | the IDE you've been watching is a vieww app — devtools turns on the studio itself | every studio shot since S04 |
| **K4** | 2:21 | what you watched, from outside — the three trees unfold; the signal apart, reading **7** | the whole session as one picture |
| **K5** | 2:43 | this film was rendered with vieww | the film itself |

### 1.4 The escalation curve

**Richness escalates deliberately, and every escalation is a demo.** The curve
is the argument:

- **Act I opens degraded by design** — but degraded-*crafted*: grain, dust,
  defocus and 24-in-60 judder rendered through the same FilterChain grammar
  the aurora plate proved (E-21). The poverty of the image is a *rendered*
  effect, which is itself the first quiet demo — and the hard cut into S02
  (silence, 60fps smoothness) is the first escalation: the cadence *is* the
  product's argument.
- **Act II is clean and restrained** — the studio register: frosted glass,
  springs, dithered gradients. The fireworks here are data (the receipts), not
  ornament. Restraint keeps the budget for the back half.
- **Act III spends** — the wonder register: Plus-blended light, perspective
  unfolds, receipts blooming in space. The last 55 seconds carry the visual
  budget.
- **Act IV settles** — the fold-back, typographic calm, one accent. The
  escalation resolves into stillness; the end card holds.

### 1.5 The inherited rules

The handoff's five rules survive this re-architecture untouched — they are the
culture, not the structure:

1. **No number is typed by a human** — every figure is emitted by the pipeline
   it describes. (Two incidents already proved the rule: 4320→10,800 when
   60fps locked; 2550→2551 when the mount was forgotten. Expect `frames + 1`.)
2. **No effect is added in post** — visible in a product shot ⇒ the product
   rendered it.
3. **The edit only chooses where the knife falls** — cuts select, never
   fabricate; within the session, strips are deliberately unfelt.
4. **Richness escalates deliberately — every escalation is a demo.**
5. **If an annotation cannot be sourced live, it does not appear.**

One correction and one addition, both from evidence in this repository (see
§7.2): the S04 caption drops the phrase *"nothing was compiled"* — the studio's
own `06-say.md` documents the real pipeline (`say → codegen → rustc → cdylib →
dlopen`), so the honest caption is **"alive in N seconds"** with N measured by
the T-01 probe; and the manifest now carries its **bench identity** (round 11
found glyph rasterization is a property of the host's fonts — determinism is
per-bench, and the manifest says which bench).

---

## 2 · The capability audit — what vieww can do today

Everything below is verified in this repository with receipts, organized into
the four families the film draws on. Nothing in §3–§6 reaches past this
surface.

### 2.1 Family A — viewwstudio, the product being released

The studio is itself a vieww app (dogfooded — the fact K3 twists), and its
entire loop is the film's Act II stage.

| capability | what it is | film use |
|---|---|---|
| **the preview loop** | a buffer is a standalone crate; `fn screen() -> impl Widget` is the entry; edit a line, see the picture change | S04's first paint; S06's per-keystroke compose |
| **`say`** | the English-facing screen language: `keep` state that survives recompiles, screens, a fixed widget/property/action/icon vocabulary, all problems reported at once; codegen emits Rust annotated `// say: file:line` | S04 (K0); the ladder's birth |
| **the descent** | the same buffer as ordinary Rust, state held across the switch — "doorway, not a wall" | S05 |
| **live compose** | per-keystroke preview; a deliberate error renders as a boundary (ErrorPlaceholder), not a crash | S06 |
| **state survives** | `keep` values carry across remounts, edits, refreshes | S08 |
| **the receipts** | build counts, the damage overlay, FrameStats ms·px, the scrub's 10-writes-1-rebuild coalescing made visible | S07, S09 |
| **platform preview** | device frames (iOS / Android / Desktop), safe areas and theme conventions switched without touching `screen()` | S10's staging |
| **build & run** | Desktop, Windows cross-compile, Android debug-signed APK (named as such), iOS — each tool checked before a build starts | S12's reach |
| **devtools** | the inspector — which can inspect the studio itself | S11 (K3) |
| **the craft surface** | LSP, completion, folding, find bar, file tree, git panel, command palette, problems panel, recovery | the texture of every studio shot |

### 2.2 Family B — the framework powers (the film's own toolkit)

The film's motion graphics layer is built from exactly these primitives, each
proven by a lab plate:

| power | primitive | receipt |
|---|---|---|
| **signals outside the tree** | `runtime().signal(v)`; reading during `build` **is** the subscription; rebuild is pull-based and coalesced — ten writes → one rebuild | the scrub plate: 10 writes · 1 rebuild · 1 damage rect, counted live |
| **three trees** | Widget → Element → Render; identity + state survive rebuilds | the reveal's three planes are the real trees, from the live inspector |
| **own native rasterizer** | `vieww-paint` `native`: 4×4 supersampling, gradients, blurs, shadows, all 28 blend modes, glyph caches — headless via `FrameDriver` | byte-identical across runs; every receipt PNG in the repo was drawn by it |
| **springs** | `vieww-animation`: `Spring`/`SpringSpec`/`SpringPreset` (Expressive, Standard), `retarget`/`retune` mid-flight, `Spring2D`, staggered `Timeline` | spring plate: the wordmark drop + underline overshoot, springs drawn as their own receipt |
| **paths** | `Path` verbs, full-turn `arc` (U-22 closed), ~40k segments in one stroke verb, dash-phase, `stroke_outline` glyph outlines | circuit plate: the self-drawing session line; megapath: a night landscape as one line |
| **gradients** | linear/radial/sweep, 16 stops (U-23 closed), `with_stops_resampled` for ramps of any length | prism plate: 256 stops as 256 breathing rects |
| **the blur economy** | gaussian + directional `blur_angle` (motion blur along velocity); `Filtered` chains; backdrop blur (frosted glass); 32-layer guard | rackfocus (focus pull as cinematography); filterstack (nested σ sweep); shadowplay (30/32 layers, gauge drawn live) |
| **blend modes** | 28 modes through the rasterizer; `Plus` for additive light, `Screen`, the non-separable four; sketch-level `blended_layer` | blendmatrix: 16/16 distinct, "production-grade"; blackhole/startrail: light as accumulation |
| **3D** | `Transform3::project_rect` + the manual pipeline (painter's sort, Lambert + Blinn-Phong, perspective camera); 4D→3D→2D demonstrated | unfold: three planes fanning; dolly: the vertigo shot at 0.1% drift; tesseract |
| **damage tracking** | one write lights one region; partial repaint; `render_damaged` | damage plate: PerformanceOverlay with damage-area history measured from the mock's own geometry |
| **SceneReport** | per-frame counted work: shapes, images, glyph_runs, layers, `unsupported_blends`, `open_subpath_fills` | the manifest: the film audits itself; the census caught U-22 and swarm's 38,419 open triangles |
| **text** | shaping, CJK through the shaper (107 glyph runs/frame, LXGW WenKai + Noto Serif SC, tofu probe), glyph-run storms at 936/frame | han; kinetic (380 runs); typo (39 glyphs become weather) |
| **platforms** | winit desktop + Android (**59.3 fps / 4.09 ms median on a Redmi Note 7 Pro, 2019** — device-suite receipts), web-dom (real DOM), wasm | S10's world tour; S12's card |
| **the ceilings** | galaxy: **60,527 shapes/frame in 101 ms** · mandel: 57,602 rects in 69 ms · hero4k: 3840×2160 at 440–489 ms/frame · longplay: 256 frames, RSS flat 38.3→39.0 MiB | "nothing in the film is limited by the renderer" — the constraint is taste |

### 2.3 Family C — the props library (the 90 plates)

The lab is the film's motion language, and every plate ships the three-artifact
receipt set (`anim.gif` + `sheet.png` + `metrics.txt`). Three registers:

**The spine register** (plates that *are* scenes, reused directly):
`kinetic` (type-on, hour-counter, odometer, count-ups) · `spring` (wordmark
drop, overshoot) · `morph` (say→rust scan-line sweep, state held) · `scrub`
(10→1 coalescing, build-count badge) · `damage` (one region lights) ·
`rackfocus` (the blur ramp between buffer and preview) · `circuit` (the
self-drawing session line, node blooms, fork ×3, the axis) · `globe` (the
reveal spine: the 7, the ghost part, the pull-back, planes fanning) ·
`unfold` (Transform3 planes + tree glyphs) · `receipts` (benchmark card in 3D
+ CI timeline) · `endcard` (wordmark, install lines, manifest, the sting) ·
`light` (the sheet-vf aesthetic floor itself) · `aurora` (the degradation
grammar: FilterChain ramp + grain + dust).

**The texture register** (motifs and grammar): `wordmark` (glyph outlines,
re-sorted chrome, glint, mirrored reflection) · `typo` (a sentence rides a
flow field and condenses into a drop) · `settle` (scramble-to-settle,
per-glyph pops) · `ink` / `sea` (drop → nebula; drop → entire ocean — the
genesis arcs) · `ghosts` (motion persistence, direction-bucketed blur
economy) · `currents` (streamline phase, dash marching) · `beams` (volumetric
shafts, 6,000 dust motes through one Plus-blended layer) · `dolly` (the
vertigo) · `prism` (the stop field) · `han` (the script axis) · `megapath`
(the single-path axis).

**The wonder register** (the visual spend, Acts III–IV): `eclipse` (totality
as a documented light sequence) · `blackhole` (Doppler-beamed orbits, lensed
stars) · `startrail` (long exposure as Plus accumulation) · `galaxy`
(60,527-shape record) · `avatar` (a line becomes a lit bust) · `fourier` (the
line re-drawn by its own choir) · `harmony` (the phase choir that rephases
exactly) · `bubble` (thin-film iridescence) · `caustics` · `orrery` ·
`storm` · `tesseract` · `forest` · `city` — plus the law machines (rounds
9–11: quantum, smoke, threebody, neural, ising, crystal, sandpile, lorenz,
gw, planck, pathtrace …) held as the deep reserve for cutdowns and the
making-of.

### 2.4 Family D — the ecosystem proof

- **Four real apps, migrated and enriched:** `three` (temporal 3D capture +
  social, 47 tests), `vavlt` (consent-first vault; phone + desktop, dark +
  light screenshot suites), `upxcale` (Lanczos-3 + unsharp render flow, 21
  tests), `snapsearch` (semantic photo search, CLIP ViT-B/32 behind a flag,
  38–47 tests). Each ships sheet + GIF + metrics rendered headless through
  the one renderer.
- **viewwsite** — the product page as a vieww widget tree on a `<canvas>`
  (wasm, ~2.7 MB → ~900 KB compressed), with the page's real copy as the
  canvas fallback for screen readers and crawlers. The page is the argument
  made of itself; so is the film.
- **Android with receipts:** 59.3 fps / 4.09 ms median on a 2019 Redmi Note
  7 Pro via `ci/mobile/device-suite.sh` — the numbers arrive as CI artifact
  files, and the film prints them without touching them.
- **iOS stays excluded** — disputed by the codebase's own docs; not in the
  film, per the honesty ledger.

---

## 3 · The feature shortlist — what the film uses

Every effect names the primitive that renders it. This is the complete
shortlist; anything not on it does not appear.

### 3.1 The spine

| feature | primitive | lands |
|---|---|---|
| the witness counter | a `say` buffer, `keep` state, one `Signal` | S04 → S10, returns S13 |
| session continuity | the counter never resets; the session chip ticks elapsed | every studio scene |
| the counter ladder | one increment per human touch, 1–7, never reset | S04·S05·S06·S08·S09·S10 |
| the session line | `Path` + dash-phase progressive stroke; forks ×3 at the world tour | S04 → S13, where it becomes the axis |

### 3.2 The look (the sheet-vf floor, and above it)

| feature | primitive | lands |
|---|---|---|
| the dark plate | `Color::hex` background, one bright anchor per frame | everywhere |
| frosted glass | `Filtered::blur().with_backdrop().tint()` | studio chrome, cards |
| dithered gradients | gradient + the aurora/prism dither grammar | backdrops, title |
| rim strokes + shadows | `Sketchbook::stroke_styled`, `path_shadow` | cards, the wordmark |
| crafted degradation | `FilterChain` ramp + grain + dust + 24-in-60 cadence | S01 only |

### 3.3 The motion grammar

| feature | primitive | lands |
|---|---|---|
| springs | `vieww-animation` `Spring` + `SpringPreset`, mid-flight `retarget` | wordmark drop, overlays, the unfold |
| type-on kinetics | `Text` + signal substring; scramble-to-settle | S02's question, captions |
| motion persistence | ghosts evaluated not remembered, direction-bucketed `with_blur_angle` | F2's parting, S13 |
| rack focus | blur ramp between two `Filtered` layers, both σ values printed live | S07's entry |
| the flow field | RK2-integrated tables, then pure phase | S02's condensing question |

### 3.4 The light (the blend-mode economy)

| feature | primitive | lands |
|---|---|---|
| additive light | one `Plus`-blended group per source (sketch `blended_layer`) | coronas, glints, the photon ring |
| volumetric shafts | one blurred Plus layer + cone-lit dust motes | the receipts-in-space bloom |
| glow accumulation | Plus across frames (the startrail economy) | F5's build-count bloom |

### 3.5 The receipts (data, not ornament)

| feature | primitive | lands |
|---|---|---|
| latency N | T-01 probe: edit committed → parsed → driven → raster | S04's caption, the manifest |
| 10 → 1 | the scrub's coalescing, counted live | S07 |
| ms · px | FrameStats on screen | S09 |
| build counts | `element.build_count()` surfaced as overlays | S07 |
| one region lights | damage tracking + PerformanceOverlay | S09 |
| 59.3 · 4.09 | device-suite CI artifacts, arriving as files | S12 |
| the manifest | SceneReport accumulated across 10,800 frames | S15's bars |

### 3.6 The reveal machinery

| feature | primitive | lands |
|---|---|---|
| the three planes | `Transform3::project_rect` fan + real tree glyphs from the live inspector | S13 F4 |
| the axis | the session line's final node extended into a spatial axis | S13 F2 |
| the signal apart | one node outside all three trees, one tether, reading 7 | S13 F6 |
| the fold-back | the unfold in reverse, spring-eased | S14 |
| the inspector, live | vieww-devtools on the studio, rendered angled | S11, S13 |

### 3.7 The reserve

The wonder register (§2.3) is budgeted, not decorative: the eclipse ladder is
the light-sequencing model for S13's bloom; startrail and blackhole set the
accumulation grammar for F5; galaxy is the standing proof that no frame in
this film stresses the rasterizer. The law machines stay in the library for
the extended cut — the keynote master keeps its spend focused on the product.

---

## 4 · The four acts — scene beat sheets

Sixteen scenes. Setups: **A** the wide (S04–S06) · **B** pure render (S07–S09)
· **C** handheld + scrcpy + laptop (S10). Continuity markers always visible in
studio scenes: the buffer tab `counter` · the session chip (elapsed, emitted)
· the witness counter · the session line (E-17).

### Act I · KI (起) — THE WORLD OF THE WAIT · 0:00–0:40

Introduce the world as it is. No villain, no product named, no logo. The
poverty of the image is rendered, not excused — and the act's first cut is
already the argument: silence and 60fps.

| # | scene | time | tier | content | effects | props |
|---|---|---|---|---|---|---|
| S01 | THE WAIT | 0:00–0:13 | A | a generic montage of the dev-world's texture — terminals, spinners, progress bars, a clock — degraded-crafted: grain, dust, defocus, **24-in-60 judder** (a 24Hz clock sampled inside a 60Hz render; the judder is the mechanism). An hour-counter ticks in the corner, its number emitted by the overlay's own clock. Nothing is named. | E-01, E-21 | aurora (degradation grammar), kinetic (hour-counter) |
| S02 | THE QUESTION | 0:13–0:20 | A | black. type-on: *"how long should it take to see what you built?"* — then the sentence lifts: 39 glyphs ride a deterministic flow field, accelerate, and **condense into one falling drop**. | E-02 | typo, settle |
| S03 | THE TITLE | 0:20–0:26 | A | the drop falls; where it lands, **vieww** springs up — the wordmark drop (E-03) with the underline's overshoot (E-04). One drop became a word. The film's genesis image: everything else grows from here. | E-03, E-04 | spring, ink (drop grammar), wordmark |
| S04 | FIRST PAINT | 0:26–0:40 | B | the studio: a buffer `counter.say`, three lines of say typed. The preview blooms alive. Caption: **"alive in N seconds"** — N from the T-01 probe, emitted. Tap → the counter reads **1**. First shown code of a Rust framework: `say` (K0 at 0:36). | E-05 | light (frosted floor), kinetic (type-on) — new build N-01 |

*Ki closes the world of the wait by opening its opposite: a door. 13+7+6+14 = 40s.*

### Act II · SHŌ (承) — THE SESSION · 0:40–2:05

One session, one artifact, carried across every surface. Comprehension
builds: alive → deep → composed → measured → carried → everywhere. No twist
lives here except the quiet ones (K0's say, K1's cadence, and K2 at the act's
last beat) — Shō deepens, it does not turn.

| # | scene | time | tier | content | effects | props |
|---|---|---|---|---|---|---|
| S05 | THE DESCENT | 0:40–0:51 | B | the same buffer, now as Rust — a scan-line sweep crosses and the code resolves (E-06), the counter never resetting through the switch. Caption: *"start in say · go as deep as you want."* Tap → **2**. | E-06 | morph |
| S06 | COMPOSE LIVE | 0:51–1:11 | B | keystrokes land per-keystroke in the preview. At ~1:03 a deliberate error renders in the preview as a **boundary, not a crash** (E-07); fixed in three keystrokes. The app grows a dial and a spring. Tap → **3**. | E-07 | scrub grammar, settle (fix lands with a per-glyph pop) |
| S07 | THE RECEIPT | 1:11–1:29 | B | rack focus pulls from editor to pure render (E-16, both σ values live). Overlays on: build counts bloom (E-09). The scrub: one frame window, ten writes driven through the real input pipeline — **10 → 1** — the coalescing scheduler made visible (E-08). | E-08, E-09, E-16 | rackfocus, scrub |
| S08 | STATE SURVIVES | 1:29–1:40 | B | edited mid-flight; the spring continues; state carries. *No effect on purpose — the carry is the shot.* Tap → **4**. | — | — |
| S09 | DAMAGE IN PIXELS | 1:40–1:51 | B | one write lights one region (E-10); FrameStats **ms · px** on screen, measured not typed. Tap → **5**. | E-10 | damage |
| S10 | WORLD TOUR | 1:51–2:05 | B→C | the buffer leaves: native window → browser DOM → the phone in hand. The session line forks ×3 (E-22). Desktop tap → **6**. Phone tap → **7** — **every surface updates** (K2). Caption: *"one session · every surface."* | E-11, E-22 | circuit (fork), new build N-07 |

*Shō closes on the counter at 7 — the session's witness — on three platforms at once. 11+20+18+11+11+14 = 85s.*

### Act III · TEN (転) — THE TWISTS · 2:05–2:35

The turn: three twists in thirty seconds, descending product → meta →
structure. Each recontextualizes a specific earlier image; none of them is a
conflict.

| # | scene | time | tier | content | effects | props |
|---|---|---|---|---|---|---|
| S11 | THE MIRROR | 2:05–2:13 | B | devtools turns on the studio itself; its own damage lights up (E-12). The inspector shows the studio's real trees. **The IDE you've been watching is a vieww app** (K3). | E-12 | new build N-05 |
| S12 | THE RECEIPTS | 2:13–2:21 | A+C | the benchmark card assembles in 3D around real widget props (E-13/E-18), the CI timeline drawing as a path (E-19): **59.3 · 4.09 · Redmi Note 7 Pro (2019)** — numbers arriving as files. Caption: *"that's a 2019 phone."* | E-13, E-18, E-19 | receipts, kinetic (count-ups) — new build N-04 |
| S13 | THE UNFOLD | 2:21–2:35 | A+B | the reveal proper (E-20), eight frames compressed to six + the fold-back that opens Act IV. See §4.1. | E-17, E-20 | globe, unfold, circuit — new build N-03 |

*TEN closes having recontextualized: the studio (K3), the platform story
(K2 → S12), and the whole session (K4). 8+8+14 = 30s.*

### Act IV · KETSU (結) — THE RECONCILIATION · 2:35–3:00

The new understanding settles. Kishōtenketsu closes by returning to the
opening image, transformed — and this film's opening image was a door.

| # | scene | time | tier | content | effects | props |
|---|---|---|---|---|---|---|
| S14 | THE FOLD-BACK | 2:35–2:39 | A | the unfold in reverse, fast, spring-eased — the world flattens to **one surface: the studio, as it was in S04.** Everything you watched was one thing. | E-20 (F7) | globe/unfold reversed |
| S15 | THE END CARD | 2:39–2:52 | A | the wordmark; `vieww.dev`; the install lines, single-sourced and printed by the tool they install; **the manifest bars** — frames, shapes, glyph runs, layers, springs, blurs, paths — the film's own audit, emitted at render. At +4s the sting: *"this film was rendered with vieww."* (K5 at 2:43) | E-15 | endcard — new build N-06 |
| S16 | THE LOOP | 2:52–3:00 | A | the camera pulls back from the end card: it is **one more buffer open in the studio** — the session never ended; it rendered the film. The session chip reads **3:00**, emitted by the session clock. The buffer tab says `endcard`. Fade on the wordmark's reflection. | E-15 variant | new build N-08 |

*Ketsu reconciles: the tool that escaped the wait is the tool that rendered
the film about escaping it. 4+13+8 = 25s. Total: 40+85+30+25 = 180s.*

### 4.1 The reveal — S13/S14, frame by frame

Layer colors inherited from the E-20 storyboard: description `#58a6ff` ·
identity `#3fb950` · geometry `#ffa657` · signal `#bc8cff` · damage `#f85149`.
Durations close: 3+1+2+3+2+3 = 14s (S13) + 4s (F7, S14).

| F | time | content | data source (no fabrication) |
|---|---|---|---|
| F1 | 2:21–2:24 | **the return** — the counter full-frame, flat, reading **7**; the session line arrives node by node | the real counter widget, Tier B |
| F2 | 2:24–2:25 | **the turn** — ghosts part by pixels; the completed line extends into an axis. Caption: *"what you watched, from outside."* | the line's real node list; the axis is E-17's final node (reversible decision — fallback: unmotivated pull-back) |
| F3 | 2:25–2:27 | **depth opens** — root pull-back, spring-eased; a floor plane arrives | Transform3 |
| F4 | 2:27–2:30 | **the unfold** — three planes fan and are labelled. Caption: *"description · identity · geometry."* | the counter's real three trees, glyph-rendered from the live inspector |
| F5 | 2:30–2:32 | **receipts in space** — build counts bloom in sequence; tap 7's damage rect pulses. Caption: *"every number was measured."* | `element.build_count()` live; tap 7's actual damage rect from the session |
| F6 | 2:32–2:35 | **the signal** — one node apart from all three trees, one tether, reading **7**. Caption: *"state lives outside the tree."* — the thesis, spatialized | the real Signal handle |
| F7 | 2:35–2:39 | **the fold-back** — the unfold in reverse, fast; the world flattens to the studio as it was | — |

Disciplines inherited whole: annotations bloom in sequence, never
simultaneously · key info center-frame for the vertical cutdowns · angular
velocity capped · every beat captioned.

### 4.2 The session rail — the counter ladder

One number per human touch, never reset: **1** born (S04, say) · **2** rust
(S05) · **3** composed (S06, after the fix) · **4** carried (S08) · **5** the
damage write (S09) · **6** desktop (S10) · **7** the phone (S10) — and **7
returns in space** at F6, the only number allowed to appear twice. The camera
and the setup change; the session never restarts. **The counter is the
witness.**

### 4.3 The transition grammar

| act | grammar | devices |
|---|---|---|
| I | abrupt, mechanical | HARD CUT (into silence · into 60 — S01→S02) · FADE UP |
| II | non-cuts — in-frame state changes | CONTINUE · KEEP TYPING · RACK FOCUS · OVERLAY ON · LIFT OFF |
| III | cinematic | CUT · the camera turns around |
| IV | settling | the fold-back · FADE (from the reflection) |

### 4.4 The numbers rail

| figure | lands | source |
|---|---|---|
| hours on the counter | S01 | the overlay's own clock, emitted |
| **N** | S04 | preview-latency probe (T-01), three samples, median |
| **10 → 1** | S07 | the scrub, measured live |
| **ms · px** | S09 | FrameStats on screen |
| **7** | S10 | the witness, every surface |
| **59.3 · 4.09** | S12 | device-suite CI artifacts |
| **7 · in space** | F6 2:32 | the signal, read live |
| **MANIFEST** | S15 | SceneReport accumulated across the render: 10,800 frames @60 · shapes · glyph runs · layers · springs · blurs · paths — plus the bench identity |
| **3:00** | S16 | the session chip, emitted |

---

## 5 · New builds and the build board

The film composes primarily from the 90 proven plates plus the studio itself;
eight bespoke builds cover what the story demands and the library does not
own. Each is a workspace crate in the film tree, each ships the three-artifact
receipt set, and each names the plates it fuses.

### 5.1 The eight new builds

| id | build | scope | fuses | blocks |
|---|---|---|---|---|
| **N-01** | **the door** | S04 whole: the studio's first-paint as a premium-UI composite — frosted chrome, the buffer typing, the preview blooming alive, N emitted, tap → 1 | `light` floor + `kinetic` type-on + the premium-test lineage (52.5 ms mean, 0 overflows) | S04 |
| **N-02** | **the drop** | S02→S03 whole: the question's glyphs lift, ride the field, condense to one drop; the drop lands and the wordmark springs up — one continuous shot from typography to settle | `typo` + `spring` + `ink`'s drop grammar | S02, S03 |
| **N-03** | **the axis** | the reveal, film-resolution: the return, the turn, depth, the unfold, receipts in space, the signal, the fold-back — the counter's real trees, tap 7's real damage rect | `globe` + `unfold` + `circuit` + the receipt overlays | S13, S14 |
| **N-04** | **the receipts card** | S12: the benchmark card assembling in 3D around real widget props; the CI timeline as a self-drawing path; the numbers arriving as files | `receipts` + `kinetic` count-ups, CI-fed | S12 |
| **N-05** | **the mirror** | S11: devtools on the studio itself — the inspector's live views rendered angled, the studio's own damage lighting up | the devtools inspector + `damage` overlays | S11 |
| **N-06** | **the end card** | S15: wordmark, install lines (single-sourced from the `vieww-cli` constant), manifest bars from accumulated SceneReports, the sting, the hold | `endcard` + `wordmark` | S15 |
| **N-07** | **the world tour rail** | S10: the three-surface composite — native window, browser DOM, the phone in hand (scrcpy) — the line forking ×3, all surfaces updating on tap 7 | `circuit` fork + the platform switcher staging | S10 |
| **N-08** | **the loop** | S16: the pull-back from end card to studio — the end card as one more buffer, the session chip reading 3:00, the reflection fade | N-06 + N-01's staging reversed | S16 |

### 5.2 The build board

| task | status | scope | blocks |
|---|---|---|---|
| **T-01 latency probe** | DESIGNED | keystroke → preview-visible along the real pipeline; N = raster − edit; three samples from the film's typing; median → `numbers.json` → S04 + manifest; doubles as a CI budget test | S04's caption |
| **T-02 the crafted wait** | BUILD | the S01 montage, procedural-first (no licensing, no product named): terminals/spinners/progress as rendered texture + the E-21 degradation pass + the E-01 hour-counter | S01 |
| **T-03 the session spine** | SCRIPTED | the beat-by-beat script above; setups A/B/C; carries E-05..E-10, E-16, E-17 and the counter ladder — **critical path: unblocks six scenes** | S04–S10 |
| **T-04 receipts from CI** | BUILD | the benchmark card pulls 59.3/4.09 from device-suite artifacts — numbers arrive as files | S12 |
| **T-05 the mirror** | BUILD | devtools on the studio itself | S11 |
| **T-06 the reveal** | BUILD | N-03 + N-04 + N-05; needs T-03's session data (the real trees, the real damage rect) | S11–S14 |
| **T-07 the close** | BUILD | N-06 + N-08; the end-card system; install lines single-sourced | S15, S16 |
| **T-08 master & cutdowns** | LOCKED (fps, install) | 1080p@60 master, 10,800 frames; manifest; render gate curls the install URLs; vertical cutdowns after the master | delivery |

Order: T-01 and T-03 first (T-03 is the critical path); T-02 parallel; then
T-04/T-05, then T-06/T-07, then T-08. Nothing in this board requires a design
decision — the graph is closed; what remains are deployment gates: vieww.dev
answers · the installer ships · the CI artifacts exist · then the master
renders.

---

## 6 · Render pipeline, receipts, and risk

### 6.1 Master spec

| parameter | value |
|---|---|
| resolution | 1920×1080 (4K cutdowns optional, chunked) |
| cadence | 60fps master · the wait at 24-in-60 (true 2-3-2-3, sampled in-render) |
| frames | 10,800 (emitted, never typed — two incidents already proved the rule) |
| renderer | `FrameDriver` + `NativeRenderer`, headless, fixed clock |
| encode | raw RGBA → `ffmpeg -c:v libx264 -pix_fmt yuv420p -crf 18` |
| budget | hero-class frames 149–171 ms → a full master ≈ 30–45 min; galaxy-class 101 ms proves headroom; plan 60 min per pass with iterations |
| reproducibility | byte-identical per bench; glyph rasterization follows the host's fonts (`use_system_fonts`) — the manifest names the bench |

### 6.2 The receipts and audit loop

- **Per-scene receipt sets**: `metrics.txt` with the measured `gif=` line ·
  `sheet.png` contact sheet (`fps=10,scale=480:-1,tile=4x4`) · `anim.gif` per
  scene — the house recipe (two-pass palette, floyd-steinberg; settled by the
  audit loop, not chosen).
- **The VLM audit with intent-carrying prompts** — the fadeaway lesson: the
  same pixels scored 2/10 context-free and 9/10 with the design intent named.
  Dark plates audit with their intent attached; dark plates keep one bright
  anchor on stage (the lit contour, the photon ring, the drop).
- **Full-res audits for fine detail** (the han lesson: taper invisible at
  sheet scale, clear at full frame).
- **The census**: `SceneReport.open_subpath_fills` runs on every scene;
  `unsupported_blends` must read **0** on the master (the no-post honesty
  counter).
- **The gates**: the render gate curls the install URLs before shipping · the
  manifest emits itself from accumulated SceneReports · iOS excluded · screen
  readers never claimed.

### 6.3 The risk register

| risk | evidence | mitigation |
|---|---|---|
| glyph determinism is per-bench | round 11: 17 of 90 sheets moved between benches; `use_system_fonts` reads the host | render the master on one named bench; the manifest carries the bench identity; `FILM_EMBEDDED_FONTS=1` kept as the fallback for plates that can use it |
| 4K memory, not speed | hero4k SIGKILLed at 4-frame chunks on the 4 GB bench | master at 1080p; 4K cutdowns in chunked passes or on a bigger box |
| audit false positives on dark plates | fadeaway 2/10 vs 9/10, same pixels | intent-carrying prompts; bright anchors; full-res second pass |
| the axis decision untested | handoff: perceptual verdicts withdrawn with the gray-box medium | the rich pass (T-03/T-06) is the instrument; fallback pre-scoped: unmotivated pull-back |
| "nothing was compiled" wording | `06-say.md`: say → codegen → rustc → cdylib → dlopen | caption is **"alive in N seconds"**, N measured; the ledger is updated (§7.2) |
| the wait montage licensing | generic footage needs sourcing | procedural-first: render the wait in-house — it also quietly reinforces the meta-move |
| 10,800-frame render time | hero budget 149 ms mean | 30–45 min per pass is acceptable; iterate on scene crops, master rarely |
| sound deferred | author's job; mp4 has no audio track | the cut works muted; every beat captioned |
| iOS claims | disputed by the repo's own docs | excluded, per the ledger |
| scope creep in the wonder register | 90 plates are seductive | the keynote master spends only where §3 shortlists; the law machines wait for the extended cut |

---

## 7 · Appendix

### 7.1 What changed from scene graph v0.7, and why

Inherited systems (session rail, numbers rail, effects inventory, transition
grammar, build board) carry over with node ids intact. The structural changes:

| change | v0.7 | this plan | why |
|---|---|---|---|
| act frame | three acts (45+95+40) | **four explicit movements** (40+85+30+25) | the strategy note named kishōtenketsu as the engine; promoting it to the skeleton gives Ten its own movement and Ketsu its own breath — the twist cascade and the reconciliation stop competing for the same 40 seconds |
| the reveal's home | all of E-20 inside S12 (20s) | F1–F6 in S13 (Ten), **F7 opens Ketsu** | the fold-back is reconciliation, not twist — it belongs to the act whose job is settling |
| the close | end card (6s) | end card + **the loop** (S16, 8s) | kishōtenketsu closes by returning to the opening image transformed; the film's first image was the studio door — S16 shows the door again, now holding the film itself |
| the title | question → wordmark (10s, one scene) | question → **the drop** → wordmark (13s, two beats) | the drop is the film's genesis image (one drop → a word → a world), reusing the lab's two strongest genesis arcs; it also plants the reflection motif S16 fades on |
| first paint | S03 at 0:22–0:45 | S04 at 0:26–0:40 | Ki needs its 40s; the door arrives 4s later and loses none of its silence |
| S04 caption | "alive in N seconds — nothing was compiled" | **"alive in N seconds"** | the studio's own docs describe the codegen+compile pipeline; the dropped phrase was a claim the product's documentation does not make |
| the wait | generic montage (footage TBD) | **procedural-first** | no licensing, no sourcing risk, and the degraded world is itself a vieww render — the meta-move starts in frame one |

Everything else — the tiers, the counter ladder, the five rules, the install
lines, 60fps, the sting, the escalation discipline — is inherited unchanged.

### 7.2 The honesty ledger (updated)

1. Android — verified on a real device · iOS — disputed by our own docs; not
   in the film
2. Screen readers — wired, never heard; not claimed · no widget wall · no
   mockups
3. The wait names no product · no number is typed by a human
4. No effect in post — everything visible in a product shot was rendered by
   the product · tier C may be graded; carries no claims
5. The edit only chooses where the knife falls — selects, never fabricates
6. The 3D is the framework's own transforms — no external engine
7. The reveal is the devtools inspector, live — real trees, not a fabrication
8. If an annotation cannot be sourced live, it does not appear
9. The film runs at 60 — the cadence the product delivers; the wait at
   24-in-60, deliberately
10. Install lines single-sourced, printed by the tool they install · the
    render gate curls the URL
11. **"alive in N seconds" is the claim; N is measured, and the say pipeline
    compiles (codegen → cdylib → dlopen) — the studio escapes your project's
    build, not compilation itself**
12. **Determinism is per-bench** (the host's fonts shape the glyphs); the
    manifest names the bench that rendered it

### 7.3 Open questions for the author

1. **The axis (F2)** — ratified but reversible; the rich pass is the
   instrument, the fallback is pre-scoped.
2. **The loop (S16)** — new: does the final recursion read as closure or as
   one twist too many? The fallback is a plain hold on the end card's
   reflection (cuts S16 to 5s and gifts 3s to S15's manifest bars).
3. **The wait's texture** — procedural-first is proposed; if the author has
   camera footage with no claims attached, tier C remains available for the
   montage's warmth, graded, carrying no numbers.
4. **The drop (S03)** — whether the wordmark springs from the impact point
   (proposed) or the drop *becomes* the wordmark's ink as it falls (the
   alternative; one line of the two reads better in the rich pass).

*The graph is closed. The film is a build program. First move: T-01 and the
session spine.*
