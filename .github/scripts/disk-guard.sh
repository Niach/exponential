#!/usr/bin/env bash
# EXP-1396: the MacBook (self-hosted `exp-mac` + the dev box) ran out of disk
# mid-release-train. Every job on the Mac runs this first.
#
#   disk-guard.sh             report; below DISK_GUARD_MIN_GB clean, below DISK_GUARD_FLOOR_GB fail
#   disk-guard.sh --check     report only; exit 1 below DISK_GUARD_MIN_GB (the operator's doctor)
#   disk-guard.sh --dry-run   report + list what a cleanup would delete, delete nothing
#
# Cleanup order, each step only while free space is still below the minimum:
#   1. Xcode DerivedData entries older than a day
#   2. cargo `incremental/` dirs under the cargo target dirs
#   3. simulators whose runtime is gone (`simctl delete unavailable`; never a
#      booted or available device)
#   4. dangling docker images, if docker (OrbStack) is up
#   5. the runner's sccache (~/Library/Caches/sccache-ci)
#
# Env: DISK_GUARD_MIN_GB (25), DISK_GUARD_FLOOR_GB (10), DISK_GUARD_CARGO_TARGETS
# (colon-separated extra target dirs; CARGO_TARGET_DIR and the checkout's
# apps/desktop/target are always included).
# Not macOS = a no-op (exit 0), so a GitHub-hosted or Linux job is never hurt.
set -euo pipefail

MIN_GB="${DISK_GUARD_MIN_GB:-25}"
FLOOR_GB="${DISK_GUARD_FLOOR_GB:-10}"
MODE=clean
case "${1:-}" in
  "") ;;
  --check) MODE=check ;;
  --dry-run) MODE=dry-run ;;
  -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
  *) echo "usage: $0 [--check|--dry-run]" >&2; exit 2 ;;
esac

if [ "$(uname -s)" != "Darwin" ]; then
  echo "disk-guard: not macOS, nothing to do"
  exit 0
fi

error() {
  if [ "${GITHUB_ACTIONS:-}" = "true" ]; then echo "::error title=Disk full::$1"; else echo "error: $1" >&2; fi
}
warn() {
  if [ "${GITHUB_ACTIONS:-}" = "true" ]; then echo "::warning title=Disk low::$1"; else echo "warning: $1" >&2; fi
}

# Free GB on the volume holding $HOME (the APFS Data volume).
free_gb() { df -k "$HOME" | awk 'NR == 2 { printf "%d", $4 / 1048576 }'; }
low() { [ "$(free_gb)" -lt "$MIN_GB" ]; }
size_of() { du -sh "$@" 2>/dev/null | awk '{ s = s (s ? ", " : "") $1 " " $2 } END { print s }'; }

# Delete (or, in a dry run, list) paths.
remove() {
  [ "$#" -gt 0 ] || return 0
  local path
  for path in "$@"; do
    if [ "$MODE" = "dry-run" ]; then
      echo "  would delete: $(size_of "$path")"
    else
      echo "  delete: $(size_of "$path")"
      rm -rf "$path"
    fi
  done
}

# In a dry run nothing frees space, so every step is listed.
still_low() { [ "$MODE" = "dry-run" ] || low; }

start="$(free_gb)"
echo "disk-guard: ${start} GB free on $(df -k "$HOME" | awk 'NR == 2 { print $NF }') (minimum ${MIN_GB} GB, floor ${FLOOR_GB} GB)"

if [ "$MODE" = "check" ]; then
  if low; then
    error "${start} GB free, below the ${MIN_GB} GB minimum: run .github/scripts/disk-guard.sh to clean"
    exit 1
  fi
  echo "disk-guard: ok"
  exit 0
fi

if ! low; then
  echo "disk-guard: ok"
  exit 0
fi

[ "$MODE" = "dry-run" ] && echo "disk-guard: dry run, nothing is deleted"

