# Remaining work (handed over 2026-10-11)

Verified state of each item is in `PENDING.md`; this is the short list.

1. **Push** — the branch could not be pushed from the session (GitHub account
   not linked). Apply `vieww-gap-closure-session.patch` (`git am`) and push.
2. **Re-run the full studio suite** — `cargo test -p viewwstudio`. Only
   `tests/remote.rs` (4/4) and clippy were run after the out-of-process
   preview landed; the full run was cut off by a full disk.
3. **Out-of-process preview** (PENDING §3.3): forward keyboard/IME, scroll and
   hover; remote preview-state capture; run `tests/remote.rs` on Windows.
4. **Metal / D3D12** (PENDING §1.2/§1.3): read the first macOS/Windows CI
   results of `metal_smoke` / `d3d12_smoke`; then port `SceneRenderer`.
5. **GPU 3D seam** for `vieww-3d` (not started): a `GpuRenderer` behind the
   existing `(scene, camera, settings) -> RGBA8` contract, CPU renderer as
   the pixel oracle; first increment = unlit forward pass on Vulkan.
6. **SIMD compositing** for the two over-budget fixtures (PENDING §4.1) —
   measure with callgrind, not wall clock.
7. **Animated general paths on GPU** (PENDING §2.1 residual) — measure, then
   MSAA for shapes that changed this frame if it matters.
8. **Hardware runs**: real GPU (§1.1), iOS simulator/device (§1.4), Android
   device (§1.5), Firefox/WebKit web parity (§1.6), a two-monitor refresh-rate
   check (§3.2).
9. **Coverage gate** (§5.1): drop `continue-on-error` once it has history.
