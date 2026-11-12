#!/bin/bash
# The Mint's disk keeper: a systemd user timer (every 15 minutes,
# install-disk-keeper.sh) that keeps free disk above the minimum across every
# session, the two CI runners and the local stacks. The Linux twin of
# ../mac/disk-keeper.sh.
# 2026-11-12: free disk fell from 241 GB (11-08) to 9.5 GB: 107 GB of Docker
# build cache the weekly prune never matched, ~80 GB of per-session cargo
# targets outside their worktrees (~/.cache/<x>-target, /tmp/<x>/target-*:
# nothing removes them when the session ends), the shared CARGO_TARGET_DIR's
# debug test binaries (0.8 GB per `ui` test build) and a 43 GB runner sccache.
#
#   disk-keeper.sh            below DISK_KEEPER_TARGET_GB clean stale build output until above it
#   disk-keeper.sh --dry-run  list what a cleanup would delete, delete nothing
#
# Deletes ONLY regenerable build output, each step only while free space is still low:
#   1. Docker build cache above 20 GB, dangling images (never volumes, never containers)
#   2. scratch cargo targets OUTSIDE any worktree (a dir whose CACHEDIR.TAG cargo wrote,
#      directly in ~/.cache or /tmp, or one level below /tmp): whole, when nothing
#      wrote to it for two days and no process has it open or as its cwd
#   3. cargo `deps` files (every profile) and `incremental/` dirs older than a day:
#      the remaining scratch targets, idle worktrees' `target*` dirs, then the shared
#      CARGO_TARGET_DIR (a profile only while no build holds its `.cargo-lock`)
#   4. the CI runners' work dirs (`/tmp/runner-N/work/**/target`): deps files older
#      than a day and `incremental/`, only while the runner runs no job
#   5. Gradle: build-cache entries older than a day, idle worktrees' Android `build/` dirs
#   6. /tmp scratch: the user's top-level /tmp entries nothing wrote to for a day
#      and no process has open (capture stacks, test dirs, logs)
#   7. idle worktrees' `node_modules`
#   8. lane AVDs (`exp*`, never another AVD) nothing wrote to for two days and no
#      emulator runs, then cargo git checkouts unused for three days
# A worktree is LIVE (never touched) when it is locked, holds a `.git` index.lock,
# is some process's cwd, has a file written in the last day (build output aside),
# or has anything uncommitted. Inside an idle worktree only git-ignored paths go.
#
# Log: ~/.local/state/exponential/disk-keeper.log (every removal). Warnings go to
# that log AND, at most once an hour, one desktop notification (notify-send).
# Env: DISK_KEEPER_TARGET_GB (80: clean until this much is free), DISK_KEEPER_MIN_GB
# (60: warn below it after cleaning), DISK_KEEPER_REPOS (~/Exponential/repos),
# DISK_KEEPER_RUNNERS (exp-runners-runner-1-1 exp-runners-runner-2-1), CARGO_TARGET_DIR
# (default: <repos>/Niach/exponential.cargo-target when it exists).
set -uo pipefail

TARGET_GB="${DISK_KEEPER_TARGET_GB:-80}"
MIN_GB="${DISK_KEEPER_MIN_GB:-60}"
REPOS="${DISK_KEEPER_REPOS:-$HOME/Exponential/repos}"
RUNNERS="${DISK_KEEPER_RUNNERS:-exp-runners-runner-1-1 exp-runners-runner-2-1}"
SHARED_TARGET="${CARGO_TARGET_DIR:-$REPOS/Niach/exponential.cargo-target}"
LOG_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/exponential"
LOG="$LOG_DIR/disk-keeper.log"
ME="$(id -un)"
MODE=clean
case "${1:-}" in
  "") ;;
  --dry-run) MODE=dry-run ;;
  -h|--help) sed -n '2,41p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
  *) echo "usage: $0 [--dry-run]" >&2; exit 2 ;;
esac
[ "$(uname -s)" = "Linux" ] || { echo "disk-keeper: Linux only" >&2; exit 0; }

mkdir -p "$LOG_DIR"
# One run at a time: a slow du on a full disk must not stack up runs.
LOCK="$LOG_DIR/disk-keeper.lock"
if ! mkdir "$LOCK" 2>/dev/null; then
  if [ -n "$(find "$LOCK" -maxdepth 0 -mmin +60 2>/dev/null)" ]; then rm -rf "$LOCK"; mkdir "$LOCK"; else exit 0; fi
fi
trap 'rm -rf "$LOCK"' EXIT

log() {
  local line
  line="$(date '+%Y-%m-%d %H:%M:%S') $*"
  if [ "$MODE" = "dry-run" ]; then echo "$line"; else echo "$line" >> "$LOG"; fi
}
warn() {
  log "WARN $1"
  [ "$MODE" = "dry-run" ] && return 0
  local stamp="$LOG_DIR/disk-keeper.warned"
  if [ -z "$(find "$stamp" -maxdepth 0 -mmin -60 2>/dev/null)" ]; then
    touch "$stamp"
    notify-send -u critical "Disk low" "$1" >/dev/null 2>&1 || true
  fi
}

