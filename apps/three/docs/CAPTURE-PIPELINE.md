# Device capture pipeline

The intended phone pipeline is:

```text
RGB camera ───────────────┐
Depth/LiDAR (optional) ───┼──> synchronized observations
IMU (optional) ───────────┤
AR/SLAM pose (optional) ──┘
                           |
                           v
                 temporal reconstruction
                           |
                           v
                    .3 representation
                           |
                           v
                    Vieww / 3 player
```

## First real prototype

Start with a constrained capture mode: the user moves the phone around a
mostly static subject for roughly three seconds. This supplies multiple views
and is substantially more tractable than promising complete 3D reconstruction
from one stationary RGB frame.

## Moving subjects

Once the static-object pipeline works, add temporal correspondence and dynamic
reconstruction. A moving person is fundamentally harder because the camera and
subject can both change between observations.

## Quality tiers

The capture metadata should expose whether a result was produced with:

- RGB only;
- RGB + depth;
- RGB + LiDAR;
- multiple cameras.

The viewer should gracefully degrade instead of refusing a lower-capability
capture.
