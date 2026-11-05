#!/usr/bin/env bash
# Builds ede_<version>_<arch>.deb for Mint/Ubuntu into apps/ede/target-deb/.
#   apps/ede/packaging/build-deb.sh [--no-xwayland] [--dev]
# Build deps: cargo, dpkg-deb, and the headers in apps/ede/README.md (udev
# feature), plus the shell workspace's (gpui: libxkbcommon, wayland, vulkan
# loader, fontconfig, clang).
# It bundles the Xwayland from xwayland/build.sh (#NEW-120: 24.1+, explicit
# sync; built in ubuntu:24.04 via docker when this host has no meson), or a
# prebuilt one from EDE_XWAYLAND=<build.sh out>. --no-xwayland depends on the
# distro's xwayland instead (quick local builds). --dev packages the dev
# profile's binaries (a VM check in minutes; never a release).
# Contents (#EDE-2): ede-compositor, ede-session, ede-ui, ede-shell and
# ede-sealed (#EDE-17: lock, polkit, consent) in /usr/bin, the factory
# documents in /usr/share/ede/ui, the session's systemd units
# (ede-session.target starts ede-shell.service = `ede-ui supervise --
# ede-shell` and ede-polkit-agent.service = `ede-sealed polkit`), the login
# screen entries.
set -euo pipefail
bundle_xwayland=1
profile=release
for arg in "$@"; do
  case "$arg" in
    --no-xwayland) bundle_xwayland=0 ;;
    --dev) profile=debug ;;
    *) echo "usage: $0 [--no-xwayland] [--dev]" >&2; exit 2 ;;
  esac
done
here="$(cd "$(dirname "$0")" && pwd)"
ede="$(dirname "$here")"
cd "$ede"

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
arch="$(dpkg --print-architecture)"
target="${CARGO_TARGET_DIR:-$ede/target}"
# The shell is a second workspace with its own target directory (unless a
# shared CARGO_TARGET_DIR says otherwise).
shell_target="${CARGO_TARGET_DIR:-$ede/shell/target}"
out="$ede/target-deb"
stage="$(mktemp -d)"
chmod 755 "$stage"
trap 'rm -rf "$stage"' EXIT

release=(--release); [ "$profile" = debug ] && release=()
cargo build "${release[@]}" --locked -p ede-compositor -p ede-session -p ede-ui -p ede-services --features ede-compositor/udev
# The shell is its own workspace (gpui never enters the compositor's).
(cd "$ede/shell" && cargo build "${release[@]}" --locked -p ede-shell --bins)

for bin in ede-compositor ede-session ede-ui ede-services; do
  install -Dm755 "$target/$profile/$bin" "$stage/usr/bin/$bin"
done
install -Dm755 "$shell_target/$profile/ede-shell" "$stage/usr/bin/ede-shell"
# Tier B (#EDE-17): the lock screen, the polkit agent and consent.
install -Dm755 "$shell_target/$profile/ede-sealed" "$stage/usr/bin/ede-sealed"
for f in "$ede"/ui/*.json; do install -Dm644 "$f" "$stage/usr/share/ede/ui/$(basename "$f")"; done
for f in "$here"/wayland-sessions/*.desktop; do install -Dm644 "$f" "$stage/usr/share/wayland-sessions/$(basename "$f")"; done
for f in "$here"/systemd/*; do install -Dm644 "$f" "$stage/usr/lib/systemd/user/$(basename "$f")"; done
install -Dm644 "$ede/../../LICENSE" "$stage/usr/share/doc/ede/copyright"

# The compositor's, then the shell's (gpui on Wayland: Vulkan through the
# loader + a driver, fontconfig/freetype) and ede-ui's history (git).
depends="libc6, libseat1, libinput10, libgbm1, libudev1, libxkbcommon0, libpixman-1-0, libegl1, libgles2, libwayland-client0, libvulkan1, mesa-vulkan-drivers, libfontconfig1, libfreetype6, git"
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
Recommends: gnome-keyring
Description: eDE, the Exponential desktop
 A Wayland desktop session: the ede-compositor (floating windows with edge
 snapping, Xwayland 24.1+ with explicit sync) started by ede-session from the login
 screen as "Exponential", with crash recovery and a safe mode, and ede-shell, the
 panel drawn from genUI documents (/usr/share/ede/ui, a person's copies in
 ~/.config/ede/ui) under the ede-ui supervisor.
CONTROL

mkdir -p "$out"
deb="$out/ede_${version}_${arch}.deb"
dpkg-deb --root-owner-group --build "$stage" "$deb" >/dev/null
echo "$deb"
