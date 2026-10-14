# ViewW pending work

Everything known to be missing, in one place.

## How this file relates to `TRACKER.md`

They answer opposite questions and nothing belongs on both.

- [`TRACKER.md`](./TRACKER.md) is what has been **done**, and how it was
  verified. It is a record, written after the fact, and its value is that its
  claims are checkable.
- This file is what has **not**. Its value is different: it is the thing that
  stops a gap from being rediscovered, re-scoped and re-argued by the next
  person, and it is the answer to "what would I work on".

When an item here is finished it moves to `TRACKER.md` with the evidence
attached, and its entry here is deleted rather than ticked. A file of
struck-through lines is an archive; this is meant to be a worklist.

Every item says four things: **what** it is, **why** it is not done, **what**
it blocks, and **where to start** — a real command or a real file, not a
gesture. An item that cannot say the fourth is not understood well enough to be
written down yet, and there are two of those, marked as such.

## Read this before believing anything is easy here

Three facts sit behind most of the list and are not repeated in every entry:

1. **No code in this workspace has ever run on real graphics hardware.** The
   GPU path is verified against software Vulkan implementations — `lavapipe`
   on the original machine, Playwright's SwiftShader ICD on the current one.
   Every GPU statement anywhere in this repository is a correctness claim and
   none of them is a performance claim.
2. **Every line of the framework has been compiled only on Linux, Android and
   iOS.** The "Build Mobile Apps" workflow has built the four apps — which
   compile the vieww crates they depend on — green on GitHub runners since
   2026-10-05 (23 runs, latest success 2026-10-08). macOS and Windows are real,
   reviewed, unit-tested code that no compiler for that target has ever seen;
   the `framework-checks` workflow added beside it runs
   `ci/check/platform-check.sh macos` and `windows` on every push, and
   `ci/check/platform-check.sh` remains one command each for a person with the
   machine.
3. **A frame has never reached a physical screen on any platform but Linux.**
   Compiling for Android and iOS is not running there: the device suites
   (`ci/mobile/device-suite.sh`, `ci/mobile/a11y-android.sh`) need hardware
   plugged in, and no run of them is recorded.

---

# 1. Blocked on hardware or a toolchain

Not ordinary work. Each of these needs a machine this one is not, and none of
them can be closed by reading the code more carefully.

## 1.1 A GPU frame on real graphics hardware

**2026-10-14 — first run on real GPUs** (`ci/hardware/close-remaining.sh`, a
Kaby Lake laptop under GNOME/Wayland, rustc 1.98.1):

| GPU | Driver | `vieww-hal --features vulkan -- --ignored` | `vieww-3d` GPU | GPU frame in a real window |
|---|---|---|---|---|
| Intel HD Graphics 620 | Mesa 26.0.8 | 44/44 (compositor 17/17 incl. backdrop blur) | 7/7 | `wait_loop` 5/5, `gpu_frame` ok |
| NVIDIA GeForce 920MX | 580.178.04 (PRIME) | 44/44 | — | — |

So the GPU paths are correct on two real drivers, not only on lavapipe and
SwiftShader — including the backdrop-blur edge mode of §2.2, held to the CPU
oracle on Mesa's real Intel driver. The desktop suite on the same machine:
21/21, 1.76 ms median frame (347 fps).

**Still open here:** GPU *timings*. `examples/fixtures` renders on the CPU
(185.3 ms for the gallery on this laptop; the same two fixtures over budget as
§4.1, plus the `33-sheet` motion at 14.2 ms mean); it has no switch to plan its
scenes onto the GPU, which is what "measure with `Placement::Gpu`" needs.

**What.** Run `cargo test -p vieww-hal --features vulkan -- --ignored` on a
machine with a discrete or integrated GPU, and measure `examples/fixtures` with
`Placement::Gpu` where `ScenePlan::is_complete` allows it. The windowed GPU path
([`App::prefer_gpu`]) is now real and exercised on a real (Xvfb) window under
SwiftShader — but a software rasterizer presenting to a virtual display is
still not hardware.

