#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
source "$ROOT/packaging/build-bundle.sh"

TOOLS="$DIST/tools"
APPDIR="$DIST/AppDir"
LINUXDEPLOY="$TOOLS/linuxdeploy-x86_64.AppImage"
GTK_PLUGIN="$TOOLS/linuxdeploy-plugin-gtk.sh"
APPIMAGETOOL="$TOOLS/appimagetool-x86_64.AppImage"
mkdir -p "$TOOLS"

download() {
  local url=$1
  local destination=$2
  if [ ! -s "$destination" ]; then
    curl --fail --location --retry 3 --output "$destination" "$url"
    chmod +x "$destination"
  fi
}

download "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage" "$LINUXDEPLOY"
download "https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh" "$GTK_PLUGIN"
download "https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage" "$APPIMAGETOOL"

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" \
  "$APPDIR/usr/share/icons/hicolor/512x512/apps" "$APPDIR/usr/share/metainfo"
install -Dm755 "$STAGE/elxtube" "$APPDIR/usr/bin/elxtube"
install -Dm644 "$STAGE/elxtube.desktop" "$APPDIR/usr/share/applications/elxtube.desktop"
install -Dm644 "$STAGE/icons/elxtube.png" "$APPDIR/usr/share/icons/hicolor/512x512/apps/elxtube.png"
cp "$APPDIR/usr/share/icons/hicolor/512x512/apps/elxtube.png" "$APPDIR/.DirIcon"

cat > "$APPDIR/AppRun" <<'EOF'
#!/usr/bin/env bash
export PATH="${APPDIR}/usr/bin:${PATH}"
export LD_LIBRARY_PATH="${APPDIR}/usr/lib:${LD_LIBRARY_PATH:-}"
export GTK_PATH="${APPDIR}/usr/lib/gtk-4.0"
export GIO_MODULE_DIR="${APPDIR}/usr/lib/gio/modules"
export XDG_DATA_DIRS="${APPDIR}/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
exec "${APPDIR}/usr/bin/elxtube" "$@"
EOF
chmod +x "$APPDIR/AppRun"

mkdir -p "$APPDIR/usr/lib/gtk-4.0" "$APPDIR/usr/share/glib-2.0/schemas"
if [ -d /usr/lib/gtk-4.0 ]; then
  cp -a /usr/lib/gtk-4.0/. "$APPDIR/usr/lib/gtk-4.0/"
fi
if [ -d /usr/share/glib-2.0/schemas ]; then
  cp -a /usr/share/glib-2.0/schemas/. "$APPDIR/usr/share/glib-2.0/schemas/"
fi
if command -v glib-compile-schemas >/dev/null 2>&1; then
  glib-compile-schemas "$APPDIR/usr/share/glib-2.0/schemas"
fi

cat > "$APPDIR/usr/share/metainfo/io.github.cgnoy.elxtube.appdata.xml" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>io.github.cgnoy.elxtube</id>
  <name>elxTube</name>
  <summary>Lightweight native YouTube client</summary>
  <metadata_license>CC0-1.0</metadata_license>
  <project_license>GPL-3.0-or-later</project_license>
  <url type="homepage">https://github.com/cgnoy/elxtube</url>
  <developer id="cgnoy"><name>cgnoy</name></developer>
  <content_rating type="oars-1.1"/>
  <description><p>elxTube is a lightweight native YouTube client built with GTK4 and libmpv for low-spec Linux systems.</p></description>
  <launchable type="desktop-id">elxtube.desktop</launchable>
  <provides><binary>elxtube</binary></provides>
  <releases><release version="0.1.0" date="2026-11-27"/></releases>
</component>
EOF

rm -f "$DIST/elxTube-$ARCH.AppImage" "$DIST/elxtube-$VERSION-$ARCH.AppImage"

(
  cd "$DIST"
  PATH="$TOOLS:$PATH" "$LINUXDEPLOY" --appimage-extract-and-run --output appimage \
    --appdir "$APPDIR" \
    --executable "$STAGE/elxtube" \
    --desktop-file "$STAGE/elxtube.desktop" \
    --icon-file "$STAGE/icons/elxtube.png"
)

APPIMAGE_PATH="$DIST/elxtube-$VERSION-$ARCH.AppImage"
rm -f "$DIST/elxTube-$ARCH.AppImage" "$APPIMAGE_PATH"
"$APPIMAGETOOL" --appimage-extract-and-run "$APPDIR" "$APPIMAGE_PATH"
chmod +x "$APPIMAGE_PATH"
printf 'Built %s\n' "$APPIMAGE_PATH"
