#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
source "$ROOT/packaging/build-bundle.sh"

RPM_TOP="$DIST/rpmbuild"
RPM_ROOT="$RPM_TOP/BUILDROOT/elxtube-$VERSION-$ARCH"
rm -rf "$RPM_TOP"
mkdir -p "$RPM_ROOT/usr/bin" "$RPM_ROOT/usr/share/applications" \
  "$RPM_ROOT/usr/share/icons/hicolor/512x512/apps" "$RPM_ROOT/usr/share/licenses/elxtube" "$RPM_TOP/SPECS"

install -Dm755 "$STAGE/elxtube" "$RPM_ROOT/usr/bin/elxtube"
install -Dm644 "$STAGE/elxtube.desktop" "$RPM_ROOT/usr/share/applications/elxtube.desktop"
install -Dm644 "$STAGE/icons/elxtube.png" "$RPM_ROOT/usr/share/icons/hicolor/512x512/apps/elxtube.png"
install -Dm644 "$STAGE/LICENSE" "$RPM_ROOT/usr/share/licenses/elxtube/LICENSE"

cat > "$RPM_TOP/SPECS/elxtube.spec" <<EOF
Name:           elxtube
Version:        $VERSION
Release:        1
Summary:        Lightweight native YouTube client
License:        GPL-3.0-or-later
URL:            https://github.com/cgnoy/elxtube
BuildArch:      $ARCH
Requires:       gtk4
Requires:       mpv-libs
Requires:       python3
Requires:       ca-certificates

%description
A GTK4 and libmpv client designed for low-spec Linux systems.

%install
install -Dm755 %{_topdir}/BUILDROOT/elxtube-$VERSION-$ARCH/usr/bin/elxtube %{buildroot}/usr/bin/elxtube
install -Dm644 %{_topdir}/BUILDROOT/elxtube-$VERSION-$ARCH/usr/share/applications/elxtube.desktop %{buildroot}/usr/share/applications/elxtube.desktop
install -Dm644 %{_topdir}/BUILDROOT/elxtube-$VERSION-$ARCH/usr/share/icons/hicolor/512x512/apps/elxtube.png %{buildroot}/usr/share/icons/hicolor/512x512/apps/elxtube.png
install -Dm644 %{_topdir}/BUILDROOT/elxtube-$VERSION-$ARCH/usr/share/licenses/elxtube/LICENSE %{buildroot}/usr/share/licenses/elxtube/LICENSE

%files
%license /usr/share/licenses/elxtube/LICENSE
%{_bindir}/elxtube
%{_datadir}/applications/elxtube.desktop
%{_datadir}/icons/hicolor/512x512/apps/elxtube.png
EOF

RPM_PATH="$DIST/elxtube-$VERSION-1.$ARCH.rpm"
rm -f "$RPM_PATH"
if command -v rpmbuild >/dev/null 2>&1; then
  rpmbuild -bb --define "_topdir $RPM_TOP" "$RPM_TOP/SPECS/elxtube.spec"
  cp "$RPM_TOP/RPMS/$ARCH/elxtube-$VERSION-1.$ARCH.rpm" "$RPM_PATH"
else
  podman run --rm -v "$RPM_TOP:/rpmbuild:Z" docker.io/library/ubuntu:26.04 bash -lc \
    "apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq rpm && rpmbuild -bb --define '_topdir /rpmbuild' /rpmbuild/SPECS/elxtube.spec"
  cp "$RPM_TOP/RPMS/$ARCH/elxtube-$VERSION-1.$ARCH.rpm" "$RPM_PATH"
fi
printf 'Built %s\n' "$RPM_PATH"
