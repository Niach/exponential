#!/usr/bin/env bash
#
# Build a Linux AppImage for the Exponential desktop IDE.
#
#   scripts/build-appimage.sh <channel>          # channel: production | staging
#
# Mirrors the approach Zed uses for its relocatable Linux build (bundle the
# non-core shared libs the binary pulls in, leave the GPU/Wayland driver stack
# to the host), packaged as a single-file AppImage via linuxdeploy. The GPU/EGL
# libs are host-provided ON PURPOSE — they must match the user's driver — so an
# AppImage still needs the same host prereqs a normal build does
# (mesa/vulkan userspace, and libxkbcommon-x11 gets bundled by linuxdeploy).
#
# Assumes the release binary is already built:
#   production: cargo build --release -p app
#   staging:    cargo build --release -p app --features staging
#
# Requires on PATH: rsvg-convert (librsvg2-bin), wget, file. FUSE is NOT
# required — linuxdeploy runs via APPIMAGE_EXTRACT_AND_RUN.
set -euo pipefail

CHANNEL="${1:-production}"
DESKTOP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-${DESKTOP_DIR}/target}"
BIN="${TARGET_DIR}/release/exp-desktop"
ARCH="$(uname -m)"

case "$CHANNEL" in
  production) APP_ID="at.exponential";         APP_NAME="Exponential" ;;
  staging)    APP_ID="at.exponential.staging"; APP_NAME="Exponential (staging)" ;;
  *) echo "unknown channel: $CHANNEL (expected production|staging)" >&2; exit 1 ;;
esac

[ -x "$BIN" ] || { echo "release binary not found at $BIN — build it first" >&2; exit 1; }

WORK="${TARGET_DIR}/appimage-${CHANNEL}"
APPDIR="${WORK}/AppDir"
rm -rf "$WORK"
mkdir -p "$APPDIR/usr/bin" \
         "$APPDIR/usr/share/applications" \
         "$APPDIR/usr/share/icons/hicolor/256x256/apps"

# --- Binary ---------------------------------------------------------------
cp "$BIN" "$APPDIR/usr/bin/exp-desktop"

# --- Icon (rasterize the vector logo) -------------------------------------
# Use the WHITE-on-transparent logo variant (EXP-16): the plain `logo.svg` is a
# black disc that all but vanishes on the dark taskbars/launchers desktop Linux
# defaults to. The white disc reads on both light and dark shelves.
ICON_PNG="$APPDIR/usr/share/icons/hicolor/256x256/apps/${APP_ID}.png"
LOGO_SVG="${DESKTOP_DIR}/assets/icons/logo-white.svg"
if command -v rsvg-convert >/dev/null; then
  rsvg-convert -w 256 -h 256 "$LOGO_SVG" -o "$ICON_PNG"
elif command -v convert >/dev/null; then
  convert -background none -resize 256x256 "$LOGO_SVG" "$ICON_PNG"
else
  echo "need rsvg-convert (librsvg2-bin) or ImageMagick to rasterize the icon" >&2
  exit 1
fi

# --- .desktop -------------------------------------------------------------
# Exec is a bare `exp-desktop` here (AppImage metadata); at runtime the app
# self-registers a host .desktop pointing at $APPIMAGE for exp:// callbacks.
DESKTOP="$APPDIR/usr/share/applications/${APP_ID}.desktop"
cat > "$DESKTOP" <<EOF
[Desktop Entry]
Type=Application
Name=${APP_NAME}
Comment=Real-time issue tracker and coding IDE
Exec=exp-desktop %U
Icon=${APP_ID}
Terminal=false
Categories=Development;ProjectManagement;
MimeType=x-scheme-handler/exp;
StartupWMClass=exp-desktop
EOF

# --- linuxdeploy ----------------------------------------------------------
TOOLS="${TARGET_DIR}/appimage-tools"
mkdir -p "$TOOLS"
LINUXDEPLOY="${TOOLS}/linuxdeploy-${ARCH}.AppImage"
if [ ! -x "$LINUXDEPLOY" ]; then
  wget -q -O "$LINUXDEPLOY" \
    "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-${ARCH}.AppImage"
  chmod +x "$LINUXDEPLOY"
fi

export APPIMAGE_EXTRACT_AND_RUN=1
export OUTPUT="${TARGET_DIR}/Exponential-${CHANNEL}-${ARCH}.AppImage"
rm -f "$OUTPUT"

"$LINUXDEPLOY" \
  --appdir "$APPDIR" \
  --executable "$APPDIR/usr/bin/exp-desktop" \
  --desktop-file "$DESKTOP" \
  --icon-file "$ICON_PNG" \
  --output appimage

# The AppImage must be executable — and a browser/HTTP download of the raw
# asset from a GitHub Release strips the +x bit. Ship a `.tar.gz` alongside the
# raw `.AppImage` (EXP-16): tar preserves file mode, so `tar xzf …` yields a
# ready-to-run AppImage with no manual `chmod +x`. The raw asset stays published
# too, so existing `releases/latest/download/<name>.AppImage` links keep working.
chmod +x "$OUTPUT"
TARBALL="${OUTPUT}.tar.gz"
rm -f "$TARBALL"
tar -czf "$TARBALL" -C "$(dirname "$OUTPUT")" "$(basename "$OUTPUT")"

echo "built: $OUTPUT"
echo "built: $TARBALL"
