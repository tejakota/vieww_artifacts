# product_film — THE DISTANCE

**The viewwstudio product film.** Five minutes, 23 scenes, five
movements, 18,000 frames at 1920×1080 · 60 fps — **every frame rendered
by vieww itself**, through the headless `FrameDriver` + the CPU
rasteriser, raw RGBA streamed into ffmpeg. Nothing was added in post;
there is no audio, by construction.

```text
product_film.mp4        the film (5:00 · 1080p60 · h264 · 30 MB · silent)
sheets/                 one 4×4 contact sheet per scene (23 PNGs) — the audit surface
manifest.txt            the film's own census: every number measured, none typed
scale_4k_endcard.png    the 4K receipt — SCALE_FACTOR=2, glyph edges scan-converted
calibrate/              the tap-calibration receipts (hit-tested against the real app)
source/                 a copy of the film's source, for review
```

## The receipts (pass 1, the census — pass 2 quotes them)

This master is the **camera/cuts/caret re-render**: the film's camera
plan (movement III holds `Cam::STILL` by construction), decelerating
entry / accelerating exit cut handles with a luminance ramp, the
shaper-measured type-on caret, the top caption band, the screen-space
chrome split, and the A04 device splay through `panel_3d` (2×2
piecewise-affine, so the silhouette is the true perspective quad).

```text
frames=18000            60 fps × 300 s, derived from the scene table
shapes=4647195          fill + stroke commands across the whole film
glyph_runs=1154403      DrawGlyphs commands
glyphs=17499560         glyph instances placed
layers=114991           (filtered_layers=46207 — the blur economy)
strokes=590131          · shadows=82890
alive_seconds=0.151     the measured edit→pixels latency through the real studio,
                        this bench (2-core container), at master resolution
frame_ms=144.73         median sampled build+raster frame time at 1080p
scale_factor=1          this master; the 4K receipt beside it ran at 2
bench=linux · x86_64 · rust 1.98.1 · 6 embedded faces
```

The ledger scene additionally quotes the repository's own certification
run — startup 27.5 ms · p95 11.4 ms · worst frame 13.1 ms · 4,225
workspace tests · 0 steady-state allocations over 60 frames — verbatim
from `vieww_base/docs/VIEWW-PHASE-STATUS.md` (2026-09-15), and the film
says on screen where each column came from.

## The story — one number, falling

Two points: a caret (a thought) and a screen (a person waiting to see
it). Between them, a measured distance in milliseconds. The film is the
story of that number collapsing:

| movement | scenes | the distance |
|---|---|---|
| **P** · the question | `the_two_points` | *how far is a thought from a screen?* |
| **I** · the far | `the_wait` · `the_tolls` · `the_jank` · `the_drift` | 45,000 ms — the old world's four pains: latency, the middleman, the jank, the drift |
| **II** · the engine | `three_trees` · `the_crates` · `the_rasterizer` · `the_budget` | 8,400 → 620 ms — Widget → Element → RenderObject, 36 crates, no wgpu by design, damage + signal, budget kept |
| **III** · the studio | `studio_opens` … `build_ships` (9 scenes) | ~30 ms — **the actual app**, driven live: the screen() contract, live compose, a real tap, say → rust → cdylib → preview, state that carries the recompile, three platform frames, the token editor, the build |
| **IV+V** · the proof, zero | `the_ledger` · `the_pullback` · `zero_distance` · `the_endcard` · `the_hold` | 16 ms → 0 — the receipts, the constellation, the poles meet and become the mark, **beta release available today** |

The emotional arc: **curiosity** (the question) → **need** (the wait,
the tolls, the jank, the drift) → **relief** (the engine that owns its
own pixels, and the studio built on it).

The end card carries the launch line — **beta release available today**
— the studio's own code-drawn logo (`viewwstudio::ui::brand`, used, not
copied: two overlapping rounded panels, the editor and the preview),
the repository, the licence and platforms, the manifest bars, and the
closing sting: *this film was rendered with vieww.*

## The medium's own claim

