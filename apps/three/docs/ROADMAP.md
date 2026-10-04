# Roadmap

## Milestone 1 — the depth-video foundation

- [x] Core types: `DepthVideo`, per-frame color+depth, intrinsics, source kinds.
- [x] `.3` v1: JPEG + zstd, deterministic encoding, lazy per-frame decoding.
- [x] Capture traits + the deterministic host source.
- [x] The depth-warp renderer and the viewer camera (pan/pinch/hold).
- [x] The reference Vieww viewer with the gesture set.

## Milestone 2 — the social product core

- [x] User/profile domain, feeds, likes, comments, follows, search.
- [x] `.3` upload/download contract; publish/delete; visibility.
- [x] Remix ancestry, notifications, navigation.
- [x] The Vieww social application shell with interactive `.3` feed content.
- [x] The offline reference backend.

## Milestone 3 — real devices

- [x] Android `camera2` backend: YUV color + `DEPTH16` where present, the
      Java shim compiled and dexed in-build, CAMERA permission, honest
      `no-depth` degradation. (Shipped in this iteration; device testing is
      the remaining gap.)
- [x] iOS AVFoundation backend: LiDAR depth where present, compile-verified
      in CI. (First device run pending — see below.)
- [ ] On-device tuning: capture resolution vs warp cost, depth-confidence
      threshold, the sticky-depth cadence on real LiDAR hardware.
- [ ] Camera intrinsics validation against the warp's expectations.

## Milestone 4 — production service adapters

- [ ] Authenticated remote `SocialBackend` transport.
- [ ] Durable repository; object storage and CDN delivery.
- [ ] Progressive/chunked `.3` streaming (the format's v2 offset table).
- [ ] Server-side feed ranking; push notifications.
- [ ] Moderation, reporting, blocking; export and deletion workflows.

## Milestone 5 — depth-video craft

- [ ] Warp quality: temporal depth smoothing to kill per-frame flicker;
      hole-filling improvements (inpainting vs the directional sweep).
- [ ] Capture UX: framing guide, depth-confidence preview, retake flow.
- [ ] Quality tiers for low/mid/high-end devices (warp resolution scale).
- [ ] Spatial reactions; remix on depth (re-cut with a different camera
      path through the same moment).
