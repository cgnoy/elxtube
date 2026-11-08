#!/usr/bin/env bash
# Build & run ytkw, log ke ~/lfitube/debug.log (overwrite tiap run)

set -e
cd "$(dirname "$0")"

echo "=== Building ==="
cargo build --release 2>&1 | tail -3

echo "=== Running ytkw (log -> debug.log) ==="
./target/release/ytkw 2>&1 | tee debug.log
