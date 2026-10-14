#!/usr/bin/env bash
#
# Drive the demo app through Maestro (https://maestro.dev) and keep what it saw.
#
#   ci/mobile/maestro.sh android        # emulator/device from adb, app already installed
#   ci/mobile/maestro.sh ios            # booted simulator, app already installed
#
# Writes to $VIEWW_MAESTRO_OUT (default ../ci-artifacts/maestro-<platform>):
#   report.xml         JUnit, one case per flow
#   hierarchy-*.json   the tree Maestro sees, before and after the flows
#   screenshots, logs  from --test-output-dir
#
# # Why the hierarchy is saved even on success
#
# Maestro reads the accessibility tree. vieww draws its own pixels and publishes
# that tree through AccessKit, which on Android sits in a one-node placeholder
# until an accessibility client asks for more (see the header of
# a11y-android.sh). Whether Maestro's UiAutomation driver counts as such a
# client is exactly what this run measures: a dump with "Tap me" in it says yes;
# a single featureless View says the flows failed for that reason and not for a
# broken widget.
#
# Never builds or installs: ios-app.sh / apk.sh do that.

set -euo pipefail

platform="${1:?usage: maestro.sh android|ios}"
here="$(cd "$(dirname "$0")" && pwd)"
out="${VIEWW_MAESTRO_OUT:-$here/../../../ci-artifacts/maestro-$platform}"
mkdir -p "$out"

if ! command -v maestro >/dev/null 2>&1; then
    curl -fsSL "https://get.maestro.mobile.dev" | bash
    export PATH="$HOME/.maestro/bin:$PATH"
fi
# Maestro wants Java 17+; GitHub's images carry one without selecting it.
if [[ -n "${JAVA_HOME_17_X64:-}" ]]; then export JAVA_HOME="$JAVA_HOME_17_X64"; fi
if [[ -n "${JAVA_HOME_17_arm64:-}" ]]; then export JAVA_HOME="$JAVA_HOME_17_arm64"; fi
maestro --version

device=()
case "$platform" in
    android)
        serial="$(adb devices | awk 'NR>1 && $2=="device" {print $1; exit}')"
        [[ -n "$serial" ]] || { echo "maestro.sh: no adb device" >&2; exit 2; }
        device=(--device "$serial")
        # A slow emulator raises "Pixel Launcher isn't responding" over
        # whatever is on screen, and Maestro then sees the dialog instead of
        # the app (third CI run). Suppress such dialogs, and close one already up.
        adb -s "$serial" shell settings put global hide_error_dialogs 1 >/dev/null 2>&1 || true
        adb -s "$serial" shell am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS >/dev/null 2>&1 || true
        adb -s "$serial" shell am force-stop dev.vieww.demo || true
        # `am start`, not `monkey -c LAUNCHER 1`: monkey's one event is not
        # guaranteed to be the launch alone.
        adb -s "$serial" shell am start -n dev.vieww.demo/android.app.NativeActivity >/dev/null 2>&1 || true
        ;;
    ios)
        udid="${VIEWW_SIM_DEVICE:-$(xcrun simctl list devices booted -j | python3 -c "import json,sys; print(next(x['udid'] for v in json.load(sys.stdin)['devices'].values() for x in v))")}"
        device=(--device "$udid")
        # The app's own stdout/stderr — vieww's log lines and any panic — go
        # nowhere under a plain launch, and the first run showed a black screen
        # with nothing to say why. Relaunch with both captured.
        # SIMCTL_CHILD_* reaches the app as its own environment: the startup
        # trace (window, GPU, accessibility, first tree) says where it stops.
        # The second CI run had a black screen and an empty stderr without it.
        SIMCTL_CHILD_VIEWW_TRACE_FRAMES=1 SIMCTL_CHILD_RUST_BACKTRACE=1 \
        xcrun simctl launch --terminate-running-process \
            --stdout="$out/app-stdout.txt" --stderr="$out/app-stderr.txt" \
            "$udid" dev.vieww.demo || true
        ;;
    *) echo "maestro.sh: unknown platform $platform" >&2; exit 2 ;;
esac

sleep 10
maestro "${device[@]}" hierarchy > "$out/hierarchy-before.json" 2> "$out/hierarchy-before.err" || true
# Is the app still alive? A missing label means nothing until that is known:
# the first iOS run found "Tap me" absent and a screenshot of the home screen.
alive=0
case "$platform" in
    android)
        pid="$(adb -s "$serial" shell pidof dev.vieww.demo 2>/dev/null | tr -d '\r' || true)"
        [[ -n "$pid" ]] && alive=1
        adb -s "$serial" logcat -d -t 2000 > "$out/logcat.txt" 2>&1 || true
        ;;
    ios)
        if xcrun simctl spawn "$udid" launchctl list 2>/dev/null | grep -q 'dev.vieww.demo'; then alive=1; fi
        xcrun simctl spawn "$udid" log show --last 3m --style compact \
            --predicate 'process == "vieww"' > "$out/app-log.txt" 2>&1 || true
        mkdir -p "$out/crashes"
        find "$HOME/Library/Logs/DiagnosticReports" -newer "$here/maestro.sh" \
            \( -name 'vieww*' -o -name '*dev.vieww*' \) -exec cp {} "$out/crashes/" \; 2>/dev/null || true
        xcrun simctl io "$udid" screenshot "$out/screen-before.png" >/dev/null 2>&1 || true
        ;;
esac
if [[ "$alive" = 1 ]]; then
    echo "maestro.sh: the app is running"
else
    echo "::warning::maestro.sh: dev.vieww.demo is NOT running ten seconds after launch — see app-log / logcat / crashes in the artifact"
fi

if grep -q '"Tap me"' "$out/hierarchy-before.json" 2>/dev/null; then
    echo "maestro.sh: the semantics tree is visible to Maestro (found \"Tap me\")"
else
    echo "::warning::maestro.sh: \"Tap me\" is NOT in Maestro's hierarchy (app running: $alive) — the flows will fail on lookup, not on behaviour"
fi

status=0
maestro "${device[@]}" test \
    --format junit --output "$out/report.xml" \
    --test-output-dir "$out/run" \
    "$here/maestro" || status=$?

maestro "${device[@]}" hierarchy > "$out/hierarchy-after.json" 2>/dev/null || true
exit "$status"
