#!/usr/bin/env python3
"""The gpuscale promotion decision: which tiers' quality profiles have
earned their default, from what the Device Farm actually measured.

    python3 ci/mobile/gpuscale_verdict.py --summary df-out/summary.json --out df-out
    python3 ci/mobile/gpuscale_verdict.py --self-test

`quality.rs` documents the deal the tier profiles are under: the scale is
opt-in (`VIEWW_GPU_SCALE`) because pixels move, and "if 0.5x turns the LG
Stylo 5's 40.1 ms into a pass, the tier profile has earned its default; if
not, nothing changed". Until now that judgment was a human reading
SUMMARY.md against that sentence. This script *is* the sentence, applied:

For each device in the run (from `device_farm.py`'s summary.json, or a
bare `result.txt` via --result):

1. **Budget.** Parsed from the pass's own `frame_time` observation
   (`median 36.2ms, worst 68.6ms, budget 16.7ms (600 samples)`) — the
   budget the device's display refresh set, not a constant from this file.
2. **Candidate.** A device *needs* the tier default when the current
   policy — best of the CPU and GPU paths at full resolution — misses the
   budget. A device that already passes has nothing for the scale to fix,
   and a profile promoted "for headroom" would be a guess.
3. **The test.** A candidate contributes *promote* when the `gpuscale`
   pass actually engaged (its observed scale is below 1.0) and its median
   lands at or under the budget. A candidate whose scaled run still
   misses contributes *keep*, with the number that said so. A candidate
   with no usable gpuscale data contributes *incomplete* — the honest
   answer, and the reason the farm runs the pass on every phone.
4. **Verdict per tier.** A tier is promoted only when *every* device of
   that tier in the run was a candidate and every candidate passed the
   test. Any keep or any missing datum holds the whole tier back: the
   default applies to every future device the policy calls that tier, and
   one phone's pass is not their pass.

Output: `gpuscale-verdict.json` in --out (verdict, per-tier, per-device,
and the rule as applied) and a printed memo ending in the exact change a
promoted verdict licenses in `quality.rs`. Exit status: 0 decided (promote
or keep), 1 incomplete, 2 setup error.

The numbers this file ships are only in `--self-test`, clearly synthetic,
exercising the four ways the rule can go. Real verdicts come from real
runs; nothing here invents one.
"""

import argparse
import json
import pathlib
import re
import sys

# The tier policy, mirrored from `vieww-platform-winit/src/device_tier.rs`:
# RAM below LOW_MAX_KB is Low, below MEDIUM_MAX_KB is Medium, above is High;
# four or fewer usable cores is Low regardless (the small-device catch).
LOW_MAX_KB = 3 * 1024 * 1024
MEDIUM_MAX_KB = 7 * 1024 * 1024

_DURATION = re.compile(r"^(\d+(?:\.\d+)?)(ns|us|µs|ms|s)$")


def parse_duration(text):
    """`'36.2ms'` -> 36.2 (ms), the only unit the suite prints budgets in.

    Duration's Debug format trims to ns precision, so a 120 Hz budget
    arrives as `8.333333ms` and a fast phone's median can be `1.2ms` —
    but a microsecond-scale value would print `950µs`, and `950ns`/`1.5s`
    are possible in principle. Return milliseconds as a float, or None
    when the text is not one duration.
    """
    match = _DURATION.match((text or "").strip())
    if not match:
        return None
    value, unit = float(match.group(1)), match.group(2)
    return {
        "ns": value / 1e6,
        "us": value / 1e3,
        "µs": value / 1e3,
        "ms": value,
        "s": value * 1e3,
    }[unit]


def parse_frame_time(text):
    """`frame_time`'s observation -> (median_ms, budget_ms).

    The line is `median 36.2ms, worst 68.6ms, budget 16.7ms (600 samples)`.
    The median and the budget are the two numbers the decision needs; the
    worst and the sample count are the frame's own story. None for either
    field the line does not honestly carry.
    """
    if not text:
        return None, None
    median = parse_duration(re.search(r"median (\S+?),", text).group(1)) if re.search(r"median (\S+?),", text) else None
    budget = parse_duration(re.search(r"budget (\S+?)(?:,| \()", text).group(1)) if re.search(r"budget (\S+?)(?:,| \()", text) else None
    return median, budget


