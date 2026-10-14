#!/usr/bin/env bash
# Package built plugin libraries the way hosts look for them:
#
#   <name>.clap   a file on Linux/Windows; a bundle directory on macOS
#   <name>.vst3   the VST3 bundle: Contents/<arch>-linux/<name>.so,
#                 Contents/MacOS/<name> (+ Info.plist, PkgInfo),
#                 Contents/<arch>-win/<name>.vst3
#
# Usage: tools/bundle.sh <dir with built libraries> <output dir> <name>...
set -euo pipefail
SRC=$1; DST=$2; shift 2
mkdir -p "$DST"

case "$(uname -s)" in
  Linux*)  OS=linux ;;
  Darwin*) OS=macos ;;
  *)       OS=windows ;;
esac
ARCH="$(uname -m)"; [[ "$ARCH" == "arm64" ]] && ARCH=aarch64

plist() { # name kind
  cat <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleDevelopmentRegion</key><string>English</string>
  <key>CFBundleExecutable</key><string>$1</string>
  <key>CFBundleIdentifier</key><string>dev.vieww.$1.$2</string>
  <key>CFBundleName</key><string>$1</string>
  <key>CFBundlePackageType</key><string>BNDL</string>
  <key>CFBundleSignature</key><string>????</string>
  <key>CFBundleVersion</key><string>1.0.0</string>
</dict></plist>
EOF
}

for name in "$@"; do
  case $OS in
    linux)
      lib="$SRC/lib$name.so"
      cp "$lib" "$DST/$name.clap"
      mkdir -p "$DST/$name.vst3/Contents/$ARCH-linux"
      cp "$lib" "$DST/$name.vst3/Contents/$ARCH-linux/$name.so"
      ;;
    macos)
      lib="$SRC/lib$name.dylib"
      for kind in clap vst3; do
        b="$DST/$name.$kind/Contents"
        mkdir -p "$b/MacOS"
        cp "$lib" "$b/MacOS/$name"
        plist "$name" "$kind" > "$b/Info.plist"
        printf 'BNDL????' > "$b/PkgInfo"
      done
      ;;
    windows)
      lib="$SRC/$name.dll"
      cp "$lib" "$DST/$name.clap"
      w=x86_64-win; [[ "$ARCH" == aarch64 ]] && w=arm64-win
      mkdir -p "$DST/$name.vst3/Contents/$w"
      cp "$lib" "$DST/$name.vst3/Contents/$w/$name.vst3"
      ;;
  esac
  echo "  $DST/$name.clap  $DST/$name.vst3"
done
