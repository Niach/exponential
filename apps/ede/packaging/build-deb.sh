#!/usr/bin/env bash
# Builds exponential-desktop_<version>_<arch>.deb for Mint/Ubuntu into apps/ede/target-deb/.
#   apps/ede/packaging/build-deb.sh [--no-xwayland] [--dev]
# Build deps: cargo, dpkg-deb, and the headers in apps/ede/README.md (udev
# feature), plus the shell workspace's (gpui: libxkbcommon, wayland, vulkan
# loader, fontconfig, clang).
# It bundles the Xwayland from xwayland/build.sh (#NEW-120: 24.1+, explicit
# sync; built in ubuntu:24.04 via docker when this host has no meson), or a
# prebuilt one from EDE_XWAYLAND=<build.sh out>. --no-xwayland depends on the
# distro's xwayland instead (quick local builds). --dev packages the dev
# profile's binaries (a VM check in minutes; never a release).
# Contents (#EDE-2): exponential-compositor, exponential-session, exponential-ui, exponential-shell and
# exponential-sealed (#EDE-17: lock, polkit, consent), exponential-services (#EDE-13),
# xdg-desktop-portal-exponential (#EDE-15) and exponential-theme (#EDE-19) in /usr/bin, the factory
# documents in /usr/share/exponential-desktop/ui, the session's systemd units
# (exponential-session.target starts exponential-shell.service = `exponential-ui supervise --
# exponential-shell` and exponential-polkit-agent.service = `exponential-sealed polkit`), the login
# screen entries (GDM and LightDM read /usr/share/wayland-sessions) and the
# lock screen's PAM service (/etc/pam.d/exponential-desktop).
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
cargo build "${release[@]}" --locked -p ede-compositor -p ede-session -p ede-ui -p ede-services -p ede-portal -p ede-theme --features ede-compositor/udev
# The shell is its own workspace (gpui never enters the compositor's).
(cd "$ede/shell" && cargo build "${release[@]}" --locked -p ede-shell --bins)

for bin in exponential-compositor exponential-session exponential-ui exponential-services exponential-theme; do
  install -Dm755 "$target/$profile/$bin" "$stage/usr/bin/$bin"
done
install -Dm755 "$shell_target/$profile/exponential-shell" "$stage/usr/bin/exponential-shell"
# The portal backend (#EDE-15): ScreenCast, Screenshot, RemoteDesktop,
# GlobalShortcuts, Settings; the frontend routes them here (exponential-portals.conf).
install -Dm755 "$target/$profile/xdg-desktop-portal-exponential" "$stage/usr/libexec/xdg-desktop-portal-exponential"
install -Dm644 "$here/portal/exponential.portal" "$stage/usr/share/xdg-desktop-portal/portals/exponential.portal"
install -Dm644 "$here/portal/exponential-portals.conf" "$stage/usr/share/xdg-desktop-portal/exponential-portals.conf"
install -Dm644 "$here/portal/org.freedesktop.impl.portal.desktop.exponential.service" "$stage/usr/share/dbus-1/services/org.freedesktop.impl.portal.desktop.exponential.service"
install -Dm644 "$here/portal/xdg-desktop-portal-exponential.service" "$stage/usr/lib/systemd/user/xdg-desktop-portal-exponential.service"
# Tier B (#EDE-17): the lock screen, the polkit agent and consent.
install -Dm755 "$shell_target/$profile/exponential-sealed" "$stage/usr/bin/exponential-sealed"
for f in "$ede"/ui/*.json; do install -Dm644 "$f" "$stage/usr/share/exponential-desktop/ui/$(basename "$f")"; done
for f in "$here"/wayland-sessions/*.desktop; do install -Dm644 "$f" "$stage/usr/share/wayland-sessions/$(basename "$f")"; done
for f in "$here"/systemd/*; do install -Dm644 "$f" "$stage/usr/lib/systemd/user/$(basename "$f")"; done
# The lock screen's PAM service (#EDE-17); a conffile, so a local edit survives upgrades.
install -Dm644 "$here/pam/exponential-desktop" "$stage/etc/pam.d/exponential-desktop"
install -Dm644 "$ede/../../LICENSE" "$stage/usr/share/doc/exponential-desktop/copyright"

# The compositor's, then the shell's (gpui on Wayland: Vulkan through the
# loader + a driver, fontconfig/freetype) and exponential-ui's history (git).
depends="libc6, libpipewire-0.3-0, libseat1, libinput10, libgbm1, libudev1, libxkbcommon0, libpixman-1-0, libegl1, libgles2, libwayland-client0, libvulkan1, mesa-vulkan-drivers, libfontconfig1, libfreetype6, git"
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
echo /etc/pam.d/exponential-desktop > "$stage/DEBIAN/conffiles"
# The upgrade from 0.1.x (the `ede` package, session `ede`): login screens that
# remember the last session (AccountsService for GDM and LightDM, LightDM's
# dmrc cache) keep picking it under its new name. The person's own ede
# directories move at their next login (exponential-session).
cat > "$stage/DEBIAN/postinst" <<'POSTINST'
#!/bin/sh
set -e
if [ "$1" = configure ]; then
  for f in /var/lib/AccountsService/users/* /var/cache/lightdm/dmrc/*.dmrc; do
    [ -f "$f" ] || continue
    sed -i -e 's/^\(X\{0,1\}Session\)=ede$/\1=exponential/' -e 's/^\(X\{0,1\}Session\)=ede-safe$/\1=exponential-safe/' "$f" || true
  done
fi
exit 0
POSTINST
chmod 755 "$stage/DEBIAN/postinst"
cat > "$stage/DEBIAN/control" <<CONTROL
Package: exponential-desktop
Version: $version
Architecture: $arch
Maintainer: Exponential <hello@exponential.at>
Section: x11
Priority: optional
Homepage: https://exponential.at
Depends: $depends, dbus-user-session | dbus-x11, systemd, libglib2.0-bin, xdg-desktop-portal (>= 1.18), xdg-desktop-portal-gtk, pipewire
Recommends: gnome-keyring, fonts-inter
Replaces: ede
Conflicts: ede
Provides: ede
Description: Exponential Desktop
 A Wayland desktop session, "Exponential" on the login screen: a compositor
 with floating windows, edge snapping and Xwayland 24.1+ (explicit sync),
 crash recovery and a safe mode, and a shell drawn from genUI documents
 (/usr/share/exponential-desktop/ui; a person's copies in
 ~/.config/exponential-desktop/ui, every edit a commit).
CONTROL

mkdir -p "$out"
deb="$out/exponential-desktop_${version}_${arch}.deb"
dpkg-deb --root-owner-group --build "$stage" "$deb" >/dev/null
echo "$deb"