def parse_scale(text):
    """`gpu_scale`'s observation -> the fraction, or None when not engaged.

    The line is `GPU path renders at 0.50x resolution (VIEWW_GPU_SCALE=auto:
    tier low)`. A fraction at or above 1.0 is "off" by another name — the
    same answer `quality.rs`'s `gpu_scale()` gives (`(scale < 1.0)
    .then_some(scale)`) — and no observation at all is the full-resolution
    default. Both are "not engaged": the test cannot credit a scale that
    never ran.
    """
    match = re.search(r"renders at (\d+(?:\.\d+)?)x", text or "")
    if not match:
        return None
    fraction = float(match.group(1))
    return fraction if fraction < 1.0 else None


def tier_of(row):
    """The tier the run attributed to a device, cross-checked against the
    Rust policy when only RAM/cores are known.

    The observations carry the tier the app itself resolved (`device:` /
    `quality_overlay:` start with `tier low…`); when neither is present the
    shell's `mem_total_kb`/`cpu_cores` are run through the same thresholds
    `device_tier.rs` uses, so a missing observation degrades to the policy
    rather than to a guess. `None` when nothing is known.
    """
    for key in ("gpuscale_device", "gpu_device", "cpu_device"):
        match = re.search(r"tier (low|medium|high)\b", row.get(key, ""))
        if match:
            return match.group(1)
    mem = row.get("mem_total_kb", "")
    cores = row.get("cpu_cores", "")
    if mem.isdigit():
        kb = int(mem)
        if kb < LOW_MAX_KB or (cores.isdigit() and int(cores) <= 4):
            return "low"
        if kb < MEDIUM_MAX_KB:
            return "medium"
        return "high"
    return None


def decide_device(row):
    """The rule, for one device. Returns (role, detail) where role is
    "promote", "keep", "incomplete", or "unneeded"."""
    device = row.get("device_name") or row.get("model") or "?"
    tier = tier_of(row)
    budget = None
    for key in ("gpuscale_frame_time", "gpu_frame_time", "cpu_frame_time"):
        _, budget = parse_frame_time(row.get(key))
        if budget is not None:
            break

    def median_of(key):
        median, _ = parse_frame_time(row.get(key))
        return median

    cpu, gpu, scaled = median_of("cpu_frame_time"), median_of("gpu_frame_time"), median_of("gpuscale_frame_time")
    scale_engaged = parse_scale(row.get("gpuscale_gpu_scale", ""))
    # The scale only "engaged" if the GPU drew the frames: a pass the CPU
    # drew under the scale label (run f91d07c, Pixel 5a and S26 Ultra) is no
    # evidence about the scaled GPU path, whatever its frame time says.
    m = re.match(r"\s*(\d+) of \d+", row.get("gpuscale_gpu_frames") or "")
    if m and int(m.group(1)) == 0:
        scale_engaged = None

    detail = {"device": device, "tier": tier, "budget_ms": budget,
              "cpu_median_ms": cpu, "gpu_median_ms": gpu, "gpuscale_median_ms": scaled,
              "scale_engaged": scale_engaged}

    if budget is None or cpu is None or gpu is None or tier is None:
        return "incomplete", {**detail, "why": "missing full-resolution frame data or tier"}
    detail["best_full_res_ms"] = min(cpu, gpu)
    if detail["best_full_res_ms"] <= budget:
        return "unneeded", {**detail, "why": f"passes at full resolution ({detail['best_full_res_ms']:.1f} ms <= {budget:.1f} ms)"}
    if scaled is None or scale_engaged is None:
        return "incomplete", {**detail, "why": "the gpuscale pass never engaged for a device that needs it"}
    if scaled <= budget:
        return "promote", {**detail, "why": f"the scale passes where full resolution misses ({scaled:.1f} <= {budget:.1f} ms)"}
    return "keep", {**detail, "why": f"the scale still misses the budget ({scaled:.1f} > {budget:.1f} ms)"}


