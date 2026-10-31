#!/usr/bin/env bash
# Builds the Xwayland eDE ships (#NEW-120): noble's 23.2.6 has no explicit
# sync, so X11 games flicker on NVIDIA. Pinned 24.1.x, meson, linked against
# Ubuntu 24.04's system libs, staged as <out>/usr/lib/ede/bin/Xwayland plus
# <out>/DEPENDS (the .deb's Depends for it, from dpkg-shlibdeps).
#   apps/ede/packaging/xwayland/build.sh [--install-deps] [--docker] <out>
#   --install-deps  apt-get the build deps (root or sudo; CI)
#   --docker        build inside ubuntu:24.04 (a host without meson/headers)
# The tarballs are cached in ${XWAYLAND_CACHE:-~/.cache/ede-xwayland}; an <out>
# built by this exact script is reused as is (<out>/STAMP).
set -euo pipefail

# sha256s from the xorg-announce release mails.
VERSION=24.1.14
SHA256=4eb6b98d678299a4b96138d886345b294130695760345ef84596225a8224078a
# Build-time headers only: 24.1 needs presentproto 1.4, noble has 1.3.
PROTO_VERSION=2024.1
PROTO_SHA256=372225fd40815b8423547f5d890c5debc72e88b91088fbfb13158c20495ccb59
ARCHIVE=https://xorg.freedesktop.org/archive/individual
PREFIX=/usr/lib/ede
BUILD_DEPS=(
  ca-certificates curl xz-utils build-essential meson ninja-build pkg-config dpkg-dev
  x11proto-dev xtrans-dev libxkbfile-dev libxfont-dev libxcvt-dev libxshmfence-dev
  libxau-dev libxdmcp-dev libpixman-1-dev libdrm-dev libgbm-dev libepoxy-dev
  libegl-dev libgl-dev mesa-common-dev libwayland-dev wayland-protocols
  nettle-dev libbsd-dev libudev-dev
)

install_deps=0
docker=0
while [ $# -gt 1 ]; do
  case "$1" in
    --install-deps) install_deps=1 ;;
    --docker) docker=1 ;;
    *) echo "unknown flag $1" >&2; exit 2 ;;
  esac
  shift
done
[ $# -eq 1 ] || { echo "usage: $0 [--install-deps] [--docker] <out>" >&2; exit 2; }
mkdir -p "$1"
out="$(cd "$1" && pwd)"
here="$(cd "$(dirname "$0")" && pwd)"
cache="${XWAYLAND_CACHE:-$HOME/.cache/ede-xwayland}"
stamp="$(sha256sum "$here/build.sh" | cut -d' ' -f1)"
if [ "$(cat "$out/STAMP" 2>/dev/null)" = "$stamp" ] && [ -x "$out$PREFIX/bin/Xwayland" ]; then
  echo "$out$PREFIX/bin/Xwayland (cached)"
  exit 0
fi
mkdir -p "$cache"

# fetch <url> <sha256>: into the cache once, verified every time.
fetch() {
  local file="$cache/$(basename "$1")"
  if ! echo "$2  $file" | sha256sum -c --status 2>/dev/null; then
    curl -fsSL "$1" -o "$file.part"
    echo "$2  $file.part" | sha256sum -c --quiet || { rm -f "$file.part"; echo "$(basename "$1"): sha256 mismatch" >&2; exit 1; }
    mv "$file.part" "$file"
  fi
}
fetch "$ARCHIVE/xserver/xwayland-$VERSION.tar.xz" "$SHA256"
fetch "$ARCHIVE/proto/xorgproto-$PROTO_VERSION.tar.xz" "$PROTO_SHA256"

if [ "$docker" = 1 ]; then
  exec docker run --rm -v "$here:/src:ro" -v "$cache:/cache" -v "$out:/out" -e XWAYLAND_CACHE=/cache \
    ubuntu:24.04 bash -c "/src/build.sh --install-deps /out && chown -R $(id -u):$(id -g) /out"
fi

if [ "$install_deps" = 1 ]; then
  sudo=""; [ "$(id -u)" = 0 ] || sudo=sudo
  $sudo apt-get update -q
  DEBIAN_FRONTEND=noninteractive $sudo apt-get install -y -q --no-install-recommends "${BUILD_DEPS[@]}"
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
tar -xf "$cache/xorgproto-$PROTO_VERSION.tar.xz" -C "$work"
meson setup "$work/proto-build" "$work/xorgproto-$PROTO_VERSION" --prefix="$work/proto"
ninja -C "$work/proto-build" install
export PKG_CONFIG_PATH="$work/proto/share/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"

tar -xf "$cache/xwayland-$VERSION.tar.xz" -C "$work"
src="$work/xwayland-$VERSION"

# Paths match noble's xwayland so it finds the system's keymaps and xkbcomp.
meson setup "$work/build" "$src" --buildtype=release --prefix="$PREFIX" --libdir=lib \
  -Dxkb_dir=/usr/share/X11/xkb -Dxkb_output_dir=/var/lib/xkb -Dxkb_bin_dir=/usr/bin \
  -Ddefault_font_path=/usr/share/fonts/X11/misc,built-ins \
  -Dglamor=true -Ddrm=true -Dglx=true \
  -Dlibdecor=false -Dxwayland_ei=false -Dxvfb=false -Dsecure-rpc=false -Ddocs=false -Ddocs-pdf=false
ninja -C "$work/build"
DESTDIR="$work/stage" ninja -C "$work/build" install

rm -rf "$out/usr" "$out/STAMP"
install -Dm755 "$work/stage$PREFIX/bin/Xwayland" "$out$PREFIX/bin/Xwayland"
strip "$out$PREFIX/bin/Xwayland"
install -Dm644 "$src/COPYING" "$out/usr/share/doc/ede/xwayland-copyright"

# dpkg-shlibdeps wants a debian/control next to it.
mkdir -p "$work/shlibs/debian"
printf 'Source: ede\n\nPackage: ede\nArchitecture: any\n' > "$work/shlibs/debian/control"
deps="$(cd "$work/shlibs" && dpkg-shlibdeps -O "$out$PREFIX/bin/Xwayland" | sed -n 's/^shlibs:Depends=//p')"
# xkbcomp + the keymaps (noble's xwayland pulls them in via xserver-common);
# libEGL is dlopened by epoxy, so shlibdeps can't see it.
echo "$deps, x11-xkb-utils, xkb-data, libegl1" > "$out/DEPENDS"
echo "$stamp" > "$out/STAMP"
"$out$PREFIX/bin/Xwayland" -version 2>&1 | head -1
echo "$out$PREFIX/bin/Xwayland"
