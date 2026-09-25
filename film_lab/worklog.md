# vieww launch film — multi-agent worklog

---
Task ID: 1
Agent: main (Super Z)
Task: Understand complete context from vieww_artifacts repo (handoff, scene graph v0.7, strategy, sheets, code browsers); establish runnable environment for the create-evaluate iterate loop.

Work Log:
- Cloned https://github.com/tejakota/vieww_artifacts.git → /home/z/my-project/vieww_artifacts (32MB)
- Read handoff.md fully (500 lines): project model, mission, strategy, scene graph v0.7 checkpoint, animatic pass 1 state, aesthetic verdict ("lower than the basic I guessed"), the reframe (no gray-box stage; pass 2 = rich pass), verified API cheatsheet, next actions, collaboration protocol
- VLM-analyzed: sheet-bx.png (2000×1688), sheet-vf.png (2000×750), Keynote strategy.png
- Parsed scene_graph_v07.svg (3000×2300, 602 text elements) — all 12 scenes, 21 effects (E-01..E-22), transitions, counter ladder 1-7, E-20 reveal F1-F8, escalation curve
- Extracted embedded JSON data from both code browser HTMLs:
  - vieww_beta_code_browser_updated.html → window data in <script id="data"> (13.88MB JSON): tree "vieww_release-main", 1044 files, 795 .rs, 36 crates
  - vieww_crates_syntax_browser.html → 8.88MB JSON: 613 files + syntax reference (10 topics)
- Reconstructed full Rust workspace → /home/z/my-project/vieww_ws (crates/, examples/ 23 example crates, apps/, ci/, docs/, Cargo.toml+lock, rust-toolchain.toml pinned 1.98.1)
- Installed Rust 1.98.1 via rustup (+ rustfmt, clippy, llvm-tools-preview — the last one hung once, fixed with direct `rustup component add`)
- ffmpeg 7.1.5 available
- Started `cargo build --release -p test-premium-ui` (build in progress)
- Mined APIs from examples + crates:
  * test-premium-ui (771 lines): function-style widgets returning WidgetNode, FrameDriver+NativeRenderer loop, Filtered::blur().with_backdrop().tint() (frosted glass), BoxDecoration (radius/border/shadow/gradient), Gradient (radial/vertical + with_stops), Transformed::translate, Painting::sized + PaintWith + Sketchbook custom paint (book.circle/fill + Path), LineChart/BarChart/Chip/Avatar/Badge/LinearProgress/CircularProgress/Icon/Switch/Checkbox/Slider/Button, gif output
  * audit-sheets (1337 lines): vieww umbrella crate prelude, RouteTransition::wrap(screen,t,surface) pure fn, Theme/ThemeData, signal granularity proof (read in build vs construction — Handle widget + widget_node_from! macro), vieww::animation::{Tickers,Tween}, vieww_element::Animation, ScrollTimeline, Filtered vs blur_rgba kernel parity
  * animation-showcase: vieww_platform_winit::App window path, TimelineBuilder.stagger().build().attach(driver.tickers()) → TimelinePlayer.play(), PhysicsDrag + SpringPreset::{Expressive,Standard}, runtime.signal()
  * vieww-animation crate: SpringSpec, SpringPreset, SpringAnimation::new(from,preset).retarget/.retune/.jump/.value, Spring2D
  * foundation: Sketchbook (fill/stroke/stroke_styled/rect/rrect/circle/ring/arc/line/shadow/layer/transformed), Path (move_to/line_to/cubic_to/close/rect/rounded_rect/rounded_ring/arc/arc_ring/extend/reversed/transformed/fitted/bounds/stroke_outline), Transform (2D affine: translate/scale/rotate/rotate_around/scale_around/then/apply/invert — NO 3D matrix), Gradient (Linear/Radial/Sweep conic, with_stops, between, faded, Bayer 4×4 dither!), StrokeDash (even/dotted/offset — dash-phase progressive stroke possible), BlendMode (12 modes SrcOver..Exclusion)
  * vieww-effects: BackdropFilter/BackdropBlur/Filter (brightness/saturation/grayscale/sepia)/FilterChain/Blend
  * Filtered widget: blur/saturation/grayscale/brightness/sepia/tint/matrix/with_backdrop
  * full-showcase: render_to_png direct, ShapeMorph (Circle→RoundedRectangle .progress()), Carousel/Grid/ColorPicker
  * Text widget: style/color/size/weight/bold/italic/align/direction/overflow/max_lines

