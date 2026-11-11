#!/usr/bin/env bash
# See action.yml. Rewrites every place apt reads a mirror from: the classic
# sources.list, sources.list.d (one-line .list and deb822 .sources) and the
# runner image's mirror+file list (/etc/apt/apt-mirrors.txt).
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

# apt-get update exits 0 when only some indexes failed: read its output too.
update() {
  local out rc=0
  out="$($sudo apt-get update -o Acquire::Retries=5 2>&1)" || rc=$?
  printf '%s\n' "$out"
  [ "$rc" -eq 0 ] || return 1
  if grep -E '^(W|E|Err):' <<<"$out" | grep -qE '(archive\.ubuntu\.com|mirrors\.edge\.kernel\.org)'; then
    return 1
  fi
}

rewrite 's#azure\.archive\.ubuntu\.com#archive.ubuntu.com#g'
if update; then exit 0; fi

echo "::warning::archive.ubuntu.com failed, falling back to mirrors.edge.kernel.org"
rewrite 's#https?://archive\.ubuntu\.com/ubuntu#http://mirrors.edge.kernel.org/ubuntu#g'
if update; then exit 0; fi

echo "::error::apt-get update failed on archive.ubuntu.com and mirrors.edge.kernel.org"
exit 1
