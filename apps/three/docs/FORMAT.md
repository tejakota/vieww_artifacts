# The `.3` format — version 1

A `.3` v1 is **actual video plus per-frame depth**, converted at capture
time by the device that recorded it.

## What it stores

- **Color**: JPEG, quality 82 (the social-upload bracket, ~4:2:0 chroma
  subsampling below quality 90 — the standard camera-pipeline trade).
- **Depth**: raw `u16` millimeters per pixel, zstd level 6, lossless.
  **0 means unknown** — the value both Android `DEPTH16` and iOS depth
  APIs use for "no measurement here", kept verbatim so a warp skips rather
  than guesses.
- **One camera model** for the whole shot (`fx, fy, cx, cy`): a moment is a
  single continuous take. The warp re-projects through exactly these
  numbers, so the neutral pose is the footage as filmed.
- **Source kind**: which pipeline measured the depth — `lidar`,
  `camera2-depth16`, `truedepth`, `no-depth` (a device without a depth
  pipeline; the video plays flat, honestly), or `test-depth` (the
  development importer's generated depth, never presented as measured).

## Layout

```text
magic "DOT3"            4 bytes
version                 u16 LE      (= 1)
flags                   u16 LE      (= 0)
payload_len             u64 LE
── payload ──────────────────────────────────────────────────────────
title_len               u32 LE
title                   UTF-8, exactly title_len bytes
duration_ns             u64 LE
fps                     u32 LE
width, height           u32 LE × 2
fx, fy, cx, cy          f32 LE × 4
source                  u8
frame_count             u32 LE
── per frame, in timestamp order ────────────────────────────────────
timestamp_ns            u64 LE
color_len               u32 LE
color                   JPEG bytes, exactly color_len
depth_len               u32 LE
depth                   zstd bytes; decompresses to width×height u16 LE mm
```

Strictly front-to-back: no offsets, no index. A reader that meets a version
it does not know refuses the file rather than guessing.

## Encoding is deterministic

Fixed JPEG quality, fixed zstd level, no timestamps-of-encoding, no paths,
no RNG: two encodes of one capture produce identical bytes, which the
round-trip and stability tests assert on.

## Reading: eager and lazy

`decode` materializes the whole capture — right for tests, the CLI, and the
importer. A viewer on a phone cannot afford that: a 3-second, 640×480,
24 fps capture is ~130 MB decompressed against ~2 MB compressed.
`LazyCapture::open` indexes the frame records in one pass that decompresses
nothing; `frame(n)` decodes exactly one frame on demand. The reference
viewer keeps exactly one decoded frame alive at a time.

## Roadmap (version 2 candidates)

- An offset table after the frame records, for true seeking.
- Chunked/streamable delivery for progressive feed loading.
- Per-frame exposure/gain metadata for relighting.
