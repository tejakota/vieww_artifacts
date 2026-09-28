# keynote_v5 — the viewwstudio release film

**The fifth build of the launch keynote — and the return to the native
medium.** Every frame of this film was rendered by **vieww itself**,
headless, through `FrameDriver` + `NativeRenderer` at 1920×1080 · 60 fps,
streamed raw into ffmpeg — no browser, no compositing, nothing added in
post. The closing sting is the contract: *this film was rendered with
vieww.*

**3:39 · 22 scenes · 13,140 frames · four acts · viewwstudio the hero,
vieww the foundation.**

| act | movement | scenes | the job |
|-----|----------|--------|---------|
| I · THE LIE | 起 | the terminal · silent truncation · rust lockout · **the reveal** | the old world's receipts — 41.7 MB before your first pixel, trees amputated at depth 3, rust caged — until one spark says *this is vieww* |
| II · THE FOUNDATION | 承 | 36 crates · own rasterizer · sub-pixel floor · one click · the ceilings | the engine beneath: its own canvas, 4× supersampling, byte-identical determinism, the hairline check down to 0.10 px, `.apk` + `.ipa` in one click |
| III · THE STUDIO | 転 | studio opens · first paint · live compose · the descent · **the swarm** · the fourier canvas · cross build · devtools mirror | **viewwstudio**, live: `say` types and the preview paints in measured seconds · per-keystroke compose · say→rust with state held · 2,400 boids inside a UI sheet · a choir of circles · onto real devices · the inspector eats the studio itself |
| IV · THE LEDGER | 結 | green ledger · 90 plates · built on vieww · the end card · the loop | receipts over claims: the test wall, the determinism wall, the pull-back — the studio is one more buffer on 36 crates · the end card audits itself |

## The receipts

House rule, inherited: **no number in this film is invented.** Each figure
is emitted by the pipeline it describes, or quoted from this repository's
own artifacts (sources in each scene's doc comment):

| on screen | source |
|-----------|--------|
| frames · shapes · glyph runs · glyphs · layers (the end card's bars) | the film's own two-pass census, run on the bench below — measured, never typed |
| `alive in 0.075 s` (S11's first paint) | the census's alive probe — median sampled edit→visible latency, this bench, at master resolution |
| 36 crates | `vieww_base/Cargo.toml` workspace members — counted by the code that draws them |
| 4× supersampling · 28 blend modes · 16-stop gradients | `vieww_base/README.md` engine capabilities |
| 2,400 boids · 2,511 shapes/frame · build 66.4 ms · raster 214.1 ms | `film_lab/renders/swarm/metrics.txt` |
| fourier · 2,004 shapes · 32 frames | `film_lab/renders/fourier/metrics.txt` |
| galaxy 60,527 shapes/frame · 101 ms · mandel 57,602 rects · 69 ms · hero4k 3840×2160 · 440–489 ms · longplay RSS 38.3→39.0 MiB | `film_lab/renders/*/metrics.txt`, quoted in the root README |
| 59.3 fps · 4.09 ms · Redmi Note 7 Pro (2019) | device-suite CI artifacts, quoted in the README |
| 518 + 237 + 13 tests · 0 open regressions · 90 plates | the root README and `KEYNOTE_FILM_PLAN.md` §2.3 |
| the staged build, under 15 s | the plan's own keynote-optimization checklist |
| the bench line on the end card | the manifest's `bench=` — per-bench determinism names its bench |

**This build's own manifest** (the census of this very film, this bench —
the numbers the end card's bars display):

```
frames=13140 · shapes=6,923,194 · glyph_runs=393,613 · glyphs=4,711,220
layers=96,074 (filtered 77,153) · strokes=798,006 · shadows=1,507
alive_seconds=0.075 · frame_ms=52.76 (median, 264 samples at 1080p)
bench=linux · x86_64 · rust 1.98.1 · 4 system fonts
```

## The artifacts

Per the storage policy: **only the MP4 and the per-scene contact sheets
ship in-tree** — no raw frames (the round-10 lesson: 1.4 GB of frame PNGs
helped nobody), no GIFs. The sheets are the house audit format
(`fps=10, scale=480:-1, tile=4x4`).

```
keynote_v5.mp4       the film — 1920×1080 · 60 fps · libx264 crf 18 ·
                     yuv420p · 3:39 · no audio track (the cut works muted)
sheets/S01_*.png …   22 contact sheets, one per scene, 16 strided
                     frames each — the visual audit of every scene
```

## Rebuilding the film

The source is the proof — the film is a Rust module in this repo:

```
vieww_base/examples/film_lab/src/keynote_v5/
├── mod.rs          the registry: 22 scenes, the witness ladder, the census probe
├── master.rs       the two-pass harness: census5 · master5 · k5:<scene>
├── studio.rs       the viewwstudio IDE kit — the hero's chrome
└── s01…s22         the scenes, one file each
```

```console
cd vieww_base
cargo run --release -p film_lab -- census5     # pass 1 — the audit → manifest.txt
cargo run --release -p film_lab -- master5     # pass 2 — the MP4 + the sheets
cargo run --release -p film_lab -- k5:swarm    # one scene, 16 frames, a sheet
```

Scenes are pure functions of film-time; the fixed clock means the census's
counts and the master's pixels agree by construction, and the film
re-renders to the byte on a given bench (the manifest names its bench).
The MP4 carries no audio — sound is the author's, later; every beat is
captioned by construction, so the cut works muted.

## The lineage

- **v1–v3** — MP4s rendered by vieww's own rasterizer (`film_lab/keynote/`,
  the 3:00 sixteen-scene cut; its source: `keynote/` in this crate).
- **v4** — the web-native build (`film_lab/keynote_v4/`): cinema where the
  browser is good at cinema — but the film around the plates was HTML/CSS/JS,
  and the honesty ledger said so.
- **v5** — this film: the release keynote script (the hook → architecture →
  live demo → guarantee arc, viewwstudio as hero, vieww the foundation)
  rebuilt in the native medium, on the v2/v3 two-pass harness, with v4's
  receipts discipline. The branches `keynote-v2/v3/v4` are deleted; main
  carries the canon.
