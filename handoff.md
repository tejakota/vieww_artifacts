Here it is — the complete handoff. Everything a fresh session needs: the project model, the strategy, the v0.7 checkpoint, the animatic state including the aesthetic verdict and what it changed, the verified API facts, and the next moves. Save as `FILM_HANDOFF.md` (or drop it straight into the new session as the opening message):

````markdown
# vieww launch film — session handoff
**Version:** handoff-1 · follows scene graph v0.7 + animatic pass 1
**Purpose:** complete context for continuing the launch-film work in a new session.
Everything below is settled unless explicitly marked OPEN.

---

## 0 · How to use this document

You (the assistant) are collaborating with the framework's author on a **launch
keynote film** for **vieww**. The author runs all code locally and pastes
results back. This doc is your full state. Sections:

- §1 The project (what vieww is — the mental model)
- §2 The mission (the film, and the meta-move)
- §3 The strategy (structure + the five rules)
- §4 The scene graph v0.7 — **the checkpoint** (scenes, rails, effects, board)
- §5 Artifacts inventory (what files exist where)
- §6 The animatic — state, receipts, **the aesthetic verdict**, the reframe
- §7 Verified API cheatsheet (facts proven by compiling code)
- §8 Next actions (ordered)
- §9 Collaboration protocol (what reaches you, markup conventions)

---

## 1 · The project — what vieww is

**vieww** is a cross-platform UI framework in Rust with a **three-tree
architecture** (Flutter's shape, Rust's body):

| tree | crate | job |
|---|---|---|
| Widget | `vieww-widget` | describe the UI (one frame) |
| Element | `vieww-element` | identity + state (across frames) |
| Render | `vieww-render` | layout + paint (constraints-down/size-up, slivers, semantics) |

Key facts:
- **Signals, outside the tree.** `runtime().signal(v)`; widgets hold handles;
  reading during `build` IS the subscription. Rebuild is pull-based and
  coalesced (ten writes → one rebuild). This is the spine of the whole film.
- **The renderer is vieww's own**, not vello: `vieww-paint`'s `native` module —
  CPU reference rasterizer (4×4 supersampling, gradients, blurs, shadows, all
  28 blend modes, glyph caches), presented via `vieww-hal`'s raw Vulkan
  swapchain in `vieww-platform-winit`. Feature-gated: `native`.
- **Android verified with receipts**: 59.3fps / 4.09ms median on a Redmi Note
  7 Pro (2019, Adreno 612), via `ci/mobile/device-suite.sh`.
- **iOS is disputed** by the codebase's own docs — excluded from all claims.
- **viewwstudio** is the IDE, itself a vieww app (dogfooded): preview renders
  in seconds, escaping build time. An embedded language **`say`** exists
  (`vieww-say-codegen`) — codegen to idiomatic Rust; the studio likely
  interprets it for the fast loop (exact preview mechanism was never fully
  pinned down — see §8 OPEN-1).
- Culture: **receipts**. Tests named as claims, assertions on pixels not
  stored values, honesty about gaps. The film inherits this culture.

Crates map (top→bottom): say DSL → codegen → Rust views; widget/element/
render; paint (CPU raster + damage) / scene / render-graph / render-planner
(CPU|GPU|hybrid by budget) / gpu / hal (Metal/D3D12/Vulkan/null); shaders;
platform-winit (desktop+iOS+Android), platform-web-dom (real DOM), web;
foundation; runtime/gestures/scroll/animation/interaction; cli/build/reload/
devtools/test-harness/accessibility/plugin.

---

## 2 · The mission — the launch film

A **3:00 keynote film** for launch, and the meta-move that defines it:
**the film's motion graphics layer is rendered by vieww itself** (headless
`FrameDriver`, fixed clock, PNG/raw frames out; re-renderable to the byte).
"The graphics in this film were rendered with vieww" is the closing sting.

What the film is allowed to fake: **nothing in a product shot.** Tier C camera
footage may be color-graded (carries no claims). Everything else is the
product rendering itself at capture.

Tiers:
- **A** — headless render, deterministic
- **B** — scripted replay through the real input pipeline (deterministic ≠ fake)
- **C** — camera capture (warmth, no claims)

