# studio_film — THE SPARK (keynote cut)

The viewwstudio keynote film, rendered from `vieww_base/examples/film_lab/src/studio_film/`
on the `feature/vieww_code_base` branch.

## Contents

- `studio_film.mp4` — the final film. 1920×1080 · 60 fps · 420 s (7:00) · H.264 + AAC 48 kHz.
  The score is synthesised by `vieww-audio` (`studio_film::score`) and muxed in.
- `score.wav` — the synthesised score (vieww-audio, end to end).
- `sheets/` — 32 contact sheets (4×4 @ 10 fps, 960 px tiles), one per scene.
- `segments/` — 32 H.264 segments (one per scene), concatenated into the final mp4.
  `segments/list.txt` is ffmpeg's concat list (relative paths).
- `cut/` — the entrance-dissolve A-frames (each scene's final frame, held at full
  brightness so the next scene's dissolve has something to blend from).
- `manifest.txt` — the film's census receipt (frame count, shape count, bench identity).
  **Note:** the full census could not complete under the 4 GB container memory limit
  (OOM at Z10C where 12 lab plates mount simultaneously); the frame count is derived
  from the scene table, the raster samples are preview-measured, and the remaining
  probe values are placeholders. A true census requires ≥ 8 GB RAM.
- `samples.txt` — per-scene raster samples (draw commands, milliseconds).

## The 32 scenes

| Movement | ID | Name | Seconds | Carries |
|----------|----|------|---------|---------|
| Prologue | Z00 | the_hush | 11 | |
| I · NEED | Z01 | the_same_picture | 12 | |
| I · NEED | Z02 | the_tolls | 12 | |
| I · NEED | Z05 | the_question | 13 | |
| II · ENGINE | Z06 | the_engine | 15 | |
| II · ENGINE | Z10C | the_machine_room | 14 | |
| II · ENGINE | Z10D | the_mark_in_3d | 11 | vieww-3d |
| II · ENGINE | Z10B | beyond_ui | 14 | |
| II · ENGINE | Z10E | the_shutter | 13 | **vieww-video** — motion blur, linear-light shutter |
| II · ENGINE | Z10F | the_cook | 12 | **vieww-graph** — dirty-flag dataflow, the trail |
| II · ENGINE | Z10G | the_swarm | 13 | **vieww-game** — ECS fixed loop, A* re-routing |
| III · STUDIO | Z11 | studio_opens | 16 | the real app |
| III · STUDIO | Z12 | the_shell | 13 | the real app |
| III · STUDIO | Z12B | make_it_yours | 12 | the real app |
| III · STUDIO | Z13 | live_compose | 15 | the real app |
| III · STUDIO | Z13B | the_inspector | 12 | the real app |
| III · STUDIO | Z13C | the_arena | 13 | **vieww-gestures + vieww-canvas** — recognisers, arena, fling, Transformer |
| III · STUDIO | Z13D | the_designer | 12 | **vieww-lottie + vieww-image + vieww-asset** — Bodymovin, sprites, mips, atlas |
| III · STUDIO | Z14 | say_to_rust | 16 | the real app, say-codegen |
| III · STUDIO | Z15 | ships_everywhere | 14 | the real app |
| III · STUDIO | Z16 | the_devices | 13 | the real app |
| III · STUDIO | Z16B | together | 12 | **vieww-collab** — RGA text, presence, the merge |
| III · STUDIO | Z17 | build_and_ship | 13 | the real app |
| IV · PROOF | Z18 | the_ledger | 19 | |
| IV · PROOF | Z19 | the_receipts | 13 | vieww-dataviz treemap |
| IV · PROOF | Z19B | built_with_vieww | 13 | |
| IV · PROOF | Z19C | the_frame_itself | 14 | **vieww-scene + render-graph + render-planner + devtools + test-harness** |
| IV · PROOF | Z19D | the_contrast | 11 | **vieww-accessibility** — WCAG ratios, live |
| V · RELEASE | Z20 | the_pullback | 10 | the real app |
| V · RELEASE | Z20B | the_flight | 11 | **vieww-element + vieww-animation** — shared flights, duplicator, particles |
| V · RELEASE | Z21 | the_endcard | 18 | |
| V · RELEASE | Z22 | the_hold | 10 | |

**Total: 420 s (7:00) · 25,200 frames at 60 fps.**

The studio act (Z11–Z17, Z20) mounts the **actual `viewwstudio` app** on the film's
own `FrameDriver` — the film types and the real editor types, presses the real Render
(Z14: `say`-codegen → `rustc` → `cdylib` → `dlopen`) and taps the compiled counter for
real, with state that carries the recompile.

The nine **depth scenes** (Z10E, Z10F, Z10G, Z13C, Z13D, Z16B, Z19C, Z19D, Z20B) are
the seven-minute cut's own additions: every framework crate that could carry a beat
on screen got one, each with its receipts measured from the crate's own data — the
shutter's mean ΔL from the pixels, the cook's counters from the graph, the swarm's
fixed steps from the loop, the fling's settle from the curve, the merge's equality
from the snapshots, the flight's landing error from the rectangles.

## How to re-render

```console
cd vieww_base
cargo run --release -p film_lab -- censussf   # pass 1: census
cargo run --release -p film_lab -- mastersf   # pass 2: master (sheets + mp4)
```

`SCALE_FACTOR=2` renders 4K; `SCALE_FACTOR=4` renders 8K. `SF_ONLY=Z11,Z12` renders
just those scenes (resume-friendly). `STUDIO_FILM_OUT=/path` overrides the output dir.
A scene's contact sheet is its completion marker — a re-run skips every scene whose
sheet already exists, so an interrupted master resumes across processes.
