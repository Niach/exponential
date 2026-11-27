#!/usr/bin/env bash
#
# PipeWire 1.0.5 HEADERS for a build on Ubuntu 22.04 (the glibc 2.35 floor).
#
#   eval "$(scripts/pipewire-headers.sh <dir>)"   # exports PKG_CONFIG_PATH
#
# The AppImage builds on ubuntu-22.04 so its glibc floor is 2.35
# (build-desktop.yml). pipewire-rs 0.9 (libspa/pipewire-sys) needs SPA headers
# newer than 22.04's 0.3.48 (spa_video_info_raw.flags, an i64 modifier), so
# this unpacks the headers of Ubuntu 24.04's release packages (pinned by
# sha256) and writes .pc files pointing at them. Only the headers: the libdir
# stays the system's, so the binary still LINKS against 22.04's
# libpipewire-0.3.so (0.3.48). A pw_* function newer than 0.3.48 is a link
# error here, never a missing symbol on a user's machine, and the AppImage's
# libpipewire fallback (build-appimage.sh) stays 22.04's copy. The SPA
# helpers are static inline, compiled into the binary.
set -euo pipefail

DIR="$(mkdir -p "${1:?usage: pipewire-headers.sh <dir>}" && cd "$1" && pwd)"
VERSION=1.0.5-1
POOL=http://archive.ubuntu.com/ubuntu/pool/main/p/pipewire
declare -A SHA=(
  [libpipewire-0.3-dev]=c0b34ed6a34391a94c0711966dbea9b6b5a98758d88813824c05d6662e0b60c9
  [libspa-0.2-dev]=6c99d162e23e13cacf8bc3edc312c60545281078398e52e34dc5ebb4209649dc
)
LIBDIR="$(pkg-config --variable=libdir libpipewire-0.3 2>/dev/null || true)"
[ -n "$LIBDIR" ] && [ -e "$LIBDIR/libpipewire-0.3.so" ] || {
  echo "pipewire-headers.sh: install the system libpipewire-0.3-dev first (its .so is what links)" >&2
  exit 1
}

for pkg in "${!SHA[@]}"; do
  deb="$DIR/${pkg}_${VERSION}_amd64.deb"
  [ -f "$deb" ] || curl -fsSL --retry 5 -o "$deb" "$POOL/${pkg}_${VERSION}_amd64.deb"
  echo "${SHA[$pkg]}  $deb" | sha256sum -c --quiet - >&2
  rm -rf "$DIR/$pkg" && mkdir -p "$DIR/$pkg"
  dpkg-deb -x "$deb" "$DIR/$pkg"
done

mkdir -p "$DIR/pkgconfig"
cat > "$DIR/pkgconfig/libspa-0.2.pc" <<EOF
includedir=$DIR/libspa-0.2-dev/usr/include

Name: libspa
Description: Simple Plugin API (1.0.5 headers)
Version: 0.2
Cflags: -I\${includedir}/spa-0.2 -D_REENTRANT
EOF
cat > "$DIR/pkgconfig/libpipewire-0.3.pc" <<EOF
includedir=$DIR/libpipewire-0.3-dev/usr/include
libdir=$LIBDIR

Name: libpipewire
Description: PipeWire Interface (1.0.5 headers, the system library)
Version: 1.0.5
Requires: libspa-0.2
Libs: -L\${libdir} -lpipewire-0.3
Cflags: -I\${includedir}/pipewire-0.3 -D_REENTRANT
EOF
echo "export PKG_CONFIG_PATH=$DIR/pkgconfig\${PKG_CONFIG_PATH:+:\$PKG_CONFIG_PATH}"
