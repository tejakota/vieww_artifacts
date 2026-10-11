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
rect family and with the CPU's own coverage for every other shape, a GPU frame is rendered straight into a real window's
swapchain image ([`App::prefer_gpu`]), and the glyph atlas evicts between frames — all closed
with evidence in `TRACKER.md`; the full picture is
[`docs/GPU-RENDERER-STATUS.md`](./docs/GPU-RENDERER-STATUS.md). What remains:

## 2.1 A general path that changes every frame pays CPU coverage every frame

**What.** Non-analytic shapes — rotated rects, arbitrary paths, every stroke —
are now antialiased on the GPU with the CPU rasterizer's own coverage
(`GpuSeam::shape_coverage`, cached in the mask atlas by content; see
`TRACKER.md` 2026-10-11). A shape that is the same next frame costs nothing
again. One that *changes* every frame — a morphing path, a stroke being drawn,
anything rotating — is re-rasterised on the CPU each frame, which is the cost
the GPU path exists to avoid. Shapes whose patch exceeds ~1.4 Mpx fall back to
hard-edged tessellation rather than claim the atlas.

**Why not.** Parity first, then speed: matching the CPU's edges exactly is
what made the census's interior verdict fall from 6 fixtures to 2, and an
MSAA path would differ from the CPU on every edge again.

**Blocks.** Smooth animation of large arbitrary paths on the GPU path.

**Start.** Measure first — `00-curves`/`00-strokes` in the census are the
proxies, and on lavapipe the coverage cost is invisible under the readback.
If it matters on hardware, 4× MSAA on the tessellated material *for shapes
that changed this frame* is the complementary path; the tessellator and
`vulkan_mesh_smoke.rs` already exist for it.

## 2.2 Backdrop blur: GPU and CPU disagree at the layer's edge on Mesa 25.2

**What.** `vulkan_compositor::a_backdrop_blur_blurs_what_is_behind_and_keeps_the_foreground_sharp`
fails on lavapipe from Mesa 25.2.8: 816 pixels differ by more than 2,
worst 34, all along the edge of the blurred region (e.g. `(31, 16)`:
GPU `[143, 162, 215]`, CPU `[177, 186, 186]`). The other 40 Vulkan tests
pass, including the plain blurred layer.

**Why not.** Found on 2026-10-08 during the fifth gap pass, on a Mesa
newer than the one the suite was last certified on. The GPU seeds the
backdrop copy with the layer's rect and blurs inside it (edge samples
clamp to the copy), while the CPU reference blurs a buffer whose edge
treatment differs; which one is right is a design decision (CSS
`backdrop-filter` duplicates edge pixels) before it is a fix.

**Blocks.** Nothing on the CPU path. GPU parity for frosted panels.

**Start.** `cargo test -p vieww-hal --features vulkan -- --ignored --test-threads=1 backdrop`,
then decide the edge mode in `vieww-paint/src/native/reference.rs`'s
backdrop seeding and `vieww-hal/src/vulkan/scene.rs`'s `PushLayer` branch
together, so the two agree by construction.

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
