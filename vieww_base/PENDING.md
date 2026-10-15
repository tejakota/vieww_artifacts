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

Three facts sit behind most of the list and are not repeated in every entry
(all three changed on 2026-10-14; `TRACKER.md` has that day's evidence):

1. **GPU correctness is proven on real hardware; GPU speed is proven on one
   phone.** The Vulkan suites pass on Intel (Mesa) and NVIDIA drivers, and the
   planned GPU path is 1.7x faster than the CPU rasterizer on an Adreno 612.
   No desktop GPU *timing* exists yet (§1.1). Metal has run only on a CI
   runner's paravirtual GPU and D3D12 only on WARP, so neither has a
   performance number either.
2. **Every leg of `framework-checks.yml` has passed** — Linux, macOS (Metal),
   Windows (D3D12 on WARP), the cross type-check, web in three browsers, the
   Android emulator and the iOS simulator. The workflow is manual-only; legs
   that have passed run again only with `run_passed` ticked.
3. **Frames have reached real screens on Linux, Android and iOS** — a laptop,
   a Redmi Note 7 Pro and an iPhone 13 Pro. Never yet on a real Mac or a
   Windows PC with a GPU.

---

# 1. Blocked on hardware or a toolchain

Not ordinary work. Each of these needs a machine this one is not, and none of
them can be closed by reading the code more carefully.

## 1.1 GPU timings on a desktop GPU

Correctness on real GPUs is closed (TRACKER, 2026-10-14). What is left is
speed on a desktop.

**What.** `examples/fixtures` timed with its scenes planned onto the GPU
(`Placement::Gpu` where `ScenePlan::is_complete` allows), beside its CPU
numbers (185.3 ms for the gallery on the Kaby Lake laptop).

**Why not.** Only for want of the machine. (The previous wording here said the
gallery had no GPU mode; it has had one all along — `fixtures --census` renders
every plannable fixture through `vulkan::SceneRenderer`, with CPU/GPU parity —
and since 2026-10-15 it writes a receipt naming the GPU, driver type, build and
commit beside every raw run, plus a planner shadow log.)

**Blocks.** A desktop answer to §6.1 (the phone already has one: 1.7x), and
the GPU half of the planner's calibration (§2.3).

**Start.** `ci/hardware/close-remaining.sh gallery` on the Kaby Lake laptop:
it now runs the gallery and the census with `--runs 30 --receipt --shadow`.
Read `census/receipt.json`'s `gpu` block first — if it says `CPU`, the run
picked lavapipe and the numbers are not a GPU's. Then update the "unmeasured
for speed" wording in `docs/guide/README.md`, `crates/vieww-hal/src/lib.rs`
and `docs/GPU-RENDERER-STATUS.md`.

## 1.2 macOS — the Metal backend runs; its scene renderer is not ported

**What.** `crates/vieww-hal/src/metal.rs` has `render_clear_to_pixels` and
`render_mesh_to_pixels` (WGSL → MSL through `vieww-shaders`), and
`tests/metal_smoke.rs` passes on the `framework-checks` macOS runner with
`VIEWW_REQUIRE_HARDWARE=gpu` — so the generated MSL compiles and draws
correctly on Apple's driver. `SceneRenderer` is not ported, so on macOS and
iOS a vieww window renders through Vulkan on MoltenVK (as the simulator and
the iPhone do), not native Metal.

**Why not closed.** The scene path (`vulkan/scene.rs`) has no Metal twin; the
runner's GPU is paravirtual, so no number from it is a performance claim.

**Blocks.** Native Metal rendering (dropping the MoltenVK dependency).

**Start.** Port `SceneRenderer` onto `MetalDevice::pipeline`/`render_target`/
`read_texture`, passing `post.wgsl`'s `Edge` flag (§2.2).

## 1.3 Windows — the D3D12 backend runs on WARP; its scene renderer is not ported

**What.** `crates/vieww-hal/src/d3d12.rs` has the same pair (naga HLSL via
`d3dcompiler_47`, root constants, a fenced readback, a WARP fallback behind
`VIEWW_D3D12_ALLOW_WARP=1`), and `tests/d3d12_smoke.rs` passes on the
`framework-checks` Windows runner on WARP. `SceneRenderer` is not ported.

**Why not closed.** The smoke suite has now passed on a **hardware** adapter
too (2026-10-15, `ci/hardware/windows.ps1`, the Kaby Lake laptop under
Windows 10 — receipts in `receipts/2026-10-15-kabylake-i5-7200u-windows/`),
so bring-up is real. There is still nothing to *time*: `SceneRenderer` is not
ported, so no D3D12 frame exists.

**Blocks.** Windows GPU rendering. Windows also has a *separate* unsolved
problem — see 3.3.

**Start.** Port `SceneRenderer`; then run the suite on a Windows PC with a GPU.

## 1.4 iOS — runs on a real iPhone; its numbers are not recorded

Closed 2026-10-14 (TRACKER): the simulator (device suite 12/12, 6.8 ms
median) and a real **iPhone 13 Pro on iOS 15** through LambdaTest's
real-device cloud, driven by hand: rendering, taps, the system keyboard,
shared selection, animation and the drawer all work.

**What is left.**
- The device suite's verdict and frame times on a real iPhone. The suite
  prints them as `__VIEWW_SUITE__` lines to the device log; that session's
  log was not exported.
- iOS 16 and later on a real device (only iOS 15 was available free).
- An unattended run: `ci/mobile/test-lab.sh ios` / the `test-lab-ios` job need
  a `.ipa` signed with a real Apple development identity (`IOS_*` secrets);
  Firebase Test Lab refuses ad-hoc signatures.

**Start.** On LambdaTest, type `VIEWW` into Device Logs' filter (or download
the log) during a run and record the suite's lines here. With an Apple
developer account: set the `IOS_*` secrets and run the workflow with
`test_lab_ios` ticked.

