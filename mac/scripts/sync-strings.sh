#!/bin/bash
# Adds every string the code uses to Resources/Localizable.xcstrings. Translate the new entries,
# then run ./scripts/test.sh, which fails if any language is missing one.
set -euo pipefail
cd "$(dirname "$0")/.."
export CLANG_MODULE_CACHE_PATH="$PWD/.build/ModuleCache"
export SOUNDVEIL_COREML="${COREML:-OFF}"
STRINGS=$(mktemp -d)
trap 'rm -rf "$STRINGS"' EXIT
find Sources/Soundveil Sources/CaptionCore Sources/ShortcutCore Sources/OverlayCore -name '*.swift' -exec touch {} +
swift build -c release --disable-sandbox --cache-path "$PWD/.build/cache" --product Soundveil \
 -Xswiftc -emit-localized-strings -Xswiftc -emit-localized-strings-path -Xswiftc "$STRINGS"
DATA=()
for file in "$STRINGS"/*.stringsdata; do DATA+=(--stringsdata "$file"); done
xcrun xcstringstool sync Resources/Localizable.xcstrings "${DATA[@]}"
echo "Synced $(xcrun xcstringstool print Resources/Localizable.xcstrings | wc -l | tr -d ' ') lines of strings."
