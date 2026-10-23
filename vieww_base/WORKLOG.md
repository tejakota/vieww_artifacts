# Work log

The hand-off between working sessions: where things stand, how to work here,
and what is next. Read this first when picking the work up.

It sits beside two older files and does not replace them:

- [`PENDING.md`](./PENDING.md) is everything known to be missing, with why and
  where to start.
- [`TRACKER.md`](./TRACKER.md) is what has been done, with the evidence.

This file is the short version for a new session. When an item here turns out
to be large, it belongs in `PENDING.md` with a pointer from here.

Last updated: 2026-10-23, after Device Farm results for `42c6d92`.

---

## 1. Where things stand

### The goal of the current stretch

The aim is to make vieww measurably the best: fast, light and correct on real
phones, with every claim backed by a Device Farm number or a test. The
standing instruction is to finish what is started. A slow loop is fine; leaving
work half-done is not.

### The list we worked through, and what came of it

| # | Item | State | Where it landed |
|---|---|---|---|
| 1 | Heavy stress screen, and Device Farm passes for it | done | `examples/shared/screen.rs` `Demo::stress_screen`; passes `stresscpu`, `stressgpu` |
| 2 | Sustained pass: thermal, headroom, memory over time | done | `sustained` (60 s by the clock), `thermal.rs`, `pss_over_time` |
| 3 | CPU time per frame (energy stand-in) | done | `cpu_time.rs`, `cpu_time` observation |
| 4 | About 30–40 taps for input latency | done | `ci/mobile/device-suite.sh` |
| 5 | Publish iOS results even when a run fails | done | `.github/workflows/device-farm.yml` (`10587c1`) |
| 6 | CPU raster profile by kind of work | done | `RasterKindTimes`, `raster_by_kind` (`VIEWW_RASTER_PROFILE=1`) |
| 7 | Shrink the full-screen swapchain copy | done | `vieww-hal/src/vulkan/present_history.rs` (partial copies, verified on the device) |
| 8 | Verify everything | done for every patch so far | see section 3 |

### What the phones say (Device Farm, `42c6d92`, 2026-10-22)

| | LG Stylo 5 (low) | Galaxy A73 (medium) | Galaxy S26 Ultra (high) |
|---|---|---|---|
| Demo, GPU path | 10.1 ms | 1.3 ms | 0.39 ms |
| Demo, CPU path | 35 ms (known failure) | 11.7 ms | 5.5 ms |
| Swapchain copy, partial vs whole | 0.27 vs 2.94 ms | 0.04 vs 0.81 ms | 0.02 vs 0.23 ms |
| Stress, CPU path | 729 ms | 144 ms | 54 ms |
| Stress, GPU path | 951 ms | 136 ms | 20 ms |
| Stress GPU: backdrop blur ("filters") | 830 ms | 117 ms | 16 ms |
| No-trails check (`gpucheck`) | 594 images kept, 0 trails | 593, 0 | 592, 0 |

The run failed on one gate: the S26's `auto` pass, which shipped the CPU path
there. 15 of 240 frames were over budget (12 allowed), all of them waiting on
`acquire`. That is the S26's known CPU-path pacing problem. The fix is the
`auto` policy change in section 2.

### Commits of this stretch (all on `main` except the last)

- `6de1a8f`: device suite: stress screen, sustained pass, thermal, CPU time,
  more taps.
- `81099e6`: partial swapchain copies, verified on the device. Also fixes the
  scaled copy, which drew a `gpuscale` frame into the top-left quarter only.
- `0a57619`: CPU raster time by kind of command.
- `72b7c37`: shadows keyed by shape, exact at the screen's edge, one GPU pass.
- `cf1174f`: shadow sub-pixel phases snapped to a quarter pixel.
- `ff6884f`: stress and sustained passes finish; `gpuscale` no longer leaks
  into later passes.
- `d4b5c93`: a scrolling list's shadows stay in the caches. They now cache
  coverage rather than tinted pixels, never clamp a screen-sized shadow, snap
  the size, and the mask atlas keeps four frames.
- `42c6d92`: Device Farm waits on progress, not a fixed deadline; check mode
  compares one frame in eight.