def decide(rows):
    """The rule, for a run. Returns the verdict dict written to JSON."""
    decisions = [decide_device(row) for row in rows]
    by_tier = {}
    for role, detail in decisions:
        by_tier.setdefault(detail["tier"] or "unknown", []).append((role, detail))

    tiers, incomplete = {}, []
    for tier, entries in sorted(by_tier.items(), key=lambda kv: str(kv[0])):
        roles = [role for role, _ in entries]
        if all(role == "unneeded" for role in roles):
            tiers[tier] = {"verdict": "no-change", "why": "every device of this tier passes at full resolution"}
        elif all(role == "promote" for role in roles):
            tiers[tier] = {"verdict": "promote", "why": "every device of this tier passes only at the tier's scale"}
        elif "incomplete" in roles:
            tiers[tier] = {"verdict": "incomplete", "why": "a device of this tier is missing the data to decide"}
        else:
            keeps = [d["why"] for role, d in entries if role == "keep"]
            tiers[tier] = {"verdict": "keep", "why": "; ".join(keeps) or "the scale did not earn the default on this tier"}

    promoted = sorted(t for t, v in tiers.items() if v["verdict"] == "promote")
    any_incomplete = any(v["verdict"] == "incomplete" for v in tiers.values())
    verdict = ("incomplete" if any_incomplete else ("promote" if promoted else "keep-opt-in"))
    return {
        "verdict": verdict,
        "promote_tiers": promoted,
        "tiers": tiers,
        "devices": [{"role": role, **detail} for role, detail in decisions],
        "rule": {
            "candidate": "best of cpu/gpu median at full resolution misses the pass's own budget",
            "test": "gpuscale median <= budget with the observed scale below 1.0",
            "tier": "promoted only when every device of the tier in this run was a candidate and passed",
            "source": "quality.rs: the scale is opt-in until the farm run earns the default",
        },
    }


FLIP = """
The flip a promoted verdict licenses (quality.rs, `gpu_scale()`): when
`VIEWW_GPU_SCALE` is unset, take `{tiers}`'s profile scale instead of
returning None — `QualityProfile::for_tier(device_facts().tier)
.resolution_scale`, gated to the promoted tiers so a tier the farm has not
tested keeps full resolution. `VIEWW_GPU_SCALE=off` still wins: the
kill-switch outlives the promotion. Commit the verdict JSON beside the
flip (`ci/mobile/`), because the numbers that licensed it are the review.
"""


def memo(verdict):
    """What the human reads: the verdict, per tier, and the licensed flip."""
    lines = [f"gpuscale verdict: {verdict['verdict'].upper()}", ""]
    for tier, entry in sorted(verdict["tiers"].items()):
        lines.append(f"  {tier}: {entry['verdict']} — {entry['why']}")
    lines.append("")
    for device in verdict["devices"]:
        lines.append(f"  {device['device']} [{device['tier']}]: {device['role']} — {device['why']}")
    lines.append(FLIP.format(tiers=", ".join(verdict["promote_tiers"]) or "(none)"))
    return "\n".join(lines)


# ------------------------------------------------------------- self-test
#
# Synthetic rows, clearly labeled, exercising the four ways the rule can
# go. The numbers are shaped like the farm's (an LG Stylo 5 that misses on
# both paths, an A36 that passes on the GPU, an S26 Ultra with room) but
# they are fixtures, not measurements — a verdict produced without
# --self-test and without a summary.json is a setup error, not a result.

