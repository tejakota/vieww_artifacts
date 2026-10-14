#!/usr/bin/env bash
#
# Run vieww's device suite on real phones in Firebase Test Lab, and bring the
# results back as one summary.
#
#   ci/mobile/test-lab.sh android                 # build, game loop (CPU + GPU) on 3 phones, Robo on 1
#   ci/mobile/test-lab.sh android --devices "oriole:33 a51:31"   # pick the phones yourself
#   ci/mobile/test-lab.sh android --apk path.apk  # skip the build
#   ci/mobile/test-lab.sh android --no-robo       # game loop only
#   ci/mobile/test-lab.sh ios --ipa vieww.ipa     # iOS game loop on 2 iPhones
#   ci/mobile/test-lab.sh list android|ios        # what Test Lab has right now
#
# Writes test-lab-results-<time>/ (and a .tar.gz of it) with summary.txt — one
# row per device and scenario: the suite verdict, the median/worst frame, and
# how many frames the GPU drew. Attach the tarball or paste summary.txt.
#
# # What it runs
#
# **Game Loop** (https://firebase.google.com/docs/test-lab/android/game-loop):
# the demo recognises Test Lab's launch (`examples/android.rs`, `GameLoop`),
# runs the same in-app device suite as `device-suite.sh`, writes the report to
# the results file Test Lab collects, and finishes. Scenario 1 is the CPU
# rasterizer, scenario 2 the planned GPU path — so every phone answers "is the
# GPU path correct and faster on *this* GPU" (Adreno, Mali, Tensor, ...) in one
# upload. On iOS the report goes to Documents/GameLoopResults/ (see
# examples/ios.rs) and each loop ends at Test Lab's timeout.
#
# **Robo** (Android only): Test Lab crawls the app by itself through its
# accessibility tree — vieww's AccessKit bridge, exercised on a phone nobody
# scripted — and reports crashes and what it could reach.
#
# # What it needs
#
# - The Google Cloud SDK (`gcloud`), signed in (`gcloud auth login`, or a
#   service account in CI) and pointed at a Firebase project
#   (`gcloud config set project <id>`, or FTL_PROJECT=<id>).
# - Android: what `apk.sh` needs (SDK, NDK, cargo-apk); the APK is signed with
#   the debug keystore android-env.sh creates, which Test Lab accepts.
# - iOS: a .ipa built on macOS by `ios-app.sh --ipa` **with VIEWW_IOS_IDENTITY
#   and VIEWW_IOS_PROFILE set** (Test Lab refuses an ad-hoc signature and
#   re-signs a real one) and VIEWW_MOLTENVK pointing at MoltenVK's *device*
#   slice (ios-arm64). The `device-cloud` workflow does that on a macOS runner.
#
# # Cost
#
# Firebase's no-cost plan allows 5 physical-device test runs a day (each device
# in a run counts). The default here is 3 phones for the game loop (both
# scenarios run in one session per phone) plus Robo on 1 = 4. Paid (Blaze) is
# $5 per physical-device hour, per minute, after 30 free minutes a day.
# Firebase Test Lab is deprecated and shuts down on 30 September 2027.

set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

platform="${1:-}"
[ -n "$platform" ] && shift || true
devices=""
apk=""
ipa=""
robo=1
count_android=3
count_ios=2
timeout="15m"

while [ "$#" -gt 0 ]; do
    case "$1" in
        --devices) devices="$2"; shift ;;
        --apk) apk="$2"; shift ;;
        --ipa) ipa="$2"; shift ;;
        --no-robo) robo=0 ;;
        --count) count_android="$2"; count_ios="$2"; shift ;;
        --timeout) timeout="$2"; shift ;;
        -h|--help) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) [ "$platform" = list ] && break; echo "test-lab: unknown argument: $1" >&2; exit 2 ;;
    esac
    shift
done

