# Architecture

```text
   camera2 (JNI shim)  /  AVFoundation  /  SyntheticDepthCamera
                     |
                     v
               three-capture          DepthVideoSource (poll contract)
                     |
                     v
               three-runtime          players, Recording/CaptureSession, import
                     |
                     v
                three-core            DepthVideo, intrinsics, invariants
                     |
                     +──> three-format ──> .3 v1 (JPEG + zstd, lazy reader)
                     |
                     v
                three-vieww           warp renderer + ViewCamera (pan/pinch/hold)
                     |
                     v
             Vieww (vieww-integration, three-social-app)
```

## The one rule

The media stack (`core → format → capture → runtime → vieww`) has **no
UI-framework dependency**; the Vieww crates depend on it, never the reverse.
`cargo test` builds and passes without a Vieww checkout — the two Vieww
examples are non-default workspace members.

## What each layer owns

- **three-core** — plain data: the frame model (RGBA + `u16` mm, 0 =
  unknown), the pinhole model with `unproject`/`project`, and the
  invariants `validate` enforces. No clocks, no platform types.
- **three-format** — the container. Encode is deterministic; decode is
  strict; `LazyCapture` indexes frames without decompressing them.
- **three-capture** — the `DepthVideoSource` trait and its implementations.
  Depth conversion happens here, at capture time, on the recording device.
- **three-runtime** — playback as a pure function of accumulated time
  (`DepthVideoPlayer`), capture orchestration (`CaptureSession` sync,
  `Recording` per-tick), and the ffmpeg import pipeline.
- **three-vieww** — `warp_frame`: the z-buffered, hole-filled forward
  splat that re-photographs a frame from the viewer's virtual camera;
  `ViewCamera`: the clamped orbit/dolly state and the recenter spring.
- **three-social / three-app** — the product domain and navigation; the
  app layer hands the social app bytes (never pixels).

## The render path, end to end

1. A feed card opens a post's bytes as a `LazyCapture` (shared `Rc`).
2. The card's `DepthView` decodes **exactly the current frame**, caches it.
3. Neutral camera → the warp is the identity → the pixels go straight to
   an `Image` widget (the framework's own rasterizer and `BoxFit`).
4. A gesture moves the `ViewCamera` → the next tick's render runs the
   splat warp → new pixels, same widget.
5. Hold → the spring animates the camera home → the render converges back
   to the identity, and the loop goes back to sleep.
