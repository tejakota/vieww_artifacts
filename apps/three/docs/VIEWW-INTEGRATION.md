# Vieww integration

The repository assumes **Vieww is the UI framework** for `3`.

Do not make the `.3` stack depend on Vieww. Instead:

1. `three-capture` records video with depth through the `DepthVideoSource`
   trait (camera2 / AVFoundation / synthetic).
2. `three-format` serializes it as `.3` v1 — JPEG color, zstd depth, one
   camera model, a source kind.
3. `three-runtime` plays it (`DepthVideoPlayer`), records it
   (`Recording`/`CaptureSession`), and imports real video into it
   (`import_mp4`).
4. `three-vieww` renders it: `warp_frame` re-photographs a frame from the
   viewer's `ViewCamera`; `DepthView` is the lazy-capture + player +
   camera + one-decoded-frame state machine a surface drives.
5. `three-app` owns social application state.
6. Vieww screens render `three-app` state and `DepthView` renders.

## The two halves of the boundary

```text
        (no Vieww dependency)                (Vieww side)
  ┌──────────────────────────────┐   ┌───────────────────────────────────┐
  │ three-vieww                  │   │ examples/vieww-integration        │
  │                              │   │                                   │
  │  DepthView ─ media state,   │   │  ViewerScreen ─ root widget       │
  │   one cached decoded frame   │──▶│  PlaybackState ─ element state     │
  │  ViewCamera ─ orbit/dolly/  │   │  ThreeMediaView ─ gesture surface  │
  │   recentre spring           │   │  ControlsBar — transport/scrub     │
  │  warp_frame ─ the splat      │   │  (pixels go into an Image widget)  │
  └──────────────────────────────┘   └───────────────────────────────────┘
```

The Vieww side owns elements, gesture recognizers and widgets; the media
side owns everything a pixel depends on. The pixels themselves cross as
`RenderedFrame` — shared `Arc<Vec<u8>>` RGBA — and land in the framework's
`Image` widget, which brings its own rasterizer and `BoxFit` for free.

## Why an Image widget and not a Painter

The warp produces complete RGBA frames; the framework's rasterizer already
draws complete RGBA frames through `DrawImage`, with damage tracking and
`BoxFit` letterboxing. Re-implementing a blit inside a `Painter` would
duplicate what the renderer does better — the old mesh viewer needed a
painter because it projected triangles; a depth video needs a photograph.

## The gesture wiring

`ThreeMediaView` wraps the image in one `GestureDetector`:

- `on_drag_update` → `ViewCamera::drag` (pan = orbit)
- `on_scale_update`/`on_scale_end` → incremental `pinch` steps (dolly)
- `on_scroll` → `zoom` (desktop's dolly)
- `on_long_press` → `begin_recentre` (**hold** = the spring home)

Writes reach the `PlaybackState` through the element's state handle; the
next `tick` renders with the new camera and rebuilds the image.

## The frame loop

`PlaybackState::tick` advances the player while playing, steps the
recenter spring, renders the current frame (through the cache: same frame +
same camera ⇒ no re-render), and returns whether anything is animating. A
paused, neutral viewer renders nothing and lets the loop sleep — the same
discipline the framework's own media examples follow.

## Running it

```bash
cargo run -p vieww-integration                       # the sample capture
cargo run -p vieww-integration -- ./imported.3      # any imported .3
cargo test -p vieww-integration                      # headless, CPU rasterizer
cargo test -p vieww-integration --test previews -- --ignored --nocapture
```
