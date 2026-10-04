# 3 — depth video, socially

`3` is a Rust-first **social application for depth-video moments**. Its
native content is an **actual video, recorded with the device's camera, that
carries depth with every frame**: viewers pan to orbit the subject, pinch
to move genuinely closer *in depth*, and hold to return to the angle the
footage was filmed from.

The project is two things:

- **`.3`** — a binary container for a short video plus per-frame depth,
  converted at capture time by the recording device.
- **`3`** — the social product: record, publish, discover, and interact with
  `.3` moments.

The repository does **not** fabricate geometry: a `.3`'s depth is what the
recording hardware measured (Android `camera2` `DEPTH16`, iPhone LiDAR) —
or the capture says so, plainly, and the moment plays flat. The one
exception is the development importer, whose generated depth is labeled
`test-depth` everywhere a viewer can read, and never presented as measured.

## Workspace

```text
crates/three-core            depth-video types, camera intrinsics, invariants
crates/three-format          .3 v1: JPEG color + zstd depth, lazy per-frame decoding
crates/three-capture         capture traits + camera2 (JNI shim) + AVFoundation backends
crates/three-runtime         players, capture sessions, the ffmpeg import pipeline
crates/three-vieww           the depth-warp renderer + the viewer's virtual camera
crates/three-social          the social domain + the offline reference backend
crates/three-app             product navigation/state/orchestration
crates/three-cli             developer CLI (inspect / create-sample / import)
examples/vieww-integration   the reference Vieww viewer (pan/pinch/hold)
examples/social-app          the Vieww `3` social application
```

## The gestures are the product

On any `.3` moment — in the feed or the viewer:

- **Pan** — orbit the camera around the subject. This is a re-photograph of
  real geometry: near objects sweep past far ones exactly as walking around
  the scene would show.
- **Pinch** — dolly in and out *in depth*. Closer is genuinely closer: the
  foreground overtakes the background, the way an ordinary 2D zoom can never
  fake.
- **Hold** — the recenter spring, returning the view to the one viewpoint
  where every pixel is ground truth: where the footage was filmed from.

At the neutral pose the warp is the identity — autoplay in a feed costs a
frame copy, and the warp runs only while a finger is on the glass.

## Build

The `.3` stack builds standalone — no UI framework, no window:

```bash
cargo test                      # the foundation (default members, no Vieww)
cargo run -p three-cli -- create-sample ./sample.3
cargo run -p three-cli -- inspect ./sample.3
cargo run -p three-cli -- import ./clip.mp4 ./imported.3   # ffmpeg on PATH
```

The reference viewer and the social app need a Vieww checkout alongside
(see the workspace `Cargo.toml` for the path):

```bash
cargo test --workspace          # everything, viewer included (headless)
cargo run -p vieww-integration  # the viewer, playing the sample capture
cargo run -p vieww-integration -- ./imported.3
cargo run -p three-social-app   # the social app
```

Android (needs the NDK, a JDK, and an Android SDK — see `BUILD.md` in the
vavlt app for the environment this mirrors):

```bash
cargo apk build -p three-social-app --lib --release
```

## Design principle

A `.3` describes *what the camera measured*, not what a reconstruction
guessed. Depth quality is metadata (`lidar`, `camera2-depth16`,
`truedepth`, `no-depth`, `test-depth`), and every consumer can decide how
much to trust it — the format never decides for them.
