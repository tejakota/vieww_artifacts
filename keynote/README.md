# keynote — the viewwstudio release film

**5:30 · 18 scenes · 19,800 frames · four movements · viewwstudio the hero,
vieww the foundation.**

Every frame of this film was rendered by **vieww itself** — headless, through
`FrameDriver` + `vieww-paint`'s `native` rasterizer, at 1920×1080 · 60 fps,
streamed raw into ffmpeg. No browser, no compositor, no after-effects pass.
The closing sting is a contract: *every frame of this film was rendered by
vieww.*

| act | movement | scenes | the job |
|-----|----------|--------|---------|
| I · **THE WAIT** | 起 | the wait · the cost · the question · the mark | the world the film argues against, shown in its own texture — a build that regresses, a year of rebuilds counted on screen, one sentence, and then the name |
| II · **THE STUDIO** | 承 | opens · first paint · compose · the descent · the machine room · damage | **viewwstudio**, live: the shell assembles, three lines of `say` bring a preview alive in a measured number of seconds, an error renders as a boundary without stopping its siblings, the buffer becomes Rust with the program still running, the preview turns out to be a real renderer, and one write lights one rectangle |
| III · **THE TWISTS** | 転 | the mirror · the world tour · the foundation · the unfold | the inspector turned on the studio itself · one session on three renderers, where a tap on the phone lands **7** everywhere · the 36-crate stack the session was standing on · the three trees pulled apart, and the signal outside all of them |
| IV · **THE LEDGER** | 結 | the ledger · one click · the end card · the loop | receipts and refusals in the same type · three targets exported on screen · the end card that audits the film with the film's own census · the last frame, which is the first |

## The receipts

House rule: **no number in this film is typed by a human.** Every figure is
either measured by the render that prints it, or quoted from this repository
with its source named on screen.

