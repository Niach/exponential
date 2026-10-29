#!/usr/bin/env bash
# EXP-1430: play-upload preflight. Materializes the service-account key,
# refuses what Play would reject (unsigned bundle, production track), and
# turns fastlane changelogs into the whatsnew-<locale> files the upload wants.
set -euo pipefail

out() { echo "$1=$2" >> "$GITHUB_OUTPUT"; }
out upload false

if [ -z "${SA_JSON:-}" ]; then
  echo "::warning::play-upload: no Play service-account secret, skipping the upload of $AAB"
  exit 0
fi

case "$TRACK" in
  production) echo "::error::play-upload never publishes to production; promote in the Play Console (or fastlane production)"; exit 1 ;;
  '') echo "::error::play-upload: empty track"; exit 1 ;;
esac
case "$STATUS" in
  completed|draft) ;;
  *) echo "::error::play-upload: status must be completed or draft, got '$STATUS'"; exit 1 ;;
esac

[ -f "$AAB" ] || { echo "::error::play-upload: $AAB not found"; exit 1; }
# A jarsigner-signed bundle carries META-INF/<alias>.{RSA,DSA,EC}; Play rejects
# anything else, so fail here with a readable message.
if ! unzip -Z1 "$AAB" | grep -Eq '^META-INF/[^/]+\.(RSA|DSA|EC)$'; then
  echo "::error::play-upload: $AAB is unsigned"
  exit 1
fi

sa="$RUNNER_TEMP/play-sa-$$.json"
umask 077
if [[ "$SA_JSON" =~ ^[[:space:]]*\{ ]]; then
  printf '%s' "$SA_JSON" > "$sa"
else
  printf '%s' "$SA_JSON" | base64 -d > "$sa"
fi
grep -q '"private_key"' "$sa" || { rm -f "$sa"; echo "::error::play-upload: the service-account secret is not a key JSON (raw or base64)"; exit 1; }
out sa-path "$sa"

if [ -n "${CHANGELOG_DIR:-}" ]; then
  [ -n "${VERSION_CODE:-}" ] || { echo "::error::play-upload: changelog-dir needs version-code"; exit 1; }
  wn="$RUNNER_TEMP/play-whatsnew-$$"
  mkdir -p "$wn"
  for f in "$CHANGELOG_DIR"/*/changelogs/"$VERSION_CODE".txt; do
    [ -f "$f" ] || continue
    locale="$(basename "$(dirname "$(dirname "$f")")")"
    # Play caps release notes at 500 characters per locale.
    n="$(LC_ALL=C.UTF-8 wc -m < "$f" | tr -d ' ')"
    [ "$n" -le 500 ] || { echo "::error::play-upload: $f is $n characters, Play allows 500"; exit 1; }
    cp "$f" "$wn/whatsnew-$locale"
  done
  if [ -n "$(ls -A "$wn")" ]; then
    echo "play-upload: release notes for $(ls "$wn" | sed 's/^whatsnew-//' | tr '\n' ' ')"
    out whatsnew-dir "$wn"
  else
    echo "::warning::play-upload: no changelogs/$VERSION_CODE.txt under $CHANGELOG_DIR, uploading without release notes"
  fi
fi

if [ -n "${MAPPING:-}" ]; then
  [ -f "$MAPPING" ] || { echo "::error::play-upload: mapping file $MAPPING not found"; exit 1; }
  out mapping "$MAPPING"
fi

out upload true
