# Architecture

```text
Device camera / depth / IMU
          |
          v
    three-capture
          |
          v
 three-reconstruction
          |
          v
      three-core
          |
          +------> three-format ------> .3
          |
          v
     three-runtime
          |
          +------> three-vieww ------> Vieww / 3 UI
```

The important boundary is between **capture**, **reconstruction**, and
**presentation**. A phone camera implementation should never know how a social
feed is rendered. Likewise, Vieww should not need to understand camera drivers.