free_gb() { df -Pk "$HOME" | awk 'NR == 2 { printf "%d", $4 / 1048576 }'; }
low() { [ "$MODE" = "dry-run" ] || [ "$(free_gb)" -lt "$TARGET_GB" ]; }
kb_of() { du -sk "$@" 2>/dev/null | awk '{ s += $1 } END { printf "%d", s }'; }
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
  kb="$(find "$dir" -type f -mtime +0 -printf '%k\n' 2>/dev/null | awk '{ s += $1 } END { printf "%d", s }')"
  [ "${kb:-0}" -gt 1024 ] || return 0
  if [ "$MODE" = "dry-run" ]; then log "would delete $(human "$kb") of files older than a day in $dir"; else
    find "$dir" -type f -mtime +0 -delete 2>/dev/null
    log "deleted $(human "$kb") of files older than a day in $dir"
  fi
}
# A build holds this profile: any process has its .cargo-lock open.
building() { [ -e "$1/.cargo-lock" ] && fuser -s "$1/.cargo-lock" 2>/dev/null; }
# The stale part of one cargo target dir: every profile's (debug, release,
# <triple>/<profile>) deps files and incremental dirs older than a day,
# skipping a profile while a build holds its .cargo-lock.
clean_cargo_target() {
  local t="$1" p sub
  [ -d "$t" ] || return 0
  while IFS= read -r -d '' p; do
    if building "$p"; then log "skipped $p: a build holds its .cargo-lock"; continue; fi
    remove_old_files "$p/deps"
    [ -d "$p/incremental" ] || continue
    while IFS= read -r -d '' sub; do remove "$sub"; done \
      < <(find "$p/incremental" -mindepth 1 -maxdepth 1 -mtime +0 -print0 2>/dev/null)
    low || return 0
  done < <(find "$t" -mindepth 2 -maxdepth 3 -type d -name deps -printf '%h\0' 2>/dev/null)
}

start="$(free_gb)"
if [ "$MODE" != "dry-run" ] && [ "$start" -ge "$TARGET_GB" ]; then
  # Quiet when healthy: one line an hour so the log shows the job is alive.
  [ "$(date +%M)" -lt 15 ] && log "ok ${start} GB free"
  exit 0
fi
log "start ${start} GB free (cleaning until ${TARGET_GB} GB)"

# Every process's cwd and open files, once (the "something works here" signal).
# /proc of the user's own processes only; the CI containers' are root's.
CWDS="$(for p in /proc/[0-9]*; do readlink "$p/cwd" 2>/dev/null; done | sort -u)"
OPEN="$(for p in /proc/[0-9]*; do readlink "$p/cwd" 2>/dev/null; ls -l "$p/fd" 2>/dev/null | sed -n 's/.* -> \(\/.*\)/\1/p'; done | sort -u)"
# Anything under $1 is a cwd or open file.
in_use() { printf '%s\n' "$OPEN" | awk -v p="$1" '$0 == p || index($0, p "/") == 1 { f = 1 } END { exit !f }'; }
# Nothing under $1 written within $2 days.
untouched() { [ -z "$(find "$1" -mtime -"$2" -print -quit 2>/dev/null)" ]; }

# 1. Docker: build cache above 20 GB, dangling images. The runners and every
# local stack keep their containers and volumes.
if low && command -v docker >/dev/null && docker info >/dev/null 2>&1; then
  if [ "$MODE" = "dry-run" ]; then
    log "would prune docker build cache to 20 GB: $(docker system df --format '{{.Type}} {{.Size}}' 2>/dev/null | sed -n 's/^Build Cache //p')"
  else
    out="$(docker builder prune --all --force --max-used-space 20GB 2>&1 | tail -1)"
    log "docker build cache pruned to 20 GB: ${out}"
    out="$(docker image prune --force 2>&1 | tail -1)"
    log "docker dangling images: ${out}"
  fi
fi

# 2. Scratch cargo targets outside the worktrees, gone with their session.
SCRATCH=()
while IFS= read -r -d '' tag; do SCRATCH+=("${tag%/CACHEDIR.TAG}"); done < <(
  find "$HOME/.cache" /tmp -mindepth 2 -maxdepth 2 -user "$ME" -name CACHEDIR.TAG -print0 2>/dev/null
  find /tmp -mindepth 3 -maxdepth 3 -user "$ME" -name CACHEDIR.TAG -print0 2>/dev/null
)
for t in ${SCRATCH[@]+"${SCRATCH[@]}"}; do
  low || break
  [ "$t" = "$SHARED_TARGET" ] && continue
  grep -qs 'created by cargo' "$t/CACHEDIR.TAG" || continue
  if untouched "$t" 2 && ! in_use "$t"; then remove "$t"; fi
