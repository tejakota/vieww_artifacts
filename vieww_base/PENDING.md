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

**2026-10-18, the walk goes row-parallel and the general fill is
memoised** (commits f004294, 0502ae0; no machine receipt yet — this
container cannot produce one honestly, which is why this paragraph names
the measurement that would). Two of the "spread out" leftovers took their
hit: the gray scanline walk now splits a mask's rows across up to eight
`std::thread::scope` workers (size-gated at 128 rows/columns,
`VIEWW_RASTER_THREADS=0` the off switch) with the serial path kept as the
shared row function; and `paint_shape`'s general fill — icons, rounded
outlines, arbitrary vector shapes — is memoised in a content-addressed
`ResidencyCache` (`native/fill_cache.rs`, 8 MiB default,
`VIEWW_PATH_CACHE_MB`), the last uncached hot `rasterize` site. Identity
is not a claim here: `raster-identity.sh` reports all 30 gallery files
byte-identical, and the walk's serial-vs-parallel bit equality is tested
over a seeded polygon soup. What would credit the speed: a `fixtures
--runs 10 --receipt` pair with `VIEWW_RASTER_THREADS=0` and unset on the
reference laptop, and a callgrind count for glass with a warm fill cache.

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

## 4.3 The sixteen-fix "same pixels, faster" advisory, mapped to what actually exists

**What.** A third external advisory proposed sixteen performance fixes
under one hard constraint — *same pixels, faster; frame time is the only
quantity allowed to move* — against crates (`vieww-render`, `vieww-raster`,
`vieww-text`, `vieww-assets`) and mechanisms (SSAA-2×, a scanline-lerp
inner loop, 256×256 tile rasterisation) that mostly do not exist here.
This entry is the honest re-mapping onto the real codebase, so the next
session starts from the map instead of re-deriving it. Re-audited against
main at 0502ae0, 2026-10-18.

**Already closed before the advisory arrived** (each in TRACKER.md):
the blur-band cache keyed on the input band's bits (advisory Fix 6 —
landed as `effects::blur_cached` a month of advisories early); the
authoritative hybrid planner behind its RMSE gate (Fix 16 — machinery,
PENDING 2.3 owns the actuation); the pipeline cache across launches
(roughly half of Fix 15 — `pipeline_cache.rs`); background font-store
loading (`vieww-text`'s `system_fallback_in_background` — the loading
half of Fix 12).

**Landed by the re-mapping itself** (f004294, 0502ae0):
- Fix 10 (parallel rasterisation) → the honest form here is **row**-
  parallel, not tile-parallel: the scanline walk is row-wise, rows are
  independent, and row chunks keep byte identity structural. §4.1's
  dated paragraph.
- Fix 7 (tessellation cache) → the shadow/clip/glyph/shape caches already
  covered the named cases; the remainder was `paint_shape`'s general fill,
  now `fill_cache.rs`.
- Fix 9 (NEON scanline) → the doc's premise (a scalar lerp inner loop)
  does not match this rasterizer, but its target audience does: the aarch64
  gap was real, and it was in the compositing loops, not the scanline.
  `native/simd/neon.rs` is the NEON half, written and cross-type-checked,
  **not yet run on ARM** — its Start is below.

**Rejected under the advisory's own hard constraint** (each would move
pixels by design, and "perceptually identical" is not "byte-identical"):
Fix 3 (MSAA-4× instead of supersampling — also moot: there is no SSAA-2×
here to replace; the quality lever is `QualityProfile`'s resolution
scale), Fix 4 (narrowing the scene target's `R16G16B16A16_SFLOAT` or the
shadow/blur intermediates), Fix 5 (ASTC — also needs an encoder and an
Adreno-506 support check). If the pixel-equality policy ever changes,
these re-open with device diffs as the referee; until then they are not
gaps, they are decisions.

**Open, and why — with a Start each:**
- **Fix 1/2 (load/store ops and the subpass chain).** The audit: the three
  `vulkan/mod.rs` render passes are already correct (CLEAR on first use,
  STORE only where the content is consumed, depth CLEAR + DONT_CARE, all
  initial UNDEFINED); `scene.rs` builds LOAD and CLEAR pass variants and
  its all-STORE color attachment is the conservative choice for targets
  some pass later samples. Proving which intermediate targets nobody
  reads — the prerequisite for DONT_CARE and for folding shadow→blur into
  subpasses with input attachments — needs the render-graph's target
  lifecycle written down and then a real GPU run to validate the fold.
  Start: annotate each `scene.rs` target with its consumers (read or
  never-read), then `ci/check/`-style reasoning on paper before any device
  run; `vulkan/scene.rs`'s 2,433 lines are the reason this is not a
  one-session change.
