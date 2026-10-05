# studio_film — THE SPARK (keynote cut)

The viewwstudio keynote film, rendered from `vieww_base/examples/film_lab/src/studio_film/`
on the `z_keynote_film_zero` branch.

## Contents

- `studio_film.mp4` — the final film. 1920×1080 · 60 fps · 309 s (5:09) · H.264 + AAC 48 kHz.
  The score is synthesised by `vieww-audio` (`studio_film::score`) and muxed in.
- `score.wav` — the synthesised score (vieww-audio, end to end).
- `sheets/` — 23 contact sheets (4×4 @ 10 fps, 960 px tiles), one per scene.
- `segments/` — 23 H.264 segments (one per scene), concatenated into the final mp4.
  `segments/list.txt` is ffmpeg's concat list.
- `cut/` — the entrance-dissolve A-frames (each scene's final frame, held at full
  brightness so the next scene's dissolve has something to blend from).
- `manifest.txt` — the film's census receipt (frame count, shape count, bench identity).
  **Note:** the full census could not complete under the 4 GB container memory limit
  (OOM at Z10C where 12 lab plates mount simultaneously); these are placeholder
  probe values. A true census requires ≥ 8 GB RAM.
- `samples.txt` — per-scene raster samples (draw commands, milliseconds).

## The 23 scenes

| Movement | ID | Name | Seconds |
|----------|----|------|---------|
| Prologue | Z00 | the_hush | 11 |
| I · NEED | Z01 | the_same_picture | 12 |
| I · NEED | Z02 | the_tolls | 12 |
| I · NEED | Z05 | the_question | 13 |
| II · ENGINE | Z06 | the_engine | 15 |
| II · ENGINE | Z10C | the_machine_room | 14 |
| II · ENGINE | Z10D | the_mark_in_3d | 11 |
| II · ENGINE | Z10B | beyond_ui | 14 |
| III · STUDIO | Z11 | studio_opens | 16 |
| III · STUDIO | Z12 | the_shell | 13 |
| III · STUDIO | Z12B | make_it_yours | 12 |
| III · STUDIO | Z13 | live_compose | 15 |
| III · STUDIO | Z13B | the_inspector | 12 |
| III · STUDIO | Z14 | say_to_rust | 16 |
| III · STUDIO | Z15 | ships_everywhere | 14 |
| III · STUDIO | Z16 | the_devices | 13 |
| III · STUDIO | Z17 | build_and_ship | 13 |
| IV · PROOF | Z18 | the_ledger | 19 |
| IV · PROOF | Z19 | the_receipts | 13 |
| IV · PROOF | Z19B | built_with_vieww | 13 |
| V · RELEASE | Z20 | the_pullback | 10 |
| V · RELEASE | Z21 | the_endcard | 18 |
| V · RELEASE | Z22 | the_hold | 10 |

**Total: 309 s (5:09) · 18,540 frames at 60 fps.**

The studio act (Z11–Z17, Z20) mounts the **actual `viewwstudio` app** on the film's
own `FrameDriver` — the film types and the real editor types, presses the real Render
(Z14: `say`-codegen → `rustc` → `cdylib` → `dlopen`) and taps the compiled counter for
real, with state that carries the recompile.

## How to re-render

```console
cd vieww_base
cargo run --release -p film_lab -- censussf   # pass 1: census
cargo run --release -p film_lab -- mastersf   # pass 2: master (sheets + mp4)
```

`SCALE_FACTOR=2` renders 4K; `SCALE_FACTOR=4` renders 8K. `SF_ONLY=Z11,Z12` renders
just those scenes (resume-friendly). `STUDIO_FILM_OUT=/path` overrides the output dir.