---

## 3 · The strategy

### Structure — "the four act"
Three-act frame, powered by **kishōtenketsu** (the four-movement Japanese
structure whose engine is the *twist that recontextualizes* — no villain):

- **Act I · THE WAIT** (0:00–0:45 · 45s) — the cost the film argues against
- **Act II · THE SESSION** (0:45–2:20 · 95s) — one session, three platforms;
  continuity claimed and shown, **not** one take (deliberate decision:
  "claim session continuity" instead of literal no-cut)
- **Act III · THE REVEALS** (2:20–3:00 · 40s) — twists descend product → meta

Durations close: **45 + 95 + 40 = 180s.**

The twists (K):
- **K0** 0:36 — a Rust framework whose first shown code is `say`, not Rust
- **K1** act break — the wait is obsolete
- **K2** 2:16 — tap on the phone; the desktop felt it (7 everywhere)
- **K3** 2:25 — the IDE you've been watching is a vieww app
- **K4** 2:47 — this film was rendered with vieww

### The five rules (engraved on the graph)
1. **No number is typed by a human** — every figure is emitted by the pipeline
   it describes (probe, element tree, FrameStats, CI artifacts, the runtime
   itself). Two incidents already proved the rule: 4320→10,800 when 60fps was
   locked; 2550→2551 when the mount was forgotten (the receipt's *expectation*
   was wrong, not the code — expect `frames + 1`).
2. **No effect is added in post** — visible in a product shot ⇒ the product
   rendered it.
3. **The edit only chooses where the knife falls** — cuts select, never
   fabricate. Within the session, strips are *deliberately unfelt*.
4. **Richness escalates deliberately — every escalation is a demo.** Act I is
   deliberately poor (E-21 degradation + 24-in-60 judder); the cut into S02 is
   silence **and** 60fps smoothness; Act III spends the visual budget. The last
   40 seconds carry it.
5. **If an annotation cannot be sourced live, it does not appear.**

Plus: **the cut works muted** (sound is deferred — added later by the author;
the mp4 currently has no audio track at all, which satisfies this by
construction).

Transition grammar by act: I = abrupt/mechanical (HARD CUT, FADE UP);
II = non-cuts, in-frame state changes (RACK FOCUS, CONTINUE, KEEP TYPING,
OVERLAY ON, LIFT OFF); III = cinematic (CUT, FADE).

