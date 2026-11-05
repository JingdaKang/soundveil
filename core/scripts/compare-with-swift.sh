#!/bin/bash
# Runs the Swift original and the Rust port on the same seeded input and compares every event,
# job, and caption update. Run this after changing logic in either, until the Mac app moves
# onto the core.
set -euo pipefail
cd "$(dirname "$0")/.."
SEEDS="${1:-300}"
(
 cd ../mac
 export CLANG_MODULE_CACHE_PATH="$PWD/.build/ModuleCache"
 swift build -c release --disable-sandbox --cache-path "$PWD/.build/cache" --product CoreTrace
)
cargo build -q --release -p soundveil-core --example trace
../mac/.build/release/CoreTrace "$SEEDS" > target/trace-swift.txt
target/release/examples/trace "$SEEDS" > target/trace-rust.txt
if cmp target/trace-swift.txt target/trace-rust.txt; then
 echo "Identical: $(wc -l < target/trace-rust.txt | tr -d ' ') lines from $SEEDS seeds."
else
 diff target/trace-swift.txt target/trace-rust.txt | head -20
 exit 1
fi
