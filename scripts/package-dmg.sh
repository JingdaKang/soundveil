#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."

case "${1:-}" in
 "") ./scripts/build.sh ;;
 --skip-build) ;;
 *) echo "Usage: $0 [--skip-build]" >&2; exit 2 ;;
esac

APP="$PWD/dist/Soundveil.app"
[ -d "$APP" ] || { echo "Build Soundveil.app first." >&2; exit 1; }
codesign --verify --deep --strict "$APP"
VERSION=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "Expected a numeric major.minor.patch version." >&2; exit 1; }
NAME="Soundveil-$VERSION-arm64.dmg"
STAGING=$(mktemp -d "$PWD/dist/.dmg-stage.XXXXXX")
trap 'rm -rf "$STAGING"' EXIT
mkdir "$STAGING/contents"
ditto "$APP" "$STAGING/contents/Soundveil.app"
ln -s /Applications "$STAGING/contents/Applications"
cat > "$STAGING/contents/Install.txt" <<'EOF'
Soundveil — local live captions for macOS

Requires Apple Silicon and macOS 14 or later.

1. Drag Soundveil.app onto Applications.
2. Eject this disk image and open Soundveil from Applications.
3. Download a recognition model in the app, then start captions.
4. Grant the macOS recording permission when prompted.

Model weights are downloaded separately; they are not included in this image.
This development build is ad-hoc signed and is not notarized by Apple.
EOF

hdiutil create -volname Soundveil -srcfolder "$STAGING/contents" \
 -fs HFS+ -format UDZO "$STAGING/$NAME"
hdiutil verify "$STAGING/$NAME"
mv -f "$STAGING/$NAME" "dist/$NAME"
(cd dist && LC_ALL=C shasum -a 256 "$NAME" > "$NAME.sha256")
echo "Packaged: $PWD/dist/$NAME"