## 1.5 Android — runs on a phone; the GPU path is 4.3 ms over budget

Closed 2026-10-14 (TRACKER): emulator and a real phone (Redmi Note 7 Pro,
Adreno 612) — device suite, accessibility suite, Maestro, TalkBack by ear, and
the planned GPU path correct and 1.7x faster than the CPU rasterizer.

**What is left.**
- **21.0 ms median on the GPU path against a 16.7 ms budget.** Where the time
  goes — planning on the CPU, the GPU's own work, or the present — is not
  measured.
- **The GPU path is not the default.** `examples/android.rs` takes it only
  with `debug.vieww.gpu=1` or Test Lab's scenario 2.
- **More than one phone.** `ci/mobile/test-lab.sh android` (and the
  `test-lab-android` job) runs CPU and GPU scenarios on three phones from
  different makers. Not run yet: the console rejected the first upload ("One
  of your APKs that should contain code has none" — a pure-Rust APK has no
  `classes.dex`); `ci/mobile/apk-add-code.sh` now adds one, and the CI APK
  build runs it. Test Lab shuts down on 30 September 2027.

**Start.** `ci/mobile/device-suite.sh --gpu` on the phone and read the new
`frame_stages` observation (added 2026-10-15): median scale, plan, CPU raster,
GPU upload / record / submit / fence wait / readback, acquire and present, per
frame. The fence wait is an upper bound on the GPU's own work while one frame
is in flight. Then decide the default.
For more phones: the `GCP_SA_KEY`/`FTL_PROJECT` secrets and the workflow with
`test_lab_android` ticked, or upload `vieww-arm64.apk` from the `vieww-apk`
artifact to Test Lab's console as a **Game Loop** test.

## 1.6 The web backend on other browsers — closed 2026-10-14

Canvas parity passes in Chromium, Firefox and WebKit in the `web` leg of
`framework-checks.yml` (`verify_web.py` through Playwright). See TRACKER.

## 1.7 The GPU 3D seam: unlit and Lambert passes, per-call bring-up

**What.** `vieww-3d`'s `GpuRenderer` (behind the crate's `vulkan` feature) holds
the CPU renderer's `(scene, camera) -> RGBA8` contract with two passes through
`vieww-hal`'s Vulkan backend, both held to the CPU oracle in interiors on
lavapipe (and, 2026-10-14, 7/7 on a real Intel HD 620 under Mesa):
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

## 2.3 The hybrid planner runs in shadow; it is not authoritative

**What.** `vieww_render_planner::plan_frame` places each pass on the CPU, the
GPU or both, and since 2026-10-15 it runs beside real frames in shadow
(`fixtures --shadow`, `VIEWW_PLANNER_SHADOW` on a window) and logs what it
would have done. Nothing yet *acts* on its placement: `present_planned` still
draws a whole frame on the GPU when `ScenePlan::is_complete` and a whole frame
on the CPU otherwise.

