#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
export CLANG_MODULE_CACHE_PATH="$PWD/.build/ModuleCache"
export SOUNDVEIL_COREML="${COREML:-OFF}"
swift test --disable-sandbox --cache-path "$PWD/.build/cache"
