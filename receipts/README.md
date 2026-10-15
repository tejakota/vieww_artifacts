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

Next to add: the Kaby Lake laptop's gallery and census
(`ci/hardware/close-remaining.sh gallery`, which writes both receipts and
shadow logs), and the Redmi Note 7 Pro's device suite.