**Why not.** This machine has no GPU. SwiftShader stands in, and it is a
software rasterizer wearing a Vulkan interface — it can prove that the shader
maths, the vertex layout, the atlas coordinates and the blend state are right,
and it can prove nothing whatever about speed.

**Blocks.** Every performance claim about the GPU path. Also the only honest
answer to "is the GPU actually faster than your CPU rasterizer here", which is
currently unknown in both directions: the CPU path is unusually good, and the
GPU path is one draw call for a whole frame, and nobody has measured them
against each other on hardware.

**Start.** The tests already exist and already run; a GPU-backed machine needs
to do nothing but run them, then run the gallery. Record the numbers in
`TRACKER.md` beside the CPU ones, and delete the "no number was measured on
real hardware" caveat from this file, `TRACKER.md`, `README.md`,
`docs/guide/README.md` and `crates/vieww-hal/src/lib.rs` — it is deliberately
stated in all five.

## 1.2 macOS — the Metal backend: written, type-checked, never run

**What.** `crates/vieww-hal/src/metal.rs` now has `render_clear_to_pixels`
and `render_mesh_to_pixels` (WGSL → MSL through `vieww-shaders`, which now
binds real Metal slots instead of naga's `[[user(fake0)]]` placeholders), and
`tests/metal_smoke.rs` makes the Vulkan smoke suites' claims against it. It
type-checks and passes `clippy -D warnings` for `aarch64-apple-darwin` and
`aarch64-apple-ios` from Linux (`ci/check/cross-check.sh`). `SceneRenderer`
is not ported.

**Why not closed.** Type-checked is not run. There is no software Metal, so
the first execution is the macOS leg of `framework-checks.yml`, which runs the
suite with `VIEWW_REQUIRE_HARDWARE=gpu` (a missing device fails, not skips).
The MSL itself has been compiled by nothing yet — `newLibraryWithSource` on
that runner is its first compiler.

**Blocks.** The GPU scene path on macOS and iOS.

**Start.** Read the macOS leg's result. Then port `vulkan/scene.rs`'s
`SceneRenderer` onto `MetalDevice::pipeline`/`render_target`/`read_texture`.

## 1.3 Windows — the D3D12 backend: written, type-checked, never run

**What.** `crates/vieww-hal/src/d3d12.rs` now has the same pair: naga HLSL
compiled with `d3dcompiler_47` (SM 5.1), root constants for the uniform, a
fenced readback, and a WARP fallback behind `VIEWW_D3D12_ALLOW_WARP=1`.
`tests/d3d12_smoke.rs` mirrors the Metal suite. Type-checks and lints clean
for `x86_64-pc-windows-msvc`; every built-in shader's HLSL compiles with DXC
on Linux. `SceneRenderer` is not ported.

**Why not closed.** Never executed. The Windows leg runs it on WARP, which is
correctness evidence (a conformant D3D12 driver) and never a performance
number.

**Blocks.** Windows GPU rendering. Windows also has a *separate* unsolved
problem — see 3.3.

**Start.** Read the Windows leg's result; then port `SceneRenderer`.

## 1.4 iOS

**What.** `vieww-platform-winit`'s iOS support is real code — and the four
apps' iOS IPAs now build green on GitHub's macOS runners (see fact 2 above) —
but no frame has ever reached an iPhone, simulated or real: the runners build
and sign, they do not run.

**Why not.** The iOS SDK exists only on macOS, and the runners do not boot
simulators.

**Blocks.** The mobile half of "cross-platform".

**Start.** `ci/check/platform-check.sh ios` for the compile, then
`ci/mobile/ios-app.sh --sim` — which already exists and already knows how to build a
`.app`, boot a simulator and install onto it.

**2026-10-14, from the simulator job in CI.** The runners *do* boot
simulators: the `ios-simulator` job builds, installs and launches the demo.
It then drew a black screen. With `VIEWW_TRACE_FRAMES` on, the startup trace
ended at `window created (1260x2361); opening the GPU` — every window opens
through `vieww-hal`'s Vulkan backend, iOS has no Vulkan, and the bundle had no
MoltenVK. The failure was silent because `run_app` never returns on iOS and
`exit()` is ignored there; `fail_to_open` now prints the reason and ends the
process. Next: MoltenVK in the bundle (`VIEWW_MOLTENVK`, fetched by the CI
job from Khronos' release), then whatever the first real frame shows.
Khronos' `MoltenVK-ios.tar` turned out to carry only the device slice
(`dynamic/MoltenVK.xcframework/ios-arm64`); the job now takes the simulator
slice from `MoltenVK-all.tar`, linking the static one into a dylib if no
dynamic one ships.

**2026-10-14, later: the first vieww frame on an iPhone (simulator).** With the
dynamic simulator slice bundled and Metal argument buffers off (the
simulator's paravirtualised GPU has none), the GPU opened in 36 ms, the tree
built, and the in-app device suite reported **12 of 12 checks green** — safe
area, density 3x, keyboard inset, the modal drawer, 600 frames at a 6.8 ms
median. Two defects showed on the way: the window took the desktop preview
size (420x880 on a 393x852 screen), which shifted the accessibility frames by
(27, 28) so Maestro tapped beside "Tap me"; and the Metal layer kept
`contentsScale` 1.0 on a 3x screen, rendering soft. Both fixed in
`vieww-platform-winit` (`surface` ignores the requested size on phones;
`ios::match_layer_scale` before each swapchain rebuild).

## 1.5 Android

**2026-10-14, from the `android-emulator` CI job** (x86_64, API 34, Pixel 6
profile): the APK builds, signs, installs and launches; Maestro drives the demo
end to end through the AccessKit tree (taps, the shared selection, the modal
drawer open and dismissed); the in-app device suite reports green apart from
the two frame-budget checks, which on an emulator measure SwiftShader and are
reported as warnings there only. The drawer half found a real bug — the panel
covered its own scrim, so a drawer could not be dismissed by touch (fixed in
`Drawer::build`, pinned by `vieww-test-harness/tests/drawer_dismiss.rs`).
What only a phone can still answer: real-GPU frame times and TalkBack by ear.

**2026-10-14, on the phone itself** (Redmi Note 7 Pro, Android 16, arm64,
`ci/hardware/close-remaining.sh`): the device suite reports **15 of 17**
green — clipboard round trip, semantic click, a real tap, the modal, keyboard
inset (290 logical px), safe area, density 2.75x, damage tracking, and a real
guest library `dlopen`ed and executed. TalkBack-bound dump: 30 nodes, 26
labelled, roles right (Button, RadioButton with `checked`, ProgressBar with its
value); the modal hides the screen behind it through the bridge (3/3 panel
items, "Tap me" gone) — confirmed on a device for the first time. Maestro's
flow passes end to end on the phone.

**The two failures are real and are performance:** median frame **37.4 ms**
(26 fps), worst 107 ms, 240/240 over the 16.7 ms budget, at 1080x2340. That is
the CPU rasteriser on the phone's cores (the emulator, on a runner's x86
cores, measured 41 ms). The GPU path is correct on real Mesa and NVIDIA
drivers (§1.1) but the Android demo does not use it; trying it on Adreno is
the next measurement. **TalkBack by ear:** answered "no" — what was wrong is
not yet recorded.

**What.** Real code, an NDK-aware build (`ci/mobile/apk.sh`), a device suite
(`ci/mobile/device-suite.sh`), an accessibility suite (`ci/mobile/a11y-android.sh`) and
a hot-reload path. The APKs build green on runners; nothing has ever run on a
device.

**Why not.** The runners build and upload; there is no attached phone and no
device cloud in the loop.

**Blocks.** The other mobile half.

**Start.** `ci/check/platform-check.sh android` for the compile. `ci/mobile/android-env.sh`
already resolves the SDK and NDK and already knows the traps (a version
directory without `source.properties` is a half-finished download); ask it
rather than guessing.

## 1.6 The web backend on other browsers

**What.** The byte-for-byte canvas parity (`examples/test-web`,
`verify_web.py`) is a Chromium-on-Linux result. `ci/check/wasm-check.sh` passed on
2026-09-14 and again on 2026-10-09 on the pinned toolchain; see `TRACKER.md`.

**Why not.** No Firefox/WebKit run has been scripted.

**Start.** `verify_web.py` drives Playwright; `p.firefox` and `p.webkit` are the
same API.

## 1.7 The GPU 3D seam: unlit and Lambert passes, per-call bring-up

**What.** `vieww-3d`'s `GpuRenderer` (behind the crate's `vulkan` feature) holds
the CPU renderer's `(scene, camera) -> RGBA8` contract with two passes through
`vieww-hal`'s Vulkan backend, both held to the CPU oracle in interiors on
lavapipe:
- **unlit** (2026-10-11) — `Shading::Basic` scenes, `tests/gpu_unlit.rs`;
- **lit** (2026-10-12) — `Shading::Lambert` (and Basic alongside) under
  ambient, hemisphere and directional lights, with the shadow map of the first
  shadow-casting directional light, `tests/gpu_lit.rs`: a smooth and a flat
  sphere, a rotated box, a coloured second light, a long cast shadow (lit and
  shadowed ground both asserted), and single-/double-sided back faces.
  Mutation-checked: disabling the shadow lookup fails it by 1,644 interior
  pixels, not flipping back-face normals by 4,670.

The lit pass decides back faces per triangle on the CPU while packing (cull
single-sided, flip double-sided — the CPU rasterizer's rule, by the side of
the triangle's plane the eye is on), and uses the **CPU renderer's own shadow
map**, uploaded as a storage buffer and filtered with the CPU's 3×3 PCF in
the shader. Both `vieww-hal` passes now share one `render_3d_pass` (the unlit
one became a thin wrapper; its tests are unchanged and green). Anything
outside the reach falls back to the CPU renderer for the whole frame; the
module doc lists it.

**Why not closed.** Phong/Standard/Toon/Normal shading, point and spot lights,
image-based lighting and GI probes, textures and the transparent pass are not
in the passes yet. The shadow map is rasterised on the CPU, not the GPU. And
the whole pipeline — render pass, shaders, descriptor set — is still brought
up and torn down per render; a cached-pipeline struct is the step the 2D path
took with `SceneRenderer`.

**Blocks.** Nothing; the CPU renderer remains the full-fidelity path.

**Start.** Point and spot lights, then Phong (the shader's light loop already
has the CPU's structure — add the cases with the CPU's formulas verbatim, and
a parity scene each). A GPU-rendered shadow map has to reproduce the CPU's
`raster_depth` sampling to keep shadow decisions identical, so measure first
whether the CPU shadow pass matters next to the per-render bring-up.

---

# 2. GPU renderer coverage

`vieww_gpu::ScenePlan::is_complete` is the gate. As of 2026-10-09 every
`Command` plans on the GPU, geometry edges are antialiased analytically for the
rect family and with the CPU's own coverage for every other shape, a GPU frame is rendered straight into a real window's
swapchain image ([`App::prefer_gpu`]), and the glyph atlas evicts between frames — all closed
with evidence in `TRACKER.md`; the full picture is
[`docs/GPU-RENDERER-STATUS.md`](./docs/GPU-RENDERER-STATUS.md). What remains:

## 2.1 A general path that changes every frame — measured; the upload was the cost, and is fixed

**What.** Non-analytic shapes are antialiased on the GPU with the CPU
rasterizer's own coverage, cached in the mask atlas by content
(`GpuSeam::shape_coverage`; `TRACKER.md` 2026-10-11). A shape that changes
every frame misses that cache every frame. This item asked to measure that
before reaching for MSAA, and `vieww-gpu`'s `examples/anim_paths` now does:

```
scenario (120 frames, 1366x679)   coverage/frame   upload/frame: before   after (mean)
rotating 12-point star, r=160          0.37 ms             1.05 MB         0.26 MB
3px stroke drawn along a wave          0.004 ms              —             0.15 MB
rotating 600x360 rounded panel         1.3 ms              4.19 MB         1.02 MB
all three                              1.9 ms             16.78 MB         1.91 MB
```

The CPU coverage is real but bounded — ~1.3 ms for a large rotating panel —
and does not justify an MSAA path that would differ from the CPU on every
edge. **The upload was the cost**: the Vulkan backend re-uploaded the whole
mask texture on every version change, and the atlas, which compacted only
on live texels, *grew* to 4096² under churn instead of compacting. Fixed
2026-10-12:
- `MaskAtlas::changes_since` reports the rectangles blitted since the frame
  began (or `Full` after a compaction, a grow, or a skipped frame), and
  `SceneRenderer::upload_masks` copies only those, keeping the rest of the
  texture.
- Compaction triggers on the packing frontier (past three quarters) when at
  least half the packed texels are dead — so a churning atlas compacts at
  its size instead of growing, and never every frame.

Held by `mask_atlas::changes_since_brings_a_texture_exactly_up_to_date`
(replays the changes onto a model texture across compactions, grows and a
skipped frame) and `vulkan_scene::partial_mask_uploads_match_full_uploads_frame_for_frame`
(48 animated frames on lavapipe, each byte-identical to a fresh renderer's
full upload). The GPU census's parity numbers are unchanged.

**What remains.** Frames with a compaction still upload the whole texture
(a compaction moves every live mask); a frame with a grow allocates a new
one. On lavapipe the upload is a memcpy, so the real-GPU benefit — PCIe
bytes and the submit-and-wait per upload — needs hardware to put a number
on (§1.1).

## 2.2 Backdrop blur edge mode — closed 2026-10-13

`vulkan_compositor::a_backdrop_blur_blurs_what_is_behind_and_keeps_the_foreground_sharp`
failed on lavapipe (Mesa 25.2): 816 pixels over tolerance, worst 34, all on
the rim of the frosted panel. Cause: `post.wgsl`'s box passes read past their
region as **transparent** (right for shadows, which must fade at their own
edge), while `effects::blur` **clamps** to the edge pixel. A backdrop copy
blurred with transparent edges loses alpha along its rim, so the composite
showed the unblurred page through it.

Decision: CSS's `duplicate` edge mode, which the CPU oracle already
implements. The GPU blur now takes an `Edge` — `Transparent` for shadows,
`Duplicate` for layer and backdrop filters (`pc.args.z`). Vulkan suite 17/17
(and the rest of `vieww-hal --features vulkan -- --ignored`) on lavapipe.
Metal/D3D12 do not run `post.wgsl` yet; their `SceneRenderer` ports must pass
the same flag.

---

# 3. Platform and product gaps

## 3.1 Outgoing drag-and-drop is a stub

**What.** `vieww-interaction`'s `NullDragStarter`. Dragging a file *out* of a
vieww window to another application does nothing.

**Why not.** No portable API in `winit` 0.30 — tracked upstream as winit issue
#1550. The stub is honest and documented rather than faking a success path.

**Blocks.** File-manager-shaped applications.

**Start.** Per-platform, below winit: `NSDraggingSession` on macOS,
`DoDragDrop` on Windows, XDND / the `wlr-data-control` protocol on Linux.

## 3.2 A monitor change re-reads the refresh rate — untested with two monitors

**What.** The rate is read from the platform everywhere vieww runs: winit
answers on macOS, iOS, Windows, X11 and Wayland, and Android (where winit
hard-codes `None`) is read below it through `Display.getRefreshRate()`. Since
2026-10-11 it is also re-read when a window moves to another monitor or a
Wayland surface enters an output (`WindowState::display_may_have_changed`),
where before it was read only at open.

**Why not closed.** The re-read has never seen a real monitor change: this
machine has one virtual display, and Xvfb's single CRTC is all winit's X11
backend enumerates.

**Blocks.** Nothing until someone drags a window between a 60Hz and a 120Hz
display; then the `vieww: … reports …Hz` log line should appear again.

**Start.** Do exactly that, with `RUST_LOG=info`.

**2026-10-14, first attempt on a real two-monitor GNOME/Wayland laptop:** no
refresh-rate line at all — not at open, not after the drag. Two causes, both
fixed: the desktop example installed no logger, so `RUST_LOG` printed nothing
(it now has a minimal stderr logger); and on Wayland none of the three
re-read triggers fired (`current_monitor` is `None` at open, `Moved` is never
sent, `ScaleFactorChanged` only when the scale changes). The draw path now
re-checks the monitor at most once a second (`WindowState::recheck_display`).
The second run still printed nothing: the re-check sat on `RedrawRequested`,
and most frames are drawn straight from `about_to_wait`. It now runs inside
`WindowState::draw`. Verified on a headless Weston (a real Wayland compositor):
`vieww: window#primary reports 60.0Hz` now appears, after the open-time read
found no monitor. A monitor that names no rate is now logged too. Needs one
more run of `ci/hardware/close-remaining.sh monitors` with two monitors.

## 3.3 The out-of-process preview exists; it is not yet complete or proven on Windows

**What.** `apps/viewwstudio/src/remote.rs` + the `viewwstudio-preview` binary
(2026-10-11): each Render's guest runs in a child process, drawn back over a
pipe; a guest `abort` costs the child, not the studio. Setting "Run preview in
its own process", on by default on Windows. `tests/remote.rs` (4 tests) passes
on Linux: frames, taps, 2x scale, abort isolation with the guest's stderr
returned, later-rebuild panic, and the studio's own Render into a child.

**Not done.** Keyboard/IME, scroll and hover are not forwarded; preview state
capture across Renders (`capture_preview_state`) has no remote counterpart;
never run on Windows (the reason it exists); the full `viewwstudio` suite was
not re-run after the change (the disk filled mid-run).

**Start.** `cargo test -p viewwstudio` on Linux, then on Windows; then add
key/scroll forwarding to the wire in `remote.rs`.

---

# 4. CPU rasterizer performance

## 4.1 Two fixtures are over a 60 Hz full-repaint budget — narrowed, not closed

**What.** `00-layers-nested` and `23-editor-glass`, full repaint at their
sizes, against 16.7 ms. On the session machine (2026-10-12, 2 cores) they
went from ~28.5 ms and ~27 ms to **~20–22 ms and ~21 ms**; the whole gallery
from 232.7 ms to 164.6 ms. Every gallery output (30 files: stills, motion
GIFs, interaction frames) is byte-identical before and after.

**What was done** (TRACKER.md's rendering-performance section has the
detail):
- `-C target-cpu=x86-64-v2` restored in `.cargo/config.toml` — the file's
  reconstruction (`375aa13`) had dropped it, so `floor`/`ceil` were library
  calls again. Worth ~3% of instructions here.
- The RGBA8 output conversion as SSE2, four pixels per 16-byte store
  (`color::write_straight_u8`).
- `composite_flat`'s and `composite_layer`'s normal-blend rows as SSE2
  (`native/simd.rs`), branches kept only where they are long, predictable
  runs (no coverage; an opaque colour at full coverage).
- A layer's pool clear fused into its composite
  (`Target::composite_layer_and_clear`) — memory traffic, timed: nested
  25.7 → 22.0 ms median of ten interleaved runs.

callgrind, whole fixture run (warm-up + 3 frames + PNG encode): glass
1,454 M → 1,172 M instructions, nested 1,573 M → 1,005 M.

**Why not closed.** Still ~4–5 ms over. What is left is spread out: blur
(~12% of glass), shadows, glyph and path rasterisation, the root buffer's
reset, and — for nested — memory bandwidth on 16-byte-per-pixel layers
rather than arithmetic (the layer row is ~19 instructions a pixel, near the
SSE2 floor). A retained repaint of glass is now ~7.5 ms.

**Start.** Measure first, both ways (callgrind counts `rep stosb` per byte,
so `memset` looks ~20x worse than it is; time anything that moves memory).
Candidates: an SSE2 blur (the horizontal running sum and the vertical strip
pass); half-float or 8-bit layer storage for layers that need no extended
range (cuts nested's traffic in half or more, but changes bytes — needs a
tolerance decision); AVX2 behind runtime detection.

---

# 5. Process and infrastructure

## 5.1 The coverage number is produced, not watched

**What.** Closed as a gate: `ci/check/coverage.sh` fails below the floor
(70% of lines) and the `framework-checks` workflow's `coverage` job runs it on
every push with no `continue-on-error` — a number below the floor reds the
build. (The workflow itself is in `.github/workflows-pending/` awaiting a
`workflow`-scoped token to be placed live; see its README.) The number is produced on the pinned toolchain; see `TRACKER.md` for
the runs. (History note: the earlier shape of this item — "drop
`continue-on-error` once it has history" — was overtaken by events: the
`framework-checks` workflow was written fresh with the job already a gate,
2026-10-11, and the first number plus the floor holding across the session's
full-workspace rerun was taken as the history the old wording asked for.)

**Blocks.** A merge whose tests' line coverage falls below the floor, which is
the point.

**Start.** Nothing — unless the floor itself needs arguing about, in which
case the place is `ci/check/coverage.sh`'s `FLOOR=70` and the reasons in its
header comment, not this file.

---

# 6. Known-unknowns

Two things are worth working on and are not yet understood well enough to have
a "start here". They are listed so they are not mistaken for oversights.

## 6.1 Is the GPU path actually worth it on this workload?

Nobody knows. The CPU rasterizer is unusually good — a full studio frame in
about 49 ms of wall clock after a 1.73× improvement, and a *retained* repaint
of the same screen in under 10 ms — and the GPU path is one draw call for a
whole frame but has never run on hardware. It is entirely possible that for
dense, static, text-heavy interfaces the honest answer is "the CPU path is the
product and the GPU path is for animation-heavy screens". §1.1 is what would
tell us, and until it happens this repository should be careful not to imply an
answer it does not have.

## 6.2 What is the story for embedded native views?

Video, a map, a web view, a camera preview — anything the OS draws that vieww
must position and clip but not rasterize. `vieww_foundation::capability` has
`platform_view` types, and no application has ever used one. Whether the
existing shape is the right one is unknown, and it is the kind of thing whose
design is decided by the second consumer rather than the first.

---

# Deliberate non-goals

Listed so they are not repeatedly re-raised as gaps. Each has its reasoning
written where the decision lives.

- **`serde`.** Not used anywhere in this workspace, by choice — see
  `crates/vieww-devtools/src/json_export.rs`'s module doc.
- **`wgpu`.** Removed with vello and not coming back. The render graph, the
  damage tracking, the CPU/GPU placement decision and the frame scheduling are
  what vieww exists to own; designing them around another abstraction's model
  is the outcome that choice avoids. See `README.md`.
- **Cross-backend parity as a suite.** There is one renderer compiled two ways,
  not two renderers. Where two genuinely independent implementations *do* meet
  — tessellated geometry versus analytic scanline coverage — `vulkan_scene.rs`
  compares them. Where they do not, `vulkan_text.rs` demands exactness instead.
  See `TRACKER.md`.
- **A separate container-query widget.** `LayoutBuilder` already is one.
- **Renaming crates to match an earlier architecture sketch**
  (`vieww-window`, `vieww-native`). The functionality exists under other names;
  a risky rename to match a document is not a fix.
- **COLRv1 paint graphs.** The colour-glyph resolver does COLRv0 exactly and
  bitmap strikes, and stays out of v1 deliberately: an approximate paint-graph
  interpreter is worse than none. See
  `crates/vieww-paint/src/native/color_glyphs.rs`'s module doc.
