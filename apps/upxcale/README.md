# upxcale

Upscale photographs. A [`vieww`](../vieww) application.

Built from the HTML prototype in `upscale_app/prototype/`, module for module,
so the two can be read side by side.

```console
# desktop harness — a phone-shaped window
cargo run --release

# every screen state to a PNG, no window and no GPU
cargo run --example shots --features cpu --release -- shots/

# the tests
cargo test
```

**Run it in release.** A debug `vello` is roughly an order of magnitude slower,
and an impression of the frame rate from one is an impression of `rustc -O0`.

## Layout

`upxcale/` expects `vieww/` beside it — the dependencies are path deps:

```
parent/
├── vieww/      ← the framework
└── upxcale/    ← this app
```

It is its own workspace, not a member of vieww's, so building it can never
change how the framework's own CI resolves.

## What it does

The landing screen opens on a masonry of photographs with a **Render** button
floating at the bottom. Tapping it opens the picker; choosing photographs and
confirming runs the upscaler behind a progress overlay; the results replace the
grid, each wearing a `4x UPSCALED` badge and a ring that fades after a beat.
Tapping a result opens a before/after sheet with a divider you drag.

### The upscaler is real

This is the one place the app does more than the prototype did. There,
"upscaled" was a CSS `filter: contrast() saturate() brightness()` over a
higher-resolution copy of the same URL — exactly right for a prototype, and a
lie in an application.

`src/upscale.rs` is a **Lanczos-3 resampler** followed by an **unsharp mask**:

- Separable, so a 6-tap horizontal pass then a 6-tap vertical one rather than a
  36-tap 2D convolution.
- Premultiplied throughout, so a transparent region cannot bleed its colour
  channels into an opaque neighbour.
- Weights normalised per output sample, so a flat field stays flat.
- Sampled at pixel *centres*, so the picture does not drift half a pixel toward
  the origin.

Measured against the twelve photographs the app ships, the sharpening pass
lifts acutance **8–13%** over a plain enlargement of identical size.
`tests/upscale.rs` asserts that on every one of them.

That test exists because an earlier version of these parameters measured
beautifully on a synthetic checkerboard and did **nothing at all** to a real
photograph: a checkerboard is 20-vs-235 at every edge, so it clears any
threshold and survives any blur radius. The mask's radius has to scale with the
enlargement factor, which is the kind of thing only a real input tells you.

## Module map

Each module answers to one file of the prototype. A change that was scoped to
one CSS file there is scoped to one module here.

| prototype file | module |
|---|---|
| `shared/theme.css` | `src/theme.rs` |
| `screens/landing/screen.css` | `src/screens/landing/screen.rs` |
| `screens/landing/backdrop-blur.css` | `src/screens/landing/backdrop.rs` |
| `screens/landing/photo-grid.css` | `src/screens/landing/photo_grid.rs` |
| `screens/landing/photo-tile.css` | `src/screens/landing/photo_tile.rs` |
| `screens/landing/render-button.css` | `src/screens/landing/render_button.rs` |
| `screens/landing/picker-dialog.css` | `src/screens/landing/picker_dialog.rs` |
| `screens/landing/progress-overlay.css` | `src/screens/landing/progress_overlay.rs` |
| `screens/landing/compare-sheet.css` | `src/screens/landing/compare_sheet.rs` |
| `app.js` | `src/state.rs` |

`shared/foundation.css`, `shared/widgets.css` and `shared/controls.css` have no
counterpart on purpose — they were the prototype's stand-in for
`vieww-foundation` and `vieww-widget`, and the real thing has replaced them.

Four modules have no prototype counterpart, because the prototype did not do
the work: `upscale.rs` (the resampler), `render.rs` (running it off the frame
thread), `export.rs` (writing a PNG back out), and `icons.rs` (the glyphs the
framework does not ship).

## Where the prototype and the app deliberately differ

