#!/bin/sh
# EXP-1393: run one command inside a lane: take the lane's lock, run, release
# it through release_lock (only ever our own), whatever the exit.
#
#   scripts/lanes/lane.sh <ios|android|heavy> <owner> [--own-target] -- <command...>
#   scripts/lanes/lane.sh heavy my-session -- bun run ios:test
#   scripts/lanes/lane.sh heavy my-session --own-target -- cargo test -p ui settings_bench
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
trap 'release_lock "$dir" "$owner" 2>/dev/null' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
# This shell lives for the whole run: a waiter takes over only once it is gone.
LANE_LOCK_PID=$$
lock_owner_ok "$owner" || exit 2
slots=$(lane_slots "$lane")
if [ "$slots" -le 1 ]; then
  take_lock "$dir" "$owner" || exit $?
else
  # Any free slot of the lane (lane_slots); `dir` names the one we hold, so
  # the EXIT trap releases exactly that one.
  while :; do
    k=1
    while [ "$k" -le "$slots" ]; do
      dir=$(lane_slot_dir "$lane" "$k")
      try_take_lock "$dir" "$owner" && break 2
      k=$((k + 1))
    done
    echo "lane.sh: all $slots $lane slots busy, waiting" >&2
    sleep "$LANE_LOCK_POLL_SECONDS"
  done
fi
if [ -n "$own_target" ]; then
  top=$(git rev-parse --show-toplevel 2>/dev/null) || { echo "lane.sh: --own-target needs a git checkout" >&2; exit 2; }
  CARGO_TARGET_DIR="$top/apps/desktop/target"
  export CARGO_TARGET_DIR
  echo "lane.sh: building into $CARGO_TARGET_DIR (own target)" >&2
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