- **Fix 8 (temporal tile reuse via `copy_texture_to_texture`).** The hal
  has image→buffer copies but no image→image copy, and the CPU side's
  equivalent already exists (`damage.rs` + `TargetPool`). GPU tile reuse
  is a `scene.rs` change with no in-container referee. Start: same
  target-consumer annotation as Fix 1/2; the reuse key is the fill
  cache's (`FillKey`) discipline carried to tiles.
- **Fix 12's remainder (async glyph rasterisation), Fix 13 (async image
  decode).** *Fix 13 done, Fix 12 closed by measurement, 2026-10-19 (§4.4).* `vieww-foundation`'s `Pool` is the worker infrastructure;
  the gap is that paint's glyph raster is synchronous inside the frame.
  A placeholder-first design changes visible pixels mid-load — it needs
  the fidelity policy's answer to "a frame with placeholders" written
  down before code. Start: `crates/vieww-image/src/codec/` decode
  off-thread first (decode is deterministic, so bytes are safe) with
  `Pool`, measuring an A36-like workload before the glyph half.
- **Fix 14 (FrameBudgetGuard).** *Done 2026-10-19 (§4.4): the census and the guard.* Was: buildable, deferred deliberately: the
  scheduler has budget accounting (`FrameReport::over_budget`) but no
  census of what work is "non-critical" — the type is easy, the deferral
  policy without callers is a stub. Start: list the frame's deferrable
  work (image decode, mip building, shadow re-render) with their phases
  in `scheduler.rs`, then the guard is a small type over that list.
- **Fix 15's remainder (splash-time full warm-up).** *Closed by audit 2026-10-19 (§4.4): nothing in the UI path is created lazily.* Pipelines are
  created at `SceneRenderer::new` through the persistent pipeline cache;
  what "warm every shader variant" adds is enumerating variants the
  first frames would compile lazily — a GPU-side claim needing a device
  to verify the cold-start delta. Start: instrument first-frame pipeline
  compiles on Device Farm (`Claude-Session` receipts already carry
  start-up stages), then decide whether the enumeration is worth its
  splash time.

**The NEON half's Start.** ~~Run `framework-checks.yml`'s Android-emulator
or iOS-simulator leg~~ — neither runs vieww-paint on ARM (the emulator is
x86_64; the simulator leg tests vieww-foundation and vieww-text only), and
until 2026-10-19 the workflow could not run at all (§4.4). Now: Actions →
Framework checks → Run workflow; the macOS leg's "NEON compositing loops,
bit for bit (aarch64)" step is the verdict. A Device Farm frame-time A/B
(scalar fallback vs NEON dispatch) on the LG Stylo 5 is still the number
the advisory wanted.

## 4.4 After Device Farm f91d07c: what the first run on the new code said, and what was done about it

**What the run said** (2026-10-19, main at d3bf2f6, three Android phones
and three iPhones; `ci-results/device-farm-main`):

- The CPU raster stage did not get faster after the row-parallel walk,
  the fill cache and NEON landed — LG Stylo 5 30.6 → 34.8 ms, S26 Ultra
  4.7–4.9 → 6.6 ms — and CPU-path PSS rose 10–16 MB (LG 76 → 92, S26
  78 → 88 MB). Different units of each phone than the earlier runs, so a
  reading, not a proof.
- The `gpuscale` pass on the Pixel 5a and S26 Ultra drew 0 of 600 frames
  on the GPU under a "GPU at 0.75x" label.
- `auto` kept the CPU on the iPhone 15 Pro (CPU 8.77, GPU 8.19, budget
  8.33 ms) and then failed the budget at 11.1 ms.
- The LG's GPU path is fill-bound, not bandwidth-bound as the sixteen-fix
  advisory assumed: at 0.5x, `gpu_wait` falls 34.6 → 12.4 ms.
- On iPhone, the GPU path's cost is `gpu_submit` (MoltenVK translation:
  iPhone 13 p95 24.7 ms submit vs 8.5 ms wait) — the strongest evidence
  yet for a native Metal `SceneRenderer` (§1.2).

