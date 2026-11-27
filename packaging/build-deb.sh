#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
source "$ROOT/packaging/build-bundle.sh"
DEB_ARCH=amd64

PACKAGE_ROOT="$DIST/deb-root"
rm -rf "$PACKAGE_ROOT"
mkdir -p "$PACKAGE_ROOT/DEBIAN" "$PACKAGE_ROOT/usr/bin" "$PACKAGE_ROOT/usr/share/applications" \
  "$PACKAGE_ROOT/usr/share/icons/hicolor/512x512/apps" "$PACKAGE_ROOT/usr/share/doc/elxtube"

install -Dm755 "$STAGE/elxtube" "$PACKAGE_ROOT/usr/bin/elxtube"
install -Dm644 "$STAGE/elxtube.desktop" "$PACKAGE_ROOT/usr/share/applications/elxtube.desktop"
install -Dm644 "$STAGE/icons/elxtube.png" "$PACKAGE_ROOT/usr/share/icons/hicolor/512x512/apps/elxtube.png"
install -Dm644 "$STAGE/LICENSE" "$PACKAGE_ROOT/usr/share/doc/elxtube/copyright"

cat > "$PACKAGE_ROOT/DEBIAN/control" <<EOF
Package: elxtube
Version: $VERSION
Section: video
Priority: optional
Architecture: $DEB_ARCH
Maintainer: elxTube contributors <noreply@example.com>
Depends: libc6, libgtk-4-1, libmpv2, python3, ca-certificates
Description: Lightweight native YouTube client
 A GTK4 and libmpv client designed for low-spec Linux systems.
EOF

(
  cd "$PACKAGE_ROOT/DEBIAN"
  find . -type f ! -name md5sums -printf '%P\0' | sort -z | xargs -0 md5sum > md5sums
)

DEB_PATH="$DIST/elxtube_${VERSION}_${DEB_ARCH}.deb"
rm -f "$DEB_PATH"
if command -v dpkg-deb >/dev/null 2>&1; then
  dpkg-deb --root-owner-group --build "$PACKAGE_ROOT" "$DEB_PATH"
else
  podman run --rm -v "$DIST:/dist:Z" docker.io/library/ubuntu:26.04 \
    dpkg-deb --root-owner-group --build "/dist/deb-root" "/dist/elxtube_${VERSION}_${DEB_ARCH}.deb"
fi
printf 'Built %s\n' "$DEB_PATH"
