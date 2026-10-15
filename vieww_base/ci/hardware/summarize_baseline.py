#!/usr/bin/env python3
"""Summarise a directory of `examples/baseline` reports (`cpu-N.json`,
`gpu-N.json`) against the audit's "lightest framework" targets.

    python3 ci/hardware/summarize_baseline.py hardware-results/linux-.../baseline

Prints one block per path and writes `summary.json` beside the reports. Used by
both `ci/hardware/linux.sh` and `ci/hardware/windows.ps1`, so the two machines'
numbers come out in the same shape.

Medians across launches, not means: one launch that met a cold disk or a
background update should not move the figure, and every raw report stays in
the directory for anyone who wants another statistic.
"""

import json
import pathlib
import statistics
import sys

# The audit's proposed contract (Phase 6). Desktop rows only; the phone rows
# (p95 under 15 ms, cold start under 120 ms, APK under 6 MB) come from the
# device suite, and the energy row is relative to Flutter and cannot be judged
# from vieww alone.
TARGETS = {
    "peak_rss_bytes": (40 * 1024 * 1024, "baseline RSS under 40 MB"),
    "idle_frames": (0, "0 frames presented while nothing changes"),
    "idle_rasterised_px": (0, "0 pixels rasterised while nothing changes"),
}

FIELDS = [
    ("launch_to_first_frame_ms", "launch to first frame", "ms"),
    ("main_to_first_frame_ms", "main() to first frame", "ms"),
    ("first_frame_rss_bytes", "RSS at first frame", "MB"),
    ("peak_rss_bytes", "peak RSS", "MB"),
    ("idle_frames", "idle: frames presented", ""),
    ("idle_rasterised_px", "idle: pixels rasterised", ""),
    ("idle_cpu_percent", "idle: CPU", "%"),
    ("idle_package_power_w", "idle: package power", "W"),
    ("animate_fps", "animate: fps", ""),
    ("animate_gpu_frames", "animate: GPU-drawn frames", ""),
    ("work_median_ms", "animate: frame work median", "ms"),
    ("work_p95_ms", "animate: frame work p95", "ms"),
    ("animate_cpu_percent", "animate: CPU", "%"),
    ("marginal_energy_per_frame_mj", "animate: marginal energy / frame", "mJ"),
]


def fmt(value, unit):
    if value is None:
        return "n/a"
    if unit == "MB":
        return f"{value / (1024 * 1024):.1f} MB"
    if unit in ("ms", "W", "mJ"):
        return f"{value:.2f} {unit}"
    if unit == "%":
        return f"{value:.1f}%"
    return f"{value:g}"


def median(values):
    values = [v for v in values if v is not None]
    return statistics.median(values) if values else None


def main(directory):
    directory = pathlib.Path(directory)
    out = {}
    for mode in ("cpu", "gpu"):
        reports = []
        for path in sorted(directory.glob(f"{mode}-*.json")):
            try:
                reports.append(json.loads(path.read_text()))
            except (OSError, ValueError) as error:
                print(f"{path.name}: unreadable ({error})")
        if not reports:
            continue
        print(f"{mode.upper()} path, {len(reports)} launches (medians):")
        block = {"launches": len(reports)}
        for key, label, unit in FIELDS:
            value = median(r.get(key) for r in reports)
            block[key] = value
            line = f"  {label:<36} {fmt(value, unit)}"
            if key in TARGETS and value is not None:
                limit, words = TARGETS[key]
                line += f"   [{'meets' if value <= limit else 'MISSES'}: {words}]"
            print(line)
        stages = {}
        for name in reports[0].get("stage_medians_ms", {}):
            stages[name] = median(r["stage_medians_ms"].get(name) for r in reports)
        nonzero = {k: v for k, v in stages.items() if v}
        if nonzero:
            print("  frame stages (median ms): " + ", ".join(f"{k} {v:.2f}" for k, v in nonzero.items()))
        block["stage_medians_ms"] = stages
        if all(r.get("energy_source") is None for r in reports):
            print("  energy: not measured on this machine (see the env step)")
        out[mode] = block
    (directory / "summary.json").write_text(json.dumps(out, indent=2) + "\n")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(sys.argv[1])
