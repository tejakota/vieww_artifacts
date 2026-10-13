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
        adb -s "$serial" shell am force-stop dev.vieww.demo || true
        adb -s "$serial" shell monkey -p dev.vieww.demo -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1 || true
        ;;
    ios)
        udid="${VIEWW_SIM_DEVICE:-$(xcrun simctl list devices booted -j | python3 -c "import json,sys; print(next(x['udid'] for v in json.load(sys.stdin)['devices'].values() for x in v))")}"
        device=(--device "$udid")
        xcrun simctl launch "$udid" dev.vieww.demo || true
        ;;
    *) echo "maestro.sh: unknown platform $platform" >&2; exit 2 ;;
esac

sleep 10
maestro "${device[@]}" hierarchy > "$out/hierarchy-before.json" 2> "$out/hierarchy-before.err" || true
if grep -q '"Tap me"' "$out/hierarchy-before.json" 2>/dev/null; then
    echo "maestro.sh: the semantics tree is visible to Maestro (found \"Tap me\")"
else
    echo "maestro.sh: \"Tap me\" is NOT in Maestro's hierarchy — expect the flows to fail on lookup, not on behaviour" >&2
fi

status=0
maestro "${device[@]}" test \
    --format junit --output "$out/report.xml" \
    --test-output-dir "$out/run" \
    "$here/maestro" || status=$?

maestro "${device[@]}" hierarchy > "$out/hierarchy-after.json" 2>/dev/null || true
exit "$status"
