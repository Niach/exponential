#!/usr/bin/env bash
#
# Sign the executables an AppDir ships, for eDE (exponential.at's desktop):
#
#   scripts/sign-binaries.sh <AppDir> <channel>
#
# Writes <AppDir>/usr/share/exponential/binaries.sha256 (sha256sum lines,
# `<hex>  usr/bin/exp-desktop`, one per ELF file under usr/: the app and the
# libraries linuxdeploy bundled) and its minisign signature
# binaries.sha256.minisig, made with the self-updater's key (EXP-1430).
# eDE's compositor pins the public key: a process whose executable and every
# library it maps verify against a signed list is the Exponential app, and
# gets computer use at login without the "Allow Exponential to use your
# screen?" prompt. Anything else is still asked.
#
# The trusted comment starts with `exponential-binaries `, so the release
# feed's SHA256SUMS.txt (signed with the same key) never passes for this list.
#
# Env: BINARIES_SIGNING_KEY (the minisign secret key, raw or base64 of the
# file) and BINARIES_PUBLIC_KEY (the second line of the .pub). No key = no
# list (a warning; eDE then asks, as for any older app); a public key
# without the secret fails, as the release feed does.
set -euo pipefail

APPDIR="${1:?usage: sign-binaries.sh <AppDir> <channel>}"
CHANNEL="${2:?usage: sign-binaries.sh <AppDir> <channel>}"
LIST=usr/share/exponential/binaries.sha256
SIG="$LIST.minisig"
SIGNING_KEY="${BINARIES_SIGNING_KEY:-}"
PUBLIC_KEY="${BINARIES_PUBLIC_KEY:-}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"

cd "$APPDIR"
rm -f "$LIST" "$SIG"

if [ -z "$SIGNING_KEY" ]; then
  if [ -n "$PUBLIC_KEY" ]; then
    echo "::error::a public key is pinned but no signing key was given: the AppImage's binaries would ship unsigned" >&2
    exit 1
  fi
  echo "::warning::no signing key: the AppImage ships no signed binaries list (eDE asks before it lets the app use the screen)"
  exit 0
fi

mkdir -p "$(dirname "$LIST")"
# Every ELF file under usr/, sorted by path.
find usr -type f -print0 | sort -z | while IFS= read -r -d '' file; do
  if [ "$(head -c 4 "$file" | od -An -tx1 | tr -d ' \n')" = 7f454c46 ]; then
    sha256sum "$file"
  fi
done > "$LIST"
grep -q '  usr/bin/exp-desktop$' "$LIST" || { echo "::error::$APPDIR has no usr/bin/exp-desktop to sign" >&2; exit 1; }
cat "$LIST"

# shellcheck source=../../../.github/actions/updater-feed/minisign.sh
source "$REPO_ROOT/.github/actions/updater-feed/minisign.sh"
work="$(mktemp -d "${RUNNER_TEMP:-/tmp}/sign-binaries.XXXXXX")"
trap 'rm -rf "$work"' EXIT
fetch_minisign "$work"
load_minisign_key "$SIGNING_KEY" "$work/binaries.key"

"$minisign" -S -s "$work/binaries.key" -m "$LIST" -x "$SIG" \
  -t "exponential-binaries v1 channel:$CHANNEL ref:${GITHUB_REF_NAME:-local} timestamp:$(date -u +%s)" </dev/null
if [ -n "$PUBLIC_KEY" ]; then
  "$minisign" -V -H -q -P "$PUBLIC_KEY" -m "$LIST" -x "$SIG"
  echo "the binaries list verifies against the pinned public key"
fi
cat "$SIG"
