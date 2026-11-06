#!/usr/bin/env bash
# Adds packages to the static, signed repositories in <www> (#EDE-34), the
# tree the package host serves as is (build-ede.yml rsyncs it down, runs
# this, rsyncs it up):
#   apps/ede/packaging/repo/publish.sh <incoming> <www>
# <incoming> holds the packs: *.deb (pack-deb.sh; `~noble` = the noble
# suite, the rest go to every other suite), *.rpm (pack-rpm.sh, rpmsigned
# here), *.pkg.tar.zst (pack-arch.sh). The newest $PKG_KEEP versions of each
# package stay. Needs: gpg with the signing key (PKG_KEY_FPR), apt-ftparchive,
# createrepo_c, rpmsign, and docker for the pacman database (repo-add).
# Layout:
#   exponential.asc                      the key
#   setup.sh                             the one-line setup (also on the site)
#   apt/dists/<suite>/{InRelease,Release,Release.gpg,main/binary-amd64/Packages*}
#   apt/pool/<suite>/*.deb
#   rpm/exponential.repo, rpm/x86_64/{*.rpm,repodata/}
#   arch/x86_64/{exponential.db*,*.pkg.tar.zst{,.sig}}
set -euo pipefail
[ $# -eq 2 ] || { echo "usage: $0 <incoming> <www>" >&2; exit 2; }
in="$(cd "$1" && pwd)"; mkdir -p "$2"; www="$(cd "$2" && pwd)"
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=../repo.env
. "$here/../repo.env"
key="$PKG_KEY_FPR"
gpg --list-secret-keys "$key" >/dev/null

# Keeps the newest $PKG_KEEP files of each package in <dir> (name = what
# precedes the first `_` for debs, the name-version split for the rest).
prune() {
  local dir="$1" sep="$2"
  ls "$dir" 2>/dev/null | grep -E '\.(deb|rpm|pkg\.tar\.zst)$' | sed -E "s/$sep.*//" | sort -u | while read -r name; do
    ls "$dir" | grep -E "^${name}${sep}" | grep -E '\.(deb|rpm|pkg\.tar\.zst)$' | sort -V | head -n -"$PKG_KEEP" | while read -r old; do
      rm -f "$dir/$old" "$dir/$old.sig"
    done
  done
}

install -Dm644 "$here/exponential.asc" "$www/exponential.asc"
sed -e "s|@PKG_HOST@|$PKG_HOST|g" -e "s|@PKG_KEY_FPR@|$PKG_KEY_FPR|g" -e "s|@PKG_APT_SUITES@|$PKG_APT_SUITES|g" "$here/setup.sh" > "$www/setup.sh"

# apt: one pool per suite (hard links share the bytes of a deb in several).
for suite in $PKG_APT_SUITES; do
  pool="$www/apt/pool/$suite"; mkdir -p "$pool"
  for deb in "$in"/*.deb; do
    [ -e "$deb" ] || continue
    b="$(basename "$deb")"
    case "$b" in
      *"~$suite"*) ln -f "$deb" "$pool/$b" ;;
      *~noble*|*~resolute*|*~trixie*) ;;
      *) [ "$suite" = noble ] || ln -f "$deb" "$pool/$b" ;;
    esac
  done
  prune "$pool" _
  dist="$www/apt/dists/$suite"; bin="$dist/main/binary-amd64"
  rm -rf "$dist"; mkdir -p "$bin"
  (cd "$www/apt" && apt-ftparchive packages "pool/$suite") > "$bin/Packages"
  gzip -9nk "$bin/Packages"; xz -kf "$bin/Packages"
  apt-ftparchive -o APT::FTPArchive::Release::Origin=Exponential -o APT::FTPArchive::Release::Label=Exponential \
    -o APT::FTPArchive::Release::Suite="$suite" -o APT::FTPArchive::Release::Codename="$suite" \
    -o APT::FTPArchive::Release::Architectures="amd64 all" -o APT::FTPArchive::Release::Components=main \
    release "$dist" > "$dist/Release.tmp"
  mv "$dist/Release.tmp" "$dist/Release"
  gpg --batch --yes --local-user "$key" --clearsign -o "$dist/InRelease" "$dist/Release"
  gpg --batch --yes --local-user "$key" -abs -o "$dist/Release.gpg" "$dist/Release"
done

# dnf: signed packages, signed metadata.
rpms="$www/rpm/x86_64"; mkdir -p "$rpms"
for rpm in "$in"/*.rpm; do
  [ -e "$rpm" ] || continue
  cp "$rpm" "$rpms/"
  rpmsign --define "__gpg $(command -v gpg)" --define "_gpg_name $key" --define "_openpgp_sign_id $key" --addsign "$rpms/$(basename "$rpm")" >/dev/null
done
prune "$rpms" '-[0-9]'
sed "s|@PKG_HOST@|$PKG_HOST|g" "$here/exponential.repo.in" > "$www/rpm/exponential.repo"
createrepo_c --quiet --update "$rpms"
gpg --batch --yes --local-user "$key" --detach-sign --armor -o "$rpms/repodata/repomd.xml.asc" "$rpms/repodata/repomd.xml"

# pacman: a detached signature per package, a signed database.
arch="$www/arch/x86_64"; mkdir -p "$arch"
for pkg in "$in"/*.pkg.tar.zst; do
  [ -e "$pkg" ] || continue
  cp "$pkg" "$arch/"
  gpg --batch --yes --local-user "$key" --detach-sign --no-armor -o "$arch/$(basename "$pkg").sig" "$arch/$(basename "$pkg")"
done
prune "$arch" '-[0-9]'
rm -f "$arch"/exponential.db* "$arch"/exponential.files*
if ls "$arch"/*.pkg.tar.zst >/dev/null 2>&1; then
  # repo-add is Arch's; the database is plain tar + desc files it writes.
  docker run --rm -v "$arch:/repo" -w /repo archlinux:latest bash -c 'repo-add -q exponential.db.tar.gz *.pkg.tar.zst && chown -R '"$(id -u):$(id -g)"' /repo'
  for f in exponential.db.tar.gz exponential.files.tar.gz; do
    gpg --batch --yes --local-user "$key" --detach-sign --no-armor -o "$arch/$f.sig" "$arch/$f"
  done
  # pacman reads exponential.db: plain copies, not symlinks (served as files everywhere).
  for f in db files; do
    rm -f "$arch/exponential.$f" "$arch/exponential.$f.sig"
    cp "$arch/exponential.$f.tar.gz" "$arch/exponential.$f"; cp "$arch/exponential.$f.tar.gz.sig" "$arch/exponential.$f.sig"
  done
fi
echo "$www"
