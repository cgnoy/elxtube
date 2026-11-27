#!/usr/bin/env bash
# Build & run elxTube, log ke debug.log (overwrite tiap run)

set -euo pipefail
cd "$(dirname "$0")"

echo "=== Building ==="
cargo build --release --locked

echo "=== Running elxTube (log -> debug.log) ==="
./target/release/elxtube 2>&1 | tee debug.log
