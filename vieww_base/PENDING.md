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

**2026-10-16: the first automated real-iPhone run** (AWS Device Farm,
`ci/mobile/device-farm/run-ios.sh`, ad-hoc .ipa re-signed by Device Farm).
iPhone 15 Plus, iOS 17.4: device suite **PASS on both paths**, 13/13 checks,
taps through XCUITest landing. CPU path median 5.9 ms (raster 3.1, present
2.5); GPU path (MoltenVK) median 16.3 ms — `gpu_submit` 6.9 + `gpu_wait` 8.0,
so on this phone the CPU path is the faster one by ~3x. First frame from
`main` ~377 ms (CPU) / ~400 ms (GPU), well over the 120 ms target. The iPhone
13 (iOS 16) and iPhone 18 Pro Max (iOS 27) did not run — fixed in the scripts,
not yet re-run.

**Second run, same day — three iPhones, three routes:**

| iPhone | iOS | route | CPU path | GPU path (MoltenVK) | first frame CPU / GPU |
|---|---|---|---|---|---|
| 13 | 16.0 | Appium only | 7.8 ms PASS | 16.2 ms PASS | n/a (no console below 17) |
| 14 | 17.6 | devicectl | 8.0 ms PASS | 16.2 ms PASS | 352 / ~390 ms |
| 18 Pro Max | 27.0 | devicectl | 2.8 ms PASS (120 Hz budget) | 16.3 ms **FAIL** (8.3 ms budget) | 280 / ~290 ms |

The GPU path's ~16.2 ms is the same on every iPhone, from a 2021 A15 to a 2026
part, with `gpu_submit` 7-14 ms of it: that is vsync, not work. MoltenVK
acquires the Metal drawable lazily inside the submit, so waiting for the next
display refresh lands in `gpu_submit`, and the app was held to 60 Hz on the
120 Hz phone because the bundle lacked `CADisableMinimumFrameDurationOnPhone`
(now set). Separating drawable wait from GPU work on MoltenVK is open; until
then the iOS GPU path's frame time is a pacing figure, and the CPU path —
which is under budget on all three — is the honest work number.

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

**2026-10-18: the default decision has an input, a lever and a procedure; what
it does not have yet is the data.** The device-shaped input every quality
decision below the rasterizer choice was missing now exists:
`vieww-platform-winit`'s `device_tier` (RAM, core count, SoC and API level,
read from system properties without loading a driver; Low/Medium/High;
`VIEWW_DEVICE_TIER` overrides). It feeds a `QualityProfile` whose one field is
the GPU path's resolution scale: `debug.vieww.scale=auto` renders the planned
scene at the tier's resolution (Low 0.5×) and blits up; the CPU path never
scales, because its bytes are the fidelity reference (FIDELITY.md). The
Device Farm's sixth pass runs exactly that, the farm summary carries
`quality_overlay` — the tier policy and the frame clock on one line, a report
out of `vieww-devtools` rather than a widget (no egui in this tree, and
overlay pixels drawn into the scene would be CPU-path pixels the fidelity
rule does not own) — and `ci/mobile/gpuscale_verdict.py` applies the
promotion rule `quality.rs` documents: a device needs the tier default only
if the best of its CPU and GPU medians misses the pass's own budget; the
scale earns it only if it engaged below 1.0 and landed at or under that
budget; a tier is promoted only when every device of it passes. The tool
writes `gpuscale-verdict.json` and the SUMMARY.md memo. What remains is the
farm run itself — until then the scale stays opt-in, and the flip in
`quality.rs` is the one line a promoted verdict licenses.

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
would have done. Since 2026-10-18 it also has the machinery to be trusted
and then obeyed, both behind data:

- **Per-command complexity.** `CostHint::cpu_complexity` prices strokes,
  curves and shadows at 2–2.7x a plain fill of the same area (the gallery
  shadow log's measured band; the within-band attribution — strokes worst,
  shadows least — is stated as an assumption in `vieww-scene`'s
  `cost.rs`), and `HeuristicCostModel` multiplies its CPU estimate by it.
  An area-only model priced a strokes-heavy pass identically to a plain
  fill, which is what made every close CPU/GPU call on such a scene wrong.
