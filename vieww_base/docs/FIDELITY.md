# Output fidelity

What vieww promises about the pixels it draws, and how each promise is
checked. Decided 2026-10-16: **byte-identical by default; tolerances only per
backend, and only written down here.**

## 1. The CPU rasterizer is the reference, and it does not drift

`vieww_paint::native::NativeRenderer` is the renderer every fixture, golden and
screenshot in this repository was drawn with. Its output is **byte-identical
across changes** unless a change says otherwise.

- A performance change (SIMD, threads, a cheaper blur, a new cache) must leave
  every byte where it was. "Visually the same" is not the bar; the same bytes
  are.
- A change that is *meant* to move pixels (a fixed antialiasing bug, a new
  default) says so in its commit message with a `Pixels-Changed:` trailer:
  what moved, and why that is better.
- **Gate:** `ci/check/raster-identity.sh [BASE]` renders the fixture gallery
  (`examples/fixtures`, every reference screen and animation) at `BASE` and at
  the working tree and compares the files byte for byte. It fails on any
  difference; `--allow-changed` turns that into a report, for a commit that
  carries the trailer. The 2026-10-16 blur speed-up (17.5 → 15.5 ms) was
  checked the same way by hand, every output identical.

Two identities hold with **no** exception, because a tolerance would hide
exactly the fault they exist for:

| Identity | Test |
|---|---|
| A retained, damaged repaint equals a full repaint, byte for byte | `crates/vieww-paint/tests/retained_equivalence.rs` |
| A frame drawn in tiles or threads equals the same frame drawn in one pass | to be added with multi-threaded raster (roadmap 2.1), before it is enabled |

## 2. Every other backend has its own tolerance against the reference

A second rasterizer is checked against the CPU one, not against stored
images, and each declares how close it must be. A tolerance belongs to one
backend and one kind of content; there is no global one.

| Backend | Content | Bound against the CPU reference | Test |
|---|---|---|---|
| Vulkan `SceneRenderer` | filled and stroked shapes | **interior** pixels within 2/255 per channel, none over; edge pixels free (analytic scanline coverage against triangulation plus hardware sampling disagree by design only there) | `crates/vieww-hal/tests/vulkan_scene.rs` |
| Vulkan `SceneRenderer` | text | **every** pixel within 1/255, rims included (one glyph rasterizer; the atlas stores coverage as `u8`) | `crates/vieww-hal/tests/vulkan_text.rs` |
| Vulkan compositor | layers, group opacity, blur, backdrop blur, colour matrix, shaped clips, shadows, transparent clear, winding, dashed strokes | **every** pixel within 2/255, none over | `crates/vieww-hal/tests/vulkan_compositor.rs` |
| Metal | — | not declared; the port must add its rows here before it draws a user's frame | `metal_smoke.rs` is a smoke test only |
| D3D12 | — | not declared; same rule | `d3d12_smoke.rs` is a smoke test only |
| WebGPU | — | not declared; same rule | — |

Where they run: lavapipe in ordinary CI, real GPUs in `ci/hardware/linux.sh`
and `ci/hardware/windows.ps1` (`cargo test -p vieww-hal --features vulkan --
--ignored`). Phones are not covered yet; Device Farm runs the device suite,
not these parity tests.

## 3. Changing a bound

A bound is loosened only by editing its row in the same commit as the test,
with the reason in the commit message — never by a constant changing alone.
Tightening needs no ceremony.

## 4. Why this matters more now

With `App::auto_rasterizer` the device, not the developer, picks the
rasterizer: the same build draws on the CPU on one phone and on the GPU on the
next. The bounds in section 2 are therefore also the bound on how different
two users' screens may look for the same build.
