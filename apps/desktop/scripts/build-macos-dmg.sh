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

# EXP-1430: the .dmg + notarize + staple steps live in the shared
# .github/actions/macos-app/dmg.sh (NOTARY_* + MACOS_SIGN_IDENTITY pass through).
REPO_ROOT="$(cd "${DESKTOP_DIR}/../.." && pwd)"
exec "${REPO_ROOT}/.github/actions/macos-app/dmg.sh" \
  --app "$APP_DIR" \
  --dmg "${TARGET_DIR}/Exponential-${CHANNEL}.dmg" \
  --volname "$APP_NAME"
