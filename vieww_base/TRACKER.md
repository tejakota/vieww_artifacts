# ViewW status tracker

This file is the honest, current answer to "what actually works, and how do I
check that" across the workspace.

**What is *not* done lives in [`PENDING.md`](./PENDING.md)**, and nothing
belongs on both lists. That split is new. This file used to end with a "What
genuinely remains" section, which meant the record of what had been verified
and the worklist of what had not were the same document — so the worklist was
buried under a history that only grows, and every gap had to be rediscovered by
reading to the bottom. The section is still here for continuity, and
`PENDING.md` is the one to read if the question is "what should I work on".

## 2026-10-18: the sixteen-fix performance advisory, re-mapped — a row-parallel scanline walk, a fill cache for the last uncached hot rasterize site, and the compositing loops' NEON half (written, type-checked, labelled not-yet-run)

**Context.** A third advisory proposed sixteen fixes under one hard
constraint — *same pixels, faster; frame time is the only quantity
allowed to move* — written against crates and mechanisms this workspace
does not have (`vieww-raster`, SSAA-2×, a scalar-lerp scanline inner
loop, 256×256 tile rasterisation). Before landing anything, every item
was mapped onto the code that actually exists; the map, with each fix's
verdict (already closed / landed / hardware-gated / rejected under the
byte-identity constraint) is **PENDING §4.3**, and it is the entry to
read first. This record covers the three commits the mapping turned into.

**What was built, and how each claim is checked.**

- **The scanline walk takes rows apart** (f004294). Rows of a coverage
  mask are independent, so `rasterize_general` splits them across up to
  eight `std::thread::scope` workers — one contiguous row block each
  (`split_at_mut` making the disjointness provable), per-worker scratch,
  no shared mutable state. A chunk inherits the admitted-edge prefix the
  serial walk would have built by its first row (`partition_point` over
  the ymin-sorted order), so the `active` sequence — and through the
  stable sort, the crossings order — is identical to the serial walk's;
  the serial path *is* the shared `rasterize_row_range`, not a second
  implementation. A size gate refuses the spawn below 128 rows or 128
  columns and caps at eight workers (`VIEWW_RASTER_THREADS=0` is the
  serial off switch; unparseable fails safe to serial), because threading
  a glyph-sized mask measures the spawn, not the raster. The LCD walk
  stays serial on the same reasoning, written where it is decided.
  **Identity, structural then tested:** serial-vs-parallel bit equality
  over a seeded polygon soup at 2/4/7/64 requested workers, a
  chunk-spanning edge case, and the entry point against the serial walk
  on whatever machine runs it.
- **The general path fill runs once** (same commit). Clip masks, shadow
  patches, glyph outlines, blur bands and shaping each grew a cache when
  a measurement named them; `paint_shape`'s `rasterize` — every
  non-rectangular fill, every repaint — was the one hot site left.
  `native/fill_cache.rs` keys a `ResidencyCache` on an FNV-1a over every
  point bit, the closed flags, the count and the clip's four words: a hit
  is by construction the bytes the walk would have produced — the oracle
  is memoised, never reimplemented. Default 8 MiB,
  `VIEWW_PATH_CACHE_MB` to move it, `0` a true bypass that skips even the
  hash. The rectangle fast path still runs first, so a four-point panel
  never pays the hash. Stats sit beside the blur cache's
  (`NativeRenderer::fill_cache_stats`); six tests cover hit/miss,
  geometry-bit and clip sensitivity, eviction, bypass and the empty case.
- **The compositing loops speak NEON** (0502ae0). The SSE2 half's two
  loops (`composite_flat`'s and `composite_layer`'s normal-blend rows)
  now have an aarch64 mirror, `native/simd/neon.rs`: lane for lane the
  same arithmetic, never fused; compares that answer false on `NaN` where
  the scalar ones do; `vbslq_f32` as the select, which is literally the
  SSE2 `select`'s definition. Dispatch is by `target_arch` — NEON is the
  aarch64 baseline the way SSE2 is the x86-64 one. **Written and
  type-checked, not yet run:** it type-checks for
  `aarch64-unknown-linux-gnu` (lib and tests) on the pinned 1.98.1, the
  cross-check gate's `aarch64-apple-ios` / `aarch64-linux-android` legs
  keep it compiling, and `simd_rows_match_scalar_bit_for_bit` — which
  dispatches through the same entry points — is the ARM referee the first
  emulator or simulator run will supply. The module says all of this
  about itself.

**Full-suite evidence.** 269 lib tests with `--features native` (11 new);
clippy `-D warnings` clean on the pinned 1.98.1; `cargo fmt` clean on
every file this entry touches (pre-existing drift elsewhere is not this
entry's to churn); reverse dependencies (`vieww-devtools`,
`vieww-element`, `vieww-render`, `vieww-widget`, `vieww-runtime`,
`vieww-gpu`) compile `--all-targets`; and — the gate this entry exists to
cite — **`ci/check/raster-identity.sh` reports all 30 gallery files
byte-identical to origin/main**, run at each of the two commits' trees.
What is *not* claimed: any frame-time number. This container cannot
produce a receipt honestly; PENDING §4.1's dated paragraph names the
`fixtures --receipt` pair that would credit the parallelism and the
cache.

**What the advisory's other fixes got, and where they live.** Not
silently dropped: PENDING §4.3 records the four already closed (blur
cache, planner machinery, pipeline cache, background font loading), the
three rejected under the advisory's own hard constraint (MSAA, narrower
intermediates, ASTC — "perceptually identical" is not "byte-identical",
and the pixel-equality policy is a decision, not an oversight), and the
open ones with a Start each — the load/store / subpass fold (needs a
target-consumer audit and a GPU), GPU tile reuse (same prerequisite),
async decode and glyph raster (needs the fidelity policy's answer to
placeholder frames first), the budget guard (needs a deferrable-work
census), and the splash warm-up's remainder (needs a device).

## 2026-10-18: the nine-fix advisory's buildable half — a byte-budgeted pool, a blur cache, a planner that earns its decisions, a conformance matrix with a gate, and two platform bridges

