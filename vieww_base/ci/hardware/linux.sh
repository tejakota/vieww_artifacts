#!/usr/bin/env bash
#
# Everything a Linux machine with a real GPU and a display can close, in one
# unattended run. The Windows twin is `ci/hardware/windows.ps1`; the phone,
# TalkBack and two-monitor steps stay in `ci/hardware/close-remaining.sh`
# because they need a person.
#
#   ci/hardware/linux.sh                    # every step
#   ci/hardware/linux.sh baseline stress    # only some
#
# Steps, and what each closes (PENDING.md / the audit's P0 list):
#   env       the machine, driver, toolchain and RAPL state, recorded first
#   gpu       §1.1  Vulkan suites, vieww-3d GPU parity, a GPU frame in a real
#                   window — on this machine's own driver
#   nvidia    §1.1  the same suites on a discrete NVIDIA GPU (PRIME offload)
#   gallery   §1.1 §2.3 §4.1  CPU gallery + GPU census, 30 runs each, with
#                   receipts and planner shadow logs
#   baseline  P0    a real window, launched N times on the CPU path and N on
#                   the GPU path: launch-to-first-frame, RSS, idle frames / CPU /
#                   power, animated fps, frame-stage medians, marginal energy
#                   per animated frame (`examples/baseline.rs`)
#   size      P0    shipped (`--profile dist`, static std) size of that windowed app, and
#                   the framework-core size gate (`ci/check/size-budget.sh`)
#   stress    §1.1  the animation stress suite five times on this fixed
#                   machine: one reproducible p95, with its spread
#   desktop   —     the on-desktop suite (`ci/certify/desktop-suite.sh`)
#
# Environment:
#   VIEWW_HW_OUT=dir         results directory (default hardware-results/linux-<stamp>)
#   VIEWW_HW_LAUNCHES=5      baseline launches per path
#   VIEWW_HW_DROP_CACHES=1   drop the page cache before each launch (needs
#                            passwordless sudo) — a true cold start; otherwise
#                            the start-up figures are warm-cache and say so
#
# Needs: the pinned toolchain, a Vulkan driver, a graphical session (DISPLAY or
# WAYLAND_DISPLAY) for the window steps, python3 for the summaries. For energy:
# RAPL is root-only since 2020 — `sudo chmod o+r
# /sys/class/powercap/intel-rapl:*/energy_uj` once per boot, or run the
# `env` step's printed command. As a GitHub self-hosted runner: start the
# runner from inside the desktop session (`./run.sh` in a terminal), not as a
# system service, or the window steps have no display to open on.
#
# Every step runs even when an earlier one failed. Exit status is non-zero if
# any step failed; a step this machine cannot run is SKIP, never PASS.

set -uo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root" || exit 1
stamp="$(date +%Y%m%d-%H%M%S)"
out="${VIEWW_HW_OUT:-$root/hardware-results/linux-$stamp}"
mkdir -p "$out"
summary="$out/summary.txt"
launches="${VIEWW_HW_LAUNCHES:-5}"
failed=0