- **A calibration the planner applies itself.** `Calibration` tracks both
  the ratio of sums (what `Calibration::calibrated` corrects the profile
  with) and the per-frame ratio RMSE (how *consistent* the error is). The
  raw model's 8.4x GPU optimism on the reference laptop is exactly a
  steady-multiplicative error, which is the kind a calibration removes.
- **A mode machine.** `ShadowPlanner` goes `Shadow` → `ShadowCalibrated`
  (after 1,000 observed frames it plans with the calibrated profile and
  starts a fresh gate) → `Authoritative` (the gate cleared), only when the
  application opts in with `VIEWW_PLANNER_AUTHORITATIVE=1` beside
  `VIEWW_PLANNER_SHADOW`. Actuation is deliberately narrow: whole-frame
  CPU recommendations only, only while the gate is `ready` at that moment
  (RMSE ≤ 5% over 1,000+ frames), `Mixed` never (no backend can execute a
  split pass), GPU never (the live path already prefers the GPU when
  `ScenePlan::is_complete` passes). `is_complete` remains the per-frame
  fallback gate; a resolution-scaled frame is never planned for a decision
  (the plan would describe a picture nobody drew).

**Why not closed.** The gate has never been fed real frames. The checked-in
receipts are raw-model runs (`receipts/2026-10-15-kabylake-*/baseline/`,
11.5k GPU frames per OS, every one labeled `window`, every one on the GPU) —
enough to *derive* a calibration, not enough to *test* one. No CPU-side
shadow log from the gallery or the phone exists yet, so the CPU gate — the
one actuation actually needs — has zero real samples. The per-command
weights' within-band attribution is still an assumption for the same
reason: the per-fixture gallery log was never checked in.

**Blocks.** Roadmap P2 — the hybrid engine making the live decision.

**Start.** On the reference laptop: `close-remaining.sh gallery` with both
`VIEWW_PLANNER_SHADOW=<log>` and `VIEWW_PLANNER_AUTHORITATIVE=1` set. The
machine promotes itself when its windows fill; the log's frames carry the
mode in the predictions' provenance. When an authoritative CPU decision
fires, compare that frame's time against the `is_complete` path's on the
same content — that comparison, not a benchmark, is the number that says
the planner belongs in charge. Land the per-fixture gallery shadow log in
`receipts/` and re-attribute the complexity band if the data disagrees
with the stated assumption.
---

# 3. Platform and product gaps

## 3.1 Outgoing drag-and-drop is a stub

**What.** `vieww-interaction`'s `NullDragStarter`. Dragging a file *out* of a
vieww window to another application does nothing.

**Where it moved, 2026-10-18.** Windows is no longer a stub in the way this
entry described: `drag_out_windows.rs` implements
`PlatformDragStarter` through OLE `DoDragDrop` — the shell's own data
object carrying a `CF_HDROP` built by `drag_out::hdrop_bytes` (a pure
function, unit-tested on every machine: the exact `DROPFILES` byte layout),
a two-decision drop source, and the effect mapping back to `DropEffect`.
It type-checks against the fetched `windows` 0.58 crate on the
`x86_64-pc-windows-msvc` target and has **never run** — the
[session-verifies] list in that file's module doc (shell data object
accepts `SetData`; `OleInitialize` on the calling thread; blocking the
event loop for the drag's duration) is what a first Windows session
confirms. The remaining platforms are untouched, and the adapter that
would construct the starter does not exist yet.

**Why not closed.** No portable API in `winit` 0.30 — tracked upstream as
winit issue #1550 — so every platform is below winit, and nothing here can
be proven without sessions this container does not have.

**Blocks.** File-manager-shaped applications.

**Start.** On Windows: wire `WindowsDragStarter` into the adapter, run
`ci/check/checks.sh`'s build, then drag a file from the demo into Explorer
and confirm the path arrives — that one observation closes items 1–3 of
the [session-verifies] list. Text formats (`CF_UNICODETEXT`/
`CFSTR_INETURLW`) after that, macOS `NSDraggingSession` and Linux
XDND / `wlr-data-control` after that.

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

## 3.5 Windows start-up is ~4x Linux on the same laptop

