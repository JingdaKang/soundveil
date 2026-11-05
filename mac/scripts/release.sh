#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."

# Publishes the disk image as a GitHub release. The app's update check offers the
# newest published release whose tag is newer than its own CFBundleShortVersionString.
case "${1:-}" in
 ""|--skip-build) ;;
 *) echo "Usage: $0 [--skip-build]" >&2; exit 2 ;;
esac
command -v gh >/dev/null || { echo "Install and sign in to the GitHub CLI (gh) first." >&2; exit 1; }
VERSION=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' Info.plist)
TAG="v$VERSION"
NAME="Soundveil-$VERSION-arm64.dmg"
[ -z "$(git status --porcelain)" ] || { echo "Commit or stash changes before releasing." >&2; exit 1; }
git fetch -q origin
[ "$(git rev-parse HEAD)" = "$(git rev-parse '@{u}')" ] || { echo "Push this branch first; the release must point at published source." >&2; exit 1; }
# Releases bump the patch number only; minor and major bumps are deliberate edits.
NEXT="${VERSION%.*}.$(( ${VERSION##*.} + 1 ))"
! gh release view "$TAG" >/dev/null 2>&1 || { echo "Release $TAG exists. Set CFBundleShortVersionString in Info.plist to $NEXT." >&2; exit 1; }
LATEST=$(gh release view --json tagName --jq .tagName 2>/dev/null || true)
if [ -n "$LATEST" ] && [ "$(printf '%s\n%s\n' "${LATEST#v}" "$VERSION" | sort -V | tail -1)" != "$VERSION" ]; then
 echo "$VERSION is older than the latest release $LATEST; installed apps would never offer it." >&2; exit 1
fi

./scripts/package-dmg.sh ${1:-}
BUILT=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' dist/Soundveil.app/Contents/Info.plist)
[ "$BUILT" = "$VERSION" ] || { echo "dist/Soundveil.app is $BUILT but Info.plist is $VERSION; rebuild without --skip-build." >&2; exit 1; }

gh release create "$TAG" "dist/$NAME" "dist/$NAME.sha256" --target "$(git rev-parse HEAD)" \
 --title "Soundveil $VERSION" --generate-notes --notes-file - <<'NOTES'
Open the disk image and drag Soundveil onto Applications, replacing the old copy.
Requires Apple Silicon and macOS 14 or later. Downloaded models are kept.

This build has no Apple Developer ID and is not notarized. If macOS says it can't verify
Soundveil, open System Settings → Privacy & Security and choose Open Anyway.
If captions are refused after updating while Soundveil is switched on under Screen &
System Audio Recording, run `tccutil reset ScreenCapture org.soundveil.mac`, click
Start captions, turn Soundveil on again, and reopen it.
NOTES
