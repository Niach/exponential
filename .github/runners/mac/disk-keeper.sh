#!/bin/bash
# The MacBook's disk keeper: a launchd job (every 15 minutes, install-disk-keeper.sh)
# that keeps free disk above the minimum across every thread on the machine.
# 2026-11-06: free disk fell to 5 GB, OrbStack stopped and took every stack down.
# The CI jobs' own guard (.github/scripts/disk-guard.sh) only runs when a job starts.
#
#   disk-keeper.sh            below DISK_KEEPER_TARGET_GB clean stale build output until above it
#   disk-keeper.sh --dry-run  list what a cleanup would delete, delete nothing
#
# Deletes ONLY stale build output, each step only while free space is still low:
#   1. cargo `deps` files (every profile) and `incremental/` dirs older than a day, in the
#      `target*` dirs of idle worktrees, then in the shared CARGO_TARGET_DIR
#      (a profile only while no build holds its `.cargo-lock`)
#   2. Xcode DerivedData entries with nothing written for a day, and idle
#      worktrees' `apps/ios/build`
#   3. Gradle: build-cache entries older than a day, idle worktrees' Android `build/` dirs
#   4. simulators whose runtime is gone (`simctl delete unavailable`)
#   5. finished capture stacks: docker containers exited over a day ago, then the
#      volumes no container (running or not) uses; never the `exponential` stack,
#      never starts docker
#   6. /tmp scratch: the user's top-level /private/tmp entries nothing wrote to
#      for a day and no process has open (lane DerivedData, capture stacks, logs)
#   7. idle worktrees' `node_modules` and `apps/ios/Tuist/.build` (~5 GB of
#      SwiftPM binary artifacts each)
#   8. lane simulators (`exp-*`, never the shared `exp-store`) nothing wrote to
#      for two days, then cargo git checkouts unused for three days (they
#      re-check out from ~/.cargo/git/db offline)
# A worktree is LIVE (never touched) when it is locked (`git worktree lock`, a
# Claude agent's), holds a `.git` index.lock, is some process's cwd, has a file
# written in the last day (build output aside), or has anything uncommitted
# (modified, staged or untracked). Inside an idle worktree only git-ignored
# paths are deleted.
#
# Log: ~/Library/Logs/exponential/disk-keeper.log (every removal). Warnings go
# to that log AND, at most once an hour, one Notification Center banner.
# Env: DISK_KEEPER_TARGET_GB (40: clean until this much is free), DISK_KEEPER_MIN_GB
# (30: warn below it after cleaning), DISK_KEEPER_REPOS (~/Exponential/repos:
# where the repos and their worktrees live), CARGO_TARGET_DIR.
set -uo pipefail

TARGET_GB="${DISK_KEEPER_TARGET_GB:-40}"
MIN_GB="${DISK_KEEPER_MIN_GB:-30}"
REPOS="${DISK_KEEPER_REPOS:-$HOME/Exponential/repos}"
LOG_DIR="$HOME/Library/Logs/exponential"
LOG="$LOG_DIR/disk-keeper.log"
MODE=clean
case "${1:-}" in
  "") ;;
  --dry-run) MODE=dry-run ;;
  -h|--help) sed -n '2,38p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
  *) echo "usage: $0 [--dry-run]" >&2; exit 2 ;;
esac
[ "$(uname -s)" = "Darwin" ] || { echo "disk-keeper: macOS only" >&2; exit 0; }

mkdir -p "$LOG_DIR"
# One run at a time: a slow du on a full disk must not stack up runs.
LOCK="$LOG_DIR/disk-keeper.lock"
if ! mkdir "$LOCK" 2>/dev/null; then
  if [ -n "$(find "$LOCK" -maxdepth 0 -mmin +60 2>/dev/null)" ]; then rm -rf "$LOCK"; mkdir "$LOCK"; else exit 0; fi
fi
trap 'rm -rf "$LOCK"' EXIT

log() {
  local line="$(date '+%Y-%m-%d %H:%M:%S') $*"
  if [ "$MODE" = "dry-run" ]; then echo "$line"; else echo "$line" >> "$LOG"; fi
}
warn() {
  log "WARN $1"
  [ "$MODE" = "dry-run" ] && return 0
  local stamp="$LOG_DIR/disk-keeper.warned"
  if [ -z "$(find "$stamp" -maxdepth 0 -mmin -60 2>/dev/null)" ]; then
    touch "$stamp"
    osascript -e "display notification \"$1\" with title \"Disk low\"" >/dev/null 2>&1 || true
  fi
}

