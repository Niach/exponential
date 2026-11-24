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
. "$here/sweep.sh"

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
        lock_overdue "$d" && note=" [past the max hold of $(lane_age "$(lane_max_hold "$d")"): the next waiter ends its job and takes over]"
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
    read -r w_owner w_since w_pid w_session w_dir _ < "$f" || continue
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
# and the trap. #FEED-306: whatever ends this run (the command's own end, a
# signal, an orphaned caller, a waiter ending an overdue holder) ends the
# command's whole process tree with it: a lane job never outlives its lane.sh.
watchdog=
orphan_watch=
job=
lane_end_job() {
  [ -n "$job" ] || return 0
  lane_end_tree "$job" "$LANE_KILL_GRACE_SECONDS" || echo "lane.sh: $lane: part of the command (pid $job) did not end" >&2
  wait "$job" 2>/dev/null
  job=
}
trap '[ -n "$watchdog" ] && kill "$watchdog" 2>/dev/null; [ -n "$orphan_watch" ] && kill "$orphan_watch" 2>/dev/null; lane_end_job; lane_wait_end; session_manifest_remove lane "$$" 2>/dev/null; release_lock "$dir" "$owner" 2>/dev/null' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
# This shell lives for the whole run: a waiter takes over only once it is gone.
LANE_LOCK_PID=$$
lock_owner_ok "$owner" || exit 2
# #FEED-306: a session's lane job goes with the session. Its manifest lists
# this lane.sh (the daemon ends its tree at the session's teardown, on macOS
# too); the caller (the session's shell) gone = orphaned: reparented to pid 1
# when a session's turn is stopped (2026-11-24: a build ran on for hours).
. "$here/session-manifest.sh"
session_manifest_add lane "$$" 2>/dev/null
lane_parent=$PPID
lane_orphaned() {
  [ "$lane_parent" -gt 1 ] 2>/dev/null || return 1
  [ "$(ps -o ppid= -p $$ 2>/dev/null | tr -d ' ')" != "$lane_parent" ]
}
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
    try_take_lock "$dir" "$owner" "$(lane_lock_dir "$lane")" && break 2
    holders="$holders${holders:+; }$(lock_describe "$dir")"
    k=$((k + 1))
  done
  if lane_orphaned; then
    echo "lane.sh: $lane: the caller (pid $lane_parent) is gone; not waiting any longer" >&2
    exit 129
  fi
  lane_wait_begin "$(lane_lock_dir "$lane")" "$owner"
  if [ "$holders" != "$seen" ]; then
    seen=$holders
    echo "lane.sh: $lane lane busy: $holders; waiting (scripts/lanes/lane.sh status)" >&2
  fi
  sleep "$LANE_LOCK_POLL_SECONDS"
done
# #EXP-1844: past the lane's max hold, tell the holder (this run) that the
# next waiter may take the slot; #FEED-306: that waiter ends this run first.
max_hold=$(lane_max_hold "$dir")
if [ "${max_hold:-0}" -gt 0 ] 2>/dev/null; then
  (
    trap 'kill "$sleeper" 2>/dev/null; exit 0' TERM
    sleep "$max_hold" &
    sleeper=$!
    wait "$sleeper"
    if [ "$(lock_field "$dir" 3)" = "$LANE_LOCK_PID" ]; then
      echo "lane.sh: $lane: '$owner' has held $(basename "$dir") for $(lane_age "$max_hold") (LANE_MAX_HOLD_SECONDS): the next waiter ends this command and takes the slot over" >&2
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
# shared cargo target if its disk is low (a no-op otherwise; sweep.sh finds
# a CLI that knows `sweep-target` and says so when there is none). Never a
# failure.
if [ -z "$own_target" ] && [ -n "${CARGO_TARGET_DIR:-}" ]; then
  lane_sweep "$CARGO_TARGET_DIR"
fi
# PERF-4: a lane's build is the machine's background work: nice 19 and, on
# Linux, idle IO, so the desktop keeps its cores and disk while it runs
# (.github/runners/README.md "Desktop first"). Not exec'd: the lock holder
# above must outlive the command. #FEED-306: started in the background (its
# stdin kept: fd 3) so a signal reaches this shell's traps at once instead
# of after the command, and `job` names the tree the EXIT trap ends.
exec 3<&0
if command -v ionice >/dev/null 2>&1; then
  nice -n 19 ionice -c 3 "$@" <&3 3<&- &
else
  nice -n 19 "$@" <&3 3<&- &
fi
job=$!
exec 3<&-
# #FEED-306: the caller gone (a stopped turn's shell) = this run goes too.
(
  trap 'kill "$sleeper" 2>/dev/null; exit 0' TERM
  while :; do
    sleep "${LANE_ORPHAN_POLL_SECONDS:-5}" &
    sleeper=$!
    wait "$sleeper"
    if lane_orphaned; then
      lane_log "$dir" "orphaned owner=$owner session=$(lane_session) pid=$LANE_LOCK_PID parent=$lane_parent"
      kill -TERM "$LANE_LOCK_PID" 2>/dev/null
      exit 0
    fi
  done
) >/dev/null 2>&1 &
orphan_watch=$!
wait "$job"
status=$?
job=
exit "$status"
