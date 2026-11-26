#!/usr/bin/env bash
# EXP-1430: the updater-feed action body (action.yml passes FEED_* env).
set -euo pipefail

SUMS=SHA256SUMS.txt
SIG=SHA256SUMS.txt.minisig
# shellcheck source=minisign.sh
source "$(dirname "$0")/minisign.sh"

cd "$FEED_DIR"

# 1. The sums: sha256sum format, `<hex>  <name>`, sorted by name.
if [[ "$FEED_APPEND" =~ [^[:space:]] ]]; then
  [ -f "$SUMS" ] || { echo "::error::append needs an existing $FEED_DIR/$SUMS" >&2; exit 1; }
  for name in $FEED_APPEND; do
    [ -f "$name" ] || { echo "::error::append: no $FEED_DIR/$name" >&2; exit 1; }
    # Drop a previous line for the same name (a re-run), then add the fresh one.
    awk -v n="$name" '{ f = $2; sub(/^\*/, "", f) } f != n' "$SUMS" > "$SUMS.tmp"
    sha256sum "$name" >> "$SUMS.tmp"
    mv "$SUMS.tmp" "$SUMS"
  done
else
  files=()
  for f in *; do
    [ -f "$f" ] && [ "$f" != "$SUMS" ] && [ "$f" != "$SIG" ] && files+=("$f")
  done
  [ ${#files[@]} -gt 0 ] || { echo "::error::$FEED_DIR has no files to sum" >&2; exit 1; }
  sha256sum "${files[@]}" > "$SUMS"
fi
cat "$SUMS"
rm -f "$SIG"

# 2. The signature.
if [ -z "$FEED_SIGNING_KEY" ]; then
  if [ -n "$FEED_PUBLIC_KEY" ]; then
    echo "::error::a feed public key is pinned but no signing key was given — every pinned client would refuse this release" >&2
    exit 1
  fi
  echo "::warning::no updater signing key — $SUMS ships unsigned"
  echo "signed=false" >> "$GITHUB_OUTPUT"
  exit 0
fi

work="$(mktemp -d "${RUNNER_TEMP:-/tmp}/updater-feed.XXXXXX")"
trap 'rm -rf "$work"' EXIT
fetch_minisign "$work"
load_minisign_key "$FEED_SIGNING_KEY" "$work/feed.key"

comment="$FEED_COMMENT"
[ -n "$comment" ] || comment="repo:${GITHUB_REPOSITORY:-} ref:${GITHUB_REF_NAME:-} timestamp:$(date -u +%s)"
"$minisign" -S -s "$work/feed.key" -m "$SUMS" -x "$SIG" -t "$comment" </dev/null

# A secret that does not match the pinned public key never ships.
if [ -n "$FEED_PUBLIC_KEY" ]; then
  "$minisign" -V -H -q -P "$FEED_PUBLIC_KEY" -m "$SUMS" -x "$SIG"
  echo "signature verified against the pinned public key"
fi
cat "$SIG"
echo "signed=true" >> "$GITHUB_OUTPUT"
