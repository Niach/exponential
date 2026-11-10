#!/bin/sh
# The install test of one distro (#EDE-35), inside its container, against a
# copy of the repositories at $EXPONENTIAL_PKG_URL (build-ede.yml):
#   EXPONENTIAL_PKG_URL=http://127.0.0.1:8080 apps/ede/packaging/smoke.sh
# Installs through setup.sh, then: every binary finds its libraries, the
# factory documents validate, the theme bridge writes its files, the doctor
# runs. On apt it first installs a stand-in for the 0.1.x `ede` package and
# checks the upgrade (ede replaced, the remembered session renamed).
set -eu
. /etc/os-release
echo "== $PRETTY_NAME"
case " ${ID:-} ${ID_LIKE:-} " in
  *" debian "*|*" ubuntu "*)
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq && apt-get install -y -qq curl gpg ca-certificates >/dev/null
    old="$(mktemp -d)"; mkdir -p "$old/DEBIAN" "$old/usr/bin" "$old/etc/pam.d"
    printf 'Package: ede\nVersion: 0.1.2\nArchitecture: amd64\nMaintainer: x <x@x>\nDescription: stand-in\n stand-in\n' > "$old/DEBIAN/control"
    echo /etc/pam.d/ede > "$old/DEBIAN/conffiles"; : > "$old/etc/pam.d/ede"; : > "$old/usr/bin/ede-compositor"
    dpkg-deb --build "$old" /tmp/ede.deb >/dev/null && dpkg -i /tmp/ede.deb >/dev/null
    mkdir -p /var/lib/AccountsService/users && printf '[User]\nSession=ede\n' > /var/lib/AccountsService/users/person
    ;;
  *" fedora "*) dnf install -y -q curl gnupg2 >/dev/null ;;
  *" arch "*) pacman-key --init >/dev/null 2>&1; pacman -Syu --noconfirm --needed curl gnupg >/dev/null ;;
esac
# The rendered script the repositories serve (the host and key filled in).
curl -fsSL "$EXPONENTIAL_PKG_URL/setup.sh" | sh

for b in /usr/bin/exponential-compositor /usr/bin/exponential-session /usr/bin/exponential-shell /usr/bin/exponential-sealed /usr/bin/exponential-ui /usr/bin/exponential-services /usr/bin/exponential-theme /usr/bin/exponential-ctl /usr/libexec/xdg-desktop-portal-exponential; do
  test -x "$b"
  if ldd "$b" | grep 'not found'; then echo "$b: missing libraries" >&2; exit 1; fi
done
exponential-ui --dir "$(mktemp -d)/ui" check
test -f /usr/share/exponential-desktop/apps/notes.app.json
HOME="$(mktemp -d)" exponential-theme apply --files-only | grep -q '^GTK '
grep -q exponential-shell.service /usr/lib/systemd/user/exponential-session.target
grep -q '^Exec=exponential-session$' /usr/share/wayland-sessions/exponential.desktop
test -f /etc/pam.d/exponential-desktop
exponential-compositor doctor || true
if command -v dpkg >/dev/null 2>&1; then
  if dpkg-query -W -f '${Status}' ede 2>/dev/null | grep -q 'ok installed'; then echo "ede is still installed" >&2; exit 1; fi
  grep -q '^Session=exponential$' /var/lib/AccountsService/users/person
  grep -q "^Suites: ${UBUNTU_CODENAME:-$VERSION_CODENAME}$" /etc/apt/sources.list.d/exponential.sources
  case "${UBUNTU_CODENAME:-}" in
    noble) /usr/lib/exponential-desktop/bin/Xwayland -version 2>&1 | grep 'Xwayland Version 24\.1\.' ;;
    *) test ! -e /usr/lib/exponential-desktop/bin/Xwayland ;;
  esac
fi
echo "== $PRETTY_NAME: ok"