free_gb() { df -k "$HOME" | awk 'NR == 2 { printf "%d", $4 / 1048576 }'; }
low() { [ "$MODE" = "dry-run" ] || [ "$(free_gb)" -lt "$TARGET_GB" ]; }
kb_of() { du -sk "$@" 2>/dev/null | awk '{ s += $1 } END { printf "%d", s }'; }
# Docker's timestamps (RFC 3339: ...Z or ...+01:00) as epoch seconds, 0 when unparsable.
epoch() {
  local t="$1" base="${1:0:19}" zone
  case "$t" in *Z) zone="+0000" ;; *) zone="$(printf '%s' "${t: -6}" | tr -d ':')" ;; esac
  date -j -f '%Y-%m-%dT%H:%M:%S%z' "$base$zone" +%s 2>/dev/null || echo 0
}
human() { awk -v k="$1" 'BEGIN { if (k >= 1048576) printf "%.1f GB", k / 1048576; else printf "%d MB", k / 1024 }'; }

# Delete whole paths (dirs or files).
remove() {
  local path kb
  for path in "$@"; do
    [ -e "$path" ] || continue
    kb="$(kb_of "$path")"
    if [ "$MODE" = "dry-run" ]; then log "would delete $(human "$kb") $path"; else rm -rf "$path" && log "deleted $(human "$kb") $path"; fi
  done
}
# Delete the files under $1 older than a day (one log line for the lot).
remove_old_files() {
  local dir="$1" kb
  [ -d "$dir" ] || return 0
  kb="$(find "$dir" -type f -mtime +0 -print0 2>/dev/null | xargs -0 du -ck 2>/dev/null | awk 'END { printf "%d", $1 }')"
  [ "${kb:-0}" -gt 0 ] || return 0
  if [ "$MODE" = "dry-run" ]; then log "would delete $(human "$kb") of files older than a day in $dir"; else
    find "$dir" -type f -mtime +0 -delete 2>/dev/null
    log "deleted $(human "$kb") of files older than a day in $dir"
  fi
}
# The stale part of one cargo target dir: every profile's (debug, release,
# <triple>/<profile>) deps files and incremental dirs older than a day,
# skipping a profile while a build holds its .cargo-lock.
clean_cargo_target() {
  local t="$1" p sub
  [ -d "$t" ] || return 0
  while IFS= read -r -d '' p; do
    if [ -n "$(lsof -t "$p/.cargo-lock" 2>/dev/null)" ]; then log "skipped $p: a build holds its .cargo-lock"; continue; fi
    remove_old_files "$p/deps"
    [ -d "$p/incremental" ] || continue
    while IFS= read -r -d '' sub; do remove "$sub"; done \
      < <(find "$p/incremental" -mindepth 1 -maxdepth 1 -mtime +0 -print0 2>/dev/null)
  done < <(find "$t" -mindepth 2 -maxdepth 3 -type d -name deps -print0 2>/dev/null | xargs -0 -n1 dirname 2>/dev/null | tr '\n' '\0')
}

start="$(free_gb)"
if [ "$MODE" != "dry-run" ] && [ "$start" -ge "$TARGET_GB" ]; then
  # Quiet when healthy: one line an hour so the log shows the job is alive.
  [ "$(date +%M)" -lt 15 ] && log "ok ${start} GB free"
  exit 0
fi
log "start ${start} GB free (cleaning until ${TARGET_GB} GB)"

# Every process's cwd, once (the cheapest "a session is working here" signal).
CWDS="$(lsof -a -u "$(id -un)" -d cwd -Fn 2>/dev/null | sed -n 's/^n//p' | sort -u)"

