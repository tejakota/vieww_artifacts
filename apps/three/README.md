# 3 — temporal 3D capture and `.3`

`3` is a Rust-first **social application for living 3D moments**. Its native
content is a moving spatial capture created from a device camera and stored as `.3`.

The project is intentionally split into two products:

- **`.3`** — a binary container for a short, time-varying 3D capture.
- **`3`** — the social product: create, publish, discover, interact with and remix `.3` moments.

The repository does **not** pretend that a single RGB camera can recover hidden
surfaces perfectly. Capture and reconstruction are explicit capabilities with
quality metadata, and platform-specific implementations can be added without
changing the file format.

## Workspace

```text
crates/three-core            shared 3D types, invariants, bounds
crates/three-format          `.3` encoder/decoder
crates/three-capture         camera/IMU/depth abstraction
crates/three-reconstruction  temporal reconstruction pipeline interfaces
crates/three-runtime         playback, validation, capture orchestration, demo capture
crates/three-vieww           media-state + presentation-prep layer for Vieww (no Vieww dependency)
crates/three-social          executable social domain + backend contract
crates/three-app             product navigation/state/orchestration
crates/three-cli             developer CLI
examples/capture-demo        deterministic end-to-end example
examples/vieww-integration   focused Vieww `.3` media viewer
examples/social-app          the Vieww `3` social application
```

## Current status

The repository now contains both the working `.3` media foundation and the
**social product core**: profiles, asset publishing, feeds, likes, comments,
follows, search, notifications, privacy, deletion, remixes, product navigation
and a Vieww social application shell with interactive `.3` content in the feed.
Real Android/iOS camera implementations and production Internet-service
adapters remain isolated behind explicit traits.

## Design principle

The `.3` format describes *what was captured*, not *how one particular device
happened to reconstruct it*. A `.3` may contain meshes, points, depth samples,
textures, motion tracks and camera metadata. New representations can therefore
be added without invalidating the social format.

## Build

The `.3` stack builds standalone — no UI framework, no window:

```bash
cargo test                # the foundation (default members, no Vieww needed)
cargo run -p three-cli -- create-demo ./sample.3
cargo run -p three-cli -- inspect ./sample.3
cargo run -p capture-demo
```

The reference viewer is the one workspace member that depends on **Vieww**,
by path. It expects a Vieww checkout next to this one:

```text
parent/
  three/            this workspace
  vieww-develop/    a checkout of Vieww
```

With that in place (and a Rust toolchain new enough for Vieww's dependency
tree — see `rust-toolchain.toml`):

```bash
cargo test --workspace                    # everything, viewer tests included (headless)
cargo run -p vieww-integration            # the viewer, playing the demo capture
cargo run -p vieww-integration -- ./sample.3
```

See `docs/VIEWW-INTEGRATION.md` for how the integration is layered and where
to point the path if your Vieww checkout lives elsewhere.

## Run the social app

With Vieww checked out next to this workspace as `../vieww-develop`:

```bash
cargo run -p three-social-app
```

See `docs/SOCIAL-APP.md` for the product architecture and `TRACKER.md` for the
implementation/verification status.

## Vieww

`three-vieww` contains no dependency on an external UI framework. It owns the
media state (playback clock, frame selection) and the presentation preparation
(perspective projection, flat shading, depth sorting) that a renderer needs.
`examples/vieww-integration` is the Vieww side of the boundary: the concrete
widget, its element state, and the painter that records frames into a Vieww
`Sketchbook`. This keeps the capture/media stack independent from Vieww while
`3` uses Vieww as its native UI/runtime layer.
