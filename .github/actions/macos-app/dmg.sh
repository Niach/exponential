#!/usr/bin/env bash
#
# EXP-1430: wrap a built .app into a .dmg (app + /Applications drop target)
# and, when App Store Connect notary credentials are present, sign, notarize
# and staple it. Shared by Exponential's build-macos-dmg.sh and the macos-app
# action.
#
#   dmg.sh --app <path.app> --dmg <out.dmg> [--volname <name>] [--identity <id>]
#
# Notary credentials (all three → notarize; any missing → un-notarized .dmg):
#   NOTARY_KEY_PATH (the .p8 file), NOTARY_KEY_ID, NOTARY_ISSUER_ID
# Notarization REQUIRES a hardened-runtime Developer ID signature on the .app.
set -euo pipefail

APP_DIR=""; DMG=""; VOLNAME=""; IDENTITY="${MACOS_SIGN_IDENTITY:-}"
while [ $# -gt 0 ]; do
  case "$1" in
    --app) APP_DIR="$2"; shift 2 ;;
    --dmg) DMG="$2"; shift 2 ;;
    --volname) VOLNAME="$2"; shift 2 ;;
    --identity) IDENTITY="$2"; shift 2 ;;
    *) echo "dmg.sh: unknown argument $1" >&2; exit 2 ;;
  esac
done
[ -d "$APP_DIR" ] || { echo "dmg.sh: no .app at '$APP_DIR'" >&2; exit 1; }
[ -n "$DMG" ] || { echo "dmg.sh: missing --dmg" >&2; exit 2; }
[ -n "$VOLNAME" ] || VOLNAME="$(basename "$APP_DIR" .app)"
rm -f "$DMG"

STAGE="$(mktemp -d)"
cp -R "$APP_DIR" "$STAGE/"
ln -s /Applications "$STAGE/Applications"

# `hdiutil create` intermittently dies with "Resource busy" on the CI runners
# (diskarbitration/Spotlight still holding the freshly detached scratch volume
# — it took out the 0.8.34 staging leg). Transient every time: retry.
for attempt in 1 2 3; do
  if hdiutil create -volname "$VOLNAME" -srcfolder "$STAGE" -fs HFS+ -format UDZO -ov "$DMG"; then
    break
  fi
  [ "$attempt" = 3 ] && { echo "hdiutil create failed after 3 attempts" >&2; exit 1; }
  echo "hdiutil create failed (attempt ${attempt}/3) — retrying in 15s …" >&2
  sleep 15
done
rm -rf "$STAGE"

if [ -n "${NOTARY_KEY_PATH:-}" ] && [ -n "${NOTARY_KEY_ID:-}" ] && [ -n "${NOTARY_ISSUER_ID:-}" ]; then
  echo "notarizing $DMG …"
  # Sign the .dmg itself with the same identity when available (recommended).
  if [ -n "$IDENTITY" ]; then
    codesign --force --timestamp --sign "$IDENTITY" "$DMG"
  fi
  xcrun notarytool submit "$DMG" \
    --key "$NOTARY_KEY_PATH" --key-id "$NOTARY_KEY_ID" --issuer "$NOTARY_ISSUER_ID" --wait
  # Staple the ticket so Gatekeeper validates offline.
  xcrun stapler staple "$DMG"
  xcrun stapler validate "$DMG"
  echo "notarized + stapled: $DMG"
else
  echo "note: notary creds absent — built un-notarized .dmg (not Gatekeeper-clean): $DMG"
fi
echo "built: $DMG"