FIXTURES = {
    "promote": [
        {"device_name": "LG Stylo 5 (SYNTHETIC)", "mem_total_kb": "1900000", "cpu_cores": "8",
         "cpu_frame_time": "median 36.2ms, worst 68.6ms, budget 16.666667ms (600 samples)",
         "gpu_frame_time": "median 40.1ms, worst 71.0ms, budget 16.666667ms (600 samples)",
         "gpuscale_frame_time": "median 14.2ms, worst 30.0ms, budget 16.666667ms (600 samples)",
         "gpuscale_gpu_scale": "GPU path renders at 0.50x resolution (VIEWW_GPU_SCALE=auto: tier low)"},
        {"device_name": "Galaxy A36 (SYNTHETIC)", "mem_total_kb": "6000000", "cpu_cores": "8",
         "cpu_frame_time": "median 13.9ms, worst 68.6ms, budget 16.666667ms (600 samples)",
         "gpu_frame_time": "median 6.8ms, worst 20.0ms, budget 16.666667ms (600 samples)",
         "gpuscale_frame_time": "median 6.9ms, worst 21.0ms, budget 16.666667ms (600 samples)",
         "gpuscale_gpu_scale": ""},
        {"device_name": "Galaxy S26 Ultra (SYNTHETIC)", "mem_total_kb": "12000000", "cpu_cores": "8",
         "cpu_frame_time": "median 5.6ms, worst 12.0ms, budget 8.333333ms (600 samples)",
         "gpu_frame_time": "median 2.2ms, worst 5.0ms, budget 8.333333ms (600 samples)",
         "gpuscale_frame_time": "median 2.3ms, worst 5.1ms, budget 8.333333ms (600 samples)",
         "gpuscale_gpu_scale": ""},
    ],
    "keep": [
        {"device_name": "LG Stylo 5 (SYNTHETIC)", "mem_total_kb": "1900000", "cpu_cores": "8",
         "cpu_frame_time": "median 36.2ms, worst 68.6ms, budget 16.666667ms (600 samples)",
         "gpu_frame_time": "median 40.1ms, worst 71.0ms, budget 16.666667ms (600 samples)",
         "gpuscale_frame_time": "median 19.5ms, worst 31.0ms, budget 16.666667ms (600 samples)",
         "gpuscale_gpu_scale": "GPU path renders at 0.50x resolution (VIEWW_GPU_SCALE=auto: tier low)"},
    ],
    "incomplete": [
        {"device_name": "LG Stylo 5 (SYNTHETIC)", "mem_total_kb": "1900000", "cpu_cores": "8",
         "cpu_frame_time": "median 36.2ms, worst 68.6ms, budget 16.666667ms (600 samples)",
         "gpu_frame_time": "median 40.1ms, worst 71.0ms, budget 16.666667ms (600 samples)",
         "gpuscale_gpu_scale": ""},
    ],
    # The f91d07c shape: a scaled pass that passes the budget, but the CPU
    # drew every frame. It must not promote anything.
    "cpu_drew_it": [
        {"device_name": "LG Stylo 5 (SYNTHETIC)", "mem_total_kb": "1900000", "cpu_cores": "8",
         "cpu_frame_time": "median 36.2ms, worst 68.6ms, budget 16.666667ms (600 samples)",
         "gpu_frame_time": "median 40.1ms, worst 71.0ms, budget 16.666667ms (600 samples)",
         "gpuscale_frame_time": "median 14.2ms, worst 30.0ms, budget 16.666667ms (600 samples)",
         "gpuscale_gpu_scale": "GPU path renders at 0.50x resolution (VIEWW_GPU_SCALE=auto: tier low)",
         "gpuscale_gpu_frames": "0 of 600 drawn by the GPU rasterizer (0 straight into the swapchain)"},
    ],
    "unneeded": [
        {"device_name": "Galaxy S26 Ultra (SYNTHETIC)", "mem_total_kb": "12000000", "cpu_cores": "8",
         "cpu_frame_time": "median 5.6ms, worst 12.0ms, budget 8.333333ms (600 samples)",
         "gpu_frame_time": "median 2.2ms, worst 5.0ms, budget 8.333333ms (600 samples)",
         "gpuscale_frame_time": "median 2.3ms, worst 5.1ms, budget 8.333333ms (600 samples)",
         "gpuscale_gpu_scale": ""},
    ],
}


