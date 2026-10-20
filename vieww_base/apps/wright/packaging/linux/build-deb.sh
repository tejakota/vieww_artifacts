#!/usr/bin/env bash
# Wright for Linux: a .deb (Debian, Ubuntu and friends) and a .tar.gz.
#
#   packaging/linux/build-deb.sh <version> <binary> <out dir>
#
# The binary is built on Ubuntu 22.04, so it runs on glibc 2.35 and newer.
# Window libraries (X11 / Wayland), audio (PipeWire / Pulse / ALSA) and the
# keychain (`secret-tool`) are loaded at run time and listed as
# recommendations, not hard dependencies: the CLI and `wright daemon` run
# on a headless server with none of them.
set -euo pipefail

version="$1"
bin="$2"
out="$3"
here="$(cd "$(dirname "$0")" && pwd)"
app="$(cd "$here/../.." && pwd)"
work="$(mktemp -d)"
root="$work/wright_${version}_amd64"

mkdir -p "$root/DEBIAN" "$root/usr/bin" "$root/usr/share/applications" "$root/usr/share/doc/wright" "$out"
install -m 755 "$bin" "$root/usr/bin/wright"
install -m 644 "$here/wright.desktop" "$root/usr/share/applications/wright.desktop"
for s in 64 128 256 512; do
  mkdir -p "$root/usr/share/icons/hicolor/${s}x${s}/apps"
  "$bin" ui --icon "$root/usr/share/icons/hicolor/${s}x${s}/apps/wright.png" --size "$s"
done
install -m 644 "$app/README.md" "$app/CHANGELOG.md" "$root/usr/share/doc/wright/"
install -m 644 "$app/LICENSE" "$root/usr/share/doc/wright/copyright"
cp -r "$app/examples" "$root/usr/share/doc/wright/examples"

size_kb="$(du -sk "$root/usr" | cut -f1)"
cat > "$root/DEBIAN/control" <<EOF
Package: wright
Version: ${version}
Architecture: amd64
Maintainer: The Wright Authors <noreply@wright.dev>
Installed-Size: ${size_kb}
Depends: libc6 (>= 2.35)
Recommends: libxkbcommon0, libwayland-client0 | libx11-6, libsecret-tools, zenity | kdialog
Section: devel
Priority: optional
Homepage: https://github.com/tejakota/vieww_artifacts
Description: Describe the outcome; agents build, check and deliver it.
 Wright turns what you describe into a typed plan, shows exactly what
 will run and what it may touch, and only after you approve does a
 workforce of agents build it in a sandbox, test it, review it and
 deliver it. Run `wright` for the window, `wright help` for the CLI.
EOF

dpkg-deb --build --root-owner-group "$root" "$out/wright_${version}_amd64.deb"

# The portable archive: the same files, no installer.
tar_root="$work/wright-${version}-linux-x64"
mkdir -p "$tar_root"
cp "$bin" "$tar_root/wright"
cp "$here/wright.desktop" "$app/README.md" "$app/CHANGELOG.md" "$app/LICENSE" "$tar_root/"
cp "$root/usr/share/icons/hicolor/512x512/apps/wright.png" "$tar_root/wright.png"
cp -r "$app/examples" "$tar_root/examples"
tar -C "$work" -czf "$out/wright-${version}-linux-x64.tar.gz" "wright-${version}-linux-x64"
echo "built $out/wright_${version}_amd64.deb and the .tar.gz"