Six places. Each is a decision, not an oversight.

**The picker no longer opens on launch.** The app opens to photographs with the
Render button waiting, and the button is what opens the picker. This is the
change you asked for after reviewing the prototype.

**The Render button is an extended FAB, composed by hand.**
`vieww_widget::FloatingActionButton` is a 56pt circle carrying one icon; its
`label()` is the screen-reader string and is never painted. "Render" is the
screen's primary action and a bare sparkle would not say so, so
`render_button.rs` composes `Pressable` + `Container` + `Flex` + `Icon` +
`Text`. `controls/` exists so an application can write its own control the same
way the built-in ones are written.

**The masonry packs itself.** `GridView` is a fixed column count at a *uniform*
cell extent, and the whole point of this screen is that the cells are not
uniform. `photo_grid.rs` walks the photographs and puts each into whichever
column is currently shortest — the same algorithm CSS `column-count: 2` was
running in the browser. It is O(n) per build, which is right at a dozen
photographs and would not be at ten thousand; the fix at that size is to
virtualise, not to pack more cleverly.

**The blur behind a popup is gone.** vieww has no `BackdropFilter` — that is a
compositor feature, and faking it means rendering the body to an offscreen
layer, blurring it, and compositing it back, every frame a popup is up, against
a 16.67ms budget. `backdrop.rs` uses a deeper scrim instead. If vieww grows a
real backdrop filter, that is the one file that changes.

**The progress bar tells the truth.** The prototype's was a `setInterval`
adding a random amount. Here the worker reports each stage it finishes — two
per photograph, the resample and the sharpen — and the bar is that count. It
cannot reach 90% and sit there, and it cannot reach 100% before the pixels
exist.

**"Save to Photos" writes a file.** There is no photo-library service in vieww;
`docs/PRODUCTION-GAPS.md` lists camera, geolocation, push and biometrics as
each needing per-platform FFI, and this is the same shape of gap. `export.rs`
writes a PNG into `upxcale-exports/` and says where it went. That is a real
save, and it is deliberately not dressed up as the integration the label
implies.

## Threading

`vieww` owns no executor — `Spawn` is `Box<dyn FnOnce() + Send>` — and neither
does this. `main.rs` passes `Threads`; `tests/` and `examples/shots.rs` pass
`Inline` and the whole render runs synchronously with no other change. An app
that already ran tokio would pass its own and nothing else would move.

The worker is handed *pixels*, never a `Photo` or the `Library`: neither is
`Send`, so keeping the thread's inputs to plain data is a compile-time fact
rather than a rule to remember.

## Assets

Twelve procedural landscapes in `assets/photos/`, 200–340px on the long edge,
regenerated by `tools/make_photos.py`. They are synthetic on purpose: the app
needs *low-resolution* sources so that upscaling them is a visible operation,
and it needs them under a licence nobody has to check. Generation is seeded by
name, so re-running the script never produces a diff.

A real build would populate the catalogue from the device's photo library. The
picker's job and the grid's job do not change when it does, which is why every
screen takes `&[Photo]` rather than reaching for the constant.

## What has not been checked here

The app was built and verified on a machine with no display, so:

- **Every screen state renders** — `examples/shots.rs` drives the real tree
  through all five and rasterises them through vieww's CPU backend, which
  produces the same `Scene` the GPU one does.
- **20 tests pass**, covering the resampler's properties, the PNG round trip,
  the icon geometry, and the whole screen flow from first open through render
  to comparison.
- **`cargo clippy --all-targets` is clean.**

Not checked, and needing a device:

- The frame budget. vieww's `ci/device-suite.sh` is the tool for that.
- Touch: the compare drag, the scroll physics, and the fling.
- `SafeArea` insets, which read zero headlessly because there is no notch to
  inset from.
- The Android build. `android_main` is wired in `main.rs`, but shipping an APK
  needs `cargo-apk` and an attached phone.
