#!/usr/bin/env bash
#
# EXP-1430: assemble + codesign a macOS .app from a built executable. Shared by
# every desktop app of this owner (Exponential's build-macos-app.sh delegates
# here; Niach/spezl calls it through the macos-app action).
#
#   bundle.sh --binary <path> [--binary <path> …]   one per arch slice; 2+ → `lipo -create` (universal)
#             --app-name <name>                     bundle dir + CFBundleName/DisplayName
#             --bundle-id <id>                      CFBundleIdentifier
#             --executable <name>                   Contents/MacOS/<name> + CFBundleExecutable
#             --plist <Info.plist template>         ${VERSION}/${BUILD} substituted; URL schemes live here
#             --out-dir <dir>                       writes <dir>/<app-name>.app
#             [--version <v>] [--build <n>]         default 0.0.0 / 0
#             [--entitlements <file>]               applied to the executable + bundle (signed builds)
#             [--icon <file.svg|file.icns>]         svg rasterized when a rasterizer is on PATH
#             [--frameworks-dir <dir>]              its *.dylib/*.framework → Contents/Frameworks, signed with the app identity
#             [--resource <file|dir>] …             → Contents/Resources (a dir's contents), copied BEFORE signing
#             [--lsuielement true|false]            true → LSUIElement (tray-only, no Dock icon); false → key removed
#             [--identity <codesign identity>]      also MACOS_SIGN_IDENTITY; empty → ad-hoc `-s -`
#
# Signed (identity set) = hardened runtime + secure timestamp, inner code first
# (the notarization prerequisite); ad-hoc = launches locally, not Gatekeeper-clean.
set -euo pipefail

BINARIES=(); RESOURCES=()
APP_NAME=""; BUNDLE_ID=""; EXECUTABLE=""; PLIST_SRC=""; OUT_DIR=""
VERSION="${VERSION:-0.0.0}"; BUILD="${BUILD:-0}"
ENTITLEMENTS=""; ICON=""; FRAMEWORKS_DIR=""; LSUIELEMENT=""
IDENTITY="${MACOS_SIGN_IDENTITY:-}"

while [ $# -gt 0 ]; do
  case "$1" in
    --binary) BINARIES+=("$2"); shift 2 ;;
    --app-name) APP_NAME="$2"; shift 2 ;;
    --bundle-id) BUNDLE_ID="$2"; shift 2 ;;
    --executable) EXECUTABLE="$2"; shift 2 ;;
    --plist) PLIST_SRC="$2"; shift 2 ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --build) BUILD="$2"; shift 2 ;;
    --entitlements) ENTITLEMENTS="$2"; shift 2 ;;
    --icon) ICON="$2"; shift 2 ;;
    --frameworks-dir) FRAMEWORKS_DIR="$2"; shift 2 ;;
    --resource) RESOURCES+=("$2"); shift 2 ;;
    --lsuielement) LSUIELEMENT="$2"; shift 2 ;;
    --identity) IDENTITY="$2"; shift 2 ;;
    *) echo "bundle.sh: unknown argument $1" >&2; exit 2 ;;
  esac
done

for req in APP_NAME BUNDLE_ID EXECUTABLE PLIST_SRC OUT_DIR; do
  [ -n "${!req}" ] || { echo "bundle.sh: missing --$(echo "$req" | tr 'A-Z_' 'a-z-')" >&2; exit 2; }
done
[ ${#BINARIES[@]} -gt 0 ] || { echo "bundle.sh: missing --binary" >&2; exit 2; }
for b in "${BINARIES[@]}"; do
  [ -x "$b" ] || { echo "bundle.sh: executable not found at $b — build it first" >&2; exit 1; }
done
[ -f "$PLIST_SRC" ] || { echo "bundle.sh: no Info.plist template at $PLIST_SRC" >&2; exit 1; }
case "$LSUIELEMENT" in ""|true|false) ;; *) echo "bundle.sh: --lsuielement is true|false" >&2; exit 2 ;; esac

mkdir -p "$OUT_DIR"
APP_DIR="$(cd "$OUT_DIR" && pwd)/${APP_NAME}.app"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"
EXE="$APP_DIR/Contents/MacOS/${EXECUTABLE}"

