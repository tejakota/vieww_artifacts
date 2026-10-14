# Three implementation tracker

## 2026-09-12 — social product pass

Status: implemented; runtime verification requires a Rust toolchain.

### Completed

- Replaced the placeholder `three-social` command enum with an executable,
  transport-independent social domain and `SocialBackend` contract.
- Added profiles, `.3` asset upload/download, posts, cursor feeds, likes,
  comments, follows, search, notifications, privacy, deletion and remixes.
- Added `MemorySocialBackend` as a deterministic offline/reference backend.
- Rebuilt `three-app` around product routes, tabs, feed state, composer state,
  search, profile feeds and media opening.
- Made publishing an end-to-end transaction: encode/validate `.3` -> upload
  asset -> publish post -> refresh feed.
- Added a Vieww `three-social-app` reference application with Home, Explore,
  Create, Activity and Profile surfaces.
- Embedded the existing interactive `.3` Vieww viewer into the Home feed so
  the native social post is spatial media, not a disconnected viewer demo.
- Preserved `three-viewer` as a focused media diagnostic.
- Added social architecture/product documentation and verification script.

### Verification state

The execution container used for this pass does not provide `cargo`, `rustc`
or `rustfmt`, so no build/test result is claimed here. Run `./verify.sh` in a
Rust 1.89+ environment; the script is the canonical gate.

### Production-service work that remains deployment-specific

- authenticated remote `SocialBackend` adapter;
- durable user/post relational store;
- object storage/CDN and progressive `.3` streaming;
- moderation/abuse systems and server-side privacy authorization;
- push notification transport;
- real Android/iOS capture backends and reconstruction acceleration.

These are service/device adapters, not missing social product semantics in the
client architecture.

## 2026-10-04 — the depth-video redesign

Status: implemented and verified (host tests, clippy -D warnings, fmt,
Android cross-compile, APK build, real-video import round trip).

### The redesign

The product is **actual video converted to depth**, not reconstructed or
cartoonized geometry. Replaced wholesale:

- `three-core`: `Capture3D` (meshes/points/cameras) → `DepthVideo`
  (per-frame RGBA + u16 mm depth + one `CameraIntrinsics` + a
  `DepthSourceKind` that separates measured from generated from absent).
- `three-format`: v0 mesh container → v1 (JPEG color q82 + zstd u16
  depth, deterministic, strictly front-to-back) with a `LazyCapture`
  per-frame reader — a 3 s capture is ~2 MB compressed vs ~130 MB raw,
  and a phone feed cannot hold the raw.
- `three-reconstruction`: deleted. There is nothing to reconstruct; the
  device measured the depth at capture time.
- `three-vieww`: triangle projection + orbit camera → the depth-warp
  splat renderer + `ViewCamera` (pan=orbit, pinch=dolly-in-depth,
  hold=recentre spring). At the neutral pose the warp is the identity —
  feed autoplay costs a copy, not a re-projection.
- The viewer: flat-shaded triangles through a Painter → warped RGBA into
  the framework's own `Image` widget (its rasterizer, its BoxFit).

### Platform capture

- Android `camera2` through a Java shim compiled and dexed in-build (the
  vavlt pattern — no Gradle): `YUV_420_888` color + `DEPTH16` where the
  device has it (confidence-filtered), `LENS_INTRINSIC_CALIBRATION` when
  published, CAMERA permission in the manifest, honest `no-depth`
  degradation otherwise.
- iOS AVFoundation backend (LiDAR depth where present), compile-verified;
  first device run pending.
- `Recording` — the per-frame-tick capture driver a UI needs; completion
  is the frame count, never the source's silence.
- The Create tab records real moments on Android; the same publish
  pipeline runs on desktop through the sample/import path.

### Verification

- 60+ tests across the workspace, including hand-computed warp-position
  assertions (independent of the renderer's own math).
- `three import` round-trips real MP4s (ffmpeg on PATH) — 72 frames,
  3 s, verified through inspect and five rendered camera poses.
- The APK builds: 3.9 MB, signed, `uses-permission CAMERA`, minSdk 24.