**Why not.** Its inputs are not yet trustworthy enough to put in charge:
- **CPU cost is area-only.** On the gallery, strokes, curves and shadows cost
  2–2.7x a plain fill of the same area (shadow log, 2026-10-15); the heuristic
  has no per-command complexity term. Plain fills, text and the editor
  screens are within ~20%.
- **GPU cost is 8x optimistic, now measured.** The baseline window's shadow
  logs on the reference laptop's NVIDIA 920MX (2026-10-15, ~11,500 GPU frames
  per OS, `receipts/2026-10-15-kabylake-*/baseline/gpu-shadow.jsonl.gz`):
  predicted 0.22 ms, measured median 1.85 ms (Linux) / 1.83 ms (Windows) —
  8.4x / 8.3x. The planner still chose correctly there (GPU over a 3.8 ms CPU
  estimate), but a model 8x off cannot make close calls. The census's GPU
  figures include a PCIe readback every frame and are not usable for this.
  The CPU side on the same laptop: 1.67x (Linux) / 1.72x (Windows)
  optimistic over the gallery.
- **The 250,000-pixel hybrid threshold is a guess**, and no backend can
  execute a split pass.

**Blocks.** Roadmap P2 — the hybrid engine making the live decision.

**Start.** Collect shadow logs from the laptop (`close-remaining.sh gallery`)
and the phone (`VIEWW_PLANNER_SHADOW`, or the device suite) and read
`Calibration::calibrated` for each. Add a per-command complexity weight to
`HeuristicCostModel` from the gallery's error ratios (`00-strokes`,
`00-curves`, `00-shadows` are the outliers). Only then put an
authoritative mode behind a feature flag, keeping `is_complete` as the
per-frame fallback gate.

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
**Paused (2026-10-14): no second monitor available.** Open until one is.

## 3.3 The out-of-process preview exists; it is not yet complete or proven on Windows

**What.** `apps/viewwstudio/src/remote.rs` + the `viewwstudio-preview` binary
(2026-10-11): each Render's guest runs in a child process, drawn back over a
pipe; a guest `abort` costs the child, not the studio. Setting "Run preview in
its own process", on by default on Windows. `tests/remote.rs` (4 tests) passes
on Linux: frames, taps, 2x scale, abort isolation with the guest's stderr
returned, later-rebuild panic, and the studio's own Render into a child.

**Not done.** Keyboard/IME, scroll and hover are not forwarded; preview state
capture across Renders (`capture_preview_state`) has no remote counterpart;
the full `viewwstudio` suite was not re-run after the change (the disk filled
mid-run). `tests/remote.rs` **passed on Windows** on 2026-10-15 (Windows 10,
`ci/hardware/windows.ps1 remote`) — the platform it was written for.

**Start.** `cargo test -p viewwstudio` on Linux, then on Windows; then add
key/scroll forwarding to the wire in `remote.rs`.

## 3.4 Windows + NVIDIA: the process crashes at teardown, after a good frame

**What.** `tests/wait_loop.rs`' `self_close` and `gpu_frame` scenarios exit
with `0xc0000005` (access violation) on Windows 10 with the NVIDIA GeForce 920MX
(driver 32.0.15.7652) — **after** each scenario has passed and `run` has
returned its report (2026-10-15, `receipts/2026-10-15-kabylake-i5-7200u-windows/gpu-window.log`).
The same scenarios pass on the same laptop under Linux. Drawing works; the
process dies tearing Vulkan down.

**Why not closed.** Not yet reproduced with the diagnostic: `Gpu`'s `Drop` in
`app.rs` already describes an NVIDIA driver fault on instance teardown and
gates the destruction behind `VIEWW_KEEP_VULKAN`.

**Blocks.** Any Windows app on an NVIDIA GPU exits with a crash code — crash
reporters fire, installers and CI see a failure.