- **Pending (this session's patch):** `auto` measures the GPU on its second
  launch even when the CPU path settled, and keeps a GPU that is at least
  twice as fast within budget (`MUCH_FASTER`). Also collects `shadow_blurs`,
  and adds this file.

---

## 2. What is next, in order

1. **Check the next Device Farm run for the `auto` change.**
   - The S26's `auto` should now choose the GPU, and its gate should pass.
   - The A73 and the Stylo should be unchanged (the A73's CPU never settled;
     the Stylo's GPU is not twice as fast at a budget it fits).
   - `shadow_blurs` should appear per pass. Expect about 1.4 a frame on the
     stress screen (the animated floater) and 0 on the demo screen.
2. **Run the iPhones.** `RUN.txt` says `ipa: skipped · iphones: skipped` for
   recent runs. The `auto` change applies there too. Expected: the CPU stays,
   because the iPhone GPU path has been slower (`rasterizer_choice.rs` table).
3. **The backdrop blur is now the biggest GPU cost** on the stress screen: 117
   ms on the A73, 830 ms on the Stylo. The GPU box blur (`post.wgsl` modes
   1/2) reads `2r + 1` texels a pixel, three passes each way. The CPU's
   running-sum blur is O(1) a pixel. Options, both of which must keep parity
   with `NativeRenderer` (the oracle):
   - a compute pass with a running sum per row and per column;
   - bilinear taps at exact half-texel offsets, which halves the reads.
   Start: `vieww-hal/src/vulkan/scene.rs` `fn blur`, and
   `vieww-shaders/shaders/post.wgsl`.
4. **The CPU stress frame** on the A73 is 144 ms: rects 25.9, layers 23.8,
   images 22.8, shadows 22.2 and composite 19.3 ms (`raster_by_kind`). The
   shadow share is now mostly the animated floater's blur, one a frame.
   Next: layers and composites (the frosted header's backdrop).
5. **Stylo CPU path** is 35 ms against 16.7 ms: known failure RC5,
   `PENDING.md` §4.7. `auto` ships the GPU path there, at 10 ms.
6. **Small things the data showed:**
   - The S26's `thermal` reads a "hottest zone" of 105.0 °C, constant. That
     is a zone that is not a temperature getting through the 150 °C filter in
     `thermal.rs`. Filter by zone `type`, or report the status and headroom
     only.
   - `idle_frames_do_not_repaint_the_whole_surface` failed on the S26's
     `stressgpu`. The stress screen never idles; make that check
     screen-aware, as the damage check already is (`device_tests.rs`, search
     `VIEWW_SCREEN`).
   - `known-failures.txt` entries for the A73 `cpu` and S26 `cpu` passes
     passed once on `42c6d92`. Both are pacing flakes, so delete them only
     after several clean runs.
   - The CI results are published while `checks.sh (Linux)` is still
     `in_progress` (`ci-results/main` `SUMMARY.md`), so that leg's verdict
     comes later. Check the run itself, or make the publisher wait for it.
   - Memory: the GPU path's PSS is 145–195 MB against 75 MB on the CPU path.
     The Stylo's stress screen reached about 500 MB, then held. `PENDING.md`
     §4.2.

---

## 3. How to work here

### Toolchain and the gate

- Rust is pinned to **1.98.1** (`rust-toolchain.toml`). Verify on that
  toolchain, never another: verifying on 1.97 is how a whole session once
  went in circles.
- **The gate is `bash ci/check/checks.sh`**, from `vieww_base/`. It runs fmt,
  clippy (features `vieww-paint/native,vieww-hal/vulkan`), docs with
  `-D warnings`, every test, wasm, the fixture gallery, raster identity,
  conformance, hot reload and packaging. Run all of it before handing work
  over. It takes about an hour here.
- **`bash ci/check/cross-check.sh`** type-checks macOS, iOS, Windows and
  Android from Linux. It needs `rust-src`. Code that is only used on
  Linux/Android needs `#[cfg]`, or macOS fails on dead code.
- **GPU parity under validation:**

  ```sh
  VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation \
  VK_LAYER_ENABLES=VK_VALIDATION_FEATURE_ENABLE_SYNCHRONIZATION_VALIDATION_EXT \
  cargo test -p vieww-hal --features vulkan -- --ignored --test-threads=1
  ```

  It runs on lavapipe. Grep the output for `VUID` and `SYNC-HAZARD`: there
  should be none.
- **The desktop suite on the stress screen:**

  ```sh
  VIEWW_SCREEN=stress VIEWW_GPU=1 WINIT_X11_SCALE_FACTOR=2.625 \
    xvfb-run -a -s "-screen 0 1300x2500x24" \
    target/release/examples/desktop
  ```

  The scale factor imitates a phone's density: whole-pixel scrolling at a
  density of 1 hid the shadow-cache misses. `VIEWW_SUITE_SECONDS`,
  `VIEWW_PARTIAL_BLIT=0|1` and `VIEWW_PARTIAL_BLIT_CHECK=1` behave as on
  the phones.
- **Disk:** the target directory grows past 20 GB. Delete `target/tmp`,
  `target/package` and `target/debug/incremental` when space runs out.

### Pixels

**Raster identity** (`ci/check/raster-identity.sh`) fails when any fixture's
bytes change.

- If a change is intended, say what moved and by how much in a
  `Pixels-Changed:` commit trailer. `checks.sh` then reports the change
  instead of failing.
- Measure the change against the merge base. Compare decoded RGB, not with
  `getbbox()`, which in Pillow looks only at alpha on RGBA images.
- The GIF fixtures re-quantise their palettes per frame, so a small change
  moves unrelated pixels. Judge by the PNGs.

### Device Farm and the results branches

- **Read results with a forced fetch.** A plain `git fetch` can leave the
  remote-tracking ref stale, and once that produced a wrong "nothing new":

  ```sh
  git fetch origin \
    +refs/heads/ci-results/main:refs/remotes/origin/ci-results/main \
    +refs/heads/ci-results/device-farm-main:refs/remotes/origin/ci-results/device-farm-main
  ```

- **`ci-results/device-farm-main`:**
  - `RUN.txt`: what ran.
  - `android/SUMMARY.md`: per phone and pass.
  - `android/<phone>/Customer_Artifacts/Host_Machine_Files/$DEVICEFARM_LOG_DIR/vieww/`:
    `result.txt` (every key the run recorded), `<pass>-suite.log`,
    `<pass>-logcat.txt` and `sustained-pss.txt`.
- **`ci-results/main`:** `SUMMARY.md` (legs) and `jobs.json`.
- **Passes** (`ci/mobile/device-farm/run-android.sh`):
  - **Gating:** `cpu`, `gpu`, `auto`. The rest are diagnostic.
  - **Diagnostic:** `cpuvk`, `cpuhint`, `gpuscale`, `gpuserial`, `cpubare`,
    `cpunothreads`, `cpunofill`, `cpunoblur`, `stresscpu`, `stressgpu`,
    `sustained`, `gpucheck` and `gpufullcopy`.
  - **`gpucheck`'s exception:** its `partial_copies_leave_no_trails` check
    fails the run on any phone.
- **Accepted failures** go in `ci/mobile/device-farm/known-failures.txt`,
  keyed by `ro.product.model`. A listed check that passes produces a "stale
  entry" warning.
- **Android properties** (`debug.vieww.*`) set every option per pass. Set an
  empty value as `'""'`: a bare `""` is refused, and that is how `gpuscale`
  leaked into every later pass.

### Delivering work

This session cannot push or use the GitHub API: the user's GitHub account is
not linked to it. Work is handed over as a `git format-patch` file that the
user applies with `git am`.

- Before sending, check that it applies to `origin/main` in a scratch worktree
  and that the tree matches.
- End commit messages with the attribution lines the session gives.
- Keep commits reviewable: one change each, each one passing clippy on its
  own.

### How results have been read, and what went wrong doing it

These have each happened once. Do not repeat them:

- **A claim is data only when a number backs it.** Say which parts of a
  conclusion are judgement.
- **A desktop at density 1 does not behave like a phone.** It scrolls in whole
  pixels, so caches keyed by sub-pixel phase look perfect.
- **A pass that "never reported" is usually a deadline**, not a crash. Read its
  `-suite.log` and `-logcat.txt` before guessing.
- **Float keys compared by bit pattern** need snapping: `bottom - top` differs
  in the last bit by position.
- **An LRU cache over a cycle longer than itself hits nothing.** Size it for
  the working set, or store less per entry.