def self_test():
    """Each fixture against its expected verdict; non-zero on any miss."""
    expected = {
        "promote": ("promote", ["low"]),
        "keep": ("keep-opt-in", []),
        "incomplete": ("incomplete", []),
        "cpu_drew_it": ("incomplete", []),
        "unneeded": ("keep-opt-in", []),
    }
    failures = []
    for name, rows in FIXTURES.items():
        verdict = decide(rows)
        want, want_tiers = expected[name]
        got = (verdict["verdict"], verdict["promote_tiers"])
        status = "ok" if got == (want, want_tiers) else "FAIL"
        if status == "FAIL":
            failures.append(f"{name}: expected {want}/{want_tiers}, got {got[0]}/{got[1]}")
        print(f"  {name}: {verdict['verdict']} {verdict['promote_tiers']} — {status}")
    # The unit corners the prose leans on: duration parsing, scale parsing,
    # and the tier policy's small-device catch.
    corners = [
        (parse_duration("8.333333ms"), 8.333333),
        (parse_duration("950µs"), 0.95),
        (parse_duration("1.5s"), 1500.0),
        (parse_duration("600 samples"), None),
        (parse_scale("GPU path renders at 0.50x resolution (VIEWW_GPU_SCALE=auto: tier low)"), 0.5),
        (parse_scale("GPU path renders at 1.00x resolution"), None),
        (parse_scale(""), None),
        (tier_of({"mem_total_kb": "1900000", "cpu_cores": "8"}), "low"),
        (tier_of({"mem_total_kb": "6000000", "cpu_cores": "4"}), "low"),
        (tier_of({"mem_total_kb": "6000000", "cpu_cores": "8"}), "medium"),
        (tier_of({"mem_total_kb": "12000000", "cpu_cores": "8"}), "high"),
        (tier_of({}), None),
    ]
    for got, want in corners:
        if got != want:
            failures.append(f"corner: expected {want!r}, got {got!r}")
    if failures:
        print("\ngpuscale_verdict SELF-TEST FAILED:")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print("gpuscale_verdict self-test: all fixtures and corners pass (numbers are SYNTHETIC)")
    return 0


def load_rows(args):
    """Devices from device_farm.py's summary.json, or one result.txt."""
    if args.summary:
        summary = json.loads(pathlib.Path(args.summary).read_text())
        return [row["result"] for row in summary.get("devices", []) if row.get("result")]
    if args.result:
        values = {}
        for line in pathlib.Path(args.result).read_text(errors="replace").splitlines():
            if "=" in line:
                key, value = line.split("=", 1)
                values[key.strip()] = value.strip()
        return [values]
    return []


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--summary", help="device_farm.py's summary.json")
    parser.add_argument("--result", help="one device's result.txt (--summary's per-device unit)")
    parser.add_argument("--out", help="directory for gpuscale-verdict.json")
    parser.add_argument("--self-test", action="store_true", help="run the synthetic fixtures and exit")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    rows = load_rows(args)
    if not rows:
        print("gpuscale_verdict: no device results in --summary/--result", file=sys.stderr)
        return 2
    if not any(row.get("gpuscale_frame_time") or row.get("gpuscale_gpu_scale") for row in rows):
        print("gpuscale_verdict: the run carried no gpuscale pass — nothing to decide")
        return 2

    verdict = decide(rows)
    print(memo(verdict))
    if args.out:
        out = pathlib.Path(args.out)
        out.mkdir(parents=True, exist_ok=True)
        (out / "gpuscale-verdict.json").write_text(json.dumps(verdict, indent=2) + "\n")
        print(f"gpuscale_verdict: wrote {out / 'gpuscale-verdict.json'}")
    return 0 if verdict["verdict"] != "incomplete" else 1


if __name__ == "__main__":
    sys.exit(main())
