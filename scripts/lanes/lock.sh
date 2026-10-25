# EXP-1393: the device and heavy-build lane locks, ONE implementation.
#
# A lock is a directory (`mkdir` is atomic) holding an `owner` file whose
# first word names the holder: "<owner> <epoch>". Work only after your own
# take_lock returned; release only what you own. release_lock REFUSES (exit 1,
# the lock stays) when the owner file names someone else, so one lane's
# cleanup can never free another lane's device.
#
#   . scripts/lanes/lock.sh    # sh, bash or zsh; then take_lock / release_lock <dir> <owner>
#   scripts/lanes/lane.sh <lane> <owner> -- <command...>   # the whole cycle
#
# Lane locks (lane_lock_dir): ios = the simulator, android = the emulator,
# heavy = one heavy build or perf run at a time.

LANE_LOCK_STALE_SECONDS=${LANE_LOCK_STALE_SECONDS:-300}
LANE_LOCK_POLL_SECONDS=${LANE_LOCK_POLL_SECONDS:-30}

lane_lock_dir() {
  case "$1" in
    ios) echo "${EXP_LANE_LOCK_ROOT:-/tmp}/exp-ios-sim-1" ;;
    android) echo "${EXP_LANE_LOCK_ROOT:-/tmp}/exp-android-emu-1" ;;
    heavy) echo "${EXP_LANE_LOCK_ROOT:-/tmp}/exp-perf-heavy-1" ;;
    *) echo "lane_lock_dir: unknown lane '$1' (ios, android, heavy)" >&2; return 2 ;;
  esac
}

# The holder named by the lock's owner file (empty = none written).
lock_owner() {
  cut -d' ' -f1 "$1/owner" 2>/dev/null
}

# Blocks until <dir> is ours. A dir left without an owner file for longer
# than LANE_LOCK_STALE_SECONDS (a holder killed between mkdir and the write)
# is reclaimed; an owned lock is never taken over. The owner file is written
# with noclobber: if two waiters reclaim the same stale dir at once, only the
# first write wins and the other keeps waiting.
take_lock() {
  [ -n "${1:-}" ] && [ -n "${2:-}" ] || { echo "take_lock: usage: take_lock <dir> <owner>" >&2; return 2; }
  lock_owner_ok "$2" || return 2
  while ! { mkdir "$1" 2>/dev/null && (set -C; echo "$2 $(date +%s)" > "$1/owner") 2>/dev/null; }; do
    if [ -d "$1" ] && [ ! -e "$1/owner" ]; then
      # GNU stat first: its `-f` is filesystem mode, not BSD's format flag.
      # No mtime = not stale: a failed stat never reclaims.
      mtime=$(stat -c %Y "$1" 2>/dev/null || stat -f %m "$1" 2>/dev/null) || mtime=
      if [ -n "$mtime" ] && [ $(( $(date +%s) - mtime )) -gt "$LANE_LOCK_STALE_SECONDS" ]; then
        rmdir "$1" 2>/dev/null && continue
      fi
    fi
    echo "take_lock: $1 busy ($(lock_owner "$1")), waiting" >&2
    sleep "$LANE_LOCK_POLL_SECONDS"
  done
}

# An owner is one word: lock_owner reads the first word back.
lock_owner_ok() {
  case "$1" in
    *" "* | *"	"*) echo "lane lock: owner '$1' must be one word" >&2; return 2 ;;
  esac
}

# Removes <dir> only when its owner file names <owner>. Nothing there = done.
release_lock() {
  [ -n "${1:-}" ] && [ -n "${2:-}" ] || { echo "release_lock: usage: release_lock <dir> <owner>" >&2; return 2; }
  lock_owner_ok "$2" || return 2
  [ -e "$1" ] || return 0
  holder=$(lock_owner "$1")
  if [ "$holder" != "$2" ]; then
    echo "release_lock: refusing to remove $1: owned by '${holder:-nobody}', not '$2'" >&2
    return 1
  fi
  rm -rf "$1"
}
