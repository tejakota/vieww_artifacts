# Remaining work (updated 2026-10-12)

Verified state of each item is in `PENDING.md`; this is the short list.

## Done on 2026-10-12 (this session's commits, on top of `8fae3b4`)

- **Branch verified**: studio suite 921/921, remote-preview tests, vieww-3d
  GPU tests, workspace clippy — on Rust 1.97 (the 1.98.1 pin could not be
  downloaded in the session); `film_lab` given a reasoned
  `manual_range_contains` allow (the rewrite changes NaN behaviour).
- **SIMD compositing** (PENDING §4.1, narrowed): SSE2 output conversion and
  compositing rows, the layer clear fused into the composite,
  `-C target-cpu=x86-64-v2` restored. Gallery 232.7 → 164.6 ms, every output
  byte-identical; `layers-nested` ~28.5 → ~20–22 ms, `editor-glass`
  ~27 → ~21 ms — still over 16.7 ms.
- **Animated GPU paths** (PENDING §2.1, measured and fixed): CPU coverage is
  bounded (~1.3 ms for a large rotating panel), so no MSAA; the cost was
  whole-texture mask uploads (up to 16.8 MB/frame). Now partial uploads and a
  churn-aware compaction: 1.9 MB/frame mean for three moving shapes.
- **GPU 3D lit pass** (PENDING §1.7): Lambert under ambient, hemisphere and
  directional lights, with the CPU's shadow map, back faces culled/flipped as
  on the CPU — held to the CPU oracle on lavapipe.

## Open

1. **Push** — the session's GitHub access was not linked, so these commits
   arrive as `vieww-session-2026-10-12.patch` (`git am` on
   `feature/vieww_code_base2`).
2. **Place the workflows** (`.github/workflows-pending/restore.sh`) — needs a
   `workflow`-scoped token. Note: the Vulkan job will be red on the known
   backdrop-blur edge (PENDING §2.2) until that edge mode is decided.
3. **Backdrop-blur edge mode** (PENDING §2.2) — a design decision (CSS
   duplicates edge pixels), then align `reference.rs` and the Vulkan
   `PushLayer` branch.
4. **The two over-budget fixtures** (PENDING §4.1) — ~4–5 ms left, spread
   over blur, shadows, rasterisation and memory traffic. Candidates: SSE2
   blur; narrower layer storage (changes bytes — needs a tolerance decision);
   AVX2 behind runtime detection. Time anything that moves memory.
5. **GPU 3D next increments** (PENDING §1.7): point and spot lights, Phong,
   textures, the transparent pass; a cached pipeline instead of per-render
   bring-up; a GPU shadow map only if measured to matter.
6. **Windows / macOS / hardware** — `tests/remote.rs` on Windows (§3.3),
   first Metal/D3D12 CI results and the `SceneRenderer` port (§1.2/§1.3), a
   real GPU (§1.1, including what partial mask uploads save over PCIe), iOS
   and Android devices (§1.4/§1.5), Firefox/WebKit parity (§1.6), a
   two-monitor refresh-rate check (§3.2).
7. **Toolchain** — re-run `ci/check/checks.sh --quick` on the pinned 1.98.1
   where it can be downloaded; this session's clippy runs were on 1.97.
