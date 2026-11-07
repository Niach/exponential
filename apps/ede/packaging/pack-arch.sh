#!/usr/bin/env bash
# Packs a staged tree (stage.sh) into the Arch packages (#EDE-34), on Arch
# as a non-root user (makepkg): exponential-desktop and exponential-repo
# (the pacman mirrorlist the [exponential] section includes, so a new host
# reaches every install as an update):
#   apps/ede/packaging/pack-arch.sh <stage> <out>
# Xwayland is the distro's (24.1 or newer).
set -euo pipefail
[ $# -eq 2 ] || { echo "usage: $0 <stage> <out>" >&2; exit 2; }
src="$(cd "$1" && pwd)"; mkdir -p "$2"; out="$(cd "$2" && pwd)"
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=repo.env
. "$here/repo.env"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
sed "s|@PKG_HOST@|$PKG_HOST|g" "$here/repo/exponential-mirrorlist.in" > "$work/mirrorlist"
cp "$here/pam/exponential-desktop.login" "$work/pam"
cat > "$work/exponential-repo.install" <<'INSTALL'
post_install() {
  cp /usr/share/exponential-repo/mirrorlist /etc/pacman.d/exponential-mirrorlist
}
post_upgrade() {
  post_install
}
INSTALL
cat > "$work/PKGBUILD" <<PKGBUILD
pkgbase=exponential-desktop
pkgname=(exponential-desktop exponential-repo)
pkgver=$(cat "$src/.version")
pkgrel=1
arch=(x86_64)
url=https://exponential.at/desktop/
license=(Apache-2.0)
options=(!strip !debug)

package_exponential-desktop() {
  pkgdesc='Exponential Desktop: a Wayland desktop session with a shell drawn from genUI documents'
  depends=(glibc gcc-libs pipewire libinput seatd systemd-libs libxkbcommon pixman libglvnd mesa wayland vulkan-icd-loader fontconfig freetype2 git xorg-xwayland xdg-desktop-portal xdg-desktop-portal-gtk dbus glib2 systemd)
  optdepends=('exponential-repo: updates from the Exponential repository' 'vulkan-driver: the shell draws with Vulkan' 'vulkan-tools: the doctor names the GPU the shell takes' 'gnome-keyring: secrets')
  backup=(etc/pam.d/exponential-desktop)
  cp -a "$src/." "\$pkgdir/"
  rm -f "\$pkgdir/.version"
  install -Dm644 "$work/pam" "\$pkgdir/etc/pam.d/exponential-desktop"
}

package_exponential-repo() {
  arch=(any)
  pkgdesc='Exponential package repository (the pacman mirrorlist)'
  install=exponential-repo.install
  install -Dm644 "$work/mirrorlist" "\$pkgdir/usr/share/exponential-repo/mirrorlist"
}
PKGBUILD
(cd "$work" && PKGDEST="$out" makepkg -f --nodeps >&2)
ls "$out"/*.pkg.tar.zst
