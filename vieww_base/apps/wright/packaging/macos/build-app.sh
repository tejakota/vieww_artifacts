#!/usr/bin/env bash
# Wright for macOS: a universal (Apple silicon + Intel) Wright.app in a .dmg.
#
#   packaging/macos/build-app.sh <version> <arm64 binary> <x86_64 binary> <out dir>
#
# The app runs `wright` with no arguments, which opens the window; the
# same binary inside the bundle is the CLI (Contents/MacOS/wright).
# Signed ad hoc (Apple silicon refuses unsigned code); not notarized, so
# the first launch needs right-click > Open — the release notes say so.
set -euo pipefail

version="$1"
arm="$2"
intel="$3"
out="$4"
here="$(cd "$(dirname "$0")" && pwd)"
work="$(mktemp -d)"
app="$work/Wright.app"

mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$out"
lipo -create -output "$app/Contents/MacOS/wright" "$arm" "$intel"
chmod +x "$app/Contents/MacOS/wright"

# Icon: rendered by the app itself (the same painter as the in-app mark),
# with the margin macOS icons keep, then every size iconutil wants.
"$app/Contents/MacOS/wright" ui --icon "$work/icon-1024.png" --size 1024 --padded
iconset="$work/wright.iconset"
mkdir -p "$iconset"
for s in 16 32 128 256 512; do
  sips -z "$s" "$s" "$work/icon-1024.png" --out "$iconset/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2))
  sips -z "$d" "$d" "$work/icon-1024.png" --out "$iconset/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/wright.icns"

sed "s/@VERSION@/$version/g" "$here/Info.plist" > "$app/Contents/Info.plist"
codesign --force --deep --sign - "$app"

# The disk image: the app and a shortcut to Applications, side by side.
stage="$work/dmg"
mkdir -p "$stage"
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
hdiutil create -volname "Wright $version" -srcfolder "$stage" -ov -format UDZO "$out/Wright-$version-macos-universal.dmg"
(cd "$work" && zip -qry "$out/Wright-$version-macos-universal.app.zip" Wright.app)
echo "built $out/Wright-$version-macos-universal.dmg"
