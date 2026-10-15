#!/usr/bin/env bash
#
# Runs ON the AWS Device Farm Android host, once per device, from the test
# spec (`testspec-android.yml`). Device Farm has already installed the APK.
# Never run by hand; `ci/mobile/device_farm.py` packages and schedules it.
#
# For each rasterizer path (CPU, then the planned GPU path):
#   1. cold start x3: force-stop, `am start -W`, record TotalTime — the
#      platform's own launch-to-first-frame figure
#   2. relaunch and run the existing on-device suite
#      (`ci/mobile/device-suite.sh --no-build`): taps where the app says its
#      widgets are, waits for the app's own verdict, and reports frame_time,
#      frame_stages, gpu_frames and every check
#   3. memory (`dumpsys meminfo`), a screenshot, the app's logcat
#
# Everything lands in $DEVICEFARM_LOG_DIR/vieww/, which Device Farm returns as
# the job's customer artifacts; `result.txt` there is what device_farm.py
# reads. Exits non-zero if either suite failed, so Device Farm marks the job
# failed — after everything has been collected.

set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
# The package mirrors the repository's layout from vieww_base/ down, so
# device-suite.sh finds android-env.sh where it expects it.
root="$(cd "$here/../../.." && pwd)"
out="${DEVICEFARM_LOG_DIR:-/tmp}/vieww"
mkdir -p "$out"
result="$out/result.txt"
: > "$result"
package="dev.vieww.demo"
export ANDROID_SERIAL="${DEVICEFARM_DEVICE_UDID:-}"
[ -n "$ANDROID_SERIAL" ] || unset ANDROID_SERIAL

kv() { echo "$1=$2" >> "$result"; }
sh_() { adb shell "$@" 2>/dev/null | tr -d '\r'; }

# ── the device, as it describes itself ───────────────────────────────────────
kv device_name "${DEVICEFARM_DEVICE_NAME:-unknown}"
kv manufacturer "$(sh_ getprop ro.product.manufacturer)"
kv model "$(sh_ getprop ro.product.model)"
kv android "$(sh_ getprop ro.build.version.release)"
kv sdk "$(sh_ getprop ro.build.version.sdk)"
soc_model="$(sh_ getprop ro.soc.model)"; platform="$(sh_ getprop ro.board.platform)"
kv soc "${soc_model:-$platform}${soc_model:+ ($platform)}"
kv abi "$(sh_ getprop ro.product.cpu.abi)"
kv cpu_cores "$(sh_ 'grep -c ^processor /proc/cpuinfo')"
kv mem_total_kb "$(sh_ 'grep MemTotal /proc/meminfo' | awk '{print $2}')"
# `GLES: vendor, renderer, version`. Android 13+ also prints a
# "------------RE GLES------------" banner first, so match the field, not
# the word; fall back to the Vulkan/EGL hardware names.
gpu_line="$(sh_ dumpsys SurfaceFlinger | grep -m1 -E '^[[:space:]]*GLES: ' | sed 's/^[[:space:]]*GLES: //')"
[ -n "$gpu_line" ] || gpu_line="vulkan:$(sh_ getprop ro.hardware.vulkan) egl:$(sh_ getprop ro.hardware.egl)"
kv gpu "$gpu_line"
kv display "$(sh_ wm size | awk -F': ' '/Physical/ {print $2}')"
kv density "$(sh_ wm density | awk -F': ' '/Physical/ {print $2}')"
if [ -n "${DEVICEFARM_APP_PATH:-}" ] && [ -f "$DEVICEFARM_APP_PATH" ]; then
    kv apk_bytes "$(stat -c %s "$DEVICEFARM_APP_PATH")"
fi

# Keep the screen on and dialogs off for the whole run: a dark screen draws no
# frames, and the suite needs 600 of them.
sh_ svc power stayon true >/dev/null
sh_ settings put global hide_error_dialogs 1 >/dev/null
sh_ input keyevent KEYCODE_WAKEUP >/dev/null
sh_ wm dismiss-keyguard >/dev/null

