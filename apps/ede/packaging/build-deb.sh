#!/usr/bin/env bash
# Builds ede_<version>_<arch>.deb for Mint/Ubuntu into apps/ede/target-deb/.
#   apps/ede/packaging/build-deb.sh [--no-xwayland]
# Build deps: cargo, dpkg-deb, and the headers in apps/ede/README.md (udev feature).
# It bundles the Xwayland from xwayland/build.sh (#NEW-120: 24.1+, explicit
# sync; built in ubuntu:24.04 via docker when this host has no meson), or a
# prebuilt one from EDE_XWAYLAND=<build.sh out>. --no-xwayland depends on the
# distro's xwayland instead (quick local builds).
set -euo pipefail
bundle_xwayland=1
case "${1:-}" in
  --no-xwayland) bundle_xwayland=0 ;;
  "") ;;
  *) echo "usage: $0 [--no-xwayland]" >&2; exit 2 ;;
esac
here="$(cd "$(dirname "$0")" && pwd)"
ede="$(dirname "$here")"
cd "$ede"

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
arch="$(dpkg --print-architecture)"
target="${CARGO_TARGET_DIR:-$ede/target}"
out="$ede/target-deb"
stage="$(mktemp -d)"
chmod 755 "$stage"
trap 'rm -rf "$stage"' EXIT

cargo build --release --locked -p ede-compositor -p ede-session --features ede-compositor/udev

install -Dm755 "$target/release/ede-compositor" "$stage/usr/bin/ede-compositor"
install -Dm755 "$target/release/ede-session" "$stage/usr/bin/ede-session"
install -Dm755 "$here/ede-polkit-agent" "$stage/usr/lib/ede/ede-polkit-agent"
for f in "$here"/wayland-sessions/*.desktop; do install -Dm644 "$f" "$stage/usr/share/wayland-sessions/$(basename "$f")"; done
for f in "$here"/systemd/*; do install -Dm644 "$f" "$stage/usr/lib/systemd/user/$(basename "$f")"; done
install -Dm644 "$ede/../../LICENSE" "$stage/usr/share/doc/ede/copyright"

depends="libc6, libseat1, libinput10, libgbm1, libudev1, libxkbcommon0, libpixman-1-0, libegl1, libgles2"
if [ "$bundle_xwayland" = 1 ]; then
  xwl="${EDE_XWAYLAND:-$target/ede-xwayland}"
  if [ -z "${EDE_XWAYLAND:-}" ]; then
    docker=(); command -v meson >/dev/null || docker=(--docker)
    "$here/xwayland/build.sh" "${docker[@]}" "$xwl" >&2
  fi
  cp -a "$xwl/usr" "$stage/"
  # One entry per package: the versioned one wins.
  depends="$(printf '%s, %s' "$depends" "$(cat "$xwl/DEPENDS")" | tr ',' '\n' | sed 's/^ *//' | awk '
    { n = $1 } !(n in seen) { seen[n] = $0; order[++k] = n; next } index($0, "(") && !index(seen[n], "(") { seen[n] = $0 }
    END { for (i = 1; i <= k; i++) printf "%s%s", (i > 1 ? ", " : ""), seen[order[i]] }')"
else
  depends="$depends, xwayland"
fi

mkdir -p "$stage/DEBIAN"
cat > "$stage/DEBIAN/control" <<CONTROL
Package: ede
Version: $version
Architecture: $arch
Maintainer: Exponential <hello@exponential.at>
Section: x11
Priority: optional
Homepage: https://exponential.at
Depends: $depends, dbus-user-session | dbus-x11, systemd, libglib2.0-bin
Recommends: policykit-1-gnome | mate-polkit, gnome-keyring, swaylock
Description: eDE, the Exponential desktop
 A Wayland desktop session: the ede-compositor (floating windows with edge
 snapping, Xwayland 24.1+ with explicit sync) started by ede-session from the login
 screen as "Exponential", with crash recovery and a safe mode.
CONTROL

mkdir -p "$out"
deb="$out/ede_${version}_${arch}.deb"
dpkg-deb --root-owner-group --build "$stage" "$deb" >/dev/null
echo "$deb"
