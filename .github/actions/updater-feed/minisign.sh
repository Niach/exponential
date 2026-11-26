#!/usr/bin/env bash
# EXP-1430: sourced by feed.sh and apps/desktop/scripts/sign-binaries.sh.
# `fetch_minisign <dir>` sets `minisign` to a minisign binary: the pinned
# upstream release in <dir>, else the distro's. `load_minisign_key <value>
# <file>` writes a secret key given raw or as base64 of the file.

# minisign is pinned to an upstream release tarball (static x86_64 + aarch64
# Linux binaries) by sha256 — apt's minisign is 0.9 on 22.04 and writes
# legacy (non-prehashed) signatures the clients refuse.
MINISIGN_VERSION=0.12
MINISIGN_SHA256=9a599b48ba6eb7b1e80f12f36b94ceca7c00b7a5173c95c3efc88d9822957e73

fetch_minisign() {
  local work="$1" arch
  [ "$(uname -s)" = Linux ] || { echo "::error::minisign signs on Linux runners only" >&2; return 1; }
  case "$(uname -m)" in
    x86_64 | amd64) arch=x86_64 ;;
    aarch64 | arm64) arch=aarch64 ;;
    *) echo "::error::no minisign binary for $(uname -m)" >&2; return 1 ;;
  esac
  # The pinned release, checked against its digest; when github.com's release
  # download is down (the 2026-11-08 504s), the distro's minisign instead (same
  # key and signature format; `-H` signs prehashed on every version since 0.8).
  if curl -fsSL --retry 3 -o "$work/minisign.tgz" \
    "https://github.com/jedisct1/minisign/releases/download/$MINISIGN_VERSION/minisign-$MINISIGN_VERSION-linux.tar.gz" &&
    echo "$MINISIGN_SHA256  $work/minisign.tgz" | sha256sum -c --quiet -; then
    tar --warning=no-unknown-keyword -xzf "$work/minisign.tgz" -C "$work"
    minisign="$work/minisign-linux/$arch/minisign"
  else
    echo "::warning::the minisign release download failed; using the distro's minisign"
    if ! command -v minisign >/dev/null; then
      # The shared mirror fallback (#2668) when the checkout carries it.
      local apt_mirror
      apt_mirror="$(dirname "${BASH_SOURCE[0]}")/../apt-mirror/apt-mirror.sh"
      if [ -f "$apt_mirror" ]; then bash "$apt_mirror" >/dev/null; else sudo apt-get update -qq; fi
      sudo apt-get install -y -qq minisign >/dev/null
    fi
    minisign="$(command -v minisign)"
  fi
}

load_minisign_key() {
  local value="$1" file="$2"
  (
    umask 077
    if printf '%s' "$value" | head -n1 | grep -q '^untrusted comment:'; then
      printf '%s\n' "$value" > "$file"
    else
      printf '%s' "$value" | tr -d ' \r\n' | base64 -d > "$file"
    fi
  )
}