Movement III is **not a mock**. The film mounts the actual `viewwstudio`
app — the real `Shell`, the real `Studio` state, the real element
runtime, the real `rustc` compile pipeline — on the film's own
`FrameDriver`, and drives it with a frame-indexed script
(`product_film/script.rs`). When the film types, the studio's editor
types. When the film taps, the studio's demo navigates (a real
`PointerEvent` pair through the real gesture disambiguation). When the
film presses Render, `rustc` runs: say-codegen → `counter.rs` → cdylib
→ `dlopen` → the compiled counter mounts, and the film taps *Add one*
for real — the count goes 0 → 1, and then survives a recompile with a
different screen (`keep` carrying the state).

The tap coordinates are **hit-tested against the real tree** by the
calibration probe (`film_lab pfcal`); its receipts are committed in
`calibrate/`. Pointer pairs never straddle a `set_root` — the harness
applies each press/release atomically, in the frame where the release
is due, because `FrameDriver::set_root` clears live pointers and a
split pair would die silently in the gesture arena.

## SCALE_FACTOR — 1080p base, 4K and beyond

The master rasterises the *same* 1920×1080 logical frame at any device
resolution. `SCALE_FACTOR` (an environment variable, default `1`)
multiplies the raster dimensions, and the command list is scaled
*after* compositing via `Scene::scaled` — glyph outlines are
scan-converted at device resolution, never magnified. Layout, the
session script's taps, and the studio itself are all in logical points
and never change with scale.

```sh
SCALE_FACTOR=1 film_lab masterpf   # 1920×1080 (the base master — this MP4)
SCALE_FACTOR=2 film_lab masterpf   # 3840×2160 (4K)
SCALE_FACTOR=4 film_lab masterpf   # 7680×4320 (8K)
```

`scale_4k_endcard.png` beside this README is the 4K receipt: the end
card re-rendered at `SCALE_FACTOR=2` from the *same* code, every glyph
edge scan-converted at 3840×2160 — the same film, rasterised denser.

## Rendering it yourself

The film is part of the film lab (`vieww_base/examples/film_lab`):

```sh
cd vieww_base
cargo build --release -p film_lab

# The two passes — the census (audit + measurements), then the master:
./target/release/film_lab censuspf                    # pass 1 — counts every frame
SCALE_FACTOR=1 ./target/release/film_lab masterpf     # pass 2 — renders + encodes

# One scene, sixteen frames, a sheet (the review loop):
./target/release/film_lab pf:the_wait

# The 4K receipt:
SCALE_FACTOR=2 ./target/release/film_lab pf:endcard

# Re-verify the script's tap coordinates against the real tree:
./target/release/film_lab pfcal
```

The master writes segments per scene (resumable at scene boundaries —
a segment and a sheet together mark a scene done), concatenates with
stream copy, and tiles sixteen strided frames per scene into the house
contact sheet (`fps=10, scale=480:-1, tile=4x4`).

## The receipts' rule, and its one exception

Inherited from the lab: **no number in a caption is typed by a human;
anything printed is measured.** The one exception is labelled as
itself — the distance narration (the falling number on the chip) is
the *story*, anchored to real measurements at both ends, and the film
quotes the measured edit→pixels latency beside it, named as this
bench's own.

## The palette and the type

Every colour the film's chrome draws is taken verbatim from
`apps/viewwsite/src/lib.rs` — the product page's palette (GROUND
`#0F0D0B` · INK `#F8F4F2` · ACCENT `#B491FF` · ACCENT_DEEP `#7E5CE8`,
the syntax ramp). The typography is the site's own Geist, included from
its assets. The studio renders in its own shipped theme exactly as the
product does — ungraded, un-vignetted, its pixels its own (the honesty
rule: visible ⇒ the product rendered it).

## Where things live

| path | what |
|---|---|
| `vieww_base/examples/film_lab/src/product_film/` | the film — 23 scenes, the master harness, the session script |
| `vieww_base/examples/film_lab/product_film_workspace/` | the studio workspace the session opens (the demo screens + `counter.say`) |
| `product_film/` (this directory) | the shipped artifacts — MP4, sheets, manifest, the 4K receipt, the calibration receipts, this README |
| `product_film/source/` (this directory) | a copy of the film's source, for review without the workspace |

The film's ancestors — `film_lab/keynote_z/` (THE SPARK) and its
predecessors — remain in the repository untouched. This is a different
film, built on the same harness discipline.
