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
# `cpuvk` is a diagnostic third pass: the CPU rasterizer presented through the
# Vulkan swapchain (`debug.vieww.present=vulkan`) instead of ANativeWindow_lock,
# so the two present paths are compared on the same phone in the same run. Its
# verdict is recorded but does not fail the job.
#
# `auto` is `App::auto_rasterizer` (`debug.vieww.gpu=auto`): the app measures
# the rasterizers itself and keeps the faster. Its three cold starts double as
# the calibration — the first forgets any earlier answer
# (`debug.vieww.recalibrate=1`) and measures the CPU path, the second measures
# the GPU path if the CPU was not comfortable, the third uses the answer — and
# the suite then runs on whatever was chosen (`renderer_choice`). Diagnostic
# for this round: it reports, it does not fail the job.
#
# `cpuhint` is the CPU path with an Android performance-hint session over the
# UI thread (`debug.vieww.hint=1`, Android 13+; the LG Stylo 5 reports it
# unavailable): whether the window-present path's slower raster is the
# scheduler's choice of core and clock. Diagnostic only.
#
# `gpuscale` is the GPU path at the probed device tier's resolution
# (`debug.vieww.scale=auto`, `VIEWW_GPU_SCALE`): the plan renders into targets
# a fraction of the swapchain's size and the final blit enlarges. The test of
# whether a fill-rate cut is what lets the GPU path pass where it currently
# loses (LG Stylo 5: CPU 36.2 ms, GPU 40.1 ms, 2026-10-17) — the `device`
# observation says which tier was resolved, `gpu_scale` which fraction ran.
# Diagnostic only.
#
# `cpubare` is the CPU path with the 2026-10-18/19 raster additions off
# (`debug.vieww.bare=1`: no row-parallel walk, no path-fill cache, no blur
# cache) — the same-unit A/B against `cpu` that says whether those commits
# moved frame time or PSS, which run f91d07c could only compare across
# units. Diagnostic only.
for mode in cpu gpu auto cpuvk cpuhint gpuscale cpubare; do
    # `gpuscale` is a GPU pass: the property must say so *before* the launch
    # below. Until 2026-10-19 it was left at 0 here and only device-suite.sh's
    # `--gpu` set it — after the app was already up, which `--no-build` does
    # not relaunch — so the Pixel 5a and S26 Ultra rows of run f91d07c drew
    # all 600 frames on the CPU under a "GPU at 0.75x" label (the LG Stylo 5
    # happened to read the property late enough to get the GPU).
    gpu=0; { [ "$mode" = gpu ] || [ "$mode" = gpuscale ]; } && gpu=1; [ "$mode" = auto ] && gpu=auto
    present=""; [ "$mode" = cpuvk ] && present="vulkan"
    hint=0; [ "$mode" = cpuhint ] && hint=1
    scale=""; [ "$mode" = gpuscale ] && scale=auto
    bare=0; [ "$mode" = cpubare ] && bare=1
    sh_ setprop debug.vieww.bare "$bare" >/dev/null
    sh_ setprop debug.vieww.hint "$hint" >/dev/null
    sh_ setprop debug.vieww.gpu "$gpu" >/dev/null
    sh_ setprop debug.vieww.present "\"$present\"" >/dev/null
    sh_ setprop debug.vieww.scale "$scale" >/dev/null

    # 1. Cold start, three times. TotalTime is the platform's own measure of
    # launch to first frame drawn; WaitTime adds the system's share.
    starts=()
    for i in 1 2 3; do
        sh_ am force-stop "$package" >/dev/null
        recalibrate=0; [ "$mode" = auto ] && [ "$i" = 1 ] && recalibrate=1
        sh_ setprop debug.vieww.recalibrate "$recalibrate" >/dev/null
        sleep 2
        [ "$mode" = auto ] && adb logcat -c 2>/dev/null
        line="$(sh_ am start -W -n "$component" | tee -a "$out/$mode-am-start.txt" | grep -m1 '^TotalTime' | awk '{print $2}')"
        [ -n "$line" ] && starts+=("$line")
        # Long enough for `auto` to measure 60 frames on a 30 fps phone.
        if [ "$mode" = auto ]; then sleep 6; else sleep 3; fi
        if [ "$mode" = auto ]; then
            echo "launch $i:" >> "$out/auto-calibration.txt"
            adb logcat -d 2>/dev/null | tr -d '\r' | grep 'vieww: rasterizer' >> "$out/auto-calibration.txt"
        fi
    done
    sh_ setprop debug.vieww.recalibrate 0 >/dev/null
    [ "$mode" = auto ] && kv auto_calibration "$(tr '\n' ' ' < "$out/auto-calibration.txt" 2>/dev/null)"
    kv "${mode}_cold_start_ms" "$(printf '%s ' "${starts[@]:-}")"

    # 2. The suite, against a fresh launch.
    sh_ am force-stop "$package" >/dev/null
    sleep 1
    adb logcat -c 2>/dev/null
    sh_ am start -n "$component" >/dev/null
    # Not an array: Device Farm's host runs bash 4.2, where "${empty[@]}"
    # under `set -u` is an unbound variable — which is exactly how the first
    # real run (2026-10-15) died here on all three phones, before any suite.
    gpu_flag=""; { [ "$mode" = gpu ] || [ "$mode" = gpuscale ]; } && gpu_flag="--gpu"
    # shellcheck disable=SC2086 # $gpu_flag is empty or one word, on purpose
    if VIEWW_SCREENSHOT="$out/$mode-screen.png" \
        bash "$root/ci/mobile/device-suite.sh" --no-build $gpu_flag > "$out/$mode-suite.log" 2>&1; then
        kv "${mode}_suite" PASS
    else
        kv "${mode}_suite" FAIL
        case "$mode" in cpuvk|cpuhint|gpuscale|cpubare|auto) ;; *) status=1 ;; esac
    fi
    for key in frame_time frame_stages frame_stages_p95 worst_frame startup_stages renderer_choice raster_by_core cpu_clocks perf_hint pipeline_cache gpu_frames device gpu_scale cache_budgets quality_overlay; do
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
sh_ setprop debug.vieww.present '""' >/dev/null
sh_ setprop debug.vieww.recalibrate 0 >/dev/null
sh_ setprop debug.vieww.hint 0 >/dev/null
sh_ setprop debug.vieww.scale "" >/dev/null
sh_ setprop debug.vieww.bare 0 >/dev/null
sh_ am force-stop "$package" >/dev/null
sh_ svc power stayon false >/dev/null
kv status "$([ "$status" = 0 ] && echo PASS || echo FAIL)"
cat "$result"
exit "$status"
