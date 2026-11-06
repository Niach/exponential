#!/bin/sh
# Exponential Desktop: adds the Exponential package repository and installs
# it. Ubuntu 24.04 and 26.04, Linux Mint 22, Debian 13, Fedora, Arch.
#   curl -fsSL <this URL> | sudo sh
# Updates then come with the system's own updates.
set -eu
HOST="@PKG_HOST@"
FPR="@PKG_KEY_FPR@"
SUITES="@PKG_APT_SUITES@"
# Another copy of the repositories (CI's install tests): EXPONENTIAL_PKG_URL.
BASE="${EXPONENTIAL_PKG_URL:-https://$HOST}"

say() { printf '%s\n' "$*" >&2; }
[ "$(id -u)" = 0 ] || { say "Run it as root: curl -fsSL … | sudo sh"; exit 1; }
fetch() { if command -v curl >/dev/null 2>&1; then curl -fsSL "$1"; else wget -qO- "$1"; fi; }
. /etc/os-release

# The key, checked against the fingerprint this script carries.
key="$(mktemp)"; trap 'rm -f "$key"' EXIT
fetch "$BASE/exponential.asc" > "$key"
if command -v gpg >/dev/null 2>&1; then
  gpg --show-keys --with-colons "$key" 2>/dev/null | grep -q "^fpr:::::::::$FPR:" || { say "The key at $BASE/exponential.asc is not $FPR."; exit 1; }
fi

case " ${ID:-} ${ID_LIKE:-} " in
  *" debian "*|*" ubuntu "*)
    suite="${UBUNTU_CODENAME:-${VERSION_CODENAME:-}}"
    case " $SUITES " in *" $suite "*) ;; *) say "Exponential Desktop has no packages for ${PRETTY_NAME:-this system} ($suite) yet: $SUITES."; exit 1 ;; esac
    install -d -m 755 /etc/apt/keyrings
    install -m 644 "$key" /etc/apt/keyrings/exponential.asc
    cat > /etc/apt/sources.list.d/exponential.sources <<SOURCES
Types: deb
URIs: $BASE/apt
Suites: $suite
Components: main
Signed-By: /etc/apt/keyrings/exponential.asc
SOURCES
    apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get install -y exponential-desktop exponential-repo
    ;;
  *" fedora "*)
    fetch "$BASE/rpm/exponential.repo" | sed "s|https://$HOST|$BASE|g" > /etc/yum.repos.d/exponential.repo
    rpm --import "$key"
    dnf install -y exponential-desktop exponential-repo
    ;;
  *" arch "*)
    pacman-key --add "$key"
    pacman-key --lsign-key "$FPR"
    printf 'Server = %s/arch/$arch\n' "$BASE" > /etc/pacman.d/exponential-mirrorlist
    grep -q '^\[exponential\]' /etc/pacman.conf || printf '\n[exponential]\nInclude = /etc/pacman.d/exponential-mirrorlist\n' >> /etc/pacman.conf
    pacman -Sy --noconfirm exponential-desktop exponential-repo
    ;;
  *) say "Exponential Desktop has no packages for ${PRETTY_NAME:-this system} yet."; exit 1 ;;
esac
say "Exponential Desktop is installed. Log out and pick \"Exponential\" on the login screen."