**Done** (2026-10-19, this entry's patch; TRACKER has the evidence):

- `run-android.sh` sets `debug.vieww.gpu=1` *before* the `gpuscale`
  launch; `device_farm.py` flags any GPU-labelled pass the CPU drew, and
  `gpuscale_verdict.py` no longer counts one as the scale engaging
  (self-test fixture `cpu_drew_it`).
- `rasterizer_choice::after_gpu` takes the budget: the GPU also wins when
  it is the only one of the two inside it. The f91d07c phones are fixtures.
- A `cpubare` Device Farm pass (`debug.vieww.bare=1`: raster threads,
  fill cache and blur cache off) is the same-unit A/B for both the speed
  and the PSS readings above; every pass now prints its PSS.
- The blur and fill cache budgets follow the device tier
  (`quality::cache_budgets_for`: Low 2+2 MiB, Medium 4+4, High 8+8 as
  shipped); the env overrides still win. Pixels cannot move — both caches
  memoise a deterministic function.
- Fix 13: `vieww_image::codec::decode_async` — the audit found no decode
  inside a frame anywhere in the framework, so this is the app-facing
  half: same bytes as `decode`, on a `Spawn`.
- Fix 14: `frame_budget::FrameBudgetGuard` over a written census of the
  UI thread's non-drawing chores; the pipeline-cache save runs in a frame
  with slack or at a 500 ms deadline, and is flushed on surface drop.
- Fix 15: closed by audit — all three UI scene pipelines are built eagerly
  in `SceneRenderer::new` through the persistent cache (seeded ~39 KB on
  every phone in f91d07c); nothing lazy exists to warm. The four pipelines
  outside the UI path (clear, mesh, 3D, compute) now use the same cache.
- **`framework-checks.yml` moved to the repository root.** It had lived
  at `vieww_base/.github/workflows/`, which GitHub never reads, so on
  `main` none of its legs could run — including every "referee" this file
  names for written-not-run code. Manual trigger, like the root's other
  workflows; its macOS leg now runs the NEON bit-for-bit test on arm64.

**Still open, and why.** The next Device Farm run is the verdict on the
first four bullets (`cpubare` vs `cpu` per phone; `cache budgets` and PSS
per pass; the `gpuscale` rows drawing on the GPU; the iPhone 15 Pro's
`auto` choice). Fix 12's pre-warm: see §4.3's open list and this patch's
TRACKER entry for the measurement that decided it: closed by
measurement — outlining every printable ASCII glyph of the embedded face
from cold takes 0.11 ms in total (release, x86-64, 2026-10-19), so a
pre-warm would move ~2 ms once even on the LG; the per-size coverage
raster that remains is this frame's pixels and cannot be deferred without
a placeholder the fidelity policy forbids. The platform ports
(§1.2, §1.3, §3.1's iOS/Android halves, §3.6, §6.2) were not attempted:
the session had no cross targets (rustup could not download them), so
the code could not even be type-checked — below this repository's
"written, type-checked, labelled" bar.

## 4.5 After Device Farm 39382602099: the A/B answered, and what is left is policy

**What the run said** (2026-10-20, main at 1c30b7a; Stylo 5, A73 5G, S26
Ultra). It FAILED on the two gating passes: Stylo 5 CPU and GPU, A73 CPU.

**Done** (this entry's patch):

- *Harness race.* The A73 and Stylo 5 `auto` passes drew 600/600 frames on
  the CPU: `device-suite.sh --no-build` reset `debug.vieww.gpu` to 0 while
  the app was starting. It now leaves the property to the launcher; the app
  reads it first thing; `run-android.sh` fails the run when a pass's
  `rasterizer:` observation does not match its mode.
- *Caches.* The same-unit A/B: the path-fill cache cost frame time on all
  three phones (−1.89 / −6.62 / −0.73 ms median with it off) and 13–15 MB
  PSS; the blur cache cost on Low/Medium, paid on High. Fill cache off by
  default everywhere, blur cache on High/desktop only, a zero budget a real
  bypass. The row-parallel walk stays (+2.68 ms without it on the Stylo 5).
- *Tier line.* `MEDIUM_MAX_KB` 7 → 8 GiB: the A73 (7,477,188 kB) was High.
  The S20 (8 GB) moves to Medium with it.
- *Stall check.* `no_frame_stalled` judges against max(4 × budget,
  3 × median), so a slow phone's normal spread is not a second failure; the
  A73's 5.7-median stall still fails.

**Still open — decisions, not code** (none of these is implemented; each
needs a choice or a phone):

- *The budget follows the panel's maximum refresh* (RC4). **Decided and
  implemented in §4.6**: the rate adapts (panel or a steady 60 Hz), and
  the farm gates on `auto`.
- *The LG Stylo 5 does not meet 16.7 ms on any path today* (RC5): CPU bare
  35.9, CPU 38.3, GPU 40.4 (`gpu_wait` 32.5), GPU at 0.5× 18.0 ms. This
  entry first called it a hardware limit; **it is not shown to be one** —
  see §4.6: the GPU path was repainting every pixel while 8.6% changed.
  Decided: 60 Hz at full resolution stays the target; no 30 fps tier, no
  observe-only. Keep CPU present there either way (`cpuvk`: Vulkan present
  19.1 ms vs 2.4).
- *Cold-core stalls* (RC6, engine half). The worst frames land on cores
  that drew 1–3 frames (A73 cpu5 125.6 ms, cpu4 108 ms). Pinning the raster
  thread's affinity, or a bounded spin to keep the big core warm, needs a
  phone to show it helps and does not cost battery; APerformanceHint is
  absent on API 28 and was declined on the A73. Not attempted blind.
- *A fill cache that could earn its keep*: shared masks instead of clones,
  a size cap, admission only for keys seen twice. Only worth building
  against a workload that repeats geometry; the farm demo does not.

## 4.6 After Device Farm 39402544760: redraw what changed, hold a rate you can

**What the run said** (2026-10-20, main at 1495c40, the §4.5 patch). Every
§4.5 fix showed: the A73's `auto` calibrated to the GPU and ran a 7.76 ms
median; it reported tier Medium; CPU PSS fell to 77/84/76 MB; the A73's
CPU median fell ~23 → 16.6 ms. Two findings changed the picture:

- **The GPU path repainted the whole screen every frame.** The Stylo 5's
  frame log: 0.086 full repaints per frame — 8.6% of the screen changing —
  while `present_planned` ignored damage. Its GPU is fill-bound (0.5× cut
  `gpu_wait` 32 → 12 ms), so it was paying for ~11× the pixels that
  changed. RC5 is a design gap first, not a hardware limit.
- **A median inside budget is not smooth.** The A73's shipping path:
  median 7.7 ms, 52 of 240 frames over 8.33 ms — visible stutter at 120 Hz.

Also: the CPU path redraws only damage, yet the Stylo's raster stage is
30 ms for 8.6% of the screen — ~10× what a full repaint at that rate would
predict; most of that cost does not scale with what changed. And
calibration underestimates the CPU's steady cost by 34–38% (Stylo 22.1 vs
35.7 ms, A73 11.0 vs 16.6); it changed no choice this run.

**Done** (this entry's patch; byte-exact and end-to-end evidence in the
commit messages):

- *GPU damage.* `SceneRenderer` keeps `L[0]` and redraws only a frame's
  damage: writes into the frame target are clipped; layers, blurs and
  shadow work run in full and are skipped when their composite misses the
  damage; a backdrop filter reaching outside the damage, a new size or
  clear colour, or any non-GPU frame in between redraws in full.
  `vulkan_damage` (lavapipe) holds damaged frames byte-identical to full
  ones; `wait_loop`'s `gpu_damage` measures 0.05 repaints per frame on a
  blinking square (1.00 with damage disabled).
- *Adaptive rate* (`vieww_platform_winit` `cadence`). Panel rate while ≤5%
  of frames miss it; a steady 60 Hz otherwise (`ANativeWindow_setFrameRate`
  on Android 11+, the display link on iOS, a fixed pacing grid everywhere);
  back up after three clean windows with 25% headroom; the first second
  (start-up) is not evidence. Resolution is never
  traded for rate. `VIEWW_CADENCE=panel` / `debug.vieww.cadence=panel`.
- *Gating.* `auto` and `cpu` gate (the CPU judged at its adapted rate — the
  fallback's bar), `gpu` where `auto` chose it. The suite adds
  `frames_inside_their_budget` (≤ 1 in 20). `known-failures.txt` names the
  Stylo 5 (RC5) and the A73's CPU fallback with their reasons; anything
  else fails the run, and a listed entry that passes is flagged stale.
- *Row-parallel walk.* Serial by default on Medium and High phones; kept on
  Low phones (it won on the Stylo in both runs) and on desktops
  (`quality::raster_threads_for` has the numbers).

**Still open:**

- The next Device Farm run is the verdict on the GPU damage path (Stylo 5
  `gpu`/`gpuscale` `gpu_wait`, and whether `auto` now picks the GPU there)
  and on the adapted rate (each pass's `frame rate:` line; whether the
  compositor honoured 60 Hz — `adb shell dumpsys display` shows the mode).
- The CPU path's cost per damaged pixel (above): needs a profile of a
  Stylo 5 frame, not a guess.
- Calibration's CPU underestimate: measure which frames the calibration
  launch times against the steady state.
- Rendering the GPU frame straight into the swapchain image, without the
  host round trip (the module docs' follow-up).
- Cold-core stalls (RC6) and the ideas in §4.5's list, unchanged.

## 4.7 After Device Farm 39538891917: overlap the CPU and the GPU, and measure the GPU

**What the run said** (2026-10-21, main at 8e6996a). The run passed: the
Stylo 5 and the A73's CPU fallback as known failures, the S26's CPU entry
flagged stale. The GPU damage path took the Stylo 5's GPU frame from
40 ms to 17.3 ms — over a 16.7 ms budget by 0.6 ms, with 11.9 ms of it the
CPU blocked on the GPU (`gpu_wait`). `SceneRenderer` waited for every
frame's fence before returning, so the application's ~5 ms and the GPU's
~12 ms ran one after the other.

Read against the cross-framework research (Flutter's UI/raster threads,
Qt's render thread, Lynx's background thread, Impeller's precompiled
pipelines, Qt's batching, Uno's partial repaint, Flutter's dynamic
resolution): vieww already has every technique on that list except two —
CPU/GPU overlap, and reusing a layer's *pixels* across frames
(`RepaintBoundary` keeps a subtree's recording, not its raster). Which of
the two the Stylo needs could not be read from one `gpu_wait` number.

**Done** (this entry's patch; evidence in the commit messages):

- *GPU time by kind of work.* Timestamp queries split each GPU frame into
  draw, layers, filters, shadows and blit (`GpuPassTimes`); the device
  suite reports `gpu_passes`, or that the queue records no timestamps.
- *Input to present.* `latency.rs`'s log is wired per window (presses,
  touches, wheel, key presses, IME text; not hover, releases or IME
  enable/disable); an input belongs to the first frame whose build starts
  after it; an input nothing answers within 1 s is counted apart. The
  suite reports `input_latency`, observed and not gated.
- *Frames in flight.* A GPU frame into the swapchain is submitted and left
  in flight; the present waits on a per-image semaphore instead of the CPU
  on a fence; the renderer settles it before touching any resource, and
  the platform layer before any acquire or CPU present. One frame in
  flight: the application's work overlaps the GPU's, two frames' GPU work
  never does. `VIEWW_GPU_OVERLAP=0` / `debug.vieww.overlap=0` turns it off;
  Device Farm's new `gpuserial` pass is that, beside `gpu`.
- *Found by running everything under Vulkan's synchronisation validation*
  (now the way to check this renderer): readback copies unordered after
  the render passes writing them (three offscreen passes), and
  `present_pixels`' layout transition unordered after the acquire — both
  predating this patch, both fixed; and, in this patch's own first draft,
  an abandoned image presented on a semaphore nothing signalled — a hang
  on a resize — fixed before it shipped.

**What lavapipe cannot show.** Its X11 present blocks until the frame is
rendered, so the GPU's time moves from `gpu_wait` into `present` and the
frame gets no faster there (`wait_loop`'s `gpu_overlap` prints the
breakdown). Android's WSI hands the semaphore to the compositor. The
`gpu` against `gpuserial` passes are the measurement.

**Still open:**

- The next run's numbers: the Stylo 5's `gpu` frame time against
  `gpuserial`, and its `gpu_passes`. Filters, shadows and layers
  dominating says reuse layer pixels next; plain draw dominating says
  overdraw.
- Two frames' resources (separate command buffers, uniform, vertex and
  index buffers) would also overlap recording with the GPU; only if one
  frame in flight falls short.
- Input latency from the platform's own event time (Android's
  `MotionEvent.getEventTime`), and to scan-out rather than present.

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