steps=("$@")
[ ${#steps[@]} -eq 0 ] && steps=(env gpu nvidia gallery baseline size stress desktop)
want() { local s; for s in "${steps[@]}"; do [ "$s" = "$1" ] && return 0; done; return 1; }

# run NAME CMD... — run, tee to NAME.log, record pass/fail.
run() {
    local name="$1"; shift
    echo
    echo "==> $name: $*"
    if "$@" 2>&1 | tee "$out/$name.log"; then
        echo "PASS  $name" | tee -a "$summary"
    else
        echo "FAIL  $name (see $name.log)" | tee -a "$summary"
        failed=1
    fi
}
skip() { echo "SKIP  $1 ($2)" | tee -a "$summary"; }
note() { echo "      $*" | tee -a "$summary"; }

has_display() { [ -n "${DISPLAY:-}" ] || [ -n "${WAYLAND_DISPLAY:-}" ]; }

{
    echo "vieww Linux hardware run $stamp"
    echo "commit: $(git rev-parse HEAD 2>/dev/null || echo unknown)$(git diff --quiet 2>/dev/null || echo ' (dirty)')"
    echo "host: $(hostname) · kernel $(uname -r)"
    echo "cpu: $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ //')"
    echo "session: ${XDG_SESSION_TYPE:-none} ${XDG_CURRENT_DESKTOP:-}"
    echo "rustc: $(rustc -V 2>/dev/null || echo 'NOT RESOLVED — rustup could not provide the pinned toolchain') (pinned: $(grep -m1 '^channel' rust-toolchain.toml | cut -d'"' -f2))"
    echo
} > "$summary"
# The real driver, never a leftover software ICD from a CI-style shell.
unset VK_ICD_FILENAMES VK_DRIVER_FILES

if want env; then
    command -v vulkaninfo >/dev/null && vulkaninfo --summary > "$out/vulkaninfo.txt" 2>&1
    command -v lspci >/dev/null && lspci | grep -Ei 'vga|3d|display' > "$out/gpus.txt" 2>&1
    {
        echo "governor: $(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo unknown)"
        case "$(cat /sys/class/power_supply/A*/online 2>/dev/null | head -1)" in
            1) echo "power: on mains" ;;
            0) echo "power: ON BATTERY — throttling and power saving will move every number" ;;
            *) echo "power: unknown (no AC adapter reported)" ;;
        esac
        if [ -r /sys/class/powercap/intel-rapl:0/energy_uj ]; then
            echo "rapl: readable"
        elif [ -e /sys/class/powercap/intel-rapl:0/energy_uj ]; then
            echo "rapl: present but root-only — energy will be null. To enable for this boot:"
            echo "      sudo chmod o+r /sys/class/powercap/intel-rapl:*/energy_uj"
        else
            echo "rapl: absent — energy will be null"
        fi
        [ -f "$out/vulkaninfo.txt" ] && grep -m3 -E 'deviceName|driverName' "$out/vulkaninfo.txt" | sed 's/^ */vulkan: /'
    } | tee -a "$summary"
    pinned="$(grep -m1 '^channel' rust-toolchain.toml | cut -d'"' -f2)"
    if ! rustc -V 2>/dev/null | grep -q " $pinned "; then
        note "WARNING: rustc is not the pinned $pinned — numbers are for a different compiler"
    fi
    if grep -qiE 'llvmpipe|lavapipe|swiftshader' "$out/vulkaninfo.txt" 2>/dev/null &&
       ! grep -qiE 'DISCRETE_GPU|INTEGRATED_GPU' "$out/vulkaninfo.txt" 2>/dev/null; then
        note "WARNING: only a software Vulkan device is visible — GPU numbers would be a CPU's"
    fi
fi

if want gpu; then
    run gpu-hal cargo test --release -p vieww-hal --features vulkan -- --ignored --test-threads=1
    run gpu-3d  cargo test --release -p vieww-3d  --features vulkan -- --ignored --test-threads=1
    if has_display; then
        run gpu-window env VIEWW_REQUIRE_HARDWARE=display,gpu \
            cargo test --release -p vieww-platform-winit --test wait_loop
    else
        skip gpu-window "no DISPLAY or WAYLAND_DISPLAY"
    fi
fi

if want nvidia; then
    if command -v nvidia-smi >/dev/null 2>&1 || lspci 2>/dev/null | grep -qi nvidia; then
        run nvidia-hal env __NV_PRIME_RENDER_OFFLOAD=1 __VK_LAYER_NV_optimus=NVIDIA_only \
            cargo test --release -p vieww-hal --features vulkan -- --ignored --test-threads=1
        run nvidia-census env __NV_PRIME_RENDER_OFFLOAD=1 __VK_LAYER_NV_optimus=NVIDIA_only \
            cargo run --release -p fixtures -- "$out/nvidia-census" --census \
            --runs 30 --receipt "$out/nvidia-census/receipt.json" --shadow "$out/nvidia-census/shadow.jsonl"
    else
        skip nvidia "no NVIDIA GPU found"
    fi
fi

if want gallery; then
    run gallery cargo run --release -p fixtures -- "$out/fixtures" \
        --runs 30 --receipt "$out/fixtures/receipt.json" --shadow "$out/fixtures/shadow.jsonl"
    run census cargo run --release -p fixtures -- "$out/census" --census \
        --runs 30 --receipt "$out/census/receipt.json" --shadow "$out/census/shadow.jsonl"
    if command -v python3 >/dev/null && [ -f "$out/census/receipt.json" ]; then
        python3 - "$out/census/receipt.json" <<'PY' | tee -a "$summary"
import json, sys
r = json.load(open(sys.argv[1]))
g = r.get("gpu") or {}
o = r["outcome"]
print(f"      census: gpu={g.get('name')} [{g.get('device_type')}] "
      f"rendered_on_gpu={o['rendered_on_gpu']}/{o['fixtures']} parity={o['parity_verdict']}")