**What.** `examples/baseline.rs`, warm cache, median of 5: `main` to first
frame **840 ms on Windows vs 221 ms on Linux** (same i5-7200U, same commit).
The window test's trace attributes the big pieces on Windows: Vulkan device
ready after **215 ms**, the first swapchain present **197–236 ms**, and on the
GPU path the first frame's command recording **302 ms** (pipeline creation,
no pipeline cache).

**2026-10-18: the first of these is done.** `vieww_hal::vulkan::pipeline_cache`
(1c589d4) opens one `VkPipelineCache` per device, seeded from the file
`set_pipeline_cache_file` names when its 32-byte header names this GPU
(vendor, device) and driver (cache UUID) — anything else is dropped before
the driver sees it, because old mobile drivers have crashed on foreign data —
and saved atomically on Drop. The platform layer points devices at
`pipelines-<exe>.bin` beside the rasterizer calibration
(`VIEWW_PIPELINE_CACHE=<file>` or `=off` override), saves after the first GPU
frame (a phone app is killed, not exited), and the device suite observes
`pipeline_cache`. The 302 ms of first-frame pipeline creation measured above
on Windows, and the 1463 ms first frame the Device Farm timed on the LG
Stylo 5 on every launch, are the numbers that motivated it; what the cache
actually removes on a real driver is still Device Farm's run to show
(lavapipe keeps no compiled code, so the local test only proves the header
round-trip).

**Start.** Create the device in parallel with the window; run `baseline` with
`VIEWW_TRACE_FRAMES=1` to attribute the remainder.

## 3.6 iOS — VoiceOver has never been heard on vieww

**What.** The screen-reader contract is half-proven. The static half is
machine-checked: `vieww-accessibility`'s `audit.rs` walks a `SemanticsTree`
and names every interactive node with no label, every button missing the
button trait, every interactive node that is not focusable — exactly the
bugs that otherwise only surface in a manual session. The Android half is
real: TalkBack has been heard speaking (1.5, 2026-10-14) and
`ci/mobile/a11y-android.sh` runs the suite. What does not exist is the iOS
half: no VoiceOver run is recorded, no XCUITest asserts that a vieww button
is *heard* — label, traits, focus — the way the Android suite does, and no
`UIAccessibility` notification is ever posted from vieww's semantics.

**Where it moved, 2026-10-18.** The iOS side now exists as code:
`vieww-platform-winit`'s `ios_accessibility.rs` builds
`UIAccessibilityElement`s from the same semantics tree `a11y.rs` feeds
AccessKit — label, button/static-text/`NotEnabled` traits, screen-point
frame, the packed render id as identifier — sets the host view up as a
container, and posts `screenChanged` with the focused node's element,
VoiceOver-gated so a device without a screen reader pays nothing. It
type-checks on `aarch64-apple-ios` (selectors verified against the fetched
`objc2-ui-kit` 0.2.2 source) and has **never run** — same standing as
`ios.rs`, whose doc explains why that is still progress. The assertion set
(`ci/mobile/xcuitest/ViewwA11yTests.swift`) and the runner
(`ci/mobile/a11y-ios.sh`) are checked in and equally unrun; the XCUITest
half is blocked on an Xcode project, which `ios-app.sh`'s doc explains the
repository deliberately does not carry.

**Why not closed.** Nothing has been *heard*. The code publishes; a
simulator or device run — and the manual session beside the TalkBack
record — is the part that no container without Apple hardware can do.

**Blocks.** The claim that vieww's accessibility is platform-parity rather
than Android-parity.

**Start.** On macOS: `cargo check --target aarch64-apple-ios` still, then
`ci/mobile/ios-app.sh --sim`, `ci/mobile/a11y-ios.sh --sim`, and the
XCUITest set once a project exists; record what VoiceOver says beside the
TalkBack record. On this container, the honest check is the type-check
and the module docs.

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
| Android release APK | 15.6 MB → **2.6 MB** (`dist` profile, 2026-10-16 Device Farm run) | under 6 MB — **met** |

**2026-10-16, after the CPU present path and the `dist` profile** (Device
Farm run 393108fd, three phones): device-suite PSS on the CPU path 80 / 86 /
86 MB (LG Stylo 5 / Galaxy A36 / S26 Ultra), GPU path 137 / 104 / 176 MB.
Desktop under Xvfb: ~31 MB steady RSS on the CPU path (no GPU driver loaded).
On the LG the CPU path's present fell from 19.8 ms to 5.1 ms, but its
rasterisation rose from 20.1 to 29.8 ms in the same run — an A/B of the
`dist` and `release` profiles on that phone (`device-farm.yml`'s `profile`
input) is what tells a build regression from a different device instance.

