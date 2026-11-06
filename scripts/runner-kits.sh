#!/usr/bin/env bash
# #VAPP-270 (E7): build the prebuilt runner kits `exponential bundle --runners <dir>`
# reads, from the workspace (what the `eengine-v*` release publishes as
# eengine-runner-kit-<target>.tar.gz, packages/eengine/fixtures/bundle.json `kits`).
#
#   scripts/runner-kits.sh --out <dir> <target>...   # targets: macos windows linux android ios web
#
# Each target's recipe is scripts/runner-kits/<target>.sh <kit dir>: it fills
# <dir>/<target>/ with the runner and its NOTICES.txt.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
out=""
targets=()
while [ $# -gt 0 ]; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    *) targets+=("$1"); shift ;;
  esac
done
[ -n "$out" ] && [ ${#targets[@]} -gt 0 ] || { echo "usage: $0 --out <dir> <target>..." >&2; exit 2; }
for t in "${targets[@]}"; do
  recipe="$here/runner-kits/$t.sh"
  [ -f "$recipe" ] || { echo "no recipe for target $t ($recipe)" >&2; exit 2; }
  rm -rf "$out/$t" && mkdir -p "$out/$t"
  bash "$recipe" "$(cd "$out/$t" && pwd)"
  [ -f "$out/$t/NOTICES.txt" ] || { echo "$t: the kit has no NOTICES.txt" >&2; exit 1; }
  echo "kit $t: $out/$t"
done
