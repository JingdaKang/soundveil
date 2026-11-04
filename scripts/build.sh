#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
PIN=48f628a84833905ee4a0658ee6d4a5c915ce1997
if [ ! -d vendor/whisper.cpp/.git ]; then
 mkdir -p vendor
 git clone --depth 1 --branch v1.8.7 https://github.com/ggml-org/whisper.cpp.git vendor/whisper.cpp
fi
[ "$(git -C vendor/whisper.cpp rev-parse HEAD)" = "$PIN" ] || { echo "Unexpected whisper.cpp revision" >&2; exit 1; }
cmake -S vendor/whisper.cpp -B vendor/whisper.cpp/build -DCMAKE_BUILD_TYPE=Release -DCMAKE_OSX_DEPLOYMENT_TARGET=14.0 -DBUILD_SHARED_LIBS=OFF -DGGML_METAL=ON -DGGML_METAL_EMBED_LIBRARY=ON -DWHISPER_COREML="${COREML:-OFF}" -DWHISPER_BUILD_TESTS=OFF -DWHISPER_BUILD_EXAMPLES=ON
cmake --build vendor/whisper.cpp/build -j "$(sysctl -n hw.logicalcpu)"
export CLANG_MODULE_CACHE_PATH="$PWD/.build/ModuleCache"
export SOUNDVEIL_COREML="${COREML:-OFF}"
swift build -c release --disable-sandbox --cache-path "$PWD/.build/cache"
APP="$PWD/dist/Soundveil.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp .build/release/Soundveil "$APP/Contents/MacOS/Soundveil"
cp Info.plist "$APP/Contents/Info.plist"
cp LICENSE "$APP/Contents/Resources/LICENSE.txt"
cp vendor/whisper.cpp/LICENSE "$APP/Contents/Resources/whisper-LICENSE.txt"
IDENTITY="${SOUNDVEIL_SIGN_IDENTITY:-Soundveil Local Signing}"
if security find-certificate -c "$IDENTITY" >/dev/null 2>&1; then
 codesign --force --sign "$IDENTITY" --identifier org.soundveil.mac "$APP"
else
 echo "warning: no \"$IDENTITY\" identity; ad-hoc signing. Recording permission will not survive this build. See scripts/setup-signing.sh." >&2
 codesign --force --sign - "$APP"
fi
echo "Built: $APP"