done

# 3. cargo deps + incremental: what is left of the scratch targets, idle worktrees, the shared target.
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
          elif [ -n "$(find "$wt" \( -type d \( -name node_modules -o -name '.git' -o -name 'target*' -o -name build -o -name .gradle -o -name .turbo -o -name dist -o -name .output \) -prune \) -o -type f -mtime -1 -print -quit 2>/dev/null)" ]; then why="files written in the last day"
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
ignored() { git -C "$1" check-ignore -q "${2#"$1"/}/" 2>/dev/null; }

if low; then
  for t in ${SCRATCH[@]+"${SCRATCH[@]}"}; do
    [ -d "$t" ] && [ "$t" != "$SHARED_TARGET" ] && grep -qs 'created by cargo' "$t/CACHEDIR.TAG" && clean_cargo_target "$t"
    low || break
  done
fi
if low; then
  for wt in ${IDLE[@]+"${IDLE[@]}"}; do
    for t in "$wt"/target "$wt"/target-* "$wt"/apps/*/target "$wt"/apps/*/target-*; do
      [ -d "$t" ] && ignored "$wt" "$t" && clean_cargo_target "$t"
    done
    low || break
  done
fi
if low && [ -d "$SHARED_TARGET" ]; then
  clean_cargo_target "$SHARED_TARGET"
fi

# 4. The CI runners' work dirs, only between jobs (no Runner.Worker in the container).
if low && command -v docker >/dev/null && docker info >/dev/null 2>&1; then
  for c in $RUNNERS; do
    [ "$(docker inspect -f '{{.State.Running}}' "$c" 2>/dev/null)" = "true" ] || continue
    if docker exec "$c" pgrep -f Runner.Worker >/dev/null 2>&1; then log "skipped runner $c: a job runs"; continue; fi
    find_cmd='find /tmp/runner-*/work -mindepth 3 -maxdepth 9 -type d -path "*/target/*" \( -name deps -o -name incremental \) 2>/dev/null'
    if [ "$MODE" = "dry-run" ]; then
      kb="$(docker exec -u root "$c" sh -c "$find_cmd | while read -r d; do find \"\$d\" -type f -mtime +0 -printf '%k\n'; done" | awk '{ s += $1 } END { printf "%d", s }')"
      log "would delete $(human "${kb:-0}") of stale target files in runner $c"
    else
      kb="$(docker exec -u root "$c" sh -c "$find_cmd | while read -r d; do find \"\$d\" -type f -mtime +0 -printf '%k\n' -delete; done" | awk '{ s += $1 } END { printf "%d", s }')"
      log "deleted $(human "${kb:-0}") of stale target files in runner $c"
    fi
  done
fi

# 5. Gradle.
if low; then
  remove_old_files "$HOME/.gradle/caches/build-cache-1"
  for wt in ${IDLE[@]+"${IDLE[@]}"}; do
    for b in "$wt"/apps/android/build "$wt"/apps/android/*/build "$wt"/apps/android/.gradle; do
      [ -d "$b" ] && ignored "$wt" "$b" && remove "$b"
    done
  done
fi

# 6. /tmp scratch nobody uses: the user's own top-level entries, no write for a
# day, no process cwd or open file in it. Sockets and session dirs stay.
if low; then
  while IFS= read -r -d '' e; do
    case "${e#/tmp/}" in .*|ssh-*|tmux-*|claude-*|systemd-*|snap-*|*.sock|*.lck|kotlin-*|hsperfdata_*) continue ;; esac
    [ -S "$e" ] && continue
    in_use "$e" && continue
    untouched "$e" 1 || continue
    remove "$e"
    low || break
  done < <(find /tmp -mindepth 1 -maxdepth 1 -user "$ME" -mtime +0 -print0 2>/dev/null)
fi

# 7. Idle worktrees' dependency installs.
if low; then
  for wt in ${IDLE[@]+"${IDLE[@]}"}; do
    [ -d "$wt/node_modules" ] && ignored "$wt" "$wt/node_modules" && remove "$wt/node_modules"
    low || break
  done
fi

# 8. Lane AVDs left behind, then stale cargo git checkouts.
avds="$HOME/.android/avd"
if low && [ -d "$avds" ]; then
  EMUS="$(pgrep -af 'qemu-system|emulator' 2>/dev/null)"
  while IFS= read -r -d '' a; do
    name="$(basename "$a" .avd)"
    printf '%s\n' "$EMUS" | grep -qE -- "-avd $name( |$)" && continue
    untouched "$a" 2 || continue
    remove "$a" "$avds/$name.ini"
  done < <(find "$avds" -mindepth 1 -maxdepth 1 -type d -name 'exp*.avd' -print0 2>/dev/null)
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
