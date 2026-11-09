#!/usr/bin/env bash
# #LOGS-2: moves the DWARF out of the staged binaries for GlitchTip's
# symbolication: apps/ede/packaging/split-debug.sh <stage> <out>
# Each ELF in <stage>/usr/bin and the portal backend gets <out>/<name>.debug
# (objcopy --only-keep-debug, the GNU build-id note kept), then loses its
# debug sections in place (--strip-debug, a .gnu_debuglink to the file), so
# what ships is what a release without debug info ships (cargo's default
# strip = "debuginfo"). build-ede.yml builds with
# CARGO_PROFILE_RELEASE_DEBUG=line-tables-only and uploads <out>.
set -euo pipefail
[ $# -eq 2 ] || { echo "usage: $0 <stage> <out>" >&2; exit 2; }
stage="$1"
out="$2"
mkdir -p "$out"
for bin in "$stage"/usr/bin/* "$stage"/usr/libexec/xdg-desktop-portal-exponential; do
  [ -f "$bin" ] || continue
  file -b "$bin" | grep -q '^ELF' || continue
  name="$(basename "$bin")"
  objcopy --only-keep-debug "$bin" "$out/$name.debug"
  objcopy --strip-debug --add-gnu-debuglink="$out/$name.debug" "$bin"
  echo "$name: $(stat -c %s "$bin") bytes shipped, $(stat -c %s "$out/$name.debug") bytes of debug info"
done
