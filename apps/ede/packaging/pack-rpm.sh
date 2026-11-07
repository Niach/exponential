#!/usr/bin/env bash
# Packs a staged tree (stage.sh) into the Fedora .rpm (#EDE-34), in a
# Fedora container (rpmbuild), plus exponential-repo (the .repo file and the
# key, owned by a package so a new host or key reaches every install):
#   apps/ede/packaging/pack-rpm.sh <stage> <out>
# Library requirements come from the binaries (rpm's ELF dependency
# generator); Xwayland is the distro's (24.1 or newer).
set -euo pipefail
[ $# -eq 2 ] || { echo "usage: $0 <stage> <out>" >&2; exit 2; }
src="$(cd "$1" && pwd)"; mkdir -p "$2"; out="$(cd "$2" && pwd)"
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=repo.env
. "$here/repo.env"
version="$(cat "$src/.version")"
top="$(mktemp -d)"
trap 'rm -rf "$top"' EXIT
mkdir -p "$top/root" "$top/repo/etc/yum.repos.d" "$top/repo/etc/pki/rpm-gpg"
cp -a "$src/." "$top/root/"; rm -f "$top/root/.version"
install -Dm644 "$here/pam/exponential-desktop.login" "$top/root/etc/pam.d/exponential-desktop"
sed "s|@PKG_HOST@|$PKG_HOST|g" "$here/repo/exponential.repo.in" > "$top/repo/etc/yum.repos.d/exponential.repo"
cp "$here/repo/exponential.asc" "$top/repo/etc/pki/rpm-gpg/RPM-GPG-KEY-exponential"
# Files, plus the directories that are ours alone.
files() { (cd "$1" && find . -mindepth 1 -type d -path '*exponential*' | sed 's|^\.|%dir |' | sort
  find . -mindepth 1 \( -type f -o -type l \) | sed 's|^\.||' | sort | while read -r f; do
    case "$f" in /etc/pam.d/*) echo "%config(noreplace) $f" ;; *) echo "$f" ;; esac
  done); }
files "$top/root" > "$top/desktop.files"
files "$top/repo" > "$top/repo.files"
cat > "$top/exponential-desktop.spec" <<SPEC
%global debug_package %{nil}
%global _build_id_links none
%global __brp_strip %{nil}
%global __brp_strip_comment_note %{nil}
%global __brp_strip_static_archive %{nil}
Name: exponential-desktop
Version: $version
Release: 1
Summary: Exponential Desktop
License: Apache-2.0
URL: https://exponential.at/desktop/
Requires: xorg-x11-server-Xwayland >= 24.1
Requires: xdg-desktop-portal >= 1.18
Requires: xdg-desktop-portal-gtk
Requires: pipewire
Requires: git-core
Requires: dbus
Requires: systemd
Requires: glib2
Recommends: exponential-repo
Recommends: mesa-vulkan-drivers
Recommends: vulkan-tools
Recommends: gnome-keyring

%description
A Wayland desktop session, "Exponential" on the login screen: a compositor
with floating windows, edge snapping and Xwayland (explicit sync), crash
recovery and a safe mode, and a shell drawn from genUI documents.

%package -n exponential-repo
Version: $PKG_REPO_VERSION
Release: 1
Summary: Exponential package repository
BuildArch: noarch

%description -n exponential-repo
The dnf repository and signing key of the Exponential packages.

%install
cp -a $top/root/. %{buildroot}/

%files -f $top/desktop.files

%files -n exponential-repo -f $top/repo.files
SPEC
# The repo package's files go in the same buildroot.
cp -a "$top/repo/." "$top/root/"
rpmbuild -bb --define "_topdir $top/build" --define "_rpmdir $out" --define "_build_name_fmt %%{NAME}-%%{VERSION}-%%{RELEASE}.%%{ARCH}.rpm" "$top/exponential-desktop.spec" >&2
ls "$out"/*.rpm
