#!/usr/bin/env bash
# Builds ede_<version>_<arch>.deb for Mint/Ubuntu into apps/ede/target-deb/.
#   apps/ede/packaging/build-deb.sh
# Build deps: cargo, dpkg-deb, and the headers in apps/ede/README.md (udev feature).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
ede="$(dirname "$here")"
cd "$ede"

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
arch="$(dpkg --print-architecture)"
target="${CARGO_TARGET_DIR:-$ede/target}"
out="$ede/target-deb"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT

cargo build --release --locked -p ede-compositor -p ede-session --features ede-compositor/udev

install -Dm755 "$target/release/ede-compositor" "$stage/usr/bin/ede-compositor"
install -Dm755 "$target/release/ede-session" "$stage/usr/bin/ede-session"
install -Dm755 "$here/ede-polkit-agent" "$stage/usr/lib/ede/ede-polkit-agent"
for f in "$here"/wayland-sessions/*.desktop; do install -Dm644 "$f" "$stage/usr/share/wayland-sessions/$(basename "$f")"; done
for f in "$here"/systemd/*; do install -Dm644 "$f" "$stage/usr/lib/systemd/user/$(basename "$f")"; done
install -Dm644 "$ede/../../LICENSE" "$stage/usr/share/doc/ede/copyright"

mkdir -p "$stage/DEBIAN"
cat > "$stage/DEBIAN/control" <<CONTROL
Package: ede
Version: $version
Architecture: $arch
Maintainer: Exponential <hello@exponential.at>
Section: x11
Priority: optional
Homepage: https://exponential.at
Depends: libc6, libseat1, libinput10, libgbm1, libudev1, libxkbcommon0, libpixman-1-0, libegl1, libgles2, xwayland, dbus-user-session | dbus-x11, systemd, libglib2.0-bin
Recommends: policykit-1-gnome | mate-polkit, gnome-keyring, swaylock
Description: eDE, the Exponential desktop
 A Wayland desktop session: the ede-compositor (floating windows with edge
 snapping, Xwayland, explicit sync) started by ede-session from the login
 screen as "Exponential", with crash recovery and a safe mode.
CONTROL

mkdir -p "$out"
deb="$out/ede_${version}_${arch}.deb"
dpkg-deb --root-owner-group --build "$stage" "$deb" >/dev/null
echo "$deb"
