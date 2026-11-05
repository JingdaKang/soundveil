#!/bin/bash
# Packs a built program into a .deb and a .tar.gz. Run it from desktop/ after a release build.
set -euo pipefail

binary=${1:-target/release/soundveil}
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
name="soundveil-$version-linux-x86_64"
out=target/package
rm -rf "$out"
mkdir -p "$out"

stage="$out/stage"
install -Dm755 "$binary" "$stage/usr/bin/soundveil"
install -Dm644 packaging/linux/soundveil.desktop "$stage/usr/share/applications/soundveil.desktop"
install -Dm644 packaging/icon.png "$stage/usr/share/icons/hicolor/256x256/apps/soundveil.png"
install -Dm644 ../LICENSE "$stage/usr/share/doc/soundveil/copyright"

tar -czf "$out/$name.tar.gz" -C "$stage/usr" bin share

mkdir -p "$stage/DEBIAN"
size=$(du -sk "$stage/usr" | cut -f1)
cat > "$stage/DEBIAN/control" <<CONTROL
Package: soundveil
Version: $version
Architecture: amd64
Maintainer: Soundveil <noreply@users.noreply.github.com>
Installed-Size: $size
Depends: libpulse0, libfontconfig1, libfreetype6, libxkbcommon0, libgl1, libvulkan1
Section: sound
Priority: optional
Homepage: https://github.com/JingdaKang/soundveil
Description: Offline live captions for the audio playing on your computer
 Speech is recognized on this computer with whisper.cpp, and no audio or text
 leaves it.
CONTROL
dpkg-deb --root-owner-group --build "$stage" "$out/soundveil_${version}_amd64.deb"
rm -rf "$stage"
ls -la "$out"
