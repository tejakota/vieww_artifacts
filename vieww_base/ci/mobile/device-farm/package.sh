#!/usr/bin/env bash
#
# Build the Device Farm test package: an Appium-Python-shaped zip (the upload
# type a custom test spec runs from) carrying the scripts the spec runs.
#
#   ci/mobile/device-farm/package.sh OUT.zip
#
# Device Farm validates the shape before it accepts the upload: requirements.txt
# at the top, a wheelhouse/ with one pure-Python wheel per requirement line, and
# a tests/ directory pytest can collect. None of it runs — the spec calls
# run-android.sh or run-ios.sh directly — but an upload that fails validation never reaches
# a device.

set -euo pipefail
out="${1:?usage: package.sh OUT.zip}"
root="$(cd "$(dirname "$0")/../../.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/tests" "$work/wheelhouse" "$work/ci/mobile/device-farm"
cp "$root/ci/mobile/device-suite.sh" "$root/ci/mobile/android-env.sh" "$work/ci/mobile/"
cp "$root/ci/mobile/device-farm/run-android.sh" "$root/ci/mobile/device-farm/run-ios.sh" \
    "$root/ci/mobile/device-farm/tap_ios.py" "$root/ci/mobile/device-farm/known-failures.txt" \
    "$work/ci/mobile/device-farm/"

cat > "$work/tests/test_vieww_package.py" <<'PY'
"""Device Farm requires a collectable pytest file in an Appium Python
package. The vieww run does not use it: the custom test spec runs
ci/mobile/device-farm/run-android.sh directly."""


def test_package_is_the_vieww_device_suite():
    assert True
PY

# One pinned pytest and exactly the pure-Python wheels it pulls in. The
# validator wants one wheel per requirements line, so the file is written
# from the wheels that were actually downloaded.
python3 -m pip download --quiet --only-binary=:all: --dest "$work/wheelhouse" "pytest==8.3.3"
: > "$work/requirements.txt"
for wheel in "$work"/wheelhouse/*.whl; do
    name="$(basename "$wheel")"
    case "$name" in
        *-any.whl|*-linux_x86_64.whl) ;;
        *) echo "package.sh: $name is not a pure-Python wheel Device Farm accepts" >&2; exit 1 ;;
    esac
    dist="${name%%-*}"; rest="${name#*-}"; version="${rest%%-*}"
    echo "${dist//_/-}==$version" >> "$work/requirements.txt"
done

mkdir -p "$(dirname "$out")"
out="$(cd "$(dirname "$out")" && pwd)/$(basename "$out")"
rm -f "$out"
(cd "$work" && zip -qr "$out" .)
echo "package.sh: $(unzip -l "$out" | tail -1 | awk '{print $2}') files -> $out"
cat "$work/requirements.txt"
