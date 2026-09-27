# keynote_v4 — the viewwstudio release film, web-native

**The fourth build of the launch keynote — and a change of medium.**
v1–v3 were MP4s rendered by vieww's own rasterizer, and they hit that
rasterizer's ceiling: the verdict on the cut was *"the worst — a skip, not
a watch."* v4 makes the out-of-the-box move: **the film is a web page** —
one folder, no server, no build step, that plays itself as a timed film
*and* hands the presenter a stage. Cinema where the browser is good at
cinema (type, glass, 3D, springs, live particles), receipts where the repo
is already the authority (every number on screen is quoted from this
repository's own artifacts, sources listed below).

```
open index.html          # that's it — everything is local
```

## The film

**4:42 · 24 scenes · four movements, kishōtenketsu kept explicit.**
The hero is **viewwstudio**; **vieww** is revealed as its foundation.

| act | movement | scenes | the job |
|-----|----------|--------|---------|
| I · 起 | THE WAIT | the wait · the question · the genesis | the world as it is — degraded-crafted, judder and all — until one drop becomes the wordmark |
| II · 承 | THE SESSION | the studio opens → first paint → the descent → compose live → the receipt → state survives → damage in pixels → the world tour | one buffer, one preview, one session; the witness counter climbs **1…7** |
| III · 転 | THE TWISTS | the mirror · the machine room · the receipts · the unfold | devtools on the studio itself · 2,400 live boids + the fourier choir + the ceilings wall · 59.3 fps on a 2019 phone · the three trees, and the signal apart |
| IV · 結 | THE RECONCILIATION | the fold-back · built on vieww · the green ledger · the end card · the loop | everything you watched was one thing · 36 crates beneath the studio · receipts over claims · the end card is one more buffer |

## Presenting it

| input | does |
|-------|------|
| `space` / click stage | play · pause (pause freezes the film clock — typing, taps and captions all hold) |
| `←` `→` | previous / next scene (full choreography) |
| `↑` `↓` | previous / next act |
| `G` | chapter grid — jump anywhere |
| `S` | the synthesized score (muted by default; WebAudio, no files) |
| `F` | fullscreen |
| timeline | hover names the scene · click jumps |

The film **pauses itself when the tab loses focus**, keeps an honest
session clock, and works from `file://` with zero network — fonts are
embedded, plates are local GIFs. Best on Chrome/Edge/Firefox, 1080p+,
fullscreen.

## The receipts — where every number comes from

The house rule survives the medium change: **no number in this film is
invented.** Each figure is quoted from this repository's own artifacts:

| on screen | source |
|-----------|--------|
| `alive in 0.246 s` | `film_lab/keynote_v3/manifest.txt` — the census3 alive probe, this bench |
| swarm · 2,511 shapes/frame · build 66.4 ms · raster 214.1 ms | `film_lab/renders/swarm/metrics.txt` (40,179 shapes ÷ 16 frames; 2,400 boids per the README) |
| fourier · 2,004 shapes · 32 frames | `film_lab/renders/fourier/metrics.txt` |
| galaxy 60,527 shapes/frame · 101 ms · mandel 57,602 rects · 69 ms · megapath one line, 40,000 segments · hero4k 3840×2160, 440–489 ms · longplay 256 frames, RSS 38.3→39.0 MiB | repo `README.md`, rounds 6–7 receipts |
| 59.3 fps · 4.09 ms · Redmi Note 7 Pro (2019) | device-suite CI artifacts, quoted in README §Family D |
| 90 plates · 518 + 237 + 13 tests · U-01…U-23, 0 open · four apps | repo `README.md` + `todo-upgrades.md` |
| 36 crates · 4× supersampling · 28 blend modes · 16-stop gradients | `vieww_base/crates/` (counted) + README engine capabilities |
| the end card's manifest bars (scenes · frames · shapes · taps) | **measured live in your browser by the page itself** — the web film audits itself the way v2/v3's manifests audited theirs |

Two honesty notes, in the ledger's spirit:

1. **The plates are vieww's; the page is not.** Every GIF on the machine
   room and the ledger wall is a real `vieww` render (sheet · gif ·
   metrics — the three-artifact set). The film around them is HTML/CSS/JS.
   The end-card sting says exactly that and nothing more.
2. **The live canvas scenes are siblings, not clones.** The 2,400-boid
   swarm and the fourier choir run live in your browser beside their
   plate receipts — same rules, other bench. The captions keep the two
   apart.

## The build

```
index.html          the stage, the chrome, the overlays
film.css            the design system — brand tokens + every scene
js/data.js          the script: scene table, code, copy, receipts
js/engine.js        the clock, scene lifecycle, session rail, timeline
js/fx.js            ambient dust, springs, glyph sprites + the WebAudio score
js/act1..4.js       the four movements
assets/fonts.css    Geist + Geist Mono, subset, embedded (SIL OFL — see assets/OFL.txt)
assets/plates/      15 real vieww renders, copied from film_lab/renders
```

No framework, no dependencies, no build step — the way a conference-hall
laptop likes it. The fonts are subset to the film's charset with
`fonttools` and inlined as woff2 data URIs so `file://` presents with
zero network. Regenerate with `python3 scripts/subset_fonts.py`-equivalent
if the charset ever grows.

## If you must have an MP4

Present the page; or screen-record it at 1080p60 — the film is
frame-paced to real time and captions carry every beat, so a capture is
faithful. A native render pipeline (Playwright → ffmpeg) is a
straightforward follow-up and deliberately not faked here.
