# film_lab — element experiments for the vieww launch film

Produced by an autonomous session (2026-09-24) running the create → render → VLM-audit → iterate loop against the actual vieww framework. Eight element experiments ("props" for the film), each rendered at 1280×720 with per-run receipts.

## What's in here

| Path | Contents |
|------|----------|
| `source/` | Complete Rust source: `film_lib.rs` (palette, RNG, clock, easing, render harness + SceneReport receipts), `three_d.rs` (Vec3, perspective camera, mesh builder, painter's sort, gradient ramps), `main.rs` (selector), `exp_*.rs` (one file per experiment). Registered as a workspace crate — drop into `examples/` of the release tree. |
| `renders/<name>/` | `sheet.png` (4×4 contact sheet, 16 frames) + `metrics.txt` for each experiment |
| `premium-test/` | Pipeline verification: test-premium-ui through FrameDriver, 56 frames, 55/55 moving, 0 overflows (GIF + first/last frame + metrics) |
| `vieww_film_lab_lite.zip` | 9 MB — sheets + metrics + source + premium-test |
| `vieww_film_lab_full.zip` | 44 MB — everything + all 128 individual PNG frames |
| `worklog.md` | Full session log: receipts, VLM audit verdicts, the bisection ladders, the alpha() incident |

## The experiments

| Name | What it proves for the film | Receipts |
|------|------------------------------|----------|
| `light` | Frosted-glass card, dithered gradients, rim strokes, starfield — the sheet-vf aesthetic floor | 4,010 shapes/frame, 96 layers, 133 ms/frame |
| `mesh` | Manual 3D pipeline (Transform is 2D affine only): painter's sort, Lambert + Blinn-Phong, spring-animated torus orbit with mid-flight retarget at t=0.63 | 9,688 shapes/frame, 39 ms/frame |
| `ocean` | 44×44 heightfield ocean growing from a drop impact, glint bloom pass | 15,771 shapes/frame, 41 ms/frame |
| `globe` | 3D globe experiment (three-planes-fan F4 direction) | 1,689 shapes/frame, 44 ms/frame |
| `aurora` | Gradient-mesh nebula / aurora background technique | 4,487 shapes/frame, 110 layers, 130 ms/frame |
| `circuit` | Circuit-board texture / tech-map motif | 1,810 shapes/frame, 51 ms/frame |
| `kinetic` | Text-kinetics family — 380 glyph runs, dash-phase line drawing (E-17 technique) | 1,427 shapes/frame, 42 ms/frame |
| `receipts` | The honesty-ledger render: metrics as on-screen evidence | 1,272 shapes/frame, 45 ms/frame |

## Running the source

The crate expects to live inside the vieww workspace (`vieww_ws/examples/film_lab`), pinned to Rust 1.98.1:

```
cargo run --release -p film_lab -- <experiment>
```

Experiments: `light`, `mesh`, `ocean`, `globe`, `aurora`, `circuit`, `kinetic`, `receipts`. Env gates (FILM_BISECT and FILM_DUMP_CMDS) are documented in the sources.

## Key lessons logged

- `driver.use_system_fonts()` is mandatory for any text-bearing render — embedded DejaVu subsets panic otherwise (fixed at vieww-render/src/frame.rs:1096).
- The `alpha()` convention incident: caller passed 0–255-style values into a 0–1 API, making every translucent swatch opaque. The framework was innocent (verified against native/color.rs, target.rs, reference.rs); found via FILM_DUMP_CMDS command-stream dump. Same species as the 2550-vs-2551 incident in handoff.md.
- Aesthetic floor verified: 8/10 VLM distance to the author's own sheet-vf reference frame.
- Framework boundary confirmed: `Transform` is deliberately 2D affine only — true 3D requires manual projection in custom Painting closures (see three_d.rs).
