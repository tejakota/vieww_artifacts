# `.3` format — version 0

## Goal

`.3` is a compact, streamable container for short temporal 3D captures. It is
closer to a 3D-native animated media format than to a general-purpose authoring
format.

## Binary layout

```text
HEADER
  magic[4]       = "DOT3"
  version[u16]
  flags[u16]
  payload_len[u64]

PAYLOAD
  scene metadata
  camera samples
  frames
  representation data
  optional audio/extension chunks
```

The reference implementation currently uses a simple length-delimited binary
payload. The format is deliberately versioned from the beginning so a future
implementation can introduce compression, chunk streaming and random access.

## Coordinate system

The core coordinate system is right-handed:

- +X = right
- +Y = up
- +Z = forward

All distances are expressed in meters. Timestamps are expressed in nanoseconds
from capture start.

## Representation

A frame may contain a mesh, a point cloud, or both. Production implementations
may add Gaussian or neural representations as extension chunks. The runtime
must never assume that a particular representation exists.

## Capture quality

The file records capture provenance and reconstruction quality. A viewer can
therefore distinguish between a LiDAR-assisted scan, a depth-camera capture,
and an RGB-only reconstruction.

## Compatibility rule

Readers must reject unknown major versions and may ignore unknown extension
chunks from a supported major version. Writers must never silently downgrade a
capture and claim that information was preserved when it was not.
