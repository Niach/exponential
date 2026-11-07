#!/usr/bin/env bash
# Builds Exponential Desktop and stages what every package ships (#EDE-34):
#   apps/ede/packaging/stage.sh [--dev] [--no-build] <dir>
# The .deb, .rpm and Arch package are packed from the same tree
# (pack-deb.sh, pack-rpm.sh, pack-arch.sh), built once on Ubuntu 24.04 (the
# oldest glibc of the matrix, so the binaries run on every newer distro).
# Build deps: cargo and the headers in apps/ede/README.md (udev feature),
# plus the shell's, built in apps/desktop (gpui: libxkbcommon, wayland, vulkan loader,
# fontconfig, clang). --dev stages the dev profile (a VM check in minutes;
# never a release); --no-build stages what target/ already holds.
# Contents (#EDE-2): exponential-compositor, exponential-session,
# exponential-ui, exponential-shell, exponential-sealed (#EDE-17: lock,
# polkit, consent), exponential-services (#EDE-13), exponential-theme
# (#EDE-19) in /usr/bin, xdg-desktop-portal-exponential (#EDE-15), the
# factory documents in /usr/share/exponential-desktop/ui, the factory apps in
# /usr/share/exponential-desktop/apps (#VAPP-273), the session's
# systemd user units (exponential-session.target starts
# exponential-shell.service = `exponential-ui supervise -- exponential-shell`),
# the login screen entries (/usr/share/wayland-sessions: GDM, LightDM, SDDM).
# The lock screen's PAM service is per distro family (pack-*.sh).
set -euo pipefail
profile=release
build=1
while [ $# -gt 1 ]; do
  case "$1" in
    --dev) profile=debug ;;
    --no-build) build=0 ;;
    *) echo "usage: $0 [--dev] [--no-build] <dir>" >&2; exit 2 ;;
  esac
  shift
done
[ $# -eq 1 ] || { echo "usage: $0 [--dev] [--no-build] <dir>" >&2; exit 2; }
stage="$1"
here="$(cd "$(dirname "$0")" && pwd)"
ede="$(dirname "$here")"
target="${CARGO_TARGET_DIR:-$ede/target}"
# The shell is a member of the IDE's workspace (apps/desktop, #EDE-51), with
# its own target directory unless a shared CARGO_TARGET_DIR says otherwise.
desktop="$(dirname "$ede")/desktop"
shell_target="${CARGO_TARGET_DIR:-$desktop/target}"

if [ "$build" = 1 ]; then
  release=(--release); [ "$profile" = debug ] && release=()
  (cd "$ede" && cargo build "${release[@]}" --locked -p ede-compositor -p ede-session -p ede-ui -p ede-services -p ede-portal -p ede-theme -p ede-ctl --features ede-compositor/udev)
  # The shell builds in the IDE's workspace (gpui never enters the compositor's).
  (cd "$desktop" && cargo build "${release[@]}" --locked -p ede-shell --bins)
fi

rm -rf "$stage"; mkdir -p "$stage"; chmod 755 "$stage"
for bin in exponential-compositor exponential-session exponential-ui exponential-services exponential-theme exponential-ctl; do
  install -Dm755 "$target/$profile/$bin" "$stage/usr/bin/$bin"
done
install -Dm755 "$shell_target/$profile/exponential-shell" "$stage/usr/bin/exponential-shell"
# Tier B (#EDE-17): the lock screen, the polkit agent and consent.
install -Dm755 "$shell_target/$profile/exponential-sealed" "$stage/usr/bin/exponential-sealed"
# The portal backend (#EDE-15): ScreenCast, Screenshot, RemoteDesktop,
# GlobalShortcuts, Settings; the frontend routes them here (exponential-portals.conf).
install -Dm755 "$target/$profile/xdg-desktop-portal-exponential" "$stage/usr/libexec/xdg-desktop-portal-exponential"
install -Dm644 "$here/portal/exponential.portal" "$stage/usr/share/xdg-desktop-portal/portals/exponential.portal"
install -Dm644 "$here/portal/exponential-portals.conf" "$stage/usr/share/xdg-desktop-portal/exponential-portals.conf"
install -Dm644 "$here/portal/org.freedesktop.impl.portal.desktop.exponential.service" "$stage/usr/share/dbus-1/services/org.freedesktop.impl.portal.desktop.exponential.service"
install -Dm644 "$here/portal/xdg-desktop-portal-exponential.service" "$stage/usr/lib/systemd/user/xdg-desktop-portal-exponential.service"
for f in "$ede"/ui/*.json; do install -Dm644 "$f" "$stage/usr/share/exponential-desktop/ui/$(basename "$f")"; done
# #VAPP-273: the factory apps (Apps packages the shell runs on the Engine); a
# person's ~/.config/exponential-desktop/apps/<name>.app.json replaces one by file name.
for f in "$ede"/apps/*.app.json; do install -Dm644 "$f" "$stage/usr/share/exponential-desktop/apps/$(basename "$f")"; done
for f in "$here"/wayland-sessions/*.desktop; do install -Dm644 "$f" "$stage/usr/share/wayland-sessions/$(basename "$f")"; done
for f in "$here"/systemd/*; do install -Dm644 "$f" "$stage/usr/lib/systemd/user/$(basename "$f")"; done
install -Dm644 "$ede/../../LICENSE" "$stage/usr/share/licenses/exponential-desktop/LICENSE"
sed -n 's/^version = "\(.*\)"/\1/p' "$ede/Cargo.toml" | head -1 > "$stage/.version"
echo "$stage"
