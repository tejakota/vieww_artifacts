#!/usr/bin/env bash
#
# Give a cargo-apk APK the Java code Firebase Test Lab insists on.
#
#   ci/mobile/apk-add-code.sh IN.apk OUT.apk
#
# # Why
#
# vieww's Android app is all Rust behind `android.app.NativeActivity`, so
# cargo-apk builds it with `android:hasCode="false"` and no `classes.dex`. That
# installs and runs on every phone, but Test Lab refuses the upload outright:
# "Upload invalid: One of your APKs that should contain code has none."
#
# This adds one empty class as `classes.dex`, flips `hasCode` to true in the
# compiled manifest (Android refuses an APK that has dex but says it has
# none, and the reverse), then re-aligns and re-signs. The app still runs
# nothing but Rust: no Java code ever calls the class.
#
# Done after the build rather than in Cargo.toml: `has_code = true` there would
# make every plain `cargo apk run` produce an APK that PackageManager rejects
# ("code is missing"), since cargo-apk has no way to add a dex itself.
#
# Needs: ANDROID_HOME with a build-tools that has d8, zipalign and apksigner;
# javac; python3. Signs with CARGO_APK_RELEASE_KEYSTORE (android-env.sh sets
# it) or ~/.android/debug.keystore. Safe to run twice on the same APK.

set -euo pipefail

in="${1:?usage: apk-add-code.sh IN.apk OUT.apk}"
out="${2:?usage: apk-add-code.sh IN.apk OUT.apk}"
here="$(cd "$(dirname "$0")" && pwd)"

: "${ANDROID_HOME:=$HOME/Android/Sdk}"
bt=""
while IFS= read -r d; do
    if [ -x "$d/d8" ] && [ -x "$d/zipalign" ] && [ -x "$d/apksigner" ]; then bt="$d"; break; fi
done < <(find "$ANDROID_HOME/build-tools" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort -V -r)
[ -n "$bt" ] || { echo "apk-add-code: no build-tools with d8, zipalign and apksigner under $ANDROID_HOME/build-tools" >&2; exit 6; }

ks="${CARGO_APK_RELEASE_KEYSTORE:-$HOME/.android/debug.keystore}"
ks_pass="${CARGO_APK_RELEASE_KEYSTORE_PASSWORD:-android}"
[ -f "$ks" ] || { echo "apk-add-code: no keystore at $ks (source android-env.sh first)" >&2; exit 6; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# One empty class. Java 8 bytecode is what every d8 accepts without an
# android.jar; minApi matches the manifest's floor.
mkdir -p "$work/src/dev/vieww/demo" "$work/classes" "$work/dex"
cat > "$work/src/dev/vieww/demo/TestLabStub.java" <<'JAVA'
package dev.vieww.demo;

/** Never called: Firebase Test Lab only accepts an APK that contains code. */
final class TestLabStub {
    private TestLabStub() {}
}
JAVA
javac --release 8 -nowarn -d "$work/classes" "$work/src/dev/vieww/demo/TestLabStub.java"
"$bt/d8" --release --min-api 24 --output "$work/dex" "$work/classes/dev/vieww/demo/TestLabStub.class"

python3 -I "$here/apk_add_code.py" "$in" "$work/dex/classes.dex" "$work/unaligned.apk"
"$bt/zipalign" -p -f 4 "$work/unaligned.apk" "$work/aligned.apk"
"$bt/apksigner" sign --ks "$ks" --ks-pass "pass:$ks_pass" --out "$out" "$work/aligned.apk"
"$bt/apksigner" verify "$out"
rm -f "$out.idsig"
echo "apk-add-code: $out has classes.dex and hasCode=true (build-tools $(basename "$bt"))"
