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

## 1.2 macOS — the Metal backend

**What.** `crates/vieww-hal/src/metal.rs` brings a device and a command queue
up, and stops there. `render_clear_to_pixels` and `render_mesh_to_pixels` are
not ported, and neither is `SceneRenderer`.

**Why not.** No macOS Rust target and no Xcode here, so not one line of it can
be type-checked, let alone run. The module's own doc explains at length why an
unverified 150-line pipeline port was not written blind, and that reasoning
still holds. The `framework-checks` workflow compiles the workspace on a macOS
runner every push; that is the compile half, and it is new — the port itself
still needs a person on the machine.

**Blocks.** macOS and iOS entirely — they share this backend.

**Start.** `ci/check/platform-check.sh macos` on a Mac. Expect the compile to have
things to say; that is the point. Then port
`vulkan::VulkanDevice::render_clear_to_pixels` onto the calls
`src/metal.rs`'s doc lists (they were checked against the real `metal` 0.31
source, so the port is translation rather than discovery), translating WGSL with
`naga::back::msl::Writer` instead of the SPIR-V writer.

## 1.3 Windows — the D3D12 backend

**What.** `crates/vieww-hal/src/d3d12.rs` does adapter and device bring-up and
one command queue. No render pipeline.

**Why not.** As above: no Windows target, no MSVC here — though the
`framework-checks` workflow now compiles the workspace on a Windows runner
every push, which is the compile half of this entry.

**Blocks.** Windows GPU rendering. Note that Windows also has a *separate*
unsolved problem — see 5.3.

**Start.** `ci/check/platform-check.sh windows` on a Windows machine with the MSVC
toolchain.

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

## 1.5 Android

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

---

# 2. GPU renderer coverage

`vieww_gpu::ScenePlan::is_complete` is the gate. As of 2026-10-09 every
`Command` plans on the GPU, geometry edges are antialiased analytically for the
rect family, a GPU frame can be presented to a real window
([`App::prefer_gpu`]), and the glyph atlas evicts between frames — all closed
with evidence in `TRACKER.md`; the full picture is
[`docs/GPU-RENDERER-STATUS.md`](./docs/GPU-RENDERER-STATUS.md). What remains:

## 2.1 General-path edges are still tessellated without AA

**What.** Fills and strokes that are not rects or rounded rects — arbitrary
paths, rotated rectangles — rasterize through the tessellated material, which
has no edge coverage. The analytic SDF material covers the rect family because
that is the overwhelming majority of UI geometry; a rotated card still has a
jagged edge next to the CPU renderer's.

**Why not.** The analytic path is per-shape: a distance function per geometry
family. Rects and rounded rects share one; a general path would need either
MSAA on the tessellated material or a per-pixel coverage buffer, and the two
have different costs and different seams with the compositor.

**Blocks.** Visual parity with the CPU renderer for arbitrary geometry — the
census still records edge-band differences on rotated/curved geometry, and it
now records *only* those.

**Start.** `crates/vieww-gpu/src/scene.rs`'s `emit_shape_painted` (where the
rect family routes to the analytic material) and
`crates/vieww-shaders/shaders/scene.wgsl`'s `shape_distance`. The two honest
options are 4× MSAA on the tessellated material with a resolve per target, or
a coverage-R8 pass like the clip masks already use. Measure with
`cargo run --release -p fixtures -- <outdir> --census`.

## 2.2 The GPU windowed path reads back through host memory

**What.** `NativeRenderer::present_planned` executes a complete plan with
`SceneRenderer`, reads the pixels back into a host buffer, and presents that
through the swapchain exactly as the CPU rasterizer's buffer would be. Every
GPU frame pays a full-frame readback plus a full-frame upload that a
render-to-swapchain-image path would not.

**Why not.** The scene pipeline renders into its own framebuffer, not a
`VK_KHR_swapchain` image: targeting the presentable image means importing it,
matching its format, and handling the acquire/release barriers — its own
design step, taken deliberately *after* the correctness join landed rather
than together with it.

