#!/usr/bin/env bash
# EXP-1430: the TestFlight upload build-ios.yml ran inline, as a reusable step.
set -euo pipefail

[ -f "$IPA" ] || { echo "::error::testflight-upload: $IPA not found"; exit 1; }
for v in ASC_KEY_ID ASC_ISSUER_ID ASC_KEY_CONTENT; do
  [ -n "${!v}" ] || { echo "::error::testflight-upload: $v is empty"; exit 1; }
done

dir="$(mktemp -d "$RUNNER_TEMP/testflight.XXXXXX")"
trap 'rm -rf "$dir"' EXIT
umask 077

# The secret may be raw PEM or base64-wrapped PEM: accept both.
if [[ "$ASC_KEY_CONTENT" == *"BEGIN PRIVATE KEY"* ]]; then
  printf '%s' "$ASC_KEY_CONTENT" > "$dir/asc.p8"
else
  printf '%s' "$ASC_KEY_CONTENT" | base64 -d > "$dir/asc.p8"
fi
grep -q "BEGIN PRIVATE KEY" "$dir/asc.p8" || { echo "::error::testflight-upload: ASC key is not valid PEM"; exit 1; }

jq -n --arg key_id "$ASC_KEY_ID" --arg issuer_id "$ASC_ISSUER_ID" \
  --rawfile key "$dir/asc.p8" \
  '{key_id: $key_id, issuer_id: $issuer_id, key: $key, in_house: false}' \
  > "$dir/asc_key.json"

command -v fastlane >/dev/null || gem install fastlane --no-document

args=(--ipa "$IPA" --api_key_path "$dir/asc_key.json")
if [ "$DISTRIBUTE_EXTERNAL" = "true" ]; then
  [ -n "$TF_GROUPS" ] || { echo "::error::testflight-upload: distribute-external needs groups"; exit 1; }
  [ -n "$CHANGELOG" ] || { echo "::error::testflight-upload: distribute-external needs a changelog"; exit 1; }
  # pilot assigns external groups only after processing, so no skip_waiting.
  args+=(--distribute_external true --groups "$TF_GROUPS" --changelog "$CHANGELOG")
else
  args+=(--skip_waiting_for_build_processing true --distribute_external false)
fi

fastlane pilot upload "${args[@]}"
