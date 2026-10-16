#!/usr/bin/env bash
#
# Runs ON the AWS Device Farm iOS host (macOS, Xcode), once per iPhone, from
# the test spec (`testspec-ios.yml`). The iOS twin of `run-android.sh`; never
# run by hand — `ci/mobile/device_farm.py --platform ios` packages and
# schedules it.
#
# For each rasterizer path (CPU, then the planned GPU path via VIEWW_GPU=1):
#   1. start-up x3: launch with VIEWW_TRACE_FRAMES=1 and read the app's own
#      trace — the timestamp of the first presented frame, from `main`
#   2. the on-device suite: launch with the console attached, wait for the
#      app to say where its button and field are (`__VIEWW_TAPS_PT__`, in
#      points), tap them through Appium's XCUITest driver (`tap_ios.py`), and
#      wait for the suite's verdict (`__VIEWW_SUITE__ result`)
#   3. the suite report the app also writes into its own Documents folder,
#      copied off the device as a second channel
#
# Everything lands in $DEVICEFARM_LOG_DIR/vieww/; `result.txt` there is what
# device_farm.py reads, with the same keys run-android.sh writes.
#
# Written for the bash 3.2 macOS ships: no arrays under `set -u`, no mapfile.

set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
out="${DEVICEFARM_LOG_DIR:-/tmp}/vieww"
mkdir -p "$out"
result="$out/result.txt"
: > "$result"
udid="${DEVICEFARM_DEVICE_UDID:?DEVICEFARM_DEVICE_UDID is not set}"
app="${DEVICEFARM_APP_PATH:-}"

kv() { echo "$1=$2" >> "$result"; }
log() { echo "run-ios: $*"; }

kv device_name "${DEVICEFARM_DEVICE_NAME:-unknown}"
kv ios "${DEVICEFARM_DEVICE_OS_VERSION:-unknown}"
xcrun devicectl device info details --device "$udid" > "$out/device-info.txt" 2>&1 || true
kv model "$(awk -F': ' '/marketingName|productType/ {print $2; exit}' "$out/device-info.txt")"
xcodebuild -version > "$out/xcode.txt" 2>&1 || true

# The bundle id, from the uploaded .ipa itself rather than a constant.
bundle_id="dev.vieww.demo"
if [ -n "$app" ] && [ -f "$app" ]; then
    kv ipa_bytes "$(stat -f %z "$app" 2>/dev/null || stat -c %s "$app")"
    plist="$(mktemp)"
    if unzip -p "$app" 'Payload/*.app/Info.plist' > "$plist" 2>/dev/null; then
        id="$(plutil -extract CFBundleIdentifier raw -o - "$plist" 2>/dev/null)"
        [ -n "$id" ] && bundle_id="$id"
    fi
fi
kv bundle_id "$bundle_id"

# **Below iOS 17, devicectl cannot see the phone** (CoreDevice is iOS 17+):
# Device Farm's iPhone 13 at iOS 16.0.2 answered "the specified device was not
# found" to every devicectl call (2026-10-16). Those phones run the suite
# through Appium alone — `tap_ios.py suite` — further down.
ios_major="${DEVICEFARM_DEVICE_OS_VERSION%%.*}"
legacy=0
case "$ios_major" in ''|*[!0-9]*) ;; *) [ "$ios_major" -lt 17 ] && legacy=1 ;; esac
kv route "$([ "$legacy" = 1 ] && echo appium-only || echo devicectl)"

# Device Farm installs the app before the test phase; make sure, without
# failing when it already is.
if [ "$legacy" = 0 ] && [ -n "$app" ] && [ -f "$app" ]; then
    if ! xcrun devicectl device info apps --device "$udid" 2>/dev/null | grep -q "$bundle_id"; then
        log "installing $app"
        xcrun devicectl device install app --device "$udid" "$app" > "$out/install.log" 2>&1 || true
    fi
fi

# ── Appium, for the taps ─────────────────────────────────────────────────────
# Taps go through XCUITest (Apple's own UI automation), driven by Appium with
# Device Farm's prebuilt WebDriverAgent. The session is opened once, on the
# home screen, before the app is launched, because WebDriverAgent takes tens
# of seconds to start and the suite should not wait on it.
wda="${DEVICEFARM_APPIUM_WDA_DERIVED_DATA_PATH_V9:-${DEVICEFARM_APPIUM_WDA_DERIVED_DATA_PATH_V8:-${DEVICEFARM_WDA_DERIVED_DATA_PATH_V9:-}}}"
appium_pid=""
session=""
if command -v appium > /dev/null 2>&1; then
    appium --port 4723 --log-timestamp --log "$out/appium.log" > /dev/null 2>&1 &
    appium_pid=$!
    for _ in $(seq 1 30); do
        curl -fs http://127.0.0.1:4723/status > /dev/null 2>&1 && break
        sleep 1
    done
fi

