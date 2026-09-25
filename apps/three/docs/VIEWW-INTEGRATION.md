# Vieww integration

The repository assumes **Vieww is the UI framework** for `3`.

Do not make `.3` depend on Vieww. Instead:

1. `three-capture` obtains native camera/depth/IMU observations.
2. `three-reconstruction` turns observations into temporal 3D.
3. `three-format` serializes the result as `.3`.
4. `three-runtime` loads and seeks `.3` media.
5. `three-vieww` adapts the media runtime to Vieww.
6. `three-app` owns social application state.
7. Vieww screens render `three-app` state and `ThreeView`.

That layering is now **implemented**, not just planned: the reference viewer
in `examples/vieww-integration` is a real Vieww application. This document
says where each piece lives and why.

## The two halves of the boundary

```text
        (no Vieww dependency)                (Vieww side)
  ┌──────────────────────────────┐   ┌───────────────────────────────────┐
  │ three-vieww                  │   │ examples/vieww-integration        │
  │                              │   │                                   │
  │  ThreeView  ─ media clock,   │   │  ViewerScreen ─ root widget       │
  │  OrbitCamera   frame picking │──▶│  PlaybackState ─ element state    │
  │  Projector   ─ perspective   │   │  ThreeMediaView ─ gesture surface │
  │  shaded_triangles / wireframe│   │  MeshPainter ─ Sketchbook painter │
  │  projected_points            │   │  ControlsBar ─ transport + scrub  │
  └──────────────────────────────┘   └───────────────────────────────────┘
        pure geometry + media            widgets, gestures, theming
```

`three-vieww` produces numbers; the viewer turns numbers into drawing. The
split is deliberate: the projection math is testable without a UI framework,
and a future GPU backend could consume the same projected triangles without
touching the widget layer.

## How the viewer is put together

- **`PlaybackState`** (element state on the root screen) owns the
  `three_vieww::ThreeView` media controller and the `OrbitCamera`. Its
  `ElementState::tick(now)` advances the media clock by the wall delta while
  playing — capped at 250 ms per frame so a stalled loop cannot skip the
  capture — and `is_animating()` answers `playing`, so a paused viewer costs
  the loop nothing. Control and gesture handlers write through a
  `StateHandle` (the channel `BuildContext::state_handle` exists for) and set
  a pending flag the next frame consumes.
- **`MeshPainter`** implements vieww's `Painter` trait: given the box it was
  laid out at, it projects the current frame through `three-vieww` and
  records shaded triangles (or wireframe edges, or point motes) into the
  `Sketchbook`, far-to-near. `should_repaint` compares everything the drawing
  depends on, so an unchanged frame is never re-projected.
- **`ThreeMediaView`** is the gesture surface: drag orbits, scroll and pinch
  zoom, wrapped in a `Semantics` container so a screen reader is told what
  the ink rectangle is.
- **`ControlsBar`** is the transport: play/pause, wireframe, motes, loop,
  reset view, and a scrub slider bound to `ThreeView::progress`.

A Vieww-side widget therefore follows the pattern the boundary promises:

```rust,ignore
let capture = vieww_integration::load_capture(Some("./sample.3"))?;
let widget = WidgetNode::new(ViewerScreen { capture });
```

## Layout of the checkouts

The viewer is the only workspace member with a Vieww dependency, and it is
**not a default member** — `cargo test` / `cargo build` at the root exercise
the whole `.3` stack with no Vieww present. The three path dependencies live
in the root `Cargo.toml`:

```toml
vieww                = { path = "../vieww-develop/crates/vieww" }
vieww-platform-winit = { path = "../vieww-develop/crates/vieww-platform-winit" }
vieww-test-harness   = { path = "../vieww-develop/crates/vieww-test-harness" }
```

They assume the two checkouts sit side by side. Moving or renaming the Vieww
checkout means editing those three lines, nowhere else. `rust-toolchain.toml`
pins the compiler the integration was developed against; a `rustup`-managed
cargo installs it automatically, a system cargo ignores it (but must still be
new enough for Vieww's dependency tree — its own checkout pins 1.98.1).

## Testing without a screen

The viewer's tests run through `vieww-test-harness`: a real element tree, a
real frame loop, and a clock the test owns. Playback follows `harness.tick`,
gestures are injected as pointer events, and pixels come out of the CPU
rasterizer. The suite covers the media clock (advance, loop, pause, resume,
scrub), orbit/scroll gestures, wireframe switching, and pixel-level proof
that the capture lights up the media area and that orbiting moves pixels.

Two oracles are used on purpose:

- **State reads** through the element tree — the same channel the control
  handlers use — for behaviour.
- **`vieww::debug_tree` and rendered pixels** for things only the tree or
  the rasterizer can prove. (Note: the *element* tree's `debug_tree` prints
  widget names and build counts only; text data appears in the widget-level
  `vieww::debug_tree`.)

PNG previews can be rendered on demand:

```bash
cargo test -p vieww-integration --test previews -- --ignored --nocapture
```

## What is deliberately not here

- **Textures.** `.3` carries no image data in version 0, so the viewer draws
  flat-shaded geometry. The format has room to grow; the painter's palette
  already comes from the theme so a textured path can slot in beside it.
- **A GPU path.** Everything draws through the CPU rasterizer on every
  backend Vieww has today. `three_vieww::ThreeRenderer` remains the boundary
  a GPU backend would implement, and `shaded_triangles` is already producing
  the depth-sorted geometry such a backend would rasterize.
- **The social feed.** `three-app`'s state machine is exercised by the
  viewer's `open_asset` path; feed screens belong to the app, not to this
  repository's foundation.
