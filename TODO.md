# Remaining work (updated 2026-10-11, after the session that pushed this branch)

Verified state of each item is in `PENDING.md`; this is the short list.

1. **Push** — done: both patches (`vieww_premium_pass.patch`,
   `vieww-gap-closure-session.patch`) are applied on
   `feature/vieww_code_base2` and the branch is pushed.
2. **Re-run the full studio suite** — done: `cargo test -p viewwstudio`
   green, 921 tests over 30 suites, on the pinned toolchain (the run needs
   `CARGO_INCREMENTAL=0` and no debuginfo on small disks — see the session
   worklog).
3. **Out-of-process preview** (PENDING §3.3): the forwarding half is done
   (keyboard/IME/scroll/hover, remote preview-state capture — commit
   `782717a`); what remains is running `tests/remote.rs` on Windows, which
   needs the Windows CI leg.
4. **Metal / D3D12** (PENDING §1.2/§1.3): render paths, smoke tests and the
   cross-platform type-check landed (commit `f695c04`); reading the first
   macOS/Windows CI results and porting `SceneRenderer` remain CI/hardware
   work.
5. **GPU 3D seam** — first increment done (PENDING §1.7): `GpuRenderer`
   behind the `(scene, camera) -> RGBA8` contract, unlit forward pass on
   Vulkan, parity suite `tests/gpu_unlit.rs` green on lavapipe and wired
   into the `framework-checks` workflow. Next: lighting (Lambert + one
   shadow map), then textures, then the transparent pass.
6. **SIMD compositing** for the two over-budget fixtures (PENDING §4.1) —
   measure with callgrind, not wall clock.
7. **Animated general paths on GPU** (PENDING §2.1 residual) — measure, then
   MSAA for shapes that changed this frame if it matters.
8. **Hardware runs**: real GPU (§1.1), iOS simulator/device (§1.4), Android
   device (§1.5), Firefox/WebKit web parity (§1.6), a two-monitor refresh-rate
   check (§3.2).
9. **Coverage gate** (§5.1) — closed: the `framework-checks` workflow's
   coverage job already runs `ci/check/coverage.sh` as a hard gate (no
   `continue-on-error`; the script fails below the 70% floor).
