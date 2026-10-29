#!/usr/bin/env bash
# EXP-1430: p12 -> unlocked temp keychain, the same commands build-ios.yml ran.
set -euo pipefail

[ -n "${P12_B64:-}" ] || { echo "::error::apple-signing: empty p12 secret"; exit 1; }

KEYCHAIN="$RUNNER_TEMP/$KEYCHAIN_NAME"
KEYCHAIN_PW="$(uuidgen)"
security create-keychain -p "$KEYCHAIN_PW" "$KEYCHAIN"
security set-keychain-settings -lut 21600 "$KEYCHAIN"
security unlock-keychain -p "$KEYCHAIN_PW" "$KEYCHAIN"
p12="$RUNNER_TEMP/signing-$$.p12"
trap 'rm -f "$p12"' EXIT
echo "$P12_B64" | base64 -d > "$p12"
security import "$p12" -k "$KEYCHAIN" \
  -P "$P12_PASSWORD" -A -t cert -f pkcs12
security set-key-partition-list -S apple-tool:,apple: -s -k "$KEYCHAIN_PW" "$KEYCHAIN" >/dev/null
security list-keychains -d user -s "$KEYCHAIN" login.keychain-db
ids="$(security find-identity -v -p codesigning "$KEYCHAIN")"
echo "$ids"

identity="$(awk '/"Apple Distribution/ { print $2; exit }' <<<"$ids")"
[ -n "$identity" ] || identity="$(awk '/^ *[0-9]+\)/ { print $2; exit }' <<<"$ids")"
[ -n "$identity" ] || { echo "::error::apple-signing: no valid codesigning identity in the p12"; exit 1; }

echo "keychain-path=$KEYCHAIN" >> "$GITHUB_OUTPUT"
echo "identity=$identity" >> "$GITHUB_OUTPUT"
