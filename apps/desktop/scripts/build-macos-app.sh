#!/usr/bin/env bash
#
# Assemble a macOS .app bundle for the Exponential desktop IDE.
#
#   scripts/build-macos-app.sh <channel>          # channel: production | staging
#
# Why a bundle at all: macOS only routes a custom URL scheme (our `exponential://`
# OAuth-callback / invite deep link, §5.7) to a Launch-Services-registered
# .app that declares `CFBundleURLTypes`. A bare Mach-O binary (what `cargo
# build` emits, and what CI used to ship) can never be that handler, so the
# callback fell through to whatever else claimed `exponential:`. This wraps the release
# binary in the bundle whose Info.plist (assets/packaging/Info.plist) declares
# the scheme, so `on_open_urls` (wired in app/src/main.rs) actually fires.
#
# Assumes the release binary is already built:
#   production: cargo build --release -p app
#   staging:    cargo build --release -p app --features staging
#
# Output: ${TARGET_DIR}/<AppName>.app (ad-hoc codesigned so it launches).
# Registration + launch is the caller's job (the `macapp:desktop` bun script
# does it locally; CI just zips + uploads the bundle).
set -euo pipefail

CHANNEL="${1:-production}"
DESKTOP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-${DESKTOP_DIR}/target}"
# Defaults to the release binary; override with EXP_DESKTOP_BIN for a debug or
# custom build (e.g. EXP_DESKTOP_BIN=target/debug/exp-desktop for local runs).
BIN="${EXP_DESKTOP_BIN:-${TARGET_DIR}/release/exp-desktop}"

# Channel identity. The bundle id is DISTINCT from the iOS app (`at.exponential`)
# so Launch Services never confuses the two; staging gets its own id + name so a
# prod and a staging build coexist (the iOS `AppConstants.isStaging` analog).
case "$CHANNEL" in
  production) BUNDLE_ID="at.exponential.desktop";         APP_NAME="Exponential" ;;
  staging)    BUNDLE_ID="at.exponential.desktop.staging"; APP_NAME="Exponential (staging)" ;;
  *) echo "unknown channel: $CHANNEL (expected production|staging)" >&2; exit 1 ;;
esac

[ -x "$BIN" ] || {
  echo "release binary not found at $BIN — build it first:" >&2
  echo "  cargo build --release -p app${CHANNEL:+ }${CHANNEL:+# add --features staging for staging}" >&2
  exit 1
}

# Version strings: real values come from the release tag via env; local builds
# default to a dev placeholder. CFBundleVersion must be a monotonic integer-ish
# string, CFBundleShortVersionString a dotted version.
VERSION="${VERSION:-0.0.0}"
BUILD="${BUILD:-0}"

APP_DIR="${TARGET_DIR}/${APP_NAME}.app"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"

# --- Binary ---------------------------------------------------------------
cp "$BIN" "$APP_DIR/Contents/MacOS/exp-desktop"
chmod +x "$APP_DIR/Contents/MacOS/exp-desktop"

# --- Info.plist (substitute placeholders, then patch per channel) ---------
PLIST_SRC="${DESKTOP_DIR}/assets/packaging/Info.plist"
PLIST_DST="$APP_DIR/Contents/Info.plist"
sed -e "s/\${VERSION}/${VERSION}/g" -e "s/\${BUILD}/${BUILD}/g" "$PLIST_SRC" > "$PLIST_DST"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier ${BUNDLE_ID}" "$PLIST_DST"
/usr/libexec/PlistBuddy -c "Set :CFBundleName ${APP_NAME}" "$PLIST_DST"
/usr/libexec/PlistBuddy -c "Set :CFBundleDisplayName ${APP_NAME}" "$PLIST_DST"

# --- Icon (optional) ------------------------------------------------------
# Rasterize the vector logo → .icns when a rasterizer is on PATH; otherwise
# ship without an icon (a generic one shows — irrelevant to exponential:// routing).
# Uses the macOS variant (EXP-68/EXP-143): logo-macos.svg draws the dark
# rounded-square plate macOS icons are expected to fill (a bare white disc on
# transparent washed out on the system's default grey icon backdrop) with the
# white disc padded inside it. Fall back to logo-white.svg if the macOS
# variant ever goes missing.
LOGO_SVG="${DESKTOP_DIR}/assets/icons/logo-macos.svg"
[ -f "$LOGO_SVG" ] || LOGO_SVG="${DESKTOP_DIR}/assets/icons/logo-white.svg"
RASTERIZE=""
command -v rsvg-convert >/dev/null 2>&1 && RASTERIZE="rsvg-convert"
[ -z "$RASTERIZE" ] && command -v convert >/dev/null 2>&1 && RASTERIZE="convert"
if [ -n "$RASTERIZE" ] && [ -f "$LOGO_SVG" ]; then
  ICONSET="$(mktemp -d)/exp.iconset"
  mkdir -p "$ICONSET"
  for size in 16 32 128 256 512; do
    for scale in 1 2; do
      px=$((size * scale))
      name="icon_${size}x${size}"; [ "$scale" = "2" ] && name="${name}@2x"
      if [ "$RASTERIZE" = "rsvg-convert" ]; then
        rsvg-convert -w "$px" -h "$px" "$LOGO_SVG" -o "$ICONSET/${name}.png"
      else
        convert -background none -resize "${px}x${px}" "$LOGO_SVG" "$ICONSET/${name}.png"
      fi
    done
  done
  iconutil -c icns "$ICONSET" -o "$APP_DIR/Contents/Resources/AppIcon.icns" \
    && /usr/libexec/PlistBuddy -c "Add :CFBundleIconFile string AppIcon" "$PLIST_DST" 2>/dev/null || true
else
  echo "note: no SVG rasterizer (rsvg-convert/ImageMagick) — bundling without a custom icon"
fi

# --- Codesign -------------------------------------------------------------
# Two modes, chosen by whether a real signing identity is configured:
#
#   MACOS_SIGN_IDENTITY set  → Developer ID Application, HARDENED RUNTIME +
#     secure timestamp (the notarization prerequisite). Release CI imports the
#     cert into a keychain and passes the identity hash; scripts/build-macos-dmg.sh
#     then notarizes + staples the .dmg.
#   unset                    → ad-hoc `-s -`. Gives the bundle a stable identity
#     so Launch Services treats it as one app and it launches locally without a
#     "damaged" prompt, but it is NOT Gatekeeper-clean for download (pre-account
#     fallback + local dev builds).
if [ -n "${MACOS_SIGN_IDENTITY:-}" ]; then
  echo "signing with Developer ID: ${MACOS_SIGN_IDENTITY}"
  # Inner Mach-O first, then the bundle (outside-in signing is invalid).
  codesign --force --options runtime --timestamp \
    --sign "$MACOS_SIGN_IDENTITY" "$APP_DIR/Contents/MacOS/exp-desktop"
  codesign --force --options runtime --timestamp \
    --sign "$MACOS_SIGN_IDENTITY" "$APP_DIR"
  codesign --verify --strict --verbose=2 "$APP_DIR"
else
  codesign --force --sign - "$APP_DIR/Contents/MacOS/exp-desktop" 2>/dev/null || true
  codesign --force --sign - "$APP_DIR" 2>/dev/null \
    || echo "warn: ad-hoc codesign failed (bundle still usable locally)"
fi

echo "built: $APP_DIR  (id=${BUNDLE_ID}, exponential:// handler)"