### Locked decisions (do not reopen without cause)
- Runtime **3:00** · master at **60fps** · the wait at **24-in-60** (a 24Hz
  clock sampled inside a 60Hz render — true 2-3-2-3 holds; the judder is the
  mechanism, E-21's fourth word)
- **say-first**: S03 is first paint in `say`; S04 is the descent to Rust
  ("start in say · go as deep as you want")
- The wait is a **generic montage** — no product named, no logo
- **Sound: later** (author's job)
- Install lines locked, single-sourced (intended to live as a constant in
  `vieww-cli`, read by both film and site — build item):
  `curl -fsSL https://vieww.dev | sh` and `irm https://vieww.dev/install.ps1 | iex`
  A render gate curls the URL before shipping (no dead links).
- The bookend: S06's counter returns in S12, now legible — and unfolded
- The reveal's trigger (F2 "the axis") is ratified **but reversible**:
  fallback is an unmotivated pull-back. The perceptual verdict was never
  obtained (see §6) — treat as untested.

---

## 4 · The scene graph v0.7 — THE CHECKPOINT

Full artifact: `scene_graph_v07.svg` (and v0.1–v0.6 for history). Layout
contract: acts left→right, scenes left→right inside them, transitions as
vertical strips (label reads top→bottom), arrows carry the flow — left→right
only. Six systems, all cross-referenced: scenes, session, transitions+effects,
numbers, reveal, build board.

### 4.1 The twelve scenes

| # | scene | time | tier | content | E | T |
|---|---|---|---|---|---|---|
| S01 | THE WAIT | 0:00–0:12 | C+A | generic degraded montage; hour-counter; 24-in-60 | E-01,E-21 | T-02 |
| S02 | QUESTION → TITLE | 0:12–0:22 | A | "how long should it take to see what you built?" · wordmark spring drop | E-02/03/04 | — |
| S03 | FIRST PAINT · say | 0:22–0:45 | B | three lines of say typed → preview alive · "alive in N seconds — nothing was compiled" (N emitted) · tap→**1** | E-05 | T-01 |
| S04 | THE DESCENT | 0:45–0:57 | B | same buffer as Rust, state held ("doorway, not a wall") · tap→**2** | E-06 | — |
| S05 | COMPOSE LIVE | 0:57–1:19 | B | keystrokes land per-keystroke; deliberate error at 1:07 renders in the preview (boundary, not crash); fixed; tap→**3** | E-07 | — |
| S06 | THE RECEIPT | 1:19–1:39 | B | rack focus into pure render; overlays on; scrub driven 10× in one frame window: 10 writes · 1 rebuild · 1 damage rect | E-08/09/16 | — |
| S07 | STATE SURVIVES | 1:39–1:51 | B | edited mid-flight, spring continues, state carries · tap→**4** · *no effect on purpose — the carry is the shot* | — | — |
| S08 | DAMAGE IN PIXELS | 1:51–2:04 | B | one write lights one region; FrameStats ms·px on screen · tap→**5** | E-10 | — |
| S09 | WORLD TOUR | 2:04–2:20 | B→C | native window · browser DOM · Redmi in hand · desktop tap→**6** · phone tap→**7**, all surfaces update · line forks ×3 | E-11,E-22 | T-03 |
| S10 | THE MIRROR | 2:20–2:30 | B | devtools on the studio itself; its own damage lights up | E-12 | T-05 |
| S11 | THE RECEIPTS | 2:30–2:40 | A+C | benchmark card assembles in 3D; CI timeline draws as a path; 59.3 · 4.09 · Redmi Note 7 Pro, from CI artifacts | E-13/18/19 | T-04 |
| S12 | THE CLOSE | 2:40–3:00 | A+B | the reveal F1–F8 (below) | E-17/20/15 | T-06 |

Transitions (vertical strips): T01 HARD CUT (into silence · into 60) ·
T02 FADE UP · AB1 · T04 CONTINUE · T05 RACK FOCUS (E-16) · T06 KEEP TYPING ·
T07 OVERLAY ON · T08 LIFT OFF · AB2 (the camera turns around) · T10 CUT ·
T11 FADE (to the reveal).

### 4.2 The session rail — the counter ladder

Continuity markers always visible: the buffer tab `counter` · session chip
(elapsed, emitted) · the witness counter · the session line (E-17).

**Ladder — one number per human touch, never reset:**
1 born (S03, say) · 2 rust (S04) · 3 composed (S05, after the fix) ·
4 carried (S07, survives refresh) · 5 the damage write (S08) · 6 desktop
(S09) · **7 the phone (S09)** — then 7 returns in space at F6.
"The camera and the setup change; the session never restarts — the counter is
the witness." Setups: **A** the wide (S03–S05) · **B** pure render (S06–S08) ·
**C** handheld + scrcpy + laptop (S09).

### 4.3 The reveal — E-20, eight frames (S12, 2:40–3:00)

Durations close: **3+1+2+3+2+1+2+6 = 20s.** Full artifact:
`e20_storyboard_v1.svg`. Layer colors: description #58a6ff · identity #3fb950 ·
geometry #ffa657 · signal #bc8cff · damage #f85149.

| F | time | content | note |
|---|---|---|---|
| F1 | 2:40–2:43 | the return — counter flat, full-frame, reading 7; the line arrives node by node | Tier B, FREE |
| F2 | 2:43–2:44 | **the turn** — ghosts part by pixels; the completed line extends into an axis. Caption: "what you watched, from outside." | **the axis — reversible** |
| F3 | 2:44–2:46 | depth opens — root pull-back, spring-eased; floor plane | |
| F4 | 2:46–2:49 | the unfold — three planes fan, labelled; real tree glyphs. Caption: "description · identity · geometry" | |
| F5 | 2:49–2:51 | receipts in space — build counts bloom in sequence; tap 7's damage rect pulses. Caption: "every number was measured" | |
| F6 | 2:51–2:52 | **the signal** — one node apart from all three trees, one tether, reading 7. Caption: "state lives outside the tree." | the thesis, spatialized |
| F7 | 2:52–2:54 | the fold-back — the unfold in reverse, fast; world flattens to one surface | closure over the parked trees-assemble alternative |
| F8 | 2:54–3:00 | end card — wordmark, vieww.dev, install lines (single-sourced), manifest bars, the sting at +4.0s: "this film was rendered with vieww." | |

Disciplines: annotations bloom in sequence, never simultaneously · key info
center-frame for vertical cutdowns · angular velocity capped · every beat
captioned (muted rule). Data sources (no fabrication): `element.build_count()`
live · tap 7's actual damage rect from the session · the real Signal handle ·
the counter's real three trees · live devtools inspector views, rendered angled.

### 4.4 The effects inventory — 21 effects (E-14 folded into E-20)

9 FREE · 12 BUILD. Build items fold into the T-tasks — no double-counting.

| id | effect | scene | rendered by | status |
|---|---|---|---|---|
| E-01 | hour-counter tick | S01 | signal overlay, A over C | BUILD |
| E-02 | type-on question | S02 | Text + signal substring | FREE |
| E-03 | wordmark spring drop | S02 | vieww-animation Spring | FREE |
| E-04 | underline overshoot | S02 | Spring, higher ω | FREE |
| E-05 | first paint | S03 | the preview loop itself | FREE |
| E-06 | say→rust morph, state held | S04 | studio language switch | FREE |
| E-07 | error renders in preview | S05 | ErrorPlaceholder | FREE |
| E-08 | the scrub 10→1 | S06 | scheduler coalescing made visible | BUILD |
| E-09 | build-count overlays | S06 | element.build_count() surfaced | BUILD |
| E-10 | damage flash | S08 | vieww-paint damage + PerformanceOverlay | BUILD |
| E-11 | platform morph | S09 | one session, three backends | FREE |
| E-12 | self-inspection glow | S10 | vieww-devtools on the studio | BUILD |
| E-13 | benchmark card | S11 | choreography, perspective via E-18, CI-fed | BUILD |
| E-15 | end card + sting | S12 | title-card system; manifest + install lines | BUILD |
| E-16 | rack focus | S06 strip | vieww-effects blur ramp on layers | FREE |
| E-17 | session line drawing itself | S03–S12 | vector path, progressive stroke; forks ×3 at S09; **its final node (2:43) is E-20's axis** | BUILD |
| E-18 | card assembles in 3D | S11 | perspective transforms, spring-eased | BUILD |
| E-19 | CI timeline as path | S11 | stroke progression, artifact-fed | BUILD |
| E-20 | THE REVEAL | S12 | F1–F8; the axis; the signal apart; the fold-back; live inspector | BUILD · STORYBOARDED |
| E-21 | grain · dust · defocus · judder | S01 | tier A degradation — texture AND cadence | BUILD |
| E-22 | line forks ×3 | S09 | vector path split | FREE |

### 4.5 The numbers rail — "no number typed by a human"

| figure | lands | source |
|---|---|---|
| **N** | S03 0:36 | preview-latency probe, run when the film renders |
| **10 → 1** | S06 1:27 | the scrub, measured live |
| **ms · px** | S08 1:56 | FrameStats on screen |
| **7** | S09 2:16 | the witness, every surface |
| **7 · in space** | F6 2:51 | the signal, read live (only number appearing twice) |
| **59.3 · 4.09** | S11 2:35 | ci/mobile device-suite artifacts |
| **MANIFEST** | S12 | 10,800 frames @60 · springs · blurs · paths · 3D · rects — the film's own audit, emitted at render, printed on the end card |

**SceneReport discovery (filed for T-06):** `render_to_pixels` also returns a
`SceneReport` with per-frame counted work (`shapes, images, glyph_runs,
layers, unsupported_blends`). Accumulate across frames → the manifest emits
itself from the renderer's account. `unsupported_blends` doubles as a no-post
honesty counter.

### 4.6 The honesty ledger (all ten lines)

1. Android — verified on a real device · iOS — disputed by our own docs; not in the film
2. Screen readers — wired, never heard; not claimed · no widget wall · no mockups
3. The wait names no product · no number is typed by a human
4. No effect in post — everything visible in a product shot was rendered by the product · tier C may be graded; carries no claims
5. The edit only chooses where the knife falls — selects, never fabricates
6. The 3D is the framework's own transforms — no external engine
7. The reveal is the devtools inspector, live — real trees, not a fabrication
8. If an annotation cannot be sourced live, it does not appear
9. The film runs at 60 — the cadence the product delivers; the wait at 24-in-60, deliberately
10. Install lines single-sourced, printed by the tool they install · the render gate curls the URL

### 4.7 The build board

| task | status | scope | blocks |
|---|---|---|---|
| **T-01 latency probe** | DESIGNED | times keystroke→preview-visible along the real pipeline; N = raster − edit; three samples from the film's typing; median → manifest; doubles as CI budget test (idle_cost tradition) | S03 |
| **T-02 wait montage** | BUILD | generic footage; degradation pass (E-21 + E-01) renders over it | S01 |
| **T-03 session spine** | SCRIPTED | the beat-by-beat script is settled (§4.1+4.2); setups A/B/C; carries E-08/09/10/16/17 | S03–S09 |
| **T-04 receipts from CI** | BUILD | benchmark card pulls 59.3/4.09 from device-suite artifacts — numbers arrive as files | S11 |
| **T-05 mirror** | BUILD | devtools on the studio itself | S10 |
| **T-06 master & cutdowns** | LOCKED (fps, install) | 60fps master; E-20 storyboard folded; E-15 end card + manifest; cutdowns after master | S12 · delivery |

No design decisions remain open. What remains are deployment gates:
vieww.dev answers · the installer ships · the CI artifacts exist · then the
master renders. **The graph is a build program.**

---

## 5 · Artifacts inventory

| file | what it is |
|---|---|
| `scene_graph_v0{1..7}.svg` | the graph, iterated; **v0.7 is current** (3000×2300, GitHub-dark palette) |
| `e20_storyboard_v1.svg` | the reveal, eight frames, full detail |
| `examples/animatic/` | working crate: `Cargo.toml` + `src/main.rs` (see §6) |
| `target/film/animatic.mp4` | pass-1 output: 960×540 smoke, 42.50s, 60fps, yuv420p, **no audio track** |
| `sheets/*.png` | extracted key frames (axis, cut_01/02, foldback, endcard) |
| beat table | lives as constants at the top of `animatic/src/main.rs` — the film's clock for the two sequences under test |

The animatic's Cargo.toml (author's own):
```toml
[package]
name = "animatic"
description = "Interactive animatic test"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish = false

[dependencies]
vieww = { workspace = true, features = ["native"] }
vieww-animation.workspace = true
vieww-element.workspace = true
vieww-foundation.workspace = true
vieww-platform-winit.workspace = true
vieww-widget.workspace = true

[lints]
workspace = true
```
**Read the dependency list as a spec**: the author added `vieww-animation`
(real springs) and `vieww-platform-winit` (a live window) — signals about
ambition level that pass 1 ignored (see §6).

Runbook that produced the mp4:
```bash
cargo run -p animatic --release -- --smoke     # 960×540, ~5GB raw frames
ffmpeg -y -f rawvideo -pix_fmt rgba -s 960x540 -r 60 \
  -i target/film/animatic/frame_%04d.rgba \
  -c:v libx264 -pix_fmt yuv420p -crf 18 target/film/animatic.mp4
```
ffmpeg 8 notes: use `-update 1` for single-image extracts; `-vsync` is
deprecated (use `-fps_mode`). Contact sheets via
`-vf "fps=10,scale=480:-1,tile=4x4"`.

---

## 6 · The animatic — state and the verdict that changes the next pass

### 6.1 Mechanical state: PROVEN
- Pipeline end-to-end: scene → native rasterizer → raw frames → h264.
- **Byte-identical across two runs** — the film is reproducible; no reshoots.
- Build count **2551 = 1 mount + 2550 frames** — the spine ran the film.
  (The receipt's original expectation "2550" was the assistant's error —
  a typed number that should have been derived. Expect `frames + 1`.)
- The rasterizer seam is **CLOSED**, verbatim from the parity suite (see §7).
- Stills audited via contact sheets: end card ✓, cut pair ✓ (frame 720 is
  black + caret by design — first character lands ~frame 735), fold-back
  staging ✓, wait frame **arithmetic-verified** (14:24 · 1852 h at frame 719),
  axis composition present ✓.

### 6.2 The aesthetic verdict — THE PIVOT
> Author, after watching: **"Almost the whole video is not what I was
> expecting. It is at lower than the basic that I have guessed."**

Causes (both real):
1. **Written blind against the safest API subset** — the whole pass used only
   rect/dot/text primitives to avoid more unverified seams while the
   rasterizer seam was still open.
2. **Wrong convention** — gray-box animatic conventions, for an author whose
   baseline is finished rich rendering (they have prior keynote samples with
   advanced 3D artifacts rendered through vieww).

The four perceptual verdicts (axis / cadence / stillness / fold-back) were
**withdrawn** — unobtainable through a medium the author rejected.

### 6.3 The reframe (settled)
**This pipeline has no gray-box stage.** Rendering is cheap and deterministic;
the animatic is not a test *of* the film but the film's **first real pass**,
iterating upward. The next pass swaps:

| pass 1 (flat) | next pass (rich) |
|---|---|
| closed-form `spring()` | **`vieww-animation` real `Spring`** (dep already present) |
| rect+dots session line | **real `Path`, progressively stroked** (E-17 as designed) |
| scale/offset fake depth | **real perspective transforms** |
| offset ghosts (F2) | **blur rack-focus ramp** — E-16, `vieww-effects` |
| flat Containers | **`DecoratedBox`**: radius, border, shadow |
| flicker rect as grain | **crafted degradation**: defocus, desaturation, grain (E-21 spec) |
| — | **gradients** (linear/radial/sweep — native renderer implements all, 4×4 supersampled, 28 blend modes) |

The wait (0:00–0:12) stays *degraded by design* — but degraded-**crafted**,
not degraded-poor. The capability audit says the film can be rich; pass 1
simply never asked.

**The pending ask (first action of the new session): the author pastes one
existing keynote sample** — the code behind `tour.rs` / `walkthrough.rs` /
whatever `ci/gifs.sh` drives. It buys both **API ground truth** (effect calls
that provably compile — closes the next seams in one round) and the
**aesthetic floor** (what "basic" means to the author). The animatic Cargo
deps hint the sample may already use `vieww-platform-winit` for a live
window — an interactive player (space = play/pause, ←/→ = scrub, 1–7 = jump
to test beats) is also on the table as the better scoring instrument.

---

## 7 · Verified API cheatsheet (proven by compiling/running code)

```rust
// THE RENDER SEAM — closed. Verbatim usage from crates/vieww-paint/tests/native_parity.rs
let mut renderer = vieww::paint::native::NativeRenderer::new();   // no args
let (pixels, report) = renderer
    .render_to_pixels(driver.scene(), w, h, Color::hex(BG))
    .expect("vieww's own rasterizer needs no display");
let bytes: Vec<u8> = pixels.data().to_vec();   // Pixels::data() -> &[u8]
// SceneReport fields: shapes, images, glyph_runs, layers, unsupported_blends

// The driver (headless, deterministic)
let mut driver = FrameDriver::new(Size::new(w, h));
let runtime = driver.elements().runtime().clone();
let clock = runtime.signal(0.0_f32);
driver.set_root(MyWidget { clock: clock.clone(), .. });
clock.set(t);
driver.draw_frame_at(Duration::from_secs_f64(t as f64));
driver.elements().find("MyWidget").unwrap().build_count(); // = frames + 1

// Widgets (proven shapes)
Stack::new().children(children![a, b])      // ALSO accepts a bare Vec<WidgetNode>
Positioned::new().left(x).top(y).width(w).height(h).child(c)
Container::new().color(c).radius(r).border(Border{color,width}).child(c).size(w,h).padding(..)
Text::new(s).style(TextStyle::new(size).color(c))
// #[widget] impl Block { fn build(&self, _ctx:&BuildContext) -> impl Into<WidgetNode> }
// — return the tree BARE (no .into()) under impl Into<WidgetNode> (E0283 otherwise)
// widget_node_from!(Name); for manual `impl Widget` styles

// Facts
Color::hex(0x0D1117).with_alpha(u8)         // alpha is 0–255, takes u8
ColorPipeline::GammaSpace default           // deliberate
AaMode::Gray default                        // "byte-identical to historical output"
                                            // → film-friendly text AA needs no config
renderer also has: render_in_place, last_frame() -> &[u8]  (zero-copy path, parked)
                 render_retained, render_to_png, render_damaged
with_aa_mode / with_color_pipeline / with_glyph_outline_budget_bytes constructors
```
Known-good greps for future seams:
`grep -n "pub fn" crates/vieww-paint/src/native/reference.rs` ·
`grep -rn "native::" apps/viewwstudio/src crates/vieww/tests` ·
the parity suite and `crates/vieww/tests/partial_repaint.rs` are canonical
usage sources. `cargo doc -p vieww-paint --features native --no-deps` works.

---

## 8 · Next actions (ordered)

1. **Get the author's keynote sample** (tour/walkthrough/gifs.sh source) —
   API ground truth + aesthetic floor. Then:
2. **Animatic pass 2 — the rich pass.** Same beat table (it is the film's
   clock; changing it changes the film). Swap per §6.3. Target: author's
   "basic" as the floor, escalation curve above it. Fix while in there:
   the caret beat (first character ~frame 735 — consider pulling type-on
   onset earlier if the cut reads as dead video).
3. **Obtain the four perceptual verdicts** from the rich pass:
   axis (KEEPS/HOLDS LONGER/FALLS BACK — fallback pre-scoped: unmotivated
   pull-back) · cadence (RELIEF/DEEPEN/DROP JUDDER) · stillness (EARNED/STEAL
   0.5s FROM F5) · fold-back (CLOSURE/SLOW IT/CUT TO F8). Every branch is
   scoped; no verdict sends us backward.
4. **T-01 the latency probe** (`examples/probe_latency.rs`): edit committed →
   parsed → driven → raster; N = raster − edit; writes
   `target/film/numbers.json`; S03's caption and the manifest read it;
   doubles as a CI budget test.
5. **T-03 the session spine** — critical path, unblocks five scenes.
6. Then T-02 / T-04 / T-05, then T-06 master (1080p, 10,800 frames,
   manifest from accumulated SceneReports, install lines from the
   single-sourced constant, render gate curls the URL).

**OPEN-1 (never fully pinned):** the studio preview's exact mechanism —
interpreter vs guest-dylib — which determines the literal truth of "nothing
was compiled" in S03's caption and the real magnitude of N. Ask the author;
it gates T-01's wording.

---

## 9 · Collaboration protocol

- The author runs everything locally and pastes terminal output. **Images
  sometimes arrive via a lossy describer** (descriptions, currently in
  Chinese); **video and GIFs never reach the assistant — motion cannot be
  watched.** Perceptual verdicts always belong to the author; mechanical
  receipts and static composition belong to the assistant.
- Contact-sheet technique for auditing sequence from stills:
  `fps=10,scale=480:-1,tile=4x4`.
- Graph markup convention: node ids (`S05`, `T-06`, `E-20`, `F4`) + changes;
  duration edits are free while sums still close (45+95+40=180 · reveal
  3+1+2+3+2+1+2+6=20).
- House style: receipts over claims; typed numbers are bugs (two incidents
  already); every effect names the primitive that renders it; honesty ledger
  updated whenever a new rule is born.
- The five rules + ten ledger lines (§3, §4.6) govern the assistant too.
  When in doubt: derive, don't type; source it live or cut it.

*End of handoff. The graph is closed; the film is a build program. First move:
ask for the keynote sample.*
````

Two things I'd flag before you close this session: the handoff marks the **axis decision as untested** (fair — the verdict was withdrawn with the medium), and it front-loads the **keynote-sample ask** as the first move, since it's the one thing that collapses the next session's seam risk and defines "basic." Everything else — the graph, the rules, the receipts, the API facts — is preserved verbatim. Good luck with the rich pass; I'll see it from the handoff.
