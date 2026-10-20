#!/usr/bin/env python3
"""Run vieww's device suite on real phones in AWS Device Farm.

    python3 ci/mobile/device_farm.py --apk dist/vieww-devicefarm.apk --out df-out
    python3 ci/mobile/device_farm.py --platform ios --ipa dist/vieww.ipa --out df-out
        [--project-arn ARN]          default: $DEVICE_FARM_PROJECT_ARN
        [--devices "Pixel 8,Galaxy A14"]  name substrings; default: picked by tier
        [--count 3]                  phones when picking by tier (low ... high)
        [--region us-west-2] [--timeout-min 90] [--video]

What happens:

1. **Phones.** Unless named, the currently HIGHLY_AVAILABLE arm64 Android
   phones (Android 7.0+, the app's minSdk 24) are ranked by memory, then CPU
   clock, and `--count` are taken evenly from the cheapest to the most capable
   — so the default of three is one low-end, one mid-range, one high-end.
2. **Uploads.** The APK, the test package (`device-farm/package.sh`) and the
   test spec (`device-farm/testspec-android.yml`), each waited on until Device
   Farm has validated it. A validation failure prints Device Farm's own reason.
3. **Run.** One run across the phones; on each, `run-android.sh` measures cold
   start three times per path and runs the on-device suite on the CPU path and
   the planned GPU path (see that script).
4. **Results.** Every job's files are downloaded into OUT/<device>/, each
   device's `result.txt` is parsed, and OUT/SUMMARY.md and OUT/summary.json
   say, per phone: tier, SoC, GPU, memory, cold start (median of three) per
   path, frame time, frame-stage medians, GPU-drawn frames, PSS, the suite
   verdicts and any failed checks. When the `gpuscale` pass ran, the
   promotion decision it exists for is computed too (see
   `ci/mobile/gpuscale_verdict.py`): which tiers' quality profiles earned
   their default, which did not, and which devices were missing the data
   to decide.

Exit status: 0 when every phone's gating passes passed, 1 otherwise (2 for a
setup error). Which passes gate is `run-android.sh`'s call: `auto` and `cpu`
always, `gpu` where `auto` chose the GPU; a gating pass that failed only the
checks `device-farm/known-failures.txt` lists for that phone is reported and
does not fail the run. Uses the `aws` CLI and `curl`, both on GitHub's hosted runners, so the
script needs no Python packages.
"""

import argparse
import json
import os
import pathlib
import re
import statistics
import subprocess
import sys
import time
import zipfile

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent  # vieww_base/


class Aws:
    def __init__(self, region):
        self.region = region

    def __call__(self, *args):
        cmd = ["aws", "devicefarm", *args, "--region", self.region, "--output", "json"]
        proc = subprocess.run(cmd, capture_output=True, text=True)
        if proc.returncode != 0:
            raise SystemExit(f"device_farm: `aws devicefarm {args[0]}` failed:\n{proc.stderr.strip()}")
        return json.loads(proc.stdout or "{}")


def say(message):
    print(f"device_farm: {message}", flush=True)


def version_tuple(text):
    return tuple(int(p) for p in re.findall(r"\d+", text or "0")[:3]) or (0,)


