#!/bin/bash
# Fetches the pinned whisper.cpp that soundveil-whisper builds. Every part uses this revision.
set -euo pipefail
cd "$(dirname "$0")/.."
PIN=48f628a84833905ee4a0658ee6d4a5c915ce1997
if [ ! -d vendor/whisper.cpp/.git ]; then
 mkdir -p vendor
 git clone --depth 1 --branch v1.8.7 https://github.com/ggml-org/whisper.cpp.git vendor/whisper.cpp
fi
[ "$(git -C vendor/whisper.cpp rev-parse HEAD)" = "$PIN" ] || { echo "Unexpected whisper.cpp revision" >&2; exit 1; }
