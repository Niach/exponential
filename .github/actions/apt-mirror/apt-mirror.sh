#!/usr/bin/env bash
# See action.yml. Rewrites every place apt reads a mirror from: the classic
# sources.list, sources.list.d (one-line .list and deb822 .sources) and the
# runner image's mirror+file list (/etc/apt/apt-mirrors.txt).
#
# Order: archive.ubuntu.com (security.ubuntu.com kept) when Canonical's hosts
# resolve; otherwise (2026-11-11: azure.archive, archive AND security all
# failed DNS on GitHub runners) EVERY source, security included, moves to a
# non-Canonical mirror. Those carry the -security pockets under the same path.
set -euo pipefail

sudo=""
[ "$(id -u)" -eq 0 ] || sudo="sudo"

files=()
for f in /etc/apt/sources.list /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources /etc/apt/apt-mirrors.txt; do
  [ -f "$f" ] && files+=("$f")
done

rewrite() { # sed expression
  [ "${#files[@]}" -eq 0 ] || $sudo sed -i -E "$1" "${files[@]}"
}

# Every apt-get of the job (installs too) retries a failed download.
printf 'Acquire::Retries "5";\nAcquire::http::Timeout "30";\nAcquire::https::Timeout "30";\n' \
  | $sudo tee /etc/apt/apt.conf.d/80-ci-retries >/dev/null

resolves() { getent hosts "$1" >/dev/null 2>&1; }

# apt-get update exits 0 when only some indexes failed: read its output too.
update() { # host the ubuntu sources point at now
  local out rc=0
  out="$($sudo apt-get update -o Acquire::Retries=5 2>&1)" || rc=$?
  printf '%s\n' "$out"
  [ "$rc" -eq 0 ] || return 1
  if grep -E '^(W|E|Err):' <<<"$out" | grep -qF -e 'ubuntu.com' -e "$1"; then
    return 1
  fi
}

canonical='https?://((azure\.)?archive|security)\.ubuntu\.com/ubuntu/?'
current=''

if resolves archive.ubuntu.com && resolves security.ubuntu.com; then
  rewrite 's#azure\.archive\.ubuntu\.com#archive.ubuntu.com#g'
  if update archive.ubuntu.com; then exit 0; fi
  echo "::warning::archive.ubuntu.com failed; moving every source to a non-Canonical mirror"
else
  echo "::warning::Canonical's apt hosts do not resolve; moving every source to a non-Canonical mirror"
fi

for mirror in mirrors.edge.kernel.org/ubuntu ftp.halifax.rwth-aachen.de/ubuntu; do
  host="${mirror%%/*}"
  if ! resolves "$host"; then
    echo "::warning::$host does not resolve"
    continue
  fi
  if [ -z "$current" ]; then
    rewrite "s#${canonical}#http://${mirror}/#g"
  else
    rewrite "s#https?://${current//./\\.}/?#http://${mirror}/#g"
  fi
  current="$mirror"
  if update "$host"; then exit 0; fi
  echo "::warning::$host failed"
done

echo "::error::apt-get update failed on Canonical's mirrors and every fallback"
exit 1
