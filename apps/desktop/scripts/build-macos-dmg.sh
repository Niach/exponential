#!/usr/bin/env bash
#
# Package the built Exponential .app into a distributable .dmg, and — when
# App Store Connect notary credentials are present — notarize + staple it so
# Gatekeeper accepts the download without a warning (masterplan v5 §11.2).
#
#   scripts/build-macos-dmg.sh <channel>          # channel: production | staging
#
# Prereq: scripts/build-macos-app.sh already produced ${TARGET}/<AppName>.app,
# ideally Developer-ID signed (MACOS_SIGN_IDENTITY) — notarization REQUIRES a
# hardened-runtime Developer ID signature. An ad-hoc-signed .app can still be
# wrapped into a .dmg here but will not pass notarization (the submit step is
# skipped when notary creds are absent, so the .dmg is simply un-notarized).
#
# Notary credentials (all three → notarize; any missing → skip, .dmg only):
#   NOTARY_KEY_PATH   path to the App Store Connect API key .p8 file
#   NOTARY_KEY_ID     the key id
#   NOTARY_ISSUER_ID  the issuer id
#
# Output: ${TARGET}/Exponential-<channel>.dmg  (stable name → the marketing
# DownloadSection `/releases/latest/download/Exponential-production.dmg` link).
set -euo pipefail

CHANNEL="${1:-production}"
DESKTOP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-${DESKTOP_DIR}/target}"

case "$CHANNEL" in
  production) APP_NAME="Exponential" ;;
  staging)    APP_NAME="Exponential (staging)" ;;
  *) echo "unknown channel: $CHANNEL (expected production|staging)" >&2; exit 1 ;;
esac

APP_DIR="${TARGET_DIR}/${APP_NAME}.app"
[ -d "$APP_DIR" ] || {
  echo "no .app at $APP_DIR — run build-macos-app.sh $CHANNEL first" >&2
  exit 1
}

DMG="${TARGET_DIR}/Exponential-${CHANNEL}.dmg"
rm -f "$DMG"

# --- Assemble the .dmg staging root (app + /Applications drop target) ------
STAGE="$(mktemp -d)"
cp -R "$APP_DIR" "$STAGE/"
ln -s /Applications "$STAGE/Applications"

# `hdiutil create` intermittently dies with "Resource busy" on the CI runners
# (diskarbitration/Spotlight still holding the freshly detached scratch volume
# — it took out the 0.8.34 staging leg). It is transient every time, so retry
# before failing the release build.
for attempt in 1 2 3; do
  if hdiutil create \
    -volname "$APP_NAME" \
    -srcfolder "$STAGE" \
    -fs HFS+ \
    -format UDZO \
    -ov \
    "$DMG"; then
    break
  fi
  [ "$attempt" = 3 ] && { echo "hdiutil create failed after 3 attempts" >&2; exit 1; }
  echo "hdiutil create failed (attempt ${attempt}/3) — retrying in 15s …" >&2
  sleep 15
done
rm -rf "$STAGE"

# --- Notarize + staple (gated on credentials) ------------------------------
if [ -n "${NOTARY_KEY_PATH:-}" ] && [ -n "${NOTARY_KEY_ID:-}" ] && [ -n "${NOTARY_ISSUER_ID:-}" ]; then
  echo "notarizing $DMG …"
  # Sign the .dmg itself with the same identity when available (recommended;
  # the .app inside is already signed by build-macos-app.sh).
  if [ -n "${MACOS_SIGN_IDENTITY:-}" ]; then
    codesign --force --timestamp --sign "$MACOS_SIGN_IDENTITY" "$DMG"
  fi
  xcrun notarytool submit "$DMG" \
    --key "$NOTARY_KEY_PATH" \
    --key-id "$NOTARY_KEY_ID" \
    --issuer "$NOTARY_ISSUER_ID" \
    --wait
  # Staple the ticket so Gatekeeper validates offline.
  xcrun stapler staple "$DMG"
  xcrun stapler validate "$DMG"
  echo "notarized + stapled: $DMG"
else
  echo "note: notary creds absent — built un-notarized .dmg (not Gatekeeper-clean): $DMG"
fi

echo "built: $DMG"
