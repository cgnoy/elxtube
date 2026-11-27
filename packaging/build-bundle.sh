#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"

VERSION=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)
ARCH=${ARCH:-x86_64}
BUNDLE_NAME="elxtube-${VERSION}-${ARCH}"
DIST="$ROOT/dist"
STAGE="$DIST/$BUNDLE_NAME"

if [ "${ELXTUBE_SKIP_BUILD:-0}" != "1" ]; then
  rm -rf "$STAGE"
  mkdir -p "$STAGE/icons" "$DIST"
  cargo build --release --locked
  install -Dm755 "target/release/elxtube" "$STAGE/elxtube"
  install -Dm644 "elxtube.desktop" "$STAGE/elxtube.desktop"
  install -Dm644 "assets/icons/512x512.png" "$STAGE/icons/elxtube.png"
  install -Dm644 "LICENSE" "$STAGE/LICENSE"
  tar -C "$DIST" -czf "$DIST/$BUNDLE_NAME.tar.gz" "$BUNDLE_NAME"
  printf '%s\n' "$VERSION" > "$DIST/VERSION"
  printf '%s\n' "$ARCH" > "$DIST/ARCH"
  printf '%s\n' "$BUNDLE_NAME" > "$DIST/BUNDLE_NAME"
fi
