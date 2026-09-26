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

| app | what it is | status |
|---|---|---|
| **3** (`three/`) | temporal 3D capture and the `.3` binary container, plus the social product around it — profiles, feeds, remixes. `three-vieww` owns media state + presentation prep for Vieww. | migrated + enriched; the moment card, lifted viewer chamber, filled-primary transport; 80 tests pass against `vieww_base` |
| **vavlt** (`vavlt/`) | consent-first photo/video vault. One Vieww UI (`crates/app`) mounted by desktop, Android and iOS hosts; codec measurement harness with audit chain. | migrated + enriched; real frosted-glass buttons, inset-shadow tier channel, ghost wall that fits; the screenshot suite runs adapter-free |
| **upxcale** (`upxcale/`) | photo upscaler — Lanczos-3 resampler + unsharp mask behind a real render flow (grid → picker → progress → compare). | migrated + enriched; frosted picker scrim, route transitions, one elevation language; 21 tests pass |
| **SnapSearch** (`snapsearch/`) | find a photo by describing it, or by handing it one. Joint embedding space; local appearance encoder by default, CLIP ViT-B/32 behind `--features clip`. | migrated + enriched; 38 tests (47 with clip), 8 real screens re-rendered through the native rasteriser |

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
- **SnapSearch** — `src/bin/screenshot.rs`, the app's eight states
  including a live encoder-progress frame and the staggered reveal.
- **3** — the receipts test in `examples/social-app` mounts the social
  shell through `vieww-test-harness`; the GIF's thirty frames are thirty
  real renders of the 3-second `demo_capture()` playing through the
  actual frame loop, the way the feed shows it.
- **vavlt** — the screenshot suite (`crates/app/tests/screenshots.rs`)
  renders phone and desktop, dark and light — forty PNGs; the sheet
  carries eight of them.

The archives these folders replaced were removed once extracted; this
pass replaced the reproductions the first receipts shipped with the
renders themselves.