# 1. DerivedData older than a day (a running build touches its own entry).
dd="$HOME/Library/Developer/Xcode/DerivedData"
if still_low && [ -d "$dd" ]; then
  echo "1. Xcode DerivedData older than a day"
  old=()
  while IFS= read -r -d '' entry; do old+=("$entry"); done \
    < <(find "$dd" -mindepth 1 -maxdepth 1 -mtime +0 -print0)
  if [ "${#old[@]}" -gt 0 ]; then remove "${old[@]}"; else echo "  nothing"; fi
fi

# 2. cargo incremental dirs (<target>/<profile>/incremental, <target>/<triple>/<profile>/incremental).
if still_low; then
  echo "2. cargo incremental dirs"
  targets=()
  [ -n "${CARGO_TARGET_DIR:-}" ] && targets+=("$CARGO_TARGET_DIR")
  [ -n "${GITHUB_WORKSPACE:-}" ] && targets+=("$GITHUB_WORKSPACE/apps/desktop/target")
  if [ -n "${DISK_GUARD_CARGO_TARGETS:-}" ]; then
    IFS=: read -r -a extra <<<"$DISK_GUARD_CARGO_TARGETS"
    targets+=("${extra[@]}")
  fi
  inc=()
  for target in ${targets[@]+"${targets[@]}"}; do
    [ -d "$target" ] || continue
    while IFS= read -r -d '' dir; do inc+=("$dir"); done \
      < <(find "$target" -mindepth 2 -maxdepth 3 -type d -name incremental -prune -print0)
  done
  if [ "${#inc[@]}" -gt 0 ]; then remove "${inc[@]}"; else echo "  nothing"; fi
fi

# 3. Simulators whose runtime is gone. Booted and available devices stay.
if still_low && command -v xcrun >/dev/null; then
  echo "3. unavailable simulators"
  unavailable="$(xcrun simctl list devices unavailable 2>/dev/null | grep -c '(unavailable' || true)"
  if [ "${unavailable:-0}" -eq 0 ]; then
    echo "  nothing"
  elif [ "$MODE" = "dry-run" ]; then
    echo "  would run: xcrun simctl delete unavailable (${unavailable} devices)"
  else
    echo "  xcrun simctl delete unavailable (${unavailable} devices)"
    xcrun simctl delete unavailable || warn "simctl delete unavailable failed"
  fi
fi

# 4. Dangling docker images, only when docker (OrbStack) already runs.
if still_low && command -v docker >/dev/null; then
  echo "4. dangling docker images"
  if ! docker info >/dev/null 2>&1; then
    echo "  docker is not running"
  else
    dangling="$(docker images -f dangling=true -q | wc -l | tr -d ' ')"
    if [ "$dangling" -eq 0 ]; then
      echo "  nothing"
    elif [ "$MODE" = "dry-run" ]; then
      echo "  would run: docker image prune -f (${dangling} images)"
    else
      echo "  docker image prune -f (${dangling} images)"
      docker image prune -f || warn "docker image prune failed"
    fi
  fi
fi

# 5. The runner's sccache, last: it is what makes the next build fast.
sccache_dir="$HOME/Library/Caches/sccache-ci"
if still_low && [ -d "$sccache_dir" ]; then
  echo "5. the runner's sccache"
  if [ "$MODE" != "dry-run" ] && command -v sccache >/dev/null; then
    SCCACHE_DIR="$sccache_dir" sccache --stop-server >/dev/null 2>&1 || true
  fi
  remove "$sccache_dir"
fi

end="$(free_gb)"
echo "disk-guard: ${end} GB free (was ${start} GB)"
if [ "$end" -lt "$FLOOR_GB" ]; then
  if [ "$MODE" = "dry-run" ]; then
    warn "${end} GB free is below the ${FLOOR_GB} GB floor: a real run would fail the job if cleaning frees too little"
    exit 0
  fi
  error "${end} GB free after cleanup, below the ${FLOOR_GB} GB floor: free disk on the MacBook (or stop its runner: ~/actions-runner/svc.sh stop)"
  exit 1
fi
if low && [ "$MODE" != "dry-run" ]; then
  warn "${end} GB free, still below the ${MIN_GB} GB minimum"
fi
exit 0
