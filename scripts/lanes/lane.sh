#!/bin/sh
# EXP-1393: run one command inside a lane: take the lane's lock, run, release
# it through release_lock (only ever our own), whatever the exit.
#
#   scripts/lanes/lane.sh <ios|android|heavy> <owner> [--own-target] -- <command...>
#   scripts/lanes/lane.sh heavy my-session -- bun run ios:test
#   scripts/lanes/lane.sh heavy my-session --own-target -- cargo test -p ui settings_bench
#   scripts/lanes/lane.sh status     # holders (owner, session, pid, held for) + waiters
#
# --own-target: the command builds into THIS checkout's apps/desktop/target
# instead of the machine-wide CARGO_TARGET_DIR. Required for anything whose
# result must be this checkout's code: pixel benches, captures, perf runs.
# Cargo leaves a path crate's directory out of its artifact hash, so two
# worktrees' `ui` crates share one artifact in a shared target, and a build
# can count the OTHER worktree's artifact as fresh (README.md).

set -u
here=$(cd "$(dirname "$0")" && pwd)
. "$here/lock.sh"

# #EXP-1844: who holds each lane, for how long, and who waits.
lane_status() {
  now=$(date +%s)
  for lane in ios android heavy; do
    slots=$(lane_slots "$lane")
    k=1
    while [ "$k" -le "$slots" ]; do
      if [ "$k" -eq 1 ]; then d=$(lane_lock_dir "$lane"); else d=$(lane_slot_dir "$lane" "$k"); fi
      if [ -d "$d" ]; then
        note=
        lock_holder_dead "$d" && note=" [holder gone: the next waiter takes over]"
        lock_overdue "$d" && note=" [past the max hold of $(lane_age "$(lane_max_hold "$d")"): the next waiter takes over]"
        echo "$lane slot $k: $(lock_describe "$d")$note"
      else
        echo "$lane slot $k: free"
      fi
      k=$((k + 1))
    done
  done
  waiting=0
  for f in "$(lane_waiters_dir)"/*; do
    [ -f "$f" ] || continue
    # "<owner> <since> <pid> <session> <lock dir name>"
    read -r w_owner w_since w_pid w_session w_dir < "$f" || continue
    if ! ps -p "$w_pid" >/dev/null 2>&1; then rm -f "$f"; continue; fi
    [ "$waiting" -eq 0 ] && echo "waiting:"
    waiting=1
    echo "  $w_dir: $w_owner (session $w_session, pid $w_pid) for $(lane_age $((now - w_since)))"
  done
  [ "$waiting" -eq 0 ] && echo "waiting: nobody"
  echo "log: $(lane_log_file)"
  return 0
}

if [ "${1:-}" = status ]; then
  lane_status
  exit 0
fi

own_target=
if [ $# -ge 3 ] && [ "$3" = "--own-target" ]; then
  own_target=1
  a=$1; b=$2; shift 3; set -- "$a" "$b" "$@"
fi
[ $# -ge 4 ] && [ "$3" = "--" ] || { echo "usage: $0 <ios|android|heavy> <owner> [--own-target] -- <command...>" >&2; exit 2; }
lane=$1
dir=$(lane_lock_dir "$lane") || exit 2
owner=$2
shift 3

# Traps first: release_lock only frees our own lock, so an interrupt while
# waiting leaves the holder's lock alone, and none lands between the take
# and the trap.
watchdog=
trap '[ -n "$watchdog" ] && kill "$watchdog" 2>/dev/null; lane_wait_end; release_lock "$dir" "$owner" 2>/dev/null' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
# This shell lives for the whole run: a waiter takes over only once it is gone.
LANE_LOCK_PID=$$
lock_owner_ok "$owner" || exit 2
slots=$(lane_slots "$lane")
# Any free slot of the lane (lane_slots; slot 1 = lane_lock_dir); `dir`
# names the one we hold, so the EXIT trap releases exactly that one.
# #EXP-1844: a busy lane names its holders once per change of holders.
seen=
while :; do
  k=1
  holders=
  while [ "$k" -le "$slots" ]; do
    if [ "$k" -eq 1 ]; then dir=$(lane_lock_dir "$lane"); else dir=$(lane_slot_dir "$lane" "$k"); fi
    try_take_lock "$dir" "$owner" && break 2
    holders="$holders${holders:+; }$(lock_describe "$dir")"
    k=$((k + 1))
  done
  lane_wait_begin "$(lane_lock_dir "$lane")" "$owner"
  if [ "$holders" != "$seen" ]; then
    seen=$holders
    echo "lane.sh: $lane lane busy: $holders; waiting (scripts/lanes/lane.sh status)" >&2
  fi
  sleep "$LANE_LOCK_POLL_SECONDS"
done
# #EXP-1844: past the lane's max hold, tell the holder (this run) that the
# next waiter may take the slot; the command itself is never killed.
max_hold=$(lane_max_hold "$dir")
if [ "${max_hold:-0}" -gt 0 ] 2>/dev/null; then
  (
    trap 'kill "$sleeper" 2>/dev/null; exit 0' TERM
    sleep "$max_hold" &
    sleeper=$!
    wait "$sleeper"
    if [ "$(lock_field "$dir" 3)" = "$LANE_LOCK_PID" ]; then
      echo "lane.sh: $lane: '$owner' has held $(basename "$dir") for $(lane_age "$max_hold") (LANE_MAX_HOLD_SECONDS): the next waiter takes the slot over; this command keeps running" >&2
      lane_log "$dir" "overrun owner=$owner session=$(lane_session) pid=$LANE_LOCK_PID held=${max_hold}s"
    fi
  ) >/dev/null &
  watchdog=$!
fi
if [ -n "$own_target" ]; then
  top=$(git rev-parse --show-toplevel 2>/dev/null) || { echo "lane.sh: --own-target needs a git checkout" >&2; exit 2; }
  CARGO_TARGET_DIR="$top/apps/desktop/target"
  export CARGO_TARGET_DIR
  echo "lane.sh: building into $CARGO_TARGET_DIR (own target)" >&2
fi
# #EXP-1865: the slot is ours; before its build, sweep stale output from the
# shared cargo target if its disk is low (a no-op otherwise, and skipped
# when the CLI is missing or predates `sweep-target`). Never a failure.
if [ -z "$own_target" ] && [ -n "${CARGO_TARGET_DIR:-}" ] && command -v exponential >/dev/null 2>&1; then
  if swept=$(exponential sweep-target --if-low "$CARGO_TARGET_DIR" 2>&1) && [ -n "$swept" ]; then
    echo "lane.sh: $swept" >&2
  fi
fi
# PERF-4: a lane's build is the machine's background work: nice 19 and, on
# Linux, idle IO, so the desktop keeps its cores and disk while it runs
# (.github/runners/README.md "Desktop first"). Not exec'd: the lock holder
# above must outlive the command.
if command -v ionice >/dev/null 2>&1; then
  nice -n 19 ionice -c 3 "$@"
else
  nice -n 19 "$@"
fi