**Context.** A second external advisory named nine fixes (P0: native Metal
and D3D12 scene renderers, desktop GPU frame timing, memory under 40 MB, an
authoritative hybrid planner; P1: embedded native views and drag-out FFI;
P2: iOS VoiceOver and a framework conformance matrix). The re-audit against
main found one partial and eight open (commit a21d267's scoreboard). This
entry records the four commits that closed or advanced every item this
container could honestly close — and, just as much, what they do not claim.

**What was built, and how each claim is checked.**

- **The layer pool answers to bytes** (commit 2b03351). `TargetPool`'s
  idle set is capped at 32 MiB by default (`VIEWW_POOL_IDLE_MB`), evicting
  least-recently-released first, admitting the newcomer always, keeping an
  oversized one-off per `residency.rs`'s softness rule; `PoolStats.evicted`
  replaced `dropped` (json_export follows). Nine pool tests, including the
  LRU order, the softness case and the env parser.
- **An identical blur runs once** (same commit). `effects::blur_cached`
  keys the six-pass kernel's finished band on the input band's exact bits
  (FNV-1a over `f32::to_bits`, plus w/band_h/radius/direction — radius,
  not sigma: the kernel never sees the sigma) in the same budgeted LRU the
  glyph outlines use (8 MiB default, `VIEWW_BLUR_CACHE_MB`, 0 disables).
  `blur()` itself is unchanged and remains the oracle. Nine cache tests,
  including byte-identity against the uncached kernel for both the
  separable and directed paths. **Rendered evidence:** the `00-blurs` and
  `23-editor-glass` fixtures produce byte-identical PNGs with the cache on
  and `VIEWW_BLUR_CACHE_MB=0` — both blur call sites (plain pop-time and
  push-time backdrop) exercised.
- **The planner earns the right to decide, and then decides** (a6b2980).
  `CostHint::cpu_complexity` prices strokes 2.7x / curves 2.35x / shadows
  2.0x a plain fill — the gallery shadow log's measured band, with the
  within-band attribution written down as an assumption; `Calibration`
  grew the per-frame RMSE beside the ratio-of-sums; and `ShadowPlanner`
  runs a mode machine — Shadow → ShadowCalibrated (plan on the calibrated
  profile, fresh gate) → Authoritative — gated at RMSE ≤ 5% over 1,000+
  frames (the advisory's numbers), opt-in via
  `VIEWW_PLANNER_AUTHORITATIVE=1` beside `VIEWW_PLANNER_SHADOW`,
  actuating whole-frame CPU recommendations only, `is_complete` staying
  the per-frame fallback, `Mixed` unactionable, resolution-scaled frames
  never planned for a decision. Five new planner tests (42 total).
  **Not claimed:** any real frame through the gate — PENDING 2.3 records
  the exact run that would.
- **The framework comparison becomes a matrix with a gate** (a0eab3e,
  77a55ad). `docs/conformance/matrix.md`: fourteen dimensions against
  Flutter 3.47.7, React Native 0.87 and Qt 6.12 LTS, every vieww cell
  cited to its PENDING/TRACKER section, every other cell versioned and
  sourced; `why-not.md` carries a plan for each of the eight false/partial
  cells. `ci/check/conformance-matrix.sh` diffs vieww's machine-readable
  row against the merge base (a `Matrix-Changed:` trailer, the fidelity
  gate's own protocol, covers deliberate flips) and demands a plan per
  non-true cell; wired into `checks.sh` before the slow stages. All three
  gate paths exercised against the real base: unchanged passes, silent
  flip fails with the exact cell diff, trailer reports without failing.
  vieww's row: 4 true / 2 partial / 8 false — and the file says what that
  reading is.
- **Two platform bridges, both written-not-run and labelled so**
  (4f4f77e). iOS: `ios_accessibility.rs` publishes the semantics tree as
  `UIAccessibilityElement`s (label, traits, frame, packed id, container
  setup, `screenChanged` with the focused element, VoiceOver-gated) —
  the arrow AccessKit does not draw; type-checks on `aarch64-apple-ios`,
  selectors verified against the fetched `objc2-ui-kit` 0.2.2 source;
  the XCUITest set and `a11y-ios.sh` are checked in and equally unrun.
  Windows: `drag_out_windows.rs` implements `PlatformDragStarter` over
  OLE `DoDragDrop` (shell data object, a `CF_HDROP` built by the pure
  `hdrop_bytes` — byte-layout unit-tested on every machine — a
  two-decision drop source, the effect mapping); type-checks on
  `x86_64-pc-windows-msvc`, and its `[session-verifies]` list names
  exactly what a first Windows run confirms. PENDING 3.1 and 3.6 record
  both, their evidence, and the next real observation each needs.

**Full-suite evidence for this entry:** `vieww-paint --features native`
258 lib tests, `vieww-render-planner` 42, `vieww-scene` 7,
`vieww-render-graph` 12, `vieww-platform-winit` 146, `vieww-devtools` 62,
`vieww-interaction` 31; all thirteen reverse-dependency crates of the
touched ones compile clean (`cargo check --all-targets`); the two
fixtures' byte-identity diff above. The workspace-wide test stage did not
run to completion in this container — disk exhaustion during example
linking — so the crates-not-listed evidence is the compile check, and
saying so is the point of this paragraph.

**What this does not claim, per the advisory's nine fixes:**

- **Fix 1/2 (Metal/D3D12 `SceneRenderer`)** — not started, and the size
  is now written down rather than gestured at: `vulkan/scene.rs` is 2,433
  lines with nine public entry points, and the Metal/D3D12 smoke halves
  (`metal.rs` 467, `d3d12.rs` 885 lines) have no scene-shaped code to
  extend. PENDING 1.2/1.3 remain the owners; the port is a multi-session
  effort that needs macOS/Windows sessions to verify against.
- **Fix 3 (desktop GPU timing)** — the Kaby Lake receipts stand
  (GPU 2.0 ms vs CPU 2.6/3.7 ms); the remaining gaps are the native
  timings behind Fix 1/2 and per-plate GPU p95, both needing hardware.
- **Fix 4's headline (<40 MB)** — the two buildable pieces landed; the
  number itself stays measurement-gated (heaptrack, PENDING 4.2).
- **Fix 5's actuation** — machinery landed, gate unfed; PENDING 2.3.
- **Fix 6 (native views)** — deliberately untouched: a second consumer is
  the design input PENDING 6.2 waits for, and this container cannot be it.
- **Fix 7/8** — bridges exist, sessions pending (above).
- **Fix 9** — closed as far as a repository can close it: the matrix, the
  gate and the plans exist; the cells move when the work moves.

## 2026-10-18: device tiers, quality profiles, a resolution scale, and the decision procedure rather than the decision

**Context.** The Device Farm's 2026-10-17 runs put the same build on phones
whose right answers are opposite — the LG Stylo 5 misses 60 Hz on both paths
(CPU 36.2 ms, GPU 40.1 ms, budget 16.7) while the S26 Ultra's GPU path runs
at 2.2 ms — and `App::auto_rasterizer` measured that one choice per device
while every other quality-shaped decision had no device-shaped input at all.
Six commits give the input, the lever, the measurement, the report, the
decision rule and the gate; none of them changes a default pixel anywhere.

**What was built.**

- **`device_tier`** (6a95365) — RAM, core count, SoC and API level, read from
  system properties at process start without loading a Vulkan driver; three
  tiers (3/7 GB RAM boundaries plus a cores≤4 catch for big-RAM, few-core
  parts), `VIEWW_DEVICE_TIER` overriding for tests and for arguing with the
  policy.
- **`quality.rs` and the hal's scale** (1e6063b) — `QualityProfile::for_tier`
  maps a tier to render decisions, one field today: `resolution_scale` (Low
  0.5, Medium 0.75, High 1.0), because one field is what the render path can
  read without guessing at effects the planner cannot yet skip.
  `VIEWW_GPU_SCALE` and `debug.vieww.scale=auto` drive it: planned GPU scenes
  render into targets at the scaled extent and the final blit enlarges
  (linear filtering required, and checked before it is assumed); the CPU
  path never scales — its bytes are the fidelity reference — and a frame
  that falls back to the CPU mid-flight re-lifts to full resolution.
- **The farm's gpuscale pass** (3920c78) — `run-android.sh`'s sixth pass runs
  the GPU path with `debug.vieww.scale=auto`; `examples/android.rs` forwards
  the system property as the environment the tier probe reads;
  `device_farm.py` collects `device` and `gpuscale_frame_time` beside the
  other passes' keys.
- **The quality overlay** (9b556fd) — `vieww-devtools`' `quality_overlay`
  reports the tier policy and the frame clock (average/p95/max/over-budget,
  read from the `FrameTimeline` the caller already keeps) as one summary
  line and three HUD lines, observed by the device suite beside the other
  keys and copied into the farm summary. A report, not a widget: there is no
  egui in this tree, and overlay pixels drawn into the app's scene would be
  CPU-path pixels FIDELITY.md does not own. `vieww-platform-winit` gains
  `vieww-devtools` as a dev-dependency — nothing that ships reads a
  devtools report.
- **The verdict** (f8316d7) — `ci/mobile/gpuscale_verdict.py` is
  `quality.rs`'s opt-in deal applied rather than read: a device is a
  candidate only when the best of its CPU and GPU medians misses the pass's
  own budget; the scale earns the promotion only when it engaged below 1.0
  (1.0 is off by another name) and landed at or under that budget; a tier is
  promoted only when every device of it in the run was a candidate and
  passed; anything else is keep-opt-in or an honestly-incomplete run. It
  emits `gpuscale-verdict.json` and the SUMMARY.md memo;
  `device_farm.py` appends the memo when the pass produced data. Its
  self-test fixtures are labelled SYNTHETIC.
- **The fidelity gate, wired** (this commit) — `ci/check/checks.sh` now runs
  `ci/check/raster-identity.sh` as the first slow stage: against the merge
  base with `origin/main` (not the tip, which would report somebody else's
  commits as your pixel changes), honouring the `Pixels-Changed:` trailer
  rather than failing on a documented decision, and skipping loudly on a
  shallow clone that cannot name a BASE. The gate script itself is
  unchanged; FIDELITY.md's first rule now has a runner.

**Verified.** `cargo check` with the vulkan features, the winit examples, and
`aarch64-linux-android` (the property reads compile for the phone);
`vieww-platform-winit`'s lib suite 146/146 (3 new) and `vieww-devtools`
62/62 (7 new); clippy clean under `-D warnings`; `py_compile` and the
verdict tool's self-test green; `bash -n` on both scripts. Per-commit
standalone compiles for bisectability. For the checks.sh wiring: syntax,
BASE resolution, the trailer grep against real and synthetic history, and
the gate script's `--help` — the stage's full double release build exceeds
the disk of the container that wrote it, and says so rather than pretending
a run happened. Default pixels: unchanged everywhere — the CPU path never
scales, and the GPU path scales only behind the opt-in environment variable
or property; `raster-identity.sh` is the gate that keeps that claim
checkable.

**What this does not claim.** The 0.5×/0.75× tier defaults are not promoted:
`quality.rs`'s own deal is that a farm run on the real phones decides, the
verdict tool is that rule applied, and no run with the gpuscale pass has
happened yet. The first-frame gain the pipeline cache (1c589d4) should show
on a real driver is likewise still the farm's to publish.

## 2026-10-15 (evening): the Windows + NVIDIA exit crash, found and fixed

**Symptom.** On Windows 10 with an NVIDIA GeForce 920MX (driver
32.0.15.7652), `tests/wait_loop.rs`' `self_close` and `gpu_frame` exited with
`0xc0000005` *after* passing — on the CPU path too. The same scenarios passed
on the same laptop under Linux.

**Diagnosis.** With `VIEWW_KEEP_VULKAN=1`, which leaks the Vulkan device, all
five scenarios passed: the fault was teardown. `native::SHARED_DEVICE` held the
process's one `VkInstance`/`VkDevice` in a thread-local `Rc`, so it outlived
every window and was destroyed by the thread-local's destructor at process
exit — on Windows, in the loader's process-detach phase, where NVIDIA's driver
faults. Its doc comment said "kept alive for as long as any window holds it";
the `Rc` made that "for as long as the thread lives".

**Fix.** `SHARED_DEVICE` is a `Weak`: shared while any window holds the device,
destroyed when the last one closes, inside the event loop. `NativeRenderer`
drops its GPU rasterizer before its device — `device` is its first field, so a
renderer holding the last reference would otherwise have run
`SceneRenderer`'s `Drop` against a destroyed `VkDevice`, an ordering the
strong reference had been masking.

**Verified on the reference laptop.** Windows: `wait_loop` 5/5 without the
diagnostic; the desktop suite 21/21 (its first Windows run), including a second
window and a dialog closed while the first stays up. Linux: `linux.sh gpu
desktop` — `gpu-hal`, `gpu-3d`, `gpu-window`, `desktop` all pass, so the
multi-window case the shared device exists for is unchanged. (Both runs were
from a downloaded ZIP of `main`, so their summaries carry no commit; the fix
is what made Windows pass.)

## 2026-10-15 (later): first hardware results — the reference laptop, both OSes, and three phones

Evidence: `receipts/2026-10-15-kabylake-i5-7200u-{linux,windows}/` (the
i5-7200U laptop with Intel HD 620 + NVIDIA 920MX, dual-booted, pinned rustc
1.98.1) and the branch `ci-results/device-farm-main` (AWS Device Farm).

1. **Laptop, both OSes:** Vulkan suites, `vieww-3d` GPU parity and the census
   (23/23 fixtures drawn on the 920MX, parity **pass**) on real drivers; the
   D3D12 smoke suite on a **hardware** adapter for the first time; the studio's
   remote preview on Windows; the baseline window idle at **0 frames / 0
   pixels**; the GPU path at 2.0 ms a frame against the CPU path's 2.6 ms
   (Linux) / 3.7 ms (Windows).
2. **The planner's first real calibration** — GPU 8.4x optimistic, CPU 1.7x
   (PENDING §2.3).
3. **Phones** (LG Stylo 5 / Galaxy S20 / Galaxy S26 Ultra): the device suite
   passes on both paths on the mid and high phones; the low-end phone misses
   60 Hz at ~41 ms on both paths, and the stage timings say why — on the CPU
   path half the frame (19.8 ms) is the whole-buffer upload; on the GPU path
   34.4 ms is the Adreno 506 itself. Cold start 65 ms (S26) to ~260 ms (LG).

Open items it produced: §3.4 (Windows/NVIDIA teardown crash), §3.5 (Windows
start-up), §4.1 confirmed on the reference machine, §4.2 (memory and size).

## 2026-10-15: receipts, per-stage frame timings, the hybrid planner in shadow

Work from the roadmap's P0 ("establish the truth") and ticket 8 (`FramePlan`
in shadow mode), done on a 2-core cloud Xeon with no GPU and no device, on
rustc 1.97 (1.98.1 unreachable from that machine). Everything below is
verified there; nothing below has run on a GPU or a phone yet.

1. **Benchmark receipts.** `fixtures --receipt FILE` (gallery and
   `--census`) writes `vieww.bench-receipt/1` JSON: commit, branch and dirty
   flag; the rustc, profile, opt-level, target, `RUSTFLAGS` and compiled SIMD
   features of *that binary* (baked in by `examples/fixtures/build.rs`);
   OS, kernel, CPU, cores, memory, governor and runtime SIMD features; the
   GPU as Vulkan names it; and every raw timed run per fixture with best,
   p50 and worst. `--runs N` sets the run count. `ci/hardware/close-remaining.sh
   gallery` and `ci/certify/certify.sh` now keep a receipt and a shadow log
   beside each gallery and census run. Why it matters, from its first two
   runs: the same binary on the same VM gave `00-layers-nested` a p50 of
   14.2 ms and then 19.4 ms (best 14.0 and 13.1) — a "~20 ms" in prose
   cannot say which of those it was.
2. **The planner in shadow** (`vieww_render_planner::shadow`). `ShadowPlanner`
   plans a frame, the live path renders it as before, and `observe` writes
   one JSON line: per-pass placement and prediction, what actually drew the
   frame, how long it took, and why. `Calibration` reports the throughput
   the device profile would have needed. Predictions stay uncorrected so a
   run cannot converge on the live path and report agreement it did not
   earn. Wired into `fixtures --shadow FILE` and, for live windows, into
   `present_planned` behind `VIEWW_PLANNER_SHADOW=<file>`.
3. **A planner cost bug, found by its first shadow run.** Composite passes
   were charged the area of the target they write — the whole screen for a
   root-level layer — so `00-layers-flat` (384 small layers) was predicted at
   727 ms for a 6 ms frame, and the CPU calibration came out 6x wrong.
   `planner::work_pixels` charges a composite its layer. After the fix:
   predicted 5.8 ms vs measured 5.5 ms; CPU measured/predicted 1.26x over the
   gallery, median 1.12x. The remaining spread (strokes, curves and shadows at
   2–2.7x) is per-pixel complexity the area model cannot see — PENDING §2.3.
4. **Per-stage frame timings.** `vieww_hal::vulkan::SceneRenderer::last_timings`
   splits a GPU frame into upload, record, submit, fence wait and readback;
   `vieww_platform_winit::native::FrameStages` adds scale, plan, CPU raster,
   acquire and present, on every frame, with no allocation. They reach
   `PlannedFrame`, `NativeRenderer::last_stages`, `FrameLog::stage_medians`,
   `FrameReport`'s Display, the `VIEWW_TRACE_FRAMES` trace, and the device
   suite's new `frame_stages` observation — which is what PENDING §1.5's
   21 ms Adreno median needs.
5. **A double scale, found by measuring the stages.** A CPU fallback frame in
   `present_planned` lifted an off-1x scene to device pixels for the planner,
   then called `present_damaged`, which lifted it again. It now rasterises the
   scene it already lifted.
6. **The orphaned text-stack files are gone.** `vieww-text`'s `bidi`,
   `editing`, `fonts`, `selection`, `shaping` and `wrap` had left the module
   tree when the second stack was retired (item 5 of an earlier section below)
   but stayed on disk: 2,146 lines and 55 tests no build compiled.

Tests on that machine: `vieww-render-planner` 37, `fixtures` 7,
`vieww-platform-winit` lib 129, `vieww-render-graph` 12, `vieww-text`
104 + 10 + 4; clippy `-D warnings` on every touched crate.

## 2026-10-14: real hardware — two GPUs, an Android phone, the iOS simulator, a real iPhone

The day the "never run on hardware" caveats came off. Evidence for each line is
in the named script's output or in `PENDING.md`'s dated notes, which this
section replaces as the record.

1. **Real GPUs (PENDING §1.1).** `ci/hardware/close-remaining.sh gpu nvidia`
   on a Kaby Lake laptop, GNOME/Wayland, rustc 1.98.1:

   | GPU | Driver | `vieww-hal --features vulkan -- --ignored` | `vieww-3d` GPU | GPU frame in a real window |
   |---|---|---|---|---|
   | Intel HD Graphics 620 | Mesa 26.0.8 | 44/44 (compositor 17/17 incl. backdrop blur) | 7/7 | `wait_loop` 5/5, `gpu_frame` ok |
   | NVIDIA GeForce 920MX | 580.178.04 (PRIME) | 44/44 | — | — |

   The desktop suite on the same machine: 21/21, 1.76 ms median frame.
2. **Every CI leg green** (`.github/workflows/framework-checks.yml`, now
   manual-only, results published to `ci-results/<branch>`): `checks.sh`,
   the coverage floor, the cross type-check, the Vulkan suites on lavapipe,
   **the Metal suite on the macOS runner's (paravirtual) Apple GPU** — so the
   MSL compiled and ran for the first time — **the D3D12 suite on WARP**,
   Android compile, **canvas parity in Chromium, Firefox and WebKit**, the
   Android emulator (APK, device suite, accessibility suite, Maestro) and the
   iOS simulator. Fixes on the way: `chunks_exact` → `as_chunks` (rustc 1.98
   clippy), libxkbcommon on the runner for `wait_loop`, a debug keystore
   generated on runners that have none (`android-env.sh`).
3. **The iOS simulator — the first vieww frame on an iPhone screen.** The
   black screen was a missing Vulkan: MoltenVK is now bundled
   (`VIEWW_MOLTENVK`; the simulator slice comes from Khronos'
   `MoltenVK-all.tar`, the iOS tar has only the device slice) with Metal
   argument buffers off on the simulator. `fail_to_open` now says why a
   window could not open and exits, instead of a silent black screen. Device
   suite **12/12**, 600 frames at 6.8 ms median. Two defects fixed: the window
   took the desktop preview size on a phone (accessibility frames off by
   27,28 px, so Maestro tapped beside "Tap me"), and the Metal layer stayed
   at `contentsScale` 1.0 on a 3x screen (`ios::match_layer_scale`).
4. **A real iPhone.** The CI `vieww.ipa` (ad-hoc, MoltenVK device slice) on an
   **iPhone 13 Pro, iOS 15**, through LambdaTest's real-device cloud (which
   re-signs): renders at once, taps count, the system keyboard inserts at the
   caret, tabs/segments/radios/bottom bar stay in sync, animations run, the
   drawer opens. Interactive evidence (a screencast); the suite's numbers on
   that device are still `PENDING.md` §1.4.
5. **An Android phone** (Redmi Note 7 Pro, Adreno 612, Android 16, 1080x2340):
   device suite 15/17 on the CPU rasterizer and **16/17 on the planned GPU
   path**, which draws 600/600 frames straight into the swapchain:

   | | CPU rasterizer | GPU path |
   |---|---|---|
   | median frame | 36.4 ms | **21.0 ms** (1.7x) |
   | worst frame | 115 ms | **29 ms** |

   The two screenshots match in everything static. TalkBack bound and heard
   speaking through vieww's tree (an earlier silence was TalkBack not
   running); the TalkBack-bound tree has 30 nodes, roles and states right, and
   the modal hides what is behind it through the bridge. Maestro's flow passes
   on the phone. The GPU path is chosen with `debug.vieww.gpu=1`
   (`device-suite.sh --gpu`) or Test Lab's scenario 2.
6. **Bugs that only devices found:** the drawer's panel covered its own
   scrim, so it could not be dismissed by touch (`Drawer::build`, pinned by
   `vieww-test-harness/tests/drawer_dismiss.rs`); backdrop blur read past its
   region as transparent where the CPU oracle clamps (`Edge::Duplicate`,
   PENDING §2.2); on Wayland the refresh rate was never re-read (the check now
   runs inside `WindowState::draw`, at most once a second, verified on a
   headless Weston).
7. **Tooling.** `ci/hardware/close-remaining.sh` (one command for every
   hardware-only item, one tarball back); `ci/mobile/maestro.sh` +
   `maestro/demo.yaml` (Maestro on emulator, simulator and phone);
   `ci/mobile/test-lab.sh` (Firebase Test Lab: Game Loop on CPU and GPU per
   phone, Robo, one summary); `ci/mobile/apk-add-code.sh` (adds the
   `classes.dex` Test Lab requires to a pure-Rust APK); CI jobs that build the
   APKs and the `.ipa`/simulator `.app` as downloadable artifacts.

## 2026-10-11: every GPU edge antialiased, Metal and D3D12 render paths, cross-platform type-check

1. **GPU edges for non-analytic shapes (PENDING §2.1, the parity half).**
   Rotated rects, arbitrary fills and every stroke now draw as one quad over
   their inked pixels, with coverage from the CPU rasterizer itself
   (`GpuSeam::shape_coverage`: same polygon construction, flattening and 4×4
   coverage as `NativeRenderer`), cached in the mask atlas by content with
   any shaped clip multiplied in. Census on lavapipe, same machine, before →
   after: worst channel **203 → 47**/255, channels over 1/255 **525,299 →
   245,647**, fixtures with interior mismatches **6 → 2** (`00-transforms`
   688→0, `12-avatar-wall` 68→0, `12-avatar` 49→0, `00-blend-modes` 7→0;
   left: `23-editor-glass`'s known backdrop-blur numerics and `22-editor`'s
   56 px, both unchanged). New `vulkan_scene` test holds a rotated rect, a
   star, a dashed curve and a clipped circle within 2/255 on **every** pixel;
   with the change disabled it fails at 180/255. Consequence worth knowing:
   for these shapes the GPU-vs-CPU comparison is no longer between two
   independent rasterizers — the tessellator's independent check now lives
   in `vulkan_mesh_smoke.rs` and the planner-less path. Residual cost:
   `PENDING.md` §2.1.
2. **The GPU window path renders straight into the swapchain (PENDING
   §2.2).** `NativeRenderer::present_planned` acquires the swapchain image,
   and `SceneRenderer::render_planned_to` blits the frame's RGBA16F target
   into it on the GPU (format conversion and BGRA order included), waiting
   on the acquire semaphore only at the blit. Per 1366×768 frame that removes
   an 8.4 MB device→host readback, a CPU unpremultiply/quantise pass, a
   4.2 MB host→device upload and the BGRA swizzle loop. Opaque frames only —
   a translucent window keeps the readback, because a blit cannot
   unpremultiply. Verified on a real Xvfb window under lavapipe:
   `wait_loop`'s `gpu_frame` now asserts every GPU frame went direct
   (`FrameReport::gpu_direct_frames`) **and** grabs the X screen with
   ImageMagick to check the window shows its blue; with the blit disabled the
   screen has 0 blue pixels and it fails. No timing claimed — lavapipe's
   numbers are not a GPU's.
3. **viewwsite: light scheme, keyboard reach, and two stale gaps.** The
   README's three "not yet done" items, checked against the code: the
   device-pixel scale was already closed (`WebSurface::resize`), and the
   DOM page's controls were already links in document order — but its
   "Copy" control was a `div` with a script listener, unreachable by Tab,
   and any `GestureDetector` the DOM backend emitted had the same problem.
   Now: `set_color_tokens` in `vieww-platform-web-dom` (palette colours →
   CSS custom properties), a light theme in `index.html`, button semantics
   (`tabindex`, `role`, Enter/Space) for every non-button tap target,
   `VNode::to_html` and a `snapshot` example. Verified in Chromium under
   emulated dark and light schemes: body background `rgb(15,13,11)` /
   `rgb(251,249,247)`, the Copy button reached by Tab in both (11th stop),
   and both schemes screenshotted. The wasm build lints clean again: two
   wasm-only lints in `vieww-video` and `vieww-audio` were fixed on the way.
4. **Refresh rate (PENDING §3.2), checked against winit 0.30.13's source.**
   Every platform vieww targets already reports it — winit on macOS, iOS,
   Windows, X11 and Wayland, this crate's JNI call on Android (the one
   winit backend that hard-codes `None`, besides Redox). What was missing was
   *when*: only at window open. It is now re-read on `Moved` and
   `ScaleFactorChanged` when the window's monitor changed. Not exercised
   with a real second monitor (see PENDING §3.2).
