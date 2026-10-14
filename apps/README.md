This folder consists of real apps that must be deployed in mobiles and
desktops (if supported).

## The four apps

All four are migrated off the old vello-backed vieww onto the in-repo
`vieww_base` checkout — `vieww_paint::native::NativeRenderer` is the one
renderer, headless and windowed alike (see `vieww_base/docs/
RENDERER-MIGRATION.md`) — and each carries the UI enrichment pass: real
frosted glass where a flat translucency stood in, transitions and elevation
where screens appeared instantly and floated flat, and the host platform's
control shapes through `ThemeData::platform`.

Each app also carries a **round-3 capability ramp-up**: the new framework
crates put to work rather than merely linked. `upxcale` measures what it
ships (the compare sheet's `vieww-dataviz` chart of plain-4x vs sharpened);
`snapsearch` shows the embedding space it promises (a `vieww-dataviz::force`
map of the library, matches ringed in viridis); `vavlt` composes the grant
(the Activity tab's `vieww-dataviz::hierarchy` treemap of what was handed
over); `three` renders its captures through the framework's own 3D stack
(`vieww-3d`: depth buffer, lights, shadows) by default.

| app | what it is | status |
|---|---|---|
| **3** (`three/`) | temporal 3D capture and the `.3` binary container, plus the social product around it — profiles, feeds, remixes. `three-vieww` owns media state + presentation prep for Vieww. | round 3: the capture renders through `vieww-3d` by default (engine.rs: MeshFrame bridge, orbit-camera mapping, light rig, occluded instanced motes, `Viewport3D`); the painter stays a toggle away and remains the wireframe path; 37 tests in `vieww-integration` |
| **vavlt** (`vavlt/`) | consent-first photo/video vault. One Vieww UI (`crates/app`) mounted by desktop, Android and iOS hosts; codec measurement harness with audit chain. | round 3: the Activity tab's composition card — what was handed over as a `vieww-dataviz::hierarchy` treemap (by class, by bytes, the engine's own numbers); 5 new unit tests; a mixed-vault screenshot state |
| **upxcale** (`upxcale/`) | photo upscaler — Lanczos-3 resampler + unsharp mask behind a real render flow (grid → picker → progress → compare). | enriched round 2: landing masthead with a live count chip, fresh-tile success mark, ramped AFTER chip on the compare stage; 21 tests pass |
| **SnapSearch** (`snapsearch/`) | find a photo by describing it, or by handing it one. Joint embedding space; local appearance encoder by default, CLIP ViT-B/32 behind `--features clip`. | round 3: the Space view — the library placed on a deterministic `vieww-dataviz::force` map of its own embeddings, kNN cosine links, mean-colour dots, viridis match rings, tap-to-open; 43 tests |

## renders/

Each app ships the three receipts, following the `film_lab/renders`
convention:

- `sheet.png` — the audit surface: every screen of the app's story on one
  dark plate, numbered and captioned.
- `anim.gif` — the motion receipt: the app's flow as a seamless loop
  (two-pass palette GIF, floyd-steinberg dither — the house recipe).
- `metrics.txt` — the measured numbers, including the `gif=` receipt line.

Every receipt is now a **real render** of the real app through the native
rasteriser, headless, no display and no adapter:

- **upxcale** — `examples/shots.rs`, its clock advanced so route
  transitions settle before capture.
- **SnapSearch** — `src/bin/screenshot.rs`, the app's nine states
  including a live encoder-progress frame, the staggered reveal, and the
  no-match empty state.
- **3** — the receipts test in `examples/social-app` mounts the social
  shell through `vieww-test-harness`; the GIF's thirty frames are thirty
  real renders of the 3-second `demo_capture()` playing through the
  actual frame loop, the way the feed shows it — engine-rendered now,
  through `vieww-3d`.
- **vavlt** — the screenshot suite (`crates/app/tests/screenshots.rs`)
  renders phone and desktop, dark and light — forty PNGs; the sheet
  carries eight of them.

The archives these folders replaced were removed once extracted; this
pass replaced the reproductions the first receipts shipped with the
renders themselves.

Every receipt is regenerable: each app carries `tools/make_receipts.py`,
which composes the sheet and the GIF from the screenshot tool's own
output (`shots/`, `target/screens`, `target/moment`) — the two-pass
palette, floyd-steinberg-dithered GIF recipe and the dark-plate sheet
layout live in one script per app rather than in a session's history.
