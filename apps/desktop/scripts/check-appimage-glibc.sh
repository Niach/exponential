#!/usr/bin/env bash
#
# Fail when any ELF file inside an AppImage needs a glibc newer than the floor.
#
#   scripts/check-appimage-glibc.sh <file.AppImage> [max-glibc]   # default 2.35
#
# The AppImage must start on Ubuntu 22.04 (glibc 2.35) and pass the AppImage
# catalog's test (appimage.github.io), so the release builds on ubuntu-22.04
# (build-desktop.yml). This is the guard: a runner bump, or a bundled library
# from a newer distro, fails the release here instead of on a user's machine.
# Weak references count too (Rust's std links pidfd_spawnp weakly on a 2.39
# build host): the catalog's check reads the symbol versions, not the weakness.
# Needs objdump (binutils) and file.
set -euo pipefail

APPIMAGE="$(readlink -f "${1:?usage: check-appimage-glibc.sh <file.AppImage> [max-glibc]}")"
MAX="${2:-2.35}"

work="$(mktemp -d "${RUNNER_TEMP:-/tmp}/appimage-glibc.XXXXXX")"
trap 'rm -rf "$work"' EXIT
(cd "$work" && APPIMAGE_EXTRACT_AND_RUN=1 "$APPIMAGE" --appimage-extract >/dev/null)

bad=0
count=0
while IFS= read -r -d '' file; do
  file -b "$file" | grep -q '^ELF' || continue
  count=$((count + 1))
  newest="$(objdump -T "$file" 2>/dev/null | grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/^GLIBC_//' | sort -uV | tail -1)"
  rel="${file#"$work/squashfs-root/"}"
  if [ -z "$newest" ]; then
    echo "  $rel: no versioned glibc symbols"
    continue
  fi
  if [ "$(printf '%s\n%s\n' "$newest" "$MAX" | sort -V | tail -1)" != "$MAX" ]; then
    echo "::error::$rel needs GLIBC_$newest (floor $MAX):" >&2
    objdump -T "$file" | awk 'match($0, /GLIBC_[0-9.]+/) { print substr($0, RSTART + 6, RLENGTH - 6), $NF }' \
      | sort -uV | while read -r v sym; do
        [ "$(printf '%s\n%s\n' "$v" "$MAX" | sort -V | tail -1)" = "$MAX" ] || echo "    GLIBC_$v $sym" >&2
      done
    bad=1
  else
    echo "  $rel: GLIBC_$newest"
  fi
done < <(find "$work/squashfs-root" -type f -print0 | sort -z)

[ "$count" -gt 0 ] || { echo "::error::no ELF files found in $APPIMAGE" >&2; exit 1; }
[ "$bad" = 0 ] || exit 1
echo "all $count ELF files in $(basename "$APPIMAGE") stay at or below GLIBC_$MAX"