# --- Executable (one slice, or a universal one) -----------------------------
if [ ${#BINARIES[@]} -gt 1 ]; then
  lipo -create "${BINARIES[@]}" -output "$EXE"
else
  cp "${BINARIES[0]}" "$EXE"
fi
chmod +x "$EXE"
echo "executable: $(lipo -archs "$EXE" 2>/dev/null || echo '?') → $EXE" >&2

# --- Resources + Frameworks (BEFORE signing: files added after break the seal)
for r in ${RESOURCES[@]+"${RESOURCES[@]}"}; do
  if [ -d "$r" ]; then
    cp -R "$r"/. "$APP_DIR/Contents/Resources/"
  elif [ -f "$r" ]; then
    cp "$r" "$APP_DIR/Contents/Resources/"
  else
    echo "bundle.sh: resource $r not found" >&2; exit 1
  fi
done
if [ -n "$FRAMEWORKS_DIR" ] && [ -d "$FRAMEWORKS_DIR" ] \
  && [ -n "$(ls -A "$FRAMEWORKS_DIR" 2>/dev/null)" ]; then
  mkdir -p "$APP_DIR/Contents/Frameworks"
  cp -R "$FRAMEWORKS_DIR"/. "$APP_DIR/Contents/Frameworks/"
fi

# --- Info.plist --------------------------------------------------------------
PLIST="$APP_DIR/Contents/Info.plist"
sed -e "s/\${VERSION}/${VERSION}/g" -e "s/\${BUILD}/${BUILD}/g" "$PLIST_SRC" > "$PLIST"
pb() { /usr/libexec/PlistBuddy -c "$1" "$PLIST"; }
pset() { pb "Set :$1 $3" 2>/dev/null || pb "Add :$1 $2 $3"; }
pset CFBundleIdentifier string "$BUNDLE_ID"
pset CFBundleName string "$APP_NAME"
pset CFBundleDisplayName string "$APP_NAME"
pset CFBundleExecutable string "$EXECUTABLE"
pset CFBundlePackageType string APPL
case "$LSUIELEMENT" in
  true) pset LSUIElement bool true ;;
  false) pb "Delete :LSUIElement" 2>/dev/null || true ;;
esac

# --- Icon (optional) -----------------------------------------------------------
if [ -n "$ICON" ] && [ -f "$ICON" ]; then
  case "$ICON" in
    *.icns) cp "$ICON" "$APP_DIR/Contents/Resources/AppIcon.icns" && pset CFBundleIconFile string AppIcon ;;
    *)
      RASTERIZE=""
      command -v rsvg-convert >/dev/null 2>&1 && RASTERIZE="rsvg-convert"
      [ -z "$RASTERIZE" ] && command -v convert >/dev/null 2>&1 && RASTERIZE="convert"
      if [ -n "$RASTERIZE" ]; then
        ICONSET="$(mktemp -d)/app.iconset"
        mkdir -p "$ICONSET"
        for size in 16 32 128 256 512; do
          for scale in 1 2; do
            px=$((size * scale))
            name="icon_${size}x${size}"; [ "$scale" = "2" ] && name="${name}@2x"
            if [ "$RASTERIZE" = "rsvg-convert" ]; then
              rsvg-convert -w "$px" -h "$px" "$ICON" -o "$ICONSET/${name}.png"
            else
              convert -background none -resize "${px}x${px}" "$ICON" "$ICONSET/${name}.png"
            fi
          done
        done
        iconutil -c icns "$ICONSET" -o "$APP_DIR/Contents/Resources/AppIcon.icns" \
          && pset CFBundleIconFile string AppIcon || true
      else
        echo "note: no SVG rasterizer (rsvg-convert/ImageMagick) — bundling without a custom icon" >&2
      fi
      ;;
  esac
fi

# --- Codesign (inner code first, then the bundle; outside-in is invalid) -------
inner=()
if [ -d "$APP_DIR/Contents/Frameworks" ]; then
  while IFS= read -r f; do inner+=("$f"); done < <(
    find "$APP_DIR/Contents/Frameworks" -maxdepth 1 \( -name '*.dylib' -o -name '*.framework' \) | sort)
fi
if [ -n "$IDENTITY" ]; then
  echo "signing with Developer ID: ${IDENTITY}" >&2
  ent=(); [ -n "$ENTITLEMENTS" ] && ent=(--entitlements "$ENTITLEMENTS")
  # Bundled libraries get OUR identity, so library validation (left on) accepts them.
  for f in ${inner[@]+"${inner[@]}"}; do
    codesign --force --options runtime --timestamp --sign "$IDENTITY" "$f"
  done
  codesign --force --options runtime --timestamp ${ent[@]+"${ent[@]}"} --sign "$IDENTITY" "$EXE"
  # The bundle's signature re-signs its main executable: same entitlements.
  codesign --force --options runtime --timestamp ${ent[@]+"${ent[@]}"} --sign "$IDENTITY" "$APP_DIR"
  codesign --verify --strict --verbose=2 "$APP_DIR"
else
  for f in ${inner[@]+"${inner[@]}"}; do
    codesign --force --sign - "$f" 2>/dev/null || true
  done
  codesign --force --sign - "$EXE" 2>/dev/null || true
  codesign --force --sign - "$APP_DIR" 2>/dev/null \
    || echo "warn: ad-hoc codesign failed (bundle still usable locally)" >&2
fi

echo "built: $APP_DIR  (id=${BUNDLE_ID})" >&2
# stdout = the bundle path only (callers capture it).
echo "$APP_DIR"