**Same build, same phone, both present paths** (run 38876153617, `dist`; the
`cpuvk` pass presents CPU frames through Vulkan):

| phone | CPU present: raster + present = frame | Vulkan present: raster + present = frame | PSS CPU / Vulkan present |
|---|---|---|---|
| LG Stylo 5 | 30.6 + 5.0 = 36.3 ms | 20.9 + 19.6 = 42.0 ms | 76 / 133 MB |
| Galaxy S20 (1440x3200) | 8.1 + 7.1 = 15.4 ms | 7.7 + 3.2 = 11.4 ms | 133 / 267 MB |
| Galaxy S26 Ultra | 4.7 + 0.7 = 5.6 ms | 3.3 + 1.1 = 4.5 ms | 78 / 168 MB |

So the build is not what slowed the LG's raster — the present path is: the
same code rasterises ~45% slower (LG, S26) when frames go through
`ANativeWindow_lock`. Not understood yet; the leading suspect is memory-bus
frequency scaling (the raster is bandwidth-bound, and with no GPU work the
bus may clock down). The present itself was paying Android's copy-back,
which reads the uncached window buffer every frame; it now locks the whole
buffer and writes only what each buffer is missing (`BufferAges`). The CPU
present halves memory on every phone, so it stays the default while this is
measured.

**After the no-copy-back present** (2026-10-16 evening, reconciled main):

| phone | CPU present: raster + acquire + present = frame | Vulkan present | GPU path |
|---|---|---|---|
| LG Stylo 5 | 30.6 + 0.4 + 3.8 = 36.0 ms | 17.7 + 20.0 = 39.0 ms | 40.1 ms (GPU 32.5) |
| Galaxy S20 | 9.1 + 0.6 + 1.4 = **11.5 ms** (was 15.4) | 7.4 + 3.1 = 10.9 ms | 7.2 ms |
| Galaxy S26 Ultra | 4.9 + 0.8 + 0.2 = 6.0 ms | 3.3 + 1.1 = 4.5 ms | 2.2 ms |

The present itself is now cheap everywhere (S20: 7.1 -> 1.4 ms, with 0.6 ms
of waiting for a buffer). What remains is the raster running slower on the
CPU present path on every phone (LG 30.6 vs 17.7 ms) — still open, and now
the largest single item on the low-end phone.

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

## 5.2 The framework comparison is a survey and a source cross-check, not a conformance matrix

**What.** `What are the best UI frameworks.md` (repository root) compares
vieww against Motion Canvas, GSAP, Manim and Rive — animation tools, not UI
frameworks — and `docs/FRAMEWORK-CAPABILITY-PARITY.md` cross-checks that
survey's claims against the source, crate by crate, in honest
source-present / API-slice / needs-verification terms. Neither is the
answer to the question a UI-framework comparison exists for: on the
dimensions that define one — screen-reader support per platform, IME, bidi,
RTL layout, runtime locale switch, hot reload, devtools, golden and widget
tests, native backends, measured performance, shipped apps — how does vieww
stand against Flutter, React Native and Qt, cell by cell, every cell
checkable?

**Why not.** Populating the other frameworks' columns honestly is research
— each cell needs a version and a citation, not an impression — and a
matrix nobody enforces rots into the survey it replaced, so the gate is
part of the work, and gates get argued about before they get written.

**Blocks.** The honest "does vieww handle everything every other framework
does" answer. Every fix elsewhere in this file moves cells that nothing is
counting yet.

**Start.** Write `docs/conformance/matrix.md`: vieww's own row filled
strictly from this file and TRACKER (`desktop_gpu_measured` is now true —
the Kaby Lake receipts — but `sr_ios` is false, see 3.6), the other
frameworks' rows from their published documentation with versions named,
and `docs/conformance/why-not.md` giving the plan for every false cell. A
check that diffs vieww's row against the last committed one keeps a
regression visible instead of silent.

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
