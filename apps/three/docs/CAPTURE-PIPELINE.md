# Device capture pipeline

The pipeline is **video, converted to depth at capture time** — on the
device that records it, never on a server, never at view time:

```text
RGB camera ─────────────┐
Depth (where present) ──┼──> synchronized frames  ──> .3 v1  ──> viewer
                        │      (color + depth + intrinsics)
Permission gate ────────┘
```

## Android (`camera2`)

`three-capture`'s `Camera2Source` drives a Java shim
(`java/ThreeCameraShim.java`, compiled and dexed by `build.rs`, loaded
through `InMemoryDexClassLoader` — the vavlt picker pattern, no Gradle, no
AGP, no Kotlin):

- **Color**: `YUV_420_888`, repacked contiguous by the shim, converted to
  RGBA in Rust (BT.601).
- **Depth**: `DEPTH16` where the device has it — 13-bit millimeters plus a
  3-bit confidence field. Samples with confidence < 3 become unknown (0).
  Most phones have no `DEPTH16` output; they report that honestly through
  `CaptureCapabilities`, and their captures are real video that plays flat
  (`no-depth`).
- **Intrinsics**: `LENS_INTRINSIC_CALIBRATION` when the device publishes
  it, scaled to the capture resolution; otherwise the centered ~60°
  approximation.
- **Contract**: Rust polls; Java never calls back. One static JNI call per
  frame, newest-wins queues on a serial handler thread.

## iOS (AVFoundation)

`AvFoundationSource` runs an `AVCaptureSession` on the back camera with a
`AVCaptureVideoDataOutput` (BGRA) and — on LiDAR hardware — an
`AVCaptureDepthDataOutput` (true depth in meters, converted to filtered
millimeters). The same poll contract; the same newest-wins buffers; the
same honest `no-depth` on devices without a depth sensor. Authorization is
polled: denied is an error, not-yet-determined is a wait.

## The host source

`SyntheticDepthCamera` is deterministic — a moving color band over a
gradient with a breathing depth mound — for CI, demos, and format tests.
Its depth is labeled `test-depth` by construction.

## Recording vs session

`CaptureSession` runs a whole capture synchronously (tests, importer).
`Recording` is the same state machine cut into per-frame-tick steps for a
UI: `start` on the button press, `poll` once per frame tick until the
frame-count target is met, `finish` to assemble. Completion is the frame
count, never the source's silence — a camera's "nothing ready this tick"
mid-recording must not end the capture.

## The development importer

`three_runtime::import_mp4` shells out to **ffmpeg** (must be on `PATH`) to
turn any real video into a `.3` on a dev machine, pairing extracted frames
with generated depth (a dome-plus-luminance relief field). It exists so
the format, the warp, and the viewer can be exercised with actual footage
before any phone is involved. The capture's source is `test-depth`, and
nothing downstream can mistake it for a measurement.