**Start.** On the Windows machine: `$env:VIEWW_KEEP_VULKAN=1; cargo test
--release -p vieww-platform-winit --test wait_loop`. If it passes, the fix is
the one that comment names: one shared `VkInstance`/`VkDevice` per process, not
one per window (the trace also shows the capability probe creating and
destroying an instance before the window's own).

## 3.5 Windows start-up is ~4x Linux on the same laptop

**What.** `examples/baseline.rs`, warm cache, median of 5: `main` to first
frame **840 ms on Windows vs 221 ms on Linux** (same i5-7200U, same commit).
The window test's trace attributes the big pieces on Windows: Vulkan device
ready after **215 ms**, the first swapchain present **197–236 ms**, and on the
GPU path the first frame's command recording **302 ms** (pipeline creation,
no pipeline cache).

**Start.** A `VkPipelineCache` persisted to disk; create the device in parallel
with the window; run `baseline` with `VIEWW_TRACE_FRAMES=1` to attribute the
remainder.

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

**2026-10-15, the reference laptop itself** (i5-7200U, pinned rustc 1.98.1,
30 runs, receipts in `receipts/2026-10-15-kabylake-i5-7200u-{linux,windows}/`):
`00-layers-nested` best 20.2 / p50 23.8 ms on Linux, 20.8 / 22.2 on Windows;
`23-editor-glass` 24.6 / 24.8 on Linux, 23.3 / 24.2 on Windows; the whole
gallery 195 ms on both. Over budget by 4–8 ms on both OSes alike — this is the
renderer, not an OS. The cloud-VM figures below were optimistic.

**2026-10-15, a different machine.** A 2-core cloud Xeon (AVX-512, rustc
1.97), `fixtures --runs 10 --receipt`: nested best 14.0 / p50 14.2 ms and
glass 15.6 / 15.6 ms in one run; nested 13.1 / **19.4** ms and glass 15.9 /
**16.7** ms in the next, same binary. Under budget at best-of, not reliably at
the median, on a machine that is not the reference laptop — so this stays
open, and the reference number should come from a receipt on that laptop.

**Start.** Measure first, both ways (callgrind counts `rep stosb` per byte,
so `memset` looks ~20x worse than it is; time anything that moves memory).
Candidates: an SSE2 blur (the horizontal running sum and the vertical strip
pass); half-float or 8-bit layer storage for layers that need no extended
range (cuts nested's traffic in half or more, but changes bytes — needs a
tolerance decision); AVX2 behind runtime detection.

## 4.2 Memory and size are the farthest from the "lightest" targets

Measured 2026-10-15; targets from the audit's Phase 6.

| | Measured | Target |
|---|---|---|
| Baseline window peak RSS, laptop | 150 MB Linux / 89 MB Windows | under 40 MB |
| Device suite RSS, phones | 285–388 MB (Galaxy S20, S26 Ultra) | — |
| Windowed app, stripped release | 13.1 MB Linux (static std) / 7.4 MB Windows | — |
| Android release APK | 15.6 MB | under 6 MB |

Idle is already right: 0 frames and 0 pixels while nothing changes, on both
OSes. Linux shows 3.4% idle CPU on the CPU path with no frames — something
wakes without drawing.

**Start.** A heap profile of `baseline` (`heaptrack`) and a size breakdown
(`cargo bloat --release --example baseline -p vieww-platform-winit -n 40`);
on the phone, `dumpsys meminfo` by category is already in the Device Farm
artifacts.

---

# 5. Process and infrastructure

## 5.1 The coverage number is produced, not watched

**What.** Closed as a gate: `ci/check/coverage.sh` fails below the floor
(70% of lines) and the `framework-checks` workflow's `coverage` job runs it on
every push with no `continue-on-error` — a number below the floor reds the
build. (The workflow is live at the repository root, `.github/workflows/`,
and manual-only; the coverage leg runs with `run_passed` ticked.) The number is produced on the pinned toolchain; see `TRACKER.md` for
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

Half answered. On a phone, yes: on the Adreno 612 the planned GPU path draws
the demo in 21.0 ms against the CPU rasterizer's 36.4 ms, and its worst frame
is 4x better. On a desktop nobody knows yet. The CPU rasterizer there is
unusually good — a full studio frame in about 49 ms of wall clock, a
*retained* repaint in under 10 ms, 1.76 ms for the demo on the laptop — so it
is still possible that for dense, static, text-heavy interfaces on a desktop
the honest answer is "the CPU path is the product and the GPU path is for
phones and animation-heavy screens". §1.1 is what would tell us.

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