# Worktrees: every repo under $REPOS, each with its linked worktrees, never the main checkout.
IDLE=()
while IFS= read -r -d '' gitdir; do
  repo="${gitdir%/.git}"
  wt="" locked=""
  while IFS= read -r line; do
    case "$line" in
      "worktree "*) wt="${line#worktree }"; locked="" ;;
      locked*) locked=1 ;;
      "")
        if [ -n "$wt" ] && [ "$wt" != "$repo" ] && [ -d "$wt" ]; then
          why=""
          wgit="$(git -C "$wt" rev-parse --absolute-git-dir 2>/dev/null)"
          if [ -n "$locked" ]; then why="locked"
          elif [ -z "$wgit" ]; then why="not a git worktree"
          elif [ -e "$wgit/index.lock" ]; then why="index.lock"
          elif printf '%s\n' "$CWDS" | awk -v p="$wt" '$0 == p || index($0, p "/") == 1 { f = 1 } END { exit !f }'; then why="a process runs in it"
          elif [ -n "$(find "$wgit/index" "$wgit/HEAD" -mtime -1 2>/dev/null)" ]; then why="git activity in the last day"
          elif [ -n "$(find "$wt" \( -type d \( -name node_modules -o -name '.git' -o -name 'target*' -o -name build -o -name .gradle -o -name DerivedData -o -name .turbo -o -name dist -o -name .output \) -prune \) -o -type f -mtime -1 -print 2>/dev/null | head -1)" ]; then why="files written in the last day"
          elif [ -n "$(git -C "$wt" status --porcelain 2>/dev/null | head -1)" ]; then why="uncommitted changes"
          fi
          if [ -z "$why" ]; then IDLE+=("$wt"); elif [ "$MODE" = "dry-run" ]; then log "live ($why): $wt"; fi
        fi
        wt="" ;;
    esac
  done < <(git -C "$repo" worktree list --porcelain 2>/dev/null; echo)
done < <(find "$REPOS" -mindepth 2 -maxdepth 3 -name .git -type d -print0 2>/dev/null)
log "idle worktrees: ${#IDLE[@]}"

# Only git-ignored paths inside an idle worktree.
ignored() { git -C "$1" check-ignore -q "${2#$1/}/" 2>/dev/null; }

# 1. cargo deps + incremental.
if low; then
  for wt in ${IDLE[@]+"${IDLE[@]}"}; do
    for t in "$wt"/target "$wt"/target-* "$wt"/apps/*/target "$wt"/apps/*/target-*; do
      [ -d "$t" ] && ignored "$wt" "$t" && clean_cargo_target "$t"
    done
  done
fi
if low && [ -n "${CARGO_TARGET_DIR:-}" ] && [ -d "$CARGO_TARGET_DIR" ]; then
  clean_cargo_target "$CARGO_TARGET_DIR"
fi

# 2. DerivedData: an entry nothing wrote to for a day.
dd="$HOME/Library/Developer/Xcode/DerivedData"
if low && [ -d "$dd" ]; then
  while IFS= read -r -d '' entry; do
    [ -z "$(find "$entry" -mtime -1 -print 2>/dev/null | head -1)" ] && remove "$entry"
  done < <(find "$dd" -mindepth 1 -maxdepth 1 -print0)
fi
if low; then
  for wt in ${IDLE[@]+"${IDLE[@]}"}; do
    [ -d "$wt/apps/ios/build" ] && ignored "$wt" "$wt/apps/ios/build" && remove "$wt/apps/ios/build"
  done
fi