cpu = sum(f["cpu_best_ms"] for f in r["fixtures"] if f.get("gpu_best_ms") is not None)
gpu = sum(f["gpu_best_ms"] for f in r["fixtures"] if f.get("gpu_best_ms") is not None)
if gpu:
    print(f"      census: over GPU-drawn fixtures cpu {cpu:.1f} ms vs gpu {gpu:.1f} ms (gpu includes readback) = {cpu/gpu:.2f}x")
PY
    fi
fi

if want baseline; then
    if ! has_display; then
        skip baseline "no DISPLAY or WAYLAND_DISPLAY — run the runner inside the desktop session"
    elif run baseline-build cargo build --release -p vieww-platform-winit --example baseline; then
        mkdir -p "$out/baseline"
        bin="$root/target/release/examples/baseline"
        cold="warm-cache"
        if [ "${VIEWW_HW_DROP_CACHES:-0}" = 1 ]; then
            if sudo -n true 2>/dev/null; then cold="cold (page cache dropped before each launch)"
            else note "VIEWW_HW_DROP_CACHES=1 but sudo needs a password — falling back to warm-cache"; fi
        fi
        note "baseline start-up is $cold"
        for mode in cpu gpu; do
            ok=1
            for i in $(seq 1 "$launches"); do
                if [ "$cold" != "warm-cache" ]; then
                    sync; echo 3 | sudo -n tee /proc/sys/vm/drop_caches >/dev/null
                fi
                flag=(); env_extra=()
                if [ "$mode" = gpu ]; then
                    flag=(--gpu)
                    env_extra=(VIEWW_PLANNER_SHADOW="$out/baseline/gpu-shadow.jsonl")
                fi
                if ! env "${env_extra[@]}" VIEWW_BASELINE_LAUNCH_UNIX_NS="$(date +%s%N)" \
                    timeout 120 "$bin" "${flag[@]}" --out "$out/baseline/$mode-$i.json" \
                    >> "$out/baseline-$mode.log" 2>&1; then
                    ok=0
                fi
            done
            if [ "$ok" = 1 ]; then echo "PASS  baseline-$mode ($launches launches)" | tee -a "$summary"
            else echo "FAIL  baseline-$mode (see baseline-$mode.log)" | tee -a "$summary"; failed=1; fi
        done
        if command -v python3 >/dev/null; then
            python3 "$root/ci/hardware/summarize_baseline.py" "$out/baseline" | tee "$out/baseline/summary.txt" | sed 's/^/      /' | tee -a "$summary"
        fi
    fi
fi

if want size; then
    # What ships: `--profile dist` (Cargo.toml — LTO, stripped, dependencies
    # at opt-level "s") with **static** std. `.cargo/config.toml` links libstd
    # dynamically on Linux for the studio's cdylib previews, which a shipped
    # app would not (and which LTO refuses); exporting RUSTFLAGS replaces that
    # block, so this keeps only the x86-64-v2 baseline.
    run size-build env RUSTFLAGS="-C target-cpu=x86-64-v2" \
        cargo build --profile dist -p vieww-platform-winit --example baseline --target-dir target/size
    app="$root/target/size/dist/examples/baseline"
    if [ -f "$app" ]; then
        bytes="$(stat -c %s "$app")"
        echo "$bytes" > "$out/size-baseline-bytes.txt"
        ldd "$app" > "$out/size-baseline-ldd.txt" 2>&1
        note "windowed baseline app, dist profile, static std: $bytes bytes ($(numfmt --to=iec "$bytes" 2>/dev/null || echo "$bytes B"))"
        note "dynamic libraries it loads: $(grep -c '=>' "$out/size-baseline-ldd.txt") (see size-baseline-ldd.txt)"
    fi
    run size-budget ci/check/size-budget.sh
fi

if want stress; then
    p95s=()
    for i in 1 2 3 4 5; do
        run "stress-$i" cargo run --release -p test-animation-stress -- "$out/stress-$i"
        v="$(grep -m1 '^render_p95_ms=' "$out/stress-$i/metrics.txt" 2>/dev/null | cut -d= -f2)"
        [ -n "$v" ] && p95s+=("$v")
    done
    if [ ${#p95s[@]} -gt 0 ]; then
        note "animation stress p95 over ${#p95s[@]} runs (ms): $(printf '%s\n' "${p95s[@]}" | sort -n | tr '\n' ' ')"
    fi
fi

if want desktop; then
    if has_display; then
        run desktop ci/certify/desktop-suite.sh --log "$out/desktop-app.log"
    else
        skip desktop "no DISPLAY or WAYLAND_DISPLAY"
    fi
fi

echo
echo "================ summary ================"
cat "$summary"
echo
echo "results: $out"
exit "$failed"