| on screen | source |
|-----------|--------|
| the end card's bars — shapes · glyphs · strokes · glyph runs · layers · blurred layers | this film's own two-pass census (`manifest.txt`), measured on the bench named beside them |
| `alive in N seconds` (S06's first paint) | the census's alive probe: the median whole-frame raster time through S06's paint-in window, this bench, at master resolution |
| S09's per-frame ceilings | the same census, divided by the frame count |
| S10's `10 writes · 1 rebuild · 1 damage rect` | the scene's own arithmetic, drawn as ten ticks and one bar |
| **36 crates** | `vieww_base/Cargo.toml`'s workspace members, counted at render time by the code that draws them — `film::count_crates` |
| `518 + 237 + 13 tests` · `90 plates` · `0 open regressions` | the repository's root `README.md` and `todo-upgrades.md` |
| `59.3 fps · 4.09 ms` · Redmi Note 7 Pro (2019, Adreno 612) | `ci/mobile/device-suite.sh` CI artifacts — **labelled on screen as quoted, not measured by this render** |
| the bench line | `Probe::bench_identity` — os, arch, toolchain and the host's font count, because glyph determinism follows the host's fonts |

**This build's manifest** (the census of this very film, on this bench — the
numbers the end card's bars display):

```
frames=19800 · shapes=13,571,694 · glyph_runs=956,854 · glyphs=8,428,859
layers=65,208 (filtered 42,663) · strokes=431,175 · shadows=70,762
alive_seconds=0.254 · frame_ms=168.71 (median, 216 samples at 1080p)
bench=linux · x86_64 · 7 system fonts
```

### What the film does not claim

The ledger scene prints these on screen, in the same type and at the same
size as the green:

- **iOS** — disputed by the codebase's own docs, and excluded from every
  claim in the film, including the export scene where claiming it would have
  been free.
- **The Android figures** — quoted from CI artifacts, not measured by this
  render. The card says so.
- **Sound** — deferred. The master carries no audio track, and every beat is
  captioned, so the cut works muted by construction.
- **Byte-identical** — per bench. This render names its own.

## The palette is the product's

Two palettes, deliberately kept apart:

- **The film's** is `apps/viewwsite`'s, value for value: the warm near-black
  ground `#0F0D0B`, the stone inks `#F8F4F2 / #A69C95 / #78716B`, the
  lavender accent `#B491FF` over `#7E5CE8`, the wash `#221C33`, and the
  site's own syntax ramp for the accent colours.
- **The studio's** is `apps/viewwstudio`'s `StudioTheme::dark()`, also value
  for value: `#0E0E0E` window, the `#181818 → #2F2F2F` chrome ladder, the
  `#2B2B2B` hairline, the `#82898F` gutter, and the product's own syntax
  colours.

The studio is drawn as the **actual product**, not a mock: the card shell
(`chrome::card` — sheen, rim, hairline, elevation, a 6 pt gutter and an 8 pt
corner), the twelve activity views in the product's own order, the 36 pt
title bar, 48 pt activity rail, 24 pt status bar, 35 pt tab strip, 26 pt
breadcrumb, 64 pt minimap and 176 pt bottom panel — every metric read out of
`apps/viewwstudio/src`.

## The artifacts

Per the storage policy, only the MP4 and the per-scene contact sheets ship in
tree — **no frame PNGs.**

```
keynote.mp4        the film — 1920×1080 · 60 fps · libx264 crf 18 ·
                   yuv420p · 5:30 · no audio track (the cut works muted)
manifest.txt       the census this render printed on its own end card
sheets/S01_*.png   18 contact sheets, one per scene, 16 strided frames
                   each — the visual audit of every scene
```

## Rebuilding the film

The source is the proof. The film is **its own crate**, depending on the
released framework exactly the way any other application does:

```
vieww_base/examples/keynote/
├── src/main.rs        the CLI
├── src/film.rs        the act table, the session spine, the census probe, verify
├── src/kit.rs         the palette, easing, deterministic RNG, atmosphere, type
├── src/studio.rs      viewwstudio, redrawn — the hero's chrome
├── src/solid.rs       the film's own little 3D: vectors, camera, quad meshes
├── src/master.rs      the two-pass harness: census · master · preview
└── src/scenes/        eighteen scenes, one file each, plus the shared buffer
```

```console
cd vieww_base
cargo run --release -p keynote -- chapters   # the act table, derived from the score
cargo run --release -p keynote -- verify     # the gates
cargo run --release -p keynote -- census     # pass 1 → manifest.txt
cargo run --release -p keynote -- master     # pass 2 → keynote.mp4 + sheets
cargo run --release -p keynote -- S09        # one scene, 16 frames, a sheet
cargo run --release -p keynote -- publish ../keynote
```

### Why two passes

The end card displays the film's own totals, and the film is not finished
until the end card is drawn. Pass 1 walks all 19,800 frames and counts the
command stream the framework emits, writing `manifest.txt`; pass 2 renders
with the end card reading that file. The end card's own contribution is
layout-stable, so pass 2 is the fixed point.

### The gates

`keynote verify` fails the build on any of:

1. the act sums not closing against the runtime (60 + 120 + 80 + 70 = 330 s);
2. the frame count not equal to the runtime at the master's rate;
3. scene starts that do not tile exactly — the mount-frame rule, stated once
   so it cannot surprise anyone;
4. a witness ladder that is not monotone, does not land inside its scenes, or
   does not reach and hold 7;
5. duplicate scene ids or a non-positive duration;
6. a master resolution the sheets and cutdowns do not assume;
7. a workspace crate count that cannot be read — because the end card would
   otherwise print a number nobody measured.

## Notes on construction

- **Durations are the only numbers anyone writes.** Frame counts, act sums,
  the runtime, every scene's start and every rung of the witness ladder are
  computed from the act table in `film.rs`. The film's length is a
  consequence of its shot list.
- **The session spine lives outside every scene.** The witness counter, the
  elapsed chip and the session line are derived from absolute film-time and
  read by scenes that mount and unmount around them — which is why the
  counter survives every cut in Act II, and why S14 can pull the signal apart
  from the three trees and have it be literally true of the film's own
  construction.
- **Determinism.** Nothing reads the wall clock or a global RNG. Noise is
  seeded per call site; a scene is a pure function of its own time; the
  census's counts and the master's pixels agree by construction.
- **Memory.** Frames stream straight into ffmpeg — 19,800 frames at 8.3 MB
  each is 164 GB nobody has. Only the sixteen a contact sheet needs ever
  touch disk, and they are deleted once tiled.
- **`film_lab/` is not in this film.** That folder is the capability bench:
  plates that prove what the framework can do. The film borrows its
  *techniques* — the seeded RNG, the analytic spring, the layer-pricing
  discipline, the boids' grid neighbourhood, the Fourier choir — and none of
  its output.