command -v gcloud >/dev/null 2>&1 || {
    echo "test-lab: gcloud not found — install the Google Cloud SDK first." >&2
    exit 127
}
project_flag=()
[ -n "${FTL_PROJECT:-}" ] && project_flag=(--project "$FTL_PROJECT")

# ------------------------------------------------------------------ devices

# Physical models that can run an arm64 APK (Android) or any (iOS), one per
# manufacturer so the GPUs differ, each at its newest supported OS. Picked from
# the live catalogue rather than hard-coded, because Test Lab retires models.
pick_devices() {
    local which="$1" count="$2"
    gcloud firebase test "$which" models list "${project_flag[@]}" --format=json 2>/dev/null |
        python3 -c '
import json, sys
which, count = sys.argv[1], int(sys.argv[2])
models = json.load(sys.stdin)
def newest(m):
    vs = m.get("supportedVersionIds") or []
    def key(v):
        try: return tuple(int(p) for p in str(v).split("."))
        except ValueError: return (0,)
    return max(vs, key=key) if vs else None
picked, seen = [], set()
for m in models:
    if which == "android":
        if m.get("form") != "PHYSICAL": continue
        if "arm64-v8a" not in (m.get("supportedAbis") or []): continue
        if any(str(t).startswith("deprecated") for t in m.get("tags") or []): continue
        maker = (m.get("manufacturer") or m.get("brand") or "").lower()
    else:
        maker = m.get("name", "").split()[0].lower() + str(len(picked))
    v = newest(m)
    if not v or maker in seen: continue
    seen.add(maker)
    picked.append("%s:%s" % (m["id"], v))
    if len(picked) >= count: break
print(" ".join(picked))
' "$which" "$count"
}

device_flags() {
    local spec model version
    for spec in $1; do
        model="${spec%%:*}"; version="${spec#*:}"
        printf -- '--device\nmodel=%s,version=%s,locale=en,orientation=portrait\n' "$model" "$version"
    done
}

if [ "$platform" = list ]; then
    which="${1:-android}"
    gcloud firebase test "$which" models list "${project_flag[@]}"
    exit 0
fi
case "$platform" in
    android|ios) ;;
    *) echo "usage: ci/mobile/test-lab.sh android|ios|list [options]   (--help)" >&2; exit 2 ;;
esac

stamp="$(date +%Y%m%d-%H%M%S)"
out="$root/test-lab-results-$stamp"
mkdir -p "$out"
summary="$out/summary.txt"
{
    echo "vieww on Firebase Test Lab — $platform — $stamp"
    echo "commit: $(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
    echo
} > "$summary"

if [ -z "$devices" ]; then
    n="$count_android"; [ "$platform" = ios ] && n="$count_ios"
    devices="$(pick_devices "$platform" "$n")"
    [ -n "$devices" ] || { echo "test-lab: no devices from the catalogue — is the project set and Test Lab enabled?" >&2; exit 3; }
fi
echo "devices: $devices" | tee -a "$summary"

# -------------------------------------------------------------------- build

if [ "$platform" = android ] && [ -z "$apk" ]; then
    # Release (the frame numbers mean nothing in debug), arm64 (every physical
    # Test Lab phone), not launched (--detach would need a phone on adb).
    . ci/mobile/android-env.sh
    android_env "test-lab"
    rustup target add aarch64-linux-android >/dev/null 2>&1 || true
    marker="$(mktemp)"
    cargo apk build -p vieww-platform-winit --example android --release --target aarch64-linux-android
    apk="$(find "$root/target" -path '*/apk/*' -name '*.apk' -newer "$marker" -print | head -1)"
    rm -f "$marker"
    [ -n "$apk" ] || { echo "test-lab: the build produced no APK" >&2; exit 4; }
fi
if [ "$platform" = ios ] && [ -z "$ipa" ]; then
    if [ "$(uname)" != Darwin ]; then
        echo "test-lab: an iOS run needs a signed .ipa (--ipa). Building one needs macOS;" >&2
        echo "  the device-cloud workflow builds it on a macOS runner." >&2
        exit 4
    fi
    ci/mobile/ios-app.sh --ipa
    ipa="target/ios/vieww.ipa"
