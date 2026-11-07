#!/usr/bin/env bash
# Packs a staged tree (stage.sh) into the .deb of one apt suite (#EDE-34):
#   apps/ede/packaging/pack-deb.sh <stage> <out> <suite> [--xwayland <dir>]
# noble (Ubuntu 24.04, Linux Mint 22) bundles the Xwayland from
# xwayland/build.sh (<dir>; noble's 23.2 has no explicit sync); the other
# suites depend on the distro's (24.1 or newer). Also packs
# exponential-repo for the suite: the source entry and the key, owned by a
# package so a new host or key reaches every install as an update.
set -euo pipefail
[ $# -ge 3 ] || { echo "usage: $0 <stage> <out> <suite> [--xwayland <dir>]" >&2; exit 2; }
src="$1"; out="$2"; suite="$3"; shift 3
xwl=""
[ "${1:-}" = --xwayland ] && xwl="$2"
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=repo.env
. "$here/repo.env"
version="$(cat "$src/.version")"
arch="$(dpkg --print-architecture)"
mkdir -p "$out"
stage="$(mktemp -d)"; chmod 755 "$stage"
trap 'rm -rf "$stage"' EXIT
cp -a "$src/." "$stage/"; rm -f "$stage/.version"
install -Dm644 "$src/usr/share/licenses/exponential-desktop/LICENSE" "$stage/usr/share/doc/exponential-desktop/copyright"
rm -rf "$stage/usr/share/licenses"
# The lock screen's PAM service (#EDE-17); a conffile, so a local edit survives upgrades.
install -Dm644 "$here/pam/exponential-desktop" "$stage/etc/pam.d/exponential-desktop"

# The compositor's, then the shell's (gpui on Wayland: Vulkan through the
# loader, fontconfig/freetype) and exponential-ui's history (git). The
# Vulkan driver is a Recommends: Mesa's for AMD and Intel, NVIDIA's comes
# with its own driver packages (#EDE-59).
depends="libc6, libpipewire-0.3-0, libseat1, libinput10, libgbm1, libudev1, libxkbcommon0, libpixman-1-0, libegl1, libgles2, libwayland-client0, libvulkan1, libfontconfig1, libfreetype6, git"
if [ -n "$xwl" ]; then
  cp -a "$xwl/usr" "$stage/"
  # One entry per package: the versioned one wins.
  depends="$(printf '%s, %s' "$depends" "$(cat "$xwl/DEPENDS")" | tr ',' '\n' | sed 's/^ *//' | awk '
    { n = $1 } !(n in seen) { seen[n] = $0; order[++k] = n; next } index($0, "(") && !index(seen[n], "(") { seen[n] = $0 }
    END { for (i = 1; i <= k; i++) printf "%s%s", (i > 1 ? ", " : ""), seen[order[i]] }')"
else
  depends="$depends, xwayland (>= 2:24.1)"
fi

mkdir -p "$stage/DEBIAN"
echo /etc/pam.d/exponential-desktop > "$stage/DEBIAN/conffiles"
# The upgrade from 0.1.x (the `ede` package, session `ede`): login screens that
# remember the last session (AccountsService for GDM and LightDM, LightDM's
# dmrc cache) keep picking it under its new name. The person's own ede
# directories move at their next login (exponential-session). Nothing here
# restarts anything: a running session keeps running, the new version
# starts at the next login.
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
debversion="$version-1"
[ "$suite" = noble ] && debversion="$version-1~noble1"
cat > "$stage/DEBIAN/control" <<CONTROL
Package: exponential-desktop
Version: $debversion
Architecture: $arch
Maintainer: Exponential <hello@exponential.at>
Section: x11
Priority: optional
Homepage: https://exponential.at/desktop/
Depends: $depends, dbus-user-session | dbus-x11, systemd, libglib2.0-bin, xdg-desktop-portal (>= 1.18), xdg-desktop-portal-gtk, pipewire
Recommends: exponential-repo, mesa-vulkan-drivers, vulkan-tools, gnome-keyring, fonts-inter
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
deb="$out/exponential-desktop_${debversion}_${arch}.deb"
dpkg-deb --root-owner-group -Zxz --build "$stage" "$deb" >/dev/null
echo "$deb"

# exponential-repo: not conffiles on purpose, so an update rewrites them.
repo="$(mktemp -d)"; chmod 755 "$repo"
install -Dm644 "$here/repo/exponential.asc" "$repo/etc/apt/keyrings/exponential.asc"
mkdir -p "$repo/etc/apt/sources.list.d" "$repo/DEBIAN"
sed -e "s|@PKG_HOST@|$PKG_HOST|g" -e "s|@SUITE@|$suite|g" "$here/repo/exponential.sources.in" > "$repo/etc/apt/sources.list.d/exponential.sources"
chmod 644 "$repo/etc/apt/sources.list.d/exponential.sources"
cat > "$repo/DEBIAN/control" <<CONTROL
Package: exponential-repo
Version: $PKG_REPO_VERSION~$suite
Architecture: all
Maintainer: Exponential <hello@exponential.at>
Section: admin
Priority: optional
Homepage: https://exponential.at/desktop/
Description: Exponential package repository
 The apt source and signing key of the Exponential packages ($suite).
CONTROL
dpkg-deb --root-owner-group --build "$repo" "$out/exponential-repo_${PKG_REPO_VERSION}~${suite}_all.deb" >/dev/null
rm -rf "$repo"
echo "$out/exponential-repo_${PKG_REPO_VERSION}~${suite}_all.deb"