# Device Farm installs the app; make sure, without failing if it already is.
if ! sh_ pm list packages | grep -q "^package:$package$"; then
    [ -n "${DEVICEFARM_APP_PATH:-}" ] && adb install -r -g "$DEVICEFARM_APP_PATH" > "$out/install.log" 2>&1
fi

component="$(sh_ cmd package resolve-activity --brief "$package" | tail -1)"
case "$component" in */*) ;; *) component="$package/android.app.NativeActivity" ;; esac
kv component "$component"

status=0
for mode in cpu gpu; do
    gpu=0; [ "$mode" = gpu ] && gpu=1
    sh_ setprop debug.vieww.gpu "$gpu" >/dev/null

    # 1. Cold start, three times. TotalTime is the platform's own measure of
    # launch to first frame drawn; WaitTime adds the system's share.
    starts=()
    for i in 1 2 3; do
        sh_ am force-stop "$package" >/dev/null
        sleep 2
        line="$(sh_ am start -W -n "$component" | tee -a "$out/$mode-am-start.txt" | grep -m1 '^TotalTime' | awk '{print $2}')"
        [ -n "$line" ] && starts+=("$line")
        sleep 3
    done
    kv "${mode}_cold_start_ms" "$(printf '%s ' "${starts[@]:-}")"

    # 2. The suite, against a fresh launch.
    sh_ am force-stop "$package" >/dev/null
    sleep 1
    adb logcat -c 2>/dev/null
    sh_ am start -n "$component" >/dev/null
    # Not an array: Device Farm's host runs bash 4.2, where "${empty[@]}"
    # under `set -u` is an unbound variable — which is exactly how the first
    # real run (2026-10-15) died here on all three phones, before any suite.
    gpu_flag=""; [ "$mode" = gpu ] && gpu_flag="--gpu"
    # shellcheck disable=SC2086 # $gpu_flag is empty or one word, on purpose
    if VIEWW_SCREENSHOT="$out/$mode-screen.png" \
        bash "$root/ci/mobile/device-suite.sh" --no-build $gpu_flag > "$out/$mode-suite.log" 2>&1; then
        kv "${mode}_suite" PASS
    else
        kv "${mode}_suite" FAIL
        status=1
    fi
    for key in frame_time frame_stages gpu_frames; do
        value="$(grep -m1 -o "$key: .*" "$out/$mode-suite.log" | sed "s/^$key: //")"
        [ -n "$value" ] && kv "${mode}_${key}" "$value"
    done
    kv "${mode}_failed_checks" "$(grep -o '__VIEWW_SUITE__ FAIL [a-z_0-9]*\|  FAIL [a-z_0-9]*' "$out/$mode-suite.log" | awk '{print $NF}' | sort -u | tr '\n' ' ')"

    # 3. Memory while the app is still up, then its log.
    sh_ dumpsys meminfo "$package" > "$out/$mode-meminfo.txt"
    kv "${mode}_total_pss_kb" "$(grep -m1 -E 'TOTAL PSS:|^ *TOTAL ' "$out/$mode-meminfo.txt" | grep -oE '[0-9]+' | head -1)"
    kv "${mode}_total_rss_kb" "$(grep -m1 'TOTAL RSS:' "$out/$mode-meminfo.txt" | sed 's/.*TOTAL RSS: *\([0-9]*\).*/\1/')"
    pid="$(sh_ pidof "$package" | awk '{print $1}')"
    if [ -n "$pid" ]; then
        adb logcat -d --pid="$pid" > "$out/$mode-logcat.txt" 2>/dev/null
    else
        adb logcat -d > "$out/$mode-logcat.txt" 2>/dev/null
    fi
done

sh_ setprop debug.vieww.gpu 0 >/dev/null
sh_ am force-stop "$package" >/dev/null
sh_ svc power stayon false >/dev/null
kv status "$([ "$status" = 0 ] && echo PASS || echo FAIL)"
cat "$result"
exit "$status"