if [ "$legacy" = 1 ]; then
    status=0
    for mode in cpu gpu; do
        gpu=0; [ "$mode" = gpu ] && gpu=1
        log="$out/$mode-suite.log"
        verdict="$(python3 "$here/tap_ios.py" suite --udid "$udid" --wda "$wda" \
            --bundle "$bundle_id" --gpu "$gpu" --out "$log" 2>> "$out/$mode-appium.err" | tr -d '\r' | tail -1)"
        [ -n "$verdict" ] || verdict="ERROR"
        [ "$verdict" = PASS ] || status=1
        kv "${mode}_suite" "$verdict"
        kv "${mode}_first_frame_ms" "n/a (no console below iOS 17)"
        for key in frame_time frame_stages gpu_frames; do
            value="$(grep -m1 -o "$key: .*" "$log" | sed "s/^$key: //" | tr -d '\r')"
            [ -n "$value" ] && kv "${mode}_${key}" "$value"
        done
        kv "${mode}_failed_checks" "$(grep -o '__VIEWW_SUITE__ FAIL [a-z_0-9]*' "$log" | awk '{print $NF}' | sort -u | tr '\n' ' ')"
        sleep 2
    done
    [ -n "$appium_pid" ] && kill "$appium_pid" > /dev/null 2>&1
    kv status "$([ "$status" = 0 ] && echo PASS || echo FAIL)"
    cat "$result"
    exit "$status"
fi

if [ -n "$appium_pid" ]; then
    session="$(python3 "$here/tap_ios.py" open --udid "$udid" --wda "$wda" 2> "$out/appium-session.err")"
    [ -n "$session" ] && log "appium session $session" || log "no appium session (see appium-session.err); taps will not be sent"
else
    log "appium is not on PATH; taps will not be sent"
fi
kv appium_session "${session:-none}"

# Launch with the console attached, in the background, writing to $2.
# Echoes the launcher's pid. $1 is the environment JSON.
launch() {
    xcrun devicectl device process launch --device "$udid" --terminate-existing \
        --console --environment-variables "$1" "$bundle_id" > "$2" 2>&1 &
    echo $!
}

# Wait up to $2 seconds for a line matching $3 in file $1.
wait_for() {
    deadline=$(( $(date +%s) + $2 ))
    while [ "$(date +%s)" -lt "$deadline" ]; do
        grep -q "$3" "$1" 2> /dev/null && return 0
        sleep 0.5
    done
    return 1
}

status=0
for mode in cpu gpu; do
    gpu=0; [ "$mode" = gpu ] && gpu=1

    # 1. Start-up, three times. The trace's first `frame stages` line is the
    # first presented frame, timestamped from the app's own epoch.
    starts=""
    for i in 1 2 3; do
        f="$out/$mode-start-$i.txt"
        pid="$(launch "{\"VIEWW_GPU\":\"$gpu\",\"VIEWW_TRACE_FRAMES\":\"1\"}" "$f")"
        wait_for "$f" 20 "frame stages" || true
        kill "$pid" > /dev/null 2>&1 || true
        t="$(grep -m1 "frame stages" "$f" | awk '{print $1}')"
        [ -n "$t" ] && starts="$starts $(python3 -c "print(round(float('$t')*1000, 1))")"
        sleep 2
    done
    kv "${mode}_first_frame_ms" "${starts# }"

    # 2. The suite.
    log="$out/$mode-suite.log"
    pid="$(launch "{\"VIEWW_GPU\":\"$gpu\"}" "$log")"
    if wait_for "$log" 60 "__VIEWW_TAPS_PT__"; then
        line="$(grep -m1 "__VIEWW_TAPS_PT__" "$log")"
        kv "${mode}_tap_targets" "$(printf '%s' "${line#*__VIEWW_TAPS_PT__ }" | tr -d '\r')"
        if [ -n "$session" ]; then
            # Several times, blind, as device-suite.sh does: the app latches
            # the first tap it feels and reports once, at the end.
            for _ in 1 2 3 4 5 6; do
                grep -q "__VIEWW_SUITE__ result" "$log" && break
                python3 "$here/tap_ios.py" tap --session "$session" --line "$line" >> "$out/$mode-taps.log" 2>&1
                sleep 1
            done
        fi
    else
        log "$mode: the app never reported its tap targets"
    fi
    if wait_for "$log" 120 "__VIEWW_SUITE__ result"; then
        # `tr -d '\r'`: the console forwards CRLF, and a verdict of
        # "PASS\r" is not "PASS" — which is how the first iPhone 15 Plus run
        # (2026-10-16) passed both suites and still reported status=FAIL.
        verdict="$(grep -m1 "__VIEWW_SUITE__ result" "$log" | tr -d '\r' | awk '{print $NF}')"
    else
        verdict="TIMEOUT"
    fi
    [ "$verdict" = PASS ] || status=1
    kv "${mode}_suite" "$verdict"
    for key in frame_time frame_stages gpu_frames; do
        value="$(grep -m1 -o "$key: .*" "$log" | sed "s/^$key: //" | tr -d '\r')"
        [ -n "$value" ] && kv "${mode}_${key}" "$value"
    done
    kv "${mode}_failed_checks" "$(grep -o '__VIEWW_SUITE__ FAIL [a-z_0-9]*' "$log" | awk '{print $NF}' | sort -u | tr '\n' ' ')"

    # 3. The report the app wrote into its own sandbox — a second channel, in
    # case the console was not forwarded.
    xcrun devicectl device copy from --device "$udid" --domain-type appDataContainer \
        --domain-identifier "$bundle_id" --source Documents/GameLoopResults/vieww-suite.txt \
        --destination "$out/$mode-suite-report.txt" > /dev/null 2>&1 || true
    kill "$pid" > /dev/null 2>&1 || true
    sleep 2
done

[ -n "$session" ] && python3 "$here/tap_ios.py" close --session "$session" > /dev/null 2>&1
[ -n "$appium_pid" ] && kill "$appium_pid" > /dev/null 2>&1
kv status "$([ "$status" = 0 ] && echo PASS || echo FAIL)"
cat "$result"
exit "$status"