def pick_devices(aws, names, count, platform="ANDROID"):
    min_os = (7,) if platform == "ANDROID" else (16,)
    filters = [
        {"attribute": "PLATFORM", "operator": "EQUALS", "values": [platform]},
        {"attribute": "FORM_FACTOR", "operator": "EQUALS", "values": ["PHONE"]},
    ]
    devices = []
    token = None
    while True:
        args = ["list-devices", "--filters", json.dumps(filters)]
        if token:
            args += ["--next-token", token]
        page = aws(*args)
        devices += page.get("devices", [])
        token = page.get("nextToken")
        if not token:
            break

    def arm64(d):
        if platform == "IOS":
            return True  # every iPhone Device Farm still offers is arm64
        arch = (d.get("cpu") or {}).get("architecture", "")
        return "arm64" in arch or "aarch64" in arch

    if names:
        chosen = []
        for wanted in names:
            matches = [d for d in devices if wanted.lower() in d["name"].lower()]
            if not matches:
                raise SystemExit(f"device_farm: no {platform} phone matches {wanted!r}")
            # The newest OS of that name, and a highly available one if any.
            matches.sort(key=lambda d: (d.get("availability") == "HIGHLY_AVAILABLE", version_tuple(d.get("os"))))
            chosen.append(dict(matches[-1], tier="named"))
        return chosen

    usable = [
        d
        for d in devices
        if d.get("availability") == "HIGHLY_AVAILABLE" and arm64(d) and version_tuple(d.get("os")) >= min_os
    ]
    if not usable:
        raise SystemExit(f"device_farm: no highly available {platform} phone (OS {min_os[0]}+) in the device list")
    usable.sort(key=lambda d: (d.get("memory") or 0, (d.get("cpu") or {}).get("clock") or 0, version_tuple(d.get("os"))))
    count = max(1, min(count, len(usable)))
    if count == 1:
        picks = [len(usable) // 2]
    else:
        picks = sorted({round(i * (len(usable) - 1) / (count - 1)) for i in range(count)})
    tiers = ["low"] + ["mid"] * (len(picks) - 2) + ["high"] if len(picks) > 1 else ["mid"]
    return [dict(usable[i], tier=tier) for i, tier in zip(picks, tiers)]


def upload(aws, project_arn, path, kind, name=None):
    path = pathlib.Path(path)
    created = aws("create-upload", "--project-arn", project_arn, "--name", name or path.name, "--type", kind)["upload"]
    put = subprocess.run(["curl", "-sS", "--fail", "-T", str(path), created["url"]], capture_output=True, text=True)
    if put.returncode != 0:
        raise SystemExit(f"device_farm: uploading {path.name} failed: {put.stderr.strip()}")
    for _ in range(120):
        state = aws("get-upload", "--arn", created["arn"])["upload"]
        if state["status"] == "SUCCEEDED":
            say(f"uploaded {path.name} as {kind}")
            return created["arn"]
        if state["status"] == "FAILED":
            raise SystemExit(
                f"device_farm: Device Farm rejected {path.name} ({kind}): "
                f"{state.get('message') or state.get('metadata') or 'no reason given'}"
            )
        time.sleep(5)
    raise SystemExit(f"device_farm: {path.name} was still processing after 10 minutes")


def slug(text):
    return re.sub(r"[^A-Za-z0-9._-]+", "_", text).strip("_") or "device"


def parse_result(path):
    values = {}
    for line in path.read_text(errors="replace").splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            values[key.strip()] = value.strip()
    return values


def gpu_frames_drawn(text):
    """`"600 of 600 drawn by the GPU rasterizer (…)"` → 600; None when the
    pass did not report it."""
    m = re.match(r"\s*(\d+) of \d+", text or "")
    return int(m.group(1)) if m else None


def median_of(text):
    # Only a list of numbers is a measurement: "n/a (no console below iOS 17)"
    # once summarised as a 17 ms start-up.
    if not re.fullmatch(r"[\d.\s]+", (text or "").strip() or "x"):
        return None
    numbers = [float(n) for n in re.findall(r"\d+(?:\.\d+)?", text or "")]
    return statistics.median(numbers) if numbers else None


def download_job(aws, job, out):
    target = out / slug(job["device"]["name"] + "-" + job["device"].get("os", ""))
    target.mkdir(parents=True, exist_ok=True)
    artifacts = aws("list-artifacts", "--arn", job["arn"], "--type", "FILE").get("artifacts", [])
    for artifact in artifacts:
        name = slug(f"{artifact['name']}.{artifact.get('extension', 'bin')}")
        dest = target / name
        subprocess.run(["curl", "-sS", "--fail", "-o", str(dest), artifact["url"]], check=False)
        if dest.suffix == ".zip" and dest.exists():
            try:
                with zipfile.ZipFile(dest) as archive:
                    archive.extractall(target / dest.stem)
            except zipfile.BadZipFile:
                pass
    results = list(target.rglob("result.txt"))
    return target, (parse_result(results[0]) if results else None)


def duration_ms(text):
    """`"37.8ms"` / `"950µs"` / `"1.2s"` → milliseconds; None otherwise."""
    m = re.fullmatch(r"\s*([0-9.]+)\s*(ms|µs|us|s)\s*", text or "")
    if not m:
        return None
    value = float(m.group(1))
    return {"ms": value, "µs": value / 1000, "us": value / 1000, "s": value * 1000}[m.group(2)]


def frame_median_ms(text):
    m = re.search(r"median ([0-9.]+\s*(?:ms|µs|us|s))\b", text or "")
    return duration_ms(m.group(1)) if m else None


def raster_stage_ms(text):
    """The `cpu` stage out of a `frame_stages` line ("medians ms: ... cpu 34.21 ...")."""
    m = re.search(r"\bcpu ([0-9.]+)", text or "")
    return float(m.group(1)) if m else None


# The A/B passes, in the order the table prints them: what each switches
# off relative to `cpu`, which has all three on.
RASTER_AB = [
    ("cpu", "nothing (all three on)"),
    ("cpunothreads", "row-parallel walk"),
    ("cpunofill", "path-fill cache"),
    ("cpunoblur", "blur cache"),
    ("cpubare", "all three"),
]


def raster_ab_section(rows):
    """Per phone, the CPU path with each 2026-10-18/19 raster addition
    switched off in turn, against the same build with all of them on — same
    unit, same run, so the deltas are the features' and not the phone's.
    Negative is better (less time, less memory). Empty when no A/B pass ran."""
    lines = []
    for row in rows:
        r = row["result"] or {}
        if not any(r.get(f"{mode}_frame_time") for mode, _ in RASTER_AB[1:]):
            continue
        base = r.get("cpu_frame_time"), r.get("cpu_frame_stages"), r.get("cpu_total_pss_kb")
        base_frame, base_raster = frame_median_ms(base[0]), raster_stage_ms(base[1])
        base_pss = int(base[2]) / 1024 if (base[2] or "").isdigit() else None

        def cell(value, reference, unit, digits):
            if value is None:
                return "–"
            text = f"{value:.{digits}f} {unit}"
            if reference is not None:
                text += f" ({value - reference:+.{digits}f})"
            return text

        if not lines:
            lines += [
                "## Raster A/B: the 2026-10-18/19 CPU additions, one at a time",
                "",
                "Same phone, same run, same build; only the named feature is off. "
                "Negative deltas mean switching it off helped — the feature costs that much.",
                "",
            ]
        lines += [
            f"**{row['device']}**",
            "",
            "| pass | switched off | frame median | raster stage median | PSS |",
            "|---|---|---|---|---|",
        ]
        for mode, off in RASTER_AB:
            frame = frame_median_ms(r.get(f"{mode}_frame_time"))
            raster = raster_stage_ms(r.get(f"{mode}_frame_stages"))
            pss_raw = r.get(f"{mode}_total_pss_kb") or ""
            pss = int(pss_raw) / 1024 if pss_raw.isdigit() else None
            if mode == "cpu":
                frame_c, raster_c, pss_c = cell(frame, None, "ms", 2), cell(raster, None, "ms", 2), cell(pss, None, "MB", 0)
            else:
                frame_c = cell(frame, base_frame, "ms", 2)
                raster_c = cell(raster, base_raster, "ms", 2)
                pss_c = cell(pss, base_pss, "MB", 0)
            lines.append(f"| `{mode}` | {off} | {frame_c} | {raster_c} | {pss_c} |")
        lines.append("")
    return lines


def summarize(rows, out, run):
    lines = [
        "# vieww on AWS Device Farm",
        "",
        f"run: `{run.get('arn', '?')}` — {run.get('result', '?')}",
        "",
        "| phone | tier | Android | SoC / GPU | RAM | APK | cold start cpu / gpu (ms, median of 3) "
        "| frame time cpu | frame time gpu | frames drawn by the GPU, cpu pass / gpu pass | PSS cpu / gpu (MB) | suite cpu / gpu / auto |",
        "|---|---|---|---|---|---|---|---|---|---|---|---|",
    ]
    for row in rows:
        r = row["result"] or {}

        def mb(key):
            v = r.get(key)
            return f"{int(v) / 1024:.0f}" if v and v.isdigit() else "–"

        cold = " / ".join(
            f"{median_of(r.get(f'{m}_cold_start_ms')):.0f}" if median_of(r.get(f"{m}_cold_start_ms")) else "–"
            for m in ("cpu", "gpu")
        )
        mem = r.get("mem_total_kb")
        if not r.get("cpu_cold_start_ms") and r.get("cpu_first_frame_ms"):
            # iOS reports the app's own first-frame time instead of `am start -W`.
            cold = " / ".join(
                f"{median_of(r.get(f'{m}_first_frame_ms')):.0f}" if median_of(r.get(f"{m}_first_frame_ms")) else "–"
                for m in ("cpu", "gpu")
            )
        lines.append(
            "| {phone} | {tier} | {android} | {soc}<br>{gpu} | {ram} | {apk} | {cold} | {ftc} | {ftg} | {gf} | {pss} | {suite} |".format(
                phone=row["device"],
                tier=row["tier"],
                android=r.get("android") or r.get("ios") or row.get("os", "?"),
                soc=r.get("soc", "?"),
                gpu=(r.get("gpu") or "?")[:60],
                ram=f"{int(mem) / 1048576:.1f} GB" if mem and mem.isdigit() else "?",
                apk=next(
                    (f"{int(r[k]) / 1048576:.1f} MB" for k in ("apk_bytes", "ipa_bytes") if r.get(k, "").isdigit()),
                    "–",
                ),
                cold=cold,
                ftc=r.get("cpu_frame_time", "–"),
                ftg=r.get("gpu_frame_time", "–"),
                gf=" / ".join((r.get(f"{m}_gpu_frames") or "–").split(" drawn")[0] for m in ("cpu", "gpu")),
                pss=f"{mb('cpu_total_pss_kb')} / {mb('gpu_total_pss_kb')}",
                suite=f"{r.get('cpu_suite', '–')} / {r.get('gpu_suite', '–')} / {r.get('auto_suite', '–')}"
                if row["result"]
                else f"no result ({row['job_result']})",
            )
        )
    lines.append("")
    for row in rows:
        r = row["result"] or {}
        failed = " ".join(
            filter(None, [r.get(f"{m}_failed_checks") for m in ("cpu", "gpu", "auto")])
        ).strip()
        passes = [
            ("cpu", "CPU path"),
            ("gpu", "GPU path"),
            ("auto", "chosen by measurement (auto)"),
            ("cpuvk", "CPU path presented through Vulkan (diagnostic)"),
            ("cpuhint", "CPU path with an Android performance hint (diagnostic)"),
            ("gpuscale", "GPU path at the device tier's resolution (diagnostic)"),
            ("cpubare", "CPU path without raster threads, fill cache or blur cache (diagnostic A/B)"),
            ("cpunothreads", "CPU path without the row-parallel walk only (diagnostic A/B)"),
            ("cpunofill", "CPU path without the path-fill cache only (diagnostic A/B)"),
            ("cpunoblur", "CPU path without the blur cache only (diagnostic A/B)"),
        ]
        detail = []
        for mode, label in passes:
            if not any(r.get(f"{mode}_{k}") for k in ("frame_time", "frame_stages", "startup_stages")):
                continue
            detail.append(f"- {label}: {r.get(f'{mode}_frame_time', '–')}")
            # A pass labelled GPU whose frames the CPU drew is a mislabelled
            # number, not a slow GPU — run f91d07c printed two of those.
            if mode in ("gpu", "gpuscale") and gpu_frames_drawn(r.get(f"{mode}_gpu_frames")) == 0:
                detail.append("  - **not a GPU measurement**: 0 frames were drawn by the GPU rasterizer in this pass")
            # run-android.sh compares the app's own `rasterizer:` observation
            # with what the pass asked for, and fails the run on a mismatch.
            if r.get(f"{mode}_rasterizer_mismatch"):
                detail.append(f"  - **wrong rasterizer, run failed**: {r[f'{mode}_rasterizer_mismatch']}")
            # run-android.sh's gating verdict for this pass, when it gates.
            if r.get(f"{mode}_gate_failed"):
                detail.append(f"  - **gate failed, run failed**: {r[f'{mode}_gate_failed']}")
            if r.get(f"{mode}_known_failure"):
                detail.append(f"  - known failure (known-failures.txt, not failing the run): {r[f'{mode}_known_failure']}")
            if r.get(f"{mode}_known_failure_passed"):
                detail.append(f"  - **passed despite a known-failures.txt entry**: {r[f'{mode}_known_failure_passed']}")
            if r.get(f"{mode}_cadence"):
                detail.append(f"  - frame rate: {r[f'{mode}_cadence']}")
            pss = r.get(f"{mode}_total_pss_kb")
            if pss and pss.isdigit():
                detail.append(f"  - memory: PSS {int(pss) / 1024:.0f} MB")
            for key, name in (
                ("renderer_choice", "renderer"),
                ("calibration", "calibration launches"),
                ("startup_stages", "start-up"),
                ("frame_stages", "stages"),
                ("frame_stages_p95", "stages"),
                ("worst_frame", "worst frame"),
                ("raster_by_core", "raster by core"),
                ("cpu_clocks", "clocks at report"),
                ("perf_hint", "performance hint"),
                ("pipeline_cache", "pipeline cache"),
                ("device", "device profile"),
                ("gpu_scale", "gpu resolution"),
                ("cache_budgets", "cache budgets"),
                ("quality_overlay", "quality overlay"),
            ):
                if r.get(f"{mode}_{key}"):
                    detail.append(f"  - {name}: {r[f'{mode}_{key}']}")
        if failed or detail:
            lines.append(f"**{row['device']}**")
            if r.get("gating_passes"):
                lines.append(f"- gating passes: {r['gating_passes']}")
            lines.extend(detail)
            if failed:
                lines.append(f"- failed checks: {failed}")
            lines.append("")
    lines.extend(raster_ab_section(rows))
    lines.append(
        "Targets from the audit, for reading the table: product UI p95 under 15 ms and cold start "
        "under 120 ms on a mid-tier phone; release APK under 6 MB."
    )
    (out / "SUMMARY.md").write_text("\n".join(lines) + "\n")
    (out / "summary.json").write_text(
        json.dumps({"run": {k: run.get(k) for k in ("arn", "status", "result", "counters")}, "devices": rows}, indent=2)
        + "\n"
    )
    print("\n".join(lines))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--platform", choices=["android", "ios"], default="android")
    parser.add_argument("--apk", help="the Android app (--platform android)")
    parser.add_argument("--ipa", help="the iOS app (--platform ios)")
    parser.add_argument("--out", required=True)
    parser.add_argument("--project-arn", default=os.environ.get("DEVICE_FARM_PROJECT_ARN"))
    parser.add_argument("--devices", default="")
    parser.add_argument("--count", type=int, default=3)
    parser.add_argument("--region", default=os.environ.get("DEVICE_FARM_REGION", "us-west-2"))
    parser.add_argument("--timeout-min", type=int, default=90)
    parser.add_argument("--video", action="store_true")
    parser.add_argument("--run-name", default=os.environ.get("GITHUB_SHA", "local")[:12])
    args = parser.parse_args()
    if not args.project_arn:
        print("device_farm: no project ARN (--project-arn or DEVICE_FARM_PROJECT_ARN)", file=sys.stderr)
        return 2
    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    aws = Aws(args.region)

    ios = args.platform == "ios"
    app_path = args.ipa if ios else args.apk
    if not app_path:
        print(f"device_farm: --{'ipa' if ios else 'apk'} is required for --platform {args.platform}", file=sys.stderr)
        return 2
    names = [n.strip() for n in args.devices.split(",") if n.strip()]
    devices = pick_devices(aws, names, args.count, "IOS" if ios else "ANDROID")
    for d in devices:
        say(f"phone: {d['name']} ({'iOS' if ios else 'Android'} {d.get('os')}, {(d.get('memory') or 0) / 2**30:.1f} GB, {d['tier']})")

    package = out / "vieww-device-farm-tests.zip"
    subprocess.run(["bash", str(HERE / "device-farm" / "package.sh"), str(package)], check=True)
    app_arn = upload(aws, args.project_arn, app_path, "IOS_APP" if ios else "ANDROID_APP")
    package_arn = upload(aws, args.project_arn, package, "APPIUM_PYTHON_TEST_PACKAGE")
    spec_arn = upload(
        aws,
        args.project_arn,
        HERE / "device-farm" / ("testspec-ios.yml" if ios else "testspec-android.yml"),
        "APPIUM_PYTHON_TEST_SPEC",
    )

    run = aws(
        "schedule-run",
        "--project-arn", args.project_arn,
        "--app-arn", app_arn,
        "--name", f"vieww {args.run_name}",
        "--test", json.dumps({"type": "APPIUM_PYTHON", "testPackageArn": package_arn, "testSpecArn": spec_arn}),
        "--device-selection-configuration",
        json.dumps(
            {
                "filters": [{"attribute": "ARN", "operator": "IN", "values": [d["arn"] for d in devices]}],
                "maxDevices": len(devices),
            }
        ),
        "--execution-configuration",
        json.dumps({"jobTimeoutMinutes": 30, "videoCapture": bool(args.video)}),
    )["run"]
    say(f"scheduled {run['arn']}")
    (out / "run-arn.txt").write_text(run["arn"] + "\n")

    deadline = time.time() + args.timeout_min * 60
    while run.get("status") != "COMPLETED":
        if time.time() > deadline:
            say(f"run still {run.get('status')} after {args.timeout_min} min — stopping it")
            aws("stop-run", "--arn", run["arn"])
            break
        time.sleep(30)
        run = aws("get-run", "--arn", run["arn"])["run"]
        say(f"run {run.get('status')} {run.get('counters', {})}")

    tiers = {d["arn"]: d["tier"] for d in devices}
    rows = []
    for job in aws("list-jobs", "--arn", run["arn"]).get("jobs", []):
        folder, result = download_job(aws, job, out)
        rows.append(
            {
                "device": job["device"]["name"],
                "os": job["device"].get("os"),
                "tier": tiers.get(job["device"]["arn"], "?"),
                "job_result": job.get("result"),
                "folder": str(folder.relative_to(out)),
                "result": result,
            }
        )
    rows.sort(key=lambda r: ["low", "mid", "high", "named", "?"].index(r["tier"]))
    summarize(rows, out, run)

    # The decision the gpuscale pass exists for, computed while the numbers
    # are fresh: which tiers' quality profiles earned their default. Non-
    # failing either way — a run without the pass, or with data the rule
    # calls incomplete, still says so in the verdict file and the memo.
    if any((r["result"] or {}).get("gpuscale_frame_time") for r in rows):
        proc = subprocess.run(
            [sys.executable, str(HERE / "gpuscale_verdict.py"), "--summary", str(out / "summary.json"), "--out", str(out)],
            capture_output=True, text=True,
        )
        if proc.stdout.strip():
            print(proc.stdout.strip())
            with open(out / "SUMMARY.md", "a") as summary:
                summary.write("\n## gpuscale promotion verdict\n\n```\n" + proc.stdout.strip() + "\n```\n")
        if proc.returncode not in (0, 1):
            say(f"gpuscale_verdict could not run: {proc.stderr.strip()}")

    ok = rows and all(r["result"] and r["result"].get("status") == "PASS" for r in rows)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
