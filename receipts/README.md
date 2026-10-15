# Benchmark receipts

Machine-readable records of benchmark runs, one folder per machine and date.
Each `*.json` is a `vieww.bench-receipt/1` receipt written by
`cargo run --release -p fixtures -- OUT --receipt FILE` (see
`vieww_base/examples/fixtures/src/receipt.rs` for every field); each
`*-shadow.jsonl` is the hybrid planner's shadow log for the same run, one
`vieww_render_planner::ShadowFrame` per line.

A receipt is only comparable with another when its `build`, `machine` and
`gpu` blocks say they are. Read those first.

| Folder | Machine | What it is |
|---|---|---|
| `2026-10-15-cloud-xeon-2core/` | 2-core cloud Xeon @ 2.1 GHz (AVX-512), 8 GB, no GPU, rustc 1.97 | The first receipt: the CPU gallery at 30 runs from a clean tree, and its shadow log. **Not the reference laptop** — useful as a format example and for the planner's CPU calibration, not as a budget verdict. Its p50s for `00-layers-nested` (18.1 ms) and `23-editor-glass` (16.7 ms) are over budget while their best-of runs (12.9, 12.2 ms) are not: a shared VM's noise, which is why a receipt carries every run. |
| `2026-10-15-kabylake-i5-7200u-linux/` | **The reference laptop:** i5-7200U, Intel HD 620 + NVIDIA 920MX, Ubuntu (kernel 7.0, GNOME/Wayland), pinned rustc 1.98.1 | `ci/hardware/linux.sh`: gallery and census (920MX) receipts with shadow logs, the baseline window x5 per path (`baseline/`), stress x5, the window test log. Energy null (RAPL was root-only). |
| `2026-10-15-kabylake-i5-7200u-windows/` | The same laptop, Windows 10 19045, NVIDIA driver 32.0.15.7652 | `ci/hardware/windows.ps1`: the same set, plus the window test log showing the teardown crash (PENDING §3.4). |

Phones: AWS Device Farm results live on the branch `ci-results/device-farm-main`.

Next to add: the laptop again with RAPL readable (`sudo chmod o+r
/sys/class/powercap/intel-rapl:*/energy_uj`) for energy per frame, and the
Redmi Note 7 Pro's device suite.
