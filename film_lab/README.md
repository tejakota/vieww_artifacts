# film_lab — element experiments for the vieww launch film

Produced by autonomous sessions (2026-09-24/25) running the create → render → VLM-audit → iterate loop against the actual vieww framework. **Twenty element experiments** ("props" for the film), each rendered with per-run receipts — round 3 covered every scene-graph effect, and **round 4 demonstrates the todo-upgrades doors in use: the U-01/U-03/U-04/U-08/U-10/U-11/U-13/U-14/U-20 capabilities, each exercised by a plate built on it, capped by the 1920×1080 hero frame**.

## What's in here

| Path | Contents |
|------|----------|
| `source/` | Complete Rust source: `film_lib.rs` (palette, RNG, clock, easing, `xywh()` rect helper, render harness + SceneReport receipts), `three_d.rs` (Vec3, perspective camera, mesh builder, painter's sort, gradient ramps), `main.rs` (selector), `exp_*.rs` (one file per experiment). Registered as a workspace crate — drop into `examples/` of the release tree. |
| `renders/<name>/` | `sheet.png` (4×4 contact sheet, 16 frames) + `metrics.txt` for each experiment |
| `premium-test/` | Pipeline verification: test-premium-ui through FrameDriver, 56 frames, 55/55 moving, 0 overflows (GIF + first/last frame + metrics) |
| `vieww_film_lab_lite.zip` | 13 MB — sheets + metrics + source + premium-test |
| `vieww_film_lab_full_1of2.zip` | 58 MB — the full lab: source, sheets, metrics, premium-test + all frames for experiments 1–10 |
| `vieww_film_lab_full_2of2.zip` | 45 MB — experiments 11–20 (frames + sheets + metrics, self-sufficient). Unzip both parts into the same directory to reassemble the complete lab |
| `worklog.md` | Full session log: receipts, VLM audit verdicts, the bisection ladders, the alpha() incident, the Rect-edges incident |

## The experiments

| Name | What it proves for the film | Receipts |
|------|------------------------------|----------|
| `light` | Frosted-glass card, dithered gradients, rim strokes, starfield — the sheet-vf aesthetic floor | 4,010 shapes/frame, 96 layers, 142 ms/frame |
| `mesh` | Manual 3D pipeline (Transform is 2D affine only): painter's sort, Lambert + Blinn-Phong, spring-animated torus orbit with mid-flight retarget at t=0.63 | 9,688 shapes/frame, 40 ms/frame |
| `ocean` | 44×44 heightfield ocean growing from a drop impact, glint bloom pass | 15,771 shapes/frame, 43 ms/frame |
| `kinetic` | Text-kinetics family — type-on, hour-counter, odometer, stat count-ups (E-01/E-02, the ladder, S11) | 1,427 shapes/frame, 380 glyph runs, 35 ms/frame |
| `circuit` | The session line E-17: self-drawing path, node blooms, fork ×3 (E-22), the axis (E-20 F2) | 1,810 shapes/frame, 51 ms/frame |
| `globe` | The reveal spine E-20 F1–F4: the 7, the ghost part, the pull-back, three planes fanning | 1,689 shapes/frame, 71 layers, 49 ms/frame |
| `receipts` | S11: the benchmark card assembles in 3D around real widget props + CI timeline E-19 | 1,272 shapes/frame, 46 ms/frame |
| `aurora` | Light as material: sweep-cone, Screen-blended ribbons, E-21 crafted degradation (FilterChain ramp + grain + dust) | 4,487 shapes/frame, 110 layers, 166 ms/frame |
| `spring` | **E-03/E-04**: the wordmark spring drop + underline overshoot (higher ω) — the springs drawn as their own live receipt | 1,112 shapes/frame, 40 ms/frame |
| `scrub` | **E-08/E-09**: ten writes, one rebuild — scheduler coalescing made visible; `build_count()` badge, writes-vs-rebuilds ratio counted live | 1,276 shapes/frame, 52 ms/frame |
| `damage` | **E-10**: one region lights — a mock studio surface, four edits, PerformanceOverlay strip with damage-area history measured from the mock's own geometry | 1,543 shapes/frame, 51 ms/frame |
| `rackfocus` | **E-16**: the blur ramp between buffer and preview — the focus pull as cinematography, both layers' blur px printed live | 2,624 shapes/frame, 58 ms/frame |
| `morph` | **E-06**: say → rust, state held — the language switch via scan-line sweep, the session clock never rebuilding through it | 1,545 shapes/frame, 195 glyph runs, 47 ms/frame |
| `endcard` | **E-15**: the end card + the sting — wordmark, typed install line, manifest line, one accent firing, the hold | 603 shapes/frame, 46 ms/frame |
| `beams` | **X-05 / U-01/U-06**: eleven volumetric shafts + floor pools through ONE Plus-blended blurred `blended_layer`; 6,000 dust motes, each lit by its own cone depth | 19,574 shapes/frame, 60 layers, 46 ms/frame |
| `liquid` | **U-13/U-20**: six metaballs + a drop, marching squares, the chaining pass front and centre — closed loops only; glass fill, clipped highlight rider, Plus under-glow | 9,758 shapes/frame, 56 layers, 28 ms/frame |
| `unfold` | **P-01 / U-04**: three planes fan out of a spine through `Transform3::project_rect` (the E-20 F4 beat) — tree glyphs projected pointwise, floor grid in 3D, behind-camera drops counted | 4,545 shapes/frame, 57 layers, 36 ms/frame |
| `shatter` | **X-08 / U-11/U-14**: 64 Voronoi shards, each a flying window onto the source; the fastest decile through `Filtered::with_blur_angle` (motion blur along the velocity vector) | 15,696 shapes/frame, 796 layers, 48 ms/frame |
| `wordmark` | **X-03 / U-03**: "vieww" as real glyph outlines (the public door) — scan-reveal skeleton, phase-advancing re-sorted chrome (U-08), Plus glint (U-01), mirrored reflection, the U-10 dial | 1,356 shapes/frame, 50 layers, 30 ms/frame |
| `hero` | **The budget plate**: the worst frame at true master resolution — grid, shafts, dust, a 3,200-quad tube knot, the liquid mass, the outline wordmark, a genuine backdrop-blur caption | **1920×1080 · 4,703 shapes · 129 layers · 149 ms mean, 171 ms worst** |

## Scene-graph coverage

With rounds 3–4, every effect on the S01–S12 rail that needs a dedicated prop has one, and every upgrade door the bench opened has a plate proving it: E-01, E-02 (kinetic) · E-03, E-04 (spring) · E-06 (morph) · E-08, E-09 (scrub) · E-10 (damage) · E-15 (endcard) · E-16 (rackfocus) · E-17, E-22 (circuit) · E-18 (mesh/receipts 3D) · E-19 (receipts) · E-20 (globe) · E-21 (aurora). E-05, E-07, E-11, E-12, E-13 are widget-level beats staged inside the scenes themselves rather than standalone props.

## Running the source

The crate expects to live inside the vieww workspace (`vieww_ws/examples/film_lab`), pinned to Rust 1.98.1:

```
cargo run --release -p film_lab -- <experiment>
```

Experiments: `light`, `mesh`, `ocean`, `kinetic`, `circuit`, `globe`, `receipts`, `aurora`, `spring`, `scrub`, `damage`, `rackfocus`, `morph`, `endcard`, `beams`, `liquid`, `unfold`, `shatter`, `wordmark`, `hero` (hero renders at 1920×1080, 8 frames). Env gates (FILM_BISECT and FILM_DUMP_CMDS) are documented in the sources.

## Key lessons logged

- `driver.use_system_fonts()` is mandatory for any text-bearing render — embedded DejaVu subsets panic otherwise (fixed at vieww-render/src/frame.rs:1096).
- The `alpha()` convention incident: caller passed 0–255-style values into a 0–1 API, making every translucent swatch opaque. The framework was innocent (verified against native/color.rs, target.rs, reference.rs); found via FILM_DUMP_CMDS command-stream dump. Same species as the 2550-vs-2551 incident in handoff.md.
- **The Rect-edges incident (round 3)**: `Rect::new` takes *edges* (left, top, right, bottom), not (x, y, w, h). Origin-anchored rects are identical either way, which masks the difference — offset rects silently render degenerate (zero/negative height hairlines, tick rails that never draw). Fixed with the `xywh()` helper; kinetic's tick rail and band rules re-rendered visibly for the first time.
- **The sampled-attack trap (round 3)**: a 0.1 s attack with 5.5/s decay is invisible at 16-frame sampling (0.75 s spacing) — the flash lives and dies between frames. Effects must be tuned against the harness's sampling cadence, not just against wall-clock time. damage's envelope now spans ~1.2 s.
- Aesthetic floor verified: 8/10 VLM distance to the author's own sheet-vf reference frame; round-3 props audit at 9/10 (spring, scrub, rackfocus, morph, endcard) and 7/10 (damage, after the envelope fix — full-res verification shows wash + border + corner ticks correct).
- Framework boundary confirmed: `Transform` is deliberately 2D affine only — true 3D requires manual projection in custom Painting closures (see three_d.rs).

## Round-4 key lessons

- **The display-pipeline ghost**: `#[must_use]` reads as `#ust_use]` through this session's tool output — the `[m` is eaten as an ANSI reset somewhere in the chain. The file was innocent; `rg "must_use"` vs `rg "#ust_use"` settled it. Always verify suspected corruption with a second instrument.
- **The baseline-bake incident**: geometry built once bakes its placement constants in. The wordmark Mark carries `BASELINE = 340` inside its paths; translating by a target y without subtracting the baked one put the letters off-canvas — and only the *reflection* was visible (an upside-down "vieww" reads "AIGMM": a mirrored w is an M). Fix: translate by `target_y − BASELINE`; mirror the placed path, not the raw one.
- **The collapsed-Positioned incident**: `Opacity::new(a).child(Positioned…)` renders at origin — a Positioned needs a positioning parent (a Stack), not a plain wrapper. Symptom: hero's caption text at top-left instead of bottom-right.
- **The U-14 last inch**: `ImageFilter` carried `blur_angle`, the rasterizer honoured it, `Filtered` never exposed it — the shatter plate completed the chain with `Filtered::with_blur_angle` (now in `vieww_base`).
- **The hero budget, this session's own number**: 1920×1080 · 4,703 shapes · 129 layers · 149 ms mean, 171 ms worst. A 10,800-frame master of frames like it is ~27 minutes of rendering. The constraint remains taste.