Stage Summary:
- CONTEXT FULLY ABSORBED: 3:00 film, kishōtenketsu (K0-K4 twists), 3 acts (45+95+40), 12 scenes S01-S12, 21 effects, counter ladder 1-7, E-20 reveal F1-F8 (3+1+2+3+2+1+2+6=20s), 5 rules, honesty ledger 10 lines
- AESTHETIC FLOOR (from sheets): dark premium (#0a0a0c-#121214 bg), electric violet accent (#8b5cf6-#a78bfa), glassmorphism/frosted surfaces, gradient mesh nebula backgrounds (#4c1d95/#be185d/#0ea5e9), glows, soft shadows, 3D perspective planes, spring-curve graphs, stat typography (95.1%, 3.6ms), Inter/SF-style sans + mono for data
- KEY STRATEGY: kishōtenketsu twists (reveals recontextualize) + single-take Act II as evidence (no cuts to hide latency)
- FULL SOURCE RECONSTRUCTED: complete workspace buildable locally — the create→run→evaluate loop CAN run here
- PENDING: build finishing; then run test-premium-ui to verify pipeline; then start element library experiments in examples/
- Scene graph expects scene sources at examples/s01_the_wait.rs .. s12_close.rs (flat files) + animatic crate (examples/animatic/ per handoff §5 — not in release snapshot, needs creating)

---
Task ID: 2
Agent: main (Super Z)
Task: Prove the create-evaluate loop runs end-to-end in this environment.

Work Log:
- Fixed llvm-tools hang (direct rustup component add)
- cargo fetch + cargo build --release -p test-premium-ui → SUCCESS (2m20s, all 36 crates)
- First run panicked: cosmic-text "no default font found" (binary needs LD_LIBRARY_PATH=toolchain/rustlib/x86_64-unknown-linux-gnu/lib due to prefer-dynamic; then embedded font path failed)
- FIX: driver.use_system_fonts() (vieww-render/src/frame.rs:1096) — one line, text now renders
- Second run: 56 frames, 55/55 moving, render mean 52.55ms, worst 94.61ms, 0 overflows, GIF+PNGs written
- VLM verification: text fully legible ("Overview", "184.2k", "Performance", "Frosted glass"...), quality rated 9/10
- Framework boundary confirmed: Transform = 2D affine only (perspective deliberately out of scope, transform.rs:17) → true 3D requires manual projection in custom Painting closures
- Performance receipt: ~53ms/frame at 1120×720 release → offline film rendering viable; 1080p×10,800 frames needs render-time planning

Stage Summary:
- THE LOOP IS LIVE: create → cargo build/run → PNG/GIF frames → VLM visual evaluate → iterate, all in-environment (no author paste-backs needed — mode upgrade vs handoff §9 protocol)
- Critical film-pipeline fact: FrameDriver default fonts are embedded-only DejaVu subsets; a text-bearing render MUST call driver.use_system_fonts() (or with_application_faces with shipped typefaces) or text silently fails/panics — applies to animatic pass 2 directly
- Environment: /home/z/my-project/vieww_ws (source), deps cached, release profile warm

---
Task ID: 3
Agent: main (Super Z)
Task: Build film_lab (element experiments) per ratified berserk brief: 3D zone first, match-the-floor calibration, flat+gradient shading, full send. Iterate via render → contact sheet → VLM audit.

Work Log:
- Created examples/film_lab (registered in workspace): film_lib.rs (palette from VLM sheet analysis, deterministic xorshift RNG, clock incl. held_24_in_60, easing incl. spring_out, render harness with SceneReport receipts, ffmpeg 4x4@10fps contact sheets, FILM_DUMP_CMDS command dumper), three_d.rs (Vec3 math, Camera with manual perspective projection — Transform is 2D affine only, quad Mesh, painter's sort, back-face cull via screen-space cross, Lambert + depth fog + Blinn-Phong specular + per-face 2-stop vertical gradient ramps, grid/torus/box mesh builders), exp_light.rs, exp_mesh.rs, exp_ocean.rs
- Round 1: receipts exposed ocean drawing only 11 shapes/frame → grid winding bug (Newell normal pointed DOWN, back-face cull ate the floor) → fixed push_quad(a,d,c,b)
- Round 2: VLM audits (light 4/10, mesh 4/10, ocean 6/10) → dither (Gradient::with_dither) on all big ramps, card shadow, mesh grounding ellipse, drop slower/bigger, glint bloom layer
- Round 3: Blinn-Phong specular added to MeshStyle (specular/shininess), ocean densified 26→44 cells (985 shapes/frame), heart blob + breathing bloom behind card
- Round 4: rim strokes (stroke_rrect hugging card edge in blurred layer) replacing filled halo, tags → plain muted text, crisp white border, starfield 90→230
- THE INCIDENT (receipts culture): card rendered opaque white through rounds 2-4 while bg was correct. Bisection ladder: noglass ✓ bg fine → plasticard (proven test-premium-ui pattern) ALSO white → mini repro (1 painting + 1 glass card) white → swatch forensics: EVERY translucent swatch opaque → FILM_DUMP_CMDS revealed Paint{color: a:255} — MY alpha() helper takes 0-1 f32 but card/sweep construction passed 0-255-style values (alpha(WHITE,30.0) → 30*255 → clamp 255). The framework (renderer, premul Porter-Duff, backdrop filter, tint_matrix) was INNOCENT throughout — verified by reading native/color.rs blend, target.rs snapshot_from, reference.rs PushLayer/PopLayer. Fixed all call sites to 0-1; glass card now renders (75,57,124) frosted violet.
- Round 5 (post-fix): VLM distance to sheet-vf reference: 8/10 (from 4/10) — "card clearly reads as translucent frosted glass, text highly legible". Mesh 6/10 (specular glint visible), ocean 7/10 (smoother, glints integrated).
- FILM_BISECT env gates kept as documentation: mini, mini_widbg, mini_nofilt, mini_plain, mini_noblur, micro, micro2, swatch, noglow, noglass, norim, nosweep, flatlayers

Stage Summary:
- film_lab live at examples/film_lab; outputs /home/z/my-project/download/film_lab/{light,mesh,ocean}/ (16 PNGs + 4x4 sheet + metrics.txt each)
- Element receipts: light 108-114 shapes/frame (75 glyph_runs — text renders), mesh 605 shapes/frame (line→grid→sweep-shaded floor→specular torus with real SpringAnimation orbit + mid-flight retarget at t=0.63), ocean 985 shapes/frame (drop→strike→ripple field→44x44 heightfield ocean growing from landing point + glint bloom pass)
- Render cost: light ~133ms/frame (layers+backdrop), mesh ~39ms, ocean ~41ms at 1280x720 release — offline film rendering fully viable
- AESTHETIC FLOOR MET: 8/10 VLM distance to author's own reference frame
- API seams verified along the way: Gradient with_dither (Bayer 4x4) works; StrokeStyle+Dash::new([draw_len, total]) dash-phase line drawing works (E-17 technique); Sketchbook::layer(alpha, blur, clip) group blur works; Shadow colored glow works; Filtered backdrop+tint works (when caller's alpha units are correct!)
- NEW INCIDENT LOGGED (like handoff's 2550-vs-2551): "the alpha() helper convention" — a caller-side unit bug masquerading as a rasterizer bug; found by command-stream dump (FILM_DUMP_CMDS), the cheapest instrument in the lab
- Next: mesh/ocean still have headroom (VLM: wireframe overlay prominence, glint integration, nebula color distribution); then globe + three-planes-fan (F4) experiments; then text-kinetics family

---

## Round 3 — the berserk continuation (session 2, 2026-09-24)

Task: "Continue in going berserk rendering every artifact — creating the props."

Work Log:
- Parsed the scene graph's effects rail (E-01..E-22 with per-effect scene, name, technique, BUILD/FREE cost) and diffed against the 8 shipped props: E-03/E-04, E-06, E-08/E-09, E-10, E-15, E-16 had no dedicated experiment. Round 3 = six new props, one per missing beat.
- exp_spring (E-03/E-04): wordmark drop on an underdamped spring (ω 6.2, ζ 0.52), underline overshoot on a stiffer one (ω 13.5, ζ 0.40), squash-on-contact, impact dust ring, and the springs drawn as their own receipt — both curves live with riders at current t, constants printed from the code's own values.
- exp_scrub (E-08/E-09): three scrub gestures; write ticks spill below the track (10/7/12 writes), a coalescing bracket spans each burst, the preview card holds its old value through the whole gesture then jumps ONCE on the single rebuild (flash + border bloom), build_count badge increments 1 per gesture, live writes/rebuilds/ratio receipt.
- exp_damage (E-10): mock studio surface in four regions (sidebar/header/preview/inspector ghosts), four edits each lighting exactly one region (wash + border + corner ticks + bloom), PerformanceOverlay strip with damage-area history trace computed from the mock's own rect geometry — 0.0% idle state included.
- exp_rackfocus (E-16): the machine's view (glyph-atlas grid, scan rows, command stream, mono cold-cyan) behind the author's view (glass card, 184.2k, sparkline); Filtered::blur ramps run opposite directions per layer, focal bar + live blur-px readout at the bottom; rack reverses mid-experiment — the film looks back at the machine.
- exp_morph (E-06): say → rust via a scan-line sweep, per-line crossfade with staggered slide (amber natural language dissolving up, mono Rust rising in); STATE HELD as the whole point: a session-clock chip on an unbroken horizontal state line with 24 Hz pulse ticks that never stops through the morph; words→glyphs handover count printed live.
- exp_endcard (E-15): the last image — settled wordmark (letter-spaced 14), typed-on `cargo add vieww` with caret, manifest line (0.7.0 · 36 crates · rust 1.98.1 — the workspace's own facts), the sting at t≈0.66 (one violet sweep through the underline, one bloom, one board-lift, then the hold), closing honesty line.
- THE RECT-EDGES INCIDENT: `Rect::new` takes edges (left, top, right, bottom) — exp_light (the 8/10 prop) proves it — but (x, y, w, h) was the working habit in several files. Origin-anchored rects are identical either way, masking the difference; offset rects degenerate silently (bottom < top → negative height; the shape never draws). Victims: kinetic's tick rail + band rules (never rendered), all offset rects in the six new files. Fix: `xywh()` helper in film_lib + call-site conversion; kinetic re-rendered with its tick rail visible for the first time.
- THE SAMPLED-ATTACK TRAP: damage's first audit (6/10, "flash barely perceptible") was a sampling artifact — 0.1 s attack + 5.5/s decay lives and dies entirely between the harness's 0.75 s frame spacing. Envelope re-tuned to ~0.3 s attack / ~1.2 s decay / 1.25 spike, wash 0.10→0.22, ticks 2.2→3.2 px; accent unified to violet (mint read as a defect). 6→7/10; full-res crop audit confirms wash + border + corner L-ticks all present.
- VLM audit round 3: spring 9/10, scrub 9/10, rackfocus 9/10, morph 9/10, endcard 9/10, damage 7/10 (post-fix). No rendering defects reported on any sheet.
- Full 14-experiment registry re-render; all receipts regenerated. Packaging: lite 11 MB; full split 1of2 (40 MB, base + experiments 1–7 frames) + 2of2 (30 MB, experiments 8–14 frames) — GitHub's 100 MB file ceiling forced the split; the monolithic 44 MB full.zip is retired.

Stage Summary:
- 14 props live; every scene-graph effect that warrants a standalone experiment now has one (E-01..E-22 coverage map in README).
- Round-3 receipts: spring 69.5 shapes/frame, scrub 79.8, damage 96.4, rackfocus 164.0, morph 96.6, endcard 37.7 — all 40–58 ms/frame except spring 40 ms and endcard 46 ms; offline rendering viable throughout.
- Two new incidents logged (Rect edges, sampled attack) — same species as the alpha() incident: caller-side assumptions silently producing absent output.

---

## Round 4 — the upgrade-door round (session 3, 2026-09-25)

Task: "Hit another round of ravaging — push the code to the repo after your rounds."

Work Log:
- Environment was a cold reset: rebuilt from the repo itself — synced `vieww_base`'s U-fixed crates (filter U-14/U-15, glyph door U-03, effects Blend U-02, text font assert U-09, native parity tests, real font binaries) plus round-3 film_lab sources into a fresh vieww_ws; rustup 1.98.1 + cargo fetch + build (2m22s); 787 tests pass across vieww-foundation + vieww-paint (the U-fixes hold under their own suite).
- THE DISPLAY-PIPELINE GHOST: `#[must_use]` rendered as `#ust_use]` through this session's tool output (the `[m` is eaten as an ANSI reset somewhere in the chain). First read looked like source corruption; `rg "must_use"` vs `rg "#ust_use"` settled it — the file was innocent, the instrument was lying. Same family as the alpha() incident: a caller-side/instrument-side artifact masquerading as a defect. Verify with a second instrument before believing.
- Round 4 = six new plates, each one a *door* the upgrade file opened:
- exp_wordmark (X-03, U-03): "vieww" as real glyph outlines through `vieww_paint::native::{outline_glyph, units_per_em}` — shaped via `FontStore`/`Paragraph` runs, placed with `place_glyph`'s own arithmetic. Stroke-on scan-reveal (clip-window variant of E-17 — a closed outline has no cheap perimeter), chrome fill as ONE union path so the ramp flows across the word, stops phase-advanced and **re-sorted every frame** (U-08, count printed), one Plus-blended glint (U-01), mirrored ground-faded reflection, and the U-10 dial — a sweep ramp built by `with_stops_mirrored`, seam invisible by construction.
- exp_shatter (X-08, U-11 + U-14): a frosted card shatters into 64 Voronoi shards (Sutherland–Hodgman half-plane clips — U-13's generator, again). Each shard is the transform-outside/clip-inside window onto the source, verbatim from the upgrade file's sketch. The fastest decile renders through `Filtered::with_blur_angle` — **a new widget-side door this session added** (U-14's missing last inch: the filter struct carried `blur_angle`, the rasterizer honoured it, the widget never exposed it — six lines, plus the doc). Plus-blended strike flash and dust; a live speed histogram in the receipt with the blurred decile in violet.
- exp_beams (X-05, U-01 + U-06): eleven volumetric shafts + floor pools through ONE Plus-blended blurred `blended_layer`, 6,000 dust motes through a second crisp Plus group, each mote's brightness read from its own position in the beam cones (the field is the receipt — lit count and mean light printed live). 1,223 shapes/frame, 46 ms.
- exp_liquid (U-13 + U-20): six metaballs + a falling drop, marching squares at 9 px cells, **the chaining pass front and centre** — segments join at exact grid-edge keys, loops close, the fill is closed paths only. Glass fill across the mass's measured bounds, a clipped highlight rider (U-11's smallest form), Plus under-glow, rim stroke. The receipt's instrument is the field itself: a coarse heatmap of what the algorithm sees.
- exp_unfold (P-01, U-04): three frosted planes fan out of a spine through `Transform3::project_rect` — the E-20 F4 beat. Tree glyphs projected pointwise through each plane's transform (foreshortening automatic), floor grid as 3D lines projected two points at a time (a straight 3D line projects straight — the pinhole guarantees it). **Behind-camera drops counted, not swallowed** (U-16's discipline at the boundary). Side-elevation ray diagram in the receipt.
- exp_hero — the capstone: the worst frame the film could plausibly ask for, **at 1920×1080** — grid + 11 shafts + 6,000 motes + a 3,200-quad swept tube knot (manual-projection three_d camera, Blinn-Phong, depth fog, painter's order) + the liquid mass + the outline wordmark with chrome and reflection + a frosted caption panel through a genuine `Filtered::with_backdrop()`. **4,703 shapes/frame, 129 layers, 149 ms mean / 171 ms worst** — the budget line, measured, at master resolution.
- THE BASELINE-BAKE INCIDENT: wordmark letters placed "at baseline 0" rendered off-canvas (only the reflection was visible — reading "AIGMM", an upside-down "vieww": a mirrored 'w' is an 'M'). The Mark geometry bakes its baseline at BASELINE=340; both the hero and the wordmark plate initially translated by the target y without subtracting the baked one. Same species as alpha() and Rect-edges: geometry carrying a hidden constant. Fix: translate by `target_y - BASELINE`, mirror the PLACED path.
- THE COLLAPSED-POSITIONED INCIDENT: `Opacity::new(a).child(Positioned::new().left(x).top(y)...)` rendered at origin (hero's caption text at top-left instead of bottom-right). A Positioned needs a Stack (or equivalent positioning parent) to honour left/top; under Opacity it collapsed to (0,0). Fix: Stack → Positioned → Opacity → content.
- Hero round 1 audit 4/10 (knot alone visible) → fixed placement/brightness/occlusion (beams brightened 0.09→0.15, pivot recentered, grid 0.085→0.22, dust enlarged, knot shifted left of the liquid) → **10/10, all eight elements verified**.
- VLM audit round 4: wordmark 9/10, shatter 9/10 (first render), beams 10/10, liquid 9/10, unfold 8/10 → contrast boosted, hero 10/10.
- Full 20-experiment registry re-render; all receipts regenerated (light 138 ms is the slowest at lab resolution; hero 149 ms at 1920×1080 — 10,800-frame master ≈ 27 minutes of rendering for the worst frame in the film).

Stage Summary:
- 20 props live; the round-4 six each demonstrate a specific upgrade door in use: U-01 (blended_layer), U-03 (outline_glyph), U-04 (Transform3), U-08/U-10 (phase/sort, mirrored sweep), U-11 (window pattern), U-13 (marching squares), U-14 (with_blur_angle — completed this round), U-20 (chaining).
- vieww_base updated in-repo: `Filtered::with_blur_angle` (the U-14 widget door) synced into the framework source.
- The hero receipt: **1920×1080 · 4,703 shapes · 129 layers · 149 ms mean, 171 ms worst** — "nothing in the film is limited by the renderer" now has this session's own number.
- New incidents logged (display-pipeline ghost, baseline bake, collapsed Positioned) — the species is stable: hidden constants and layout assumptions, all caller-side, all found by the create-render-audit loop.

---

## Round 5 — the instrument round (session 4, 2026-09-25)

Task: "Let's hard hit another round" — plus push with a fresh PAT.

Work Log:
- Framework doors closed this round (both were the file's own "one PR away" notes):
  - **U-06 FIXED** — `NativeRenderer::guard_filtered_layers_below(limit)`: opt-in `debug_assert` at the tail of the core render walk (`render_in_place`, where every entry point pays the same check). The entry's "or" branch, taken. Fires named in debug; compiles out of release; the harness sets it at 32. First placement attempt landed in `render_retained_in_place`'s tail (the should_panic test caught it — the test earned its keep on day one); moved to the core walk.
  - **U-20's counter FIXED** — `SceneReport::open_subpath_fills` + `Path::open_subpaths()`: the silent-petal census, counted at the `FillPath` verb. The receipts print it for all 25 experiments.
- **U-22, the census's first catch**: ghosts' debut render came back `open_subpath_fills = 2502` — every `book.circle` in the scene. `Path::arc` built a full-turn disc as move_to + 4 quarter-cubics with no `close()`: pixel-correct all along (zero-length chord), but a census that flags every disc is noise that hides the petal. Fix: `Path::arc` closes full-turn sweeps; partial arcs stay open (stroke callers must not grow closing edges). Ghosts' census went 2502 → 0 with byte-identical shape counts. Same species as the alpha() incident: a new instrument lighting up on a mistake nobody had asked about before there was a counter.
- Harness upgrades: `Receipt` carries `filtered_layers` + `open_subpath_fills`; `Experiment::probe` — a pixel probe run on the last rendered frame, reading the RGBA buffer directly (measured pixel facts, not geometry claims). Registry migrated to `Experiment::plain`.
- Five new plates:
  - exp_ghosts (E-24): a courier flies a closed-form lissajous; 56 ghosts evaluated (not remembered) at P(s − k·dt); the trail drawn as FOUR direction buckets — four `Filtered::with_blur_angle` widgets blurring along the bucket's motion axis (U-14) with Plus ghosts inside (U-01) — the U-06 economy in its own receipt: 4+1 filtered layers for 56 ghosts. Audit 4/10 → trail/bloom/contrast pass → **9/10**.
  - exp_settle (text-kinetics II): scramble-to-settle — each glyph churns a 14 Hz deterministic pool index, settles on its own jittered clock with a `spring_out` font-size pop; underline strokes in by dash phase once quiet; the motto types on with a blinking caret. Receipt: settle-wave front, cycles burned, settle timeline instrument. 8/10.
  - exp_dolly (the vertigo): camera retreats 11→36 while FOV narrows 60°→20° in exact compensation — the subject's projected height measured through the same `Camera::project` that draws it: **114.50 → 114.42 px (0.1% drift)** across a 3.3× retreat. Dunes, pylons, dust with free parallax, fog riding camera distance, the Plus glow pinned behind the subject (the vertigo's tell: it never changes size). 1,890 shapes/frame, 46 ms. 8/10.
  - exp_currents (the river): curl-noise field (divergence-free by construction), 168 streamlines integrated once (RK2, 110 steps, arc-length tables), every frame pure phase — each line's `Dash` marching downstream at its own speed (E-17's grammar, continuous); 36 tracers looked up in the same tables at the same march. One Plus group for the river, one for the tracers. 459 shapes/frame, 55 ms. 8/10.
  - exp_probe (the instrument panel): three instruments, every number read out of the output buffer by the pixel probe — (a) U-15's full-bleed σ24 wash, four corners **max Δch = 1** (the blur's fixed point, in pixels); (b) the petal clock, 6 petals with one deliberately missing its `close()` — census counts 15/15 live frames, probe reads identical ink at open and closed centroids (the chord closure is pixel-exact: the silence is literal); (c) U-18's hairline ladder 2.00→0.03 px, rung ink measured **218 / 122 / 74 / 26 / 26 / 26 / 26** — sub-quarter-pixel strokes land identically at ~1/64 coverage; the vanishing is measured, not argued (status appended to U-18). Playhead sweeps the ladder's right side (never on probed pixels — the receipts stayed byte-stable through the addition). 7.5/10.
- VLM audit round 5: ghosts 9/10, settle 8/10, dolly 8/10, currents 8/10, probe 7.5/10 (instrument plates are evidence, not cinema — the floor is honest).
- Full 25-experiment registry re-render: every experiment's `open_subpath_fills` = 0 except probe's deliberate 15; `filtered_layers` now visible across the whole library (aurora 110/16 frames ≈ 6.9/frame, light 5/frame, hero 36/8 = 4.5/frame — all comfortably under the guard).
- Tests: vieww-foundation 515 pass (5 new: open-subpath census ×3, disc-close ×2), vieww-paint (native) 235 pass (3 new: census chord-closure, guard quiet, guard fires).

Stage Summary:
- 25 props live; the two "one PR away" framework notes are closed, and the new instrument caught its own first bug (U-22) on its first plate — the receipts culture, compounding.
- The probe: measured pixel facts in receipts — U-15's fixed point (Δch 1), U-18's floor (26/255), the chord closure's pixel-exact silence.
- Round-5 receipts: ghosts 176 shapes/frame 97 ms (bloom is the cost, counted), settle 32 shapes/frame 25 ms, dolly 1,890 shapes/frame 46 ms, currents 459 shapes/frame 55 ms, probe 29 shapes/frame 44 ms. Hero re-measured at 4,703 shapes, 153 ms mean — the budget line holds.

---

## Round 6 — the berserk spectrum (session 5, 2026-09-25)

Task: "Don't just take the three which are already being done… try anything and everything that checks the tolerance of the framework. Going berserk means this." — the author's three named arcs (line → 3D avatar, text → blurred fade-away, drop → entire ocean) became three of eleven plates, not the round's scope.

Work Log:
- Environment was a cold reset again: rustup 1.98.1, workspace rebuilt from the repo's own `vieww_base` + `film_lab/source` (the 25-experiment tree), build 2m17s clean.
- Eleven new plates, each on its own tolerance axis:
  - exp_avatar (genesis I): the profile line draws itself (dash-phase), radial copies fan into a lathe cage, the surface skins crown→base, then the light orbits the bust. 4,486 shapes/frame, 64 ms, 9/10.
  - exp_fadeaway (genesis II / transition grammar): chromatic R/G/B split through Plus → ONE blurred group σ 0→16 with printed ink compensation → per-letter 3×4 clipped shard windows (U-11 verbatim) staggered left→right → dust → baseline hairline. 32-frame sampling on purpose. 9/10 after the audit-prompt lesson (below).
  - exp_sea (genesis III): one heightfield the whole way — drop, strike, radial rings, camera rises to the horizon, wind waves, sun glitter path (380 specular samples, threshold-tested against the actual wave normals). 8/10.
  - exp_tesseract: 4D rotation (XW/YW) → w-perspective → 3D camera; w-as-colour; ghosts evaluated; the hyper-flip bloom. 9/10.
  - exp_blackhole: 2,400 Keplerian orbitals, Doppler beaming, far side bent over the top as the halo arc, photon ring, 2,400 lensed stars with tangential smear, infall spiral. 9/10.
  - exp_galaxy: THE RECORD — 60,000 stars, 3 log-spiral arms, differential rotation, dust lanes, HII regions, golden core. 60,527 shapes/frame in 101 ms. 8/10.
  - exp_forest: depth-8 recursion, apical dominance + phototropism, depth-wave growth, ~2,900 leaf billboards, fireflies. 8/10 after the dir_turn incident (below).
  - exp_city: 13×13 procedural downtown, 3-octave hashed heights, per-window coordinate-hash lights (lit fraction measured live), traffic, crescent moon. 9/10 after the window-slit fix.
  - exp_typo: 39 positioned+rotated Text widgets riding RK2-integrated flow tables, 4 direction-bucket blur groups (ghosts' economy), condensation into the drop. 936 glyph runs. 9/10.
  - exp_mandel: seahorse-valley escape-time zoom as 320×180 = 57,602 rects/frame, smooth-coloured, budget 48+42·log₂(zoom). 69 ms. 8/10.
  - exp_hero4k: the hero tree at ×2 root scale on 3840×2160 — 440–489 ms/frame; the 4-frame variant OOM-killed on the 4 GB bench (the tolerance data), 2-frame passes.
- THE dir_turn INCIDENT: yaw-around-Y collapses for a vertical vector (the perpendicular is zero) — the tree rendered as a pole with a tuft. VLM: "shepherd's crook… dandelion hair-lines." Fixed with the orthonormal-frame tilt (pitch away from d, compass direction from yaw) + apical dominance (first child continues, laterals splay) + phototropism (children pulled 0.2 toward +Y). 6 → 4 → 8/10.
- THE AUDIT-PROMPT INCIDENT: the SAME fadeaway sheet scored 2/10 context-free ("almost entirely black — extreme underexposure") and 9/10 with intent in the prompt ("intentionally dark — evaluate the transition"). Direct pixel check confirmed the text renders at (235, 229, 254). The audit prompt is part of the instrument; dark plates must be audited with their design intent named.
- THE WINDOW-SLIT INCIDENT: face-aligned quads lerped from "column u" to "column u+s" — windows counted but one-sided-sliver invisible. The count cannot tell; only the audit can.
- THE ONCELOCK DEADLOCK: total_segments() → grow() → total_segments() hung the first forest render with zero output — re-entrant get_or_init blocks on itself. Fixed by needing no census (depth-wave reveal).
- Full 36-experiment registry re-render; all receipts fresh; zips repackaged as lite + full 3-of-3 (the 2-part split retired — round-6 frames need their own part).

Stage Summary:
- 36 props live. The tolerance axes now measured: shape count (60,527 @ 101 ms), rect count (57,602 @ 69 ms), glyph runs (39/frame through 4 blur buckets), recursion (depth 8 → 9,661 faces), canvas (4K @ 440–489 ms, OOM boundary recorded), procedural density (city, galaxy), blend economy (Plus groups ≤ 3 per plate, filtered ≤ 62 per experiment).
- The hero budget line, superseded in one axis: the hero's 4,703 shapes in 149 ms was never a count limit — galaxy proves 13× the count at ⅔ the cost. The master's constraint stack: taste → memory → time.
- Round-6 audit scores: avatar 9, fadeaway 9, sea 8, tesseract 9, blackhole 9, galaxy 8, forest 8, city 9, typo 9, mandel 8, hero4k (receipt plate).
