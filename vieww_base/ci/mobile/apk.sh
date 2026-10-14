#!/usr/bin/env bash
# Build and run an Android example with the toolchain resolved first.
#
#     ci/mobile/apk.sh --example android
#     ci/mobile/apk.sh --example android --release
#     ci/mobile/apk.sh --example android --detach   # build, install, launch, return
#
# `--detach` is the default when `CI` is set. Without it this is `cargo apk
# run`, which launches the app and then **follows logcat until the app exits**
# — and the demo app never exits, so on a CI runner the step simply never
# returned and the job sat there until its timeout. With it: `cargo apk build`,
# `adb install -r`, a launcher intent, done.
#
# With no `--target`, the attached device's ABI picks one (`x86_64` on an
# emulator, `aarch64` on a phone): `build_targets` in the crate's
# `[package.metadata.android]` names only aarch64, and an arm64-only APK does
# not install on an x86_64 emulator image.
#
# # Why this exists
#
# `cargo apk` reads the NDK from the environment, and when nothing tells it
# otherwise it falls back to `$ANDROID_HOME/ndk-bundle`. On this machine that
# is **NDK r22.1**, which predates `libunwind` — so the build runs for a couple
# of minutes and then dies at link time with:
#
#     ld: error: unable to find library -lunwind
#
# That error names a missing C library and looks like a broken checkout or a
# Rust problem. It is neither: it is the wrong NDK, chosen silently by a
# fallback, and `$ANDROID_HOME/ndk/27.3.13750724` was sitting there the whole
# time. It cost two builds before anyone read the toolchain path in the
# command line rather than the error at the end of it.
#
# `ci/mobile/android-env.sh` already knew which NDK to use. The only thing missing was
# something that made *not* asking it impossible, which is all this is: it adds
# to that file rather than beside it, per the rule in `docs/HANDOFF.md`.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=ci/mobile/android-env.sh
. "$root/ci/mobile/android-env.sh"
android_env "apk" || exit 1

cd "$root"

detach=0
[ -n "${CI:-}" ] && detach=1
have_target=0
args=()
for a in "$@"; do
    case "$a" in
        --detach) detach=1 ;;
        --no-detach) detach=0 ;;
        --target|--target=*) have_target=1; args+=("$a") ;;
        *) args+=("$a") ;;
    esac
done

if [ "$have_target" = 0 ] && command -v adb >/dev/null 2>&1; then
    abi="$(adb shell getprop ro.product.cpu.abi 2>/dev/null | tr -d '\r' || true)"
    case "$abi" in
        x86_64) args+=(--target x86_64-linux-android) ;;
        arm64-v8a) args+=(--target aarch64-linux-android) ;;
        "") ;;
        *) echo "apk: device ABI '$abi' — leaving the target to build_targets" >&2 ;;
    esac
fi

if [ "$detach" = 0 ]; then
    exec cargo apk run -p vieww-platform-winit "${args[@]}"
fi

marker="$(mktemp)"
cargo apk build -p vieww-platform-winit "${args[@]}"
apk="$(find "$root/target" -path '*/apk/*' -name '*.apk' -newer "$marker" -print 2>/dev/null | head -1)"
rm -f "$marker"
if [ -z "$apk" ]; then
    echo "apk: cargo apk build succeeded but no new .apk was found under target/" >&2
    exit 5
fi
echo "==> installing $apk"
adb install -r "$apk"
package="$(sed -n '/^\[package.metadata.android\]/,/^\[/ s/^package = "\(.*\)"/\1/p' crates/vieww-platform-winit/Cargo.toml | head -1)"
adb shell am force-stop "$package" >/dev/null 2>&1 || true
adb shell am start -n "$package/android.app.NativeActivity" >/dev/null
echo "==> launched $package (detached; logcat not followed)"
