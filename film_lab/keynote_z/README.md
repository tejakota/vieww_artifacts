# keynote_z — THE SPARK · the viewwstudio release film

**The sixth build of the launch keynote — and the one that renders the
actual product.** Every frame of this film was rendered by **vieww
itself**, headless, through `FrameDriver` + `NativeRenderer` at
1920×1080 · 60 fps, raw RGBA streamed into ffmpeg — no browser, no
compositing, nothing added in post. And for the whole of Act III, what
the camera is pointed at is **the real `viewwstudio` app** — the same
`Shell`, the same `Studio` state, the same element runtime, the same
`rustc` pipeline — mounted on the film's own driver and driven by a
frame-indexed script through the real input pipeline. When the film
types, the studio's editor types. When the film taps, the studio's demo
navigates. When the film presses Render, `rustc` runs. The closing
sting is the contract: *this film was rendered with vieww.*

**3:41 · 24 scenes · 13,260 frames · four acts · viewwstudio the hero,
vieww the foundation.**

| act | movement | scenes | the job |
|-----|----------|--------|---------|
| I · THE SPARK | 起 | the blank · the wait · the break · **the spark** | the dark room and the blinking caret · the old world's 41.7 MB wait at a held 24 Hz · the fracture that lets the light in — and one point of violet light says *this is vieww* |
| II · THE MACHINE | 承 | the descent · the crates · the signal · the rasterizer · the damage · the cadence | the dive into the engine: 36 crates as a living city · the signal apart from the tree · the sub-pixel coverage grid, 28 blend modes, 16-stop gradients · damage in pixels · 24 vs 60 |
| III · THE STUDIO | 転 | studio opens · first paint · live compose · the tap · **say → rust** · the screens · the tokens · cross build · the mirror | **the actual app, driven live**: the live preview mounts with no compile · a keystroke repaints it · the real damage overlay · a real tap through the real pipeline · `counter.say` typed, compiled, tapped, re-compiled with its `keep`ed count intact · the card grid on android/ios/desktop frames · the token editor re-themes the studio in one accent · export and devices · devtools on itself |
| IV · THE SHIP | 結 | the apps · the ledger · the pullback · the end card · the loop | the four shipped apps with their own receipts · the green ledger (518 + 237 + 13) · the pull-back — the studio is one buffer on 36 crates · the end card audits itself · the loop back to the caret |

## The palette and the type are the brand's

The film's own chrome — captions, chips, the spark, the wordmark, the
end card — is drawn in the **viewwsite's palette, verbatim**
(`apps/viewwsite/src/lib.rs`): the warm ground `#0F0D0B`, the ink
`#F8F4F2`, the accent ramp `#7E5CE8 → #B491FF` (which is the studio's
own PURPLE brand pair exactly), the wash `#221C33`, and the site's
syntax ramp for the engine register. The type is the site's own Geist
and Geist Mono, included byte-for-byte from the site's assets. The
studio renders in **its own shipped theme and fonts**, ungraded — no
vignette, no grain, no flash ever crosses its pixels: what you see of
the product is what the product draws.

## The receipts

House rule, inherited: **no number in this film is invented.**

| on screen | source |
|-----------|--------|
| `alive in 0.188 s` (S12's keystroke) | the census's alive probe — the measured edit→pixels cost of one real `studio.edit` through the real studio, at master resolution, this bench |
| frames · shapes · glyph runs · glyphs · layers (the end card's bars) | the film's own two-pass census — the real studio's draw commands included |
| 36 crates | `vieww_base/Cargo.toml` workspace members — counted by the code that draws them |
| 4× supersampling · 28 blend modes · 16-stop gradients · hairline 0.10 px | the engine's own capabilities (vieww-paint's compositing module, quoted) |
| 41.7 MB · 40.2 MB engine · 1,847 lines of bindings · 1.1 GB framework | the reference script's own receipts of the old world (quoted, source named) |
| 59.3 fps · 4.09 ms · Redmi Note 7 Pro (2019) | the device-suite CI artifacts, quoted in the root README |
| 518 + 237 + 13 tests · 0 open regressions · 90 plates | the root README and the repo's own receipts |
| three 47 · upxcale 21 · SnapSearch 38 (47 with clip) tests | the apps' own READMEs |
| the taps' hit-tested coordinates | `kzcal`, the calibration mode — pixel-diff verified |

**This build's own manifest** (the census of this very film, this
bench — the numbers the end card's bars display):

```
frames=13260 · shapes=4,662,705 · glyph_runs=1,680,129
layers=69,703 (filtered 38,790) · strokes=266,112
alive_seconds=0.188 · frame_ms=144.63 (median, 358 samples at 1080p)
bench=linux · x86_64 · rust 1.98.1 · 5 embedded faces
```

## The determinism

A frame is a function of the checkout: the fixed clock, the
frame-indexed script (`script.rs` — every action at a named film-time),
compiles settled off-clock (the studio's real `Compiling` state is held
for its scripted frames because `poll_compile` is what harvests
completion), and a font store that is **embedded + the site's Geist,
both in-tree** — no system scan, so glyph metrics are a property of the
repository rather than the machine. The bench line names its bench.

## The artifacts

Per the storage policy: **only the MP4 and the per-scene contact sheets
ship in-tree** — no raw frames, no GIFs. The sheets are the house audit
format (`fps=10, scale=480:-1, tile=4x4`).

```
keynote_z.mp4       the film — 1920×1080 · 60 fps · libx264 crf 18 ·
                    yuv420p · 3:41 · no audio track (the cut works muted)
sheets/S01_*.png …  24 contact sheets, one per scene, 16 strided
                    frames each — the visual audit of every scene
```

## Rebuilding the film

The source is the proof — the film is a Rust module in this repo:

```
vieww_base/examples/film_lab/src/keynote_z/
├── mod.rs          the registry (24 scenes, two kinds), the viewwsite palette,
│                   the Geist font store, the spark and its grammar
├── master.rs       the two-pass harness: censusz · masterz · kzcal · kz:<scene>
├── script.rs       the session — the frame-indexed script that drives the
│                   REAL studio (actions, taps, compiles, overlays)
└── s01…s24         the scenes: pure graphics (I, II, IV) and overlay chrome
                    over the mounted Shell (III)
```

plus the film's workspace, `examples/film_lab/keynote_z_workspace/` —
the studio's own demo screens plus the film's `counter.say`.

```console
cd vieww_base
cargo run --release -p film_lab -- kzcal        # verify the taps (hit-tested)
cargo run --release -p film_lab -- censusz      # pass 1 — the audit → manifest.txt
cargo run --release -p film_lab -- masterz      # pass 2 — the MP4 + the sheets
cargo run --release -p film_lab -- kz:spark     # one scene, 16 frames, a sheet
```

The MP4 carries no audio — sound is the author's, later; every beat is
captioned by construction, so the cut works muted.

## The lineage

- **v1–v3** — MP4s rendered by vieww's own rasterizer (the 3:00
  sixteen-scene cut; `film_lab/keynote/`).
- **v4** — the web-native build (`film_lab/keynote_v4/`): cinema where
  the browser is good at cinema — but the film around the plates was
  HTML/CSS/JS, and the honesty ledger said so.
- **v5** — the return to the native medium (`film_lab/keynote_v5/`):
  every frame vieww's, the studio recreated as a mock — honest about
  being a stand-in, and still a stand-in.
- **z** — this film: the studio scenes are the **actual app**, driven
  through the real input pipeline and the real compile path; the
  palette and typography are the brand's own, from the product page.
