#!/usr/bin/env bash
# The local .deb (a VM check, Mint by hand): stage.sh + pack-deb.sh for one
# suite into apps/ede/target-deb/. CI packs every distro (build-ede.yml).
#   apps/ede/packaging/build-deb.sh [--suite noble|resolute|trixie] [--dev]
# noble bundles the Xwayland from xwayland/build.sh (built in ubuntu:24.04
# via docker when this host has no meson; EDE_XWAYLAND=<build.sh out> to
# reuse one). --dev packages the dev profile (minutes; never a release).
set -euo pipefail
suite=noble
dev=()
while [ $# -gt 0 ]; do
  case "$1" in
    --suite) suite="$2"; shift ;;
    --dev) dev=(--dev) ;;
    *) echo "usage: $0 [--suite noble|resolute|trixie] [--dev]" >&2; exit 2 ;;
  esac
  shift
done
here="$(cd "$(dirname "$0")" && pwd)"
ede="$(dirname "$here")"
target="${CARGO_TARGET_DIR:-$ede/target}"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
"$here/stage.sh" "${dev[@]}" "$stage/root" >/dev/null
xwl=()
if [ "$suite" = noble ]; then
  dir="${EDE_XWAYLAND:-$target/ede-xwayland}"
  if [ -z "${EDE_XWAYLAND:-}" ]; then
    docker=(); command -v meson >/dev/null || docker=(--docker)
    "$here/xwayland/build.sh" "${docker[@]}" "$dir" >&2
  fi
  xwl=(--xwayland "$dir")
fi
"$here/pack-deb.sh" "$stage/root" "$ede/target-deb" "$suite" "${xwl[@]}"
