# Vieww integration example

The reference Vieww viewer for `.3` captures — a real window, a real widget
tree, a real painter. This directory used to hold only pseudocode-level
wiring "rather than inventing APIs that are not present in the uploaded
Vieww source"; the Vieww source is now present, and the wiring is real.

## Run it

From the workspace root, with a Vieww checkout alongside (see the root
`Cargo.toml` and `docs/VIEWW-INTEGRATION.md` for the layout):

```bash
cargo run -p vieww-integration                       # the built-in demo capture
cargo run -p vieww-integration -- ./sample.3         # a .3 file
cargo run -p three-cli -- create-demo ./sample.3     # make one first
```

Interacting with it:

- **drag** on the capture — orbit the camera around the subject
- **scroll / pinch** — zoom
- **Pause / Play, Wire, Motes, Loop, Reset view** — the transport row
- **the slider** — scrub the timeline; playback continues from wherever you
  scrub to

## Read it

- `src/state.rs` — `PlaybackState`, the `ElementState` that owns the media
  clock and camera. Start here: it is the smallest complete example of
  vieww's state-survives-rebuild model, including the frame-clock `tick`
  that drives playback and the `StateHandle` write path handlers use.
- `src/painter.rs` — `MeshPainter`, a `vieww::Painter` that records a `.3`
  frame into a `Sketchbook`. The whole 3D-to-2D pipeline is a call into
  `three-vieww`; the painter is genuinely just recording.
- `src/widgets.rs` — `ViewerScreen` (root + state owner), `ThreeMediaView`
  (gestures + `Painting`), `ControlsBar` (transport + scrub).
- `src/main.rs` — the `vieww_platform_winit::App` entry, themed dark like
  every media viewer.
- `tests/` — the headless suite (see `docs/VIEWW-INTEGRATION.md`).

## Lifting it into your own app

The crate is a reference, not a runtime dependency: copy `state.rs`,
`painter.rs` and `widgets.rs` into a Vieww application, depend on
`three-vieww` + `three-runtime`, and construct the screen over any
`Rc<Capture3D>`. Nothing in the crate reaches back into the example's own
wiring — the capture comes in, widgets come out.

## Generating previews

```bash
cargo test -p vieww-integration --test previews -- --ignored --nocapture
```

writes `target/previews/viewer-*.png` — the viewer rendered through the
same headless path the tests use, which is also a quick way to check a
rendering change without opening a window.
