#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"

"$ROOT/packaging/build-bundle.sh"
export ELXTUBE_SKIP_BUILD=1
"$ROOT/packaging/build-deb.sh"
"$ROOT/packaging/build-rpm.sh"
"$ROOT/packaging/build-arch.sh"
"$ROOT/packaging/build-appimage.sh"
(
  cd "$ROOT/dist"
  sha256sum ./*.deb ./*.rpm ./*.pkg.tar.zst ./*.AppImage > SHA256SUMS
)
