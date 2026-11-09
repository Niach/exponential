# EXP-1393: the device and heavy-build lane locks, ONE implementation.
#
# A lock is a directory (`mkdir` is atomic) holding an `owner` file whose
# first word names the holder: "<owner> <epoch> <pid> <host>". Work only after your own
# take_lock returned; release only what you own. release_lock REFUSES (exit 1,
# the lock stays) when the owner file names someone else, so one lane's
# cleanup can never free another lane's device.
#
#   . scripts/lanes/lock.sh    # sh, bash or zsh; then take_lock / release_lock <dir> <owner>
#   scripts/lanes/lane.sh <lane> <owner> -- <command...>   # the whole cycle
#
# Lane locks (lane_lock_dir): ios = the simulator, android = the emulator,
# heavy = one heavy build or perf run at a time, or N at once on a machine
# whose exp-lane-slots file says `heavy=N` (lane.sh takes any free slot).
#
# The pid is LANE_LOCK_PID (lane.sh sets its own, alive for the whole run).
# A waiter on the SAME host takes over a lock whose pid is gone (a session
# ended mid-run, EXP-1393 follow-up). A lock without a pid (taken by hand
# without LANE_LOCK_PID, or by an older lock.sh) is never taken over: set
# LANE_LOCK_PID to a process that lives as long as you hold the lock.

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
  while ! try_take_lock "$1" "$2"; do
    echo "take_lock: $1 busy ($(lock_owner "$1")), waiting" >&2
    sleep "$LANE_LOCK_POLL_SECONDS"
  done
}

# ONE attempt at <dir> (no wait): 0 = it is ours now. Reclaims a dead
# holder's lock and a stale ownerless dir exactly as take_lock always did.
try_take_lock() {
  while ! { mkdir "$1" 2>/dev/null && (set -C; echo "$2 $(date +%s) ${LANE_LOCK_PID:--} $(lane_lock_host)" > "$1/owner") 2>/dev/null; }; do
    if lock_holder_dead "$1"; then
      lock_take_over "$1" && continue
    fi
    if [ -d "$1" ] && [ ! -e "$1/owner" ]; then
      # GNU stat first: its `-f` is filesystem mode, not BSD's format flag.
      # No mtime = not stale: a failed stat never reclaims.
      mtime=$(stat -c %Y "$1" 2>/dev/null || stat -f %m "$1" 2>/dev/null) || mtime=
      if [ -n "$mtime" ] && [ $(( $(date +%s) - mtime )) -gt "$LANE_LOCK_STALE_SECONDS" ]; then
        rmdir "$1" 2>/dev/null && continue
      fi
    fi
    return 1
  done
}

# How many holders <lane> admits on THIS machine: the host-local file
# `${EXP_LANE_LOCK_ROOT:-/tmp}/exp-lane-slots` (lines `heavy=2`), else 1.
# Slot 1 is the lane's classic dir (lane_lock_dir); slot k appends `-k`'s
# number in place of the trailing 1 (exp-perf-heavy-2, …).
lane_slots() {
  n=$(sed -n "s/^$1=\([0-9][0-9]*\)[[:space:]]*$/\1/p" "${EXP_LANE_LOCK_ROOT:-/tmp}/exp-lane-slots" 2>/dev/null | tail -n 1)
  case "$n" in '' | 0) echo 1 ;; *) echo "$n" ;; esac
}

lane_slot_dir() {
  base=$(lane_lock_dir "$1") || return 2
  echo "${base%-1}-$2"
}

lane_lock_host() {
  hostname 2>/dev/null | cut -d. -f1
}

# True when <dir>'s owner file names a pid on this host and no such process
# exists. No pid, another host or an unreadable file = alive (never taken).
lock_holder_dead() {
  # cut, not `set -- $line`: zsh does not word-split an unquoted variable.
  [ -r "$1/owner" ] || return 1
  _lane_pid=$(cut -d' ' -f3 "$1/owner" 2>/dev/null)
  _lane_host=$(cut -d' ' -f4 "$1/owner" 2>/dev/null)
  case "$_lane_pid" in '' | *[!0-9]*) return 1 ;; esac
  [ -n "$_lane_host" ] && [ "$_lane_host" = "$(lane_lock_host)" ] || return 1
  ! ps -p "$_lane_pid" >/dev/null 2>&1
}

# Removes a dead holder's <dir> under a short takeover mutex, re-checking
# inside it so two waiters never both remove (one could remove the other's
# fresh lock). A mutex older than 60 s (its taker died inside) is reclaimed.
lock_take_over() {
  _lane_seen=$(cat "$1/owner" 2>/dev/null) || return 1
  if ! mkdir "$1.takeover" 2>/dev/null; then
    mtime=$(stat -c %Y "$1.takeover" 2>/dev/null || stat -f %m "$1.takeover" 2>/dev/null) || mtime=
    [ -n "$mtime" ] && [ $(( $(date +%s) - mtime )) -gt 60 ] && rmdir "$1.takeover" 2>/dev/null
    return 1
  fi
  if [ "$(cat "$1/owner" 2>/dev/null)" = "$_lane_seen" ] && lock_holder_dead "$1"; then
    echo "take_lock: taking over $1 from '$(echo "$_lane_seen" | cut -d' ' -f1)' (pid $(echo "$_lane_seen" | cut -d' ' -f3) is gone)" >&2
    rm -rf "$1"
  fi
  rmdir "$1.takeover"
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