**Blocks.** The GPU windowed path being *fast*, rather than correct. This is
the follow-up §2.6 named when it was closed, kept here so it is not mistaken
for done.

**Start.** `crates/vieww-hal/src/vulkan/scene.rs`'s `SceneRenderer::render_planned`
(the framebuffer the plan renders into) and `vulkan/swapchain.rs`'s image —
the barriers are the work; the shader pipeline does not change.

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

## 3.2 The display refresh rate is not read from the platform

**What.** `winit`'s `refresh_rate_millihertz` returns `None` unconditionally on
this backend — the framework carries a `FIXME` quoting that. `App::refresh_rate`
lets an application state it instead.

**Why not.** Upstream.

**Blocks.** Correct frame budgeting on a 120 Hz display without the application
saying so. Mis-states the jank count and nothing else.

**Start.** `crates/vieww-platform-winit/src/app.rs` and `insets.rs`.

## 3.3 viewwstudio's panic boundary cannot work on Windows

**What.** The studio `dlopen`s a preview `cdylib` and catches panics from it.
That requires host and guest to share one `libstd`, which `.cargo/config.toml`
arranges with `-C prefer-dynamic` — **and the MSVC toolchain ships no dynamic
std**, so the flag is deliberately not set there.

**Why not.** Not fixable with a flag. `.cargo/config.toml` says so, at length,
and names the real answer: an out-of-process preview.

**Blocks.** A Windows studio. A guest panic there takes the window and the
unsaved buffer with it.

**Start.** Design the out-of-process preview. This is the largest single item
in this file that is *not* GPU work, and it is worth doing for every platform,
not only Windows — an in-process guest is a hazard everywhere and merely a
survivable one on Linux and macOS.

---

# 4. CPU rasterizer performance

## 4.1 Two fixtures are over a 60 Hz full-repaint budget

**What.** `00-layers-nested` (~25 ms) and `23-editor-glass` (~22 ms) against a
16.7 ms budget, at 1366×679.

**Why not closed.** It is measured, understood and bounded rather than
neglected: `TRACKER.md`'s rendering-performance section documents a 1.28×
whole-gallery improvement, the four optimisations that were tried and
*removed* for making things worse, and the fact that a **retained** repaint of
the same screen is 9.9 ms and byte-identical — which is what a live window
actually does. The full-repaint number is the right alarm for a first frame, a
resize and a theme change.

**Blocks.** Resize smoothness on the heaviest screens.

**Start.** Explicit SIMD in the compositing loops is the one remaining large
lever; the frame is roughly 200 M instructions for ~4 M blended pixels and the
cost is spread across compositing (22%), output conversion (16%), layer resolve
(8%) and blur (7.5%) rather than concentrated. Read the "four optimisations
that were tried, measured, and removed" section first — the machine's
run-to-run spread is wider than most of these effects, and `callgrind`
instruction counts are the only stable signal.

---

# 5. Process and infrastructure

## 5.1 The coverage number is produced, not watched

**What.** `ci/check/coverage.sh` has now produced a number (2026-10-09, on the
pinned toolchain — see `TRACKER.md`), and the `framework-checks` workflow runs
it on every push with `continue-on-error: true`. It is evidence, not yet a
gate that blocks a merge: the floor (70% of lines) held on the machine that
produced the first number, but one number on one machine is not a baseline
anyone should be fired over.

**Why not.** Turning it into a hard gate is a decision about the project's
risk appetite, made better after the workflow has produced numbers on a few
dozen runs.

**Blocks.** Nothing today; a slow regression in test coverage would land
quietly.

**Start.** Remove `continue-on-error` from the coverage job in
`.github/workflows/framework-checks.yml` once its numbers have a history, and
keep the "missing tool is not a failure" shape the web-sys audit stage has.

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
