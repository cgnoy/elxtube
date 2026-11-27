#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
source "$ROOT/packaging/build-bundle.sh"

BUNDLE_SHA=$(sha256sum "$DIST/$BUNDLE_NAME.tar.gz" | cut -d' ' -f1)
sed -i "s/sha256sums=('[^']*')/sha256sums=('$BUNDLE_SHA')/" "$ROOT/packaging/PKGBUILD"

cp "$DIST/$BUNDLE_NAME.tar.gz" "$ROOT/packaging/"
cleanup() {
  rm -rf "$ROOT/packaging/src" "$ROOT/packaging/pkg"
}
trap cleanup EXIT
PKGDEST="$DIST" makepkg --dir "$ROOT/packaging" --cleanbuild --force --noconfirm
printf 'Built %s\n' "$DIST/elxtube-$VERSION-1-$ARCH.pkg.tar.zst"