fi

# ---------------------------------------------------------------------- run

# Run one Test Lab matrix; remember where its results landed.
#   run_matrix NAME gcloud-args...
run_matrix() {
    local name="$1"; shift
    local log="$out/$name.gcloud.log"
    echo
    echo "==> $name"
    local status=0
    gcloud firebase test "$platform" run "${project_flag[@]}" "$@" \
        --timeout="$timeout" --format=json > "$out/$name.json" 2> >(tee "$log" >&2) || status=$?
    # "Raw results will be stored in your GCS bucket at
    #  [https://console.developers.google.com/storage/browser/<bucket>/<dir>/]"
    local where
    where="$(grep -o 'storage/browser/[^]]*' "$log" | head -1 | sed 's|storage/browser/||; s|/$||')"
    if [ -n "$where" ]; then
        echo "$name $where" >> "$out/locations.txt"
        mkdir -p "$out/$name"
        gcloud storage cp -r "gs://$where/*" "$out/$name/" > /dev/null 2>&1 \
            || echo "test-lab: could not download gs://$where" >&2
    fi
    echo "$name: gcloud exit $status" >> "$summary"
    return 0
}

mapfile -t dflags < <(device_flags "$devices")

if [ "$platform" = android ]; then
    run_matrix game-loop --type=game-loop --app="$apk" --scenario-numbers=1,2 "${dflags[@]}"
    if [ "$robo" = 1 ]; then
        first="$(echo "$devices" | awk '{print $1}')"
        mapfile -t rflags < <(device_flags "$first")
        run_matrix robo --type=robo --app="$apk" "${rflags[@]}"
    fi
else
    run_matrix game-loop --type=game-loop --app="$ipa" --scenario-numbers=1 "${dflags[@]}"
fi

# ------------------------------------------------------------------ summary

echo >> "$summary"
echo "per device and scenario (from the suite's own report):" >> "$summary"
python3 - "$out" >> "$summary" <<'PY'
import os, re, sys
root = sys.argv[1]
rows = []
for dirpath, _, files in os.walk(root):
    for f in files:
        path = os.path.join(dirpath, f)
        try:
            text = open(path, errors="replace").read()
        except OSError:
            continue
        if "__VIEWW_SUITE__ result" not in text:
            continue
        rel = os.path.relpath(path, root)
        # Results files are per scenario; a logcat holds every scenario's
        # report, so split it on "begin".
        for block in text.split("__VIEWW_SUITE__ begin")[1:]:
            def grab(pattern):
                m = re.search(pattern, block)
                return m.group(1).strip() if m else "?"
            rows.append((
                rel.split(os.sep)[1] if os.sep in rel else rel,
                grab(r"rasterizer: ([^\n(]*)"),
                grab(r"result (PASS|FAIL)"),
                grab(r"frame_time: (median [0-9.]+ms, worst [0-9.]+ms)"),
                grab(r"gpu_frames: ([0-9]+ of [0-9]+)"),
                grab(r"end ([0-9]+ checks, [0-9]+ failed)"),
                rel,
            ))
seen = set()
for row in sorted(rows):
    key = row[:6]
    if key in seen:
        continue
    seen.add(key)
    print("  %-28s %-34s %-5s %-34s gpu %-10s %s" % row[:6])
if not rows:
    print("  (no suite report found in the downloaded results — see the *.gcloud.log files)")
PY

if [ -d "$out/robo" ]; then
    echo >> "$summary"
    echo "robo: see robo.gcloud.log for the outcome table; crawl artifacts in robo/" >> "$summary"
fi

echo
cat "$summary"
tar -czf "$out.tar.gz" -C "$(dirname "$out")" "$(basename "$out")"
echo
echo "Everything is in $out.tar.gz"