5. **Metal and D3D12 render paths**, type-checked and linted on their own
   targets from Linux, never yet executed — `PENDING.md` §1.2/§1.3.
6. **`ci/check/cross-check.sh`**: the workspace type-checks for macOS, iOS,
   Windows and Android from Linux; its first run found five real bugs (see
   the commit).
7. **`vieww-spatial`** applied from the supplied patch, fixed to build.

## The most recent effort (2026-10-09): the PENDING list emptied of everything a machine can close — analytic edges on the GPU, a GPU frame in a real window, atlas eviction, the full gate on the pinned toolchain

Scope: `PENDING.md` itself, item by item, with one question per entry — "can
this be closed by work, or does it need a machine this one is not?". Eleven
entries closed with evidence; the rest are honestly blocked on hardware or a
platform toolchain and remain in `PENDING.md`, renumbered and with their
follow-ups written down as new entries rather than lost with the closure.
`PENDING.md` §3.1 (colour fonts), §3.2 (variable fonts) and the subpixel half
of §3.3 turned out to be *already closed in code* — `color_glyphs.rs`,
`FontData::with_variations`, `AaMode::Lcd` — while the file still listed them;
that is also fixed, and the record below says so rather than claiming this
pass wrote them.

**Verified, by things that ran (rustc 1.98.1 — the pinned toolchain, on a
machine that can install it, which the machine before this one could not):**

1. **The full certification gate on 1.98.1 — `PENDING.md` §1.7.** `ci/certify/
   certify.sh` ran under Xvfb with the extracted lavapipe ICD. The first pass
   failed three stages, all real, all fixed in this tree: the workspace was
   unformatted (98 files from the earlier gap-pass commits; `cargo fmt --all`
   now passes), clippy's `approx_constant` fired on ten hand-written
   `0.707…` constants in the new analytic-AA tests, and the wait-loop suite
   could not open a window because this image has no `libxkbcommon-x11` /
   `libxcb-xkb` (provided from the system debs, `scripts/` in the run's
   evidence bundle). Every suite the gate names then ran green: workspace
   tests (5,079 passed, 0 failed, including all five wait-loop scenarios),
   layout-stress, animation-stress, text-fidelity, image-effects,
   scroll-stress, native-surface, premium-ui, the fixture gallery, wasm-check,
   the web baseline, the desktop suite (21 checks, 0 failed), and
   `vieww-standard` — every clause held (startup 16.3 ms, worst frame 8.0 ms,
   p95 7.1 ms). The final rerun (2026-10-09, after the workspace's lint debt
   was cleared — see the `film_lab` note below) was staged stage-by-stage
   with certify's own commands into the same `OUT` layout: this 9.9 GB bench
   cannot host one end-to-end `certify.sh` (its `rm -rf "$OUT"` start makes
   an interrupted run cost the whole bundle, and the disk filled twice
   mid-run), so `summary.txt` carries the staging note and the per-stage
   logs carry the evidence. The staged totals: 5,079 workspace tests (4,162
   over the workspace minus the two scaffold-heavy packages, plus 911 in
   `viewwstudio`, plus `vieww-build`'s scaffold build, plus the five
   wait-loop scenarios under lavapipe), all six stress/fidelity suites, both
   visual stages, wasm-check, the web baseline, the desktop suite and the
   standard — all green; the one red is `gpu/vulkan-suite`'s backdrop-blur
   parity, below.
2. **Analytic edge coverage for the rect family on the GPU — §2.0.** A
   signed-distance material in `scene.wgsl` (`shape_distance` /
   `shape_coverage`), a new `SHAPE_SOLID`/`SHAPE_GRADIENT` material pair, a
   `shape[4]` vertex attribute, and `emit_shape_painted` routing axis-aligned
   rects and rounded rects to it. Verified: `vulkan_scene`'s
   `fractional_and_rounded_rects_have_antialiased_edges_like_the_cpu` — GPU
   versus CPU SDF within 3/255 on edges — plus the census: interior
   mismatches fell from 8 fixtures to 6, with `00-blurs` 208→0,
   `00-layers-nested` 584→0, `20-settings` 4→0 and `22-editor` 1258→56. The
   one fixture still over tolerance is `23-editor-glass`, the backdrop-blur
   screen, whose count on the *clean* tree is the same (31,827 vs 31,615 px) —
   driver blur numerics, not this change; see `docs/GPU-RENDERER-STATUS.md`'s
   note.
3. **A GPU frame presented to a real window — §2.6.** `NativeRenderer::
   present_planned` plans the frame, draws complete plans with
   `SceneRenderer`, and presents them through the same swapchain path the
   CPU's pixels always took, falling back per frame otherwise;
   `App::prefer_gpu` routes the event loop through it, and `FrameReport`
   grew `gpu_frames` so "is the GPU actually doing it" is a number instead of
   an inference. Verified by `wait_loop`'s `gpu_frame` scenario on a real
   Xvfb window under lavapipe: `gpu_frames >= 1` with the window visibly
   drawing. The readback it pays on the way to the screen is the new
   `PENDING.md` §2.2.
4. **The glyph atlas evicts — §2.7.** Generational compaction between frames
   (never within one), a deliberate-fill test that packs the atlas to its
   961-glyph capacity and asserts the next frame compacts while keeping
   still-in-use glyphs at their texels. Growth keeps shelves at their opened
   width — the pre-existing invariant, now pinned by tests.
5. **`ListView` programmatic scrolling — §5.4.** `ListView::row_position` /
   `row_offset_for` + `RowAlignment` in `vieww-widget`; `ScrollController::
   reveal_row` / `jump_to_row` in `vieww-element`, the jump refreshing the
   content extent from the list's declared length first so it works before
   the first layout. 22 tests and doctests.
6. **The text items, mostly by discovering they were done — §3.1, §3.2,
   §3.3.** `test-text-fidelity` (a certify stage, green above) renders
   variable-font instances at interpolated weights, COLRv0 palette layers
   and CBDT bitmap emoji and asserts on the ink; `glyph.rs` applies
   variation coordinates before any contour is walked and `FontData::
   with_variations` mints a fresh cache id per instance. The §3.3 decision
   note in `native/glyph.rs` was **rewritten** — it had claimed subpixel AA
   does not exist while `AaMode::Lcd` does; it now states the real policy
   (opt-in, root-surface-only, grayscale fallback) and the hinting decision.
7. **The coverage number — §6.1.** `cargo-llvm-cov` installed and the gate
   wired into the workflow, which runs it on every push
   (`continue-on-error` until its numbers have a history). The first number
   is the workflow's to produce: the instrumented build alone grows past
   5.5 GB against this bench's 9.9 GB root filesystem — the 70% floor was
   attempted twice here and both runs died on disk (the scaffold tests
   build full cargo projects into `/tmp`), not on coverage, and a number
   produced by a run that died is worse than no number. What *was* verified
   on this machine is the gate's preflight: `cargo-llvm-cov --version`
   resolves, and rustc's own `llvm-profdata` is present in the sysroot.