# 3. Gradle.
if low; then
  remove_old_files "$HOME/.gradle/caches/build-cache-1"
  for wt in ${IDLE[@]+"${IDLE[@]}"}; do
    for b in "$wt"/apps/android/build "$wt"/apps/android/*/build "$wt"/apps/android/.gradle; do
      [ -d "$b" ] && ignored "$wt" "$b" && remove "$b"
    done
  done
fi

# 4. Simulators whose runtime is gone. Booted and available devices stay.
if low && command -v xcrun >/dev/null; then
  n="$(xcrun simctl list devices unavailable 2>/dev/null | grep -c '(unavailable' || true)"
  if [ "${n:-0}" -gt 0 ]; then
    if [ "$MODE" = "dry-run" ]; then log "would delete ${n} unavailable simulators"
    else xcrun simctl delete unavailable && log "deleted ${n} unavailable simulators"; fi
  fi
fi

# 5. Finished capture stacks, only when docker (OrbStack) already runs.
if low && command -v docker >/dev/null && docker info >/dev/null 2>&1; then
  cutoff="$(date -v-1d +%s)"
  docker ps -a --filter status=exited --filter status=created --filter status=dead \
    --format '{{.ID}}|{{.Names}}|{{.Label "com.docker.compose.project"}}' 2>/dev/null |
  while IFS='|' read -r id name project; do
    [ "$project" = "exponential" ] && continue
    finished="$(docker inspect -f '{{.State.FinishedAt}}' "$id" 2>/dev/null)"
    ts="$(epoch "$finished")"
    [ "$ts" -gt 0 ] && [ "$ts" -lt "$cutoff" ] || continue
    if [ "$MODE" = "dry-run" ]; then log "would remove container $name (exited $finished)"
    else docker rm "$id" >/dev/null && log "removed container $name (exited $finished)"; fi
  done
  # Volumes no container uses, created over a day ago, never the trunk stack's.
  docker volume ls -q --filter dangling=true 2>/dev/null | while IFS= read -r v; do
    case "$v" in exponential_*) continue ;; esac
    ts="$(epoch "$(docker volume inspect -f '{{.CreatedAt}}' "$v" 2>/dev/null)")"
    [ "$ts" -gt 0 ] && [ "$ts" -lt "$cutoff" ] || continue
    if [ "$MODE" = "dry-run" ]; then log "would remove volume $v"
    else docker volume rm "$v" >/dev/null 2>&1 && log "removed volume $v"; fi
  done
fi

# 6. /tmp scratch nobody uses: no write for a day, no process cwd or open file in it.
if low; then
  OPEN="$(lsof -u "$(id -un)" -Fn 2>/dev/null | sed -n 's/^n//p' | sed 's#^/tmp/#/private/tmp/#' | awk -F/ '$2 == "private" && $3 == "tmp" && NF > 3 { print "/private/tmp/" $4 }' | sort -u)"
  while IFS= read -r -d '' e; do
    [ -O "$e" ] || continue
    printf '%s\n' "$OPEN" | grep -qxF "$e" && continue
    [ -n "$(find "$e" -mtime -1 -print 2>/dev/null | head -1)" ] && continue
    remove "$e"
    low || break
  done < <(find /private/tmp -mindepth 1 -maxdepth 1 -mtime +0 -print0 2>/dev/null)
fi

# 7. Idle worktrees' dependency installs.
if low; then
  for wt in ${IDLE[@]+"${IDLE[@]}"}; do
    for d in "$wt"/node_modules "$wt"/apps/ios/Tuist/.build; do
      [ -d "$d" ] && ignored "$wt" "$d" && remove "$d"
    done
  done
fi

# 8. Lane simulators left behind, then stale cargo git checkouts.
if low && command -v xcrun >/dev/null; then
  sims="$HOME/Library/Developer/CoreSimulator/Devices"
  xcrun simctl list devices 2>/dev/null | sed -n 's/^ *\(exp-[^ ]*\) (\([0-9A-F-]\{36\}\)).*/\1 \2/p' |
  while read -r name udid; do
    [ "$name" = "exp-store" ] && continue
    [ -d "$sims/$udid" ] || continue
    [ -n "$(find "$sims/$udid" -mtime -2 -print 2>/dev/null | head -1)" ] && continue
    if [ "$MODE" = "dry-run" ]; then log "would delete simulator $name $(human "$(kb_of "$sims/$udid")")"
    else
      kb="$(kb_of "$sims/$udid")"
      xcrun simctl shutdown "$udid" >/dev/null 2>&1
      xcrun simctl delete "$udid" && log "deleted simulator $name $(human "$kb")"
    fi
  done
fi
if low && [ -d "$HOME/.cargo/git/checkouts" ]; then
  while IFS= read -r -d '' c; do remove "$c"; done \
    < <(find "$HOME/.cargo/git/checkouts" -mindepth 2 -maxdepth 2 -type d -mtime +3 -print0 2>/dev/null)
fi

end="$(free_gb)"
log "done ${end} GB free (was ${start} GB)"
if [ "$MODE" != "dry-run" ] && [ "$end" -lt "$MIN_GB" ]; then
  warn "${end} GB free, below ${MIN_GB} GB after cleaning: see $LOG"
fi
exit 0