8. **A CI workflow that runs the gate — §6.2.** A new
   `.github/workflows/framework-checks.yml` beside the existing mobile-build
   workflow: `ci/check/checks.sh` on Linux (the job whose red means "do not
   push"), the coverage gate, and `ci/check/platform-check.sh` for macOS and
   Windows — the compile half of §1.2/§1.3 on runners nobody has to own.
   The mobile workflow has 23 recorded runs (green 2026-10-08), so the
   framework's Android and iOS *builds* are exercised on runners; a frame on
   a device remains `PENDING.md` §1.4/§1.5. *(Since 2026-10-14: frames on
   the emulator, the simulator, an Android phone and a real iPhone — see
   that day's section.)* One mechanical note, recorded
   here rather than hidden: the token this branch was pushed with lacks the
   `workflow` scope, and GitHub refuses workflow-file pushes without it —
   the file is therefore not in this commit; it lands with a workflow-scoped
   push or a web-UI upload of the identical content (kept beside the
   checkout), and everything else in this entry is in the tree and pushed.
9. **`vieww-platform-web` is publishable — §6.3.** `wasm-check.sh` passed
   again on 1.98.1 (stage-0 web-sys audit, `cargo check --all-targets`,
   clippy `-D warnings` for wasm32); the `publish = false` and its banner are
   gone, with the removal's own record in the crate's `Cargo.toml`.
10. **The workspace's lint debt cleared — the `checks.sh` gate can go green.**
    The clippy stage of `ci/check/checks.sh` had never been able to pass on
    this branch: the earlier gap-pass commits left 13 lints in the framework
    crates (`needless_range_loop`, `type_complexity`, `approx_constant` in
    `vieww-effects`/`vieww-image`/`vieww-network`/`vieww-3d`/`vieww-dataviz`/
    `vieww-game`/`vieww-video`) and `film_lab` — a workspace member since the
    consolidation — carried 767 more (unused imports and variables from the
    retired cuts, dead scene doors, `too_many_arguments` on scene fns whose
    parameters are the cinematography). All fixed or, where the dead code is
    the *policy* (retired film sources kept for byte-reproducibility),
    `#[allow(dead_code)]`'d at the module door with the reason in a comment:
    `product_film` is allowed module-wide (only `script` is live, ridden by
    `studio_film`); individual retired helpers in `studio_film`, `film_lib`
    and the round-11 experiment files carry item-level allows. Behaviour
    neutrality was the rule everywhere: the RNG call order in `exp_ink`'s
    branch roll is preserved (`||` short-circuits exactly as the retired
    `if/else` did), counters the receipts never read were removed rather
    than wired up (wiring them would change the next re-render's
    `metrics.txt`), and no arithmetic changed. After the sweep:
    `cargo clippy --workspace --all-targets --features
    "vieww-paint/native,vieww-hal/vulkan" -- -D warnings` exits 0, `cargo
    fmt --all -- --check` exits 0, and `cargo check`/`cargo test` still pass
    (the 5,079 above is *after* the sweep — the films' determinism claim
    survives the lint fixes by construction, since none of the changed lines
    feed the FrameDriver).

**What stayed red, and why it is recorded here rather than hidden:** the
compositor's backdrop-blur parity test fails under this driver (16/17 —
identical failure on the clean tree, same worst pixel and count; the
historical 17/17 was an older lavapipe). It is a driver-numerics difference
in the half-float box passes, pre-existing, and `docs/GPU-RENDERER-STATUS.md`
carries the full note and the two honest fixes. The GPU stages inside
certify itself were `SKIPPED(no Vulkan loader/ICD)` — that script's gate
wants the `vulkaninfo` CLI, which this image cannot install; the same stages
were run manually with the same ICD and the results above are theirs.

**Machine:** 3 cores, 9.9 GB root filesystem (the gate ran lean:
`ci/lib/lean.sh`), Xvfb display, lavapipe 25.0.7 from the distro deb. One
disk-full incident mid-gate (a parallel build of the reviewer's own making)
took down the wasm-check, web-baseline and desktop stages of the first
certify pass; each was re-run clean and is green above. The first pass's
animation-stress and vieww-standard failures were contention from the same
incident — both pass with the machine otherwise idle. Everything above is a
correctness claim; no performance claim is made anywhere, and lavapipe
timings (the census's 2.4 s GPU column) are a software rasterizer's numbers.

## Previous effort (2026-09-30): the layer-table audit — eight new crates, thirty-odd subsystems, eleven photographed examples, two rendering bugs fixed

Scope: every line of every per-framework layer table in the repository-root
document, asked "can a vieww program do this today?". The creative-tool
frameworks (After Effects, Blender, TouchDesigner, Unity, Unreal, GSAP,
Motion Canvas, Manim, D3, Konva, Three.js) name whole subsystems there that
vieww did not have. The root document's "Third pass" table lists each one
and what closed it.

**Verified, by things that ran (stable rustc 1.95 on this machine; the pinned
1.98.1 toolchain could not be downloaded through the sandbox proxy):**

1. **New crates:** `vieww-3d` (scene graph, software rasteriser with
   shadows/SSAA/culling/LOD/instancing, raycast picking, BVH path tracer,
   glTF import, `Viewport3D`), `vieww-canvas` (Konva-style stage,
   hit-testing, events, `Transformer`, JSON), `vieww-dataviz` (D3 scales,
   joins, shapes, hierarchy layouts, force, contours, colour),
   `vieww-graph` (cooking node graph + Blueprint exec graph), `vieww-game`
   (ECS, behaviours, coroutines, prefabs, scenes, input), `vieww-collab`
   (CRDT doc, ops as JSON, presence), `vieww-lottie` (Bodymovin playback).
2. **New modules in existing crates:**
   - `vieww-foundation`: `json`, `path_ops` (booleans, measure, trim,
     dashes, hit-tests, SVG data) and `sketch_export` (SVG/PDF).
   - `vieww-animation`: `sequence`, `flow`, `animator`, `fcurve`,
     `constraints`, `nla`, `expr`, `channels`, `particle_system`,
     `retarget` and `shared`.
   - `vieww-mesh`: `gltf` and `modifiers`.
   - `vieww-physics`: `rigid` (rotation, SAT, joints with limits and
     motors, CCD, sleeping, queries, character controller), `cloth` and
     `fluid`.
   - `vieww-video`: `comp`, `matte`, `track` and `export`.
   - `vieww-audio`: `midi` and `dsp`.
   - `vieww-network`: `osc`.
   - `vieww-effects`: `cpu::pixel`.
   - `vieww-widget`: `CodeBlock` and `CodeMorph`.
   - `vieww-element`: `binding`.
   - `vieww-image`: `sprite`.
3. **Two pre-existing rendering bugs, found by looking at the pictures and
   fixed with failing-first regression tests:**
   - `Path::arc_ring` began its inner arc (and a wedge's arc) from the
     wrong pen position, so donuts, pie wedges and progress rings bulged.
     The test is `path_ops::tests::arc_ring_band_and_wedge_have_straight_sides`.
   - The native stroker stamped round-join discs with the opposite winding
     to its segment quads, so under nonzero fill every round join became a
     hole and thick curved strokes rendered beaded. The test is
     `stroke::orientation_tests::round_joins_do_not_punch_holes`.
4. **Tests:** every framework crate's tests pass: 3,912 tests across
   `crates/*`, plus 911 in `apps/viewwstudio`, 0 failed. The one failure
   found on the way was an overflow in a test's own arithmetic, now fixed.
   `cargo clippy --workspace --all-targets -D warnings` is clean for
   everything this pass touched. Clippy still reports some pre-existing
   warnings in `examples/film_lab`, `examples/test-text-fidelity` and
   `vieww-paint/src/native/glyph.rs`; this pass did not change those.
5. **Eleven new feature examples, 81–91,** each run headless through the
   CPU rasteriser (`VIEWW_SHOT`). Every frame was looked at, and one
   representative PNG per example is kept in `examples/shots/`:
   - `81-path-ops`
   - `82-timelines`
   - `83-rigging`
   - `84-particles-retarget`
   - `85-3d-advanced`
   - `86-physics`
   - `87-canvas-dataviz`
   - `88-graph-game-collab-lottie`
   - `89-video-effects`
   - `90-midi-dsp-osc`
   - `91-code-binding-sprites`

   Example `80-3d-scene` came from the same pass. Shared drawing helpers
   (a closure painter, captioned panels, grids, shapes) live in the feature
   harness's `draw` module.

## Previous effort (2026-09-29, later): the cross-framework capability recheck — thirteen capabilities, physics joints, twenty-four photographed examples

Scope was the harder reading of the repository-root document: not "what did
the gap list say," but "what capability does *every* framework in the
document's twenty-two layer tables offer, and does vieww answer it?" The
audit went capability-by-capability; thirteen existed elsewhere and not
here. All thirteen are now closed, each with tests and a numbered feature
example run headless, plus a `Joint` in `vieww-physics` and the examples
debt of the first pass repaid.

**Verified, by things that ran (pinned rustc 1.98.1, this machine):**

1. **`vieww-animation` grew six modules, 68 tests and 2 doctests** (186 lib
   tests + 16 doctests total, all green, clippy clean): `noise` (seeded
   Perlin 1/2/3 D + fBm, bounded ±1 by the g·√N/2 argument, lattice-flat
   and neighbour-coherence pinned); `lfo` (five waveforms as a `Simulation`
   that honestly never finishes); `particles` (`ParticleField` — a pure
   function of time: births hashed from `(seed, index)`, closed-form
   ballistic positions, capacity `rate × lifetime_max` as a theorem,
   checked empirically across a 600-step sweep); `blend_tree` (`BlendTree1`
   smoothstep between threshold brackets, `BlendTree2` inverse-distance
   weights, children at a shared clock); `skeletal::solve_two_bone` (the
   law of cosines, zig-zag sign pinned by the round-trip test that caught
   the same-sign bug, unreachable targets extend and say so); keyframe
   `LoopMode` (loop wraps to frame zero, ping-pong is continuous at the
   turn); and 27 named `Curve` presets (the GSAP families on Penner's
   equations — endpoints, mirror symmetry, in-out symmetry, monotone
   families and the bounce landing values all pinned).
2. **`vieww-audio::analysis` (new module, 11 tests)**: radix-2
   Cooley–Tukey FFT (bit-reversal + butterflies), Hann window, `Spectrum`
   with bin pitches, peak bin and band energy. Its tests round-trip the
   crate's own synthesis — a rendered 440 Hz tone is *found* at 440 Hz; a
   two-note mixdown shows both notes' bands dominating quiet bands — the
   cheapest complete test there is, and no fixture files.
3. **`vieww-widget` grew four widgets, two shapes and 35 tests** (558 lib
   tests total, green, clippy clean): `FunctionGraph` (y = f(x), origin
   axes, NaN as gaps); `MathText` (TeX subset — `^`/`_`, `\frac`,
   `\sqrt[n]`, Greek, operators — real text runs, layout-box rules,
   unknown commands stay visible); `SplitText` (per-char and per-word
   cascades, the window maths pinned at both ends); `Star`/`Polygon` morph
   shapes (exact corner interpolation; flat sides, envelope and morph
   round-trip tested). Also fixed while there: `ScatterChart`, `PieChart`,
   `DonutChart` and `ChartAxes` were never in the widget prelude — a
   pre-existing gap the new examples flushed out.
4. **`vieww-physics::Joint` (9 tests)**: the distance joint — rods and
   tethers, stiffness clamped 0..=1, position correction split by inverse
   mass and a velocity impulse that cancels along-axis drift. The tests
   pin the physics the module can honestly claim: a pendulum *swings* (no
   damping, so the assertion is the circle it stays on), a hanging bob
   stays put, equal masses meet in the middle with the centre of mass
   unmoved, a swing does not gain energy. `vieww-physics` total: 22 tests.
5. **`Lerp` for `(f32, f32)`** in `vieww-animation` — the two-parameter
   pose a blend tree wants.
6. **Twenty-four new feature examples, 56–79** (`feature-keyframes` …
   `feature-ik`): every capability above *and* the first pass's
   example-less crates — audio, video, mesh, network, physics, embed,
   skeletal, state machine, keyframes, blend tree, scatter/pie/donut. Each
   animated one drives a signal from a ticker the *widget* keeps alive
   (the registry holds tickers weakly — the bug the first shot run
   produced single frozen frames for, and the reason every clock is a
   field on its widget now).
7. **Every one of the twenty-four photographed headless** — `VIEWW_SHOT`
   through the CPU rasteriser, the `ci/certify/shot-suite.sh` path:
   **76 PNGs**, animated examples as 5-frame strips verified
   frame-different (byte-compared; the lfo/morph strips' first and last
   frames coincide because their clocks wrap — checked, not assumed), and
   every static example verified non-blank by pixel statistics. The shots
   live outside the repository (out/ is for capture directories), and the
   counters were reviewed as they printed.
8. **`cargo test -p` green on every touched crate** — `vieww-animation`
   186 + 16 doctests, `vieww-audio` 42 + 4, `vieww-widget` 558, 11 doctests
   in its other suites, `vieww-physics` 22 + 1 — and `cargo clippy
   --all-targets` clean on all of them and on all twenty-four examples
   (the only warning in range is `vieww-foundation`'s pre-existing
   `Brush` variant-size note, untouched by this pass).
9. **The facade re-exports every new type** (`BlendTree1/2`, `Particle`,
   `ParticleField`, `LoopMode`, `Spectrum`, `Joint`, …), the new widgets
   are in the prelude, and `cargo test -p vieww --test facade_exports`
   stays green.

**Not verified, and why:**

- The particle field is *ballistic* — constant velocity, constant gravity —
  and says so in its module docs: drag, attractors and a per-particle force
  field are integration, which is a different contract than the one this
  crate makes. Named, not pretended.
- `MathText` is a TeX *subset*, stated in its module docs, not TeX:
  matrices, alignment and macro definition are out of scope, and an
  unknown command renders as visible text rather than vanishing.
- N-bone IK (FABRIK, CCD) remains out for the reason the skeletal module
  docs gave before two-bone IK arrived: iterative solvers have convergence
  behaviour, and the analytic two-bone case does not. The leg chains of a
  spider are still a named gap.
- The blend tree's 2D form is inverse-distance weighting rather than
  Unity's triangulated freeform — a documented choice, and the same
  answers at the children either way.
- `SplitText` loses cross-glyph kerning, exactly as GSAP's own split does,
  and its module docs say so.

## The most recent effort (2026-09-29): the capability gap-closure pass — six new crates, three animation modules, two charts, a real iframe

Scope was the "Identified Gaps" table in the repository-root document
(`What are the best UI frameworks.md`), which listed thirteen capabilities the
comparison frameworks have and vieww did not. Three of those were stale (the
widget catalogue, i18n, and line/bar charts — present all along on this
branch); the other ten are now closed. Evidence below is from
`cargo test -p <crate>` on the pinned rustc 1.98.1 on this machine, run
crate-by-crate; a single whole-workspace `cargo test --workspace` run was
started but this container's 10 GB disk cannot hold the full test-binary link
set, so per-crate results are the record.

**Verified, by things that ran:**

1. **`vieww-animation` grew three modules, 44 tests and 3 doctests** (118
   lib tests total, all green): `keyframe` (`Keyframes<T>` with
   eased/held/arrival keyframes, out-of-order insertion, same-time
   replacement, and the staggered `Timeline` of named tracks — 14 tests);
   `state_machine` (`StateMachine<T>` with declared states and guarded
   transitions, cross-fades where the from-state keeps playing, one clock
   advanced by deltas — 13 tests); `skeletal` (2D bones in a
   parents-before-children flat walk, `Pose`, `SkeletalClip`, linear-blend
   `Skin` with lazily normalised weights, pose blending — 17 tests, including
   the two-bone-arm skinning identity that catches a composition-order
   regression and the default-scale-identity test that catches a
   degenerate-matrix regression, both of which failed before their fixes).
2. **`vieww-audio` (new, 30 tests + 3 doctests)**: WAV read/write — 8- and
   16-bit PCM, chunk-walking (a `LIST` chunk before `data` parses), 8-bit
   silence-at-128, truncation and missing-chunk errors named by line and
   kind; `Waveform`/`Envelope`/`Tone` synthesis with continuous-at-boundary
   envelopes; `Mixer` with saturating sums, stereo downmix-by-mean, phase-
   locked stagger test, rate refusal; the `AudioPlayer` service trait with
   `NoAudio` (honest refusal) and `RecordingPlayer` (wiring double).
3. **`vieww-video` (new, 15 tests + 4 doctests)**: `VideoSource` trait,
   `FrameSequence`, deterministic `GeneratedVideo` patterns, `VideoPlayer`
   with delta-driven clock — pause-holds-position, loop wraps counted
   (forward and backward), speed 2×, negative speed without looping stops
   at zero, the end is the last frame exactly, empty sources report `None`.
4. **`vieww-mesh` (new, 18 tests + 1 doctest)**: OBJ — all four face
   spellings, negative indices, pentagon fan triangulation, scene records
   skipped, computed normals for files without `vn`, errors with line
   numbers, the pool/output vertex split (a quad is 4 vertices, not 8);
   STL — binary and ASCII, size-checked against the `solid`-header trap.
5. **`vieww-network` (new, 13 tests + 1 doctest)**: `Url` five-part parser
   (ports, lowercased hosts, refusal of userinfo/unknown schemes with
   reasons), `HttpRequest`/`HttpResponse`, `HttpClient` on the foundation
   `Task`, `MemoryClient` with route tables, computed answers and a request
   log, registration through `Services`.
6. **`vieww-physics` (new, 13 tests + 1 doctest)**: gravity integration
   (semi-implicit, checked at two steps), fixed bodies never move, a ball
   rests on a floor to within 1.5 px, restitution split by the pair-minimum,
   equal-mass head-on stop with momentum conserved, barge-and-pebble
   impulse split, all three narrowphase pairings with the a-to-b normal
   convention pinned both ways, correction leaves a hair under touching by
   design.
7. **`vieww-embed` (new, 4 tests + 1 doctest)**: `WebContent` (URL/HTML) →
   `PlatformViewSpec` of kind `"webview"` with the parameters a native
   factory expects; `WebView` widget with a themed placeholder build.
8. **`vieww-platform-web-dom` turns the embed into a real `<iframe>`**: the
   DOM walk recognises `vieww_embed::WebView` (the same escape-hatch shape
   `Styled`/`Tag`/`Canvas` use) and emits an `iframe` VNode with `src` or
   `srcdoc`, a `title` for assistive technology, lazy loading, and the
   widget's requested size; the patcher sets the attributes on create and
   on change. 4 new walk tests (`tree::tests`), 7 lib tests total, green.
9. **`vieww-widget` grew two charts, 29 chart tests green** (523 lib tests
   total): `ScatterChart` (points in the plane, own-axis normalisation,
   inset so extreme points keep their edges, the same axis frame as the
   line chart) and `PieChart`/`DonutChart` (real vector-arc wedges through
   the `Sketchbook` path API, chunked text legends, whole-percent shares,
   screen-reader summaries with the shares, negative slices skipped
   silently, the donut's centre-label overlay).
10. **The `vieww` facade re-exports every new module and crate** — checked
    by `cargo check -p vieww`, `cargo test -p vieww --test facade_exports`
    (green), and the full set of facade integration tests run one binary
    at a time against the disk limit: `sensitive_capture` (6),
    `settings_screen` (15), `extensibility` (10), `editor` (22),
    `paint_to_pixels` (13), `accessibility_preferences` (0),
    `facade_exports` (1), and every remaining `crates/vieww/tests/*.rs`
    suite — **zero failures**.
11. **`cargo clippy --all-targets` clean** on all ten changed or new crates
    (`vieww`, `vieww-animation`, the six new crates, `vieww-widget`,
    `vieww-platform-web-dom`) under the pinned 1.98.1.

**Not verified, and why:**

- The audio *device path*, the video *decode path* and the HTTP *transport*
  are platform work behind the traits, deliberately: this pass ships the
  shapes, the clocks, the seams and the test doubles, and the registry
  entries a platform crate registers. Nothing here claims a speaker, a
  decoder or a socket.
- GLTF is a recorded scope decision (`vieww-mesh`'s module docs), not a gap
  forgotten: the `Mesh` type is the seam a loader targets.
- Physics beyond 2D translation — rotation, joints, continuous collision —
  is named as out of scope in `vieww-physics`'s module docs.
- iOS remains `PENDING.md` §1.4: written, cross-compiled in CI, never run.
  *(Since 2026-10-14 it has run: the simulator and a real iPhone 13 Pro — see
  that day's section at the top.)*
- The whole-workspace single-command test run could not complete on this
  machine's disk; per-crate results above are the substitute, and
  `ci/certify/certify.sh` on a bigger machine remains the one command that
  runs everything.



Scope was a review of the September 15 archive: GPU planner gaps, a
certification script that did not run most suites, a standard runner that fed
literal zeros into two clauses, and a patch-workspace archive. Evidence below
is from `ci/certify/certify.sh` on a 2-core container with lavapipe
(Mesa's software Vulkan), rustc 1.95.0 — **not** the pinned 1.98.1, which this
machine could not download. Source digest of the certified tree is in
`target/cert-linux/meta/source.txt` of that run.

**Verified, by things that ran:**

1. **The uploaded tree did not load as a workspace.** `examples/launch-animation`
   was a member with no directory; removed from `members`. `.cargo/config.toml`
   was missing (dropped with hidden directories), which aborts
   `viewwstudio`'s panic-boundary tests; reconstructed from the contract in
   `ci/check/checks.sh`, `Cargo.toml` and the mobile runner scripts.
2. **The GPU path now covers every `Command`.** Offscreen layers and group
   opacity, all 28 blend modes, layer and backdrop filters, shaped clips,
   outer/inset/rotated shadows, per-fragment gradients, rotated and mip-blended
   images, colour glyphs, dashed strokes. `vieww-hal`'s new `vulkan_compositor`
   suite (17 tests) compares every pixel with `NativeRenderer`: max channel
   difference 1 wherever geometry is pixel-aligned. Mutations (blur kernel,
   Hue formula, even-odd winding, miter limit) each fail it.
   `fixtures --census`: **23 of 23 fixtures plan complete**, 0 mismatches
   beyond the geometry-edge band. `test-gpu-work`: `unsupported_gpu_commands=0`
   measured, worst frame 0.378% of pixels over tolerance 8 — identical across
   ten consecutive runs.
3. **Bugs this found in the existing GPU path**, each fixed with a test: the
   scene shader did not parse (every Vulkan scene test failed on the uploaded
   tree); group opacity folded into child alpha; radial/sweep gradients
   interpolated per vertex; rotated images drawn as bounding boxes; atlas
   growth moving live entries; per-instance atlas versions colliding across
   planners; even-odd fill rule; device-space strokes and ignored dashes;
   lyon's miter limit at twice the CPU's; 16-bit indices silently emptying large
   paths; image identity by reusable address (also in the **CPU** `MipCache`).
   Details: `docs/GPU-RENDERER-STATUS.md`.
4. **`vieww-standard` measures all twelve clauses.** `steady_allocations` by a
   counting global allocator (first honest reading: 7 per steady frame, now
   **0** over 60 frames); `unsupported_gpu_commands` from real plans (**0**
   over 91 frames). Fixes behind the zero: scratch buffers in
   `ElementTree`/`RenderTree`/`LayerTree`, and `RenderTree::set_root` clearing
   the incremental boundary cache on every frame — the cache never reused a
   subtree in a real frame loop. `NativeRenderer::render_in_place` /
   `render_retained_in_place` / `last_frame` stop the winit presenter copying
   the frame per present. Report: startup 27.5 ms, worst frame 13.1 ms, p95
   11.4 ms, latencies 1 interval, drift/trim/a11y/typography 0, PASS.
5. **One command runs every suite.** Workspace tests (**4,225 passed, 0 failed**,
   now `--no-fail-fast`), GPU planner, shaders, paint-native, Vulkan (37
   passed), GPU workload, census, premium UI, fixtures, all six stress/fidelity
   suites, `vieww-standard`, web baseline, plus measured gates. Skips are
   `SKIPPED(reason)` and `COMPLETE=false`; `VIEWW_CERT_STRICT=1` fails on them.
6. **The web verifier lives in the repository** (`examples/test-web/verify_web.py`).
   Run against the archive's prebuilt wasm and today's native baseline: both
   pointer states **EQUAL**, byte for byte, in headless Chromium.
7. **Release hygiene.** 26 `.orig`/`.rej`/`.bak` files (every rejected hunk
   checked as already present), a truncated copy of the archive nested inside
   itself, a `cargo metadata` dump with home paths, stale `cert-linux/`
   evidence, one-off patch scripts and build outputs removed.
   `ci/check/release-clean-check.sh` fails on any of them; `ci/release/package-source.sh`
   builds an archive only from a clean tree.

**Not verified, and why:**

- `suites/animation-stress` **fails its 60 Hz budget on this machine** (p95
  24.3 ms). The pristine uploaded source measures the same (p95 23.7–24.6 ms
  on the same machine), so it is hardware, not a regression — but it is a
  failure here, and the summary says `FAILED=1` for it.
- Web build and `wasm-check` (no `wasm32-unknown-unknown` target reachable),
  the desktop suite (no display), and `ci/certify/certify-windows.ps1` (no
  Windows) did not run.
- Nothing here ran on real GPU hardware. GPU timings above are lavapipe.

## The most recent effort: the web backend compiled, run, and certified byte-for-byte

The crate below (item 8 of the previous effort) said `vieww-platform-web` had
never been compiled. On 2026-09-14, on a machine that *can* reach
`static.rust-lang.org`, that changed — and then went three steps past it.

**Verified, by things that ran:**

1. **`ci/check/wasm-check.sh` passed for the first time.** The target installed
   normally, and the first honest clippy run on `wasm32-unknown-unknown`
   found seven `-D warnings` failures — all in `vieww-paint`, the crate the
   web backend builds against, all in code that had only ever been linted for
   the host target: two `#[expect]`s that do not fire on this target
   (`f32::from(u8)` is exact, and the misattributed one guarded no cast at
   all), a `contains_key`+`insert` pair, a `len() > 0`, two
   `needless_range_loop`s, and a `too_many_arguments` on `paint_shape`. All
   fixed; the gate is green, and `test-web` (below) holds it there with the
   same `-D warnings` on both targets.

2. **The web backend *ran*, and its pixels are provably the rasteriser's.**
   `examples/test-web` (new) renders one deterministic scene — gradients, a
   shadow card, a blurred circle-clipped layer, a rotated group, Latin + CJK
   text through the embedded fallback chain, and a live tap counter — through
   `WebApp` in a headless Chromium, and compares the canvas readback byte for
   byte against a native `render_to_pixels` of the same tree at
   `devicePixelRatio` 1. **Equal: all 2,304,000 bytes, in both pointer
   states** — after a real click, the canvas matches the native render of the
   tree at `taps = 1`, which is the pointer pipeline, the reactive rebuild
   and the rasteriser all agreeing across the wasm boundary. Three more taps
   move exactly the counter's pixels again; a wheel event does not kill the
   loop; the console is clean. `apps/viewwsite` (the product page, DOM
   backend with the canvas demo island inside it) loads, scrolls, reveals,
   and taps its counter in the same browser session, also clean.

3. **The workspace's own gates hold.** `cargo clippy --workspace --all-targets
   --features "vieww-paint/native,vieww-hal/vulkan" -- -D warnings` is green
   — it found fifteen more lints in the example suites that `cargo check` had
   silently passed (unused bindings left behind by refactorings, two
   `fn new` that returned `WidgetNode`, a `let_and_return`, two unaliased
   screen-array types, a match that wanted a guard, an `as u64` to `u64`);
   all fixed. `cargo fmt --all -- --check` is green too — the submitted tree
   was not fmt-clean (the fmt stage of `ci/check/checks.sh` was already red in the
   baseline), so this is a formatting-only pass over ~30 files, no semantics
   touched, all suites re-run green after it.

**Not verified, and unchanged from below:** the GPU mesh path's gates
(`vieww-gpu` scene completeness for real screens), and every non-Linux
toolchain. The wasm certification is Chromium-on-Linux; other browsers and
OSes inherit the byte-parity claim only insofar as they implement
`put_image_data` and `requestAnimationFrame` to spec.

## The effort before that: text on the GPU, and four gates that were not gates

Six things, and as always the split between verified and not is the point.

**Verified on this machine**, by tests that ran:

1. **The GPU path draws text.** `vieww_gpu::Planner` holds a glyph atlas
   (`vieww_gpu::atlas`) and plans a glyph run into one textured quad per glyph,
   batched into the same vertex buffer as the shapes around it;
   `vieww_hal::vulkan::SceneRenderer` uploads the atlas as an `R8_UNORM` image
   and samples it in `scene.wgsl`. `ScenePlan::is_complete` no longer refuses a
   frame for having a label on it, which is the difference between a GPU path
   that can draw no real screen and one that can.

   **Seven new tests in `crates/vieww-hal/tests/vulkan_text.rs`, all passing on
   `lavapipe`**, and their tolerance is the interesting part: **every pixel,
   within 1/255, including the antialiased rim of every glyph.** The shape
   parity suite compares two independent rasterizations and so can only assert
   about interiors; there is only ever *one* glyph rasterizer here — the GPU
   samples the coverage `vieww-paint` produced — so the only permitted
   difference is the atlas holding that coverage as `u8` instead of `f32`. A
   mirrored frame, a glyph one pixel out, a wrong atlas patch and a colour
   applied before coverage all fail that bound, and all of them fit inside
   "interiors match".

   Solid geometry samples a reserved full-coverage texel in the same atlas, so
   one pipeline draws a panel and the label on it in one `draw_indexed`; the
   asserted form of that claim is
   `a_label_on_a_panel_is_one_batch_and_still_matches`.

2. **A live Y-flip in `text.wgsl`**, found while doing the above. It mapped
   `y = 0` to NDC −1, which is the bottom of the framebuffer in the WebGPU
   convention `naga` adjusts from — the same defect this file already celebrates
   catching in `solid.wgsl`, still sitting in the shader library, for the reason
   that makes this class worth naming: **nothing had ever drawn through it**, so
   no test could disagree with it. Fixed. The whole-scene path is what renders
   text, and its parity tests use asymmetric text near a frame edge precisely so
   a flip cannot pass.

3. **`vieww new` still shipped the theme bug this file records as fixed.**
   `App::theme` landed, and `apps/viewwstudio`'s two templates adopted it — but
   `crates/vieww-build/src/scaffold.rs`, which is what `vieww new` writes and
   what `docs/guide/getting-started.md` documents as the way in, was left on
   `background(ThemeData::dark().colors.surface)` with no `Theme` mounted. So
   the first vieww code most people ever run still drew near-black text on a
   near-black ground, after the defect was fixed everywhere else.

   Fixed in the scaffold and in the guide.
   `the_scaffold_mounts_a_theme_it_also_paints_the_window_from` asserts on the
   generated source — the defect is a *missing call*, and only the source says
   whether a call is there, which is why the existing scaffold tests (all of
   which passed the whole time) could not see it.

4. **Five fixtures had layouts that did not fit, and the gallery knew.**
   `vieww_render::overflow::reported` has existed all along, with a doc comment
   saying a number "is what turns that from something a reviewer might notice
   into something a script can refuse" — and nothing read it. So the gallery
   printed `RenderColumn overflowed by …` on stderr, between the pictures, on
   every run, and `fixtures-out/20-settings.png` — the reference image for what
   a settings form looks like in this framework — showed its eighth row sliced
   through and its ninth missing.

   `runner.rs`, `motion.rs` and `interaction.rs` now count overflows per
   fixture, name the fixture, and `main.rs` exits non-zero. All five are fixed
   at the fixture: 20-settings 9 rows → 7, 31-stagger 8 → 5, the interaction
   list 12 → 10, `page_behind`'s spacing 10 → 8, the shared-element card 100px
   → 124px tall, and both faces of that transition wrapped in `Offstage` while
   fully faded — which also stops a 192px detail view being laid out into a
   60px box and painted outside the card meant to contain it. **The gallery
   now reports `no fixture's layout overflows`.**

   These change the gallery's timings, because they change what it draws. That
   is the correct trade and it is written down here rather than absorbed:
   a fixture's number is only comparable across a change if the fixture is
   drawing the right picture, and these were not.

5. **Two `web-sys` types were named and not enabled.** `vieww-platform-web`
   uses `web_sys::Event` (every listener closure takes one) and
   `web_sys::EventTarget` (both listener helpers coerce the window to one) and
   neither was in its feature list — two guaranteed `cannot find type` errors,
   parked behind a target install nobody could do. `ci/check/wasm-check.sh` now
   audits that list against the crate's own source **before** touching a
   toolchain, so this class is caught on any machine, and `ci/check/checks.sh` runs
   that stage on every run. Both features added.

6. **`ci/check/checks.sh` gained the two gates above** (the web-sys audit and the
   fixture gallery), and `ci/check/platform-check.sh` is new: one command per
   platform, each of which passes, fails at the compiler, or refuses to start
   naming the SDK, target or toolchain that is missing. See below.

7. **The docs gate was not covering the Vulkan backend.** `vieww_hal::vulkan`
   is behind a feature and the docs stage enabled only `vieww-paint/native`, so
   `cargo doc` never documented the module — the largest body of new code in
   the workspace had never been through that gate, and was carrying a broken
   intra-doc link (`super::VulkanDevice`, in `vulkan/scene.rs`'s module doc)
   the whole time the stage reported green.

   This file records `cargo doc -D warnings` as clean twice, at 117 links and
   then at 36, both times noting that the cause was nothing running the check.
   This is the third version of the same thing and a different shape of it: the
   check was running, on everything except the code anyone was actually worried
   about. Feature added to the stage, link fixed.

**Not verified, and the distinction is not cosmetic:**

- **No GPU number here was measured on real graphics hardware.** `lavapipe` is
  a software Vulkan implementation. What the text pipeline has is
  *correctness*, verified against the CPU rasterizer; it has no performance
  claim at all, and this file should not acquire one until it runs on a GPU.
- **Every platform other than Linux is still unbuilt**, unchanged from the
  position below. `ci/check/platform-check.sh` is the command that changes that; it
  cannot change it from this machine.

### What was actually run for the section above

Stated precisely, because "the tests pass" is the claim this file exists to
make checkable:

| gate | result |
|---|---|
| `cargo test -p vieww-foundation -p vieww-element -p vieww-widget -p vieww-render -p vieww -p vieww-text -p vieww-scroll -p vieww-accessibility` | 2,335 passed, 0 failed (72 binaries) |
| `cargo test -p vieww-paint -p vieww-gpu -p vieww-build -p vieww-hal` | 319 passed, 0 failed (17 binaries) |
| `cargo test -p vieww-hal --features vulkan -- --ignored` | 20 passed, 0 failed — 13 pre-existing, **7 new text tests** |
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings`, plus `-p vieww-hal --features vulkan` | clean |
| `cargo doc --no-deps -D warnings`, including `vieww-hal/vulkan` for the first time | clean |
| `cargo run --release -p fixtures` | 23 fixtures, 7 motion sequences, 2 interaction suites, **no layout overflows** |

**`apps/viewwstudio` was not re-run**, and neither was a single
`cargo test --workspace`. Not because either is expected to fail — nothing in
this effort touches the studio — but because this machine ran out of disk
building them (the workspace's debug artefacts exceed its allowance), and a
number this file did not observe is a number it should not print. The previous
effort's 608 studio tests stand as that effort's measurement, not as this
one's.

### `lavapipe` and concurrent devices

`crates/vieww-hal/tests/vulkan_scene.rs` began to **SIGSEGV inside
`libvulkan_lvp.so`** on roughly half of multi-threaded runs once the suite
started creating images and samplers. Every frame at the fault is lavapipe's
own; the deepest frame belonging to this workspace is an ordinary
`wait_for_fences`; the same eight tests pass every time under
`--test-threads=1`, and nothing in this crate shares a Vulkan object between
threads.

So it is serialised **in the tests**, where the constraint is, with a mutex and
the observation written beside it — rather than with synchronisation inside
`SceneRenderer` that would slow a real driver down to accommodate a software
one. Anyone adding a test to those files needs the guard; that is why the
explanation lives in the file rather than only here.

### One command per platform

`ci/check/platform-check.sh {linux|macos|windows|ios|android|web}` — or no argument
for every one this machine can attempt. Each row of the "never built on that
platform's own toolchain" status below now has exactly one command behind it,
and each does one of three things: passes (saying explicitly that compiling is
not running), fails at the compiler, or refuses to start and names the missing
prerequisite.

The third is what it is really for. A missing Rust target looks like a broken
checkout; a missing NDK looks like a Rust linker bug (see `ci/mobile/apk.sh` on
`-lunwind`); a missing Xcode looks like nothing at all until forty seconds in.
And a machine that cannot reach `static.rust-lang.org` gets `cargo --version`
failing under a twenty-six frame backtrace with a perfectly good toolchain on
disk — the same trap recorded below as costing a day in viewwstudio's own
toolchain test. The script checks for that first and tells you to set
`RUSTUP_TOOLCHAIN`.

## The effort before that one: nine things, seven verified

Kept in full rather than summarised, because the reasoning in it is still the
reasoning behind the code. Only the heading has changed: this was "the most
recent effort" until the section above it existed, and a file whose top two
sections both claim to be the newest is a file nobody can date.

Nine things were done. Seven are verified on this machine; two are not, and
which is which is the whole point of this file.

The verified half is verified by **2,873 passing tests** (2,265 across the
framework crates, 608 in the studio), `cargo fmt --all --check`, and
`cargo clippy --workspace --all-targets -- -D warnings` — all clean.

**Verified here:**

1. **The GPU path draws.** `vieww-gpu::scene` turns a `vieww_paint::Scene`
   into a batched `ScenePlan` — one vertex buffer, per-vertex colour, one
   draw call per scissor change — and `vieww_hal::vulkan::SceneRenderer`
   executes it on a real Vulkan device with a persistent pipeline (built
   once; only the target is rebuilt on resize, and the geometry buffers grow
   rather than reallocate). `lavapipe` was installed so this could be *run*
   rather than only compiled: **13 Vulkan tests pass**, five of them
   comparing the GPU's output against `NativeRenderer`'s pixel for pixel away
   from antialiased edges.
2. **A latent Y-flip in the shipped shader, found and fixed.** `solid.wgsl`
   wrote the Vulkan clip-space convention while `naga`'s SPIR-V writer was
   already flipping Y for it (`WriterFlags::ADJUST_COORDINATE_SPACE` is on in
   `Options::default()`), so every frame through it was vertically mirrored.
   It survived because the only test exercising it drew a square symmetric
   about the horizontal centre line. Both shaders now author in WebGPU's
   convention and let naga adjust per backend, and `vulkan_mesh_smoke`'s
   geometry is asymmetric in both axes so a flip cannot pass again.
   **The parity harness had the same class of flaw and it was caught the same
   way**: `interior_mismatches` originally decided "is this pixel on an edge"
   by asking whether a *neighbour also disagreed*, which returns zero when
   everything disagrees. Swapping red and blue in the planner passed three
   parity tests. It now keys off transitions in the reference image, and that
   mutation fails five.
3. **`#[widget]`.** A composed widget was four items before it said anything;
   `debug_name`, `kind` and the `widget_node_from!` call are now written by
   the attribute. 11 tests, each comparing the attribute's output against the
   hand-written long form rather than asserting about the macro in isolation.
   The scaffold and the `counter` example use it.
4. **The Apple colour schemes pass WCAG AA.** They failed seven pairs between
   them, and this file's previous version recorded that as a property of
   Apple's palette — true, and the wrong place to stop for a scheme the
   framework *ships as a default*. Both now use Apple's own published
   increased-contrast colours, plus a black label on the dark accent fills;
   see `ColorScheme::apple_light`'s doc for the numbers and
   `theme_audit.rs`'s for why pinning the failure was not good enough.
5. **The second text stack is gone.** `shaping`/`layout`/`selection`/
   `editing`/`bidi`/`fonts` — 2,262 lines reached by nothing, whose
   `layout::Paragraph` hard-coded a one-glyph-per-code-point `NaiveShaper`
   and whose crate-root re-exports shadowed `vieww-foundation`'s `FontFamily`
   and `FontWeight`. The `harfbuzz` feature went with it: its description
   claimed the default build lacked full Unicode shaping, which was never
   true of any build (`cosmic-text` shapes with `harfrust`).
6. **`task::Pool`.** `Threads`' own doc admitted it starts one OS thread per
   task; the advice was "bring a real runtime", which is wrong for an
   application whose async work is a dozen image loads. A bounded,
   dependency-free pool, with the panic-isolation and drop-joins-workers
   behaviour its doc claims each covered by a test.

7. **A test that failed for the wrong reason.**
   `viewwstudio`'s `the_host_environment_finds_the_targets_this_machine_has`
   guarded itself with `rustup --version` and then asserted on what
   `rustup target list --installed` returns. Those come apart on any machine
   that cannot reach `static.rust-lang.org`: `rustup` re-syncs channel
   metadata before answering a query, so `--version` exits 0 while
   `target list` downloads nothing — and the studio, which correctly reported
   an empty list, took the blame. The guard now runs the command the
   assertion depends on. 608 studio lib tests pass.

**Not verified, and the distinction is not cosmetic:**

8. **`vieww-platform-web` had never been compiled** — *closed 2026-09-14, see
   the top section.* This is the record as it stood: not "compiles but is
   untested" — never through `rustc` at all, because the machine it was
   written on could not install `wasm32-unknown-unknown` (`rustup target
   add` needs `static.rust-lang.org`, outside its network policy). Every
   dependency and the single module are gated to `cfg(target_arch =
   "wasm32")` so it builds to nothing everywhere else and cannot break a
   verified build, and it is `publish = false` for the same reason.
   `ci/check/wasm-check.sh` was the script that changed this; it found seven
   lints on its first honest run, then passed — and the backend has since
   been *run*, with its canvas certified byte-for-byte against the native
   rasteriser.
9. **Metal, D3D12, and every non-Linux platform** are unchanged from the
   position below: real code, unit-tested where the logic is testable on this
   host, never built on their own toolchains.

`cargo check --workspace --all-targets` and `cargo clippy --workspace
--all-targets -- -D warnings` are both clean, and the version moved to 0.1.0
— see `Cargo.toml`'s comment for what that does and does not claim.

## How to read the status column

- **Real, tested (CPU/host)** — real logic, real tests, passing on this
  machine, no further work needed to trust it on any host this workspace
  targets.
- **Real, tested (host) + GPU-pending** — the code path is real and its
  CPU-testable surface (construction, validation, CPU-side bookkeeping) is
  tested here; the GPU-facing half (a real device, a real shader compile, an
  actual frame) is not exercised by the host-only suite.

  Two corrections to what this row used to say. It named "an actual `wgpu`
  device", and **there is no `wgpu` in this workspace** — it went with vello,
  and the sentence outlived it; the device is an `ash`/Vulkan one. And it said
  the sandbox "has no GPU", which stopped being the operative limit once
  `mesa-vulkan-drivers` was installed: `lavapipe` is an ICD, the Vulkan suites
  run, and what is genuinely pending is a run on **real graphics hardware**,
  which is a performance question rather than a correctness one.
- **Real, tested (host) + other-toolchain-pending** — same idea, for a
  platform whose own toolchain (Xcode, an Android NDK, MSVC) is not present
  on this Linux sandbox: macOS/iOS/Windows/Android packaging paths, for
  instance. The code is real and unit-tested; it has not been exercised by
  an actual build on that platform.
- **Scaffolding only** — module structure and a doc comment exist; the real
  implementation is still to be written.

## Crates added or substantially extended in that effort

| Crate | What it is | Status | Verified with |
|---|---|---|---|
| `vieww-image` | Mipmaps, atlas packing, GIF sequences, color-space tagging, a residency cache | Real, tested (CPU/host) | `cargo test -p vieww-image` — 33 passed; clippy clean |
| `vieww-devtools` (extensions) | Frame timeline, render-graph/damage/memory/semantics/GPU inspectors, hand-rolled JSON export | Real, tested (host); GPU inspector's `DriverStats` is honestly `None`-always without a real driver | `cargo test -p vieww-devtools --all-features` — 63 passed; clippy clean |
| `vieww-build` / `vieww-cli` | `vieww new/doctor/build/run/test/profile/package` — real subprocess invocation, a real scaffold-and-compile integration test | Real, tested (host); macOS/Windows/Android packaging paths are real-but-unexercised on this Linux host | `cargo test -p vieww-build -p vieww-cli` — 47 + 21 passed (1 ignored by design — compiles a full scaffolded app, run with `--ignored`); clippy clean |
| `vieww-interaction` | Keyboard shortcuts, a command registry, outgoing-drag session state machine | Real, tested (CPU/host); `NullDragStarter` is an honest stub — no portable winit drag-start API exists (tracked upstream by winit issue #1550) | `cargo test -p vieww-interaction` — 28 passed; clippy clean |
| `vieww-scroll` | A draggable, proportional `Scrollbar` widget; `NestedScroll` drag-splitting between an inner and outer scroll position | Real, tested (CPU/host) | `cargo test -p vieww-scroll` — 18 passed + 1 doctest; clippy clean |
| `vieww-accessibility` | WCAG contrast math, static semantics-tree audits (missing labels, undersized touch targets, internal-consistency checks), theme contrast scanning | Real, tested (CPU/host) — and found a real property of the framework's own built-in Apple color schemes (they fail several AA pairs) rather than tuning the test to hide it | `cargo test -p vieww-accessibility` — 27 passed; clippy clean; `cargo doc` clean; example runs |
| `vieww-plugin` / `vieww-plugin-macros` | Stable, versioned, cross-`rustc` plugin ABI: `#[repr(C)]` vtables, a safe `Plugin` trait, `#[vieww_plugin]` codegen, `PluginRegistry` (`libloading`-based) | Real, tested end to end — including a real `cdylib` fixture plugin built with a genuine `cargo build` subprocess and `dlopen`ed for real across the actual compiled boundary | `cargo test -p vieww-plugin -p vieww-plugin-macros` — 15 + 5 passed; clippy clean |
| `vieww-test-harness` (extensions) | `criterion`-based benchmarks over real `TestHarness` operations; a `TestReport` with hand-rolled JSON/HTML rendering | Real, tested (CPU/host) | `cargo test -p vieww-test-harness` — 21 passed; `cargo bench -p vieww-test-harness --no-run` compiles, and a `--test` smoke run executes all three benchmarks successfully; clippy clean |
| `vieww-widget` (tokens + tiers extension) | `WindowSizeClass` (the Compact/Medium/Expanded breakpoints, published via the existing `Inherited<T>` mechanism), an open `DesignToken<T>`/`TokenSet` registry, and `WidgetTier`/`Tiered`/`TierBudget`/`TierGate` (the assessment's own Primitive/Behavior/Pattern widget classification, made actionable) | Real, tested (CPU/host); a fourth planned item — a dedicated "container query" widget — was deliberately not built, since `LayoutBuilder` (pre-existing) already is one; documented in both types' own docs rather than duplicated | `cargo test -p vieww-widget --lib` — 505 passed (25 new across both efforts); clippy clean; `cargo doc` clean; both `tiers` doctests pass |

## Naming note, not a gap

An earlier architecture sketch named separate `vieww-window`/`vieww-native`
crates. That functionality already exists under different names —
`vieww_render::window`, `vieww-platform-winit`, and
`vieww_foundation::capability::platform_view` — and a risky rename/extraction
purely to match a proposed name was deliberately not done. If the naming is
ever revisited, do it as its own tracked refactor, not folded into unrelated
feature work.

## Cross-backend parity — a deliberate non-goal, not a gap

The original architectural assessment's "cross-backend parity" suite assumed
two independent rendering backends that could disagree with each other. This
workspace does not have that shape: the vello-based second backend was
removed earlier in that effort, and `vieww`'s own `NativeRenderer` (CPU) plus
the Vulkan HAL path (there is no `wgpu` here; that word was left behind by
vello) are one implementation compiled two ways, not two implementations.
There is nothing to compare "across" — parity-testing a renderer against
itself would only restate whatever bugs it already has as if they were
agreements.

**Except in one place, and it is the exception that proves the rule.** The GPU
path's *geometry* is `lyon` tessellation and hardware sampling where the CPU
path's is analytic scanline coverage, so those genuinely are two
implementations of the same question and `tests/vulkan_scene.rs` compares them
— with a tolerance that allows the edges to disagree, because they must. Its
*text* is not: both sides use the same rasterised coverage, so
`tests/vulkan_text.rs` can and does demand every pixel within 1/255. Which
kind of comparison is available depends on whether there are really two
implementations underneath, which is the whole point of this section. What `vieww-test-harness::visual` provides instead,
and what genuinely covers the "golden suite" half of item 13, is
`assert_matches_golden` (`crates/vieww-test-harness/src/visual.rs`): a real
PNG-baseline image-diff oracle plus `assert_partial_repaint_is_complete`, a
damage-region oracle. If a second real backend is ever added, parity testing
belongs here as new golden baselines shared across both, not as a separate
mechanism.

## Widget tiers — done

Item 11 names "design-system engine + adaptive layout + widget tiers" as one
line. This effort originally built only the design-system/adaptive-layout
two thirds of it (`vieww-widget::tokens` — `WindowSizeClass`,
`DesignToken<T>`, `TokenSet`; `LayoutBuilder`'s existing container-query
behavior documented rather than duplicated) and left "widget tiers"
unaddressed, because at the time no concrete meaning for "tier" had been
pinned down against source.

It has since been built, against the actual source: the external
architectural assessment this workspace was built from names, in its own
§9, exactly three widget levels — "Primitive widgets" (`Box`/`Flex`/`Stack`/
`Scroll`/`Text`/`Image`/`Transform`/`Clip`/`CustomPaint`), "Behavior
widgets" (`Focusable`/`Hoverable`/`Draggable`/`Dismissible`/`Scrollable`/
`Animated`/`Transition`/`Portal`/`Overlay`/`Semantics`), and "Design-system
widgets" — closing with "ViewW should allow custom design systems without
needing to become forked widgets." `vieww-widget::tiers` (new) is that
classification applied to this crate's real widget catalog:
`WidgetTier::{Primitive, Behavior, Pattern}`, a `Tiered` trait each widget
type implements once, and `TierBudget`/`TierBudgetProvider`/`TierGate` — the
same ambient-publish-and-read mechanism `WindowSizeClass` already
established — so a `Pattern`-level widget a design system built from this
crate's primitives and behaviors can name a simpler fallback and degrade to
it under a tight budget, without forking anything here to do so. 14 new
tests, all passing; clippy clean; both doc examples compile and run.
`cargo test -p vieww-widget --lib` — 505 passed (was 491); clippy clean.

## The `vieww_beta_0.3` release archive, and what it did not contain

The archive this workspace was released as contains **1,224 entries and no
dotfiles at all**. That is what an archive built without them looks like, and it
is not cosmetic:

- **`.cargo/config.toml` was missing.** `Cargo.toml`'s own `[profile.release]`
  comment says, at length, that `-C prefer-dynamic` "has to be set as a
  rustflag, so it lives in `.cargo/config.toml`" — and it is what makes the
  studio and the preview `cdylib` it `dlopen`s share one `libstd`, which is what
  makes `catch_unwind` catch a panic raised inside somebody's screen. Without
  the file, every build produced a studio whose panic boundary aborts the
  process: `fatal runtime error: Rust cannot catch foreign exceptions`, SIGABRT,
  the window and the unsaved buffer gone. `apps/viewwstudio/tests/pipeline.rs`
  reproduced it exactly, and did so on the pristine archive as well as on a
  modified tree, which is how it was confirmed as shipped rather than
  introduced. The file is restored, with the reasoning written into it.

Anything else that lived in a dotfile — a CI workflow, an editor config — is
gone from the archive the same way and has not been reconstructed, because
there is no record here of what it contained.

## `ci/check/checks.sh` had not been able to run since the vello removal

The docs stage passed `--features "vieww-paint/gpu,vieww-paint/hybrid,vieww-paint/cpu"`.
Those three features were `vieww-paint`'s vello-based backends and they were
deleted with vello. Cargo does not warn about an unknown feature on the command
line; it fails resolution outright, so that stage and **every stage after it**
— MSRV, licences, the test suite, the end-to-end reload suite, the packaged
studio — had not executed since. Fixed to `vieww-paint/native`.

Separately, the test stage ran a plain `cargo test --workspace`, which does not
enable `vieww-paint/native` — so the 224 tests covering the renderer were not
failing in CI, they were not being compiled. The stage now enables it, and the
four test targets that need it declare `required-features` so that
`cargo test -p vieww-paint` skips them cleanly instead of failing to compile
with `unresolved import vieww_paint::native`.

## `ci/check/checks.sh` was failing at its second stage, not its fourth

Above, the docs stage is described as unable to run since the vello removal.
It never got that far. `cargo fmt --all -- --check` sits earlier in the same
script, and **119 files** had drifted out of `rustfmt`'s shape — so the script
stopped there, and everything after it (clippy, build, docs, MSRV, licences,
tests, the end-to-end suites, the packaged studio) had not executed either.

The workspace is now formatted and the gate passes. Note for anyone re-running
it: `examples/fixtures/src/screens.rs` needs **two** `cargo fmt` passes to reach
a fixed point, because it contains a `children![]` macro invocation whose
contents `rustfmt` re-flows only once the surrounding call has been collapsed.
That is a `rustfmt` behaviour, not a repository one, and `cargo fmt --all` twice
is the whole workaround.

## The size budget was recorded on a laptop and first measured on CI

`ci/check/size-budget.txt` held **544880 bytes** for `x86_64-unknown-linux-gnu`. The
first time the gate ran on CI it measured **593128** — 8.9%, past the 5%
ceiling — and it measured the same figure on every subsequent run, to the byte.

That reproducibility is what settles what it means. A budget is per-toolchain
and per-target by construction, and the script's own header says so: "a
different rustc, a different libc, or a cross build produces a different size
for identical source". The recorded number came from a developer machine; this
one comes from CI's `rustc 1.98.1` on the runner's libc. The two were never
comparable, and the gate had no way to say so because the *triple* matched —
which is the one thing it does check.

So the figure is re-recorded at **593128** against the machine that will be
doing the measuring from now on, and the 8.9% is not attributed to anything,
because there is nothing yet to attribute it to. What that buys is a gate whose
next red is a real one: a second CI figure that moves 5% off this baseline is a
change in this repository, not a change of laptop.

**What this does not do** is establish that the framework did not grow. If that
question matters, `cargo bloat --release -p vieww --example counter` on a runner
answers it, and this section should be replaced with the answer rather than
extended.

## The 56 feature examples had silently opted out of the workspace MSRV

`Cargo.toml` declares `rust-version = "1.85"` under `[workspace.package]`, and
every crate under `crates/` and `apps/` inherits it with
`rust-version.workspace = true`. The 56 crates under `examples/features/` did
not. They inherited `version` and `edition` and stopped there.

Nothing announced that, and two gates were quietly weaker for it.

**Clippy suggests APIs that do not exist on the declared minimum.** Clippy reads
`package.rust-version` to decide whether an MSRV-gated lint may fire. With no
`rust-version`, it assumes none, and
`examples/features/54-image-mipmaps-atlas/src/main.rs:45` was told to replace
`chunks_exact_mut(4)` with `as_chunks_mut::<4>().0` — an API stabilised in Rust
**1.88**, three releases after the minimum this workspace promises. Taking that
suggestion would have passed the clippy stage and then failed the MSRV stage of
the same script, in a different crate, for a reason the error message would not
have connected to it.

The tell was that the workspace has **seventeen** `chunks_exact` call sites and
exactly one was reported. The other sixteen are in crates that inherit the MSRV,
where clippy correctly held the lint back.

**And `ci/check/checks.sh`'s MSRV stage was checking them without meaning to.** It runs
`cargo check --workspace --all-targets` on 1.85, so the examples were compiled
against the minimum all along — but by accident of `--workspace` rather than by
a claim in their manifests, and with no lint respecting it.

Fixed by adding `rust-version.workspace = true` to all 56. No code changed; the
lint stops firing because it should never have fired.

## `ttf-parser` is unmaintained, and there are two font parsers in the binary

`cargo deny check advisories` reports RUSTSEC-2026-0192: the author of
`ttf-parser` has said it will not receive further fixes, and the advisory states
plainly that there is no safe upgrade. The named alternative is `skrifa`.

It is a **direct** dependency, not an inherited one: `vieww-text` takes it
unconditionally, and `vieww-paint` takes it behind `native` — the only renderer
this framework has, so on every real build.

**It is also unreachable to remove.** Two paths arrive without anyone here
choosing them:

    ttf-parser <- fontdb           <- cosmic-text <- vieww-text
    ttf-parser <- owned_ttf_parser <- ab_glyph <- sctk-adwaita <- winit

The second is winit's Wayland client-side decorations. A complete port of our
own code would leave both standing and the advisory still firing, which is why
`deny.toml` carries an `ignore` for it rather than a TODO. The entry says to
delete it the day either path goes away.

**Worth doing anyway, for a different reason.** `skrifa` 0.44 is *already* in
the dependency graph — `cosmic-text` and `swash` both use it. So this workspace
currently links two independent font parsers: skrifa, through the text stack,
and ttf-parser, through `vieww-paint`'s glyph outlines. Moving `vieww-paint`
onto skrifa adds no dependency and deletes one parser's worth of code from every
binary.

That is an open question against the size budget recorded in the section above.
The counter example measured 8.9% over a figure taken on another machine, and it
was re-recorded rather than attributed. Two font parsers is the first place to
look if anyone attributes it later: `cargo bloat --release -p vieww --example
counter` will say whether ttf-parser is in the counter's binary at all.

## `rust-version = "1.85"` was a promise nothing could keep

`[workspace.package]` declared 1.85. The workspace has never built on it. The
first time `ci/check/checks.sh`'s MSRV stage actually ran — it had been unreachable
behind earlier stages for months — cargo refused before compiling a line:

    cosmic-text@0.19.0 requires rustc 1.89
    smol_str@0.3.6     requires rustc 1.89
    image@0.25.10      requires rustc 1.88.0
    naga@29.0.4        requires rustc 1.87
    zbus / zvariant / zcheapstr  require rustc 1.87

`cosmic-text` is the text stack: not optional, not a dev-dependency, not behind
a feature. So the figure on crates.io was wrong for every consumer, in the
direction that costs them a build failure rather than a warning.

Now **1.89**, the maximum over the tree. Lowering it later is real work — pinning
`cosmic-text`, `image` and `naga` back to releases predating their bumps — and
worth doing only if someone needs it. Nobody has asked.

**The number now lives in one place.** `ci/check/checks.sh` already read it from
`Cargo.toml`; `.github/workflows/ci.yml` hard-coded `1.85` in two steps and would
have gone on installing a toolchain the workspace no longer claimed. Both steps
now read it with the same `sed`, so the three cannot disagree again.

### The lints the false MSRV was suppressing

`slice::as_chunks` stabilised in 1.88, so at a declared 1.85 clippy held
`chunks_exact_to_as_chunks` back everywhere. At 1.89 it fires — on **seventeen**
`chunks_exact` sites, every one of them a per-pixel loop: `vieww-effects`' blend
and matrix passes, `vieww-paint`'s target readback, `vieww-image`'s atlas and
mipmap chains, `vieww-hal`'s swapchain copy.

It is allowed workspace-wide for now, in `[workspace.lints.clippy]`, with the
reasoning at the site. Briefly: the rewrite is not the substitution the lint
suggests — `as_chunks_mut::<4>()` returns a `(&mut [[u8; 4]], &mut [u8])` pair,
so a zip over two of them changes shape as well as type and the remainder slice
has to be used or explicitly dropped. Seventeen of those at once, in the code
that writes every pixel, on a style lint, is how a blend bug gets in. Do them
deliberately, with the parity and retained-repaint suites as the check.

**`manual_is_multiple_of` is the same story**, from `is_multiple_of`'s
stabilisation in 1.87: **thirty-nine** `% n == 0` sites, from the leap-year rule
in `vieww-foundation` and the blur kernel's odd-size rounding, through
`vieww-paint`'s dash phase, to a long tail of `index % 2 == 0` row striping in
tests and gallery screens. Also allowed, also its own change — and only the
*unsigned* sites can move, because the method does not exist for signed
integers.

**Expect others.** Clippy holds back every lint that suggests an API newer than
the declared minimum, so four editions' worth of them switched on together when
the figure was corrected. The way to find the rest is one local run, not one
push per lint:

    cargo clippy --workspace --all-targets --keep-going \
      --features "vieww-paint/native,vieww-hal/vulkan" 2>&1 \
      | grep -oP 'clippy::\K[a-z_]+' | sort -u

Note the absent `-- -D warnings`: they stay warnings, nothing aborts, and the
whole set arrives at once.

### One warning left standing, on purpose

    warning: output filename collision at target/doc/vieww/index.html
    the bin target `vieww` in `vieww-cli` has the same output filename as
    the lib target `vieww` in `vieww`

`vieww-cli`'s binary is named `vieww`, which is the right name for it to have.
This is cargo#6313 and it affects the generated docs directory only. It is a
cargo warning rather than a rustdoc one, so `RUSTDOCFLAGS="-D warnings"` does
not turn it into an error and the stage is green. Renaming the binary to silence
it would be the tail wagging the dog.

## The gate could go red without anyone touching the repository

`rust-toolchain.toml` said `channel = "stable"`. That is not a version; it is
whatever rustup resolves on the morning the job runs. `ci/check/checks.sh`'s clippy
stage passes `-D warnings`. Together those two lines mean a new clippy release —
every six weeks, on somebody else's schedule — turns unchanged code into a build
failure.

That is not a hypothetical. It is the entire recent history of this gate. Once
the earlier blockers cleared and clippy could finally reach the workspace, it
failed in sequence on `manual_clamp`, `chunks_exact_to_as_chunks`,
`unused_variables`, `unused_imports`, and `manual_is_multiple_of` — **not one of
them from a change to the code they fired on**. Each cost a full CI run to learn
a single line, because `-D warnings` also stops the build at the first finding.

Three separate things made that a loop, and all three are now closed:

1. **The toolchain floated.** `channel` is pinned to `1.98.1`. The lint set is
   now a property of this checkout, not of the calendar. Upgrading is a commit:
   bump the number, run the gate, deal with the new lints in one deliberate
   change.

2. **The gate stopped at the first finding.** `--keep-going` on the clippy,
   build and docs stages. One run now names everything it can, instead of one
   thing per push.

3. **The declared MSRV was false, which silently suppressed lints.** 1.85 was a
   figure nothing could build on; correcting it to 1.89 switched on every lint
   that suggests a 1.86–1.89 API, all at once. See the section above.

**And the gate was being run in the wrong place.** `ci/check/checks.sh` is a shell
script with no CI dependencies — the loop above was expensive only because each
iteration went through GitHub Actions. It costs nothing locally:

    cargo fmt --all && ci/check/checks.sh --quick

Push when that prints `all quick checks passed`. CI is there to catch what a
machine other than yours disagrees about, not to tell you about an unused
import.

## The studio's panic boundary is configured in an untracked file

`viewwstudio`'s `tests/pipeline.rs` aborted CI with

    fatal runtime error: Rust cannot catch foreign exceptions, aborting
    process didn't exit successfully (signal: 6, SIGABRT)

in `a_panicking_build_on_a_later_rebuild_is_caught_by_the_host_not_aborted` —
taking all twenty-one tests in the file down with it, since an abort is not a
test failure.

The mechanism is the one `loaded.rs` documents at length, arriving from the one
direction it does not: **the host half was missing.** Guest and host must share
one `libstd` or a panic raised in a loaded preview is a foreign exception to the
host's unwinder. `apps/viewwstudio/src/compile.rs` passes `-C prefer-dynamic`
for the guest explicitly, in code, and that half works. The host half comes from
`.cargo/config.toml`, and on CI it was not applying.

Two things ruled out first: the workflow sets `rustflags: ""` on
`actions-rust-lang/setup-rust-toolchain`, which that action documents as leaving
`RUSTFLAGS` *unset* precisely so `target.*.rustflags` still applies; and the
guest is compiled with the flag regardless of any config, from `run_rustc`. What
is left is the file itself.

**`.cargo/` is a common `.gitignore` entry.** That is exactly how a file ends up
present on the machine it was written on and absent from every clone — including
the one CI makes. It is worth confirming with `git ls-files .cargo/`; an empty
answer is the whole explanation.

**It is now written and committed**, reconstructed from the scripts and module
docs that describe it, because CI proved the absence rather than suggesting it:
the preflight below tripped on a clean checkout. It carries three things —

  * `-C prefer-dynamic -C rpath` for the Linux-gnu and macOS hosts, `cfg`-scoped
    so it cannot reach an Android or iOS cross-build, where a dynamic `libstd`
    is a link error against a file the NDK does not ship. Windows is absent on
    purpose: MSVC ships no dynamic `std`, which is why `ci/check/platform-check.sh`'s
    Windows branch says the boundary there needs an out-of-process preview.
  * the Android linker shims (`ci/mobile/ndk-clang-{aarch64,x86_64}.sh`) and the `adb`
    test runner, per `ci/mobile/ndk-clang.sh` and `ci/mobile/adb-runner.sh`.
  * the iOS simulator runner, per `ci/mobile/simctl-runner.sh`.

Verified rather than assumed: with the file in place, `cargo build -v` passes
`-C prefer-dynamic -C rpath` to rustc, and `ldd` on the result resolves
`libstd-<hash>.so` through the recorded runpath.

**If an untracked copy exists on a working machine, merge — do not overwrite.**
This reconstruction covers what the checked-in scripts document; a local file
may have accumulated more.

**`ci/check/checks.sh` now checks for it before the test stage.** Not because a check
script should second-guess a checkout, but because of how this failure presents:
a SIGABRT with a message about the unwinder, in the middle of the suite that
exists to prove the unwinder works. It reads as a bug in the panic-catching code
rather than as three missing lines of configuration, and it costs whoever meets
it an afternoon in the wrong file.

**The same rule bites from a second direction.** Cargo takes rustflags from
exactly one source, so *setting `RUSTFLAGS` at all* — even to an empty value —
discards `target.*.rustflags` wholesale and removes this boundary. That is what
`ci/check/coverage.sh` had to work around for `cargo-llvm-cov`, which exports
`RUSTFLAGS` to add its instrumentation, and it is worth knowing before adding a
`RUSTFLAGS` to any shell profile or CI step.

## Documentation links

`cargo doc` with `-D warnings` — the gate that had not run — reported **36**
broken intra-doc links across ten crates. Most were links into the vello-era
`gpu`, `hybrid` and `cpu` modules and into items that moved or became private
with them; the rest were `[`Thing`](Thing)` forms that `rustdoc` now flags as
redundant. All 36 are fixed: a link whose target still exists was repointed, and
one whose target is genuinely gone became plain code formatting rather than a
link that renders on docs.rs as literal brackets.

This is the second time this crate has accumulated a batch of these — the
stage's own comment records the first, at 117 — and both times the cause was the
same, which is that nothing was running the check. That is now true again only
if somebody breaks the script again.

## Rendering performance

The CPU rasterizer is the renderer for everything, and it was costing about
100 ms for a full studio frame. Measured against the pristine archive on the
same machine, interleaved runs, minimum of four, over `examples/fixtures`:

| fixture | before | after | |
|---|---|---|---|
| `22-editor` (an IDE shell) | 20.41 ms | 11.51 ms | 1.77x |
| `11-typography` | 7.95 ms | 4.10 ms | 1.94x |
| `11-code-block` | 6.72 ms | 3.85 ms | 1.75x |
| `21-dashboard` | 10.41 ms | 6.29 ms | 1.66x |
| `20-settings` | 12.33 ms | 7.77 ms | 1.59x |
| `23-editor-glass` | 43.53 ms | 29.26 ms | 1.49x |
| **whole gallery** | **304.69 ms** | **238.86 ms** | **1.28x** |

`00-strokes` and `00-curves` measure 0.97x and 0.99x — untouched code paths
coming out flat is what says the rest of the table is the change and not the
machine.

> **This table is a before/after of a renderer change, and three of its
> fixtures have since been edited.** `20-settings` lost two rows, and the
> interaction screens lost rows too — see the layout-overflow entry at the top
> of this file, which is why. A fixture that draws less is faster for a reason
> that has nothing to do with the rasterizer, so the "after" column above is
> **not** comparable with what the gallery prints today and must not be
> updated in place to look like it is; the pairs in it were measured against
> each other, on one machine, on identical content.
>
> The current absolute numbers, from a run after those edits, are in the next
> section. Anyone repeating this experiment should re-measure both columns.

For a noise-free figure on the studio's own frame, `callgrind` over
`apps/viewwstudio/examples/frame_cost` (605 commands, 323 shapes, 1,447 glyphs,
13 shadows, 3 layers at 1440x900): **7,000,560,220 instructions before,
4,035,509,557 after — 1.73x fewer**, and 62.9 ms to 48.9 ms of wall clock for a
full repaint of the whole window. Wall clock improves by less than that,
because what is left is increasingly memory-bound rather than instruction-bound:
after the change the frame's largest remaining costs are `paint_shape` (35%),
`composite_layer` (11%), `Premul::to_straight_u8` (10%) and clearing buffers
(5%), and the last two are proportional to the *window*, not to the scene. Both
of those also mostly disappear on a damaged repaint, which converts and clears
only the rows it redrew — the live window's path, and not what this benchmark
measures.

What changed, in order of what it was worth:

- **`[profile.dev]` did not optimise the rasterizer.** `[profile.dev.package."*"]`
  covers non-workspace dependencies; the rule was written when vello and wgpu
  did the pixel work, and `vieww-paint` is a workspace member. Every debug build
  since the migration ran the framework's hottest loop at `opt-level = 0`.
  Measured on `vieww-paint`'s own `perf_probe`: **54.5 ms → 7.3 ms, 7.5x.** This
  is what a debug `viewwstudio` was actually suffering from, and it presented as
  a studio that appears to hang on a click, an animation that plays two of its
  frames, and a splash screen that never fades.
- **A rasterised-glyph cache** (`native/glyph_raster.rs`). A studio frame
  contains 1,447 glyphs and every one of them was re-outlined, re-placed,
  re-flattened and re-scanline-rasterised every frame. Keyed on exact bits
  rather than a quantised phase; see the module doc for the 57-pixels-by-1/255
  difference this does introduce and why it is written down rather than rounded
  to "none".
- **Gradients resolved once per shape** rather than per pixel — 8.7% of a whole
  frame, by callgrind, on a mostly flat interface.
- **A closed-form path for axis-aligned rectangles**, reproducing the scanline
  pass's own vertical quantisation exactly so a rectangle is the same bytes
  either way, and asserted as such over a sweep of sub-pixel offsets.
- **Layers composite only what they inked**, not their declared bounds.
- **Blurs run over the inked band** and the vertical pass carries a strip of
  columns. See `native/effects.rs`'s `vertical` for the measurement — and for
  the warning attached to it, which is that a single before/after sample on this
  machine gave the opposite answer and was nearly acted on.
- **The blend mode is decided once per draw, not once per pixel.** Every
  composite loop already computed a `normal` flag from the mode and the colour
  pipeline, and then went on to call `blend_with_pipeline` — which matches on
  the pipeline, then on twenty-eight blend modes — for each pixel anyway. A
  callgrind profile of `23-editor-glass` put **22% of the whole frame** in that
  dispatch, for arithmetic that is four multiplies and four adds. See
  `native/color.rs`'s `over`.
- **The gradient's device mapping is folded into its ramp.** The inverse
  transform, the shape's bounds, the two divisions and the geometry `match` were
  all per pixel and all constant per shape; for a linear gradient the whole
  composition is affine, so it collapses to one multiply and one add with the
  row's term hoisted. See `native/gradient.rs`'s `DeviceRamp`.
- **`-C target-cpu=x86-64-v2`.** The default `x86-64` baseline is SSE2 and has
  no `roundss`, so `f32::floor` and `f32::ceil` were *calls into
  `compiler-builtins`*. Worth **27.4 ms to 22.7 ms** on `23-editor-glass` by
  itself. See `.cargo/config.toml` for why v2 and not v3.

- **2026-10-12: explicit SSE2 in the three hottest loops, and the layer
  clear fused into the composite.** The output conversion (four pixels per
  16-byte store), `composite_flat`'s and `composite_layer`'s normal-blend rows
  (`native/simd.rs`), and `Target::composite_layer_and_clear`, which zeroes a
  layer as it reads it so the pool has nothing left to clear. Each is held to
  the scalar path bit for bit by its own test (every `NaN` counted as one
  value — see `simd.rs`'s module doc for why that is the only freedom
  taken), and the whole gallery's 30 outputs are byte-identical before and
  after. callgrind: glass 1,454 M → 1,172 M, nested 1,573 M → 1,005 M; the
  gallery 232.7 ms → 164.6 ms in one run each. The flat row keeps two
  branches on purpose: folding them into selects made glass 10% *worse*
  (1,301 M → 1,431 M), because every opaque interior pixel then paid for a
  blend it threw away.
- **`-C target-cpu=x86-64-v2` restored.** `.cargo/config.toml` was rebuilt
  from scratch when an archive arrived without it (`375aa13`), and the
  rebuild did not know about this flag; `floorf`/`ceilf` were back in the
  profile by name. On this session's machine it was worth ~3%, not the 17%
  measured originally — the other changes since had removed most of the
  calls.

### The two fixtures still over budget, and what that does and does not mean

`00-layers-nested` (24 isolated layers, a deliberate compositor-depth stress)
and `23-editor-glass` (the IDE shell plus a blurred palette, a full-surface
scrim and real elevation) render a **full** frame in about 25 ms and 22 ms
against a 16.7 ms budget. Both were about 45 ms.

Those two are the only ones left: the most recent full run reports **23
fixtures, 208 ms of rendering in total**, and every other fixture inside the
budget. (`20-settings` reads 7.2 ms rather than the 7.77 ms in the table above
because it now draws seven rows instead of nine — see the note there.)

The gallery measures full repaints, and that is the right alarm for a first
frame, a resize and a theme change — all of which genuinely redraw everything.
It is not what a live window does when something changes, and for as long as it
was the only number it stood in for a claim nobody had checked: that these
screens cannot hold 60 Hz.

So `examples/fixtures`' interaction suite now measures the glass screen too —
the same screen, one signal-driven control over it, changed:

```
interaction                       damage  full frame    retained  speed-up  identical?
glass: highlight moves             0.72%     21.25ms      9.85ms      2.2x  yes  within 60Hz
glass: highlight clears            0.72%     21.88ms      9.99ms      2.2x  yes  within 60Hz
```

(Re-measured after the layout-overflow fixes. The full-frame column moved
because the screen behind the palette changed — `page_behind`'s spacing and the
list's row count — not because the renderer did. The *claim* is unchanged and
is the one asserted on every run: byte-identical, and inside the budget.)

Byte-identical to the full repaint, and inside the frame budget. That is the
measurement that was missing, and it is asserted on every run rather than
described here — a retained repaint of a screen with a blurred backdrop, real
shadows and a clip stack is exactly where a region repaint goes subtly wrong.

**What would take the full-repaint numbers further** is explicit SIMD in the
compositing loops, or a GPU. The scalar loops are close to their limit: the
frame is roughly 200 million instructions for about four million blended
pixels, and the remaining costs are spread across compositing (22%), the
output conversion (16%), layer resolve (8%) and blur (7.5%) rather than
concentrated anywhere a single change would reach.

### Four optimisations that were tried, measured, and removed

Written down because each is the obvious next idea, and because the pattern in
them is the point.

- **A reduced-resolution blur.** Downsample, blur small, bilinear upsample —
  what Core Animation, Skia and Chrome all do. Implemented in full, including
  the variance-corrected sigma. Blur turned out to be 7.5% of the frame it was
  supposed to rescue; end to end it moved `23-editor-glass` from 32.21 ms to
  32.46 ms, and its upsample's `f32::floor` calls put 2.9% of the frame into
  `compiler-builtins`. Removed; see `native/effects.rs`.
- **An opaque-row `copy_from_slice` in `composite_layer`**, with a matching
  full-coverage skip in `composite_flat`. Made `23-editor-glass` 1.7% *worse* —
  the alpha scan is a second pass over the row and the blend it replaces is
  already a few instructions. Removed; see `native/target.rs`.
- **`as_chunks_mut::<4>()` instead of `chunks_exact_mut(4)` and
  `copy_from_slice`** in the output conversion, to drop a length check per
  pixel. 1,203,539,448 instructions against 1,203,539,565 — a difference of 117
  across a whole render. LLVM had already done it. Reverted.
- **A strip-wise vertical blur pass**, which *was* kept — but only after a
  second look. One run of each said it was slower and it was reverted on that
  basis; six runs of each said it was 26% faster. See `native/effects.rs`'s
  `vertical`, and `examples/blur_bench.rs`, which exists so the next person
  does not repeat it.

The common thread: on this machine the run-to-run spread on the same binary is
wider than most of these effects. A single before-and-after is not evidence, and
`callgrind`'s instruction count — deterministic, and the only reason the
`copy_from_slice` and blur results were unambiguous — is not the whole story
either. `00-fills-rrect-clip` regressed 20% in wall clock while executing **15%
fewer instructions**, because building the coverage mask with
`Vec::with_capacity` and pushing writes zeros that `vec![0.0; n]` gets from the
allocator for free. Both numbers, every time.

## Baked-in choices removed

A framework is generic to the extent that an application can replace what it
does. Four places decided something on the application's behalf that the
application had every right to decide, and in each case the mechanism for
deciding it already existed and was simply not reachable.

**Text layout constructed its own shaper.** `vieww-text`'s crate doc calls
`shaping`/`layout` "a shaper-agnostic interface" and `layout.rs`'s own header
says the bidi step "is what makes Arabic work, and it cannot be skipped".
`Paragraph::layout` built a `NaiveShaper` — one glyph per code point, monotonic
advance — inside itself, so no caller could pass a real one and
`HarfBuzzShaper` was unreachable from the layout pass however it was
configured. A trait with one hard-coded implementation is not an abstraction.
Now `Paragraph::layout_with(shaper, fonts, …)` takes it, and `layout` is that
call with the naive default named at the call site.

Two smaller versions of the same thing went with it, both silent:

- Every run was shaped as `FontHandle(0)` regardless of the span's
  `font_family`, while `FontCache::resolve` had existed the whole time to
  answer exactly that question.
- Every run was shaped with `&[]` for its OpenType features regardless of the
  span's `features`, so a span asking for `"liga" off` was given ligatures
  anyway.

Four new tests cover these, each of which could not previously have failed
*or* passed: the supplied shaper is the one that shapes, a span's features
reach it, a span's family is resolved through the cache, and the size an
unsized span gets comes from `LayoutOptions` rather than from `unwrap_or(16.0)`
in the middle of the pass. `1.2` and `0.8` in the line-box arithmetic are now
`NORMAL_LINE_HEIGHT` and `BASELINE_FRACTION`, the second of which says out loud
that it is an approximation standing in for font metrics the `Shaper` trait
does not expose.

`Paragraph` also now stores its `LayoutOptions` whole rather than three of the
five fields, because `relayout` had to invent the rest — it passed
`line_height: None`, so re-laying a paragraph out at a new width silently reset
its line spacing.

**The window's rasterizer was fixed at construction.**
`vieww-platform-winit`'s `NativeRenderer::for_window` built
`CpuRenderer::new()`, and that was the only rasterizer a windowed application
could have. `vieww-paint` offers `with_color_pipeline` — the linear-light path
documented at length in `native/linear.rs` as the colorimetrically correct way
to blend a gradient or a translucent overlay — and
`with_glyph_outline_budget_bytes` for a memory-constrained target. Neither
could be reached from a real window: configurable in a unit test, fixed in
every application, which is worse than not offering them.
`for_window_with(…, cpu)` takes the rasterizer; `for_window` is the short form
with the default.

**Two controls drew their own palette.** The carousel's page indicators were
`rgb(58, 122, 246)` and `rgb(203, 213, 225)`, and the colour picker's labels,
track and hex readout were five more hex literals — so the one control on the
screen whose entire subject is colour was the one that ignored the theme, and
on a dark scheme drew near-black text on a dark ground. Both now read
`ColorScheme` roles (`primary`, `outline`, `on_surface`, `on_surface_variant`,
`surface_variant`). The picker's *preset swatches* are deliberately still the
caller's own colours: those are its content, not its chrome.

### What was looked at and deliberately left

- **`ModalBarrier`'s scrim** and the colour picker's **default presets** are
  documented defaults with builder overrides (`.color(…)`, `.presets(…)`). A
  default a caller can replace in one call is not a baked-in choice.
- **The switch's thumb shadow** is `rgba(0, 0, 0, 0x38)` and stays that way. It
  is the only baked shadow colour left in the framework, occlusion is
  achromatic rather than a brand decision, and `ColorScheme` says of itself
  that it is "trimmed to what the controls here actually use — a role nothing
  reads is a role nobody can be sure is right". Adding an elevation role for a
  single reader would be the same mistake in the other direction. If a second
  control ever needs one, that is when it earns its place.
- **`cfg(target_os = …)`** appears 65 times and is not hard-coding: it is how a
  cross-platform crate says which platform a path belongs to.

## The preview and the built application disagreed

A scaffolded project ran with a **dark window** and **light-themed widgets**.
The heading and the body text were drawn in near-black ink on a near-black
ground — present, laid out, and invisible — while the filled button drew its own
accent colour and was the only thing on the screen.

Two settings and nothing tying them together:

* `App::background(ThemeData::dark().colors.surface)` painted the window.
* The widget tree got whatever `Theme` the application mounted, and the
  template mounted none, so `ThemeData::of` returned its
  `ThemeData::light()` fallback.

The template's own comment claimed the invariant it did not have — "the theme's
own surface rather than a colour chosen here, so the window's background cannot
drift from what the widgets draw on" — beside the line that let it drift.

**It survived review because vieww Studio's preview was right.** The preview
wraps the screen in `Theme::new(ThemeData::adaptive(platform, dark))` of its own,
so the studio showed the screen correctly the whole time and only the built
application was wrong. A preview that disagrees with the build is worse than no
preview: it is the one failure the feature exists to prevent, and it makes every
other thing the preview says untrustworthy.

Fixed at the framework rather than in the template, because a template fix
leaves the trap set for everyone who writes an `App` by hand:

* `FrameDriver::set_theme` publishes a theme above the root, the same way the
  view metrics and the accessibility preferences are already published — so an
  application does not have to remember to wrap its own tree. A `Theme` *inside*
  the tree still wins, so a dark section of a light screen is unaffected.
* `App::theme(data)` sets the window background **and** calls that, from one
  value. `App::background` remains, and its doc now says what it is for and
  what it cannot do alone.
* Both project templates use `App::theme(theme())`, where `theme()` is one
  function returning `ThemeData::light()` — light because that is what the
  studio previews with by default, so the screen you Render and the screen you
  build are the same screen. Changing that one line moves the window and every
  widget together.

Two tests in `vieww-render`: a widget that mounts no theme of its own reads the
application's, and a `Theme` inside the tree still overrides it.

## Say was a language you could preview but not ship

`vieww-say-codegen` had exactly one caller in the whole workspace: vieww Studio's
preview. Nothing in a *user's* build ever touched it.

So a Say project built cleanly and produced an application that did not contain
its own screen. The scaffolded `lib.rs` mounted a placeholder — literally
`Text::new("Open src/screens/home.say and press Render")` — and that is what the
finished, runnable, shipped application said when you ran it. The screen the
author wrote existed only inside the studio.

A Say project now scaffolds with a `build.rs` that compiles every
`src/screens/*.say` through the same generator the preview uses, into Cargo's
`OUT_DIR`; `src/screens/mod.rs` includes the result, so `home.say` becomes
`screens::home` and `mount` mounts `screens::home::screen()`. Adding a screen
means adding a `.say` file — there is no list to keep in step, which is the
point of generating it. A syntax error fails the build with file, line and
column, from the same diagnostics the Problems panel renders.

`a_scaffolded_say_project_builds_and_contains_its_screen` is the test, and it
does not stop at "it built": it greps the binary for `Tapped`, a string that
exists only in `home.say`, and asserts `press Render` is *absent*. The old
scaffold built perfectly and shipped the wrong screen, so compiling was never
the property worth asserting.

Generated Say code also emitted `warning: method 'mark' is never used` into a
file under `target/` that the author did not write and cannot edit. A warning
nobody can act on is worse than none — it teaches the person that this build is
expected to be noisy, and the next warning, one that *is* theirs, arrives into
a build they have stopped reading. The generator now scopes an `allow` to the
fixed scaffold it emits, and leaves every other lint in the file live.

## A hello-world weighed 1.3 GB, and 90% of the binary was other people's symbols

Measured on a freshly scaffolded project, `cargo build`, debug:

| artifact | before | after |
|---|---|---|
| `libapp.a` (iOS staticlib) | 813.5 MB | not built |
| `libapp.so` (Android cdylib) | 228.4 MB | not built |
| `libapp.rlib` | 41.7 MB | 7.0 MB |
| the binary | 260.5 MB | **40.6 MB** |
| the binary, `--release` | — | **7.6 MB** |

Three independent causes, and only the first is obvious once seen:

- **A desktop build was building the Android and iOS libraries.** The scaffold
  declares `crate-type = ["rlib", "cdylib", "staticlib"]` and all three are
  genuinely needed — the rlib for the desktop binary, the cdylib because an
  Android activity loads a shared library, the staticlib because
  `UIApplicationMain` links one. None of them is needed *on a desktop build*,
  and a plain `cargo build` builds every declared crate type. `Builds::spec` now
  passes `--bin <name>` for the desktop targets, which stops cargo at the rlib
  the binary links; the mobile targets keep the plain `build`.
- **Full debug info for ~380 dependencies.** `.text` in that 260 MB binary was
  17.4 MB; `.debug_info` and `.debug_str` alone were 186 MB. The scaffold now
  sets `debug = 1` for the author's crate — a panic still names their file and
  line — and `debug = false` for `[profile.dev.package."*"]`.
- **No release profile at all.** The scaffold had none, so a shipped build got
  cargo's defaults. It now sets `strip`, `lto = "thin"`,
  `codegen-units = 1` and `panic = "abort"`, each with the reason written beside
  it.

`binary_name` reads the `[[bin]]` name out of the manifest rather than guessing
it from the folder, because a project can be renamed, moved, or written by hand,
and a build that guesses wrong fails with `no bin target named …`.

## What genuinely remains

> **Superseded by [`PENDING.md`](./PENDING.md)**, which is complete where this
> section is partial — it covers the platform, text, tooling and product gaps
> this section never listed, and gives each one a starting point. What follows
> is kept because its *reasoning* is referenced from elsewhere in this file;
> where the two disagree, `PENDING.md` is current.

- **GPU coverage, not GPU existence.** This section once said no GPU code had
  ever run. Then it said the GPU could draw shapes but not text. Both are now
  out of date: `lavapipe` is installed, `vieww_hal::vulkan::SceneRenderer`
  executes a whole batched scene on it, and **text is drawn and verified** —
  see the top of this file, and `PENDING.md` for the full remaining list.

  What `vieww_gpu::ScenePlan` still cannot express is **images, shadows,
  gradients, shaped clips and layers**. The practical consequence has narrowed
  rather than gone away: a screen of text and flat panels now plans complete,
  and any screen with a photograph, an elevation shadow, a gradient or a
  rounded clip does not — which is still most real screens, and
  `ScenePlan::is_complete` still sends every one of them to the CPU rasterizer.

  A second gap sits behind the first: `lavapipe` is a *software* Vulkan
  implementation, so what has been verified is correctness, not performance.
  **No number in this file was measured on real graphics hardware.**

  The two fixtures still over a 60 Hz full-repaint budget are unchanged by
  any of this: they have blurs, shadows and shaped clips on them, so
  `is_complete` refuses them and they stay on the CPU path whatever the GPU
  can do with their text. Explicit SIMD in the composite loops remains the other
  real option and remains undone.
- **Non-Linux packaging.** `vieww-build`'s macOS/Windows/Android packaging
  paths are real code, unit-tested for their logic (argument construction,
  tool detection, error naming), but have not been exercised by an actual
  build on those platforms' own toolchains.
- ~~**`docs/architecture/*.md`** — not written.~~ **Done.** See
  [`docs/architecture/README.md`](./docs/architecture/README.md) and its four
  companion files: the UI tree (widget/element/render), the rendering
  pipeline (scene/render-graph/render-planner/CPU-GPU-Hybrid), the
  cross-cutting crates, and tooling/distribution. Written from the crates as
  they actually exist, citing each crate's own module doc rather than
  restating it.
- ~~**A full stale-doc sweep**~~ — **partly done, and the mechanical half is
  finished.** `cargo doc` under `-D warnings` now passes across the workspace,
  which is 36 broken intra-doc links fixed (see above) and is the part a
  machine can check. Two specific stale *claims* were also found and corrected
  by reading them against the code: the `rustybuzz` note below, and
  `ci/check/checks.sh`'s comment about backends that no longer exist. A prose comment
  can still be out of date without any tool noticing, so this is "no known
  stale docs" rather than "proven none".
- **Naming note on item 7 ("rustybuzz"), corrected.** The previous wording
  here said the shaping stack was "not literally the `rustybuzz` crate". That
  is half right and it hid the more interesting half, so, from the lockfile
  and the call sites rather than from memory:

  - **What actually shapes the text a widget draws** is `cosmic-text 0.19`,
    reached through `vieww_text::Paragraph` (`src/paragraph.rs`) — and
    cosmic-text shapes with **`harfrust`**, a HarfBuzz port built on
    `read-fonts`/`skrifa`. So the functional requirement is met, by a
    HarfBuzz-lineage shaper, and the named crate is indeed not the one doing
    it.
  - **`rustybuzz` is nonetheless a real direct dependency** of `vieww-text`,
    optional behind the `harfbuzz` feature, and `shaping::HarfBuzzShaper`
    genuinely wraps it and is genuinely tested. Nothing in this workspace
    enables that feature, so in a default build it is not compiled — which is
    what an optional dependency is for, and not a gap.
  - **`shaping`, `layout`, `selection`, `editing` and `bidi` are a second,
    in-progress stack** and `vieww-text`'s own crate doc says so in as many
    words. `layout::Paragraph` hard-codes `NaiveShaper` — one glyph per code
    point — and is reached only through its module; it is not what any widget
    uses. It is worth knowing that before reading `NaiveShaper` and concluding
    that the framework has no shaping, which is a conclusion this file's
    previous wording made easy to reach.
- **Everything else this tracker lists** is real, has tests that ran and
  passed on this machine, and has been clippy-clean under
  `-D warnings` at the point it was last touched.